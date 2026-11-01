import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import test from 'node:test';
import { buildPlan as buildLegacyPlan } from '../../scripts/build-wastewater-model.mjs';
import * as ModelKit from '../../scripts/wastewater-plant/model-kit.mjs';
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
  validatePipeAccessoryAlignment,
  validateProcessGraph,
  validateProjectStats,
  validateSiteRoadClearance,
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

test('visual review entry exposes four deterministic inspection distances', async (context) => {
  const scriptUrl = new URL('../../scripts/wastewater-visual-review.mjs', import.meta.url);
  let module;
  try {
    module = await import(scriptUrl);
  } catch (error) {
    assert.fail(`visual review entry is missing: ${error.message}`);
  }
  const views = module.buildWastewaterVisualReview({
    baseUrl: 'http://127.0.0.1:8150',
    token: 'wastewater-review',
  });
  context.diagnostic(JSON.stringify(views));

  assert.deepEqual(views.map(({ name }) => name), [
    'overview',
    'process-mid',
    'building-close',
    'equipment-close',
  ]);
  assert.deepEqual(
    views.map(({ distance }) => distance),
    [360, 58, 42, 30],
  );
  assert.deepEqual(views[3]?.target, [62, 2.2, 10]);
  assert.ok(views.every(({ url }) => (
    url.startsWith('http://127.0.0.1:8150/?')
    && url.includes('token=wastewater-review')
    && url.includes('target=')
    && url.includes('view=')
  )));
});

