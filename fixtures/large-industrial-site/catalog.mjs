export const AREAS = Object.freeze([
  'intake',
  'primary',
  'biological-a',
  'biological-b',
  'secondary',
  'advanced',
  'solids',
  'utilities',
]);

export const PROCESS_TRAINS = Object.freeze([101, 201, 301, 401, 501, 601]);

export const EQUIPMENT_GROUPS = Object.freeze([
  ['inlet-structure', 4],
  ['bar-screen', 8],
  ['lift-pump', 10],
  ['grit-basin', 4],
  ['primary-structure', 4],
  ['distribution-well', 4],
  ['biological-reactor', 18],
  ['mixer', 12],
  ['secondary-structure', 12],
  ['solids-pump', 16],
  ['rotating-bridge', 12],
  ['filter-cell', 12],
  ['disinfection-basin', 4],
  ['blower-equipment', 8],
  ['chemical-tank', 12],
  ['dosing-pump', 12],
  ['thickener', 4],
  ['digester', 6],
  ['dewatering-unit', 6],
  ['conveyor', 6],
  ['analyzer', 12],
  ['valve', 18],
].map(([kind, count]) => Object.freeze({ kind, count })));

export const FACILITIES = Object.freeze([
  'process-building',
  'blower-building',
  'control-building',
  'maintenance-building',
  'solids-building',
  'chemical-building',
  'odor-control',
  'power-building',
  'laboratory',
  'warehouse',
  'gatehouse',
  'utility-building',
]);

export const EXPECTED_SCENE = Object.freeze({
  primaryObjects: 216,
  primaryClarifiers: 4,
  biologicalReactors: 18,
  processTrains: 6,
  secondaryClarifiers: 12,
  areas: 8,
  facilities: 12,
  minTriangles: 5_500_000,
  maxTriangles: 7_500_000,
});

export function validateCatalogue({
  areas,
  equipmentGroups,
  facilities,
  processTrains,
}) {
  const equipmentCount = equipmentGroups.reduce(
    (sum, group) => sum + group.count,
    0,
  );
  const biologicalReactors = equipmentGroups
    .filter((group) => group.kind === 'biological-reactor')
    .reduce((sum, group) => sum + group.count, 0);
  return {
    equipmentCount,
    primaryClarifiers: equipmentGroups
      .filter((group) => group.kind === 'primary-structure')
      .reduce((sum, group) => sum + group.count, 0),
    biologicalReactors,
    secondaryClarifiers: equipmentGroups
      .filter((group) => group.kind === 'secondary-structure')
      .reduce((sum, group) => sum + group.count, 0),
    facilities: facilities.length,
    areas: areas.length,
    processTrains: processTrains.length,
  };
}
