import assert from 'node:assert/strict';
import test from 'node:test';

import {
  evaluateBenchmarkGate,
  evaluateSceneComparison,
} from '../../benchmarks/large-industrial-scene.mjs';

test('GPU scene report fails closed when the scene session failed', () => {
  const result = {
    computeMode: 'gpu',
    visibleInstanceCount: 0,
    operationCount: 659,
    stats: {
      effectiveTriangles: 6_125_878,
      uniqueMeshes: 233,
    },
    incremental: {
      cacheHits: 231,
      generatedMeshes: 2,
    },
    measurements: {
      warmStatsMs: { p50: 3 },
      incrementalUpdateMs: { p50: 100 },
    },
    compute: {
      sceneSession: {
        state: 'failed',
      },
    },
  };

  assert.deepEqual(evaluateBenchmarkGate(result), [
    'gpuSceneState',
    'gpuVisibilityEmpty',
  ]);
});

test('scene comparison separates CPU incrementality from GPU acceleration', () => {
  const fullCpu = {
    measurements: {
      incrementalUpdateMs: { p50: 1200 },
      parameterUpdateMs: { p50: 300 },
      topologyUpdateMs: { p50: 400 },
      warmStatsMs: { p50: 2 },
    },
  };
  const incrementalCpu = {
    measurements: {
      incrementalUpdateMs: { p50: 400 },
      parameterUpdateMs: { p50: 200 },
      topologyUpdateMs: { p50: 250 },
      warmStatsMs: { p50: 2.1 },
    },
    compute: { sceneSession: { state: 'idle' } },
  };
  const gpuIncremental = {
    measurements: {
      incrementalUpdateMs: { p50: 200 },
      parameterUpdateMs: { p50: 100 },
      topologyUpdateMs: { p50: 200 },
      warmStatsMs: { p50: 2.2 },
    },
    compute: { sceneSession: { state: 'idle' } },
  };

  assert.deepEqual(
    evaluateSceneComparison(fullCpu, incrementalCpu, gpuIncremental),
    {
    failures: [],
    transformSpeedup: 6,
    cpuIncrementalSpeedup: 3,
    gpuOverIncrementalSpeedup: 2,
    gpuTotalSpeedup: 6,
    warmStatsRatio: 1.1,
    parameterSpeedup: 3,
    topologySpeedup: 2,
    passed: true,
    },
  );
});