test('capability report proves the asset exercises advanced Studio modeling paths', async () => {
  const scriptUrl = new URL('../../scripts/wastewater-capability-report.mjs', import.meta.url);
  let module;
  try {
    module = await import(scriptUrl);
  } catch (error) {
    assert.fail(`capability report entry is missing: ${error.message}`);
  }
  const report = module.buildWastewaterCapabilityReport(
    buildLegacyPlan('wastewater-plant'),
  );

  assert.ok(report.componentDefinitions >= 40);
  assert.ok(report.repeatPartDefinitions >= 30);
  assert.ok(report.parameterizedComponents >= 19);
  assert.ok(report.sharedComponentDefinitions >= 10);
  assert.ok(report.customMeshParts >= 20);
  assert.ok(report.closeUpDetailComponents >= 8);
  assert.deepEqual(report.missingInstanceDefinitions, []);
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

test('close-up detail kit emits framing, weather protection and roof services', () => {
  const b = createBuilder();
  const kit = ModelKit;
  const detailWindow = kit.detailWindow;
  const detailDoor = kit.detailDoor;
  const detailLouver = kit.detailLouver;
  const detailRoofVent = kit.detailRoofVent;
  const detailDownpipe = kit.detailDownpipe;

  assert.equal(typeof detailWindow, 'function');
  assert.equal(typeof detailDoor, 'function');
  assert.equal(typeof detailLouver, 'function');
  assert.equal(typeof detailRoofVent, 'function');
  assert.equal(typeof detailDownpipe, 'function');

  const parts = [
    ...detailWindow(b, 'window-a', {
      width: 4.2, height: 1.6, x: 0, y: 3, z: 7, yaw: 0,
    }),
    ...detailDoor(b, 'door-a', {
      width: 3, height: 3.6, x: 5, y: 1.8, z: 7, yaw: 0,
    }),
    ...detailLouver(b, 'louver-a', {
      width: 5, height: 2, x: -5, y: 4, z: 7, yaw: 0, slats: 6,
    }),
    ...detailRoofVent(b, 'vent-a', { x: 2, y: 8, z: -2, radius: 0.45 }),
    ...detailDownpipe(b, 'pipe-a', {
      x: -7, y: 0, z: 7, height: 7, radius: 0.09, outletDirection: 1,
    }),
  ];
  const ids = parts.map(({ id }) => id);

  assert.equal(new Set(ids).size, ids.length);
  assert.ok(parts.length >= 24);
  assert.ok(ids.some((id) => id.includes('window-a-frame-top')));
  assert.ok(ids.some((id) => id.includes('window-a-glass')));
  assert.ok(ids.some((id) => id.includes('door-a-frame-top')));
  assert.ok(ids.some((id) => id.includes('door-a-handle')));
  assert.equal(parts.find(({ id }) => id === 'louver-a-slats')?.repeat, 6);
  assert.ok(ids.some((id) => id.includes('vent-a-cowl')));
  assert.ok(ids.some((id) => id.includes('pipe-a-shoe')));
  assert.ok(parts.every(({ content }) => typeof content?.kind === 'string'));
});

test('close-up equipment kit emits motor accessories and protection', () => {
  const b = createBuilder();
  const detailMotor = ModelKit.detailMotor;
  const detailCouplingGuard = ModelKit.detailCouplingGuard;

  assert.equal(typeof detailMotor, 'function');
  assert.equal(typeof detailCouplingGuard, 'function');
  const parts = [
    ...detailMotor(b, 'motor-a', {
      x: 2, y: 1.2, z: 0, radius: 0.55, length: 1.4, axis: 'x',
    }),
    ...detailCouplingGuard(b, 'guard-a', {
      x: 0.8, y: 1.2, z: 0, length: 0.7, width: 0.65, height: 0.55,
    }),
  ];
  const ids = parts.map(({ id }) => id);

  assert.equal(new Set(ids).size, ids.length);
  assert.ok(ids.some((id) => id.includes('motor-a-fan-cowl')));
  assert.ok(ids.some((id) => id.includes('motor-a-terminal-box')));
  assert.ok(ids.some((id) => id.includes('motor-a-anchor-plate')));
  assert.ok(ids.some((id) => id.includes('guard-a-frame')));
  assert.ok(ids.some((id) => id.includes('guard-a-mesh')));
});

test('equipment service kit emits cable containment and local controls', () => {
  const b = createBuilder();
  const parts = ModelKit.detailEquipmentServices(b, 'skid-a', {
    x: 0, y: 0.3, z: 0, length: 8, width: 4,
  });
  const byId = new Map(parts.map((part) => [part.id, part]));

  assert.ok(byId.has('skid-a-cable-tray'));
  assert.ok((byId.get('skid-a-cable-tray-supports')?.repeat ?? 0) >= 3);
  assert.ok((byId.get('skid-a-cable-drops')?.repeat ?? 0) >= 3);
  assert.ok(byId.has('skid-a-local-control-pole'));
  assert.ok(byId.has('skid-a-local-control-cabinet'));
  assert.ok(byId.has('skid-a-emergency-stop'));
  assert.equal(byId.get('skid-a-maintenance-lugs')?.repeat, 2);
});

test('process skids include cable trays and local service controls', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const definitions = plan.operations
    .filter(({ type }) => type === 'component.define')
    .map(({ definition }) => definition);
  for (const [name, prefix] of [
    ['Lift pump skid', 'lps'],
    ['Blower skid', 'bls'],
    ['Chemical dosing skid', 'cds'],
    ['Dewatering skid', 'dws'],
  ]) {
    const definition = definitions.find((candidate) => candidate.name === name);
    assert.ok(definition, name);
    const ids = new Set(definition.parts.map(({ id }) => id));
    assert.ok(ids.has(`${prefix}-cable-tray`), name);
    assert.ok(ids.has(`${prefix}-local-control-cabinet`), name);
    assert.ok(ids.has(`${prefix}-emergency-stop`), name);
    assert.ok(ids.has(`${prefix}-maintenance-lugs`), name);
  }
});

test('repeated close-up details use deterministic component expansion', () => {
  const b = createBuilder();
  const singleWindow = ModelKit.detailWindow(b, 'window-single', {
    width: 3.2, height: 1.4, x: 1.5, y: 3, z: 0, mullions: 1,
  });
  const parts = [
    ...singleWindow,
    ...ModelKit.detailWindow(b, 'window-repeat', {
      width: 5, height: 1.8, x: 0, y: 3, z: 0, mullions: 4,
    }),
    ...ModelKit.detailLouver(b, 'louver-repeat', {
      width: 6, height: 2.4, x: 0, y: 4, z: 0, slats: 8,
    }),
    ...ModelKit.detailDownpipe(b, 'pipe-repeat', {
      x: 0, y: 0, z: 0, height: 8,
    }),
  ];
  const byId = new Map(parts.map((part) => [part.id, part]));

  assert.equal(byId.get('window-single-mullions')?.repeat, undefined);
  assert.equal(
    byId.get('window-single-mullions')?.transform.translation.x,
    1.5,
  );
  assert.equal(byId.get('window-repeat-mullions')?.repeat, 4);
  assert.equal(byId.get('louver-repeat-slats')?.repeat, 8);
  assert.equal(byId.get('pipe-repeat-brackets')?.repeat, 3);
  assert.equal(
    byId.get('window-repeat-mullions')?.transform.translation.x.expr,
    '-2.5 + 5 * (i + 1) / (count + 1)',
  );
});

test('service buildings expose close-up envelope and roof details', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const definitions = plan.operations
    .filter(({ type }) => type === 'component.define')
    .map(({ definition }) => definition);
  const expected = new Map([
    ['Lift pump house', 'ps'],
    ['Blower house', 'bl'],
    ['Dewatering house', 'dw'],
    ['Odour control', 'odr'],
    ['Gate house', 'gho'],
  ]);

  for (const [name, prefix] of expected) {
    const definition = definitions.find((candidate) => candidate.name === name);
    assert.ok(definition, name);
    const ids = new Set(definition.parts.map(({ id }) => id));
    for (const suffix of ['window-frame', 'door-frame', 'roof-vent', 'downpipe']) {
      assert.ok(
        [...ids].some((id) => id.startsWith(`${prefix}-${suffix}`)),
        `${name} is missing ${suffix}`,
      );
    }
  }
});

