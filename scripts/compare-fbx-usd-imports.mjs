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
const fixtureRoot = resolve(
  studioRoot,
  'fixtures/source-import',
);
const nativeCli = join(
  studioRoot,
  'target/debug',
  process.platform === 'win32' ? 'studio-cli.exe' : 'studio-cli',
);
const reportPath = join(studioRoot, 'output/fbx-usd-import-parity.json');
const temporaryRoot = await mkdtemp(join(tmpdir(), 'ice3-fbx-usd-parity-'));
const allowedDifferenceKeys = new Set(['vertexCount', 'byteDigest']);

try {
  ensureNativeCli();
  const adapters = {};
  const failures = [];
  for (const adapter of ['fbx', 'usd']) {
    const baselinePath = join(
      studioRoot,
      `fixtures/parity/${adapter}-baseline-v1.json`,
    );
    const baseline = JSON.parse(await readFile(baselinePath, 'utf8'));
    const variantResults = {};
    for (const variant of ['ascii', 'binary']) {
      const result = await compareVariant(baseline, adapter, variant);
      variantResults[variant] = result;
      failures.push(...result.failures.map((failure) => `${adapter}.${variant}.${failure}`));
    }
    const semanticDigestEqual = (
      variantResults.ascii.semanticDigest
      === variantResults.binary.semanticDigest
    );
    if (!semanticDigestEqual) failures.push(`${adapter}.asciiBinarySemanticDigest`);
    adapters[adapter] = {
      baseline: relative(studioRoot, baselinePath),
      status: (
        variantResults.ascii.failures.length === 0
        && variantResults.binary.failures.length === 0
        && semanticDigestEqual
      ) ? 'pass' : 'fail',
      asciiBinarySemanticDigest: {
        ascii: variantResults.ascii.semanticDigest,
        binary: variantResults.binary.semanticDigest,
        equal: semanticDigestEqual,
      },
      variants: variantResults,
    };
  }

  const negativeCorpus = await runNegativeCorpus();
  failures.push(...Object.entries(negativeCorpus)
    .filter(([, result]) => result.status !== 'pass')
    .map(([name]) => `negativeCorpus.${name}`));
  const report = {
    kind: 'ice3-fbx-usd-import-parity',
    schemaVersion: '1.0',
    status: failures.length === 0 ? 'pass' : 'fail',
    failures: [...new Set(failures)].sort(),
    adapters,
    negativeCorpus,
    allowedDifferences: [
      {
        key: 'vertexCount',
        reason: 'Vertex splitting and welding differ while node, mesh, triangle, instance, Selection, bounds, material and warning semantics remain equal.',
      },
      {
        key: 'byteDigest',
        reason: 'Preparer versions, accessor layout, generated edges and traversal order differ while semantic comparisons remain equal.',
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

async function compareVariant(baseline, adapter, variant) {
  const expectedVariant = baseline.variants[variant];
  const source = join(fixtureRoot, adapter, expectedVariant.fixture);
  const output = join(
    temporaryRoot,
    `${adapter}-${variant}-${baseline.assetId}.glb`,
  );
  const command = runNative([
    'import',
    source,
    '--output',
    output,
    '--asset-id',
    baseline.assetId,
    '--adapter',
    adapter,
  ]);
  const bytes = await readFile(output);
  const parsed = parseGlb(bytes);
  const native = summarize(parsed, command);
  const nativeSemantic = semanticView(native);
  const nativeSemanticDigest = sha256(JSON.stringify(nativeSemantic));
  const expectedCanonicalDigest = sha256(
    JSON.stringify(semanticView(baseline.canonical)),
  );
  const comparisons = {
    baselineDigest: {
      recorded: baseline.canonical.semanticDigest,
      computed: expectedCanonicalDigest,
      equal: baseline.canonical.semanticDigest === expectedCanonicalDigest,
    },
    sourceDigest: {
      baseline: expectedVariant.sourceDigest,
      native: command.sourceDigest,
      equal: expectedVariant.sourceDigest === command.sourceDigest,
    },
    stats: {
      baseline: baseline.canonical.stats,
      native: nativeSemantic.stats,
      equal: JSON.stringify(baseline.canonical.stats)
        === JSON.stringify(nativeSemantic.stats),
    },
    nodeNames: {
      baseline: baseline.canonical.nodeNames,
      native: nativeSemantic.nodeNames,
      equal: JSON.stringify(baseline.canonical.nodeNames)
        === JSON.stringify(nativeSemantic.nodeNames),
    },
    selectionIds: {
      baseline: baseline.canonical.selectionIds,
      native: nativeSemantic.selectionIds,
      equal: JSON.stringify(baseline.canonical.selectionIds)
        === JSON.stringify(nativeSemantic.selectionIds),
    },
    bounds: {
      baseline: baseline.canonical.bounds,
      native: nativeSemantic.bounds,
      equal: boundsClose(baseline.canonical.bounds, nativeSemantic.bounds),
    },
    materials: {
      baseline: baseline.canonical.materials,
      native: nativeSemantic.materials,
      equal: materialsClose(
        baseline.canonical.materials,
        nativeSemantic.materials,
      ),
    },
    warningCodes: {
      baseline: baseline.canonical.warningCodes,
      native: nativeSemantic.warningCodes,
      equal: JSON.stringify(baseline.canonical.warningCodes)
        === JSON.stringify(nativeSemantic.warningCodes),
    },
    semanticDigest: {
      baseline: baseline.canonical.semanticDigest,
      native: nativeSemanticDigest,
      equal: baseline.canonical.semanticDigest === nativeSemanticDigest,
    },
    vertexCount: {
      baseline: expectedVariant.stats.vertexCount,
      native: native.vertexCount,
      equal: expectedVariant.stats.vertexCount === native.vertexCount,
      explanation: 'Allowed topology representation difference.',
    },
    byteDigest: {
      baseline: expectedVariant.glbDigest,
      native: native.byteDigest,
      equal: expectedVariant.glbDigest === native.byteDigest,
      explanation: 'Allowed deterministic output-layout difference.',
    },
  };
  const unexpectedFailures = Object.entries(comparisons)
    .filter(([key, comparison]) => (
      !allowedDifferenceKeys.has(key) && comparison.equal !== true
    ))
    .map(([key]) => key);
  return {
    fixture: relative(studioRoot, source),
    sourceDigest: command.sourceDigest,
    byteDigest: native.byteDigest,
    vertexCount: native.vertexCount,
    semanticDigest: nativeSemanticDigest,
    rawLegacy: {
      stats: expectedVariant.stats,
      selectionIds: expectedVariant.selectionIds,
      nodeNames: expectedVariant.nodeNames,
      bounds: expectedVariant.bounds,
      materials: expectedVariant.materials,
    },
    comparisons,
    failures: unexpectedFailures,
  };
}

async function runNegativeCorpus() {
  const root = join(temporaryRoot, 'negative');
  const usdRoot = join(root, 'usd');
  const deepRoot = join(usdRoot, 'deep');
  await mkdir(deepRoot, { recursive: true });
  const escapedDependencyPath = join(root, 'outside.usda');
  await writeFile(
    escapedDependencyPath,
    `#usda 1.0\ndef Xform "Outside"\n{\n${cubeBody()}\n}\n`,
  );
  await writeFile(
    join(usdRoot, 'unsafe.usda'),
    '#usda 1.0\ndef Xform "Plant" (references = @../outside.usda@</Outside>) {}\n',
  );
  for (let index = 0; index < 34; index += 1) {
    const body = index === 33
      ? cubeBody()
      : `def Xform "Level" (references = @layer-${index + 1}.usda@</Level>) {}`;
    await writeFile(
      join(deepRoot, `layer-${index}.usda`),
      `#usda 1.0\n${body}\n`,
    );
  }
  const corruptPath = join(root, 'corrupt-binary.fbx');
  const corrupt = Buffer.from(await readFile(
    join(fixtureRoot, 'fbx/synthetic-binary.fbx'),
  ));
  corrupt.write('NOT-FBX!', 0, 'ascii');
  await writeFile(corruptPath, corrupt);

  return {
    empty: await expectFailure('empty', {
      fixture: join(fixtureRoot, 'fbx/empty.fbx'),
      adapter: 'fbx',
      expectedCode: 'ICE3_STUDIO_IMPORT_EMPTY_GEOMETRY',
    }),
    truncated: await expectFailure('truncated', {
      fixture: join(fixtureRoot, 'fbx/truncated.fbx'),
      adapter: 'fbx',
      expectedCode: 'ICE3_STUDIO_IMPORT_INVALID_SOURCE',
    }),
    corruptBinary: await expectFailure('corrupt-binary', {
      fixture: corruptPath,
      adapter: 'fbx',
      expectedCode: 'ICE3_STUDIO_IMPORT_INVALID_SOURCE',
    }),
    unsafeUsdReference: await expectFailure('unsafe-usd-reference', {
      fixture: join(usdRoot, 'unsafe.usda'),
      adapter: 'usd',
      expectedCode: 'ICE3_STUDIO_IMPORT_INVALID_SOURCE',
      requiredExistingPath: escapedDependencyPath,
    }),
    deepComposition: await expectFailure('deep-composition', {
      fixture: join(deepRoot, 'layer-0.usda'),
      adapter: 'usd',
      expectedCode: 'ICE3_STUDIO_IMPORT_INVALID_SOURCE',
    }),
    limitExceeded: await expectFailure('limit-exceeded', {
      fixture: join(fixtureRoot, 'usd/synthetic-plant.usda'),
      adapter: 'usd',
      expectedCode: 'ICE3_STUDIO_IMPORT_LIMIT_EXCEEDED',
      environment: {
        ...process.env,
        ICE3_STUDIO_IMPORT_MAX_TRIANGLES: '1',
      },
    }),
    workerTimeout: await expectFailure('worker-timeout', {
      fixture: join(fixtureRoot, 'usd/synthetic-plant.usda'),
      adapter: 'usd',
      expectedCode: 'ICE3_STUDIO_IMPORT_TIMEOUT',
      preserveOutput: 'keep',
      environment: {
        ...process.env,
        ICE3_STUDIO_IMPORT_WORKER_TIMEOUT_MS: '1',
      },
    }),
    workerCrash: await expectFailure('worker-crash', {
      fixture: join(fixtureRoot, 'usd/synthetic-plant.usda'),
      adapter: 'usd',
      expectedCode: 'ICE3_STUDIO_IMPORT_WORKER_FAILED',
      preserveOutput: 'keep',
      environment: {
        ...process.env,
        ICE3_STUDIO_IMPORT_WORKER_CRASH: '1',
      },
    }),
  };
}

async function expectFailure(name, options) {
  const output = join(temporaryRoot, 'negative', `${name}.glb`);
  const preserveOutput = options.preserveOutput;
  if (preserveOutput !== undefined) {
    await writeFile(output, preserveOutput);
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
  if (preserveOutput !== undefined) args.push('--force');
  const result = spawnSync(nativeCli, args, {
    cwd: studioRoot,
    encoding: 'utf8',
    env: options.environment ?? process.env,
    maxBuffer: 16 * 1024 * 1024,
    timeout: 20_000,
  });
  const diagnosticPresent = `${result.stdout ?? ''}${result.stderr ?? ''}`
    .includes(options.expectedCode);
  const requiredExistingPathExists = options.requiredExistingPath === undefined
    || await exists(options.requiredExistingPath);
  const outputDisposition = preserveOutput === undefined
    ? (!(await exists(output)) ? 'absent' : 'published')
    : (await exists(output)
      ? (await readFile(output, 'utf8') === preserveOutput
        ? 'preserved'
        : 'replaced')
      : 'removed');
  const pass = (
    result.status !== 0
    && diagnosticPresent
    && requiredExistingPathExists
    && ['absent', 'preserved'].includes(outputDisposition)
  );
  return {
    status: pass ? 'pass' : 'fail',
    exitCode: result.status,
    expectedCode: options.expectedCode,
    diagnosticPresent,
    requiredExistingPathExists,
    outputDisposition,
  };
}

function runNative(args) {
  const result = spawnSync(nativeCli, args, {
    cwd: studioRoot,
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
    timeout: 30_000,
  });
  if (result.status !== 0) {
    throw new Error(
      `native import failed:\n${result.stderr}\n${result.stdout}`,
    );
  }
  return JSON.parse(result.stdout);
}

function summarize(glb, command) {
  let triangleCount = 0;
  let vertexCount = 0;
  const materialIndices = new Set();
  for (const mesh of glb.json.meshes ?? []) {
    for (const primitive of mesh.primitives ?? []) {
      if ((primitive.mode ?? 4) !== 4) continue;
      const position = glb.json.accessors?.[primitive.attributes?.POSITION];
      const indices = glb.json.accessors?.[primitive.indices];
      vertexCount += position?.count ?? 0;
      triangleCount += (indices?.count ?? position?.count ?? 0) / 3;
      if (primitive.material !== undefined) {
        materialIndices.add(primitive.material);
      }
    }
  }
  const meshUse = new Map();
  for (const node of glb.json.nodes ?? []) {
    if (node.mesh === undefined) continue;
    meshUse.set(node.mesh, (meshUse.get(node.mesh) ?? 0) + 1);
  }
  const instanceNodeCount = [...meshUse.values()]
    .filter((count) => count > 1)
    .reduce((total, count) => total + count, 0);
  const materials = [...materialIndices]
    .map((index) => glb.json.materials?.[index])
    .filter(Boolean)
    .map((material) => ({
      name: normalizeMaterialName(material.name),
      baseColorFactor: material.pbrMetallicRoughness?.baseColorFactor
        ?? [1, 1, 1, 1],
    }))
    .sort(compareNames);
  const prepared = glb.json.asset?.extras?.ice3 ?? {};
  const bounds = prepared.bounds ?? glb.json.asset?.extras?.bounds;
  return {
    sourceDigest: command.sourceDigest,
    byteDigest: sha256(glb.bytes),
    nodeCount: glb.json.nodes?.length ?? 0,
    meshCount: glb.json.meshes?.length ?? 0,
    triangleCount,
    vertexCount,
    semanticMaterialCount: materials.length,
    instanceNodeCount,
    nodeNames: (glb.json.nodes ?? [])
      .map((node) => node.name ?? '')
      .filter(Boolean)
      .sort(),
    selectionIds: (glb.json.nodes ?? [])
      .map((node) => node.extras?.ice3?.selectionId)
      .filter(Boolean)
      .sort(),
    bounds,
    materials,
    warningCodes: (command.warnings ?? [])
      .map((warning) => (
        typeof warning === 'string' ? warning : warning.code
      ))
      .filter(Boolean)
      .sort(),
  };
}

function semanticView(summary) {
  return {
    stats: {
      nodeCount: summary.stats?.nodeCount ?? summary.nodeCount,
      meshCount: summary.stats?.meshCount ?? summary.meshCount,
      triangleCount: summary.stats?.triangleCount ?? summary.triangleCount,
      semanticMaterialCount: (
        summary.stats?.semanticMaterialCount
        ?? summary.semanticMaterialCount
      ),
      instanceNodeCount: (
        summary.stats?.instanceNodeCount
        ?? summary.instanceNodeCount
      ),
    },
    nodeNames: [...summary.nodeNames].sort(),
    selectionIds: [...summary.selectionIds].sort(),
    bounds: {
      min: roundArray(summary.bounds.min),
      max: roundArray(summary.bounds.max),
    },
    materials: summary.materials.map((material) => ({
      name: normalizeMaterialName(material.name),
      baseColorFactor: roundArray(material.baseColorFactor),
    })).sort(compareNames),
    warningCodes: [...summary.warningCodes].sort(),
  };
}

function parseGlb(bytes) {
  if (bytes.length < 20 || bytes.readUInt32LE(0) !== 0x46546c67) {
    throw new Error('native output is not GLB');
  }
  let offset = 12;
  let json;
  while (offset + 8 <= bytes.length) {
    const length = bytes.readUInt32LE(offset);
    const kind = bytes.readUInt32LE(offset + 4);
    const chunk = bytes.subarray(offset + 8, offset + 8 + length);
    if (kind === 0x4e4f534a) {
      json = JSON.parse(chunk.toString('utf8').trim());
    }
    offset += 8 + length;
  }
  if (!json) throw new Error('native GLB JSON chunk is missing');
  return { bytes, json };
}

function normalizeMaterialName(value) {
  return String(value ?? '')
    .replace(/::Material$/u, '')
    .replace(/ edges$/u, '');
}

function compareNames(left, right) {
  return left.name.localeCompare(right.name, 'en');
}

function boundsClose(left, right) {
  return arraysClose(left.min, right.min) && arraysClose(left.max, right.max);
}

function materialsClose(left, right) {
  return left.length === right.length
    && left.every((material, index) => (
      normalizeMaterialName(material.name)
        === normalizeMaterialName(right[index].name)
      && arraysClose(material.baseColorFactor, right[index].baseColorFactor)
    ));
}

function arraysClose(left, right) {
  return Array.isArray(left)
    && Array.isArray(right)
    && left.length === right.length
    && left.every((value, index) => (
      Math.abs(value - right[index]) <= 1e-4
    ));
}

function roundArray(values) {
  return values.map((value) => {
    const rounded = Math.round(value * 1_000_000) / 1_000_000;
    return Object.is(rounded, -0) ? 0 : rounded;
  });
}

function sha256(value) {
  return createHash('sha256').update(value).digest('hex');
}

async function exists(path) {
  try {
    await access(path);
    return true;
  } catch {
    return false;
  }
}

function cubeBody() {
  return [
    'def Xform "Level"',
    '{',
    '    def Mesh "Cube"',
    '    {',
    '        int[] faceVertexCounts = [3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3]',
    '        int[] faceVertexIndices = [1, 2, 0, 3, 6, 2, 7, 4, 6, 5, 0, 4, 6, 0, 2, 3, 5, 7, 1, 3, 2, 3, 7, 6, 7, 5, 4, 5, 1, 0, 6, 4, 0, 3, 1, 5]',
    '        point3f[] points = [(-1, -1, -1), (-1, -1, 1), (-1, 1, -1), (-1, 1, 1), (1, -1, -1), (1, -1, 1), (1, 1, -1), (1, 1, 1)]',
    '        color3f[] primvars:displayColor = [(0.12, 0.38, 0.68)] (',
    '            interpolation = "constant"',
    '        )',
    '    }',
    '}',
  ].join('\n');
}
