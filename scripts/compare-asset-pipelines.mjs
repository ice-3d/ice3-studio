import { createHash } from 'node:crypto';
import {
  mkdtemp,
  mkdir,
  readFile,
  readdir,
  rm,
  writeFile,
} from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const studioRoot = resolve(scriptDir, '..');
const fixturePath = resolve(
  studioRoot,
  'fixtures/source-import/gltf/minimal.glb',
);
const reportPath = join(studioRoot, 'output/asset-pipeline-parity.json');
const temporaryRoot = await mkdtemp(join(tmpdir(), 'ice3-asset-parity-'));

try {
  await ensureCli();
  const fixture = parseGlb(await readFile(fixturePath));
  const positions = readFloatAccessor(fixture, 0);
  const indices = readIndexAccessor(fixture, 1);
  const project = await createProject(positions, indices);

  const newOutput = join(temporaryRoot, 'studio-runtime');
  run('cargo', [
    'run',
    '-q',
    '-p',
    'studio-cli',
    '--',
    'export',
    'tileset',
    '--project',
    project.directory,
    '--output',
    newOutput,
    '--profile',
    'agent-console-v1',
  ], studioRoot);

  const repeatedOutput = join(temporaryRoot, 'studio-runtime-repeat');
  run('cargo', [
    'run',
    '-q',
    '-p',
    'studio-cli',
    '--',
    'export',
    'tileset',
    '--project',
    project.directory,
    '--output',
    repeatedOutput,
    '--profile',
    'agent-console-v1',
  ], studioRoot);

  const newExport = JSON.parse(
    await readFile(join(newOutput, 'export.json'), 'utf8'),
  );
  const newTileset = JSON.parse(
    await readFile(join(newOutput, 'tileset.json'), 'utf8'),
  );
  const newParts = await partDocuments(newOutput);
  const repeatedParts = await partDocuments(repeatedOutput);
  const newSelectionIds = selectionIds(newParts);
  const newNodeCount = semanticNodeCount(newParts);
  const newPaths = newExport.files
    .map((file) => file.path)
    .filter((path) => path.startsWith('parts/'))
    .sort();
  const expectedPaths = (await readdir(join(newOutput, 'parts')))
    .filter((name) => name.endsWith('.glb'))
    .map((name) => `parts/${name}`)
    .sort();
  const firstDigests = digestMap(newParts);
  const secondDigests = digestMap(repeatedParts);
  const bounds = newTileset.root.boundingVolume.box;
  const expectedTriangleCount = indices.length / 3;
  const comparisons = {
    triangleTotals: {
      actual: newExport.stats.triangleCount,
      expected: expectedTriangleCount,
      equal: newExport.stats.triangleCount === expectedTriangleCount,
    },
    nodeCounts: {
      actual: newNodeCount,
      expected: 1,
      equal: newNodeCount === 1,
    },
    selectionIds: {
      actual: newSelectionIds,
      expected: ['node-000001'],
      equal: JSON.stringify(newSelectionIds) === JSON.stringify(['node-000001']),
    },
    tilesetBounds: {
      actual: bounds,
      equal: Array.isArray(bounds)
        && bounds.length === 12
        && bounds.every(Number.isFinite),
    },
    relativePaths: {
      actual: newPaths,
      expected: expectedPaths,
      equal: JSON.stringify(newPaths) === JSON.stringify(expectedPaths),
    },
    deterministic: {
      first: firstDigests,
      second: secondDigests,
      equal: JSON.stringify(firstDigests) === JSON.stringify(secondDigests),
    },
  };
  const failures = Object.entries(comparisons)
    .filter(([, value]) => value.equal !== true)
    .map(([key]) => key);
  const report = {
    kind: 'ice3-asset-pipeline-contract',
    schemaVersion: '1.0',
    status: failures.length === 0 ? 'pass' : 'fail',
    fixture: relative(studioRoot, fixturePath),
    profile: 'agent-console-v1',
    failures,
    comparisons,
  };
  await mkdir(dirname(reportPath), { recursive: true });
  await writeFile(reportPath, `${JSON.stringify(report, null, 2)}\n`);
  console.log(JSON.stringify(report, null, 2));
  if (failures.length > 0) process.exitCode = 1;
} finally {
  await rm(temporaryRoot, { recursive: true, force: true });
}

async function ensureCli() {
  const result = spawnSync('cargo', ['build', '--quiet', '-p', 'studio-cli'], {
    cwd: studioRoot,
    encoding: 'utf8',
  });
  if (result.status !== 0) {
    throw new Error(result.stderr || 'studio-cli build failed');
  }
}

