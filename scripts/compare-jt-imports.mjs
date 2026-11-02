import { createHash } from 'node:crypto';
import {
  mkdir,
  mkdtemp,
  readFile,
  rm,
  writeFile,
} from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { spawnSync } from 'node:child_process';

const studioRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const toolchainRoot = resolve(studioRoot, '../ice3-toolchain');
const fixture = resolve(
  studioRoot,
  'fixtures/source-import/jt/synthetic-contract.jt',
);
const converter = resolve(
  studioRoot,
  'fixtures/source-import/jt/converter.mjs',
);
const baselinePath = resolve(
  studioRoot,
  'fixtures/parity/jt-baseline-v1.json',
);

if (!process.argv.includes('--capture')) {
  throw new Error('usage: compare-jt-imports --capture');
}

const temporary = await mkdtemp(join(tmpdir(), 'ice3-jt-capture-'));
try {
  const raw = join(temporary, 'raw.glb');
  const source = join(temporary, 'legacy-source.glb');
  const prepared = join(temporary, 'legacy-prepared.glb');
  const reportPath = join(temporary, 'legacy-prepared.report.json');
  run(converter, ['--input', fixture, '--output', raw], studioRoot);

  run(process.execPath, [
    '--import',
    'tsx',
    'packages/cli/src/main.ts',
    'import',
    fixture,
    '--output',
    prepared,
    '--report',
    reportPath,
    '--asset-id',
    'parity-jt',
    '--adapter',
    'jt',
    '--json',
  ], toolchainRoot, {
    ICE3_JT_CONVERTER: converter,
  });
  const legacyReport = JSON.parse(await readFile(reportPath, 'utf8'));
  if (legacyReport.status !== 'pass') throw new Error('legacy import failed');
  if (legacyReport.adapter?.id !== 'jt') throw new Error('legacy adapter missing');

  const canonicalizer = pathToFileURL(join(
    toolchainRoot,
    'packages/jt-importer/src/canonicalizeJtGlb.ts',
  )).href;
  run(process.execPath, [
    '--import',
    'tsx',
    '--input-type=module',
    '--eval',
    `import { canonicalizeJtGlb } from ${JSON.stringify(canonicalizer)};
     await canonicalizeJtGlb(${JSON.stringify(raw)}, ${JSON.stringify(source)}, {
       assetId: 'parity-jt',
     });`,
  ], toolchainRoot);

  const fixtureBytes = await readFile(fixture);
  const rawDocument = parseGlb(await readFile(raw));
  const preparedDocument = parseGlb(await readFile(prepared));
  const sourceDocument = parseGlb(await readFile(source));
  const baseline = summarizeLegacy(
    rawDocument,
    sourceDocument,
    preparedDocument,
    legacyReport,
    sha256(fixtureBytes),
  );
  assertBaseline(baseline);
  await mkdir(dirname(baselinePath), { recursive: true });
  await writeFile(baselinePath, `${JSON.stringify(baseline, null, 2)}\n`);
  console.log(JSON.stringify(baseline, null, 2));
} finally {
  await rm(temporary, { recursive: true, force: true });
}

