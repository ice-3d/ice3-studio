import { AREAS, PROCESS_TRAINS } from './catalog.mjs';

export function buildTopology(instances) {
  const byArea = new Map(AREAS.map((area) => [area, []]));
  for (const instance of instances) {
    if (instance.metadata.kind === 'facility') continue;
    byArea.get(instance.metadata.area).push(instance);
  }
  for (const areaInstances of byArea.values()) {
    areaInstances.sort((left, right) => left.node_id.localeCompare(right.node_id));
  }

  const links = [];
  for (const area of AREAS) {
    if (area === 'biological-a' || area === 'secondary') continue;
    const areaInstances = byArea.get(area);
    for (let index = 0; index < areaInstances.length - 1; index += 1) {
      links.push([
        areaInstances[index].node_id,
        areaInstances[index + 1].node_id,
        area,
        area,
        null,
      ]);
    }
  }

  const distribution = instances.find(
    (instance) => instance.metadata.kind === 'distribution-well',
  );
  const advanced = instances.find(
    (instance) => instance.metadata.kind === 'filter-cell',
  );
  for (const train of PROCESS_TRAINS) {
    const reactors = instances.filter((instance) => (
      instance.metadata.kind === 'biological-reactor'
      && Number(instance.metadata.train) === train
    ));
    const secondary = instances.filter((instance) => (
      instance.metadata.kind === 'secondary-structure'
      && Number(instance.metadata.train) === train
    ));
    if (!distribution || reactors.length !== 3 || secondary.length !== 2) {
      throw new Error(`ICE3_LARGE_SCENE_INVALID: incomplete process train ${train}`);
    }
    links.push(
      [distribution.node_id, reactors[0].node_id, 'primary', 'biological-a', train],
      [reactors[0].node_id, reactors[1].node_id, 'biological-a', 'biological-a', train],
      [reactors[1].node_id, reactors[2].node_id, 'biological-a', 'biological-a', train],
      [reactors[2].node_id, secondary[0].node_id, 'biological-a', 'secondary', train],
      [reactors[2].node_id, secondary[1].node_id, 'biological-a', 'secondary', train],
      [secondary[0].node_id, secondary[1].node_id, 'secondary', 'secondary', train],
    );
    if (advanced) {
      links.push([
        secondary[1].node_id,
        advanced.node_id,
        'secondary',
        'advanced',
        train,
      ]);
    }
  }

  const facilities = instances.filter((instance) => instance.metadata.kind === 'facility');
  const utilitySource = byArea.get('utilities')[0]?.node_id;
  if (utilitySource) {
    for (const facility of facilities) {
      links.push([utilitySource, facility.node_id, 'utilities', 'utilities', null]);
    }
  }

  const pairs = links.map(([fromNode, toNode, fromArea, toArea, train], index) => {
    const ordinal = 1001 + index;
    const relation = {
      type: 'relation.create',
      relation: {
        id: relationId(ordinal),
        relationType: 'process',
        fromNodeId: fromNode,
        toNodeId: toNode,
        metadata: {
          medium: 'process-fluid',
          fromArea,
          toArea,
          ...(train === null ? {} : { train: String(train) }),
        },
      },
    };
    const connection = {
      type: 'connection.create',
      node_id: nodeId(ordinal),
      parent_id: null,
      name: `process-connection-${String(ordinal).padStart(4, '0')}`,
      transform: {
        translation: { x: 0, y: 0, z: 0 },
        rotation: [0, 0, 0, 1],
        scale: { x: 1, y: 1, z: 1 },
      },
      connection: {
        from: { kind: 'instance', nodeId: fromNode, port: 'process-out' },
        to: { kind: 'instance', nodeId: toNode, port: 'process-in' },
        medium: 'process-fluid',
        diameter: 0.4,
        routePolicy: {
          kind: 'rack',
          elevation: 5.5 + index % 4 * 0.2,
        },
        relationId: relationId(ordinal),
        metadata: { fromArea, toArea },
      },
      metadata: train === null ? {} : { train: String(train) },
    };
    return { relation, connection };
  });

  return {
    pairs,
    validation: {
      processTrains: PROCESS_TRAINS.length,
      danglingPorts: 0,
      invalidMedia: 0,
      connectionCount: pairs.length,
    },
  };
}

function nodeId(ordinal) {
  return `node-${String(ordinal).padStart(6, '0')}`;
}

function relationId(ordinal) {
  return `relation-${String(ordinal).padStart(6, '0')}`;
}
