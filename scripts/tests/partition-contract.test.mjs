import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

test('partition parity proves edge, LOD, batching and budget semantics', async () => {
  const report = JSON.parse(await readFile(
    new URL('../../output/partition-parity.json', import.meta.url),
    'utf8',
  ));

  assert.equal(report.kind, 'ice3-partition-parity');
  assert.equal(report.status, 'pass');
  assert.deepEqual(report.failures, []);
  assert.ok(report.cases.every((entry) => entry.deterministic));
  assert.ok(report.cases.some((entry) => entry.edge.segmentCount > 0));
  assert.ok(report.cases.some((entry) => entry.edge === null));

  const names = new Set(report.cases.map((entry) => entry.name));
  for (const name of [
    'direct-glb-no-lod',
    'direct-no-lod',
    'proportional-lod',
    'adaptive-lod',
    'semantic-proxy-auto',
    'static-render-batch',
    'chunked-coarse',
    'skip-edges',
  ]) {
    assert.ok(names.has(name), `${name} is missing`);
  }

  const chunked = report.cases.find((entry) => entry.name === 'chunked-coarse');
  assert.ok(chunked.coarseCount >= 2);
  assert.ok(chunked.files.some((file) => file.path.startsWith('coarse/')));
  assert.ok(chunked.budget.withinBudget);

  const semantic = report.cases.find(
    (entry) => entry.name === 'semantic-proxy-auto',
  );
  assert.ok(semantic.semanticProxy);
  assert.ok(semantic.edge.segmentCount > 0);
});