function summarizeLegacy(
  rawDocument,
  document,
  preparedDocument,
  legacyReport,
  sourceDigest,
) {
  const nodes = document.json.nodes ?? [];
  const meshes = document.json.meshes ?? [];
  const materials = (document.json.materials ?? [])
    .map((material) => ({
      name: material.name,
      baseColorFactor: material.pbrMetallicRoughness.baseColorFactor,
    }))
    .sort((left, right) => left.name.localeCompare(right.name, 'en'));
  const meshTriangles = meshes.map((mesh) => (
    (mesh.primitives ?? []).reduce((meshTotal, primitive) => (
      meshTotal + document.json.accessors[primitive.indices].count / 3
    ), 0)
  ));
  const triangles = meshTriangles.reduce((total, count) => total + count, 0);
  const expandedTriangles = nodes.reduce((total, node) => (
    node.mesh === undefined ? total : total + meshTriangles[node.mesh]
  ), 0);
  const referencedPositionAccessors = new Set();
  const meshUseCounts = new Map();
  for (const node of nodes) {
    if (node.mesh === undefined) continue;
    meshUseCounts.set(node.mesh, (meshUseCounts.get(node.mesh) ?? 0) + 1);
    for (const primitive of meshes[node.mesh].primitives ?? []) {
      referencedPositionAccessors.add(primitive.attributes.POSITION);
    }
  }
  const vertexCount = [...referencedPositionAccessors].reduce((total, index) => (
    total + document.json.accessors[index].count
  ), 0);
  const instanceNodeCount = [...meshUseCounts.values()].reduce((total, uses) => (
    total + Math.max(0, uses - 1)
  ), 0);
  const nodeMetadata = nodes
    .map((node) => node.extras?.ice3?.jt)
    .filter(Boolean);
  return {
    kind: 'ice3-jt-import-baseline',
    schemaVersion: '1.0',
    fixture: 'fixtures/source-import/jt/synthetic-contract.jt',
    assetId: 'parity-jt',
    sourceDigest,
    converter: 'fixtures/source-import/jt/converter.mjs',
    legacyAdapter: 'retired Toolchain JT product adapter',
    legacyPrepared: {
      status: legacyReport.status,
      adapterId: legacyReport.adapter.id,
      contentDigest: legacyReport.asset.contentDigest,
      sourceDigest: legacyReport.source.digest,
      metrics: legacyReport.metrics,
      output: {
        nodeCount: preparedDocument.json.nodes?.length ?? 0,
        meshCount: preparedDocument.json.meshes?.length ?? 0,
        selectionIds: (preparedDocument.json.nodes ?? [])
          .map((node) => node.extras?.ice3?.selectionId)
          .filter(Boolean)
          .sort(),
      },
    },
    stats: {
      nodeCount: nodes.length,
      meshCount: meshes.length,
      triangleCount: triangles,
      expandedTriangleCount: expandedTriangles,
      vertexCount,
      instanceNodeCount,
    },
    nodeNames: nodes.map((node) => node.name).sort(),
    selectionIds: nodes
      .map((node) => node.extras?.ice3?.selectionId)
      .sort(),
    instances: nodes
      .map((node) => node.extras?.ice3?.jt)
      .filter((value) => value?.instanceGroup)
      .map((value) => ({
        objectId: value.objectId,
        assemblyPath: value.assemblyPath,
        instanceGroup: value.instanceGroup,
      })),
    nodeMetadata,
    materials,
    sourceOrigin: document.json.asset?.extras?.ice3?.jt?.sourceOrigin,
    coordinateSystem: document.json.asset?.extras?.ice3?.coordinateSystem,
    sourceBounds: boundsFor(rawDocument),
    rebasedBounds: boundsFor(document),
    normalizationRules: [
      'Triangle count is unique mesh geometry; expandedTriangleCount applies node occurrences.',
      'Source-level material values are compared before Prepared edge/default materials are added.',
    ],
  };
}

function boundsFor(document) {
  const nodes = document.json.nodes ?? [];
  const worlds = worldMatrices(nodes);
  const min = [Infinity, Infinity, Infinity];
  const max = [-Infinity, -Infinity, -Infinity];
  for (const [nodeIndex, node] of nodes.entries()) {
    if (node.mesh === undefined) continue;
    const mesh = document.json.meshes[node.mesh];
    for (const primitive of mesh.primitives ?? []) {
      const accessor = document.json.accessors[primitive.attributes.POSITION];
      for (const local of [
        [accessor.min[0], accessor.min[1], accessor.min[2]],
        [accessor.min[0], accessor.min[1], accessor.max[2]],
        [accessor.min[0], accessor.max[1], accessor.min[2]],
        [accessor.min[0], accessor.max[1], accessor.max[2]],
        [accessor.max[0], accessor.min[1], accessor.min[2]],
        [accessor.max[0], accessor.min[1], accessor.max[2]],
        [accessor.max[0], accessor.max[1], accessor.min[2]],
        [accessor.max[0], accessor.max[1], accessor.max[2]],
      ]) {
        const world = transformPoint(worlds[nodeIndex], local);
        for (let axis = 0; axis < 3; axis += 1) {
          min[axis] = Math.min(min[axis], world[axis]);
          max[axis] = Math.max(max[axis], world[axis]);
        }
      }
    }
  }
  return { min, max };
}

function worldMatrices(nodes) {
  const parent = new Map();
  for (const [index, node] of nodes.entries()) {
    for (const child of node.children ?? []) parent.set(child, index);
  }
  const worlds = new Map();
  const visiting = new Set();
  const visit = (index) => {
    if (worlds.has(index)) return worlds.get(index);
    if (visiting.has(index)) throw new Error('legacy node hierarchy contains a cycle');
    if (!nodes[index]) throw new Error('legacy node child is missing');
    visiting.add(index);
    const local = localMatrix(nodes[index]);
    const parentIndex = parent.get(index);
    const matrix = parentIndex === undefined
      ? local
      : multiplyMatrices(visit(parentIndex), local);
    visiting.delete(index);
    worlds.set(index, matrix);
    return matrix;
  };
  return nodes.map((_, index) => visit(index));
}

