import { spawn, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  copyFile,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  rm,
  writeFile,
} from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { performance } from 'node:perf_hooks';
import { createInterface } from 'node:readline';
import { fileURLToPath, pathToFileURL } from 'node:url';

import { buildIndustrialScenePlan } from '../fixtures/large-industrial-site/operations.mjs';

const studioRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const baselinePath = resolve(
  studioRoot,
  'fixtures/parity/prepare-performance-v1.json',
);
const reportPath = resolve(studioRoot, 'output/prepare-performance.json');
const sampleCount = 5;
const memoryLimitBytes = 536_870_912;
const legacyLimitMultiple = 2;

const cases = [
  {
    name: 'build-edges',
    command: 'build',
    outputKind: 'file',
    args: ['--asset-id', 'large-industrial-prepare'],
  },
  {
    name: 'build-skip-edges',
    command: 'build',
    outputKind: 'file',
    args: ['--asset-id', 'large-industrial-prepare', '--skip-edges'],
  },
  {
    name: 'partition-direct',
    command: 'partition',
    outputKind: 'directory',
    args: [
      '--asset-id', 'large-industrial-prepare',
      '--max-triangles', '50000',
      '--max-bytes', '67108864',
    ],
  },
  {
    name: 'partition-adaptive-proxy',
    command: 'partition',
    outputKind: 'directory',
    args: [
      '--asset-id', 'large-industrial-prepare',
      '--max-triangles', '50000',
      '--max-bytes', '67108864',
      '--lod',
      '--lod-allocation', 'adaptive',
      '--lod-ratio', '0.15',
      '--lod-max-triangles', '250000',
      '--lod-min-unit-triangles', '8',
      '--lod-curve-samples', '8',
      '--lod-max-memory-bytes', '67108864',
      '--semantic-proxy', 'auto',
    ],
  },
  {
    name: 'partition-static-batch',
    command: 'partition',
    outputKind: 'directory',
    args: [
      '--asset-id', 'large-industrial-prepare',
      '--max-triangles', '50000',
      '--max-bytes', '67108864',
      '--render-batch', 'static',
    ],
  },
];

if (
  process.argv[1]
  && resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  const capture = process.argv.includes('--capture');
  const report = await benchmark({ capture }).catch((error) => ({
    kind: 'ice3-prepare-performance-report',
    schemaVersion: '1.0',
    status: 'fail',
    failures: ['execution'],
    error: error instanceof Error ? error.message : String(error),
    cases: [],
  }));
  await mkdir(dirname(reportPath), { recursive: true });
  await writeFile(reportPath, `${JSON.stringify(report, null, 2)}\n`);
  console.log(JSON.stringify(report, null, 2));
  if (report.status !== 'pass') process.exitCode = 1;
}

