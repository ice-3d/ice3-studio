import {
  EQUIPMENT_GROUPS,
  FACILITIES,
  PROCESS_TRAINS,
} from './catalog.mjs';
import {
  AREA_BOUNDS,
  areaPosition,
  facilityPosition,
} from './layout.mjs';
import { buildComponentDefinitions } from './templates.mjs';
import { buildTopology } from './topology.mjs';

const AREA_BY_KIND = new Map([
  ['inlet-structure', 'intake'],
  ['bar-screen', 'intake'],
  ['lift-pump', 'intake'],
  ['grit-basin', 'intake'],
  ['primary-structure', 'primary'],
  ['distribution-well', 'primary'],
  ['biological-reactor', 'biological-a'],
  ['mixer', 'biological-b'],
  ['secondary-structure', 'secondary'],
  ['solids-pump', 'solids'],
  ['rotating-bridge', 'secondary'],
  ['filter-cell', 'advanced'],
  ['disinfection-basin', 'advanced'],
  ['blower-equipment', 'utilities'],
  ['chemical-tank', 'utilities'],
  ['dosing-pump', 'utilities'],
  ['thickener', 'solids'],
  ['digester', 'solids'],
  ['dewatering-unit', 'solids'],
  ['conveyor', 'solids'],
  ['analyzer', 'advanced'],
  ['valve', 'utilities'],
]);

const MATERIAL_ID = Object.freeze({
  concrete: 'material-000001',
  steel: 'material-000002',
  water: 'material-000003',
  sludge: 'material-000004',
  safety: 'material-000005',
  building: 'material-000006',
  roof: 'material-000007',
  road: 'material-000008',
  equipment: 'material-000009',
  process: 'material-000010',
});

const KIND_MATERIAL = new Map([
  ['inlet-structure', 'concrete'],
  ['bar-screen', 'steel'],
  ['lift-pump', 'equipment'],
  ['grit-basin', 'concrete'],
  ['primary-structure', 'concrete'],
  ['distribution-well', 'concrete'],
  ['biological-reactor', 'water'],
  ['mixer', 'steel'],
  ['secondary-structure', 'water'],
  ['solids-pump', 'steel'],
  ['rotating-bridge', 'safety'],
  ['filter-cell', 'water'],
  ['disinfection-basin', 'water'],
  ['blower-equipment', 'steel'],
  ['chemical-tank', 'safety'],
  ['dosing-pump', 'steel'],
  ['thickener', 'concrete'],
  ['digester', 'equipment'],
  ['dewatering-unit', 'building'],
  ['conveyor', 'safety'],
  ['analyzer', 'safety'],
  ['valve', 'steel'],
]);

