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
const instanceFixturePath = join(
  studioRoot,
  'crates/studio-import/tests/fixtures/ifc/instanced.ifc',
);
const reportPath = join(studioRoot, 'output/ifc-import-parity.json');
const nativeCli = join(studioRoot, 'target/debug/studio-cli');
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
  await compareNative();
}

async function compareNative() {
  ensureNativeCli();
  const temporaryRoot = await mkdtemp(join(tmpdir(), 'ice3-ifc-parity-'));
  try {
    const baseline = JSON.parse(await readFile(baselinePath, 'utf8'));
    const fixtureResults = {};
    const failures = [];
    for (const [key, fixtureName, assetId] of fixtures) {
      const result = await compareFixture(
        temporaryRoot,
        key,
        fixtureName,
        assetId,
        baseline.fixtures[key],
      );
      fixtureResults[key] = result;
      failures.push(...result.failures.map((failure) => `${key}.${failure}`));
    }
    const instanceCorpus = await compareInstanceCorpus(temporaryRoot);
    failures.push(...instanceCorpus.failures.map((failure) => `instanceCorpus.${failure}`));
    const negativeCorpus = await runNegativeCorpus(temporaryRoot);
    failures.push(...Object.entries(negativeCorpus)
      .filter(([, result]) => result.status !== 'pass')
      .map(([name]) => `negativeCorpus.${name}`));
    const report = {
      kind: 'ice3-ifc-import-parity',
      schemaVersion: '1.0',
      status: failures.length === 0 ? 'pass' : 'fail',
      failures: [...new Set(failures)].sort(),
      fixtures: fixtureResults,
      instanceCorpus,
      negativeCorpus,
      allowedDifferences: [
        {
          key: 'triangleCount',
          reason: 'Tessellators may choose different triangle counts; normalized bounds, surface area, business identity and ratio bounds are enforced.',
        },
        {
          key: 'vertexCount',
          reason: 'Vertex welding and indexed layout differ while semantic counts and geometry bounds remain equivalent.',
        },
        {
          key: 'byteDigest',
          reason: 'Accessor layout, generated normals and preparation metadata differ deterministically.',
        },
      ],
    };
    await mkdir(dirname(reportPath), { recursive: true });
    await writeFile(reportPath, `${JSON.stringify(report, null, 2)}\n`);
    console.log(JSON.stringify(report, null, 2));
    if (report.status !== 'pass') process.exitCode = 1;
  } finally {
    await rm(temporaryRoot, { recursive: true, force: true });
  }
}

