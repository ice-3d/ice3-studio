import assert from 'node:assert/strict';
import test from 'node:test';
import { assertImpactInvariants } from '../agent-iteration-impact-check.mjs';

test('accepts a cached local material or parameter iteration', () => {
  assert.doesNotThrow(() => assertImpactInvariants({
    scope: 'local',
    fullSceneRebuildRequired: false,
    generatedGeometry: 0,
    cacheHits: 64,
    reasons: ['instance_parameters'],
  }, {
    expectedScope: 'local',
    maxGeneratedGeometry: 0,
  }));
});

test('rejects a full rebuild reported as local', () => {
  assert.throws(() => assertImpactInvariants({
    scope: 'local',
    fullSceneRebuildRequired: true,
    generatedGeometry: 0,
    cacheHits: 64,
    reasons: ['instance_parameters'],
  }, {
    expectedScope: 'local',
    maxGeneratedGeometry: 0,
  }));
});