function localMatrix(node) {
  if (Array.isArray(node.matrix) && node.matrix.length === 16) {
    return node.matrix;
  }
  const [tx, ty, tz] = node.translation ?? [0, 0, 0];
  const [sx, sy, sz] = node.scale ?? [1, 1, 1];
  const [qx, qy, qz, qw] = node.rotation ?? [0, 0, 0, 1];
  const length = Math.hypot(qx, qy, qz, qw);
  if (!(length > 0)) throw new Error('legacy node rotation is invalid');
  const x = qx / length;
  const y = qy / length;
  const z = qz / length;
  const w = qw / length;
  return [
    (1 - 2 * (y * y + z * z)) * sx, 2 * (x * y + z * w) * sx,
    2 * (x * z - y * w) * sx, 0,
    2 * (x * y - z * w) * sy, (1 - 2 * (x * x + z * z)) * sy,
    2 * (y * z + x * w) * sy, 0,
    2 * (x * z + y * w) * sz, 2 * (y * z - x * w) * sz,
    (1 - 2 * (x * x + y * y)) * sz, 0,
    tx, ty, tz, 1,
  ];
}

function multiplyMatrices(left, right) {
  const output = new Array(16).fill(0);
  for (let column = 0; column < 4; column += 1) {
    for (let row = 0; row < 4; row += 1) {
      for (let index = 0; index < 4; index += 1) {
        output[column * 4 + row] += (
          left[index * 4 + row] * right[column * 4 + index]
        );
      }
    }
  }
  return output;
}

function transformPoint(matrix, point) {
  return [0, 1, 2].map((row) => (
    matrix[row] * point[0]
    + matrix[4 + row] * point[1]
    + matrix[8 + row] * point[2]
    + matrix[12 + row]
  ));
}

function assertBaseline(baseline) {
  if (baseline.kind !== 'ice3-jt-import-baseline') throw new Error('baseline kind');
  if (baseline.stats.nodeCount !== 4) throw new Error('node baseline');
  if (baseline.stats.meshCount !== 2) throw new Error('mesh baseline');
  if (baseline.stats.triangleCount !== 24) throw new Error('triangle baseline');
  if (baseline.stats.expandedTriangleCount !== 36) {
    throw new Error('expanded triangle baseline');
  }
  if (baseline.stats.vertexCount !== 8) throw new Error('vertex baseline');
  if (baseline.stats.instanceNodeCount !== 1) throw new Error('instance baseline');
  if (baseline.selectionIds.length !== 4) throw new Error('selection baseline');
  if (baseline.materials.length !== 2) throw new Error('material baseline');
  if (baseline.sourceOrigin?.[0] !== -3 || baseline.sourceOrigin?.[2] !== -4) {
    throw new Error('origin baseline');
  }
  if (baseline.legacyPrepared.adapterId !== 'jt') {
    throw new Error('legacy CLI adapter baseline');
  }
  if (baseline.legacyPrepared.metrics.triangleCount !== 24) {
    throw new Error('legacy CLI triangle baseline');
  }
  if (baseline.legacyPrepared.output.nodeCount !== 4) {
    throw new Error('legacy CLI output node baseline');
  }
  if (baseline.legacyPrepared.output.meshCount !== 2) {
    throw new Error('legacy CLI output mesh baseline');
  }
  if (JSON.stringify(baseline.sourceBounds.min) !== JSON.stringify([-3, 0, -4])) {
    throw new Error('source bounds minimum baseline');
  }
  if (JSON.stringify(baseline.rebasedBounds.min) !== JSON.stringify([0, 0, 0])) {
    throw new Error('rebased bounds minimum baseline');
  }
}

function parseGlb(bytes) {
  if (bytes.length < 12 || bytes.readUInt32LE(0) !== 0x46546c67) {
    throw new Error('legacy output is not a GLB');
  }
  let offset = 12;
  let json;
  while (offset < bytes.length) {
    const length = bytes.readUInt32LE(offset);
    const kind = bytes.readUInt32LE(offset + 4);
    if (kind === 0x4e4f534a) {
      json = JSON.parse(
        bytes.subarray(offset + 8, offset + 8 + length).toString('utf8').trim(),
      );
    }
    offset += 8 + length;
  }
  if (!json) throw new Error('legacy GLB JSON chunk is missing');
  return { json };
}

function sha256(value) {
  return createHash('sha256').update(value).digest('hex');
}

function run(command, args, cwd, environment = undefined) {
  const result = spawnSync(command, args, {
    cwd,
    env: environment ? { ...process.env, ...environment } : process.env,
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
  });
  if (result.status !== 0) {
    throw new Error(
      `${command} failed:\n${result.stderr}\n${result.stdout}`,
    );
  }
  return result;
}
