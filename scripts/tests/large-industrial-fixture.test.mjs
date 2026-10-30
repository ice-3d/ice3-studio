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
