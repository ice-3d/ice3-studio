import { spawnSync } from 'node:child_process';
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

const studioRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const fixture = resolve(
  studioRoot,
  'fixtures/source-import/gltf/prepare-cube.gltf',
);
const baselinePath = resolve(
  studioRoot,
  'fixtures/parity/prepare-baseline-v1.json',
);
const parityReportPath = resolve(studioRoot, 'output/prepare-parity.json');
const mode = process.argv[2] ?? '--compare';

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
} else if (mode === '--compare') {
  await compareNative();
} else {
  console.error(
    'usage: node scripts/compare-prepare-imports.mjs '
      + '[--compare|--capture|--verify-baseline]',
  );
  process.exit(2);
}

async function compareNative() {
  let report;
  try {
    report = await buildNativeParityReport();
  } catch (error) {
    report = {
      kind: 'ice3-prepare-parity',
      schemaVersion: '1.0',
      status: 'fail',
      failures: ['execution'],
      error: error instanceof Error ? error.message : String(error),
    };
  }
  await mkdir(dirname(parityReportPath), { recursive: true });
  await writeFile(parityReportPath, `${JSON.stringify(report, null, 2)}\n`);
  console.log(JSON.stringify(report, null, 2));
  if (report.status !== 'pass') process.exitCode = 1;
}

