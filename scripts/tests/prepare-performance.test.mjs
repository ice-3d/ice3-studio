import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

test('prepare performance remains inside the accepted envelope', async () => {
  const baseline = JSON.parse(await readFile(
    new URL('../../fixtures/parity/prepare-performance-v1.json', import.meta.url),
    'utf8',
  ));

  assert.equal(baseline.kind, 'ice3-prepare-performance-baseline');
  assert.equal(baseline.schemaVersion, '1.0');
  assert.ok(baseline.fixture.sourceGlbSha256);
  assert.equal(baseline.cases.length, 5);
  for (const entry of baseline.cases) {
    assert.ok(entry.p95Ms > 0);
    assert.ok(entry.peakMemoryBytes > 0);
    assert.ok(entry.peakMemoryBytes <= 536_870_912);
    assert.ok(entry.outputBytes > 0);
    if (entry.name === 'build-skip-edges') {
      assert.equal(entry.edgeSegmentCount, 0);
    } else {
      assert.ok(entry.edgeSegmentCount > 0);
    }
    assert.ok(entry.legacyP95Ms > 0);
    assert.ok(entry.p95Ms <= entry.legacyP95Ms * 2);
    assert.ok(entry.digestStable);
  }
});
