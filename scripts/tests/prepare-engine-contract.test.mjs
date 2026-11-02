import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

test('Engine consumes Studio edges and Preparation metadata', async () => {
  const report = JSON.parse(await readFile(
    new URL('../../output/prepare-engine-contract.json', import.meta.url),
    'utf8',
  ));

  assert.equal(report.kind, 'ice3-prepare-engine-contract');
  assert.equal(report.status, 'pass');
  assert.deepEqual(report.failures, []);
  assert.equal(report.build.stats.edgeSegmentCount, 12);
  assert.equal(report.engine.edgeMaterialColor, '#e6f2ff');
  assert.equal(report.engine.edgeCount, 12);
  assert.equal(report.engine.selectionId, 'prepare-cube/node/0');
  assert.equal(report.runtimeTileset.loadedParts, report.runtimeTileset.partCount);
  assert.ok(report.runtimeTileset.edgeSegmentCount > 0);
  assert.ok(report.runtimeTileset.edgeMaterialColors.includes('#e6f2ff'));
});
