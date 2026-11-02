import { spawnSync } from 'node:child_process';
import {
  access,
  mkdir,
  mkdtemp,
  readFile,
  rm,
  writeFile,
} from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const studioRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const engineRoot = resolve(
  process.env.ICE3_ENGINE_ROOT ?? join(studioRoot, '..', 'ice3-engine'),
);
const fixture = resolve(
  studioRoot,
  'fixtures/source-import/gltf/prepare-cube.gltf',
);
const baselinePath = resolve(
  studioRoot,
  'fixtures/parity/prepare-engine-v1.json',
);
const reportPath = resolve(studioRoot, 'output/prepare-engine-contract.json');

if (
  process.argv[1]
  && resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  const mode = process.argv[2] ?? '--compare';
  if (mode !== '--compare' && mode !== '--capture') {
    console.error('usage: node scripts/verify-prepare-engine.mjs [--compare|--capture]');
    process.exitCode = 2;
  } else {
    const report = await verify(mode).catch((error) => ({
      kind: 'ice3-prepare-engine-contract',
      schemaVersion: '1.0',
      status: 'fail',
      failures: ['execution'],
      error: error instanceof Error ? error.message : String(error),
    }));
    await mkdir(dirname(reportPath), { recursive: true });
    await writeFile(reportPath, `${JSON.stringify(report, null, 2)}\n`);
    console.log(JSON.stringify(report, null, 2));
    if (report.status !== 'pass') process.exitCode = 1;
  }
}

