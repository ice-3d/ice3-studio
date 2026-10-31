import assert from 'node:assert/strict';
import test from 'node:test';
import { formatComputeStatus } from '../src/compute/ComputePanel';

test('summarizes ready native GPU evidence', () => {
  const view = formatComputeStatus({
    mode: 'auto',
    selectedBackend: 'gpu',
    adapter: 'integrated',
    tier: 'full',
    memoryBudgetBytes: 2 * 1024 * 1024 * 1024,
    health: 'ready',
    fallbackReason: null,
    queueDepth: 0,
    maxConcurrentJobs: 2,
    gpuThresholdTriangles: 250000,
    lastKernelMs: 12.4,
    lastUploadBytes: 2048,
    lastReadbackBytes: 5120 * 1024,
    lastCacheHits: 3,
  });
  assert.deepEqual(view, {
    title: 'Native compute · GPU',
    detail: 'cold · full · kernel 12.4 ms · readback 5 MiB',
    state: 'ready',
    fallbackReason: null,
    metrics: '2 GiB budget · 2 KiB upload · 3 cache hits · queue 0/2 · resident 0 B · peak 0 B · topology 0 · resident hits 0 · evictions 0 · packet 0 B · browser upload 0 B · snapshot 0 B · lookup 0 ms · kernel 12.4 ms · wait 0 ms · 0 hot-path hashes',
  });
});

test('surfaces the CPU fallback reason', () => {
  const view = formatComputeStatus({
    mode: 'auto',
    selectedBackend: 'cpu',
    adapter: 'reference',
    tier: 'cpu',
    memoryBudgetBytes: 2 * 1024 * 1024 * 1024,
    health: 'unavailable',
    fallbackReason: 'GPU device was lost',
    queueDepth: 0,
    maxConcurrentJobs: 2,
    gpuThresholdTriangles: 250000,
    lastKernelMs: 0,
    lastUploadBytes: 0,
    lastReadbackBytes: 0,
    lastCacheHits: 0,
  });
  assert.equal(view.title, 'Native compute · CPU');
  assert.equal(view.state, 'fallback');
  assert.equal(view.fallbackReason, 'GPU device was lost');
});

test('reports resident mode and partial browser upload evidence', () => {
  const view = formatComputeStatus({
    mode: 'auto',
    selectedBackend: 'gpu',
    adapter: 'integrated',
    tier: 'full',
    memoryBudgetBytes: 2 * 1024 * 1024 * 1024,
    health: 'ready',
    fallbackReason: null,
    queueDepth: 0,
    maxConcurrentJobs: 2,
    gpuThresholdTriangles: 250000,
    lastKernelMs: 2.4,
    lastUploadBytes: 0,
    lastReadbackBytes: 0,
    lastCacheHits: 8,
    lastEvaluationMode: 'resident',
    residentBytes: 1234,
    peakResidentBytes: 2048,
    topologyHits: 7,
    residentHits: 8,
    evictions: 0,
    previewPacketBytes: 512,
    browserUploadBytes: 0,
    snapshotReadbackBytes: 0,
  });

  assert.match(view.detail, /resident/);
  assert.match(view.detail, /readback 0/);
  assert.match(view.metrics, /packet 512 B/);
  assert.match(view.metrics, /browser upload 0 B/);
});

test('reports resident phase evidence separately', () => {
  const view = formatComputeStatus({
    mode: 'auto',
    selectedBackend: 'gpu',
    adapter: 'integrated',
    tier: 'full',
    memoryBudgetBytes: 2 * 1024 * 1024 * 1024,
    health: 'ready',
    fallbackReason: null,
    queueDepth: 0,
    maxConcurrentJobs: 2,
    gpuThresholdTriangles: 250000,
    lastKernelMs: 0,
    lastUploadBytes: 0,
    lastReadbackBytes: 0,
    lastCacheHits: 8,
    lastEvaluationMode: 'resident',
    residentBytes: 1234,
    residentHotPathHashes: 0,
    handleLookupMs: 0.02,
    lastGeometryPhases: {
      lookupMs: 0.02,
      planningMs: 0,
      validationMs: 0,
      encodeMs: 0,
      submitMs: 0,
      gpuKernelMs: 1.3,
      waitMs: 0,
      snapshotMs: 0,
      transportMs: 0,
      browserUploadMs: 0,
    },
  });
  assert.match(view.metrics, /lookup 0\.02 ms/);
  assert.match(view.metrics, /kernel 1\.3 ms/);
  assert.match(view.metrics, /wait 0 ms/);
});

test('reports pending scene revisions without calling them published', () => {
  const view = formatComputeStatus({
    mode: 'auto',
    selectedBackend: 'gpu',
    adapter: 'integrated',
    tier: 'full',
    memoryBudgetBytes: 2 * 1024 * 1024 * 1024,
    health: 'ready',
    fallbackReason: null,
    queueDepth: 0,
    maxConcurrentJobs: 2,
    gpuThresholdTriangles: 250000,
    lastKernelMs: 0,
    lastUploadBytes: 0,
    lastReadbackBytes: 0,
    lastCacheHits: 0,
    sceneSession: {
      enabled: true,
      sessionId: 'scene-1',
      state: 'pending',
      publishedRevision: 8,
      targetRevision: 9,
      pendingPatches: 1,
      pendingSubmissions: 1,
      geometryCount: 233,
      topologyCount: 233,
      attributeCount: 233,
      instanceCount: 1938,
      residentBytes: 4096,
      peakResidentBytes: 8192,
      dirtyRangeBytes: 0,
      readbackBytes: 0,
      submissionCount: 1,
      geometryDispatchCount: 0,
      fallbackReason: null,
      phases: {},
    },
  });

  assert.match(view.detail, /scene pending 8 → 9/);
  assert.match(view.metrics, /1938 instances/);
  assert.match(view.metrics, /dirty 0 B/);
});
