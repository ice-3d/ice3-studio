import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import test from 'node:test';
import { buildPlan as buildLegacyPlan } from '../../scripts/build-wastewater-model.mjs';
import {
  createBuilder,
  detailFlange,
  detailGuardrail,
  detailStair,
  detailSupport,
  detailValve,
} from '../../scripts/wastewater-plant/model-kit.mjs';
import {
  WWTP_STATS_LIMITS,
  assertProjectStats,
  validateConnectionPorts,
  validateProcessGraph,
  validateProjectStats,
  validateWastewaterPlan,
} from '../../scripts/wastewater-plant/validate.mjs';

const digest = (value) => createHash('sha256')
  .update(JSON.stringify(value))
  .digest('hex');

test('command wrapper and site module produce the same deterministic plan', async () => {
  const module = await import('../../scripts/wastewater-plant/site-plan.mjs');
  assert.equal(
    digest(module.buildPlan('wastewater-plant')),
    digest(buildLegacyPlan('wastewater-plant')),
  );
});

test('current plan has valid connections and unique node ids', () => {
  const result = validateWastewaterPlan(buildLegacyPlan('wastewater-plant'));
  assert.deepEqual(result.issues, []);
  assert.equal(result.ok, true);
});

test('validation rejects an unknown connection port', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const connection = plan.operations.find(({ type }) => type === 'connection.create');
  connection.connection.from.port = 'missing-port';
  const result = validateWastewaterPlan(plan);
  assert.equal(result.ok, false);
  assert.ok(result.issues.some(({ code }) => code === 'UNKNOWN_PORT'));
});

function processFixture() {
  return {
    operations: [
      {
        type: 'instance.create',
        node_id: 'node-a',
        metadata: { tag: 'A' },
      },
      {
        type: 'instance.create',
        node_id: 'node-b',
        metadata: { tag: 'B' },
      },
      {
        type: 'connection.create',
        name: 'A → B',
        connection: {
          from: { nodeId: 'node-a', port: 'outlet' },
          to: { nodeId: 'node-b', port: 'inlet' },
          medium: 'return-sludge',
          metadata: { lineId: 'L-001' },
        },
      },
    ],
  };
}

test('process graph validation requires reachable media paths', () => {
  const plan = processFixture();
  const requirement = {
    code: 'MISSING_RAS',
    fromTags: ['A'],
    toTags: ['B'],
    media: ['return-sludge'],
  };
  assert.equal(validateProcessGraph(plan, [requirement]).ok, true);
  plan.operations[2].connection.medium = 'waste-sludge';
  const result = validateProcessGraph(plan, [requirement]);
  assert.equal(result.ok, false);
  assert.equal(result.issues[0].code, 'MISSING_RAS');
  assert.match(result.issues[0].message, /A.*B/);
});

test('project budget accepts the target band and rejects overflow', () => {
  assert.equal(validateProjectStats({
    effectiveTriangles: 300000,
    uniqueTriangles: 150000,
    expandedNodes: 15000,
    instances: 500,
  }, WWTP_STATS_LIMITS).ok, true);

  assert.equal(validateProjectStats({
    effectiveTriangles: 500001,
    uniqueTriangles: 150001,
    expandedNodes: 15001,
    instances: 501,
  }, WWTP_STATS_LIMITS).ok, false);
});

