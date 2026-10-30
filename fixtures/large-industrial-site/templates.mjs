import { EQUIPMENT_GROUPS } from './catalog.mjs';
import { createEquipmentMesh } from './geometry.mjs';

export const TEMPLATE_TRIANGLE_BUDGETS = Object.freeze({
  'inlet-structure': 10_000,
  'bar-screen': 15_000,
  'lift-pump': 18_000,
  'grit-basin': 25_000,
  'primary-structure': 70_000,
  'distribution-well': 18_000,
  'biological-reactor': 55_000,
  mixer: 10_000,
  'secondary-structure': 75_000,
  'solids-pump': 18_000,
  'rotating-bridge': 35_000,
  'filter-cell': 30_000,
  'disinfection-basin': 28_000,
  'blower-equipment': 22_000,
  'chemical-tank': 20_000,
  'dosing-pump': 12_000,
  thickener: 45_000,
  digester: 75_000,
  'dewatering-unit': 40_000,
  conveyor: 24_000,
  analyzer: 8_000,
  valve: 8_000,
  'process-building': 25_000,
});

export function buildComponentDefinitions() {
  const kinds = [
    ...EQUIPMENT_GROUPS.map((group) => group.kind),
    'process-building',
  ];
  return kinds.map((kind, index) => componentDefinition(kind, index));
}

function componentDefinition(kind, index) {
  const ordinal = index + 1;
  const detail = createEquipmentMesh(kind, ordinal);
  return {
    id: `component-${String(ordinal).padStart(6, '0')}`,
    name: kind,
    parameters: [],
    parts: [
      {
        id: 'detail',
        parentId: null,
        transform: partTransform([0, 0, 0]),
        content: { kind: 'custom_mesh', mesh: detail },
        materialRole: null,
        metadata: {
          detailTriangleBudget: String(TEMPLATE_TRIANGLE_BUDGETS[kind]),
        },
        visible: true,
      },
      ...Array.from({ length: 7 }, (_, part) => ({
        id: `assembly-${part + 1}`,
        parentId: 'detail',
        transform: partTransform([(part - 3) * 0.35, 0.2 + part * 0.1, 0]),
        content: { kind: 'group' },
        materialRole: null,
        metadata: { assembly: String(part + 1) },
        visible: true,
      })),
    ],
    ports: [
      {
        name: 'process-in',
        partId: 'detail',
        position: { x: -3.25, y: 1.2, z: 0 },
        direction: { x: -1, y: 0, z: 0 },
        medium: 'process-fluid',
        diameter: 0.4,
      },
      {
        name: 'process-out',
        partId: 'detail',
        position: { x: 3.25, y: 1.2, z: 0 },
        direction: { x: 1, y: 0, z: 0 },
        medium: 'process-fluid',
        diameter: 0.4,
      },
    ],
    metadata: {
      kind,
      triangleBudget: String(TEMPLATE_TRIANGLE_BUDGETS[kind]),
    },
  };
}

function partTransform(translation) {
  return {
    translation: { kind: 'literal', value: {
      x: translation[0],
      y: translation[1],
      z: translation[2],
    } },
    rotation: [0, 0, 0, 1],
    scale: { kind: 'literal', value: { x: 1, y: 1, z: 1 } },
  };
}
