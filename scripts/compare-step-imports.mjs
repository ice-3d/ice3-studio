import { createHash } from 'node:crypto';
import {
  access,
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
const fixtureRoot = join(studioRoot, 'fixtures/source-import/step');
const fixturePath = join(fixtureRoot, 'synthetic-skid.step');
const baselinePath = join(
  studioRoot,
  'fixtures/parity/step-baseline-v1.json',
);
const instancedBaselinePath = join(
  studioRoot,
  'fixtures/parity/step-instanced-baseline-v1.json',
);
const reportPath = join(studioRoot, 'output/step-import-parity.json');
const nativeCli = join(studioRoot, 'target/debug/studio-cli');
const assetId = 'parity-step';
const instancedAssetId = 'parity-step-instances';
const mode = process.argv[2] ?? '--compare';
const repeatedOccurrence = `#300 = CONTEXT_DEPENDENT_SHAPE_REPRESENTATION(#301,#303);
#301 = ( REPRESENTATION_RELATIONSHIP('','',#36,#10)
REPRESENTATION_RELATIONSHIP_WITH_TRANSFORMATION(#302)
SHAPE_REPRESENTATION_RELATIONSHIP() );
#302 = ITEM_DEFINED_TRANSFORMATION('','',#11,#15);
#303 = PRODUCT_DEFINITION_SHAPE('Placement','Placement of an item',#304);
#304 = NEXT_ASSEMBLY_USAGE_OCCURRENCE('3','PrimaryTank Repeat','',#5,#31,$);
`;

if (!['--compare', '--capture'].includes(mode)) {
  console.error('usage: node scripts/compare-step-imports.mjs [--compare|--capture]');
  process.exit(2);
}

if (mode === '--capture') {
  await captureBaseline();
} else {
  await compareNative();
}

async function captureBaseline() {
  const temporaryRoot = await mkdtemp(join(tmpdir(), 'ice3-step-baseline-'));
  try {
    const outputPath = join(temporaryRoot, 'step.glb');
    const reportFile = join(temporaryRoot, 'step.report.json');
    const command = spawnSync('node', [
      '--import', 'tsx', 'packages/cli/src/main.ts', 'import',
      fixturePath,
      '--output', outputPath,
      '--report', reportFile,
      '--asset-id', assetId,
      '--adapter', 'step',
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
      readFile(reportFile),
      readFile(outputPath),
    ]);
    const report = JSON.parse(reportBytes.toString('utf8'));
    const summary = summarizeGlb(parseGlb(outputBytes));
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
          meshNodeCount: summary.meshNodeCount,
          sharedMeshNodeCount: summary.sharedMeshNodeCount,
          primitiveCount: report.metrics.primitiveCount,
          triangleCount: summary.triangleCount,
          vertexCount: summary.vertexCount,
          instanceNodeCount: summary.instanceNodeCount,
          semanticMaterialCount: summary.materials.length,
          edgeSegmentCount: summary.edgeSegmentCount,
          encodedBytes: report.metrics.encodedBytes,
        },
        legacySourceDigest: report.source.digest,
        contentDigest: report.asset.contentDigest,
        nodeNames: summary.nodeNames,
        selectionIds: summary.selectionIds,
        bounds: summary.bounds,
        surfaceArea: summary.surfaceArea,
        materials: summary.materials,
        warningCodes: warningCodes(report.adapter.warnings),
      },
      normalizationRules: [
        'Tessellation triangles may differ while normalized bounds, area and business identity remain equivalent.',
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

    const instancedFixturePath = join(temporaryRoot, 'instanced.step');
    const instancedOutputPath = join(temporaryRoot, 'instanced.glb');
    const instancedReportFile = join(temporaryRoot, 'instanced.report.json');
    await writeInstancedFixture(instancedFixturePath);
    const instancedCommand = spawnSync('node', [
      '--import', 'tsx', 'packages/cli/src/main.ts', 'import',
      instancedFixturePath,
      '--output', instancedOutputPath,
      '--report', instancedReportFile,
      '--asset-id', instancedAssetId,
      '--adapter', 'step',
      '--json',
    ], {
      cwd: toolchainRoot,
      encoding: 'utf8',
      maxBuffer: 64 * 1024 * 1024,
    });
    if (instancedCommand.status !== 0) {
      throw new Error(
        `Toolchain STEP instance baseline failed:\n${instancedCommand.stderr}\n${instancedCommand.stdout}`,
      );
    }
    const [instancedSource, instancedReportBytes, instancedOutput] =
      await Promise.all([
        readFile(instancedFixturePath),
        readFile(instancedReportFile),
        readFile(instancedOutputPath),
      ]);
    const instancedReport = JSON.parse(instancedReportBytes.toString('utf8'));
    const instancedSummary = summarizeGlb(parseGlb(instancedOutput));
    const instancedBaseline = {
      kind: 'ice3-step-import-instance-baseline',
      schemaVersion: '1.0',
      generator: 'append one PrimaryTank usage occurrence to synthetic-skid.step',
      assetId: instancedAssetId,
      sourceDigest: sha256(instancedSource),
      legacy: {
        tool: instancedReport.tool,
        adapter: instancedReport.adapter,
        stats: {
          nodeCount: instancedSummary.nodeCount,
          meshCount: instancedSummary.meshCount,
          meshNodeCount: instancedSummary.meshNodeCount,
          sharedMeshNodeCount: instancedSummary.sharedMeshNodeCount,
          triangleCount: instancedSummary.triangleCount,
          vertexCount: instancedSummary.vertexCount,
          instanceNodeCount: instancedSummary.instanceNodeCount,
          semanticMaterialCount: instancedSummary.materials.length,
        },
        nodeNames: instancedSummary.nodeNames,
        selectionIds: instancedSummary.selectionIds,
        bounds: instancedSummary.bounds,
        materials: instancedSummary.materials,
        warningCodes: warningCodes(instancedReport.adapter.warnings),
      },
      normalizationRules: [
        'The legacy bridge duplicates repeated part geometry into separate meshes; native STEP must reuse the source mesh and report at least two shared-instance nodes.',
      ],
    };
    assertInstancedBaseline(instancedBaseline);
    await writeFile(
      instancedBaselinePath,
      `${JSON.stringify(instancedBaseline, null, 2)}\n`,
    );
    console.log(JSON.stringify({
      status: 'captured',
      baseline: relative(studioRoot, instancedBaselinePath),
      stats: instancedBaseline.legacy.stats,
    }, null, 2));
  } finally {
    await rm(temporaryRoot, { recursive: true, force: true });
  }
}

