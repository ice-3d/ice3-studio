import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  mkdtemp,
  readFile,
  rm,
  writeFile,
} from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const studioRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const fixture = resolve(
  studioRoot,
  'fixtures/source-import/gltf/prepare-cube.gltf',
);
const baselinePath = resolve(
  studioRoot,
  'fixtures/parity/prepare-baseline-v1.json',
);
const mode = process.argv[2] ?? '--capture';

if (mode === '--capture') {
  await captureBaseline();
} else if (mode === '--verify-baseline') {
  const baseline = JSON.parse(await readFile(baselinePath, 'utf8'));
  assertBaseline(baseline);
  const source = await sourceIdentity();
  if (source.digest !== baseline.sourceDigest) {
    throw new Error('baseline source digest does not match the fixture');
  }
  console.log(JSON.stringify({
    kind: 'ice3-prepare-baseline-verification',
    status: 'pass',
    sourceDigest: baseline.sourceDigest,
  }, null, 2));
} else {
  console.error(
    'usage: node scripts/compare-prepare-imports.mjs '
      + '[--capture|--verify-baseline]',
  );
  process.exit(2);
}

async function captureBaseline() {
  const archiveRoot = process.env.ICE3_TOOLCHAIN_ARCHIVE?.trim();
  if (!archiveRoot) {
    throw new Error('ICE3_TOOLCHAIN_ARCHIVE is required for --capture');
  }

  assertSyntheticArchive(archiveRoot);
  run('npm', ['run', 'build'], archiveRoot);

  const temporary = await mkdtemp(join(tmpdir(), 'ice3-prepare-capture-'));
  try {
    const source = await sourceIdentity();
    const cases = {
      default: await captureCase(archiveRoot, temporary, 'default', []),
      skipEdges: await captureCase(
        archiveRoot,
        temporary,
        'skip-edges',
        ['--skip-edges'],
      ),
      crease90: await captureCase(
        archiveRoot,
        temporary,
        'crease-90',
        ['--crease-angle', '90'],
      ),
    };
    const baseline = {
      kind: 'ice3-prepare-baseline',
      schemaVersion: '1.0',
      fixture: relative(studioRoot, fixture),
      sourceDigest: source.digest,
      sourceFiles: source.files,
      converter: 'retired Toolchain CLI',
      options: {
        default: {
          budget: 'interactive-desktop-v1',
          creaseAngleDegrees: 30,
          edges: true,
        },
        skipEdges: {
          budget: 'interactive-desktop-v1',
          creaseAngleDegrees: 30,
          edges: false,
        },
        crease90: {
          budget: 'interactive-desktop-v1',
          creaseAngleDegrees: 90,
          edges: true,
        },
      },
      ...cases,
      budget: cases.default.budget,
    };
    assertBaseline(baseline);
    await writeFile(baselinePath, `${JSON.stringify(baseline, null, 2)}\n`);
    console.log(JSON.stringify(baseline, null, 2));
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
}

async function captureCase(archiveRoot, temporary, name, extraArguments) {
  const output = join(temporary, `${name}.glb`);
  const reportPath = join(temporary, `${name}.report.json`);
  run('node', [
    join(archiveRoot, 'packages/cli/dist/main.js'),
    'build',
    fixture,
    '--output',
    output,
    '--report',
    reportPath,
    '--asset-id',
    'prepare-cube',
    '--budget',
    'interactive-desktop-v1',
    ...extraArguments,
    '--json',
  ], archiveRoot);

  const bytes = await readFile(output);
  const document = parseGlb(bytes);
  const report = JSON.parse(await readFile(reportPath, 'utf8'));
  const profile = document.json.asset?.extras?.ice3;
  if (!profile) throw new Error(`${name}: Prepared profile is missing`);
  const edgePrimitive = findEdgePrimitive(document.json);
  const edgeMaterial = edgePrimitive
    ? document.json.materials[edgePrimitive.material]
    : undefined;
  return {
    outputDigest: sha256(bytes),
    edgesPrepared: profile.preparation?.edgesPrepared,
    stats: profile.stats,
    budget: {
      profile: profile.budget?.profile,
      status: profile.budget?.status,
      warningCount: profile.budget?.warningCount,
      errorCount: profile.budget?.errorCount,
    },
    report: {
      kind: report.kind,
      schemaVersion: report.schemaVersion,
      budget: report.budget,
      stats: report.stats,
    },
    edgeRole: edgePrimitive?.extras?.ice3?.role,
    edgeWidthPx: edgePrimitive?.extras?.ice3?.widthPx,
    edgeColor: edgeMaterial?.pbrMetallicRoughness?.baseColorFactor,
    edgeMaterialName: edgeMaterial?.name,
    selectionIds: document.json.nodes
      ?.map((node) => node.extras?.ice3?.selectionId)
      .filter(Boolean),
    bounds: profile.bounds,
  };
}

function assertBaseline(baseline) {
  if (baseline.kind !== 'ice3-prepare-baseline') {
    throw new Error('baseline kind is invalid');
  }
  if (baseline.schemaVersion !== '1.0') {
    throw new Error('baseline schemaVersion is invalid');
  }
  if (baseline.default.edgesPrepared !== true) {
    throw new Error('default edges are not prepared');
  }
  if (baseline.default.stats?.triangleCount !== 12) {
    throw new Error('default triangle baseline changed');
  }
  if (baseline.default.stats?.edgeSegmentCount !== 12) {
    throw new Error('default edge baseline changed');
  }
  if (baseline.default.stats?.materialCount !== 5) {
    throw new Error('default material baseline changed');
  }
  if (baseline.default.edgeRole !== 'edges') {
    throw new Error('edge role baseline changed');
  }
  if (baseline.default.edgeWidthPx !== 3) {
    throw new Error('edge width baseline changed');
  }
  if (baseline.skipEdges.stats?.edgeSegmentCount !== 0) {
    throw new Error('skip-edges baseline changed');
  }
  if (baseline.crease90.stats?.edgeSegmentCount !== 12) {
    throw new Error('crease-90 baseline changed');
  }
  if (baseline.budget?.profile !== 'interactive-desktop-v1') {
    throw new Error('budget profile baseline changed');
  }
  if (baseline.budget?.status !== 'pass') {
    throw new Error('budget status baseline changed');
  }
}

async function sourceIdentity() {
  const gltf = await readFile(fixture);
  const binary = await readFile(
    resolve(dirname(fixture), 'prepare-cube.bin'),
  );
  const files = {
    'prepare-cube.gltf': sha256(gltf),
    'prepare-cube.bin': sha256(binary),
  };
  return {
    digest: sha256([
      'sha256-tree-v1',
      ...Object.entries(files).map(([name, digest]) => `${name}:${digest}`),
    ].join('\n')),
    files,
  };
}

function assertSyntheticArchive(archiveRoot) {
  const fixtureRoot = resolve(archiveRoot, 'tests/fixtures');
  const result = spawnSync('rg', [
    '-n',
    '(customer|confidential|客户|/Users/)',
    fixtureRoot,
  ], {
    encoding: 'utf8',
    maxBuffer: 8 * 1024 * 1024,
  });
  if (result.error) throw result.error;
  if (result.status === 0) {
    throw new Error(
      `archived Toolchain fixture tree is not synthetic:\n${result.stdout}`,
    );
  }
  if (result.status !== 1) {
    throw new Error(`archive scan failed: ${result.stderr}`);
  }
}

function findEdgePrimitive(document) {
  for (const mesh of document.meshes ?? []) {
    for (const primitive of mesh.primitives ?? []) {
      if (
        primitive.mode === 1
        && primitive.extras?.ice3?.role === 'edges'
      ) {
        return primitive;
      }
    }
  }
  return undefined;
}

function parseGlb(bytes) {
  if (bytes.readUInt32LE(0) !== 0x46546c67) {
    throw new Error('not a GLB');
  }
  let offset = 12;
  let json;
  while (offset < bytes.length) {
    const length = bytes.readUInt32LE(offset);
    const kind = bytes.readUInt32LE(offset + 4);
    if (kind === 0x4e4f534a) {
      json = JSON.parse(
        bytes.subarray(offset + 8, offset + 8 + length)
          .toString('utf8')
          .trimEnd(),
      );
    }
    offset += 8 + length;
  }
  if (!json) throw new Error('GLB JSON chunk is missing');
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
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(
      `${command} failed:\n${result.stderr}\n${result.stdout}`,
    );
  }
  return result;
}