async function benchmark({ capture }) {
  const legacyToolchain = process.env.LEGACY_TOOLCHAIN_BIN
    ? resolve(process.env.LEGACY_TOOLCHAIN_BIN)
    : undefined;
  if (capture && !legacyToolchain) {
    throw new Error(
      'LEGACY_TOOLCHAIN_BIN is required for --capture; '
      + 'the archived artifact is used only to pin the one-time envelope',
    );
  }
  const studioBinary = await resolveStudioBinary();
  const temporary = await mkdtemp(join(tmpdir(), 'ice3-prepare-performance-'));
  try {
    const source = await createLargeSource(studioBinary, temporary);
    const measuredCases = [];
    const failures = [];
    for (const definition of cases) {
      const studio = await sampleCase(
        studioBinary,
        source.path,
        definition,
        join(temporary, 'studio', definition.name),
      );
      const legacyP95Ms = legacyToolchain
        ? (await sampleCase(
            legacyToolchain,
            source.path,
            definition,
            join(temporary, 'legacy', definition.name),
            true,
          )).p95Ms
        : undefined;
      const entry = {
        name: definition.name,
        command: definition.command,
        options: definition.args,
        p50Ms: studio.p50Ms,
        p95Ms: studio.p95Ms,
        peakMemoryBytes: studio.peakMemoryBytes,
        outputBytes: studio.outputBytes,
        triangleCount: studio.triangleCount,
        edgeSegmentCount: studio.edgeSegmentCount,
        partitionCount: studio.partitionCount,
        renderBatchCount: studio.renderBatchCount,
        coarseCount: studio.coarseCount,
        digestStable: studio.digestStable,
        digest: studio.digest,
        ...(legacyP95Ms === undefined
          ? {}
          : {
              legacyP95Ms,
              legacySource: 'archived local artifact (one-time capture)',
            }),
      };
      if (entry.peakMemoryBytes <= 0 || entry.peakMemoryBytes > memoryLimitBytes) {
        failures.push(`${entry.name}:peakMemoryBytes`);
      }
      if (!entry.digestStable) failures.push(`${entry.name}:digestStable`);
      if (
        entry.name !== 'build-skip-edges'
        && entry.edgeSegmentCount <= 0
      ) {
        failures.push(`${entry.name}:edgeSegmentCount`);
      }
      if (
        entry.name === 'build-skip-edges'
        && entry.edgeSegmentCount !== 0
      ) {
        failures.push(`${entry.name}:edgeSegmentCount`);
      }
      measuredCases.push(entry);
    }

    let baselineComparison;
    if (capture) {
      const baseline = {
        kind: 'ice3-prepare-performance-baseline',
        schemaVersion: '1.0',
        environment: {
          platform: process.platform,
          architecture: process.arch,
          node: process.version,
          studioBinary: 'release',
          legacyArtifactVersion: '0.2.0',
        },
        sampling: {
          warmupRuns: 1,
          measuredRuns: sampleCount,
          memoryLimitBytes,
          legacyLimitMultiple,
        },
        fixture: source.identity,
        cases: measuredCases,
      };
      await mkdir(dirname(baselinePath), { recursive: true });
      await writeFile(baselinePath, `${JSON.stringify(baseline, null, 2)}\n`);
      baselineComparison = { captured: true, baselinePath };
    } else {
      const baseline = JSON.parse(await readFile(baselinePath, 'utf8'));
      baselineComparison = compareWithBaseline(baseline, measuredCases);
      failures.push(...baselineComparison.failures);
    }
    return {
      kind: 'ice3-prepare-performance-report',
      schemaVersion: '1.0',
      status: failures.length === 0 ? 'pass' : 'fail',
      failures,
      fixture: source.identity,
      sampling: {
        warmupRuns: 1,
        measuredRuns: sampleCount,
        memoryLimitBytes,
        legacyLimitMultiple,
      },
      baselineComparison,
      cases: measuredCases,
    };
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
}

async function sampleCase(
  command,
  sourcePath,
  definition,
  outputRoot,
  legacy = false,
) {
  await mkdir(outputRoot, { recursive: true });
  await runMeasured(
    commandSpec(command, legacy),
    caseArgs(sourcePath, definition, join(outputRoot, 'warm')),
    outputRoot,
  );
  const samples = [];
  const digests = new Set();
  let evidence;
  for (let index = 0; index < sampleCount; index += 1) {
    const output = join(outputRoot, `sample-${index}`);
    const measured = await runMeasured(
      commandSpec(command, legacy),
      caseArgs(sourcePath, definition, output),
      outputRoot,
    );
    const current = await summarizeOutput(output, definition.outputKind);
    digests.add(current.digest);
    evidence = current;
    samples.push(measured);
  }
  const report = JSON.parse(samples.at(-1)?.stdout || '{}');
  const reportStats = report.stats ?? {};
  if (Number.isFinite(reportStats.triangleCount)) {
    evidence.decodedTriangleCount = evidence.triangleCount;
    evidence.triangleCount = reportStats.triangleCount;
  }
  if (Number.isFinite(reportStats.edgeSegmentCount)) {
    evidence.decodedEdgeSegmentCount = evidence.edgeSegmentCount;
    evidence.edgeSegmentCount = reportStats.edgeSegmentCount;
  }
  if (Number.isFinite(reportStats.partitionCount)) {
    evidence.partitionCount = reportStats.partitionCount;
  }
  if (Number.isFinite(reportStats.renderBatchCount)) {
    evidence.renderBatchCount = reportStats.renderBatchCount;
  }
  if (Number.isFinite(reportStats.coarseCount)) {
    evidence.coarseCount = reportStats.coarseCount;
  }
  return {
    p50Ms: percentile(samples.map((sample) => sample.wallMs), 0.5),
    p95Ms: percentile(samples.map((sample) => sample.wallMs), 0.95),
    peakMemoryBytes: Math.max(...samples.map((sample) => sample.peakMemoryBytes)),
    digestStable: digests.size === 1,
    digest: [...digests][0] ?? null,
    ...evidence,
  };
}

function caseArgs(sourcePath, definition, output) {
  return [
    definition.command,
    sourcePath,
    '--output',
    output,
    ...definition.args,
    '--json',
  ];
}

async function runMeasured(command, args, cwd) {
  const time = timeCommand(command, args);
  const started = performance.now();
  const result = spawnSync(time.command, time.args, {
    cwd,
    encoding: 'utf8',
    timeout: 300_000,
    maxBuffer: 32 * 1024 * 1024,
  });
  const wallMs = performance.now() - started;
  if (result.error || result.status !== 0) {
    throw new Error(
      `performance command failed: ${result.error?.message ?? result.stderr}`,
    );
  }
  return {
    wallMs,
    peakMemoryBytes: parsePeakMemory(result.stderr),
    stdout: result.stdout,
  };
}

async function summarizeOutput(path, kind) {
  if (kind === 'file') {
    const bytes = await readFile(path);
    return {
      outputBytes: bytes.byteLength,
      digest: sha256(bytes),
      edgeSegmentCount: parseGlbStats(bytes).edgeSegmentCount,
      triangleCount: parseGlbStats(bytes).triangleCount,
      partitionCount: 0,
      renderBatchCount: 0,
      coarseCount: 0,
    };
  }
  const files = await listFiles(path);
  const combined = files
    .map((file) => `${file.path}\0${file.digest}`)
    .join('\0');
  let edgeSegmentCount = 0;
  let triangleCount = 0;
  for (const file of files) {
    if (!file.path.toLowerCase().endsWith('.glb')) continue;
    const stats = parseGlbStats(file.bytes);
    edgeSegmentCount += stats.edgeSegmentCount;
    triangleCount += stats.triangleCount;
  }
  const tileset = JSON.parse(
    await readFile(join(path, 'tileset.json'), 'utf8'),
  );
  const partitionCount = files.filter(
    (file) => file.path.startsWith('parts/'),
  ).length;
  const coarseCount = files.filter(
    (file) => file.path.startsWith('coarse/'),
  ).length;
  const batching = tileset.asset?.extras?.ice3?.batching;
  return {
    outputBytes: files.reduce((sum, file) => sum + file.byteLength, 0),
    digest: sha256(Buffer.from(combined)),
    edgeSegmentCount,
    triangleCount,
    partitionCount,
    renderBatchCount: Array.isArray(batching?.batches)
      ? batching.batches.length
      : 0,
    coarseCount,
  };
}

function parseGlbStats(bytes) {
  if (bytes.readUInt32LE(0) !== 0x46546c67) {
    throw new Error('performance output is not GLB');
  }
  let offset = 12;
  let json;
  while (offset < bytes.length) {
    const length = bytes.readUInt32LE(offset);
    const type = bytes.readUInt32LE(offset + 4);
    if (type === 0x4e4f534a) {
      json = JSON.parse(
        bytes.subarray(offset + 8, offset + 8 + length).toString('utf8').trim(),
      );
      break;
    }
    offset += 8 + length;
  }
  const stats = json?.asset?.extras?.ice3?.stats;
  return {
    edgeSegmentCount: Number(stats?.edgeSegmentCount ?? 0),
    triangleCount: Number(stats?.triangleCount ?? 0),
  };
}

async function listFiles(root) {
  const files = [];
  async function walk(directory) {
    const entries = await readdir(directory, { withFileTypes: true });
    entries.sort((left, right) => left.name.localeCompare(right.name));
    for (const entry of entries) {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) {
        await walk(path);
      } else if (entry.isFile()) {
        const bytes = await readFile(path);
        files.push({
          path: path.slice(root.length + 1).split('\\').join('/'),
          byteLength: bytes.byteLength,
          digest: sha256(bytes),
          bytes,
        });
      }
    }
  }
  await walk(root);
  return files;
}

