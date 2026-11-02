import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';

test('JT baseline pins business semantics', async () => {
  const baseline = JSON.parse(await readFile(
    new URL('../../fixtures/parity/jt-baseline-v1.json', import.meta.url),
    'utf8',
  ));
  assert.equal(baseline.kind, 'ice3-jt-import-baseline');
  assert.equal(baseline.stats.nodeCount, 4);
  assert.equal(baseline.stats.meshCount, 2);
  assert.equal(baseline.stats.triangleCount, 24);
  assert.equal(baseline.stats.expandedTriangleCount, 36);
  assert.equal(baseline.stats.instanceNodeCount, 1);
  assert.deepEqual(baseline.nodeNames, [
    'TreatmentLine',
    'PrimaryPump',
    'PumpInstanceA',
    'BufferTank',
  ].sort());
  assert.ok(baseline.selectionIds.every((id) => (
    id.startsWith('parity-jt/jt/node/')
  )));
  assert.ok(baseline.instances.some((instance) => (
    instance.objectId === 'JT-PUMP-102'
  )));
  assert.deepEqual(baseline.materials, [
    { name: 'PumpBlue', baseColorFactor: [0.12, 0.38, 0.68, 1] },
    { name: 'TankTaupe', baseColorFactor: [0.62, 0.55, 0.4, 1] },
  ]);
});
