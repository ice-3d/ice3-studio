import { createHash } from 'node:crypto';
import {
  mkdir,
  mkdtemp,
  readFile,
  rm,
  writeFile,
} from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const studioRoot = resolve(scriptDir, '..');
const toolchainRoot = resolve(studioRoot, '../ice3-toolchain');
const fixturePath = join(
  toolchainRoot,
  'tests/fixtures/step/synthetic-skid.step',
);
const baselinePath = join(
  studioRoot,
  'fixtures/parity/step-baseline-v1.json',
);
const assetId = 'parity-step';

if (process.argv.slice(2).join(' ') !== '--capture') {
  console.error('usage: node scripts/compare-step-imports.mjs --capture');
  process.exit(2);
}

const temporaryRoot = await mkdtemp(join(tmpdir(), 'ice3-step-baseline-'));

try {
  const outputPath = join(temporaryRoot, 'step.glb');
  const reportPath = join(temporaryRoot, 'step.report.json');
  const command = spawnSync('node', [
    '--import',
    'tsx',
    'packages/cli/src/main.ts',
    'import',
    fixturePath,
    '--output',
    outputPath,
    '--report',
    reportPath,
    '--asset-id',
    assetId,
    '--adapter',
    'step',
    '--json',
  ], {
    cwd: toolchainRoot,
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
  });
  if (command.status !== 0) {
    throw new Error(
      `Toolchain STEP baseline import failed:\n${command.stderr}\n${command.stdout}`,
    );
  }

  const [sourceBytes, reportBytes, outputBytes] = await Promise.all([
    readFile(fixturePath),
    readFile(reportPath),
    readFile(outputPath),
  ]);
  const report = JSON.parse(reportBytes.toString('utf8'));
  const glb = parseGlb(outputBytes);
  const summary = summarizeGlb(glb);
  const baseline = {
    kind: 'ice3-step-import-baseline',
    schemaVersion: '1.0',
    fixture: relative(studioRoot, fixturePath),
    assetId,
    sourceDigest: sha256(sourceBytes),
    legacy: {
      tool: report.tool,
      adapter: report.adapter,
      stats: {
        nodeCount: summary.nodeCount,
        maxNodeDepth: report.metrics.maxNodeDepth,
        meshCount: summary.meshCount,
        primitiveCount: report.metrics.primitiveCount,
        triangleCount: summary.triangleCount,
        vertexCount: summary.vertexCount,
        instanceNodeCount: summary.instanceNodeCount,
        semanticMaterialCount: summary.materials.length,
        edgeSegmentCount: summary.edgeSegmentCount,
        encodedBytes: report.metrics.encodedBytes,
      },
      sourceGlbDigest: report.source.digest,
      contentDigest: report.asset.contentDigest,
      nodeNames: summary.nodeNames,
      selectionIds: summary.selectionIds,
      bounds: summary.bounds,
      surfaceArea: summary.surfaceArea,
      materials: summary.materials,
      warningCodes: warningCodes(report.adapter.warnings),
    },
    normalizationRules: [
      'Tessellation triangles may differ while bounds, area and business identity remain equivalent.',
    ],
  };
  assertBaseline(baseline);
  await mkdir(dirname(baselinePath), { recursive: true });
  await writeFile(baselinePath, `${JSON.stringify(baseline, null, 2)}\n`);
  const reread = JSON.parse(await readFile(baselinePath, 'utf8'));
  assertBaseline(reread);
  console.log(JSON.stringify({
    status: 'captured',
    baseline: relative(studioRoot, baselinePath),
    stats: baseline.legacy.stats,
    surfaceArea: baseline.legacy.surfaceArea,
  }, null, 2));
} finally {
  await rm(temporaryRoot, { recursive: true, force: true });
}

function assertBaseline(value) {
  if (value.kind !== 'ice3-step-import-baseline') throw new Error('baseline kind');
  if (value.legacy.stats.nodeCount !== 6) throw new Error('node baseline');
  if (value.legacy.stats.meshCount !== 2) throw new Error('mesh baseline');
  if (value.legacy.stats.triangleCount !== 188) throw new Error('triangle baseline');
  if (value.legacy.selectionIds.length !== 6) throw new Error('selection baseline');
  if (!(value.legacy.surfaceArea > 0)) throw new Error('surface area baseline');
}

function parseGlb(bytes) {
  if (bytes.readUInt32LE(0) !== 0x46546c67) throw new Error('not a GLB');
  if (bytes.readUInt32LE(4) !== 2) throw new Error('unsupported GLB version');
  let offset = 12;
  let json;
  let bin = Buffer.alloc(0);
  while (offset < bytes.length) {
    const length = bytes.readUInt32LE(offset);
    const kind = bytes.readUInt32LE(offset + 4);
    const payload = bytes.subarray(offset + 8, offset + 8 + length);
    if (kind === 0x4e4f534a) {
      json = JSON.parse(payload.toString('utf8').trim());
    } else if (kind === 0x004e4942) {
      bin = payload;
    }
    offset += 8 + length;
  }
  if (!json) throw new Error('GLB JSON chunk is missing');
  return { json, bin };
}

