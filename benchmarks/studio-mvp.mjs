import { spawn, spawnSync } from 'node:child_process';
import {
  cp,
  mkdir,
  mkdtemp,
  readFile,
  rename,
  rm,
  writeFile,
} from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { createInterface } from 'node:readline';
import { performance } from 'node:perf_hooks';

const limits = {
  serverColdStartMs: 3_000,
  projectOpen100NodesMs: 1_000,
  transaction100OperationsMs: 500,
  wastewaterMiniEvaluationMs: 300,
  sourceGlbExportMs: 2_000,
  runtimePreviewMs: 10_000,
  changesetEvaluate100OperationsMs: 500,
  changesetCandidatePreviewMs: 10_000,
};

function percentile(samples, fraction) {
  const ordered = [...samples].sort((left, right) => left - right);
  const index = Math.min(
    ordered.length - 1,
    Math.max(0, Math.ceil(ordered.length * fraction) - 1),
  );
  return ordered[index];
}

function metric(samples) {
  return {
    unit: 'ms',
    samples: samples.length,
    p50: Number(percentile(samples, 0.5).toFixed(3)),
    p95: Number(percentile(samples, 0.95).toFixed(3)),
    values: samples.map((value) => Number(value.toFixed(3))),
  };
}

function run(command, args, options = {}) {
  return spawnSync(command, args, {
    cwd: process.cwd(),
    env: process.env,
    stdio: 'inherit',
    ...options,
  });
}

async function waitForServer(child) {
  const lines = createInterface({ input: child.stdout });
  const startup = [];
  for await (const line of lines) {
    if (line.trim()) startup.push(line.trim());
    if (startup.length === 2) break;
  }
  if (startup.length !== 2) throw new Error('Studio release server did not start');
  return { address: startup[0], token: startup[1] };
}

const fixtureSource = resolve('fixtures/wastewater-mini');
const workspace = await mkdtemp(join(tmpdir(), 'ice3-studio-benchmark-'));
const fixture = join(workspace, 'fixtures/wastewater-mini');
let server;

