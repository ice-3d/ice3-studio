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
import { createEquipmentMesh } from '../../fixtures/large-industrial-site/geometry.mjs';
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

test('equipment geometry follows process semantics instead of one generic column', () => {
  const reactor = meshBounds(createEquipmentMesh('biological-reactor', 1));
  const clarifier = meshBounds(createEquipmentMesh('secondary-structure', 1));
  const building = meshBounds(createEquipmentMesh('process-building', 1));
  const digester = meshBounds(createEquipmentMesh('digester', 1));

  assert.ok(reactor.width > 30, JSON.stringify(reactor));
  assert.ok(reactor.depth > 18, JSON.stringify(reactor));
  assert.ok(reactor.height < 10, JSON.stringify(reactor));

  assert.ok(clarifier.width > 20 && clarifier.depth > 20, JSON.stringify(clarifier));
  assert.ok(clarifier.height < 9, JSON.stringify(clarifier));

  assert.ok(building.width > 18 && building.depth > 12, JSON.stringify(building));
  assert.ok(building.height > 7, JSON.stringify(building));

  assert.ok(digester.height > digester.width, JSON.stringify(digester));
  assert.ok(digester.height > 14, JSON.stringify(digester));
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
  assert.equal(first.expected.primaryObjects, 208);
  assert.equal(
    operations.filter((operation) => operation.type === 'instance.create').length,
    208,
  );
  assert.equal(first.expected.facilityObjects, 10);
  assert.equal(
    operations.filter((operation) => operation.type === 'component.define').length,
    16,
  );
  assert.equal(
    operations.filter((operation) => operation.type === 'material.upsert').length,
    12,
  );
  assert.ok(
    operations.filter((operation) => (
      operation.type === 'node.create'
      && String(operation.metadata?.role ?? '').match(/ground|road|yard|landscape/)
    )).length >= 14,
  );
  assert.equal(operations.some((operation) => (
    ![
      'component.define',
      'instance.create',
      'instanceSet.create',
      'relation.create',
      'connection.create',
      'material.upsert',
      'material.assign',
      'node.create',
    ].includes(operation.type)
  )), false);
});

test('assigns readable material roles to every primary object', () => {
  const operations = buildIndustrialScenePlan().transactions
    .flatMap((transaction) => transaction.operations);
  const materials = new Map(
    operations
      .filter((operation) => operation.type === 'material.upsert')
      .map((operation) => [operation.material_id, operation.name]),
  );
  const instances = operations.filter((operation) => operation.type === 'instance.create');
  assert.equal(instances.length, 208);
  assert.equal(instances.every((operation) => materials.has(operation.material_id)), true);
  assert.equal(
    Object.values(
      instances.find((operation) => operation.name === 'Train 1 Aeration Basin').instance.overrides,
    ).some(({ materialId }) => materialId === 'material-000004'),
    true,
  );
});

test('keeps operation batches and source node references valid', () => {
  const plan = buildIndustrialScenePlan();
  const nodeIds = new Set();
  for (const transaction of plan.transactions) {
    assert.ok(transaction.operations.length <= 300);
    for (const operation of transaction.operations) {
      if (operation.type === 'node.create') {
        if (operation.parent_id !== null) {
          assert.ok(nodeIds.has(operation.parent_id));
        }
        nodeIds.add(operation.node_id);
      }
      if (operation.type === 'instance.create') {
        if (operation.parent_id !== null) {
          assert.ok(nodeIds.has(operation.parent_id));
        }
        const { x, y, z } = operation.transform.translation;
        assert.ok(x >= -600 && x <= 650);
        assert.ok(y >= 0 && y <= 100);
        assert.ok(z >= -280 && z <= 400);
        nodeIds.add(operation.node_id);
      }
    }
  }
  assert.equal(nodeIds.size, 235);
});

test('connects every process train without dangling ports', () => {
  const plan = buildIndustrialScenePlan();
  const topology = plan.expected.topology;
  assert.equal(topology.processTrains, 6);
  assert.equal(topology.danglingPorts, 0);
  assert.equal(topology.invalidMedia, 0);
  assert.equal(topology.connectionCount, 99);

  const instances = plan.transactions
    .flatMap((transaction) => transaction.operations)
    .filter((operation) => operation.type === 'instance.create')
  const instanceIds = new Set(instances.map((operation) => operation.node_id));
  const definitions = new Map(plan.componentDefinitions.map((definition) => [definition.id, definition]));
  const definitionByInstance = new Map(instances.map((instance) => [
    instance.node_id,
    definitions.get(instance.instance.definitionId),
  ]));
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
        const fromDefinition = definitionByInstance.get(operation.connection.from.nodeId);
        const toDefinition = definitionByInstance.get(operation.connection.to.nodeId);
        assert.ok(fromDefinition.ports.some(({ name }) => name === operation.connection.from.port));
        assert.ok(toDefinition.ports.some(({ name }) => name === operation.connection.to.port));
        assert.ok(['air', 'liquid'].includes(operation.connection.medium));
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
      effectiveTriangles: 500_000,
      uniqueMeshes: 1_201,
    },
    incremental: {
      cacheHits: 0,
      generatedMeshes: 400,
    },
    operationCount: 100_001,
  });
  assert.deepEqual(failures.sort(), [
    'agentContextEvidence',
    'effectiveTriangles',
    'incrementalCacheReuse',
    'operationCount',
    'uniqueMeshes',
  ]);
});

test('assigns and connects the full treatment train', () => {
  const plan = buildIndustrialScenePlan();
  const instances = plan.transactions
    .flatMap((transaction) => transaction.operations)
    .filter((operation) => operation.type === 'instance.create');
  const byId = new Map(instances.map((instance) => [instance.node_id, instance]));

  for (const train of plan.processTrains) {
    const reactors = instances.filter(({ name }) => name === `Train ${train} Aeration Basin`);
    const secondary = instances.filter(({ name }) => name === `Train ${train} Secondary Clarifier`);
    assert.equal(reactors.length, 1, `reactor count for train ${train}`);
    assert.equal(secondary.length, 1, `secondary count for train ${train}`);

    const trainConnections = plan.transactions
      .flatMap((transaction) => transaction.operations)
      .filter((operation) => operation.type === 'connection.create')
      .filter((operation) => Number(operation.metadata.train) === train);
    assert.ok(trainConnections.length >= 6, `connections for train ${train}`);
  }
});

function meshBounds(mesh) {
  const min = [Infinity, Infinity, Infinity];
  const max = [-Infinity, -Infinity, -Infinity];
  for (const position of mesh.positions) {
    for (let axis = 0; axis < 3; axis += 1) {
      min[axis] = Math.min(min[axis], position[axis]);
      max[axis] = Math.max(max[axis], position[axis]);
    }
  }
  return {
    width: max[0] - min[0],
    height: max[1] - min[1],
    depth: max[2] - min[2],
  };
}
