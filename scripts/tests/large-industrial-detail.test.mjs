import assert from 'node:assert/strict';
import test from 'node:test';
import { buildIndustrialScenePlan } from '../../fixtures/large-industrial-site/operations.mjs';

test('large industrial fixture preserves assembled process detail', () => {
  const plan = buildIndustrialScenePlan();
  assert.equal(plan.expected.primaryObjects, 208);
  assert.ok(plan.expected.componentDefinitions >= 16);
  assert.ok(plan.expected.semanticParts >= 900);

  const definitions = plan.componentDefinitions;
  const clarifier = definitions.find(({ name }) => (
    name === 'Secondary Circular Clarifier'
  ));
  const reactor = definitions.find(({ name }) => name === 'Aeration Basin');
  const building = definitions.find(({ name }) => name === 'Operations Building');
  assert.ok(clarifier);
  assert.ok(reactor);
  assert.ok(building);

  const clarifierPartIds = new Set(clarifier.parts.map(({ id }) => id));
  assert.ok(clarifierPartIds.has('center-well'));
  assert.ok(clarifierPartIds.has('bridge'));
  assert.ok(clarifierPartIds.has('scraper-1'));
  assert.ok([...clarifierPartIds].some((id) => id.includes('rail')));
  assert.ok([...clarifierPartIds].some((id) => id.includes('stair')));

  const reactorPartIds = new Set(reactor.parts.map(({ id }) => id));
  assert.ok(reactorPartIds.has('basin'));
  assert.ok(reactorPartIds.has('water'));
  assert.ok(reactorPartIds.has('air-header-1'));
  assert.ok(reactorPartIds.has('diffuser-1'));

  const buildingPartIds = new Set(building.parts.map(({ id }) => id));
  assert.ok(buildingPartIds.has('roof'));
  assert.ok([...buildingPartIds].some((id) => id.includes('window')));
});

test('detailed fixture is deterministic and assigns materials per assembly role', () => {
  const first = buildIndustrialScenePlan();
  const second = buildIndustrialScenePlan();
  assert.deepEqual(first, second);

  const operations = first.transactions.flatMap(({ operations: values }) => values);
  const instances = operations.filter(({ type }) => type === 'instance.create');
  assert.equal(instances.length, first.expected.primaryObjects);
  assert.ok(instances.every(({ instance }) => (
    Object.values(instance.overrides).some(({ materialId }) => materialId)
  )));

  const materialNames = new Set(
    operations
      .filter(({ type }) => type === 'material.upsert')
      .map(({ name }) => name),
  );
  assert.ok(materialNames.has('Process Water'));
  assert.ok(materialNames.has('Safety Yellow'));
  assert.ok(materialNames.has('Glazing'));
});
