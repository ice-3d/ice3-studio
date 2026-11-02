import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

test('native JT parity report passes every business gate', async () => {
  const report = JSON.parse(await readFile(
    new URL('../../output/jt-import-parity.json', import.meta.url),
    'utf8',
  ));

  assert.equal(report.kind, 'ice3-jt-import-parity');
  assert.equal(report.status, 'pass');
  assert.deepEqual(report.failures, []);
  assert.ok(report.deterministic.byteEqual);
  assert.equal(report.deterministic.firstDigest, report.deterministic.secondDigest);

  assert.equal(report.native.stats.nodeCount, report.baseline.stats.nodeCount);
  assert.equal(report.native.stats.meshCount, report.baseline.stats.meshCount);
  assert.equal(report.native.stats.triangleCount, report.baseline.stats.triangleCount);
  assert.equal(
    report.native.stats.expandedTriangleCount,
    report.baseline.stats.expandedTriangleCount,
  );
  assert.equal(report.native.stats.vertexCount, report.baseline.stats.vertexCount);
  assert.equal(
    report.native.stats.instanceNodeCount,
    report.baseline.stats.instanceNodeCount,
  );
  assert.deepEqual(report.native.selectionIds, report.baseline.selectionIds);
  assert.deepEqual(report.native.businessNodes, report.baseline.businessNodes);
  assert.deepEqual(report.native.materials, report.baseline.materials);
  assert.deepEqual(report.native.sourceOrigin, report.baseline.sourceOrigin);
  assert.deepEqual(report.native.bounds, report.baseline.bounds);
  assert.match(report.normalizationRule, /no geometric\s+difference is permitted/iu);
});

test('rejects the retired JT baseline capture mode', () => {
  const result = spawnSync(
    process.execPath,
    [fileURLToPath(new URL('../../scripts/compare-jt-imports.mjs', import.meta.url)), '--capture'],
    { encoding: 'utf8' },
  );

  assert.equal(result.status, 2);
  assert.match(result.stderr, /Baseline capture is retired/);
});