test('engineering detail kit emits reusable fittings and access structures', () => {
  const b = createBuilder();
  const parts = [
    ...detailFlange(b, 'flange-a', {
      diameter: 0.8,
      position: { x: 0, y: 3, z: 0 },
      rotation: [0, 0, 0, 1],
      material: 'steel',
    }),
    ...detailValve(b, 'valve-a', {
      diameter: 0.8,
      position: { x: 1, y: 3, z: 0 },
      rotation: [0, 0, 0, 1],
      material: 'pipe-air',
    }),
    ...detailStair(b, 'stair-a', {
      x: 2, y: 0, z: 0, width: 1.2, height: 4, yaw: 0,
    }),
    ...detailGuardrail(b, 'rail-a', {
      x: 2, y: 4, z: 0, length: 8, yaw: 0,
    }),
    ...detailSupport(b, 'support-a', {
      x: 10, y: 0, z: 0, height: 4.6, width: 3.8,
    }),
  ];
  const ids = parts.map(({ id }) => id);
  assert.equal(new Set(ids).size, ids.length);
  assert.ok(ids.some((id) => id.startsWith('flange-a')));
  assert.ok(ids.some((id) => id.startsWith('valve-a')));
  assert.ok(ids.some((id) => id.startsWith('stair-a')));
  assert.ok(ids.some((id) => id.startsWith('rail-a')));
  assert.ok(ids.some((id) => id.startsWith('support-a')));
});

test('circular clarifier exposes credible civil and mechanical detail', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const definition = plan.operations
    .filter(({ type }) => type === 'component.define')
    .find(({ definition }) => definition.name === 'Secondary clarifier').definition;
  const ids = new Set(definition.parts.map(({ id }) => id));

  for (const expected of [
    'sc-wall', 'sc-water', 'sc-inlet-well', 'sc-weir', 'sc-coping',
    'sc-deck', 'sc-rail', 'sc-ladder', 'sc-bridge-top', 'sc-drive',
    'sc-drive-gearbox', 'sc-drive-motor',
    'sc-waterline', 'sc-scum-band', 'sc-inlet-sleeve', 'sc-outlet-sleeve',
  ]) {
    assert.ok(ids.has(expected), expected);
  }
  assert.ok(definition.parts.length >= 30);
  assert.equal(definition.parts.find(({ id }) => id === 'sc-wall').content.kind, 'custom_mesh');
  assert.equal(definition.parts.find(({ id }) => id === 'sc-waterline').content.kind, 'custom_mesh');
  for (const part of definition.parts.filter(({ content }) => content.kind === 'custom_mesh')) {
    assert.ok(part.content.mesh.positions.every((position) => (
      Array.isArray(position) && position.length === 3
    )), part.id);
    assert.equal(part.content.mesh.indices.length % 3, 0, part.id);
  }
});

test('aeration basin has lanes, baffles, air distribution and guardrails', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const definition = plan.operations
    .filter(({ type }) => type === 'component.define')
    .find(({ definition }) => definition.name === 'Aerobic basin').definition;
  const ids = new Set(definition.parts.map(({ id }) => id));

  assert.ok(ids.has('ae-water'));
  assert.ok(ids.has('ae-foam-band'));
  assert.ok([...ids].some((id) => id.startsWith('ae-baffle-')));
  assert.ok([...ids].some((id) => id.startsWith('ae-air-')));
  assert.ok([...ids].some((id) => id.startsWith('ae-rail-')));
  assert.ok(definition.parts.length >= 35);
  assert.ok([...ids].filter((id) => id.startsWith('ae-air-diffuser-')).length >= 90);
});

test('biological basins expose process-correct equipment for each zone', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const definitions = plan.operations
    .filter(({ type }) => type === 'component.define')
    .map(({ definition }) => definition);
  const anaerobic = definitions.find(({ name }) => name === 'Anaerobic basin');
  const anoxic = definitions.find(({ name }) => name === 'Anoxic basin');
  const aerobic = definitions.find(({ name }) => name === 'Aerobic basin');
  assert.ok(anaerobic);
  assert.ok(anoxic);
  assert.ok(aerobic);
  for (const definition of [anaerobic, anoxic]) {
    assert.equal(
      definition.parts.some(({ id }) => id.includes('air-diffuser')),
      false,
    );
    assert.equal(
      definition.parts.some(({ id }) => id.includes('mixer')),
      true,
    );
    assert.equal(
      definition.parts.filter(({ id }) => id.includes('mixer-draft-tube')).length,
      3,
    );
    assert.equal(
      definition.parts.filter(({ id }) => id.includes('mixer-access-post')).length,
      8,
    );
    const draftTube = definition.parts.find(({ id }) => id.includes('mixer-draft-tube'));
    assert.equal(draftTube.content.segments, 30);
  }
  assert.equal(
    aerobic.parts.some(({ id }) => id.includes('air-diffuser')),
    true,
  );
  assert.equal(
    aerobic.parts.some(({ id }) => id.includes('mixer')),
    false,
  );
});

