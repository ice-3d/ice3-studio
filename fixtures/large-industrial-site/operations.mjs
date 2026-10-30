import {
  EQUIPMENT_GROUPS,
  FACILITIES,
  PROCESS_TRAINS,
} from './catalog.mjs';
import { areaPosition, facilityPosition } from './layout.mjs';
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

export function buildIndustrialScenePlan() {
  const definitions = buildComponentDefinitions();
  const definitionIndex = new Map(definitions.map((definition, index) => [
    definition.name,
    index,
  ]));
  const operations = definitions.map((definition) => ({
    type: 'component.define',
    definition,
  }));
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
          metadata: { area, kind: group.kind },
        },
        material_id: null,
        metadata: { area, kind: group.kind },
      });
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
      material_id: null,
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
  const pairsPerTransaction = 40;
  for (let index = 0; index < pairs.length; index += pairsPerTransaction) {
    const slice = pairs.slice(index, index + pairsPerTransaction);
    transactions.push({
      expectedRevision: startRevision + transactions.length,
      dryRun: false,
      idempotencyKey: `large-industrial-topology-${transactions.length + 1}`,
      operations: slice.flatMap((pair) => [pair.relation, pair.connection]),
    });
  }
  return transactions;
}

function nodeId(ordinal) {
  return `node-${String(ordinal).padStart(6, '0')}`;
}