async function verify(mode) {
  const engineEntry = resolve(engineRoot, 'packages/assets/dist/index.js');
  try {
    await access(engineEntry);
  } catch {
    throw new Error(
      `Engine assets build is missing; run npm --prefix ${engineRoot} run build`,
    );
  }
  const engine = await import(pathToFileURL(engineEntry).href);
  for (const name of [
    'readPreparedGLB',
    'verifyPreparedGLBIntegrity',
    'readTileset',
  ]) {
    if (typeof engine[name] !== 'function') {
      throw new Error(`Engine asset export is missing: ${name}`);
    }
  }
  const baseline = JSON.parse(await readFile(baselinePath, 'utf8'));
  assertBaseline(baseline);
  const temporary = await mkdtemp(join(tmpdir(), 'ice3-prepare-engine-'));
  try {
    const studioBinary = resolveStudioBinary();
    const builtPath = join(temporary, 'prepare-cube.glb');
    run(studioBinary, [
      'build',
      fixture,
      '--output',
      builtPath,
      '--asset-id',
      baseline.build.assetId,
      '--json',
    ]);
    const builtBytes = await readFile(builtPath);
    const builtArrayBuffer = exactArrayBuffer(builtBytes);
    const integrity = await engine.verifyPreparedGLBIntegrity(builtArrayBuffer);
    const prepared = engine.readPreparedGLB(builtArrayBuffer);

    const tilesetDirectory = join(temporary, 'tileset');
    run(studioBinary, [
      'partition',
      fixture,
      '--output',
      tilesetDirectory,
      '--asset-id',
      baseline.build.assetId,
      '--max-triangles',
      '500000',
      '--json',
    ]);
    const tilesetPath = join(tilesetDirectory, 'tileset.json');
    const tilesetText = await readFile(tilesetPath, 'utf8');
    const tileset = engine.readTileset(
      tilesetText,
      pathToFileURL(tilesetPath).href,
    );
    const contentUris = [...new Set(
      tileset.tiles.flatMap((tile) => tile.contentUris),
    )];
    let loadedParts = 0;
    let runtimeEdgeCount = 0;
    let runtimeTriangleCount = 0;
    const runtimeSelections = new Set();
    const runtimeEdgeColors = new Set();
    for (const uri of contentUris) {
      const bytes = await readFile(fileURLToPath(uri));
      const part = engine.readPreparedGLB(exactArrayBuffer(bytes));
      loadedParts += 1;
      runtimeEdgeCount += part.stats.edgeSegmentCount;
      runtimeTriangleCount += part.stats.triangleCount;
      for (const node of part.nodes) runtimeSelections.add(node.selectionId);
      for (const material of part.materials) {
        if (material.edgeColor) runtimeEdgeColors.add(material.edgeColor);
      }
    }

    const edgeMaterials = prepared.materials.filter(
      (material) => material.edgeColor,
    );
    const engineEdgeCount = prepared.geometries.reduce(
      (total, geometry) => total + (geometry.edgeIndices?.length ?? 0) / 2,
      0,
    );
    const build = {
      assetId: prepared.assetId,
      stats: prepared.stats,
    };
    const rawSelectionId = prepared.nodes[0]?.selectionId;
    const engineEvidence = {
      integrity,
      contractStatus: prepared.contract.status,
      edgesPrepared: prepared.contract.preparation?.edgesPrepared,
      edgeCount: engineEdgeCount,
      edgeMaterialColor: edgeMaterials[0]?.edgeColor,
      selectionId: normalizeGeneratedSelectionId(rawSelectionId),
      rawSelectionId,
      stats: prepared.stats,
    };
    const runtimeTileset = {
      kind: 'direct-partition-tileset',
      tileCount: tileset.tiles.length,
      partCount: contentUris.length,
      loadedParts,
      triangleCount: runtimeTriangleCount,
      edgeSegmentCount: runtimeEdgeCount,
      edgeMaterialColors: [...runtimeEdgeColors].sort(),
      selectionIds: [...runtimeSelections].sort(),
    };
    const failures = compareBaseline(baseline, build, engineEvidence, runtimeTileset);
    const report = {
      kind: 'ice3-prepare-engine-contract',
      schemaVersion: '1.0',
      status: failures.length === 0 ? 'pass' : 'fail',
      fixture: 'fixtures/source-import/gltf/prepare-cube.gltf',
      engineEntry: 'ice3-engine/packages/assets/dist/index.js',
      baselineVersion: baseline.schemaVersion,
      failures,
      build,
      engine: engineEvidence,
      runtimeTileset,
    };
    if (mode === '--capture') {
      const captured = {
        kind: 'ice3-prepare-engine-baseline',
        schemaVersion: '1.0',
        build: {
          assetId: build.assetId,
          stats: build.stats,
        },
        engine: {
          integrity: engineEvidence.integrity,
          contractStatus: engineEvidence.contractStatus,
          edgesPrepared: engineEvidence.edgesPrepared,
          edgeCount: engineEvidence.edgeCount,
          edgeMaterialColor: engineEvidence.edgeMaterialColor,
          selectionId: engineEvidence.selectionId,
          rawSelectionId: engineEvidence.rawSelectionId,
        },
        runtimeTileset: {
          minimumParts: runtimeTileset.partCount,
          requiresEdges: true,
          requiredEdgeColor: '#e6f2ff',
        },
      };
      await writeFile(baselinePath, `${JSON.stringify(captured, null, 2)}\n`);
      report.baselineCaptured = true;
    }
    return report;
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
}

function compareBaseline(baseline, build, engine, runtimeTileset) {
  const failures = [];
  if (build.assetId !== baseline.build.assetId) failures.push('build.assetId');
  for (const [key, expected] of Object.entries(baseline.build.stats)) {
    if (build.stats[key] !== expected) failures.push(`build.stats.${key}`);
  }
  for (const [key, expected] of Object.entries(baseline.engine)) {
    if (engine[key] !== expected) failures.push(`engine.${key}`);
  }
  if (runtimeTileset.loadedParts !== runtimeTileset.partCount) {
    failures.push('runtimeTileset.loadedParts');
  }
  if (runtimeTileset.partCount < baseline.runtimeTileset.minimumParts) {
    failures.push('runtimeTileset.partCount');
  }
  if (
    baseline.runtimeTileset.requiresEdges
    && runtimeTileset.edgeSegmentCount <= 0
  ) {
    failures.push('runtimeTileset.edgeSegmentCount');
  }
  if (
    baseline.runtimeTileset.requiredEdgeColor
    && !runtimeTileset.edgeMaterialColors.includes(
      baseline.runtimeTileset.requiredEdgeColor,
    )
  ) {
    failures.push('runtimeTileset.edgeMaterialColor');
  }
  return failures;
}

function assertBaseline(baseline) {
  if (
    baseline.kind !== 'ice3-prepare-engine-baseline'
    || baseline.schemaVersion !== '1.0'
    || !baseline.build
    || !baseline.engine
    || !baseline.runtimeTileset
  ) {
    throw new Error('prepare Engine baseline is invalid');
  }
}

function run(command, args) {
  const result = spawnSync(command, args, {
    cwd: studioRoot,
    encoding: 'utf8',
    timeout: 120_000,
  });
  if (result.error || result.status !== 0) {
    throw new Error(
      `${command} ${args.slice(0, 2).join(' ')} failed: `
      + `${result.error?.message ?? result.stderr}`,
    );
  }
  return result.stdout;
}

function resolveStudioBinary() {
  if (process.env.STUDIO_CLI_BIN) return resolve(process.env.STUDIO_CLI_BIN);
  return resolve(
    studioRoot,
    'target/debug',
    process.platform === 'win32' ? 'studio-cli.exe' : 'studio-cli',
  );
}

function exactArrayBuffer(bytes) {
  return bytes.buffer.slice(
    bytes.byteOffset,
    bytes.byteOffset + bytes.byteLength,
  );
}

function normalizeGeneratedSelectionId(value) {
  return typeof value === 'string'
    ? value.replace(/\/node-(\d{6})$/, (_, ordinal) => `/${Number(ordinal)}`)
    : value;
}