export function buildIndustrialScenePlan() {
  const definitions = buildComponentDefinitions();
  const definitionIndex = new Map(definitions.map((definition, index) => [
    definition.name,
    index,
  ]));
  const operations = [
    ...materialOperations(),
    ...definitions.map((definition) => ({
      type: 'component.define',
      definition,
    })),
    ...siteOperations(),
  ];
  const areaTotals = new Map();
  for (const group of EQUIPMENT_GROUPS) {
    const area = AREA_BY_KIND.get(group.kind);
    areaTotals.set(area, (areaTotals.get(area) ?? 0) + group.count);
  }
  const areaOrdinals = new Map();
  const instances = [];
  let ordinal = 1;
  for (const group of EQUIPMENT_GROUPS) {
    const area = AREA_BY_KIND.get(group.kind);
    for (let index = 0; index < group.count; index += 1) {
      const areaOrdinal = areaOrdinals.get(area) ?? 0;
      const position = areaPosition(area, areaOrdinal, areaTotals.get(area));
      const train = group.kind === 'biological-reactor'
        ? PROCESS_TRAINS[Math.floor(index / 3)]
        : group.kind === 'secondary-structure'
          ? PROCESS_TRAINS[Math.floor(index / 2)]
          : null;
      const instanceMetadata = {
        area,
        kind: group.kind,
        ...(train === null ? {} : { train: String(train) }),
      };
      instances.push({
        type: 'instance.create',
        node_id: nodeId(ordinal),
        parent_id: null,
        name: `${group.kind}-${String(ordinal).padStart(4, '0')}`,
        transform: {
          translation: position,
          rotation: [0, 0, 0, 1],
          scale: { x: 1, y: 1, z: 1 },
        },
        instance: {
          definitionId: definitions[definitionIndex.get(group.kind)].id,
          parameters: {},
          overrides: {},
          metadata: instanceMetadata,
        },
        material_id: null,
        metadata: instanceMetadata,
      });
      instances[instances.length - 1].material_id =
        MATERIAL_ID[KIND_MATERIAL.get(group.kind) ?? 'process'];
      areaOrdinals.set(area, areaOrdinal + 1);
      ordinal += 1;
    }
  }
  for (let index = 0; index < FACILITIES.length; index += 1) {
    instances.push({
      type: 'instance.create',
      node_id: nodeId(ordinal),
      parent_id: null,
      name: FACILITIES[index],
      transform: {
        translation: facilityPosition(index),
        rotation: [0, 0, 0, 1],
        scale: { x: 1, y: 1, z: 1 },
      },
      instance: {
        definitionId: definitions[definitionIndex.get('process-building')].id,
        parameters: {},
        overrides: {},
        metadata: { area: 'utilities', kind: 'facility' },
        },
        material_id: MATERIAL_ID.building,
        metadata: { area: 'utilities', kind: 'facility' },
    });
    ordinal += 1;
  }
  const topology = buildTopology(instances);
  const baseOperations = [...operations, ...instances];
  const baseTransactions = chunkTransactions(baseOperations, 80);
  const topologyTransactions = chunkTopology(
    topology.pairs,
    baseTransactions.length,
  );
  return {
    projectId: 'large-industrial-site',
    transactions: [...baseTransactions, ...topologyTransactions],
    processTrains: PROCESS_TRAINS,
    expected: {
      primaryObjects: 216,
      facilityObjects: FACILITIES.length,
      primaryClarifiers: 4,
      processTrains: PROCESS_TRAINS.length,
      secondaryClarifiers: 12,
      minTriangles: 5_500_000,
      maxTriangles: 7_500_000,
      topology: topology.validation,
    },
  };
}

function materialOperations() {
  return [
    material('concrete', '#f2f2ed'),
    material('steel', '#dce7ec'),
    material('water', '#65d6ff'),
    material('sludge', '#d8b68e'),
    material('safety', '#ffe28a'),
    material('building', '#f4eee5'),
    material('roof', '#c7d5dc'),
    material('road', '#8e969c'),
    material('equipment', '#8ee8ff'),
    material('process', '#a8ffe8'),
  ];
}

function material(name, color) {
  return {
    type: 'material.upsert',
    material_id: MATERIAL_ID[name],
    name,
    base_color: hexColor(color),
    opacity: 1,
    edge_color: hexColor('#7b8f99'),
  };
}