async function compareFixture(root, key, fixtureName, assetId, legacy) {
  const fixturePath = join(fixtureRoot, fixtureName);
  const outputPath = join(root, `${key}.native.glb`);
  const command = runNativeImport({
    fixture: fixturePath,
    output: outputPath,
    assetId,
  });
  const [sourceBytes, nativeBytes] = await Promise.all([
    readFile(fixturePath),
    readFile(outputPath),
  ]);
  const native = summarizeGlb(parseGlb(nativeBytes), assetId);
  const triangleRatio = native.triangleCount / legacy.stats.triangleCount;
  const normalizedLegacyBounds = normalizeLegacyIfcBounds(legacy.bounds);
  const normalizedNativeBounds = normalizeNativeIfcBounds(nativeBytes);
  const comparisons = {
    sourceDigest: {
      baseline: legacy.sourceDigest,
      native: sha256(sourceBytes),
      equal: legacy.sourceDigest === sha256(sourceBytes),
    },
    nodeCount: {
      baseline: legacy.stats.nodeCount,
      native: normalizedIfcNodeNames(native.nodeNames, native.selectionIds).count,
      equal: normalizedIfcNodeNames(native.nodeNames, native.selectionIds).count
        === normalizedIfcNodeNames(legacy.nodeNames, legacy.selectionIds).count,
    },
    nodeNames: {
      baseline: normalizedIfcNodeNames(legacy.nodeNames, legacy.selectionIds).names,
      native: normalizedIfcNodeNames(native.nodeNames, native.selectionIds).names,
      equal: JSON.stringify(normalizedIfcNodeNames(legacy.nodeNames, legacy.selectionIds).names)
        === JSON.stringify(normalizedIfcNodeNames(native.nodeNames, native.selectionIds).names),
    },
    selectionIds: {
      baseline: normalizeIfcSelectionIds(legacy.selectionIds, key),
      native: normalizeIfcSelectionIds(native.selectionIds, key),
      equal: JSON.stringify(normalizeIfcSelectionIds(legacy.selectionIds, key))
        === JSON.stringify(normalizeIfcSelectionIds(native.selectionIds, key)),
    },
    spatialPaths: {
      baseline: normalizedSpatialPaths(legacy.spatialPaths),
      native: normalizedSpatialPaths(native.spatialPaths),
      equal: JSON.stringify(normalizedSpatialPaths(legacy.spatialPaths))
        === JSON.stringify(normalizedSpatialPaths(native.spatialPaths)),
    },
    meshCount: {
      baseline: legacy.stats.meshCount,
      native: native.meshCount,
      equal: legacy.stats.meshCount === native.meshCount,
    },
    instanceNodeCount: {
      baseline: legacy.stats.instanceNodeCount,
      native: native.instanceNodeCount,
      equal: legacy.stats.instanceNodeCount === native.instanceNodeCount,
    },
    materialCount: {
      baseline: legacy.stats.semanticMaterialCount,
      native: native.materials.length,
      equal: legacy.stats.semanticMaterialCount === native.materials.length,
    },
    materials: {
      baseline: normalizeMaterialColours(legacy.materials, true),
      native: normalizeMaterialColours(native.materials, false),
      equal: normalizedMaterialColoursClose(legacy.materials, native.materials),
    },
    propertySets: {
      baseline: normalizePropertySets(legacy.propertySets),
      native: normalizePropertySets(native.propertySets),
      equal: JSON.stringify(normalizePropertySets(legacy.propertySets))
        === JSON.stringify(normalizePropertySets(native.propertySets)),
    },
    quantitySets: {
      baseline: normalizeQuantitySets(legacy.quantitySets),
      native: normalizeQuantitySets(native.quantitySets),
      equal: JSON.stringify(normalizeQuantitySets(legacy.quantitySets))
        === JSON.stringify(normalizeQuantitySets(native.quantitySets)),
    },
    systems: {
      baseline: normalizeSystems(legacy.systems),
      native: normalizeSystems(native.systems),
      equal: JSON.stringify(normalizeSystems(legacy.systems))
        === JSON.stringify(normalizeSystems(native.systems)),
    },
    bounds: {
      baseline: normalizedLegacyBounds,
      native: normalizedNativeBounds,
      equal: normalizedBoundsClose(
        normalizedLegacyBounds,
        normalizedNativeBounds,
      ),
    },
    surfaceArea: {
      baseline: legacy.surfaceArea,
      native: native.surfaceArea,
      relativeError: relativeError(native.surfaceArea, legacy.surfaceArea),
      equal: relativeError(native.surfaceArea, legacy.surfaceArea) <= 0.02,
    },
    triangleCount: {
      baseline: legacy.stats.triangleCount,
      native: native.triangleCount,
      ratio: triangleRatio,
      equal: triangleRatio >= 0.75 && triangleRatio <= 1.33,
    },
    warningCodes: {
      baseline: normalizeWarningCodes(
        legacy.warningCodes,
        key === 'orphanProduct'
          && native.nodeNames.includes('Orphan Pump'),
      ),
      native: normalizeWarningCodes(warningCodes(command.warnings)),
      equal: JSON.stringify(normalizeWarningCodes(
        legacy.warningCodes,
        key === 'orphanProduct'
          && native.nodeNames.includes('Orphan Pump'),
      ))
        === JSON.stringify(normalizeWarningCodes(warningCodes(command.warnings))),
    },
  };
  const failures = Object.entries(comparisons)
    .filter(([, value]) => value.equal !== true)
    .map(([name]) => name);
  return {
    status: failures.length === 0 ? 'pass' : 'fail',
    fixture: relative(studioRoot, fixturePath),
    sourceDigest: comparisons.sourceDigest.native,
    byteDigest: sha256(nativeBytes),
    vertexCount: native.vertexCount,
    comparisons,
    failures,
  };
}

