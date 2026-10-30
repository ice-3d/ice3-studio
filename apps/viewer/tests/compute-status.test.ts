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
    detail: 'full · kernel 12.4 ms · 5120 KiB readback',
    state: 'ready',
    fallbackReason: null,
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