test('service buildings expose repeated facade bays, roof seams and service access', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const definition = plan.operations
    .filter(({ type }) => type === 'component.define')
    .map(({ definition: value }) => value)
    .find(({ name }) => name === 'Blower house');
  const byId = new Map(definition.parts.map((part) => [part.id, part]));

  assert.ok((byId.get('bl-facade-joints-s')?.repeat ?? 0) >= 5);
  assert.ok((byId.get('bl-facade-joints-n')?.repeat ?? 0) >= 5);
  assert.ok((byId.get('bl-roof-seams')?.repeat ?? 0) >= 6);
  assert.ok(byId.has('bl-service-door-frame-top'));
  assert.ok([...byId.keys()].some((id) => id.startsWith('bl-window-frame-e1-')));
  assert.ok([...byId.keys()].some((id) => id.startsWith('bl-window-frame-w1-')));
});

test('service buildings use material families that distinguish their functions', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const materialNames = new Map(
    plan.operations
      .filter(({ type }) => type === 'material.upsert')
      .map(({ material_id, name }) => [material_id, name]),
  );
  const instance = (name) => plan.operations.find(({ type, name: candidate }) => (
    type === 'instance.create' && candidate === name
  ));
  const materialFor = (name, partId) => (
    materialNames.get(instance(name)?.instance.overrides[partId]?.materialId)
  );

  assert.equal(materialFor('BLW-101 鼓风机房', 'bl-shell'), 'Insulated metal panel');
  assert.equal(materialFor('CHEM-101 加药间', 'ch-shell'), 'Masonry facade');
  assert.equal(materialFor('DW-101 脱水机房', 'dw-shell'), 'Insulated metal panel');
  assert.equal(materialFor('GTE-101 门卫房', 'gho-body'), 'Masonry facade');
  assert.equal(
    materialFor('BLW-101 鼓风机房', 'bl-roof'),
    'Standing seam roof',
  );
  assert.equal(
    materialFor('CHEM-101 加药间', 'ch-roof'),
    'Standing seam roof',
  );
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

test('concrete structures expose formwork joints and construction bands', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const definitions = plan.operations
    .filter(({ type }) => type === 'component.define')
    .map(({ definition }) => definition);
  const definition = (name) => definitions.find((candidate) => candidate.name === name);
  const parts = (name) => new Map(
    definition(name).parts.map((part) => [part.id, part]),
  );

  const secondary = parts('Secondary clarifier');
  assert.ok((secondary.get('sc-formwork-joints')?.repeat ?? 0) >= 8);
  assert.ok(secondary.has('sc-formwork-band-low'));
  assert.ok(secondary.has('sc-formwork-band-high'));

  const aerobic = parts('Aerobic basin');
  assert.ok((aerobic.get('ae-wall-formwork-joints-n')?.repeat ?? 0) >= 6);
  assert.ok((aerobic.get('ae-wall-formwork-joints-s')?.repeat ?? 0) >= 6);

  const digester = parts('Anaerobic digester');
  assert.ok((digester.get('dg-shell-weld-bands')?.repeat ?? 0) >= 3);
});

