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
  assert.equal(baseline.stats.vertexCount, 8);
  assert.equal(baseline.stats.instanceNodeCount, 1);
  assert.equal(baseline.legacyPrepared.status, 'pass');
  assert.equal(baseline.legacyPrepared.adapterId, 'jt');
  assert.equal(baseline.legacyPrepared.metrics.triangleCount, 24);
  assert.equal(baseline.legacyPrepared.output.nodeCount, 4);
  assert.equal(baseline.legacyPrepared.output.meshCount, 2);
  assert.deepEqual(
    baseline.legacyPrepared.output.selectionIds,
    baseline.selectionIds,
  );
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
    && instance.assemblyPath === '/Plant/TreatmentLine/PumpInstanceA'
    && instance.instanceGroup === 'jt/instance/0'
  )));
  const pump = baseline.nodeMetadata.find((node) => (
    node.objectId === 'JT-PUMP-101'
  ));
  assert.deepEqual(pump?.customProperties, { EquipmentTag: 'P-101' });
  assert.deepEqual(baseline.sourceOrigin, [-3, 0, -4]);
  assert.deepEqual(baseline.sourceBounds.min, [-3, 0, -4]);
  assert.deepEqual(baseline.sourceBounds.max, [3, 1, 1]);
  assert.deepEqual(baseline.rebasedBounds.min, [0, 0, 0]);
  assert.deepEqual(baseline.rebasedBounds.max, [6, 1, 5]);
  assert.deepEqual(baseline.materials, [
    { name: 'PumpBlue', baseColorFactor: [0.12, 0.38, 0.68, 1] },
    { name: 'TankTaupe', baseColorFactor: [0.62, 0.55, 0.4, 1] },
  ]);
});