test('asset exposes complete sludge, RAS, split, gas and cake topology', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const connections = plan.operations.filter(({ type }) => (
    type === 'connection.create'
  ));
  const media = (from, to) => connections.some(({ connection }) => (
    connection.metadata?.service?.includes(from) &&
    connection.metadata?.service?.includes(to)
  ));

  for (const train of ['1', '2']) {
    assert.ok(media(`PC-${train}01`, 'SCW-101'));
    assert.ok(media(`AE-${train}03`, `DD-${train}01`));
    for (let index = 1; index <= 3; index += 1) {
      assert.ok(media(`DD-${train}01`, `SC-${train}0${index}`));
      assert.ok(media(`SC-${train}0${index}`, 'RWS-101'));
    }
    assert.ok(media('RWS-101', `AE-${train}02`));
  }
  assert.ok(media('DG-101', 'GCL-101'));
  assert.ok(media('DG-102', 'GCL-101'));
  assert.ok(media('GCL-101', 'GH-101'));
  assert.ok(media('GH-101', 'GF-101'));
  assert.equal(
    connections.some(({ connection }) => (
      connection.medium === 'dewatered-sludge'
    )),
    false,
  );
  assert.ok(plan.operations.some(({ type, relation }) => (
    type === 'relation.create' &&
    relation.relationType === 'material-handling'
  )));
});

test('every fluid medium has explicit material and one-purpose ports', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const connections = plan.operations.filter(({ type }) => (
    type === 'connection.create'
  ));
  const media = new Set(connections.map(({ connection }) => connection.medium));
  for (const medium of [
    'primary-sludge',
    'waste-sludge',
    'return-sludge',
    'thickened-sludge',
    'digested-sludge',
    'biogas',
    'chemical',
    'final-effluent',
  ]) {
    assert.ok(media.has(medium), medium);
  }
  const materialNames = new Map(
    plan.operations
      .filter(({ type }) => type === 'material.upsert')
      .map(({ material_id, name }) => [material_id, name]),
  );
  for (const operation of connections) {
    assert.ok(
      materialNames.has(operation.material_id),
      operation.connection.metadata?.service,
    );
  }
  const expectedMaterials = {
    'primary-sludge': 'Pipe / primary sludge',
    'return-sludge': 'Pipe / return sludge',
    'waste-sludge': 'Pipe / sludge',
    'thickened-sludge': 'Pipe / sludge',
    'digested-sludge': 'Pipe / sludge',
    biogas: 'Pipe / biogas',
    chemical: 'Pipe / chemical',
    'final-effluent': 'Pipe / clean water',
  };
  for (const [medium, expected] of Object.entries(expectedMaterials)) {
    const operation = connections.find(({ connection }) => (
      connection.medium === medium
    ));
    assert.ok(operation, medium);
    assert.equal(materialNames.get(operation.material_id), expected, medium);
  }
  const result = validateConnectionPorts(plan);
  assert.equal(result.ok, true);
  assert.deepEqual(result.issues, []);
});

test('pipe materials use functional yellow, green, blue and red groups', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const materials = new Map(
    plan.operations
      .filter(({ type }) => type === 'material.upsert')
      .map(({ name, base_color }) => [name, base_color.slice(0, 3)]),
  );
  const groups = {
    yellow: [
      'Pipe / raw water',
      'Pipe / process water',
    ],
    green: ['Pipe / clean water'],
    blue: ['Pipe / air', 'Pipe / biogas'],
    red: [
      'Pipe / sludge',
      'Pipe / primary sludge',
      'Pipe / return sludge',
    ],
  };
  for (const name of groups.yellow) {
    const [r, g, b] = materials.get(name);
    assert.ok(r > b * 1.8 && g > b * 1.5, name);
  }
  for (const name of groups.green) {
    const [r, g, b] = materials.get(name);
    assert.ok(g > r * 1.25 && g > b * 1.05, name);
  }
  for (const name of groups.blue) {
    const [r, g, b] = materials.get(name);
    assert.ok(b > r * 1.6 && b > g * 1.15, name);
  }
  for (const name of groups.red) {
    const [r, g, b] = materials.get(name);
    assert.ok(r > g * 1.7 && r > b * 1.5, name);
  }
  const chemical = materials.get('Pipe / chemical');
  assert.ok(chemical[0] > chemical[1] * 1.3);
  assert.ok(chemical[2] > chemical[1] * 1.2);
});