test('primary and secondary clarifiers use distinct water tones', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const primary = plan.operations.find(({ type, name }) => (
    type === 'instance.create' && name === 'PC-101 初沉池'
  ));
  const secondary = plan.operations.find(({ type, name }) => (
    type === 'instance.create' && name === 'SC-101 二沉池'
  ));
  assert.ok(primary);
  assert.ok(secondary);
  const primaryMaterialId = primary.instance.overrides['pc-water'].materialId;
  const secondaryMaterialId = secondary.instance.overrides['sc-water'].materialId;
  assert.notEqual(primaryMaterialId, secondaryMaterialId);

  const materials = new Map(
    plan.operations
      .filter(({ type }) => type === 'material.upsert')
      .map(({ material_id, name, base_color: color }) => [material_id, { name, color }]),
  );
  const toSrgb = (value) => (
    value <= 0.0031308 ? value * 12.92 : 1.055 * value ** (1 / 2.4) - 0.055
  );
  const primaryColor = materials.get(primaryMaterialId).color.slice(0, 3).map(toSrgb);
  const secondaryColor = materials.get(secondaryMaterialId).color.slice(0, 3).map(toSrgb);
  const distance = Math.hypot(
    ...primaryColor.map((channel, index) => channel - secondaryColor[index]),
  );
  assert.ok(distance > 0.06, 'clarifier water tones are too similar');
  assert.ok(primaryColor[0] > primaryColor[2], 'primary water should look warmer and more turbid');
  assert.ok(secondaryColor[2] > secondaryColor[0], 'secondary water should look cooler and clearer');
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

test('pipe materials distinguish ordinary silver and functional colors', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const materials = new Map(
    plan.operations
      .filter(({ type }) => type === 'material.upsert')
      .map(({ name, base_color }) => [name, base_color.slice(0, 3)]),
  );
  const groups = {
    silver: [
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
  for (const name of groups.silver) {
    const [r, g, b] = materials.get(name);
    assert.ok(Math.min(r, g, b) > 0.5, name);
    assert.ok(Math.max(r, g, b) - Math.min(r, g, b) < 0.1, name);
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
  assert.ok(chemical[0] > chemical[2] * 1.8);
  assert.ok(chemical[1] > chemical[2] * 1.5);
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

test('gas holder uses pressure-vessel steel on a concrete and steel frame', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const holder = plan.operations.find(({ type, name }) => (
    type === 'instance.create' && name === 'GH-101 沼气柜'
  ));
  assert.ok(holder);
  const definition = plan.operations
    .filter(({ type }) => type === 'component.define')
    .map(({ definition: value }) => value)
    .find(({ id }) => id === holder.instance.definitionId);
  assert.ok(definition);
  const materialNames = new Map(
    plan.operations
      .filter(({ type }) => type === 'material.upsert')
      .map(({ material_id, name }) => [material_id, name]),
  );
  const materialFor = (partId) => (
    materialNames.get(holder.instance.overrides[partId]?.materialId)
  );

  assert.equal(materialFor('gh-sphere'), 'Pressure vessel steel');
  assert.equal(materialFor('gh-base'), 'Reinforced concrete');
  assert.equal(materialFor('gh-leg-1'), 'Galvanised steel');
  for (const partId of [
    'gh-equator-band',
    'gh-latitude-band-north',
    'gh-latitude-band-south',
    'gh-meridian-rib-1',
    'gh-meridian-rib-2',
    'gh-meridian-rib-3',
    'gh-meridian-rib-4',
  ]) {
    const part = definition.parts.find(({ id }) => id === partId);
    assert.equal(part?.content.kind, 'custom_mesh', partId);
    assert.equal(materialFor(partId), 'Dark steel', partId);
  }
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

test('asphalt surfaces stay inside the site boundary', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const materialNames = new Map(
    plan.operations
      .filter(({ type }) => type === 'material.upsert')
      .map(({ material_id, name }) => [material_id, name]),
  );
  const site = plan.operations.find(({ type, name }) => (
    type === 'node.create' && name === '场地（草坪）'
  ));
  assert.ok(site);
  const siteBounds = {
    minX: site.transform.translation.x - site.content.width / 2,
    maxX: site.transform.translation.x + site.content.width / 2,
    minZ: site.transform.translation.z - site.content.depth / 2,
    maxZ: site.transform.translation.z + site.content.depth / 2,
  };
  const asphaltNames = new Set(['Asphalt road', 'New asphalt', 'Worn asphalt']);
  const surfaces = plan.operations.filter(({ type, material_id: materialId }) => (
    type === 'node.create' && asphaltNames.has(materialNames.get(materialId))
  ));
  assert.ok(surfaces.length > 0);

  for (const surface of surfaces) {
    const bounds = {
      minX: surface.transform.translation.x - surface.content.width / 2,
      maxX: surface.transform.translation.x + surface.content.width / 2,
      minZ: surface.transform.translation.z - surface.content.depth / 2,
      maxZ: surface.transform.translation.z + surface.content.depth / 2,
    };
    assert.ok(
      bounds.minX >= siteBounds.minX && bounds.maxX <= siteBounds.maxX
        && bounds.minZ >= siteBounds.minZ && bounds.maxZ <= siteBounds.maxZ,
      `${surface.name} extends outside the site boundary`,
    );
  }
});

test('overlapping site surface materials have stable depth separation', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const materialNames = new Map(
    plan.operations
      .filter(({ type }) => type === 'material.upsert')
      .map(({ material_id, name }) => [material_id, name]),
  );
  const surfaces = plan.operations
    .filter(({ type, metadata }) => type === 'node.create' && metadata?.role === 'site')
    .map(({ name, material_id, transform, content }) => ({
      name,
      material: materialNames.get(material_id),
      y: transform.translation.y,
      minX: transform.translation.x - content.width / 2,
      maxX: transform.translation.x + content.width / 2,
      minZ: transform.translation.z - content.depth / 2,
      maxZ: transform.translation.z + content.depth / 2,
    }));
  const conflicts = [];
  for (let leftIndex = 0; leftIndex < surfaces.length; leftIndex += 1) {
    for (let rightIndex = leftIndex + 1; rightIndex < surfaces.length; rightIndex += 1) {
      const left = surfaces[leftIndex];
      const right = surfaces[rightIndex];
      if (left.material === right.material) continue;
      const overlapX = Math.min(left.maxX, right.maxX) - Math.max(left.minX, right.minX);
      const overlapZ = Math.min(left.maxZ, right.maxZ) - Math.max(left.minZ, right.minZ);
      if (overlapX > 0.01 && overlapZ > 0.01 && Math.abs(left.y - right.y) < 0.03) {
        conflicts.push(`${left.name} / ${right.name}: ${Math.abs(left.y - right.y).toFixed(3)}m`);
      }
    }
  }

  assert.deepEqual(conflicts, []);
});

test('all road surfaces use the same twelve metre carriageway width', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const roads = plan.operations.filter(({ type, name }) => (
    type === 'node.create' && name.startsWith('道路')
  ));
  assert.equal(roads.length, 7);
  for (const road of roads) {
    assert.equal(
      Math.min(road.content.width, road.content.depth),
      12,
      road.name,
    );
  }
});

test('the road network forms one connected surface', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const roads = plan.operations
    .filter(({ type, name }) => (
      type === 'node.create' && name.startsWith('道路')
    ))
    .map(({ name, transform, content }) => ({
      name,
      minX: transform.translation.x - content.width / 2,
      maxX: transform.translation.x + content.width / 2,
      minZ: transform.translation.z - content.depth / 2,
      maxZ: transform.translation.z + content.depth / 2,
    }));
  const connected = new Set([roads[0].name]);
  let changed = true;
  while (changed) {
    changed = false;
    for (const road of roads) {
      if (connected.has(road.name)) continue;
      const touches = roads.some((candidate) => (
        connected.has(candidate.name)
        && road.minX < candidate.maxX
        && road.maxX > candidate.minX
        && road.minZ < candidate.maxZ
        && road.maxZ > candidate.minZ
      ));
      if (touches) {
        connected.add(road.name);
        changed = true;
      }
    }
  }
  assert.deepEqual(
    roads.filter(({ name }) => !connected.has(name)).map(({ name }) => name),
    [],
  );
});

test('trees keep a four metre clear zone from road surfaces', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const roads = plan.operations
    .filter(({ type, name }) => (
      type === 'node.create' && name.startsWith('道路')
    ))
    .map(({ transform, content }) => ({
      minX: transform.translation.x - content.width / 2,
      maxX: transform.translation.x + content.width / 2,
      minZ: transform.translation.z - content.depth / 2,
      maxZ: transform.translation.z + content.depth / 2,
    }));
  const trees = plan.operations.filter(({ type, name }) => (
    type === 'instance.create' && name.startsWith('绿化 /')
  ));
  const obstructing = trees.filter(({ transform }) => (
    roads.some(({ minX, maxX, minZ, maxZ }) => (
      transform.translation.x > minX - 4
      && transform.translation.x < maxX + 4
      && transform.translation.z > minZ - 4
      && transform.translation.z < maxZ + 4
    ))
  ));
  assert.deepEqual(obstructing.map(({ name }) => name), []);
});

test('trees do not grow through paved surfaces', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const materialNames = new Map(
    plan.operations
      .filter(({ type }) => type === 'material.upsert')
      .map(({ material_id, name }) => [material_id, name]),
  );
  const hardscapeMaterials = new Set([
    'Paving',
    'Asphalt road',
    'New asphalt',
    'Worn asphalt',
    'Curb',
    'Drain',
    'Road marking',
  ]);
  const hardscapes = plan.operations
    .filter(({ type, metadata, material_id }) => (
      type === 'node.create'
      && metadata?.role === 'site'
      && hardscapeMaterials.has(materialNames.get(material_id))
    ))
    .map(({ name, transform, content }) => ({
      name,
      minX: transform.translation.x - content.width / 2,
      maxX: transform.translation.x + content.width / 2,
      minZ: transform.translation.z - content.depth / 2,
      maxZ: transform.translation.z + content.depth / 2,
    }));
  const trees = plan.operations.filter(({ type, name }) => (
    type === 'instance.create' && name.startsWith('绿化 / ')
  ));
  const obstructing = trees
    .filter(({ transform }) => hardscapes.some(({ minX, maxX, minZ, maxZ }) => (
      transform.translation.x > minX
      && transform.translation.x < maxX
      && transform.translation.z > minZ
      && transform.translation.z < maxZ
    )))
    .map(({ name }) => name);

  assert.deepEqual(obstructing, []);
});

