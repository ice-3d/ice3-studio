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
  run(converter, ['--input', fixture, '--output', raw], studioRoot);

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
  const document = parseGlb(await readFile(source));
  const baseline = summarizeLegacy(document, sha256(fixtureBytes));
  assertBaseline(baseline);
  await mkdir(dirname(baselinePath), { recursive: true });
  await writeFile(baselinePath, `${JSON.stringify(baseline, null, 2)}\n`);
  console.log(JSON.stringify(baseline, null, 2));
} finally {
  await rm(temporary, { recursive: true, force: true });
}

function summarizeLegacy(document, sourceDigest) {
  const nodes = document.json.nodes ?? [];
  const meshes = document.json.meshes ?? [];
  const materials = (document.json.materials ?? [])
    .map((material) => ({
      name: material.name,
      baseColorFactor: material.pbrMetallicRoughness.baseColorFactor,
    }))
    .sort((left, right) => left.name.localeCompare(right.name, 'en'));
  const triangles = meshes.reduce((total, mesh) => total + (
    mesh.primitives ?? []
  ).reduce((meshTotal, primitive) => (
    meshTotal + document.json.accessors[primitive.indices].count / 3
  ), 0), 0);
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
    legacyAdapter: 'retired Toolchain JT canonicalizer',
    stats: {
      nodeCount: nodes.length,
      meshCount: meshes.length,
      triangleCount: triangles,
      vertexCount: document.json.accessors[
        meshes[0].primitives[0].attributes.POSITION
      ].count,
      instanceNodeCount: nodeMetadata.filter((value) => value.instanceGroup).length - 1,
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
    bounds: boundsFor(document),
    normalizationRules: [
      'Triangle count is unique mesh geometry; instance expansion is recorded separately.',
      'Source-level material values are compared before Prepared edge/default materials are added.',
    ],
  };
}

function boundsFor(document) {
  const nodes = document.json.nodes ?? [];
  const min = [Infinity, Infinity, Infinity];
  const max = [-Infinity, -Infinity, -Infinity];
  for (const node of nodes) {
    if (node.mesh === undefined) continue;
    const translation = node.translation ?? [0, 0, 0];
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
        for (let axis = 0; axis < 3; axis += 1) {
          min[axis] = Math.min(min[axis], local[axis] + translation[axis]);
          max[axis] = Math.max(max[axis], local[axis] + translation[axis]);
        }
      }
    }
  }
  return { min, max };
}

function assertBaseline(baseline) {
  if (baseline.kind !== 'ice3-jt-import-baseline') throw new Error('baseline kind');
  if (baseline.stats.nodeCount !== 4) throw new Error('node baseline');
  if (baseline.stats.meshCount !== 2) throw new Error('mesh baseline');
  if (baseline.stats.triangleCount !== 24) throw new Error('triangle baseline');
  if (baseline.stats.instanceNodeCount !== 1) throw new Error('instance baseline');
  if (baseline.selectionIds.length !== 4) throw new Error('selection baseline');
  if (baseline.materials.length !== 2) throw new Error('material baseline');
  if (baseline.sourceOrigin?.[0] !== -3 || baseline.sourceOrigin?.[2] !== -4) {
    throw new Error('origin baseline');
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

function run(command, args, cwd) {
  const result = spawnSync(command, args, {
    cwd,
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