test('port validation rejects reuse of a normal process port', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const connection = structuredClone(
    plan.operations.find(({ type }) => type === 'connection.create'),
  );
  plan.operations.push(connection);
  const result = validateConnectionPorts(plan);
  assert.equal(result.ok, false);
  assert.ok(result.issues.some(({ code }) => code === 'PORT_REUSED'));
});

test('port validation rejects a pipe diameter that does not match its nozzle', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const connection = plan.operations.find(({ type }) => type === 'connection.create');
  connection.connection.diameter = 20;
  const result = validateConnectionPorts(plan);
  assert.equal(result.ok, false);
  assert.ok(result.issues.some(({ code }) => code === 'PIPE_DIAMETER_MISMATCH'));
});

test('major equipment has identity-bearing silhouettes and connecting ports', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const definitions = plan.operations
    .filter(({ type }) => type === 'component.define')
    .map(({ definition }) => definition);
  const byName = new Map(definitions.map((definition) => [definition.name, definition]));

  const pump = byName.get('Lift pump skid');
  const blower = byName.get('Blower skid');
  const dosing = byName.get('Chemical dosing skid');
  const dewatering = byName.get('Dewatering skid');
  const heatExchanger = byName.get('Sludge heat exchanger');

  assert.ok(pump);
  assert.ok(blower);
  assert.ok(dosing);
  assert.ok(dewatering);
  assert.ok(heatExchanger);
  assert.ok(pump.parts.some(({ id }) => id.includes('pump-body')));
  assert.ok(blower.parts.some(({ id }) => id.includes('blower-volute')));
  assert.ok(dosing.parts.some(({ id }) => id.includes('dosing-tank')));
  assert.ok(dewatering.parts.some(({ id }) => id.includes('centrifuge-bowl')));
  assert.ok(heatExchanger.parts.some(({ id }) => id.includes('shell')));
  assert.ok(pump.ports.length >= 2);
  assert.ok(blower.ports.length >= 2);
});

test('site plan uses readable names and complete service routes', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const instances = plan.operations.filter(({ type }) => type === 'instance.create');
  const siteNodes = plan.operations.filter(({ type }) => type === 'node.create');
  const names = instances.map(({ name }) => name).concat(siteNodes.map(({ name }) => name));

  assert.equal(names.some((name) => /^标线 z=/.test(name)), false);
  assert.ok(names.includes('南侧道路标线'));
  assert.ok(names.includes('北侧设备装卸场地'));
  assert.ok(names.includes('生物池检修通道'));
  assert.ok(instances.some(({ metadata }) => metadata.stage === 'sludge'));
  assert.equal(instances.some(({ metadata }) => !metadata.stage), false);
});

test('parallel treatment trains are related without being exact copies', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const instances = plan.operations.filter(({ type }) => type === 'instance.create');
  const trainA = instances.filter(({ metadata }) => metadata.train === 'Train1');
  const trainB = instances.filter(({ metadata }) => metadata.train === 'Train2');
  assert.equal(trainA.length, trainB.length);
  assert.notDeepEqual(
    trainA.map(({ transform }) => transform.translation),
    trainB.map(({ transform }) => transform.translation),
  );
});