async function createProject(positions, indices) {
  const workspaceRoot = join(temporaryRoot, 'projects');
  const directory = join(workspaceRoot, 'parity-fixture');
  await mkdir(join(directory, 'revisions'), { recursive: true });
  await mkdir(join(directory, 'history'), { recursive: true });
  await mkdir(join(directory, 'components'), { recursive: true });
  await mkdir(join(directory, 'resources'), { recursive: true });
  await mkdir(join(directory, 'changesets'), { recursive: true });
  await mkdir(join(directory, 'references'), { recursive: true });
  await mkdir(join(directory, 'cache/preview'), { recursive: true });
  await mkdir(join(directory, 'cache/geometry'), { recursive: true });
  const document = {
    schemaVersion: 2,
    revision: 0,
    projectId: 'parity-fixture',
    scene: {
      nodes: [{
        id: 'node-000001',
        name: 'triangle',
        parentId: null,
        transform: {
          translation: { x: 0, y: 0, z: 0 },
          rotation: [0, 0, 0, 1],
          scale: { x: 1, y: 1, z: 1 },
        },
        materialId: null,
        content: {
          type: 'custom_mesh',
          positions,
          indices,
        },
        metadata: { tag: 'PARITY-101', train: 'Train1' },
        visible: true,
      }],
    },
    materials: [],
    relations: [],
    constraints: [],
    components: [],
    metadata: {},
    idempotencyKeys: [],
  };
  const revisionBytes = Buffer.from(JSON.stringify(document));
  const digest = createHash('sha256').update(revisionBytes).digest('hex');
  await writeFile(join(directory, 'revisions/00000000.json'), revisionBytes);
  await writeFile(
    join(directory, 'history/operations.jsonl'),
    `${JSON.stringify({ revision: 0, digest, event: { kind: 'snapshot' } })}\n`,
  );
  await writeFile(join(directory, 'project.json'), JSON.stringify({
    schemaVersion: 2,
    projectId: 'parity-fixture',
    currentRevision: 0,
    snapshotDigest: digest,
    undoStack: [],
    redoStack: [],
  }));
  return { directory };
}

function parseGlb(bytes) {
  if (bytes.readUInt32LE(0) !== 0x46546c67 || bytes.readUInt32LE(4) !== 2) {
    throw new Error('fixture is not GLB v2');
  }
  let offset = 12;
  let json;
  let binary;
  while (offset < bytes.length) {
    const length = bytes.readUInt32LE(offset);
    const kind = bytes.readUInt32LE(offset + 4);
    const value = bytes.subarray(offset + 8, offset + 8 + length);
    if (kind === 0x4e4f534a) json = JSON.parse(value.toString('utf8').trim());
    if (kind === 0x004e4942) binary = value;
    offset += 8 + length;
  }
  if (!json || !binary) throw new Error('fixture GLB chunks are incomplete');
  return { json, binary };
}

function readFloatAccessor(glb, index) {
  const accessor = glb.json.accessors[index];
  const view = glb.json.bufferViews[accessor.bufferView];
  const offset = (view.byteOffset ?? 0) + (accessor.byteOffset ?? 0);
  const stride = view.byteStride ?? 12;
  return Array.from({ length: accessor.count }, (_value, element) => (
    Array.from({ length: 3 }, (_component, component) => (
      glb.binary.readFloatLE(offset + element * stride + component * 4)
    ))
  ));
}

function readIndexAccessor(glb, index) {
  const accessor = glb.json.accessors[index];
  const view = glb.json.bufferViews[accessor.bufferView];
  const offset = (view.byteOffset ?? 0) + (accessor.byteOffset ?? 0);
  const stride = view.byteStride ?? (accessor.componentType === 5123 ? 2 : 4);
  return Array.from({ length: accessor.count }, (_value, element) => (
    accessor.componentType === 5123
      ? glb.binary.readUInt16LE(offset + element * stride)
      : glb.binary.readUInt32LE(offset + element * stride)
  ));
}

async function partDocuments(root) {
  const parts = join(root, 'parts');
  const names = (await readdir(parts)).filter((name) => name.endsWith('.glb')).sort();
  return Promise.all(names.map(async (name) => ({
    ...await (async () => {
      const bytes = await readFile(join(parts, name));
      return { path: `parts/${name}`, digest: sha256(bytes), ...parseGlb(bytes) };
    })(),
  })));
}

function selectionIds(parts) {
  return [...new Set(parts.flatMap((part) => {
    const batched = part.json.asset?.extras?.ice3?.batching?.semanticIds;
    if (Array.isArray(batched)) return batched;
    return (part.json.nodes ?? [])
      .filter((node) => node.mesh !== undefined)
      .map((node) => (
        node.name
        ?? node.extras?.ice3?.id
        ?? node.extras?.ice3?.selectionId
      ));
  }).filter(Boolean))].sort();
}

function semanticNodeCount(parts) {
  return selectionIds(parts).length;
}

function digestMap(parts) {
  return Object.fromEntries(parts.map((part) => [part.path, part.digest]));
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
    throw new Error(`${command} ${args.join(' ')} failed:\n${result.stderr}\n${result.stdout}`);
  }
  return result;
}