async function buildNativeParityReport() {
  const baseline = JSON.parse(await readFile(baselinePath, 'utf8'));
  assertBaseline(baseline);
  const source = await sourceIdentity();
  if (source.digest !== baseline.sourceDigest) {
    throw new Error('baseline source digest does not match the fixture');
  }

  const studioBinary = resolveStudioBinary();
  const temporary = await mkdtemp(join(tmpdir(), 'ice3-prepare-parity-'));
  try {
    const first = await buildStudioCase(studioBinary, temporary, 'first', baseline);
    const second = await buildStudioCase(studioBinary, temporary, 'second', baseline);
    const expected = summarizeBaseline(baseline);
    const native = first.summary;
    const reportKindNormalization = normalizeReportKind(
      baseline.default.report.kind,
    );
    const comparisons = {
      sourceDigest:
        native.sourceDigest === baseline.sourceFiles['prepare-cube.gltf'],
      stats: equal(native.stats, expected.stats),
      bounds: equal(native.bounds, expected.bounds),
      businessNodeIds: equal(
        native.businessNodeIds,
        expected.businessNodeIds,
      ),
      nodeNames: equal(native.nodeNames, expected.nodeNames),
      selectionFallback: equal(
        native.selectionIds,
        expected.selectionIds,
      ),
      edgeRole: native.edgeRole === expected.edgeRole,
      edgeWidthPx: native.edgeWidthPx === expected.edgeWidthPx,
      edgeColor: equal(native.edgeColor, expected.edgeColor),
      edgeMaterialName: native.edgeMaterialName === expected.edgeMaterialName,
      edgesPrepared: native.edgesPrepared === expected.edgesPrepared,
      budget: equal(native.budget, expected.budget),
      reportKind: native.report.kind === reportKindNormalization.studio,
      reportStats: equal(native.report.stats, expected.report.stats),
      reportBudget: equal(native.report.budget, expected.report.budget),
      determinism: first.digest === second.digest,
    };
    const failures = Object.entries(comparisons)
      .filter(([, passed]) => !passed)
      .map(([name]) => name);
    return {
      kind: 'ice3-prepare-parity',
      schemaVersion: '1.0',
      status: failures.length === 0 ? 'pass' : 'fail',
      fixture: relative(studioRoot, fixture),
      studioBinary,
      failures,
      comparisons,
      deterministic: {
        byteEqual: first.bytes.equals(second.bytes),
        firstDigest: first.digest,
        secondDigest: second.digest,
      },
      reportKindNormalization,
      normalizationRules: {
        selectionIdFallback: baseline.normalizationRules.selectionIdFallback,
        studioGeneratedNodeId:
          'A generated node-NNNNNN path segment is normalized to node/<ordinal> '
          + 'before comparing legacy business and effective Selection IDs.',
        colorPrecision: 6,
      },
      baseline: expected,
      native,
    };
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
}

async function buildStudioCase(studioBinary, temporary, name, baseline) {
  const output = join(temporary, `${name}.glb`);
  const reportPath = join(temporary, `${name}.report.json`);
  const options = baseline.options.default;
  const args = [
    'build',
    fixture,
    '--output',
    output,
    '--report',
    reportPath,
    '--asset-id',
    'prepare-cube',
    '--budget',
    options.budget,
    '--crease-angle',
    String(options.creaseAngleDegrees),
    ...(options.edges ? [] : ['--skip-edges']),
    '--json',
  ];
  run(studioBinary, args, studioRoot);
  const bytes = await readFile(output);
  const document = parseGlb(bytes);
  const report = JSON.parse(await readFile(reportPath, 'utf8'));
  return {
    bytes,
    digest: sha256(bytes),
    summary: summarizeStudio(document, report, baseline),
  };
}

function summarizeBaseline(baseline) {
  const fallback = baseline.normalizationRules.selectionIdFallback;
  const effectiveSelectionIds = baseline.default.selectionIds.length > 0
    ? baseline.default.selectionIds
    : fallback === 'ice3.id'
      ? baseline.default.nodeBusinessIds
      : [];
  return {
    sourceDigest: baseline.sourceDigest,
    edgesPrepared: baseline.default.edgesPrepared,
    stats: normalizeValue(baseline.default.stats),
    budget: normalizeBudget(baseline.default.budget),
    report: {
      kind: normalizeReportKind(baseline.default.report.kind).studio,
      schemaVersion: baseline.default.report.schemaVersion,
      stats: normalizeValue(
        baseline.default.report.stats ?? baseline.default.stats,
      ),
      budget: normalizeBudget(baseline.default.report.budget),
    },
    edgeRole: baseline.default.edgeRole,
    edgeWidthPx: baseline.default.edgeWidthPx,
    edgeColor: normalizeColor(baseline.default.edgeColor),
    edgeMaterialName: baseline.default.edgeMaterialName,
    businessNodeIds: normalizeNodeIds(baseline.default.nodeBusinessIds),
    nodeNames: [...baseline.default.nodeNames],
    selectionIds: normalizeNodeIds(effectiveSelectionIds),
    bounds: normalizeValue(baseline.default.bounds),
  };
}

function summarizeStudio(document, report, baseline) {
  const profile = document.json.asset?.extras?.ice3;
  if (!profile) throw new Error('Studio Prepared profile is missing');
  const metricKeys = Object.keys(baseline.default.stats);
  const nodes = document.json.nodes ?? [];
  const nodeNames = nodes
    .map((node) => node.name)
    .filter((name) => typeof name === 'string' && name.length > 0);
  const businessNodeIds = nodes
    .map((node) => node.extras?.ice3?.selectionId ?? node.extras?.ice3?.id)
    .filter(Boolean);
  const selectionIds = nodes
    .map((node) => node.extras?.ice3?.selectionId ?? node.extras?.ice3?.id)
    .filter(Boolean);
  const edgePrimitive = findEdgePrimitive(document.json);
  const edgeMaterial = edgePrimitive
    ? document.json.materials[edgePrimitive.material]
    : undefined;
  return {
    sourceDigest: stripDigestLabel(
      profile.sourceDigest ?? report.sourceDigest ?? '',
    ),
    edgesPrepared: profile.preparation?.edgesPrepared,
    stats: pickFields(profile.stats, metricKeys),
    budget: normalizeBudget(profile.budget),
    report: {
      kind: normalizeReportKind(report.kind).studio,
      schemaVersion: report.schemaVersion,
      stats: pickFields(report.metrics, metricKeys),
      budget: normalizeBudget(report.budget),
    },
    edgeRole: edgePrimitive?.extras?.ice3?.role,
    edgeWidthPx: edgePrimitive?.extras?.ice3?.widthPx,
    edgeColor: normalizeColor(
      edgeMaterial?.pbrMetallicRoughness?.baseColorFactor,
    ),
    edgeMaterialName: edgeMaterial?.name,
    businessNodeIds: normalizeNodeIds(businessNodeIds),
    nodeNames,
    selectionIds: normalizeNodeIds(selectionIds),
    bounds: normalizeValue(profile.bounds),
  };
}

function normalizeReportKind(kind) {
  if (kind === 'ice3-asset-report') {
    return {
      legacy: kind,
      studio: 'ice3-asset-build-report',
      applied: true,
    };
  }
  return {
    legacy: kind,
    studio: kind,
    applied: false,
  };
}

function normalizeBudget(budget) {
  return budget === undefined
    ? undefined
    : {
        profile: budget.profile,
        status: budget.status,
        warningCount: budget.warningCount,
        errorCount: budget.errorCount,
      };
}

function normalizeNodeIds(values) {
  return values.map((value) => {
    const duplicateSegment = value.match(/^(.*\/node)\/node-(\d{6})$/);
    if (duplicateSegment) {
      return `${duplicateSegment[1]}/${Number(duplicateSegment[2])}`;
    }
    const assetSegment = value.match(/^(.*)\/node-(\d{6})$/);
    if (assetSegment) {
      return `${assetSegment[1]}/node/${Number(assetSegment[2])}`;
    }
    const bareSegment = value.match(/^node-(\d{6})$/);
    if (bareSegment) return `node/${Number(bareSegment[1])}`;
    return value;
  });
}

function normalizeColor(color) {
  return color?.map((value) => Number(value.toFixed(6)));
}

function normalizeValue(value) {
  if (typeof value === 'number') return Number(value.toFixed(6));
  if (Array.isArray(value)) return value.map(normalizeValue);
  if (value && typeof value === 'object') {
    return Object.fromEntries(
      Object.entries(value)
        .sort(([left], [right]) => left.localeCompare(right, 'en'))
        .map(([key, nested]) => [key, normalizeValue(nested)]),
    );
  }
  return value;
}

function pickFields(value, keys) {
  if (!value || typeof value !== 'object') return undefined;
  return Object.fromEntries(keys.map((key) => [key, value[key]]));
}

function equal(left, right) {
  return JSON.stringify(normalizeValue(left)) === JSON.stringify(
    normalizeValue(right),
  );
}

function stripDigestLabel(value) {
  return value.startsWith('sha256:') ? value.slice('sha256:'.length) : value;
}

function resolveStudioBinary() {
  const configured = process.env.ICE3_STUDIO_BIN?.trim();
  if (configured) return resolve(studioRoot, configured);
  run('cargo', ['build', '-p', 'studio-cli'], studioRoot);
  const targetRoot = process.env.CARGO_TARGET_DIR
    ? resolve(studioRoot, process.env.CARGO_TARGET_DIR)
    : resolve(studioRoot, 'target');
  return join(
    targetRoot,
    'debug',
    process.platform === 'win32' ? 'studio-cli.exe' : 'studio-cli',
  );
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
      normalizationRules: {
        selectionIdFallback: 'ice3.id',
        reason:
          'Legacy Toolchain build omits node selectionId extras; its stable '
          + 'ice3.id identifies the source business node.',
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
    nodeBusinessIds: document.json.nodes
      ?.map((node) => node.extras?.ice3?.id)
      .filter(Boolean),
    nodeNames: document.json.nodes
      ?.map((node) => node.name)
      .filter((name) => typeof name === 'string' && name.length > 0),
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
  if (baseline.normalizationRules?.selectionIdFallback !== 'ice3.id') {
    throw new Error('selection fallback baseline changed');
  }
  if (
    JSON.stringify(baseline.default.nodeBusinessIds)
    !== JSON.stringify(['prepare-cube/node/0'])
  ) {
    throw new Error('default business node identity baseline changed');
  }
  if (
    JSON.stringify(baseline.default.nodeNames)
    !== JSON.stringify(['prepared-cube'])
  ) {
    throw new Error('default node-name baseline changed');
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
    '-a',
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