async function compareNative() {
  const temporaryRoot = await mkdtemp(join(tmpdir(), 'ice3-step-parity-'));
  try {
    ensureNativeCli();
    assertBoundsComparatorRejectsTranslation();
    assertNodeSelectionPairComparatorRejectsSwap();
    const baseline = JSON.parse(await readFile(baselinePath, 'utf8'));
    const outputPath = join(temporaryRoot, 'native-step.glb');
    const command = runNativeImport({
      fixture: fixturePath,
      output: outputPath,
      adapter: 'step',
      assetId,
    });
    const [sourceBytes, nativeBytes] = await Promise.all([
      readFile(fixturePath),
      readFile(outputPath),
    ]);
    const native = summarizeGlb(parseGlb(nativeBytes));
    const legacy = baseline.legacy;
    const triangleRatio = native.triangleCount / legacy.stats.triangleCount;
    const comparisons = {
      sourceDigest: {
        baseline: baseline.sourceDigest,
        native: sha256(sourceBytes),
        equal: baseline.sourceDigest === sha256(sourceBytes),
      },
      nodeCount: {
        baseline: legacy.stats.nodeCount,
        native: native.nodeCount,
        equal: legacy.stats.nodeCount === native.nodeCount,
      },
      nodeNames: {
        baseline: legacy.nodeNames,
        native: native.nodeNames,
        equal: JSON.stringify(legacy.nodeNames) === JSON.stringify(native.nodeNames),
      },
      selectionIds: {
        baseline: legacy.selectionIds,
        native: native.selectionIds,
        nativePrefix: `${assetId}/step/node/`,
        equal: selectionIdsEquivalent(legacy.selectionIds, native.selectionIds, assetId),
      },
      nodeSelectionPairs: {
        baseline: normalizedNodeSelectionPairs(
          legacySelectionPairs(legacy),
          assetId,
        ),
        native: normalizedNodeSelectionPairs(native.nodeSelections, assetId),
        equal: nodeSelectionPairsEquivalent(
          legacySelectionPairs(legacy),
          native.nodeSelections,
          assetId,
        ),
      },
      meshCount: {
        baseline: legacy.stats.meshCount,
        native: native.meshCount,
        equal: legacy.stats.meshCount === native.meshCount,
      },
      meshNodeCount: {
        baseline: legacy.stats.meshNodeCount,
        native: native.meshNodeCount,
        equal: legacy.stats.meshNodeCount === native.meshNodeCount,
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
        baseline: legacy.materials,
        native: native.materials,
        equal: materialsClose(legacy.materials, native.materials),
      },
      normalizedBounds: {
        baseline: normalizeBounds(legacy.bounds),
        native: {
          sourceOrigin: native.sourceOrigin,
          min: native.bounds.min,
          max: native.bounds.max,
        },
        equal: normalizedBoundsClose(
          normalizeBounds(legacy.bounds),
          {
            sourceOrigin: native.sourceOrigin,
            min: native.bounds.min,
            max: native.bounds.max,
          },
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
        baseline: legacy.warningCodes,
        native: warningCodes(command.warnings),
        equal: JSON.stringify(legacy.warningCodes)
          === JSON.stringify(warningCodes(command.warnings)),
      },
    };
    const failures = Object.entries(comparisons)
      .filter(([, value]) => value.equal !== true)
      .map(([key]) => key);
    const instanceCorpus = await compareInstanceCorpus(temporaryRoot);
    failures.push(...instanceCorpus.failures.map((failure) => `instanceCorpus.${failure}`));
    const report = {
      kind: 'ice3-step-import-parity',
      schemaVersion: '1.0',
      status: failures.length === 0 ? 'pass' : 'fail',
      failures,
      fixture: relative(studioRoot, fixturePath),
      sourceDigest: comparisons.sourceDigest.native,
      byteDigest: sha256(nativeBytes),
      comparisons,
      instanceCorpus,
      allowedDifferences: [
        {
          key: 'triangleCount',
          reason: 'Tessellators may choose different triangle counts; normalized bounds, surface area, business identity and ratio bounds are enforced.',
        },
        {
          key: 'byteDigest',
          reason: 'Accessor layout and generated normals differ deterministically.',
        },
      ],
      negativeCorpus: await runNegativeCorpus(temporaryRoot),
    };
    for (const [name, result] of Object.entries(report.negativeCorpus)) {
      if (result.status !== 'pass') report.failures.push(`negativeCorpus.${name}`);
    }
    if (report.failures.length > 0) report.status = 'fail';
    await mkdir(dirname(reportPath), { recursive: true });
    await writeFile(reportPath, `${JSON.stringify(report, null, 2)}\n`);
    console.log(JSON.stringify(report, null, 2));
    if (report.status !== 'pass') process.exitCode = 1;
  } finally {
    await rm(temporaryRoot, { recursive: true, force: true });
  }
}

function assertBaseline(value) {
  if (value.kind !== 'ice3-step-import-baseline') throw new Error('baseline kind');
  if (value.legacy.stats.nodeCount !== 6) throw new Error('node baseline');
  if (value.legacy.stats.meshCount !== 2) throw new Error('mesh baseline');
  if (value.legacy.stats.meshNodeCount !== 2) throw new Error('mesh node baseline');
  if (value.legacy.stats.triangleCount !== 188) throw new Error('triangle baseline');
  if (value.legacy.selectionIds.length !== 6) throw new Error('selection baseline');
  if (!(value.legacy.surfaceArea > 0)) throw new Error('surface area baseline');
}

function assertInstancedBaseline(value) {
  if (value.kind !== 'ice3-step-import-instance-baseline') {
    throw new Error('instance baseline kind');
  }
  if (value.legacy.stats.nodeCount !== 8) throw new Error('instance node baseline');
  if (value.legacy.stats.meshNodeCount !== 3) throw new Error('instance mesh node baseline');
  if (value.legacy.stats.instanceNodeCount !== 0) {
    throw new Error('legacy instance metadata baseline');
  }
}

function assertBoundsComparatorRejectsTranslation() {
  const baseline = normalizeBounds({
    min: [0, 0, -1],
    max: [2, 1, 1],
  });
  const translated = {
    sourceOrigin: [1, 0, -1],
    min: [1, 0, 0],
    max: [3, 1, 2],
  };
  if (normalizedBoundsClose(baseline, translated)) {
    throw new Error('bounds comparator accepted a translated model');
  }
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

function runNativeImport({
  fixture,
  output,
  adapter,
  assetId: targetAssetId,
  force = false,
  environment = process.env,
}) {
  const args = [
    'import',
    fixture,
    '--output',
    output,
    '--asset-id',
    targetAssetId,
    '--adapter',
    adapter,
  ];
  if (force) args.push('--force');
  const result = spawnSync(nativeCli, args, {
    cwd: studioRoot,
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
    env: environment,
  });
  if (result.status !== 0) {
    throw new Error(`native STEP import failed:\n${result.stderr}\n${result.stdout}`);
  }
  return JSON.parse(result.stdout);
}

async function compareInstanceCorpus(temporaryRoot) {
  const root = join(temporaryRoot, 'instance-corpus');
  await mkdir(root, { recursive: true });
  const instancedFixturePath = join(root, 'instanced.step');
  const outputPath = join(root, 'instanced.glb');
  await writeInstancedFixture(instancedFixturePath);
  const baseline = JSON.parse(await readFile(instancedBaselinePath, 'utf8'));
  const command = runNativeImport({
    fixture: instancedFixturePath,
    output: outputPath,
    adapter: 'step',
    assetId: instancedAssetId,
  });
  const [sourceBytes, nativeBytes] = await Promise.all([
    readFile(instancedFixturePath),
    readFile(outputPath),
  ]);
  const native = summarizeGlb(parseGlb(nativeBytes));
  const legacy = baseline.legacy;
  const normalizedLegacyBounds = normalizeBounds(legacy.bounds);
  const normalizedNativeBounds = {
    sourceOrigin: native.sourceOrigin,
    min: native.bounds.min,
    max: native.bounds.max,
  };
  const triangleRatio = native.triangleCount / legacy.stats.triangleCount;
  const comparisons = {
    sourceDigest: {
      baseline: baseline.sourceDigest,
      native: sha256(sourceBytes),
      equal: baseline.sourceDigest === sha256(sourceBytes),
    },
    nodeCount: {
      baseline: legacy.stats.nodeCount,
      native: native.nodeCount,
      equal: legacy.stats.nodeCount === native.nodeCount,
    },
    meshNodeCount: {
      baseline: legacy.stats.meshNodeCount,
      native: native.meshNodeCount,
      equal: legacy.stats.meshNodeCount === native.meshNodeCount,
    },
    businessNames: {
      baseline: normalizeRepeatedNames(legacy.nodeNames),
      native: normalizeRepeatedNames(native.nodeNames),
      equal: JSON.stringify(normalizeRepeatedNames(legacy.nodeNames))
        === JSON.stringify(normalizeRepeatedNames(native.nodeNames)),
    },
    selectionIds: {
      baseline: legacy.selectionIds,
      native: native.selectionIds,
      equal: selectionIdsEquivalent(
        legacy.selectionIds,
        native.selectionIds,
        instancedAssetId,
      ),
    },
    nodeSelectionPairs: {
      baseline: normalizedNodeSelectionPairs(
        legacySelectionPairs(legacy),
        instancedAssetId,
      ),
      native: normalizedNodeSelectionPairs(
        native.nodeSelections,
        instancedAssetId,
      ),
      equal: nodeSelectionPairsEquivalent(
        legacySelectionPairs(legacy),
        native.nodeSelections,
        instancedAssetId,
      ),
    },
    sharedMeshInstances: {
      baseline: legacy.stats.sharedMeshNodeCount,
      native: native.sharedMeshNodeCount,
      equal: native.sharedMeshNodeCount >= 2,
    },
    instanceGroups: {
      baseline: legacy.stats.instanceNodeCount,
      native: native.instanceNodeCount,
      equal: native.instanceNodeCount >= 2,
    },
    materials: {
      baseline: legacy.materials,
      native: native.materials,
      equal: materialsClose(legacy.materials, native.materials),
    },
    normalizedBounds: {
      baseline: normalizedLegacyBounds,
      native: normalizedNativeBounds,
      equal: normalizedBoundsClose(
        normalizedLegacyBounds,
        normalizedNativeBounds,
      ),
    },
    triangleCount: {
      baseline: legacy.stats.triangleCount,
      native: native.triangleCount,
      ratio: triangleRatio,
      equal: triangleRatio >= 0.75 && triangleRatio <= 1.33,
    },
    warningCodes: {
      baseline: legacy.warningCodes,
      native: warningCodes(command.warnings),
      equal: JSON.stringify(legacy.warningCodes)
        === JSON.stringify(warningCodes(command.warnings)),
    },
  };
  const failures = Object.entries(comparisons)
    .filter(([, value]) => value.equal !== true)
    .map(([key]) => key);
  return {
    status: failures.length === 0 ? 'pass' : 'fail',
    fixture: 'generated:synthetic-skid+repeated-tank-occurrence',
    sourceDigest: comparisons.sourceDigest.native,
    byteDigest: sha256(nativeBytes),
    comparisons,
    failures,
  };
}

function normalizeRepeatedNames(names) {
  return names
    .map((name) => name.replace(/\[\d+\]$/u, ''))
    .sort((left, right) => left.localeCompare(right, 'en'));
}

async function writeInstancedFixture(path) {
  const source = await readFile(fixturePath, 'utf8');
  const marker = 'ENDSEC;\nEND-ISO-10303-21;';
  if (!source.includes(marker)) throw new Error('STEP fixture terminator is missing');
  await writeFile(
    path,
    source.replace(marker, `${repeatedOccurrence}${marker}`),
  );
}

async function runNegativeCorpus(temporaryRoot) {
  const root = join(temporaryRoot, 'negative');
  await mkdir(root, { recursive: true });
  const nonAp242Path = join(root, 'non-ap242.step');
  const nonAp242 = (await readFile(fixturePath, 'utf8')).replace(
    'AP242_MANAGED_MODEL_BASED_3D_ENGINEERING_MIM_LF',
    'AUTOMOTIVE_DESIGN',
  );
  await writeFile(nonAp242Path, nonAp242);

  return {
    empty: await expectFailure(root, 'empty', {
      fixture: join(fixtureRoot, 'empty-assembly.step'),
      adapter: 'step',
      expectedCode: 'ICE3_STUDIO_IMPORT_EMPTY_GEOMETRY',
    }),
    truncated: await expectFailure(root, 'truncated', {
      fixture: join(fixtureRoot, 'truncated.step'),
      adapter: 'step',
      expectedCode: 'ICE3_STUDIO_IMPORT_INVALID_SOURCE',
    }),
    nonAp242: await expectFailure(root, 'non-ap242', {
      fixture: nonAp242Path,
      adapter: 'step',
      expectedCode: 'ICE3_STUDIO_IMPORT_STEP_SCHEMA_UNSUPPORTED',
    }),
    limitExceeded: await expectFailure(root, 'limit', {
      fixture: fixturePath,
      adapter: 'step',
      expectedCode: 'ICE3_STUDIO_IMPORT_LIMIT_EXCEEDED',
      environment: {
        ...process.env,
        ICE3_STUDIO_IMPORT_MAX_TRIANGLES: '1',
      },
    }),
    timeout: await expectFailure(root, 'timeout', {
      fixture: fixturePath,
      adapter: 'step',
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
    '--output',
    output,
    '--asset-id',
    `negative-${name}`,
    '--adapter',
    options.adapter,
  ];
  if (options.force) args.push('--force');
  const processResult = spawnSync(nativeCli, args, {
    cwd: studioRoot,
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
    env: options.environment ?? process.env,
  });
  const outputDisposition = options.preserveOutput !== undefined
    ? (await readFile(output, 'utf8')) === options.preserveOutput
      ? 'preserved'
      : 'changed'
    : await access(output).then(() => 'present', () => 'absent');
  const status = (
    processResult.status !== 0
    && processResult.stderr.includes(options.expectedCode)
    && ['absent', 'preserved'].includes(outputDisposition)
  ) ? 'pass' : 'fail';
  return {
    status,
    exitCode: processResult.status,
    expectedCode: options.expectedCode,
    diagnosticPresent: processResult.stderr.includes(options.expectedCode),
    outputDisposition,
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
  if (!Array.isArray(left.sourceOrigin) || !Array.isArray(right.sourceOrigin)) {
    return false;
  }
  return [0, 1, 2].every((axis) => {
    if (
      !Number.isFinite(left.sourceOrigin[axis])
      || !Number.isFinite(right.sourceOrigin[axis])
    ) {
      return false;
    }
    const extent = Math.abs(left.max[axis] - left.min[axis]);
    const tolerance = Math.max(0.0001, extent * 0.00001);
    return (
      Math.abs(left.sourceOrigin[axis] - right.sourceOrigin[axis]) <= tolerance
      && Math.abs(left.min[axis] - right.min[axis]) <= tolerance
      && Math.abs(left.max[axis] - right.max[axis]) <= tolerance
    );
  });
}

function selectionIdsEquivalent(baseline, native, sourceAssetId) {
  if (baseline.length !== native.length) return false;
  const baselineOrdinals = baseline.map((id) => {
    const match = id.match(new RegExp(
      `^${escapeRegex(sourceAssetId)}/(?:step/)?node/(\\d+)$`,
    ));
    return match?.[1];
  });
  const nativeOrdinals = native.map((id) => {
    const match = id.match(new RegExp(
      `^${escapeRegex(sourceAssetId)}/step/node/(\\d+)$`,
    ));
    return match?.[1];
  });
  return baselineOrdinals.every((value, index) => (
    value !== undefined && value === nativeOrdinals[index]
  ));
}

function legacySelectionPairs(legacy) {
  return legacy.nodeNames.map((name, index) => ({
    name,
    selectionId: legacy.selectionIds[index],
  }));
}

function normalizedNodeSelectionPairs(pairs, sourceAssetId) {
  return pairs
    .map((pair) => ({
      name: pair.name.replace(/\[\d+\]$/u, ''),
      ordinal: selectionOrdinal(pair.selectionId, sourceAssetId),
    }))
    .sort((left, right) => (
      left.name.localeCompare(right.name, 'en')
      || left.ordinal - right.ordinal
    ));
}

function nodeSelectionPairsEquivalent(baseline, native, sourceAssetId) {
  if (baseline.length !== native.length) return false;
  const left = normalizedNodeSelectionPairs(baseline, sourceAssetId);
  const right = normalizedNodeSelectionPairs(native, sourceAssetId);
  return left.every((pair, index) => (
    Number.isInteger(pair.ordinal)
    && pair.name === right[index].name
    && pair.ordinal === right[index].ordinal
  ));
}

function selectionOrdinal(id, sourceAssetId) {
  if (typeof id !== 'string') return Number.NaN;
  const match = id.match(new RegExp(
    `^${escapeRegex(sourceAssetId)}/(?:step/)?node/(\\d+)$`,
  ));
  return match ? Number(match[1]) : Number.NaN;
}

function assertNodeSelectionPairComparatorRejectsSwap() {
  const baseline = [
    { name: 'PrimaryTank', selectionId: 'fixture/step/node/2' },
    { name: 'TransferPump', selectionId: 'fixture/step/node/3' },
  ];
  const swapped = [
    { name: 'PrimaryTank', selectionId: 'fixture/step/node/3' },
    { name: 'TransferPump', selectionId: 'fixture/step/node/2' },
  ];
  if (nodeSelectionPairsEquivalent(baseline, swapped, 'fixture')) {
    throw new Error('node/Selection pair comparator accepted a swap');
  }
}

function escapeRegex(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/gu, '\\$&');
}

function materialsClose(left, right) {
  return left.length === right.length
    && left.every((material, index) => (
      material.name === right[index].name
      && material.baseColorFactor.every((value, component) => (
        Math.abs(value - right[index].baseColorFactor[component]) <= 1e-5
      ))
    ));
}

function relativeError(value, reference) {
  if (reference === 0) return value === 0 ? 0 : Infinity;
  return Math.abs(value - reference) / Math.abs(reference);
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
  const nodeSelections = nodes
    .map((node) => ({
      name: node.name ?? '',
      selectionId: node.extras?.ice3?.id,
    }))
    .filter((pair) => pair.selectionId !== undefined);
  const meshNodes = nodes.filter((node) => Number.isInteger(node.mesh));
  const meshReferenceCounts = new Map();
  for (const node of meshNodes) {
    meshReferenceCounts.set(
      node.mesh,
      (meshReferenceCounts.get(node.mesh) ?? 0) + 1,
    );
  }
  const instanceNodeCount = nodes.filter(
    (node) => node.extras?.ice3?.step?.instanceGroup !== undefined,
  ).length;
  const bounds = glb.json.asset?.extras?.ice3?.bounds;
  if (!bounds) throw new Error('Prepared GLB bounds are missing');
  const sourceOrigin = (
    glb.json.asset?.extras?.ice3?.step?.sourceOrigin
    ?? glb.json.asset?.extras?.ice3?.ice3?.step?.sourceOrigin
  );
  return {
    nodeCount: nodes.length,
    meshCount: meshes.length,
    triangleCount,
    vertexCount: [...positionAccessors]
      .reduce((total, index) => total + glb.json.accessors[index].count, 0),
    meshNodeCount: meshNodes.length,
    sharedMeshNodeCount: meshNodes.filter(
      (node) => meshReferenceCounts.get(node.mesh) > 1,
    ).length,
    instanceNodeCount,
    edgeSegmentCount,
    surfaceArea,
    nodeNames,
    selectionIds,
    nodeSelections,
    bounds,
    sourceOrigin: Array.isArray(sourceOrigin) && sourceOrigin.length === 3
      ? sourceOrigin
      : null,
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