function summarizeGlb(glb) {
  const nodes = glb.json.nodes ?? [];
  const meshes = glb.json.meshes ?? [];
  const positionAccessors = new Set();
  const usedMaterials = new Set();
  let triangleCount = 0;
  let edgeSegmentCount = 0;
  let surfaceArea = 0;
  for (const mesh of meshes) {
    for (const primitive of mesh.primitives ?? []) {
      const mode = primitive.mode ?? 4;
      const indexAccessor = glb.json.accessors[primitive.indices];
      if (!indexAccessor) throw new Error('primitive indices are missing');
      if (mode === 4) {
        const positionIndex = primitive.attributes?.POSITION;
        const positionAccessor = glb.json.accessors[positionIndex];
        if (!positionAccessor) throw new Error('primitive positions are missing');
        positionAccessors.add(positionIndex);
        triangleCount += indexAccessor.count / 3;
        const positions = readAccessor(glb, positionIndex);
        const indices = readAccessor(glb, primitive.indices);
        surfaceArea += triangleArea(positions, indices);
        if (primitive.material !== undefined) usedMaterials.add(primitive.material);
      } else if (mode === 1) {
        edgeSegmentCount += indexAccessor.count / 2;
      }
    }
  }
  const materials = [...usedMaterials]
    .map((index) => glb.json.materials[index])
    .filter(Boolean)
    .map((material) => ({
      name: material.name ?? '',
      baseColorFactor: material.pbrMetallicRoughness?.baseColorFactor ?? [1, 1, 1, 1],
    }))
    .sort(compareMaterial);
  const nodeNames = nodes.map((node) => node.name ?? '').filter(Boolean);
  const selectionIds = nodes
    .map((node) => node.extras?.ice3?.id)
    .filter(Boolean)
    .sort();
  const instanceNodeCount = nodes.filter(
    (node) => node.extras?.ice3?.instanceGroup !== undefined,
  ).length;
  const bounds = glb.json.asset?.extras?.ice3?.bounds;
  if (!bounds) throw new Error('Prepared GLB bounds are missing');
  return {
    nodeCount: nodes.length,
    meshCount: meshes.length,
    triangleCount,
    vertexCount: [...positionAccessors]
      .reduce((total, index) => total + glb.json.accessors[index].count, 0),
    instanceNodeCount,
    edgeSegmentCount,
    surfaceArea,
    nodeNames,
    selectionIds,
    bounds,
    materials,
  };
}

function readAccessor(glb, accessorIndex) {
  const accessor = glb.json.accessors[accessorIndex];
  if (!accessor) throw new Error(`accessor ${accessorIndex} is missing`);
  if (accessor.sparse) throw new Error('sparse accessors are unsupported');
  const view = glb.json.bufferViews[accessor.bufferView];
  if (!view) throw new Error(`buffer view for accessor ${accessorIndex} is missing`);
  const components = {
    SCALAR: 1,
    VEC2: 2,
    VEC3: 3,
    VEC4: 4,
    MAT2: 4,
    MAT3: 9,
    MAT4: 16,
  }[accessor.type];
  const formats = {
    5120: { bytes: 1, read: 'readInt8' },
    5121: { bytes: 1, read: 'readUInt8' },
    5122: { bytes: 2, read: 'readInt16LE' },
    5123: { bytes: 2, read: 'readUInt16LE' },
    5125: { bytes: 4, read: 'readUInt32LE' },
    5126: { bytes: 4, read: 'readFloatLE' },
  };
  if (!components || !formats[accessor.componentType]) {
    throw new Error('unsupported accessor layout');
  }
  const component = formats[accessor.componentType];
  const elementBytes = component.bytes * components;
  const stride = view.byteStride ?? elementBytes;
  const start = (view.byteOffset ?? 0) + (accessor.byteOffset ?? 0);
  const values = [];
  for (let item = 0; item < accessor.count; item += 1) {
    const itemOffset = start + item * stride;
    for (let part = 0; part < components; part += 1) {
      values.push(glb.bin[component.read](itemOffset + part * component.bytes));
    }
  }
  return values;
}

function triangleArea(positions, indices) {
  let area = 0;
  for (let index = 0; index < indices.length; index += 3) {
    const a = vertex(positions, indices[index]);
    const b = vertex(positions, indices[index + 1]);
    const c = vertex(positions, indices[index + 2]);
    const ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    const ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    const cross = [
      ab[1] * ac[2] - ab[2] * ac[1],
      ab[2] * ac[0] - ab[0] * ac[2],
      ab[0] * ac[1] - ab[1] * ac[0],
    ];
    area += Math.hypot(cross[0], cross[1], cross[2]) / 2;
  }
  return area;
}

function vertex(positions, index) {
  return positions.slice(index * 3, index * 3 + 3);
}

function compareMaterial(left, right) {
  return left.name.localeCompare(right.name, 'en')
    || JSON.stringify(left.baseColorFactor).localeCompare(
      JSON.stringify(right.baseColorFactor),
      'en',
    );
}

function warningCodes(warnings) {
  return [...new Set((warnings ?? []).map((warning) => (
    typeof warning === 'string' ? warning : warning.code
  )).filter(Boolean))].sort();
}

function sha256(value) {
  return createHash('sha256').update(value).digest('hex');
}