test('clearance validation rejects overlapping process footprints', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const primary = plan.operations.find(({ name }) => name === 'PC-101 初沉池');
  const grit = plan.operations.find(({ name }) => name === 'GC-101 曝气沉砂池');
  grit.transform.translation = { ...primary.transform.translation };
  const result = validateWastewaterPlan(plan);
  assert.equal(result.ok, false);
  assert.ok(result.issues.some(({ code }) => code === 'CLEARANCE_VIOLATION'));
});

test('every visible process line has endpoint fittings and metadata', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const connections = plan.operations.filter(({ type }) => type === 'connection.create');
  const accessories = plan.operations.filter(({ type, metadata }) => (
    type === 'node.create' && String(metadata?.role ?? '').startsWith('pipe-')
  ));
  const labels = accessories.filter(({ metadata }) => metadata.role === 'pipe-label');
  const flanges = accessories.filter(({ metadata }) => metadata.role === 'pipe-flange');
  assert.ok(connections.length >= 31);
  assert.ok(accessories.length >= connections.length * 2);
  assert.equal(labels.length, 0);
  assert.ok(flanges.length >= connections.length * 2);
  assert.ok(flanges.every(({ content }) => content.kind === 'cylinder'));
  assert.ok(connections.every(({ connection }) => connection.metadata.lineId));
  assert.ok(connections.every(({ connection }) => (
    ['forward', 'reverse'].includes(connection.metadata.flow)
  )));
});

test('pipeline validation rejects a missing accessory line reference', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const accessory = plan.operations.find(({ type, metadata }) => (
    type === 'node.create' && metadata?.role === 'pipe-flange'
  ));
  accessory.metadata.lineId = 'missing-line';
  const result = validateWastewaterPlan(plan);
  assert.equal(result.ok, false);
  assert.ok(result.issues.some(({ code }) => code === 'UNKNOWN_LINE'));
});

test('materials separate civil, metal, water and site surfaces', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const names = new Set(
    plan.operations
      .filter(({ type }) => type === 'material.upsert')
      .map(({ name }) => name),
  );
  for (const expected of [
    'New concrete', 'Weathered concrete', 'Wet concrete',
    'Dark steel', 'Stainless steel', 'Light metal roof',
    'Anoxic water', 'Aerobic water', 'Clarified water', 'Sludge water',
    'Aeration foam', 'Clarifier scum', 'Worn asphalt', 'Curb', 'Drain',
  ]) {
    assert.ok(names.has(expected), expected);
  }
  assert.ok(names.size >= 35);
  assert.ok(names.size <= 48);
});

test('every component role resolves to a material', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const instance = plan.operations.find(({ name }) => name === 'PS-101 提升泵组');
  assert.ok(instance);
  assert.ok(Object.values(instance.instance.overrides).every(({ materialId }) => (
    typeof materialId === 'string' && materialId.length > 0
  )));
});

test('generation budget throws before publication when final stats exceed limits', () => {
  assert.throws(() => assertProjectStats({
    effectiveTriangles: 600000,
    uniqueTriangles: 90000,
    expandedNodes: 12000,
    instances: 300,
  }, WWTP_STATS_LIMITS), /TRIANGLE_BUDGET_EXCEEDED/);
});

test('instance metadata stays within Studio string-value schema', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const instances = plan.operations.filter(({ type }) => type === 'instance.create');
  assert.ok(instances.every(({ metadata }) => (
    Object.values(metadata).every((value) => typeof value === 'string')
  )));
  assert.ok(instances.every(({ metadata }) => (
    Number.isFinite(Number(metadata.clearanceRadius))
  )));
});

test('every standalone node operation uses a typed node content', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const nodes = plan.operations.filter(({ type }) => type === 'node.create');
  assert.ok(nodes.length > 0);
  assert.ok(nodes.every(({ content }) => typeof content?.type === 'string'));
});