try {
  const cliBuild = run('cargo', ['build', '--release', '-p', 'studio-cli']);
  if (cliBuild.status !== 0) throw new Error('release CLI build failed');

  await mkdir(join(workspace, 'fixtures'), { recursive: true });
  await cp(fixtureSource, fixture, { recursive: true });

  const binary = resolve(
    'target/release',
    process.platform === 'win32' ? 'studio-cli.exe' : 'studio-cli',
  );
  const coldStart = performance.now();
  server = spawn(
    binary,
    [
      'serve',
      '--workspace',
      workspace,
      '--project',
      'wastewater-mini',
      '--fixture',
      fixture,
      '--port',
      '0',
      '--write-mode',
      'autonomous',
    ],
    {
      cwd: process.cwd(),
      env: {
        ...process.env,
        STUDIO_SESSION_TOKEN: 'studio-benchmark',
      },
      stdio: ['ignore', 'pipe', 'inherit'],
    },
  );
  const { address, token } = await waitForServer(server);
  let requestId = 0;

  async function request(path) {
    const response = await fetch(`http://${address}${path}`);
    if (!response.ok) throw new Error(`${path} returned ${response.status}`);
    return response.json();
  }

  async function rpc(method, params = {}) {
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
      throw new Error(`${method}: ${envelope.error.code} ${envelope.error.message}`);
    }
    return envelope.result;
  }

  async function timed(operation) {
    const started = performance.now();
    const result = await operation();
    return { duration: performance.now() - started, result };
  }

  const health = await request('/health');
  if (health.status !== 'ok') throw new Error('health check failed');
  const coldStartMs = performance.now() - coldStart;

  const largeProjectId = 'benchmark-large';
  await rpc('project.create', { projectId: largeProjectId });
  const nodeIds = Array.from(
    { length: 100 },
    (_, index) => `node-${String(index + 1).padStart(6, '0')}`,
  );
  const createOperations = nodeIds.map((nodeId, index) => ({
    type: 'node.create',
    node_id: nodeId,
    parent_id: null,
    name: `Benchmark Box ${index + 1}`,
    transform: {
      translation: {
        x: (index % 10) * 2,
        y: 0,
        z: Math.floor(index / 10) * 2,
      },
      rotation: [0, 0, 0, 1],
      scale: { x: 1, y: 1, z: 1 },
    },
    content: {
      type: 'primitive',
      kind: 'box',
      width: 1.5,
      height: 1,
      depth: 1.5,
    },
    material_id: null,
    metadata: { role: 'benchmark' },
  }));
  await rpc('document.apply', {
    expectedRevision: 0,
    dryRun: false,
    idempotencyKey: 'benchmark-create-100',
    operations: createOperations,
  });

  const openSamples = [];
  for (let sample = 0; sample < 10; sample += 1) {
    await rpc('project.open', { projectId: 'wastewater-mini' });
    const measurement = await timed(() => (
      rpc('project.open', { projectId: largeProjectId })
    ));
    const opened = await rpc('document.get');
    if (opened.revision !== 1) {
      throw new Error('100-node project did not reopen at revision 1');
    }
    openSamples.push(measurement.duration);
  }

  const transactionSamples = [];
  for (let batch = 0; batch < 10; batch += 1) {
    const operations = nodeIds.map((nodeId) => ({
      type: 'node.update',
      node_id: nodeId,
      name: `Benchmark Box ${batch + 1}`,
      transform: null,
      visible: null,
      metadata: null,
    }));
    const measurement = await timed(() => rpc('document.apply', {
      expectedRevision: 1 + batch,
      dryRun: false,
      idempotencyKey: `benchmark-update-${batch}`,
      operations,
    }));
    if (measurement.result.revision !== 2 + batch) {
      throw new Error('100-operation transaction returned the wrong revision');
    }
    transactionSamples.push(measurement.duration);
  }

  const changesetSamples = [];
  const candidateDigests = [];
  for (let sample = 0; sample < 10; sample += 1) {
    const created = await rpc('changeset.create', {
      baseRevision: 11,
      title: `Benchmark proposal ${sample + 1}`,
      idempotencyKey: `benchmark-create-${sample + 1}`,
    });
    const operations = nodeIds.map((nodeId) => ({
      type: 'node.update',
      node_id: nodeId,
      name: `Candidate ${sample + 1}`,
      transform: null,
      visible: null,
      metadata: null,
    }));
    const measurement = await timed(async () => {
      await rpc('changeset.addOperations', {
        changeSetId: created.id,
        expectedVersion: 0,
        idempotencyKey: `benchmark-add-${sample + 1}`,
        entries: operations.map((operation) => ({
          source: { kind: 'agent', label: 'benchmark' },
          operation,
        })),
      });
      return rpc('changeset.evaluate', {
        changeSetId: created.id,
        expectedVersion: 1,
        includeCandidate: false,
      });
    });
    if (measurement.result.status !== 'ready') {
      throw new Error('benchmark ChangeSet did not evaluate');
    }
    changesetSamples.push(measurement.duration);
    candidateDigests.push(measurement.result.candidateDigest);
  }
  const unchanged = await rpc('document.get');
  if (unchanged.revision !== 11) {
    throw new Error('ChangeSet evaluation changed the committed revision');
  }

  const changesetPreviewSamples = [];
  for (const digest of candidateDigests.slice(0, 3)) {
    const measurement = await timed(() => rpc('export.runtimePreview', {
      candidateDigest: digest,
    }));
    if (!measurement.result.url) {
      throw new Error('candidate preview omitted its URL');
    }
    changesetPreviewSamples.push(measurement.duration);
  }

  await rpc('project.open', { projectId: 'wastewater-mini' });
  const evaluationSamples = [];
  for (let sample = 0; sample < 10; sample += 1) {
    const measurement = await timed(() => rpc('evaluation.evaluate'));
    if (!measurement.result['node-000004']) {
      throw new Error('wastewater-mini evaluation omitted a clarifier');
    }
    evaluationSamples.push(measurement.duration);
  }

  const exportSamples = [];
  for (let sample = 0; sample < 10; sample += 1) {
    const measurement = await timed(() => rpc('export.sourceGlb'));
    if (!measurement.result.sourcePath) throw new Error('source export omitted its path');
    exportSamples.push(measurement.duration);
  }

  const previewSamples = [];
  for (let sample = 0; sample < 3; sample += 1) {
    const measurement = await timed(() => rpc('export.runtimePreview'));
    if (!measurement.result.previewPath) throw new Error('preview omitted its path');
    previewSamples.push(measurement.duration);
  }

  const report = {
    suite: 'studio-mvp',
    fixture: 'wastewater-mini',
    operationsPerTransaction: 100,
    measurements: {
      serverColdStartMs: metric([coldStartMs]),
      projectOpen100NodesMs: metric(openSamples),
      transaction100OperationsMs: metric(transactionSamples),
      wastewaterMiniEvaluationMs: metric(evaluationSamples),
      sourceGlbExportMs: metric(exportSamples),
      runtimePreviewMs: metric(previewSamples),
      changesetEvaluate100OperationsMs: metric(changesetSamples),
      changesetCandidatePreviewMs: metric(changesetPreviewSamples),
    },
    limits,
  };

  const failures = [];
  for (const [name, limit] of Object.entries(limits)) {
    if (report.measurements[name].p95 > limit) failures.push(name);
  }
  report.passed = failures.length === 0;
  report.failedMetrics = failures;

  await mkdir('output', { recursive: true });
  const destination = resolve('output/studio-mvp-benchmark.json');
  const temporary = `${destination}.tmp`;
  await writeFile(temporary, `${JSON.stringify(report, null, 2)}\n`);
  await rename(temporary, destination);

  const saved = JSON.parse(await readFile(destination, 'utf8'));
  console.log(JSON.stringify(saved, null, 2));
  if (!saved.passed) {
    throw new Error(`performance limits exceeded: ${failures.join(', ')}`);
  }
} finally {
  if (server) {
    server.kill('SIGTERM');
    await new Promise((resolveExit) => {
      server.once('exit', resolveExit);
    });
  }
  await rm(workspace, { recursive: true, force: true });
}
