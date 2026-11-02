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
const fixtureRoot = join(toolchainRoot, 'tests/fixtures/ifc');
const baselinePath = join(studioRoot, 'fixtures/parity/ifc-baseline-v1.json');
const mode = process.argv[2] ?? '--compare';

const fixtures = [
  ['syntheticPlant', 'synthetic-plant.ifc', 'parity-ifc'],
  ['duplicateGlobalId', 'duplicate-global-id.ifc', 'parity-ifc-duplicate-global-id'],
  ['missingGlobalId', 'missing-global-id.ifc', 'parity-ifc-missing-global-id'],
  ['orphanProduct', 'orphan-product.ifc', 'parity-ifc-orphan-product'],
  ['semanticOnlyProduct', 'semantic-only-product.ifc', 'parity-ifc-semantic-only'],
];

if (!['--capture', '--compare'].includes(mode)) {
  console.error('usage: node scripts/compare-ifc-imports.mjs [--capture|--compare]');
  process.exit(2);
}

if (mode === '--capture') {
  await captureBaseline();
} else {
  console.error('native IFC comparison is introduced by Phase 4B Task 4');
  process.exit(2);
}

async function captureBaseline() {
  const temporaryRoot = await mkdtemp(join(tmpdir(), 'ice3-ifc-baseline-'));
  try {
    const captured = {};
    for (const [key, fixtureName, assetId] of fixtures) {
      captured[key] = await captureFixture(
        temporaryRoot,
        key,
        fixtureName,
        assetId,
      );
    }
    const baseline = {
      kind: 'ice3-ifc-import-baseline',
      schemaVersion: '1.0',
      assetId: 'parity-ifc',
      fixtures: captured,
      normalizationRules: [
        'Tessellation triangles may differ while normalized bounds, area, business identity, metadata and instance identity remain equivalent.',
      ],
    };
    assertBaseline(baseline);
    await mkdir(dirname(baselinePath), { recursive: true });
    await writeFile(baselinePath, `${JSON.stringify(baseline, null, 2)}\n`);
    assertBaseline(JSON.parse(await readFile(baselinePath, 'utf8')));
    console.log(JSON.stringify({
      status: 'captured',
      baseline: relative(studioRoot, baselinePath),
      fixtures: Object.fromEntries(
        Object.entries(captured).map(([key, value]) => [key, value.stats]),
      ),
    }, null, 2));
  } finally {
    await rm(temporaryRoot, { recursive: true, force: true });
  }
}

async function captureFixture(root, key, fixtureName, assetId) {
  const fixturePath = join(fixtureRoot, fixtureName);
  const outputPath = join(root, `${key}.glb`);
  const reportPath = join(root, `${key}.report.json`);
  const command = spawnSync('node', [
    '--import', 'tsx', 'packages/cli/src/main.ts', 'import',
    fixturePath,
    '--output', outputPath,
    '--report', reportPath,
    '--asset-id', assetId,
    '--adapter', 'ifc',
    '--json',
  ], {
    cwd: toolchainRoot,
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
  });
  if (command.status !== 0) {
    throw new Error(
      `Toolchain IFC baseline import failed for ${fixtureName}:\n`
      + `${command.stderr}\n${command.stdout}`,
    );
  }

  const [sourceBytes, reportBytes, outputBytes] = await Promise.all([
    readFile(fixturePath),
    readFile(reportPath),
    readFile(outputPath),
  ]);
  const report = JSON.parse(reportBytes.toString('utf8'));
  const summary = summarizeGlb(parseGlb(outputBytes), assetId);
  return {
    fixture: relative(studioRoot, fixturePath),
    sourceDigest: sha256(sourceBytes),
    tool: report.tool,
    adapter: report.adapter,
    metrics: report.metrics,
    stats: {
      nodeCount: summary.nodeCount,
      meshCount: summary.meshCount,
      triangleCount: summary.triangleCount,
      vertexCount: summary.vertexCount,
      instanceNodeCount: summary.instanceNodeCount,
      semanticMaterialCount: summary.materials.length,
    },
    nodeNames: summary.nodeNames,
    selectionIds: summary.selectionIds,
    spatialPaths: summary.spatialPaths,
    propertySets: summary.propertySets,
    quantitySets: summary.quantitySets,
    systems: summary.systems,
    bounds: summary.bounds,
    surfaceArea: summary.surfaceArea,
    materials: summary.materials,
    warningCodes: warningCodes(report.adapter.warnings),
  };
}