function compareWithBaseline(baseline, currentCases) {
  const failures = [];
  if (baseline.fixture?.sourceGlbSha256 === undefined) {
    failures.push('baseline.fixture');
  }
  const currentByName = new Map(
    currentCases.map((entry) => [entry.name, entry]),
  );
  for (const expected of baseline.cases ?? []) {
    const current = currentByName.get(expected.name);
    if (!current) {
      failures.push(`${expected.name}:missing`);
      continue;
    }
    if (current.p95Ms > expected.legacyP95Ms * legacyLimitMultiple) {
      failures.push(`${expected.name}:legacyP95`);
    }
    if (current.peakMemoryBytes > memoryLimitBytes) {
      failures.push(`${expected.name}:peakMemoryBytes`);
    }
    if (!current.digestStable) {
      failures.push(`${expected.name}:digestStable`);
    }
    if (current.digest !== expected.digest) {
      failures.push(`${expected.name}:digest`);
    }
  }
  return {
    captured: false,
    failures,
    baselineSourceDigest: baseline.fixture?.sourceGlbSha256,
  };
}

export async function createLargeSource(studioBinary, temporary) {
  const workspace = await mkdtemp(join(temporary, 'scene-'));
  const server = spawn(
    studioBinary,
    [
      'serve',
      '--workspace', workspace,
      '--project', 'large-industrial-benchmark-bootstrap',
      '--port', '0',
      '--write-mode', 'autonomous',
    ],
    {
      cwd: studioRoot,
      env: {
        ...process.env,
        STUDIO_SESSION_TOKEN: 'large-industrial-prepare-benchmark',
      },
      stdio: ['ignore', 'pipe', 'inherit'],
    },
  );
  try {
    const { address, token } = await waitForServer(server);
    let requestId = 0;
    const rpc = async (method, params = {}) => {
      requestId += 1;
      const response = await fetch(`http://${address}/rpc`, {
        method: 'POST',
        headers: {
          authorization: `Bearer ${token}`,
          'content-type': 'application/json',
        },
        body: JSON.stringify({
          jsonrpc: '2.0',
          id: requestId,
          method,
          params,
        }),
      });
      const envelope = await response.json();
      if (envelope.error) {
        throw new Error(`${method}: ${envelope.error.message}`);
      }
      return envelope.result;
    };
    const plan = buildIndustrialScenePlan();
    await rpc('project.create', { projectId: plan.projectId });
    for (const transaction of plan.transactions) {
      await rpc('document.apply', transaction);
    }
    const sceneStats = await rpc('evaluation.stats');
    const exported = await rpc('export.sourceGlb');
    const destination = join(temporary, 'large-industrial-source.glb');
    await copyFile(exported.sourcePath, destination);
    const bytes = await readFile(destination);
    return {
      path: destination,
      identity: {
        projectId: plan.projectId,
        sourceGlbBytes: bytes.byteLength,
        sourceGlbSha256: sha256(bytes),
        effectiveTriangles: sceneStats.effectiveTriangles,
        uniqueMeshes: sceneStats.uniqueMeshes,
      },
    };
  } finally {
    server.kill('SIGTERM');
    await new Promise((resolveExit) => {
      if (server.exitCode !== null || server.signalCode !== null) {
        resolveExit();
      } else {
        server.once('exit', resolveExit);
      }
    });
    await rm(workspace, { recursive: true, force: true });
  }
}

