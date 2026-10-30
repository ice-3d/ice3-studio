import assert from 'node:assert/strict';
import test from 'node:test';
import {
  AREAS,
  EQUIPMENT_GROUPS,
  FACILITIES,
  PROCESS_TRAINS,
  validateCatalogue,
} from '../../fixtures/large-industrial-site/catalog.mjs';
import {
  TEMPLATE_TRIANGLE_BUDGETS,
  buildComponentDefinitions,
} from '../../fixtures/large-industrial-site/templates.mjs';
import { buildIndustrialScenePlan } from '../../fixtures/large-industrial-site/operations.mjs';
import { verifyAppliedScene } from '../../scripts/run-large-industrial-fixture.mjs';
import { evaluateBenchmarkGate } from '../../benchmarks/large-industrial-scene.mjs';

test('catalogue preserves the accepted large-scene scale', () => {
  const counts = validateCatalogue({
    areas: AREAS,
    equipmentGroups: EQUIPMENT_GROUPS,
    facilities: FACILITIES,
    processTrains: PROCESS_TRAINS,
  });
  assert.equal(counts.equipmentCount + counts.facilities, 216);
  assert.equal(counts.primaryClarifiers, 4);
  assert.equal(counts.biologicalReactors, 18);
  assert.equal(counts.secondaryClarifiers, 12);
  assert.equal(counts.areas, 8);
  assert.equal(counts.facilities, 12);
  assert.equal(counts.processTrains, 6);
});

test('catalogue keys are unique and industry-neutral at the API boundary', () => {
  const keys = EQUIPMENT_GROUPS.map((group) => group.kind);
  assert.equal(new Set(keys).size, keys.length);
  assert.equal(keys.some((key) => /wastewater|sludge|clarifier/i.test(key)), false);
});

test('every equipment kind has a reusable bounded template', () => {
  const generated = buildComponentDefinitions();
  assert.equal(generated.length, EQUIPMENT_GROUPS.length + 1);
  for (const group of EQUIPMENT_GROUPS) {
    const definition = generated.find((candidate) => candidate.name === group.kind);
    assert.ok(definition, group.kind);
    assert.equal(definition.parts.length >= 8, true);
    assert.equal(definition.ports.length >= 2, true);
    assert.ok(TEMPLATE_TRIANGLE_BUDGETS[group.kind] >= 8_000);
  }
  assert.ok(generated.find((candidate) => candidate.name === 'process-building'));
});

test('catalogue weighted template budget targets the large scene band', () => {
  const weighted = EQUIPMENT_GROUPS.reduce(
    (sum, group) => sum + group.count * TEMPLATE_TRIANGLE_BUDGETS[group.kind],
    0,
  );
  assert.ok(weighted >= 5_500_000, `weighted triangles: ${weighted}`);
  assert.ok(weighted <= 7_500_000, `weighted triangles: ${weighted}`);
});

test('builds a deterministic high-level operation plan', () => {
  const first = buildIndustrialScenePlan();
  const second = buildIndustrialScenePlan();
  assert.deepEqual(first, second);
  const operations = first.transactions.flatMap((transaction) => transaction.operations);
  assert.equal(first.expected.primaryObjects, 216);
  assert.equal(
    operations.filter((operation) => operation.type === 'instance.create').length,
    216,
  );
  assert.equal(first.expected.facilityObjects, 12);
  assert.equal(
    operations.filter((operation) => operation.type === 'component.define').length,
    23,
  );
  assert.equal(operations.some((operation) => (
    ![
      'component.define',
      'instance.create',
      'instanceSet.create',
      'relation.create',
      'connection.create',
    ].includes(operation.type)
  )), false);
});

test('keeps operation batches and source node references valid', () => {
  const plan = buildIndustrialScenePlan();
  const nodeIds = new Set();
  for (const transaction of plan.transactions) {
    assert.ok(transaction.operations.length <= 80);
    for (const operation of transaction.operations) {
      if (operation.type === 'instance.create') {
        if (operation.parent_id !== null) {
          assert.ok(nodeIds.has(operation.parent_id));
        }
        const { x, y, z } = operation.transform.translation;
        assert.ok(x >= 0 && x <= 1000);
        assert.ok(y >= 0 && y <= 100);
        assert.ok(z >= 0 && z <= 550);
        nodeIds.add(operation.node_id);
      }
    }
  }
  assert.equal(nodeIds.size, 216);
});

test('connects every process train without dangling ports', () => {
  const plan = buildIndustrialScenePlan();
  const topology = plan.expected.topology;
  assert.equal(topology.processTrains, 6);
  assert.equal(topology.danglingPorts, 0);
  assert.equal(topology.invalidMedia, 0);
  assert.ok(topology.connectionCount >= 120);

  const instanceIds = new Set(plan.transactions
    .flatMap((transaction) => transaction.operations)
    .filter((operation) => operation.type === 'instance.create')
    .map((operation) => operation.node_id));
  const relationIds = new Set();
  let connectionCount = 0;
  for (const transaction of plan.transactions) {
    for (const operation of transaction.operations) {
      if (operation.type === 'relation.create') {
        relationIds.add(operation.relation.id);
      }
      if (operation.type === 'connection.create') {
        connectionCount += 1;
        assert.ok(instanceIds.has(operation.connection.from.nodeId));
        assert.ok(instanceIds.has(operation.connection.to.nodeId));
        assert.equal(operation.connection.from.port, 'process-out');
        assert.equal(operation.connection.to.port, 'process-in');
        assert.equal(operation.connection.medium, 'process-fluid');
        assert.ok(relationIds.has(operation.connection.relationId));
      }
    }
  }
  assert.equal(connectionCount, topology.connectionCount);
});

test('rejects evaluated output outside the accepted scale', () => {
  assert.throws(() => verifyAppliedScene({
    sourceNodes: 216,
    expandedNodes: 4000,
    uniqueMeshes: 22,
    effectiveTriangles: 5_000_000,
    uniqueTriangles: 500_000,
    cacheHits: 0,
    cacheMisses: 22,
    generatedMeshes: 22,
  }, {
    primaryObjects: 216,
    minTriangles: 5_500_000,
    maxTriangles: 7_500_000,
  }));
});

test('benchmark gate rejects missing scale and cache reuse', () => {
  const failures = evaluateBenchmarkGate({
    stats: {
      effectiveTriangles: 5_000_000,
      uniqueMeshes: 400,
    },
    incremental: {
      cacheHits: 0,
      generatedMeshes: 400,
    },
    operationCount: 100_001,
  });
  assert.deepEqual(failures.sort(), [
    'effectiveTriangles',
    'incrementalCacheReuse',
    'operationCount',
    'uniqueMeshes',
  ]);
});
