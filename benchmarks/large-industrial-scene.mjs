import { spawn } from 'node:child_process';
import {
  mkdir,
  mkdtemp,
  readFile,
  rename,
  rm,
  stat,
  writeFile,
} from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { performance } from 'node:perf_hooks';
import { createInterface } from 'node:readline';
import { pathToFileURL } from 'node:url';
import { buildIndustrialScenePlan } from '../fixtures/large-industrial-site/operations.mjs';

export function evaluateBenchmarkGate(result) {
  const failures = [];
  if (
    result.stats.effectiveTriangles < 5_500_000
    || result.stats.effectiveTriangles > 7_500_000
  ) {
    failures.push('effectiveTriangles');
  }
  if (result.stats.uniqueMeshes > 300) {
    failures.push('uniqueMeshes');
  }
  if (
    result.incremental.cacheHits < 1
    || result.incremental.generatedMeshes >= result.stats.uniqueMeshes
  ) {
    failures.push('incrementalCacheReuse');
  }
  if (result.operationCount >= 100_000) {
    failures.push('operationCount');
  }
  return failures;
}

async function main() {
  const workspace = await mkdtemp(join(tmpdir(), 'ice3-studio-large-benchmark-'));
  let server;
  try {
    await run('cargo', ['build', '--release', '-p', 'studio-cli']);
    const binary = resolve(
      'target/release',
      process.platform === 'win32' ? 'studio-cli.exe' : 'studio-cli',
    );
    server = spawn(
      binary,
      [
        'serve',
        '--workspace',
        workspace,
        '--project',
        'large-industrial-benchmark-bootstrap',
        '--port',
        '0',
        '--write-mode',
        'autonomous',
      ],
      {
        cwd: process.cwd(),
        env: {
          ...process.env,
          STUDIO_SESSION_TOKEN: 'large-industrial-benchmark',
          STUDIO_VIEWER_DIR: resolve('apps/viewer/dist'),
        },
        stdio: ['ignore', 'pipe', 'inherit'],
      },
    );
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
      if (envelope.error) throw new Error(`${method}: ${envelope.error.message}`);
      return envelope.result;
    };

    const planSamples = [];
    let plan;
    for (let sample = 0; sample < 7; sample += 1) {
      const started = performance.now();
      plan = buildIndustrialScenePlan();
      planSamples.push(performance.now() - started);
    }
    await rpc('project.create', { projectId: plan.projectId });
    const applyStarted = performance.now();
    let revision;
    for (const transaction of plan.transactions) {
      const result = await rpc('document.apply', transaction);
      revision = result.revision;
    }
    const fullApplyMs = performance.now() - applyStarted;
    if (revision !== plan.transactions.length) {
      throw new Error('benchmark scene did not commit all transactions');
    }

    const coldStats = await rpc('evaluation.stats');
    const warmSamples = [];
    for (let sample = 0; sample < 7; sample += 1) {
      const started = performance.now();
      await rpc('evaluation.stats');
      warmSamples.push(performance.now() - started);
    }

    await rpc('project.open', { projectId: plan.projectId });
    const document = await rpc('document.get');
    const primaryNodes = document.scene.nodes.filter(
      (node) => node.content?.type === 'component_instance',
    );
    const updateTarget = primaryNodes.find(
      (node) => node.metadata?.kind === 'secondary-structure',
    );
    if (!updateTarget) throw new Error('benchmark update target is missing');

    const fullUpdateSamples = [];
    const incrementalSamples = [];
    let incrementalStats;
    let currentRevision = document.revision;
    for (let sample = 0; sample < 7; sample += 1) {
      const started = performance.now();
      const updated = await rpc('document.apply', {
        expectedRevision: currentRevision,
        dryRun: false,
        idempotencyKey: `benchmark-all-update-${sample}`,
        operations: primaryNodes.map((node) => ({
          type: 'node.update',
          node_id: node.id,
          name: node.name,
          transform: null,
          visible: null,
          metadata: { benchmarkPass: String(sample) },
        })),
      });
      currentRevision = updated.revision;
      await rpc('evaluation.stats');
      fullUpdateSamples.push(performance.now() - started);

      const incrementalStarted = performance.now();
      const moved = await rpc('document.apply', {
        expectedRevision: currentRevision,
        dryRun: false,
        idempotencyKey: `benchmark-one-update-${sample}`,
        operations: [{
          type: 'node.update',
          node_id: updateTarget.id,
          name: null,
          transform: {
            translation: {
              x: updateTarget.transform.translation.x,
              y: updateTarget.transform.translation.y,
              z: updateTarget.transform.translation.z + sample * 0.01,
            },
            rotation: updateTarget.transform.rotation,
            scale: updateTarget.transform.scale,
          },
          visible: null,
          metadata: null,
        }],
      });
      currentRevision = moved.revision;
      incrementalStats = await rpc('evaluation.stats');
      incrementalSamples.push(performance.now() - incrementalStarted);
    }

    const exportSamples = [];
    let sourceBytes = 0;
    for (let sample = 0; sample < 3; sample += 1) {
      const started = performance.now();
      const exported = await rpc('export.sourceGlb');
      sourceBytes = (await stat(exported.sourcePath)).size;
      exportSamples.push(performance.now() - started);
    }

    const reopenSamples = [];
    for (let sample = 0; sample < 7; sample += 1) {
      await rpc('project.open', {
        projectId: 'large-industrial-benchmark-bootstrap',
      });
      const started = performance.now();
      await rpc('project.open', { projectId: plan.projectId });
      await rpc('evaluation.stats');
      reopenSamples.push(performance.now() - started);
    }

    const capabilities = await rpc('system.capabilities');
    const previewSamples = [];
    if (capabilities.toolchain?.state === 'ready') {
      for (let sample = 0; sample < 3; sample += 1) {
        const started = performance.now();
        await rpc('export.runtimePreview');
        previewSamples.push(performance.now() - started);
      }
    }

    const operationCount = plan.transactions.reduce(
      (sum, transaction) => sum + transaction.operations.length,
      0,
    );
    const result = {
      suite: 'large-industrial',
      projectId: plan.projectId,
      revision: currentRevision,
      operationCount,
      stats: coldStats,
      finalStats: incrementalStats,
      incremental: incrementalStats,
      documentBytes: Buffer.byteLength(JSON.stringify(document)),
      sourceGlbBytes: sourceBytes,
      measurements: {
        planConstructionMs: metric(planSamples),
        fullApplyMs: metric([fullApplyMs]),
        fullUpdateMs: metric(fullUpdateSamples),
        warmStatsMs: metric(warmSamples),
        incrementalUpdateMs: metric(incrementalSamples),
        sourceGlbExportMs: metric(exportSamples),
        reopenMs: metric(reopenSamples),
        runtimePreviewMs: metric(previewSamples),
      },
    };
    result.failedMetrics = evaluateBenchmarkGate(result);
    result.passed = result.failedMetrics.length === 0;
    await mkdir('output', { recursive: true });
    const destination = resolve('output/large-industrial-benchmark.json');
    const temporary = `${destination}.tmp`;
    await writeFile(temporary, `${JSON.stringify(result, null, 2)}\n`);
    await rename(temporary, destination);
    console.log(JSON.stringify(result, null, 2));
    if (!result.passed) {
      throw new Error(`large industrial benchmark failed: ${result.failedMetrics.join(', ')}`);
    }
  } finally {
    if (server) {
      server.kill('SIGTERM');
      await new Promise((resolveExit) => server.once('exit', resolveExit));
    }
    await rm(workspace, { recursive: true, force: true });
  }
}

function metric(samples) {
  if (samples.length === 0) {
    return { samples: 0, p50: null, p95: null, values: [] };
  }
  return {
    unit: 'ms',
    samples: samples.length,
    p50: percentile(samples, 0.5),
    p95: percentile(samples, 0.95),
    values: samples.map((value) => Number(value.toFixed(3))),
  };
}

function percentile(samples, fraction) {
  const ordered = [...samples].sort((left, right) => left - right);
  const index = Math.max(0, Math.ceil(ordered.length * fraction) - 1);
  return Number(ordered[index].toFixed(3));
}

function run(command, args) {
  return new Promise((resolveRun, reject) => {
    const child = spawn(command, args, {
      cwd: process.cwd(),
      env: process.env,
      stdio: 'inherit',
    });
    child.once('error', reject);
    child.once('exit', (code, signal) => {
      if (code === 0) resolveRun();
      else reject(new Error(`${command} exited with ${signal ?? code}`));
    });
  });
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

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  await main();
}
