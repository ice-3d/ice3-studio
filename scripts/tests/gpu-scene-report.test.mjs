import assert from 'node:assert/strict';
import test from 'node:test';

import { evaluateBenchmarkGate } from '../../benchmarks/large-industrial-scene.mjs';

test('GPU scene report fails closed when the scene session failed', () => {
  const result = {
    computeMode: 'gpu',
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
    },
    compute: {
      sceneSession: {
        state: 'failed',
      },
    },
  };

  assert.deepEqual(evaluateBenchmarkGate(result), ['gpuSceneState']);
});
