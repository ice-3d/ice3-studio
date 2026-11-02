import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
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

const VISIBILITY_SAMPLE_COUNT = 31;

export function evaluateBenchmarkGate(result) {
  const failures = [];
  if (
    result.stats.effectiveTriangles < 800_000
    || result.stats.effectiveTriangles > 1_000_000
  ) {
    failures.push('effectiveTriangles');
  }
  if (result.stats.uniqueMeshes > 1_200) {
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
  if (
    result.computeMode === 'gpu'
    && result.measurements.warmStatsMs.p50 >= 100
  ) {
    failures.push('gpuSceneWarmStats');
  }
  if (
    result.computeMode === 'gpu'
    && result.compute?.sceneSession?.state !== 'idle'
  ) {
    failures.push('gpuSceneState');
  }
  if (
    result.computeMode === 'gpu'
    && result.measurements.incrementalUpdateMs.p50 > 250
  ) {
    failures.push('gpuTransformBelowFivefold');
  }
  if (result.computeMode === 'gpu' && result.visibleInstanceCount === 0) {
    failures.push('gpuVisibilityEmpty');
  }
  if (!result.agentContext) {
    failures.push('agentContextEvidence');
  } else {
    if (result.agentContext.contextBytes > 16 * 1024) {
      failures.push('agentContextBytes');
    }
    if (result.agentContext.searchPageBytes > 128 * 1024) {
      failures.push('agentSearchPageBytes');
    }
    if (result.agentContext.firstPageItems > 25) {
      failures.push('agentSearchLimit');
    }
  }
  return failures;
}

export function evaluateSceneComparison(fullCpu, incrementalCpu, gpu) {
  const warmStatsSchedulingFloorMs = 1;
  const failures = [];
  const transformSpeedup =
    fullCpu.measurements.incrementalUpdateMs.p50
    / gpu.measurements.incrementalUpdateMs.p50;
  const cpuIncrementalSpeedup =
    fullCpu.measurements.incrementalUpdateMs.p50
    / incrementalCpu.measurements.incrementalUpdateMs.p50;
  const gpuOverIncrementalSpeedup =
    incrementalCpu.measurements.incrementalUpdateMs.p50
    / gpu.measurements.incrementalUpdateMs.p50;
  const gpuTotalSpeedup = transformSpeedup;
  const warmStatsRatio =
    gpu.measurements.warmStatsMs.p50
    / incrementalCpu.measurements.warmStatsMs.p50;
  const warmStatsRegression =
    gpu.measurements.warmStatsMs.p50
    > Math.max(
      incrementalCpu.measurements.warmStatsMs.p50 * 1.1,
      incrementalCpu.measurements.warmStatsMs.p50 + warmStatsSchedulingFloorMs,
    );
  const parameterSpeedup =
    fullCpu.measurements.parameterUpdateMs.p50
    / gpu.measurements.parameterUpdateMs.p50;
  const topologySpeedup =
    fullCpu.measurements.topologyUpdateMs.p50
    / gpu.measurements.topologyUpdateMs.p50;
  const visibilitySpeedup =
    incrementalCpu.measurements.instanceVisibilityMs.p50
    / gpu.measurements.instanceVisibilityMs.p50;
  if (transformSpeedup < 5) failures.push('transformBelowFivefold');
  if (warmStatsRegression) failures.push('warmStatsRegression');
  if (gpu.compute?.sceneSession?.state !== 'idle') failures.push('gpuSceneState');
  if (
    incrementalCpu.visibleInstanceCount === 0
    || incrementalCpu.visibleInstanceCount !== gpu.visibleInstanceCount
    || incrementalCpu.visibleNodeIdsSha256 !== gpu.visibleNodeIdsSha256
  ) {
    failures.push('visibilityParity');
  }
  return {
    failures,
    transformSpeedup,
    cpuIncrementalSpeedup,
    gpuOverIncrementalSpeedup,
    gpuTotalSpeedup,
    warmStatsRatio,
    parameterSpeedup,
    topologySpeedup,
    visibilitySpeedup,
    passed: failures.length === 0,
  };
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
    const sceneSessions = process.env.STUDIO_GPU_SCENE_SESSIONS === '1';
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
        ...(sceneSessions ? ['--native-gpu', 'gpu'] : []),
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
    const initialBuildMs = performance.now() - applyStarted;
    if (revision !== plan.transactions.length) {
      throw new Error('benchmark scene did not commit all transactions');
    }

    const coldStats = await rpc('evaluation.stats');
    const agentContextResponse = await rpc('scene.context');
    const agentContextBytes = Buffer.byteLength(JSON.stringify(agentContextResponse));
    const agentSearchPage = await rpc('scene.search', { scope: 'both', limit: 25 });
    const agentSearchPageBytes = Buffer.byteLength(JSON.stringify(agentSearchPage));
    const agentSearchSecondPage = agentSearchPage.nextCursor
      ? await rpc('scene.search', {
          scope: 'both',
          limit: 25,
          cursor: agentSearchPage.nextCursor,
        })
      : null;
    const agentContext = {
      contextBytes: agentContextBytes,
      searchPageBytes: agentSearchPageBytes,
      firstPageItems: agentSearchPage.items.length,
      secondPageItems: agentSearchSecondPage?.items.length ?? 0,
    };
    const visibilitySamples = [];
    let visibleInstanceCount = 0;
    let visibleNodeIdsSha256 = null;
    if (sceneSessions) await rpc('evaluation.waitScene', { revision });
    for (let sample = 0; sample < VISIBILITY_SAMPLE_COUNT; sample += 1) {
      const started = performance.now();
      const visibility = await rpc('evaluation.visibleInstances', {
        targetRevision: revision,
        planes: [
          [1, 0, 0, 1000000000],
          [-1, 0, 0, 400],
          [0, 1, 0, 1000000000],
          [0, -1, 0, 1000000000],
          [0, 0, 1, 1000000000],
          [0, 0, -1, 1000000000],
        ],
      });
      visibilitySamples.push(performance.now() - started);
      visibleInstanceCount = visibility.visibleIndices.length;
      const digest = digestVisibleNodes(visibility.visibleNodeIds);
      if (visibleNodeIdsSha256 !== null && visibleNodeIdsSha256 !== digest) {
        throw new Error('visible node identities changed between identical samples');
      }
      visibleNodeIdsSha256 = digest;
    }
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
    const transformApplySamples = [];
    const transformWaitSamples = [];
    const transformStatsSamples = [];
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
      transformApplySamples.push(performance.now() - incrementalStarted);
      const waitStarted = performance.now();
      if (sceneSessions) {
        await rpc('evaluation.waitScene', { revision: currentRevision });
      }
      transformWaitSamples.push(performance.now() - waitStarted);
      const statsStarted = performance.now();
      incrementalStats = await rpc('evaluation.stats');
      transformStatsSamples.push(performance.now() - statsStarted);
      incrementalSamples.push(performance.now() - incrementalStarted);
    }

    const createdParameter = await rpc('document.apply', {
      expectedRevision: currentRevision,
      dryRun: false,
      idempotencyKey: 'benchmark-parameter-create',
      operations: [{
        type: 'node.create',
        node_id: 'node-900001',
        parent_id: null,
        name: 'Parameter Workload',
        transform: {
          translation: { x: 0, y: 0, z: 0 },
          rotation: [0, 0, 0, 1],
          scale: { x: 1, y: 1, z: 1 },
        },
        content: {
          type: 'primitive',
          kind: 'box',
          width: 1,
          height: 1,
          depth: 1,
        },
        material_id: null,
        metadata: {},
      }],
    });
    currentRevision = createdParameter.revision;
    await rpc('evaluation.stats');
    if (sceneSessions) {
      await rpc('evaluation.waitScene', { revision: currentRevision });
    }

    const parameterSamples = [];
    const parameterApplySamples = [];
    const parameterWaitSamples = [];
    const parameterStatsSamples = [];
    for (let sample = 0; sample < 7; sample += 1) {
      const started = performance.now();
      const updated = await rpc('document.apply', {
        expectedRevision: currentRevision,
        dryRun: false,
        idempotencyKey: `benchmark-parameter-${sample}`,
        operations: [{
          type: 'primitive.update',
          node_id: 'node-900001',
          primitive: {
            type: 'primitive',
            kind: 'box',
            width: 1 + sample * 0.1,
            height: 1,
            depth: 1,
          },
        }],
      });
      currentRevision = updated.revision;
      parameterApplySamples.push(performance.now() - started);
      const waitStarted = performance.now();
      if (sceneSessions) {
        await rpc('evaluation.waitScene', { revision: currentRevision });
      }
      parameterWaitSamples.push(performance.now() - waitStarted);
      const statsStarted = performance.now();
      await rpc('evaluation.stats');
      parameterStatsSamples.push(performance.now() - statsStarted);
      parameterSamples.push(performance.now() - started);
    }

    const topologySamples = [];
    const topologyApplySamples = [];
    const topologyWaitSamples = [];
    const topologyStatsSamples = [];
    for (let sample = 0; sample < 3; sample += 1) {
      const started = performance.now();
      const updated = await rpc('document.apply', {
        expectedRevision: currentRevision,
        dryRun: false,
        idempotencyKey: `benchmark-topology-${sample}`,
        operations: [{
          type: 'primitive.update',
          node_id: 'node-900001',
          primitive: sample % 2 === 0
            ? {
                type: 'primitive',
                kind: 'cylinder',
                radius: 1,
                height: 1 + sample * 0.1,
                segments: 32,
              }
            : {
                type: 'primitive',
                kind: 'sphere',
                radius: 1,
                segments: 32,
                rings: 16,
              },
        }],
      });
      currentRevision = updated.revision;
      topologyApplySamples.push(performance.now() - started);
      const waitStarted = performance.now();
      if (sceneSessions) {
        await rpc('evaluation.waitScene', { revision: currentRevision });
      }
      topologyWaitSamples.push(performance.now() - waitStarted);
      const statsStarted = performance.now();
      await rpc('evaluation.stats');
      topologyStatsSamples.push(performance.now() - statsStarted);
      topologySamples.push(performance.now() - started);
    }

    const exportSamples = [];
    let sourceBytes = 0;
    for (let sample = 0; sample < 7; sample += 1) {
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

    if (sceneSessions) {
      await rpc('evaluation.waitScene', { revision: currentRevision });
    }
    const capabilities = await rpc('system.capabilities');
    const previewSamples = [];
    for (let sample = 0; sample < 3; sample += 1) {
      const started = performance.now();
      await rpc('export.runtimePreview');
      previewSamples.push(performance.now() - started);
    }

    const operationCount = plan.transactions.reduce(
      (sum, transaction) => sum + transaction.operations.length,
      0,
    );
    const result = {
      suite: 'large-industrial',
      computeMode: sceneSessions ? 'gpu' : 'cpu',
      compute: capabilities.compute,
      projectId: plan.projectId,
      revision: currentRevision,
      operationCount,
      stats: coldStats,
      finalStats: incrementalStats,
      incremental: incrementalStats,
      documentBytes: Buffer.byteLength(JSON.stringify(document)),
      sourceGlbBytes: sourceBytes,
      visibleInstanceCount,
      visibleNodeIdsSha256,
      agentContext,
      measurements: {
        planConstructionMs: metric(planSamples),
        initialBuildMs: metric([initialBuildMs]),
        fullApplyMs: metric(fullUpdateSamples),
        fullUpdateMs: metric(fullUpdateSamples),
        warmStatsMs: metric(warmSamples),
        incrementalUpdateMs: metric(incrementalSamples),
        transformApplyMs: metric(transformApplySamples),
        transformWaitMs: metric(transformWaitSamples),
        transformStatsMs: metric(transformStatsSamples),
        parameterUpdateMs: metric(parameterSamples),
        parameterApplyMs: metric(parameterApplySamples),
        parameterWaitMs: metric(parameterWaitSamples),
        parameterStatsMs: metric(parameterStatsSamples),
        topologyUpdateMs: metric(topologySamples),
        topologyApplyMs: metric(topologyApplySamples),
        topologyWaitMs: metric(topologyWaitSamples),
        topologyStatsMs: metric(topologyStatsSamples),
        instanceVisibilityMs: metric(visibilitySamples),
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
    console.log(`BENCHMARK_JSON:${JSON.stringify(result)}`);
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

function digestVisibleNodes(nodeIds) {
  const hash = createHash('sha256');
  for (const nodeId of [...nodeIds].sort()) {
    hash.update(nodeId);
    hash.update('\0');
  }
  return hash.digest('hex');
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
