import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';

test('prepare baseline pins edges, metrics and budget', async () => {
  const baseline = JSON.parse(await readFile(
    new URL('../../fixtures/parity/prepare-baseline-v1.json', import.meta.url),
    'utf8',
  ));
  assert.equal(baseline.kind, 'ice3-prepare-baseline');
  assert.match(baseline.sourceFiles['prepare-cube.gltf'], /^[a-f0-9]{64}$/);
  assert.match(baseline.sourceFiles['prepare-cube.bin'], /^[a-f0-9]{64}$/);
  assert.equal(baseline.default.edgesPrepared, true);
  assert.equal(baseline.default.stats.triangleCount, 12);
  assert.equal(baseline.default.stats.edgeSegmentCount, 12);
  assert.equal(baseline.default.stats.materialCount, 5);
  assert.equal(baseline.default.edgeRole, 'edges');
  assert.equal(baseline.default.edgeWidthPx, 3);
  assert.equal(baseline.skipEdges.stats.edgeSegmentCount, 0);
  assert.equal(baseline.crease90.stats.edgeSegmentCount, 12);
  assert.equal(baseline.budget.profile, 'interactive-desktop-v1');
  assert.equal(baseline.budget.status, 'pass');
});