async function waitForServer(child) {
  const lines = createInterface({ input: child.stdout });
  const startup = [];
  for await (const line of lines) {
    if (line.trim()) startup.push(line.trim());
    if (startup.length === 2) break;
  }
  if (startup.length !== 2) throw new Error('Studio server did not start');
  return { address: startup[0], token: startup[1] };
}

async function resolveStudioBinary() {
  if (process.env.STUDIO_CLI_BIN) {
    return resolve(process.env.STUDIO_CLI_BIN);
  }
  runProcess('cargo', ['build', '--release', '-p', 'studio-cli']);
  return resolve(
    studioRoot,
    'target/release',
    process.platform === 'win32' ? 'studio-cli.exe' : 'studio-cli',
  );
}

function runProcess(command, args) {
  const result = spawnSync(command, args, {
    cwd: studioRoot,
    stdio: 'inherit',
    timeout: 300_000,
  });
  if (result.error || result.status !== 0) {
    throw new Error(`${command} failed`);
  }
}

function commandSpec(command, legacy) {
  if (legacy && command.toLowerCase().endsWith('.js')) {
    return { command: process.execPath, argsPrefix: [command] };
  }
  return { command, argsPrefix: [] };
}

function timeCommand(spec, args) {
  if (process.platform === 'darwin') {
    return {
      command: '/usr/bin/time',
      args: ['-l', spec.command, ...spec.argsPrefix, ...args],
    };
  }
  if (process.platform === 'linux') {
    return {
      command: '/usr/bin/time',
      args: ['-v', spec.command, ...spec.argsPrefix, ...args],
    };
  }
  return {
    command: spec.command,
    args: [...spec.argsPrefix, ...args],
  };
}

function parsePeakMemory(stderr) {
  const mac = stderr.match(/(\d+)\s+maximum resident set size/);
  if (mac) return Number(mac[1]);
  const linux = stderr.match(/Maximum resident set size \(kbytes\):\s*(\d+)/);
  if (linux) return Number(linux[1]) * 1024;
  return 0;
}

function percentile(values, quantile) {
  const sorted = [...values].sort((left, right) => left - right);
  const index = Math.min(
    sorted.length - 1,
    Math.max(0, Math.ceil(sorted.length * quantile) - 1),
  );
  return sorted[index];
}

function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}
