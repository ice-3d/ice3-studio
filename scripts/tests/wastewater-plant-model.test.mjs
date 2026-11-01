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
  validateProjectStats,
  validateWastewaterPlan,
} from '../../scripts/wastewater-plant/validate.mjs';

const digest = (value) => createHash('sha256')
  .update(JSON.stringify(value))
  .digest('hex');

test('module split preserves the legacy production plan', async () => {
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
    'sc-waterline', 'sc-scum-band', 'sc-inlet-sleeve', 'sc-outlet-sleeve',
  ]) {
    assert.ok(ids.has(expected), expected);
  }
  assert.ok(definition.parts.length >= 30);
});

test('aeration basin has lanes, baffles, air distribution and guardrails', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const definition = plan.operations
    .filter(({ type }) => type === 'component.define')
    .find(({ definition }) => definition.name === 'Aeration basin (mixed liquor)').definition;
  const ids = new Set(definition.parts.map(({ id }) => id));

  assert.ok(ids.has('ae-water'));
  assert.ok(ids.has('ae-foam-band'));
  assert.ok([...ids].some((id) => id.startsWith('ae-baffle-')));
  assert.ok([...ids].some((id) => id.startsWith('ae-air-')));
  assert.ok([...ids].some((id) => id.startsWith('ae-rail-')));
  assert.ok(definition.parts.length >= 35);
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