function siteOperations() {
  const operations = [
    siteNode('node-002001', 'Site Ground', 'ground', 'plane', {
      width: 1040,
      depth: 590,
      x: 500,
      z: 275,
      y: -0.2,
      material: 'concrete',
    }),
  ];
  const eastWestRoads = [78, 150, 225, 300, 380, 475];
  for (const [index, z] of eastWestRoads.entries()) {
    operations.push(siteNode(
      `node-${String(2002 + index).padStart(6, '0')}`,
      `Service Road East-West ${index + 1}`,
      'road',
      'plane',
      { width: 960, depth: 7.5, x: 500, z, y: 0.05, material: 'road' },
    ));
  }
  const northSouthRoads = [115, 260, 405, 550, 695, 840];
  for (const [index, x] of northSouthRoads.entries()) {
    operations.push(siteNode(
      `node-${String(2008 + index).padStart(6, '0')}`,
      `Service Road North-South ${index + 1}`,
      'road',
      'plane',
      { width: 7, depth: 530, x, z: 285, y: 0.06, material: 'road' },
    ));
  }
  for (const [index, [area, [x, z, width, depth]]] of Object.entries(AREA_BOUNDS).entries()) {
    operations.push(siteNode(
      `node-${String(2014 + index).padStart(6, '0')}`,
      `${area} hardstanding`,
      'yard',
      'plane',
      {
        width: width * 0.94,
        depth: depth * 0.94,
        x: x + width / 2,
        z: z + depth / 2,
        y: 0.025,
        material: 'building',
      },
    ));
  }
  operations.push(
    siteNode('node-002022', 'Maintenance Parking', 'yard', 'plane', {
      width: 220,
      depth: 42,
      x: 260,
      z: 520,
      y: 0.07,
      material: 'road',
    }),
    siteNode('node-002023', 'Administration Forecourt', 'yard', 'plane', {
      width: 130,
      depth: 35,
      x: 710,
      z: 525,
      y: 0.08,
      material: 'building',
    }),
  );
  const perimeter = [
    [[15, 2.2, 15], [985, 2.2, 15]],
    [[15, 2.2, 535], [985, 2.2, 535]],
    [[15, 2.2, 15], [15, 2.2, 535]],
    [[985, 2.2, 15], [985, 2.2, 535]],
  ];
  for (const [index, points] of perimeter.entries()) {
    operations.push({
      type: 'node.create',
      node_id: `node-${String(2024 + index).padStart(6, '0')}`,
      parent_id: null,
      name: `Perimeter Fence ${index + 1}`,
      transform: identityTransform(),
      content: {
        type: 'sweep',
        points,
        radius: 0.12,
        radial_segments: 6,
      },
      material_id: MATERIAL_ID.steel,
      metadata: { role: 'fence' },
    });
  }
  return operations;
}

function siteNode(nodeId, name, role, kind, options) {
  return {
    type: 'node.create',
    node_id: nodeId,
    parent_id: null,
    name,
    transform: {
      translation: { x: options.x, y: options.y, z: options.z },
      rotation: [0, 0, 0, 1],
      scale: { x: 1, y: 1, z: 1 },
    },
    content: {
      type: 'primitive',
      kind,
      width: options.width,
      depth: options.depth,
    },
    material_id: MATERIAL_ID[options.material],
    metadata: { role },
  };
}

function identityTransform() {
  return {
    translation: { x: 0, y: 0, z: 0 },
    rotation: [0, 0, 0, 1],
    scale: { x: 1, y: 1, z: 1 },
  };
}

function hexColor(color) {
  return [
    parseInt(color.slice(1, 3), 16) / 255,
    parseInt(color.slice(3, 5), 16) / 255,
    parseInt(color.slice(5, 7), 16) / 255,
    1,
  ];
}

export function chunkTransactions(operations, size) {
  const transactions = [];
  for (let index = 0; index < operations.length; index += size) {
    transactions.push({
      expectedRevision: transactions.length,
      dryRun: false,
      idempotencyKey: `large-industrial-batch-${transactions.length + 1}`,
      operations: operations.slice(index, index + size),
    });
  }
  return transactions;
}

function chunkTopology(pairs, startRevision) {
  const transactions = [];
    const pairsPerTransaction = 25;
  for (let index = 0; index < pairs.length; index += pairsPerTransaction) {
    const slice = pairs.slice(index, index + pairsPerTransaction);
    transactions.push({
      expectedRevision: startRevision + transactions.length,
      dryRun: false,
      idempotencyKey: `large-industrial-topology-${transactions.length + 1}`,
        operations: slice.flatMap((pair) => [
          pair.relation,
          pair.connection,
          {
            type: 'material.assign',
            node_id: pair.connection.node_id,
            material_id: MATERIAL_ID.process,
          },
        ]),
    });
  }
  return transactions;
}

function nodeId(ordinal) {
  return `node-${String(ordinal).padStart(6, '0')}`;
}
