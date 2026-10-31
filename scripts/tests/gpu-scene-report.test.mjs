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
      instanceVisibilityMs: { p50: 9 },
    },
    visibleInstanceCount: 480,
    visibleNodeIdsSha256: 'visibility',
  };
  const incrementalCpu = {
    measurements: {
      incrementalUpdateMs: { p50: 400 },
      parameterUpdateMs: { p50: 200 },
      topologyUpdateMs: { p50: 250 },
      warmStatsMs: { p50: 2.1 },
      instanceVisibilityMs: { p50: 6 },
    },
    visibleInstanceCount: 480,
    visibleNodeIdsSha256: 'visibility',
    compute: { sceneSession: { state: 'idle' } },
  };
  const gpuIncremental = {
    measurements: {
      incrementalUpdateMs: { p50: 200 },
      parameterUpdateMs: { p50: 100 },
      topologyUpdateMs: { p50: 200 },
      warmStatsMs: { p50: 2.2 },
      instanceVisibilityMs: { p50: 2 },
    },
    visibleInstanceCount: 480,
    visibleNodeIdsSha256: 'visibility',
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
    warmStatsRatio: 1.0476190476190477,
    parameterSpeedup: 3,
    topologySpeedup: 2,
    visibilitySpeedup: 3,
    passed: true,
    },
  );
});

test('scene comparison ignores sub-millisecond warm stats scheduling noise', () => {
  const measurements = (incremental, parameter, topology, stats, visibility) => ({
    incrementalUpdateMs: { p50: incremental },
    parameterUpdateMs: { p50: parameter },
    topologyUpdateMs: { p50: topology },
    warmStatsMs: { p50: stats },
    instanceVisibilityMs: { p50: visibility },
  });
  const fullCpu = {
    measurements: measurements(1200, 300, 400, 0.4, 3),
    visibleInstanceCount: 243,
    visibleNodeIdsSha256: 'visibility',
  };
  const incrementalCpu = {
    measurements: measurements(100, 100, 100, 0.24, 2.8),
    visibleInstanceCount: 243,
    visibleNodeIdsSha256: 'visibility',
    compute: { sceneSession: { state: 'idle' } },
  };
  const gpu = {
    measurements: measurements(90, 90, 90, 0.27, 1),
    visibleInstanceCount: 243,
    visibleNodeIdsSha256: 'visibility',
    compute: { sceneSession: { state: 'idle' } },
  };

  const comparison = evaluateSceneComparison(fullCpu, incrementalCpu, gpu);

  assert.deepEqual(comparison.failures, []);
  assert.equal(comparison.passed, true);
});

test('scene comparison rejects matching visibility counts with different node identities', () => {
  const measurements = (visibility) => ({
    incrementalUpdateMs: { p50: 100 },
    parameterUpdateMs: { p50: 100 },
    topologyUpdateMs: { p50: 100 },
    warmStatsMs: { p50: 0.3 },
    instanceVisibilityMs: { p50: visibility },
  });
  const fullCpu = {
    measurements: measurements(3),
    visibleInstanceCount: 243,
    visibleNodeIdsSha256: 'cpu',
  };
  const incrementalCpu = {
    measurements: measurements(3),
    visibleInstanceCount: 243,
    visibleNodeIdsSha256: 'cpu',
    compute: { sceneSession: { state: 'idle' } },
  };
  const gpu = {
    measurements: measurements(1),
    visibleInstanceCount: 243,
    visibleNodeIdsSha256: 'gpu',
    compute: { sceneSession: { state: 'idle' } },
  };

  assert.ok(
    evaluateSceneComparison(fullCpu, incrementalCpu, gpu).failures
      .includes('visibilityParity'),
  );
});