function assertBaseline(value) {
  if (value.kind !== 'ice3-ifc-import-baseline') {
    throw new Error('baseline kind');
  }
  const main = value.fixtures.syntheticPlant;
  if (main.stats.meshCount !== 2) throw new Error('mesh baseline');
  if (main.stats.triangleCount !== 24) throw new Error('triangle baseline');
  if (main.stats.instanceNodeCount !== 0) throw new Error('instance baseline');
  if (!main.selectionIds.some((id) => id.startsWith('parity-ifc/ifc/'))) {
    throw new Error('selection baseline');
  }
  if (!main.nodeNames.includes('Primary Pump')) throw new Error('pump baseline');
  if (!main.nodeNames.includes('Buffer Tank')) throw new Error('tank baseline');
  if (!main.propertySets.some((set) => set.name === 'Pset_PumpTypeCommon')) {
    throw new Error('pset baseline');
  }
  if (!main.quantitySets.some((set) => set.name === 'Qto_PumpBaseQuantities')) {
    throw new Error('qto baseline');
  }
  if (!main.systems.some((system) => system.name === 'Sludge Transfer')) {
    throw new Error('system baseline');
  }
  if (!(main.surfaceArea > 0)) throw new Error('surface area baseline');
  if (!value.fixtures.duplicateGlobalId.warningCodes
    .includes('IFC_DUPLICATE_GLOBAL_ID:18')) {
    throw new Error('duplicate GlobalId baseline');
  }
  if (!value.fixtures.orphanProduct.warningCodes
    .includes('IFC_UNMAPPED_GEOMETRY:76')) {
    throw new Error('orphan product baseline');
  }
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

function summarizeGlb(glb, assetId) {
  const nodes = glb.json.nodes ?? [];
  const meshes = glb.json.meshes ?? [];
  const parents = new Map();
  for (let index = 0; index < nodes.length; index += 1) {
    for (const child of nodes[index].children ?? []) {
      parents.set(child, index);
    }
  }
  const nodePath = (index) => {
    const parts = [];
    let current = index;
    while (current !== undefined) {
      parts.push(nodes[current].name ?? `node/${current}`);
      current = parents.get(current);
    }
    return parts.reverse().join('/');
  };

  const positionAccessors = new Set();
  const usedMaterials = new Set();
  const meshReferences = new Map();
  let triangleCount = 0;
  let surfaceArea = 0;
  for (const mesh of meshes) {
    for (const primitive of mesh.primitives ?? []) {
      if ((primitive.mode ?? 4) !== 4) continue;
      const positionIndex = primitive.attributes?.POSITION;
      const indexAccessor = glb.json.accessors[primitive.indices];
      if (!indexAccessor || positionIndex === undefined) {
        throw new Error('IFC primitive accessors are missing');
      }
      positionAccessors.add(positionIndex);
      triangleCount += indexAccessor.count / 3;
      surfaceArea += triangleArea(
        readAccessor(glb, positionIndex),
        readAccessor(glb, primitive.indices),
      );
      if (primitive.material !== undefined) usedMaterials.add(primitive.material);
    }
  }
  for (const node of nodes) {
    if (!Number.isInteger(node.mesh)) continue;
    meshReferences.set(node.mesh, (meshReferences.get(node.mesh) ?? 0) + 1);
  }

  const materials = [...usedMaterials]
    .map((index) => glb.json.materials[index])
    .filter(Boolean)
    .map((material) => ({
      name: material.name ?? '',
      baseColorFactor:
        material.pbrMetallicRoughness?.baseColorFactor ?? [1, 1, 1, 1],
    }))
    .sort(compareMaterial);
  const ifcNodes = nodes
    .map((node, index) => ({
      node,
      extras: node.extras?.ice3?.ifc,
      path: nodePath(index),
    }))
    .filter((entry) => entry.extras);
  const allDefinitionSets = uniqueByName(
    ifcNodes.flatMap((entry) => entry.extras.propertySets ?? []),
  );
  return {
    nodeCount: nodes.length,
    meshCount: meshes.length,
    triangleCount,
    vertexCount: [...positionAccessors].reduce(
      (total, index) => total + glb.json.accessors[index].count,
      0,
    ),
    instanceNodeCount: nodes.filter((node) => (
      Number.isInteger(node.mesh)
      && (meshReferences.get(node.mesh) ?? 0) > 1
    )).length,
    nodeNames: nodes.map((node) => node.name ?? '').filter(Boolean),
    selectionIds: nodes
      .map((node) => node.extras?.ice3?.selectionId)
      .filter(Boolean)
      .sort(),
    spatialPaths: [...new Set(
      nodes.map((_, index) => nodePath(index)),
    )].sort(),
    propertySets: allDefinitionSets.filter(
      (set) => !String(set.name).startsWith('Qto_'),
    ),
    quantitySets: allDefinitionSets.filter(
      (set) => String(set.name).startsWith('Qto_'),
    ),
    systems: uniqueByName(
      ifcNodes.flatMap((entry) => entry.extras.systems ?? []),
    ),
    bounds: requireBounds(glb),
    surfaceArea,
    materials,
    assetId,
  };
}

function requireBounds(glb) {
  const bounds = glb.json.asset?.extras?.ice3?.bounds;
  if (!bounds) throw new Error('Prepared GLB bounds are missing');
  return bounds;
}

function uniqueByName(values) {
  const byKey = new Map();
  for (const value of values) {
    const key = `${value.name ?? ''}\0${JSON.stringify(value)}`;
    byKey.set(key, value);
  }
  return [...byKey.values()].sort((left, right) => (
    String(left.name).localeCompare(String(right.name), 'en')
    || JSON.stringify(left).localeCompare(JSON.stringify(right), 'en')
  ));
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
      values.push(glb.bin[component.read](
        itemOffset + part * component.bytes,
      ));
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
    area += Math.hypot(...cross) / 2;
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