async function compareInstanceCorpus(root) {
  const outputPath = join(root, 'ifc-instances.glb');
  const command = runNativeImport({
    fixture: instanceFixturePath,
    output: outputPath,
    assetId: 'parity-ifc-instances',
  });
  const [sourceBytes, nativeBytes] = await Promise.all([
    readFile(instanceFixturePath),
    readFile(outputPath),
  ]);
  const native = summarizeGlb(parseGlb(nativeBytes), 'parity-ifc-instances');
  const meshNodeSelections = native.nodeSelections.filter((pair) => (
    pair.mesh !== undefined
  ));
  const comparisons = {
    sharedMeshNodeCount: {
      native: native.sharedMeshNodeCount,
      equal: native.sharedMeshNodeCount >= 2,
    },
    instanceNodeCount: {
      native: native.instanceNodeCount,
      equal: native.instanceNodeCount >= 2,
    },
    distinctSelectionIds: {
      native: meshNodeSelections.map((pair) => pair.selectionId),
      equal: new Set(meshNodeSelections.map((pair) => pair.selectionId)).size
        === meshNodeSelections.length,
    },
    warningCodes: {
      native: normalizeWarningCodes(warningCodes(command.warnings)),
      equal: normalizeWarningCodes(warningCodes(command.warnings)).length === 0,
    },
  };
  const failures = Object.entries(comparisons)
    .filter(([, value]) => value.equal !== true)
    .map(([name]) => name);
  return {
    status: failures.length === 0 ? 'pass' : 'fail',
    fixture: relative(studioRoot, instanceFixturePath),
    sourceDigest: sha256(sourceBytes),
    byteDigest: sha256(nativeBytes),
    meshNodeCount: native.meshNodeCount,
    comparisons,
    failures,
  };
}

async function runNegativeCorpus(root) {
  const negativeRoot = join(root, 'negative');
  await mkdir(negativeRoot, { recursive: true });
  return {
    unsupportedSchema: await expectFailure(negativeRoot, 'unsupported-schema', {
      fixture: join(fixtureRoot, 'ifc4-legacy.ifc'),
      expectedCode: 'ICE3_STUDIO_IMPORT_IFC_SCHEMA_UNSUPPORTED',
    }),
    unsupportedUnit: await expectFailure(negativeRoot, 'unsupported-unit', {
      fixture: join(fixtureRoot, 'unsupported-unit.ifc'),
      expectedCode: 'ICE3_STUDIO_IMPORT_IFC_UNIT_UNSUPPORTED',
    }),
    conflictingUnits: await expectFailure(negativeRoot, 'conflicting-units', {
      fixture: join(fixtureRoot, 'conflicting-units.ifc'),
      expectedCode: 'ICE3_STUDIO_IMPORT_IFC_UNIT_UNSUPPORTED',
    }),
    empty: await expectFailure(negativeRoot, 'empty', {
      fixture: join(fixtureRoot, 'empty-model.ifc'),
      expectedCode: 'ICE3_STUDIO_IMPORT_EMPTY_GEOMETRY',
    }),
    truncated: await expectFailure(negativeRoot, 'truncated', {
      fixture: join(fixtureRoot, 'truncated.ifc'),
      expectedCode: 'ICE3_STUDIO_IMPORT_INVALID_SOURCE',
    }),
    limitExceeded: await expectFailure(negativeRoot, 'limit', {
      fixture: join(fixtureRoot, 'synthetic-plant.ifc'),
      expectedCode: 'ICE3_STUDIO_IMPORT_LIMIT_EXCEEDED',
      environment: {
        ...process.env,
        ICE3_STUDIO_IMPORT_MAX_TRIANGLES: '1',
      },
    }),
    timeout: await expectFailure(negativeRoot, 'timeout', {
      fixture: join(fixtureRoot, 'synthetic-plant.ifc'),
      expectedCode: 'ICE3_STUDIO_IMPORT_TIMEOUT',
      preserveOutput: 'keep',
      force: true,
      environment: {
        ...process.env,
        ICE3_STUDIO_IMPORT_WORKER_TIMEOUT_MS: '1',
      },
    }),
  };
}

async function expectFailure(root, name, options) {
  const output = join(root, `${name}.glb`);
  if (options.preserveOutput !== undefined) {
    await writeFile(output, options.preserveOutput);
  }
  const args = [
    'import',
    options.fixture,
    '--output', output,
    '--asset-id', `negative-${name}`,
    '--adapter', 'ifc',
  ];
  if (options.force) args.push('--force');
  const result = spawnSync(nativeCli, args, {
    cwd: studioRoot,
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
    env: options.environment ?? process.env,
  });
  const outputDisposition = options.preserveOutput !== undefined
    ? (await readFile(output, 'utf8')) === options.preserveOutput
      ? 'preserved'
      : 'changed'
    : await readFile(output).then(() => 'present', () => 'absent');
  const status = (
    result.status !== 0
    && result.stderr.includes(options.expectedCode)
    && ['absent', 'preserved'].includes(outputDisposition)
  ) ? 'pass' : 'fail';
  return {
    status,
    exitCode: result.status,
    expectedCode: options.expectedCode,
    diagnosticPresent: result.stderr.includes(options.expectedCode),
    outputDisposition,
  };
}

