import assert from 'node:assert/strict';

export function assertImpactInvariants(result, options) {
  assert.equal(result.scope, options.expectedScope);
  assert.equal(
    result.fullSceneRebuildRequired,
    result.scope === 'full_rebuild',
  );
  assert.ok(result.generatedGeometry <= options.maxGeneratedGeometry);
  assert.ok(Array.isArray(result.reasons));

  if (result.scope === 'local' || result.scope === 'targeted') {
    assert.equal(result.fullSceneRebuildRequired, false);
  }
  if (result.scope === 'local' && !result.reasons.includes('node_content')) {
    assert.ok(result.cacheHits > 0);
  }
  if (result.scope === 'unavailable') {
    assert.equal(result.candidateDigest, null);
  }
}