test('only pipe routes occupy road surfaces', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  assert.deepEqual(validateSiteRoadClearance(plan), { ok: true, issues: [] });

  const tree = plan.operations.find(({ type, name }) => (
    type === 'instance.create' && name.startsWith('绿化 / ')
  ));
  assert.ok(tree);
  tree.transform.translation.x = 125;
  const result = validateSiteRoadClearance(plan);
  assert.equal(result.ok, false);
  assert.ok(result.issues.some(({ code }) => code === 'ROAD_OBSTRUCTION'));
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

test('chemical handling stays outside circular clarifier footprints', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const clarifiers = plan.operations.filter(({ type, name }) => (
    type === 'instance.create' && /^SC-\d+ 二沉池$/.test(name)
  ));
  const assets = [
    { name: 'CHEM-101 加药间', halfLength: 10, halfWidth: 6 },
    { name: 'CHEM-101 加药装置', halfLength: 5, halfWidth: 2.75 },
  ];

  for (const asset of assets) {
    const operation = plan.operations.find(({ type, name }) => (
      type === 'instance.create' && name === asset.name
    ));
    assert.ok(operation, asset.name);
    for (const clarifier of clarifiers) {
      const dx = Math.max(
        Math.abs(operation.transform.translation.x - clarifier.transform.translation.x)
          - asset.halfLength,
        0,
      );
      const dz = Math.max(
        Math.abs(operation.transform.translation.z - clarifier.transform.translation.z)
          - asset.halfWidth,
        0,
      );
      const gap = Math.hypot(dx, dz) -
        Number(clarifier.instance.parameters.radius);
      assert.ok(
        gap >= 3,
        `${asset.name} enters ${clarifier.name} footprint by ${(-gap).toFixed(1)} m`,
      );
    }
  }
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

test('pipe network emits close-up collars, valve actuators and support clamps', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const connections = plan.operations.filter(({ type }) => type === 'connection.create');
  const accessories = plan.operations.filter(({ type, metadata }) => (
    type === 'node.create' && String(metadata?.role ?? '').startsWith('pipe-')
  ));
  const collars = accessories.filter(({ metadata }) => metadata.role === 'pipe-collar');
  const actuators = accessories.filter(({ metadata }) => (
    metadata.role === 'pipe-valve-actuator'
  ));
  const clamps = accessories.filter(({ metadata }) => metadata.role === 'pipe-clamp');

  assert.ok(collars.length >= connections.length * 2);
  assert.ok(actuators.length >= 6);
  assert.ok(clamps.length >= 6);
  assert.ok(actuators.every(({ metadata }) => metadata.lineId));
  assert.ok(clamps.every(({ metadata }) => metadata.lineId));
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

test('process basin water stays muted and readable instead of paint-bright', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const materials = new Map(
    plan.operations
      .filter(({ type }) => type === 'material.upsert')
      .map(({ name, base_color: color }) => [name, color]),
  );
  const luminance = ([r, g, b]) => 0.2126 * r + 0.7152 * g + 0.0722 * b;
  const toSrgb = (value) => (
    value <= 0.0031308 ? value * 12.92 : 1.055 * value ** (1 / 2.4) - 0.055
  );
  const expectedBrightness = new Map([
    ['Anoxic water', 0.42],
    ['Aerobic water', 0.50],
    ['Clarified water', 0.58],
  ]);

  for (const [name, minimum] of expectedBrightness) {
    const color = materials.get(name);
    assert.ok(color, name);
    const [r, g, b] = color.slice(0, 3).map(toSrgb);
    assert.ok(luminance([r, g, b]) >= minimum, `${name} is too dark`);
    const maximum = Math.max(r, g, b);
    const minimumChannel = Math.min(r, g, b);
    assert.ok((maximum - minimumChannel) / maximum <= 0.2, `${name} is too saturated`);
    assert.ok(g >= r * 1.05, `${name} lost its green-grey tint`);
    assert.ok(b >= r, `${name} lost its cool grey tint`);
  }
});

test('liquid surfaces declare the generic water shading mode', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const materials = new Map(
    plan.operations
      .filter(({ type }) => type === 'material.upsert')
      .map(({ name, shading, opacity }) => [name, { shading, opacity }]),
  );
  for (const name of [
    'Process water',
    'Anoxic water',
    'Aerobic water',
    'Clarified water',
    'Primary clarifier water',
    'Sludge water',
    'Sludge',
  ]) {
    const material = materials.get(name);
    assert.equal(material.shading, 'water', name);
    assert.ok(material.opacity >= 0.55 && material.opacity < 1, name);
  }
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

test('pipe supports sit on the routed pipe instead of open lawn', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  assert.deepEqual(validatePipeAccessoryAlignment(plan), { ok: true, issues: [] });
});

test('buried pipes do not leave unexplained surface covers on the lawn', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  assert.equal(plan.operations.some(({ type, metadata }) => (
    type === 'node.create' && metadata?.role === 'pipe-cover'
  )), false);
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

test('odour control carries roof stacks without a disconnected decorative duct', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const odourControl = plan.operations
    .filter(({ type }) => type === 'component.define')
    .map(({ definition }) => definition)
    .find(({ name }) => name === 'Odour control');
  const ids = new Set(odourControl.parts.map(({ id }) => id));

  assert.ok(ids.has('odr-stack-w'));
  assert.ok(ids.has('odr-stack-e'));
  assert.equal(ids.has('odr-duct'), false);
});

test('every pipe rack carries a real above-ground process line', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const racks = plan.operations.filter(({ type, metadata }) => (
    type === 'instance.create' && metadata?.role === 'piping'
  ));
  const connections = new Map(
    plan.operations
      .filter(({ type }) => type === 'connection.create')
      .map((operation) => [operation.connection.metadata.lineId, operation]),
  );

  assert.ok(racks.length > 0);
  for (const rack of racks) {
    const lineId = rack.metadata.lineId;
    assert.ok(lineId, `${rack.name} has no carrier line`);
    const line = connections.get(lineId);
    assert.ok(line, `${rack.name} references missing ${lineId}`);
    assert.notEqual(line.connection.routePolicy.kind, 'buried', rack.name);
  }
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