function ensureNativeCli() {
  const result = spawnSync('cargo', ['build', '--quiet', '-p', 'studio-cli'], {
    cwd: studioRoot,
    encoding: 'utf8',
    maxBuffer: 16 * 1024 * 1024,
  });
  if (result.status !== 0) {
    throw new Error(result.stderr || result.stdout || 'studio-cli build failed');
  }
}

function runNativeImport({ fixture, output, assetId }) {
  const result = spawnSync(nativeCli, [
    'import',
    fixture,
    '--output', output,
    '--asset-id', assetId,
    '--adapter', 'ifc',
  ], {
    cwd: studioRoot,
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
  });
  if (result.status !== 0) {
    throw new Error(`native IFC import failed:\n${result.stderr}\n${result.stdout}`);
  }
  return JSON.parse(result.stdout);
}

function normalizePropertySets(sets) {
  return (sets ?? [])
    .map((set) => ({
      name: set.name,
      properties: sortedRecord(set.properties ?? {}),
    }))
    .sort((left, right) => left.name.localeCompare(right.name, 'en'));
}

function normalizeQuantitySets(sets) {
  return (sets ?? [])
    .map((set) => ({
      name: set.name,
      properties: sortedRecord(set.quantities ?? set.properties ?? {}),
    }))
    .sort((left, right) => left.name.localeCompare(right.name, 'en'));
}

function normalizeSystems(systems) {
  return (systems ?? [])
    .map((system) => ({
      ...(system.globalId ? { globalId: system.globalId } : {}),
      name: system.name,
      ifcType: system.ifcType,
    }))
    .sort((left, right) => (
      String(left.globalId ?? '').localeCompare(String(right.globalId ?? ''), 'en')
      || left.name.localeCompare(right.name, 'en')
    ));
}

function normalizeWarningCodes(codes, ignoreLegacyUnmapped = false) {
  return [...new Set(codes.map((code) => (
    String(code)
      .replace(/^ICE3_STUDIO_IMPORT_/u, '')
      .replace(/:\d+$/u, '')
  )).filter((code) => (
    !(ignoreLegacyUnmapped && code === 'IFC_UNMAPPED_GEOMETRY')
  )))].sort();
}

function normalizeLegacyIfcBounds(bounds) {
  const corners = [];
  for (const x of [bounds.min[0], bounds.max[0]]) {
    for (const y of [bounds.min[1], bounds.max[1]]) {
      for (const z of [bounds.min[2], bounds.max[2]]) {
        corners.push([x, z, -y]);
      }
    }
  }
  const min = [0, 1, 2].map((axis) => Math.min(...corners.map((corner) => corner[axis])));
  const max = [0, 1, 2].map((axis) => Math.max(...corners.map((corner) => corner[axis])));
  return normalizeBounds({ min, max });
}

function normalizeNativeIfcBounds(bytes) {
  const glb = parseGlb(bytes);
  const assetExtras = glb.json.asset?.extras?.ice3;
  const metadata = assetExtras?.ice3?.ifc ?? assetExtras?.ifc;
  const bounds = assetExtras?.bounds;
  if (!metadata || !bounds) throw new Error('native IFC bounds metadata is missing');
  return {
    sourceOrigin: metadata.sourceOrigin,
    min: bounds.min,
    max: bounds.max,
  };
}

function normalizeBounds(bounds) {
  const sourceOrigin = [bounds.min[0], 0, bounds.min[2]];
  return {
    sourceOrigin,
    min: [0, bounds.min[1], 0],
    max: [
      bounds.max[0] - sourceOrigin[0],
      bounds.max[1],
      bounds.max[2] - sourceOrigin[2],
    ],
  };
}

function normalizedBoundsClose(left, right) {
  return ['min', 'max'].every((key) => (
    [0, 1, 2].every((axis) => {
      const extent = Math.abs(left.max[axis] - left.min[axis]);
      const tolerance = Math.max(0.0001, extent * 0.00001);
      return Math.abs(left[key][axis] - right[key][axis]) <= tolerance;
    })
  ));
}

function relativeError(value, reference) {
  if (reference === 0) return value === 0 ? 0 : Infinity;
  return Math.abs(value - reference) / Math.abs(reference);
}

