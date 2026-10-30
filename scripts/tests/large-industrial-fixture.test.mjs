import assert from 'node:assert/strict';
import test from 'node:test';
import {
  AREAS,
  EQUIPMENT_GROUPS,
  FACILITIES,
  PROCESS_TRAINS,
  validateCatalogue,
} from '../../fixtures/large-industrial-site/catalog.mjs';

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