test('component radial segment counts stay within Studio limits', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const definitions = plan.operations
    .filter(({ type }) => type === 'component.define')
    .map(({ definition }) => definition);
  for (const definition of definitions) {
    for (const part of definition.parts) {
      if (part.content.kind === 'cylinder') {
        assert.ok(part.content.segments >= 3 && part.content.segments <= 256, part.id);
      }
      if (part.content.kind === 'sphere') {
        assert.ok(part.content.segments >= 3 && part.content.segments <= 256, part.id);
        assert.ok(part.content.rings >= 2 && part.content.rings <= 128, part.id);
      }
    }
  }
});

test('site surfaces do not use baked shadow patches', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const materialNames = new Map(
    plan.operations
      .filter(({ type }) => type === 'material.upsert')
      .map(({ material_id, name }) => [material_id, name]),
  );
  const shadowSites = plan.operations.filter(({ type, metadata, material_id }) => (
    type === 'node.create' &&
    metadata?.role === 'site' &&
    materialNames.get(material_id) === 'Contact shadow'
  ));
  assert.deepEqual(shadowSites, []);
});

test('asphalt materials remain readable neutral greys', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const asphalt = plan.operations
    .filter(({ type }) => type === 'material.upsert')
    .filter(({ name }) => name.endsWith('asphalt'))
    .map(({ base_color, name }) => ({
      name,
      luminance: base_color.slice(0, 3).reduce((sum, value) => sum + value, 0) / 3,
    }));
  assert.equal(asphalt.length, 2);
  assert.ok(asphalt.every(({ luminance }) => luminance >= 0.1), JSON.stringify(asphalt));
});

test('process skids are wired into the treatment train', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const connectionNames = plan.operations
    .filter(({ type }) => type === 'connection.create')
    .map(({ name }) => name);
  const handlingServices = plan.operations
    .filter(({ type }) => type === 'relation.create')
    .map(({ relation }) => relation.metadata?.service);
  for (const expected of [
    'PS-101 → 提升泵组',
    '提升泵组 → GC-101',
    'BLW-101 → 鼓风机 1',
    '鼓风机 1 → AE-103',
    'BLW-101 → 鼓风机 2',
    '鼓风机 2 → AE-203',
    'TH-101 → HEX-101',
    'HEX-101 → DG-101',
    'CHEM-101 → FL-101',
  ]) {
    assert.ok(connectionNames.includes(expected), expected);
  }
  assert.ok(handlingServices.includes('CV-101 → DW-101 泥饼'));
  assert.ok(handlingServices.includes('CV-102 → DW-101 泥饼'));
});

test('process pipe diameters stay within the engineered line size', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const connection = plan.operations.find(({ name }) => name === 'PS-101 → 提升泵组');
  assert.ok(connection);
  assert.ok(connection.connection.diameter <= 1.25);
});

test('above-ground pipe supports are two-column assemblies', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const supports = plan.operations.filter(({ type, metadata }) => (
    type === 'node.create' && metadata?.role === 'pipe-support'
  ));
  const groups = new Map();
  for (const support of supports) {
    const group = support.metadata.supportGroup;
    assert.ok(group);
    groups.set(group, (groups.get(group) ?? 0) + 1);
  }
  assert.ok(groups.size > 0);
  assert.ok([...groups.values()].every((count) => count === 3));
});

test('site contains no generic decorative control-box equipment', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  assert.equal(plan.operations.some(({ metadata }) => metadata?.kind === 'control-box'), false);
});

test('pipe racks contain supports only, never decorative disconnected pipes', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const rack = plan.operations
    .filter(({ type }) => type === 'component.define')
    .map(({ definition }) => definition)
    .find(({ name }) => name === 'Pipe rack');
  assert.ok(rack);
  assert.equal(rack.parts.some(({ id }) => id.includes('-pipe-')), false);
});

test('pump skid headers connect to every pump body', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const pump = plan.operations
    .filter(({ type }) => type === 'component.define')
    .map(({ definition }) => definition)
    .find(({ name }) => name === 'Lift pump skid');
  const ids = new Set(pump.parts.map(({ id }) => id));
  for (let index = 1; index <= 3; index += 1) {
    assert.ok(ids.has(`lps-suction-branch-${index}`));
    assert.ok(ids.has(`lps-discharge-branch-${index}`));
  }
});