function materialsClose(left, right) {
  return left.length === right.length
    && left.every((material, index) => (
      material.baseColorFactor.every((value, component) => (
        Math.abs(value - right[index].baseColorFactor[component]) <= 1e-5
      ))
    ));
}

function normalizedMaterialColoursClose(left, right) {
  const baseline = normalizeMaterialColours(left, true);
  const native = normalizeMaterialColours(right, false);
  return baseline.length === native.length
    && baseline.every((colour, index) => colour.every((value, component) => (
      Math.abs(value - native[index][component]) <= 1e-5
    )));
}

function normalizeMaterialColours(materials, fromSrgb) {
  return [...materials]
    .map((material) => material.baseColorFactor.map((component, index) => (
      index === 3 || !fromSrgb
        ? component
        : srgbToLinear(component)
    )))
    .sort((left, right) => JSON.stringify(left).localeCompare(
      JSON.stringify(right),
      'en',
    ));
}

function srgbToLinear(component) {
  return component <= 0.04045
    ? component / 12.92
    : ((component + 0.055) / 1.055) ** 2.4;
}

function sortedRecord(value) {
  return Object.fromEntries(
    Object.entries(value).sort(([left], [right]) => left.localeCompare(right, 'en')),
  );
}

function normalizedIfcNodeNames(names) {
  const normalized = names
    .filter((name) => name !== 'Unassigned IFC Products')
    .sort((left, right) => left.localeCompare(right, 'en'));
  return { names: normalized, count: normalized.length };
}

function normalizedSpatialPaths(paths) {
  return [...new Set(paths
    .map((path) => path.replace(/\/Unassigned IFC Products(?=\/|$)/u, ''))
    .filter(Boolean))]
    .sort((left, right) => left.localeCompare(right, 'en'));
}

function normalizeIfcSelectionIds(ids, fixtureKey) {
  return ids
    .filter((id) => !id.endsWith('/unassigned'))
    .map((id) => (
      fixtureKey === 'duplicateGlobalId'
        ? id.replace(/\/express\/\d+$/u, '/express/<duplicate>')
        : id
    ))
    .sort((left, right) => left.localeCompare(right, 'en'));
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
  const propertySets = uniqueByName(
    ifcNodes.flatMap((entry) => entry.extras.propertySets ?? []),
  );
  const quantitySets = uniqueByName(
    ifcNodes.flatMap((entry) => entry.extras.quantitySets ?? []),
  );
  const meshNodes = nodes.filter((node) => Number.isInteger(node.mesh));
  const sharedMeshNodeCount = nodes.filter((node) => (
    Number.isInteger(node.mesh)
    && (meshReferences.get(node.mesh) ?? 0) > 1
  )).length;
  const spatialTypes = new Set([
    'IFCPROJECT',
    'IFCSITE',
    'IFCBUILDING',
    'IFCBUILDINGSTOREY',
    'IFCSPACE',
    'IFCUNASSIGNED',
    'IFCUNASSIGNEDPRODUCTS',
  ]);
  const nodeSelections = nodes
    .map((node) => {
      const ifc = node.extras?.ice3?.ifc;
      return {
        name: node.name ?? '',
        mesh: node.mesh,
        selectionId: node.extras?.ice3?.id
          ?? node.extras?.ice3?.selectionId,
        business: ifc !== undefined
          && !spatialTypes.has(String(ifc.ifcType).toUpperCase()),
      };
    })
    .filter((pair) => pair.selectionId !== undefined);
  return {
    nodeCount: nodes.length,
    meshCount: meshes.length,
    meshNodeCount: meshNodes.length,
    sharedMeshNodeCount,
    triangleCount,
    vertexCount: [...positionAccessors].reduce(
      (total, index) => total + glb.json.accessors[index].count,
      0,
    ),
    instanceNodeCount: nodes.filter((node) => (
      Number.isInteger(node.mesh)
      && (node.extras?.ice3?.ifc?.instanceGroup
        ?? node.extras?.ice3?.instanceGroup) !== undefined
    )).length,
    nodeNames: nodes.map((node) => node.name ?? '').filter(Boolean),
    selectionIds: nodeSelections
      .filter((pair) => pair.business)
      .map((pair) => pair.selectionId)
      .sort(),
    nodeSelections,
    spatialPaths: [...new Set(
      nodes.map((_, index) => nodePath(index)),
    )].sort(),
    propertySets,
    quantitySets,
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
