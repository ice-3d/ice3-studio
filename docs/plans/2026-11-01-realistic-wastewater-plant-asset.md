# Engineering-Grade Wastewater Plant Asset Implementation Plan

> Document ID: ICE3-STUDIO-PLAN-015
>
> Status: Historical
>
> Owner: ICE3 Studio maintainers

> **历史状态：** 本文保留已完成实施过程的审计记录，不代表当前生产依赖；当前能力以
> [Asset Pipeline](../reference/asset-pipeline.md) 和 [能力清单](../reference/capabilities.md)
> 为准。

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 `projects/wastewater-plant` 升级为原创、可专业评审、30 万至 50 万三角形的通用
污水处理厂资产。

**Architecture:** 保留 Studio 原生 Document、Component Definition、Instance 和 Connection
模型。将现有单体生成脚本拆为一个专用源码目录，通过工程校验、细节组件和确定性布局逐层
增加可信度，不引入烘焙模型或新的运行时依赖。

**Tech Stack:** Node.js 24.22.1、JavaScript ESM、Node Test、Rust Studio Core/Server、
React Viewer、WebGL2、Playwright、Google Chrome

**Spec:** `docs/specs/2026-11-01-realistic-wastewater-plant-asset-design.md`

## Global Constraints

- 使用 Node.js 24.22.1 LTS 和 npm 11 或更高版本。
- 工程必须保持原创；参考照片不得复制进仓库、测试夹具、截图或发布物。
- 不得出现竞品、厂商或第三方产品名称。
- 不增加 Studio Core 或 Viewer 的运行时依赖。
- 所有生产代码先有失败测试，再写最小实现；每个行为改动独立提交。
- Git author 和 committer 必须为 `damoqiongqiu <damoqiongqiu@126.com>`。
- 不推送远端；当前任务只产生本地提交。
- 新源码只进入 `scripts/wastewater-plant/`，测试只进入现有 `scripts/tests/`。
- 临时截图和浏览器状态必须在交付前清理；验收截图只写现有 `output/`。
- 有效三角形目标为 300,000 至 500,000，唯一三角形不超过 150,000。

## Review Focus

- 管线端点引用未知实例、未知端口或介质不匹配时，必须在创建 revision 前失败，而不是发布
  部分场景。
- 组件参数放大后出现明显池体穿插、检修净距不足或管线穿过建筑时，校验必须阻止工程生成。
- 新增重复构件后节点、组件、材质和 connection ID 重复时，构建必须失败。
- 资产细节增加后超过性能预算或在 Chrome 中出现失败资源、空白视口、黑面时，验收不得通过。
- 参考图片中的具体厂区特征、设备编号或构图被无意复制进仓库或最终截图时，提交必须被拒绝。

---

## File Structure

### New files

- `scripts/wastewater-plant/model-kit.mjs`：创建 Builder、工程细节组件和确定性几何辅助。
- `scripts/wastewater-plant/components.mjs`：所有处理构筑物、设备和通用工程组件。
- `scripts/wastewater-plant/site-plan.mjs`：厂区分区、实例、道路、景观和连接布局。
- `scripts/wastewater-plant/validate.mjs`：拓扑、净距、ID、预算和发布前校验。
- `scripts/tests/wastewater-plant-model.test.mjs`：资产计划、拓扑、性能和命名契约。

### Modified files

- `scripts/build-wastewater-model.mjs`：保留 CLI 入口和 `buildPlan` 兼容导出。
- `scripts/verify.mjs`：加入污水处理厂资产契约测试。
- `projects/README.md`：更新工艺、规模、视角和已知限制。
- `docs/README.md`：计划完成后将 SPEC-012 与 PLAN-015 状态同步为 Implemented。

### Generated files

- `projects/wastewater-plant/**`：由生成器重建的工程真源。
- `output/wwtp-after-optimization.png`：最终总览验收截图。
- `output/wwtp-detail-biological.png`：最终生物池近景验收截图。

---

### Task 1: Split the Generator Without Changing Output

**Files:**
- Create: `scripts/wastewater-plant/model-kit.mjs`
- Create: `scripts/wastewater-plant/components.mjs`
- Create: `scripts/wastewater-plant/site-plan.mjs`
- Modify: `scripts/build-wastewater-model.mjs`
- Test: `scripts/tests/wastewater-plant-model.test.mjs`

**Interfaces:**
- Consumes: current `buildPlan(projectId)` output shape.
- Produces: `buildPlan(projectId: string) => { projectId: string, operations: object[], summary: { components: number, instances: number, connections: number, materials: number } }`.

- [x] **Step 1: Write the failing module-boundary test**

```js
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import test from 'node:test';
import { buildPlan as buildLegacyPlan } from '../../scripts/build-wastewater-model.mjs';

const digest = (value) => createHash('sha256')
  .update(JSON.stringify(value))
  .digest('hex');

test('module split preserves the legacy production plan', async () => {
  const module = await import('../../scripts/wastewater-plant/site-plan.mjs');
  assert.equal(
    digest(module.buildPlan('wastewater-plant')),
    digest(buildLegacyPlan('wastewater-plant')),
  );
  assert.equal(
    digest(buildLegacyPlan('wastewater-plant')),
    '61409900f541447c8cd415d10067e8c17f246d660c9385b587bd4a4ef380b7ce',
  );
});
```

- [x] **Step 2: Run the test and verify RED**

Run:

```bash
node --test scripts/tests/wastewater-plant-model.test.mjs
```

Expected: `ERR_MODULE_NOT_FOUND` for
`scripts/wastewater-plant/site-plan.mjs`.

- [x] **Step 3: Move the existing implementation into focused modules**

`model-kit.mjs` exports the unchanged values and functions:

```js
export {
  TAU,
  srgb,
  lit,
  vec,
  nodeTransform,
  rotateY,
  LIE_ALONG_X,
  box,
  cylinder,
  sphere,
  createBuilder,
};
```

Move `createBuilder()` and every builder helper above the wastewater component functions into
`model-kit.mjs` verbatim. `createBuilder()` temporarily continues to own `material`, `repeatPart`,
`part`, `port`, `define`, `instance`, `connect`, `ground`, `walkway`, `equipment`,
`shadowPatch`, `box`, `cylinder` and `rotateY`.

`components.mjs` imports the helpers from `model-kit.mjs` and exports every existing
`screeningChannel` through `parkingBay` component function unchanged.

`site-plan.mjs` imports `createBuilder`, `srgb`, `vec` and the component functions, then contains
the existing `buildPlan(projectId)` body unchanged:

```js
export function buildPlan(projectId) {
  const b = createBuilder();
  // Existing material, component, instance, connection and site-plan operations.
  return {
    projectId,
    operations: b.state.ops,
    summary: {
      components: b.state.components.size,
      instances: b.state.ops.filter((op) => op.type === 'instance.create').length,
      connections: b.state.ops.filter((op) => op.type === 'connection.create').length,
      materials: b.state.materials.size,
    },
  };
}
```

`build-wastewater-model.mjs` keeps its command entry point and exports:

```js
export { buildPlan } from './wastewater-plant/site-plan.mjs';
```

- [x] **Step 4: Run the focused test and inspect counts**

Run:

```bash
node --test scripts/tests/wastewater-plant-model.test.mjs
node --check scripts/build-wastewater-model.mjs
```

Expected: both commands PASS; the plan digest remains
`61409900f541447c8cd415d10067e8c17f246d660c9385b587bd4a4ef380b7ce`.

- [x] **Step 5: Commit**

```bash
git add scripts/build-wastewater-model.mjs scripts/wastewater-plant \
  scripts/tests/wastewater-plant-model.test.mjs
git commit -m "refactor(wwtp): split asset generator into modules"
```

---

### Task 2: Add Engineering Validation and Budget Gates

**Files:**
- Create: `scripts/wastewater-plant/validate.mjs`
- Modify: `scripts/wastewater-plant/site-plan.mjs`
- Modify: `scripts/tests/wastewater-plant-model.test.mjs`

**Interfaces:**
- Consumes: plan shape from Task 1.
- Produces:
  - `validateWastewaterPlan(plan) => { ok: boolean, issues: Array<{ code: string, message: string }> }`.
  - `validateProjectStats(stats, limits) => { ok: boolean, issues: Array<{ code: string, message: string }> }`.
  - `WWTP_STATS_LIMITS` with `minTriangles: 300000`, `maxTriangles: 500000`,
    `maxUniqueTriangles: 150000`, `maxExpandedNodes: 15000`, `maxInstances: 500`.

- [x] **Step 1: Write failing topology and budget tests**

```js
import {
  WWTP_STATS_LIMITS,
  validateProjectStats,
  validateWastewaterPlan,
} from '../../scripts/wastewater-plant/validate.mjs';

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
```

- [x] **Step 2: Run tests and verify RED**

Run:

```bash
node --test scripts/tests/wastewater-plant-model.test.mjs
```

Expected: FAIL with `ERR_MODULE_NOT_FOUND` for `validate.mjs`.

- [x] **Step 3: Implement deterministic validation**

Implement the public interface in `validate.mjs`:

```js
export const WWTP_STATS_LIMITS = Object.freeze({
  minTriangles: 300000,
  maxTriangles: 500000,
  maxUniqueTriangles: 150000,
  maxExpandedNodes: 15000,
  maxInstances: 500,
});

export function validateWastewaterPlan(plan) {
  const issues = [];
  const definitions = new Map();
  const instances = new Map();
  const nodeIds = new Set();
  const relationIds = new Set();

  for (const operation of plan.operations) {
    if (operation.type === 'component.define') {
      definitions.set(operation.definition.id, operation.definition);
    }
    if (operation.type === 'instance.create') {
      instances.set(operation.node_id, operation);
    }
    if (operation.type === 'relation.create') {
      relationIds.add(operation.relation.id);
    }
    const id = operation.node_id ?? operation.definition?.id ??
      operation.relation?.id ?? operation.material_id;
    if (id && nodeIds.has(id)) {
      issues.push({ code: 'DUPLICATE_ID', message: `duplicate id ${id}` });
    }
    if (id) nodeIds.add(id);
  }

  for (const operation of plan.operations) {
    if (operation.type !== 'connection.create') continue;
    for (const endpoint of [operation.connection.from, operation.connection.to]) {
      const instance = instances.get(endpoint.nodeId);
      const definition = instance && definitions.get(instance.instance.definitionId);
      const port = definition?.ports.find(({ name }) => name === endpoint.port);
      if (!port) {
        issues.push({
          code: 'UNKNOWN_PORT',
          message: `${endpoint.nodeId}/${endpoint.port}`,
        });
        continue;
      }
      if (port.medium !== operation.connection.medium) {
        issues.push({
          code: 'MEDIUM_MISMATCH',
          message: `${endpoint.nodeId}/${endpoint.port}`,
        });
      }
    }
    if (!relationIds.has(operation.connection.relationId)) {
      issues.push({
        code: 'MISSING_RELATION',
        message: operation.connection.relationId,
      });
    }
  }

  return { ok: issues.length === 0, issues };
}
```

Add the explicit bounds check shown by `validateProjectStats` in the test. Call
`validateWastewaterPlan(plan)` from `buildPlan` before returning.

- [x] **Step 3a: Implement the statistics budget contract**

```js
export function validateProjectStats(stats, limits) {
  const issues = [];
  const check = (condition, code, message) => {
    if (!condition) issues.push({ code, message });
  };
  check(
    Number.isInteger(stats.effectiveTriangles) &&
      stats.effectiveTriangles >= limits.minTriangles,
    'TRIANGLE_BUDGET_UNDERFLOW',
    `effective triangles ${stats.effectiveTriangles} below ${limits.minTriangles}`,
  );
  check(
    Number.isInteger(stats.effectiveTriangles) &&
      stats.effectiveTriangles <= limits.maxTriangles,
    'TRIANGLE_BUDGET_EXCEEDED',
    `effective triangles ${stats.effectiveTriangles} above ${limits.maxTriangles}`,
  );
  check(
    Number.isInteger(stats.uniqueTriangles) &&
      stats.uniqueTriangles <= limits.maxUniqueTriangles,
    'UNIQUE_TRIANGLE_BUDGET_EXCEEDED',
    `unique triangles ${stats.uniqueTriangles} above ${limits.maxUniqueTriangles}`,
  );
  check(
    Number.isInteger(stats.expandedNodes) &&
      stats.expandedNodes <= limits.maxExpandedNodes,
    'EXPANDED_NODE_BUDGET_EXCEEDED',
    `expanded nodes ${stats.expandedNodes} above ${limits.maxExpandedNodes}`,
  );
  check(
    Number.isInteger(stats.instances) && stats.instances <= limits.maxInstances,
    'INSTANCE_BUDGET_EXCEEDED',
    `instances ${stats.instances} above ${limits.maxInstances}`,
  );
  return { ok: issues.length === 0, issues };
}
```

- [x] **Step 4: Run focused and existing Node tests**

Run:

```bash
node --test scripts/tests/wastewater-plant-model.test.mjs
node --check scripts/wastewater-plant/validate.mjs
```

Expected: PASS with no duplicate IDs, unknown ports or media mismatches.

- [x] **Step 5: Commit**

```bash
git add scripts/wastewater-plant/validate.mjs \
  scripts/wastewater-plant/site-plan.mjs \
  scripts/tests/wastewater-plant-model.test.mjs
git commit -m "test(wwtp): enforce asset topology and budget contracts"
```

---

### Task 3: Add Reusable Engineering Detail Primitives

**Files:**
- Modify: `scripts/wastewater-plant/model-kit.mjs`
- Modify: `scripts/tests/wastewater-plant-model.test.mjs`

**Interfaces:**
- Produces:
  - `detailFlange(b, prefix, { diameter, position, rotation, material }) => object[]`.
  - `detailValve(b, prefix, { diameter, position, rotation, material }) => object[]`.
  - `detailStair(b, prefix, { x, y, z, width, height, yaw }) => object[]`.
  - `detailGuardrail(b, prefix, { x, y, z, length, yaw }) => object[]`.
  - `detailSupport(b, prefix, { x, y, z, height, width }) => object[]`.
  - Every returned array uses unique part IDs derived from `prefix`.

- [x] **Step 1: Write failing detail-kit test**

```js
import {
  createBuilder,
  detailFlange,
  detailGuardrail,
  detailStair,
  detailSupport,
  detailValve,
} from '../../scripts/wastewater-plant/model-kit.mjs';

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
```

- [x] **Step 2: Run test and verify RED**

Run:

```bash
node --test scripts/tests/wastewater-plant-model.test.mjs
```

Expected: FAIL because the detail helpers are not exported.

- [x] **Step 3: Implement minimal detail helpers**

Use boxes and cylinders already supported by Studio. Each flange is a short wide cylinder plus a
short pipe sleeve. A valve is a body cylinder, bonnet box and handwheel cylinder. A stair is a
run of 8-12 treads plus two stringers. A guardrail has two posts, top rail, mid rail and toe board.
A support has two columns and a crossbeam.

All positions are local to the supplied `position` or `x/y/z`, and all rotations use the existing
`vec` and quaternion shape:

```js
export function detailFlange(b, prefix, { diameter, position, rotation, material }) {
  return [
    b.part(`${prefix}-sleeve`, b.cylinder(diameter * 0.54, 0.18, 16),
      position.x, position.y, position.z, material, { rotation }),
    b.part(`${prefix}-plate`, b.cylinder(diameter * 0.75, 0.08, 16),
      position.x, position.y, position.z, material, { rotation }),
  ];
}
```

- [x] **Step 4: Run focused tests**

Run:

```bash
node --test scripts/tests/wastewater-plant-model.test.mjs
```

Expected: PASS, with unique IDs for every detail part.

- [x] **Step 5: Commit**

```bash
git add scripts/wastewater-plant/model-kit.mjs \
  scripts/tests/wastewater-plant-model.test.mjs
git commit -m "feat(wwtp): add engineering detail primitives"
```

---

### Task 4: Rebuild Circular Clarifiers and Biological Basins

**Files:**
- Modify: `scripts/wastewater-plant/components.mjs`
- Modify: `scripts/wastewater-plant/site-plan.mjs`
- Modify: `scripts/tests/wastewater-plant-model.test.mjs`

**Interfaces:**
- Produces:
  - `circularClarifier(b, prefix, { radius, depth, wall, segments, bridgeMode }) => object[]`.
  - `aerationBasin(b, prefix, { length, width, depth, wall, lanes }) => object[]`.
- `bridgeMode` accepts `"half"` and `"full"`.
- Component ports retain stable names `inlet`, `outlet`, `air-in` and `sludge`.

- [x] **Step 1: Write failing component-fidelity tests**

```js
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
```

- [x] **Step 2: Run tests and verify RED**

Run:

```bash
node --test --test-name-pattern="circular clarifier|aeration basin" \
  scripts/tests/wastewater-plant-model.test.mjs
```

Expected: FAIL on missing waterline, scum band, sleeves, foam band and expanded part count.

- [x] **Step 3: Implement the two component families**

Use at least 96 segments for the visible circular wall, weir and coping. Keep the current
parameter expressions for `radius`, `depth` and `wall`. Add:

- A darker waterline band immediately below the coping.
- A narrow scum band inside the weir.
- Pipe sleeves centered on the inlet and outlet ports.
- A full-center walking bridge for secondary clarifiers and a half bridge for primary clarifiers.
- A stair attached to the bridge landing.
- Drive units, handrail segments and ladder supports.

For the aeration basin, generate three longitudinal lanes with baffle walls, 2 m lane widths,
two aeration branches per lane, a foam band over the first pass, a cross-basin maintenance deck,
and continuous guardrails on both longitudinal edges.

Replace the definitions in `site-plan.mjs` while retaining every existing port name and medium.

- [x] **Step 4: Run topology and component tests**

Run:

```bash
node --test scripts/tests/wastewater-plant-model.test.mjs
```

Expected: PASS; all connections still resolve and the plan remains deterministic.

- [x] **Step 5: Commit**

```bash
git add scripts/wastewater-plant/components.mjs \
  scripts/wastewater-plant/site-plan.mjs \
  scripts/tests/wastewater-plant-model.test.mjs
git commit -m "feat(wwtp): rebuild clarifiers and biological basins"
```

---

### Task 5: Add Recognizable Process Equipment and Building Detail

**Files:**
- Modify: `scripts/wastewater-plant/model-kit.mjs`
- Modify: `scripts/wastewater-plant/components.mjs`
- Modify: `scripts/wastewater-plant/site-plan.mjs`
- Modify: `scripts/tests/wastewater-plant-model.test.mjs`

**Interfaces:**
- Produces component functions:
  - `pumpSkid(b, prefix, { length, width, count }) => object[]`.
  - `blowerSkid(b, prefix, { length, width, count }) => object[]`.
  - `chemicalDosingSkid(b, prefix, { tanks, mixers }) => object[]`.
  - `dewateringSkid(b, prefix, { units }) => object[]`.
  - `sludgeHeatExchanger(b, prefix, { length, shellDiameter }) => object[]`.
  - `valveManifold(b, prefix, { branches, diameter }) => object[]`.
  - `serviceCanopy(b, prefix, { length, width, height }) => object[]`.

- [x] **Step 1: Write failing equipment-recognition tests**

```js
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
```

- [x] **Step 2: Run test and verify RED**

Run:

```bash
node --test --test-name-pattern="major equipment" \
  scripts/tests/wastewater-plant-model.test.mjs
```

Expected: FAIL because the new component definitions do not exist.

- [x] **Step 3: Implement equipment families and place them in the plan**

Each pump unit includes plinth, pump casing, motor, coupling guard, suction pipe, discharge pipe
and local isolation valve. Each blower includes plinth, volute, motor, inlet filter, discharge
elbow and acoustic enclosure rails. Dosing includes tanks, mixers, metering skids, secondary
containment and a canopy. Dewatering includes centrifuge bowl, motor, feed pump, polymer skid and
maintenance access.

Add one lift-pump skid in `PS-101`, two blower skids beside `BLW-101`, one dosing skid beside
`CHEM-101`, two dewatering skids inside `DW-101`, and one heat-exchanger skid between the
thickener and digester area. Keep the existing coarse building shells but cut recognizable door,
louver, canopy and roof-drain parts into each facade.

- [x] **Step 4: Run the complete asset test file**

Run:

```bash
node --test scripts/tests/wastewater-plant-model.test.mjs
```

Expected: PASS; all equipment definitions are deterministic and all ports resolve.

- [x] **Step 5: Commit**

```bash
git add scripts/wastewater-plant/model-kit.mjs \
  scripts/wastewater-plant/components.mjs \
  scripts/wastewater-plant/site-plan.mjs \
  scripts/tests/wastewater-plant-model.test.mjs
git commit -m "feat(wwtp): add process equipment and building detail"
```

---

## Task 6: Reorganize the Site, Roads and Readable Names

**Files:**
- Modify: `scripts/wastewater-plant/site-plan.mjs`
- Modify: `scripts/tests/wastewater-plant-model.test.mjs`

**Interfaces:**
- Produces named site metadata:
  - Every instance has `metadata.stage` in
    `"intake" | "primary" | "biological" | "secondary" | "tertiary" | "sludge" | "auxiliary" | "landscape"`.
  - Every site node has `metadata.role` in `"site" | "paving" | "landscape" | "piping"`.
- Preserves all existing stable node IDs and component ports.

- [x] **Step 1: Write failing layout and naming tests**

```js
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
```

- [x] **Step 2: Run tests and verify RED**

Run:

```bash
node --test --test-name-pattern="site plan|parallel treatment trains" \
  scripts/tests/wastewater-plant-model.test.mjs
```

Expected: FAIL because technical `标线 z=` names, service-yard names and clearance radii are
missing.

- [x] **Step 3: Replace the flat site with six functional zones**

Keep the existing site bounds unless validation shows clearance failures. Add:

- A continuous perimeter road with rounded-looking multi-segment corners.
- A service loop around biological and secondary treatment.
- Equipment unloading areas beside the pump, blower, chemical and dewatering houses.
- Raised curb segments, drains, inspection covers and footprint pads.
- A maintenance path between each clarifier row.
- Named service areas and road markings.

Replace generated road-marking names with `南侧道路标线`, `北侧道路标线`, `西侧道路标线` and
`东侧道路标线`. Modify tree placement to form irregular clusters while leaving equipment access
and fire lanes clear.

Add `metadata.clearanceRadius` to every process instance. Use the largest horizontal half-extent
of the component footprint plus 3 m maintenance allowance. Extend
`validateWastewaterPlan(plan)` to compare every pair of process instances in the same functional
zone and emit:

```js
{
  code: 'CLEARANCE_VIOLATION',
  message: `${left.name} overlaps ${right.name}`,
}
```

- [x] **Step 4: Run validation and layout tests**

Run:

```bash
node --test scripts/tests/wastewater-plant-model.test.mjs
```

Expected: PASS; no raw coordinate names, missing stage metadata or duplicate node IDs.

- [x] **Step 5: Commit**

```bash
git add scripts/wastewater-plant/site-plan.mjs \
  scripts/tests/wastewater-plant-model.test.mjs
git commit -m "feat(wwtp): rebuild plant circulation and site details"
```

---

### Task 7: Complete Pipe Routing, Fittings and Supports

**Files:**
- Modify: `scripts/wastewater-plant/model-kit.mjs`
- Modify: `scripts/wastewater-plant/site-plan.mjs`
- Modify: `scripts/wastewater-plant/validate.mjs`
- Modify: `scripts/tests/wastewater-plant-model.test.mjs`

**Interfaces:**
- Extend:
  - `connect(fromKey, fromPort, toKey, toPort, name, medium, diameter, options?)`
  - `options` accepts
    `{ lineId?: string, fittings?: boolean, supports?: boolean, flow?: "forward" | "reverse" }`.
- Produces accessory nodes with:
  - `metadata.role` equal to `"pipe-flange"`, `"pipe-valve"` or `"pipe-support"`.
  - `metadata.lineId` equal to the connection's line ID.

- [x] **Step 1: Write failing pipeline-accessory tests**

```js
test('every visible process line has endpoint fittings and metadata', () => {
  const plan = buildLegacyPlan('wastewater-plant');
  const connections = plan.operations.filter(({ type }) => type === 'connection.create');
  const accessories = plan.operations.filter(({ type, metadata }) => (
    type === 'node.create' && String(metadata?.role ?? '').startsWith('pipe-')
  ));
  const labels = accessories.filter(({ metadata }) => metadata.role === 'pipe-label');
  assert.ok(connections.length >= 31);
  assert.ok(accessories.length >= connections.length * 2);
  assert.ok(labels.length >= 5);
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
```

- [x] **Step 2: Run tests and verify RED**

Run:

```bash
node --test --test-name-pattern="process line|missing accessory" \
  scripts/tests/wastewater-plant-model.test.mjs
```

Expected: FAIL because connection metadata and accessory nodes do not exist.

- [x] **Step 3: Generate fittings and supports from resolved ports**

Extend `createBuilder()` to retain each instance's transform and definition. Add a deterministic
`resolveWorldPort(instanceKey, portName)` function that applies instance translation and yaw to the
port position and direction.

For each connection:

- Add a unique `lineId`, `medium`, `diameter`, `flow` and human-readable service metadata.
- Place flange details at both endpoint ports.
- Add isolation valves at pump discharges and branch points.
- Add pipe supports every 6 m along rack routes and every 3 m along visible above-ground routes.
- Add an inspection cover at the ground projection of buried-line endpoints.
- Add physical line-number plates to the five pipe-rack headers, without floating UI labels.

Do not add decorative fittings inside the opaque wall volume; place the sleeve on the wall plane
and the flange just outside it.

Extend `validateWastewaterPlan(plan)` with the line-reference check:

```js
const lineIds = new Set(
  plan.operations
    .filter(({ type }) => type === 'connection.create')
    .map(({ connection }) => connection.metadata?.lineId)
    .filter(Boolean),
);
for (const operation of plan.operations) {
  if (operation.type !== 'node.create') continue;
  if (!String(operation.metadata?.role ?? '').startsWith('pipe-')) continue;
  if (!lineIds.has(operation.metadata?.lineId)) {
    issues.push({
      code: 'UNKNOWN_LINE',
      message: String(operation.metadata?.lineId ?? 'missing'),
    });
  }
}
```

- [x] **Step 4: Run topology and line tests**

Run:

```bash
node --test scripts/tests/wastewater-plant-model.test.mjs
```

Expected: PASS; all accessories reference an existing line and all connections retain valid ports.

- [x] **Step 5: Commit**

```bash
git add scripts/wastewater-plant/model-kit.mjs \
  scripts/wastewater-plant/site-plan.mjs \
  scripts/wastewater-plant/validate.mjs \
  scripts/tests/wastewater-plant-model.test.mjs
git commit -m "feat(wwtp): complete pipe fittings and supports"
```

---

## Task 8: Add Material and Surface Depth

**Files:**
- Modify: `scripts/wastewater-plant/site-plan.mjs`
- Modify: `scripts/wastewater-plant/components.mjs`
- Modify: `scripts/tests/wastewater-plant-model.test.mjs`

**Interfaces:**
- Produces material IDs for:
  `concrete-new`, `concrete-old`, `concrete-wet`, `steel-dark`, `stainless`,
  `roof-light`, `water-anoxic`, `water-aerobic`, `water-clarified`, `water-sludge`,
  `foam`, `scum`, `asphalt-new`, `asphalt-worn`, `curb`, `drain`.
- Existing material IDs remain available for compatibility during migration.

- [x] **Step 1: Write failing material tests**

```js
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
```

- [x] **Step 2: Run test and verify RED**

Run:

```bash
node --test --test-name-pattern="materials separate" \
  scripts/tests/wastewater-plant-model.test.mjs
```

Expected: FAIL because the detailed material vocabulary does not exist.

- [x] **Step 3: Implement material zoning and surface geometry**

Use warm off-white for new concrete, slightly grey-beige for weathered concrete, and darker
bands for wet concrete. Use low-saturation pipe colors, galvanised rails and stainless wet-well
equipment. Use chemically distinct but restrained water colors.

Add thin geometry for:

- Waterline and splash zone on circular clarifiers.
- Foam strips in aerobic lanes and scum marks in clarifiers.
- Expansion joints on long concrete walls.
- Roof ribs, door thresholds and louver frames.
- Curbs, drains and worn pavement patches.
- Narrow, neutral contact strips under building plinths and around buried tank bases.

Do not add free-floating labels or large black shadow patches.

- [x] **Step 4: Run material and topology tests**

Run:

```bash
node --test scripts/tests/wastewater-plant-model.test.mjs
```

Expected: PASS; every material reference resolves and all IDs remain deterministic.

- [x] **Step 5: Commit**

```bash
git add scripts/wastewater-plant/components.mjs \
  scripts/wastewater-plant/site-plan.mjs \
  scripts/tests/wastewater-plant-model.test.mjs
git commit -m "feat(wwtp): add realistic material and surface detail"
```

---

## Task 9: Integrate Asset Verification into the Project Gate

**Files:**
- Modify: `scripts/verify.mjs:25-85`
- Modify: `scripts/build-wastewater-model.mjs:1750-1845`
- Modify: `scripts/tests/wastewater-plant-model.test.mjs`

**Interfaces:**
- Consumes: `validateWastewaterPlan` and `validateProjectStats`.
- Produces: `npm run verify` fails on broken asset topology, performance overflow or malformed
  generated plan.

- [x] **Step 1: Write a failing generation-budget test**

```js
test('build fails before publication when final stats exceed the asset budget', () => {
  const result = validateProjectStats({
    effectiveTriangles: 600000,
    uniqueTriangles: 90000,
    expandedNodes: 12000,
    instances: 300,
  }, WWTP_STATS_LIMITS);
  assert.equal(result.ok, false);
  assert.ok(result.issues.some(({ code }) => code === 'TRIANGLE_BUDGET_EXCEEDED'));
});
```

- [x] **Step 2: Run tests and verify RED**

Run:

```bash
node --test scripts/tests/wastewater-plant-model.test.mjs
```

Expected: FAIL because the triangle budget diagnostic is not implemented.

- [x] **Step 3: Wire generation assertions and verify gate**

After `evaluation.stats` in `scripts/build-wastewater-model.mjs`, call:

```js
const budget = validateProjectStats({
  effectiveTriangles: stats.effectiveTriangles,
  uniqueTriangles: stats.uniqueTriangles,
  expandedNodes: stats.expandedNodes,
  instances: plan.operations.filter(({ type }) => type === 'instance.create').length,
}, WWTP_STATS_LIMITS);
if (!budget.ok) {
  throw new Error(budget.issues.map(({ message }) => message).join('; '));
}
```

Add this step to `scripts/verify.mjs` before the large reference checks:

```js
{
  label: 'Wastewater plant asset contract',
  command: 'node',
  args: ['--test', 'scripts/tests/wastewater-plant-model.test.mjs'],
},
```

- [x] **Step 4: Run the focused test and verify manifest**

Run:

```bash
node --test scripts/tests/wastewater-plant-model.test.mjs
node --check scripts/verify.mjs
node --check scripts/build-wastewater-model.mjs
```

Expected: PASS.

- [x] **Step 5: Commit**

```bash
git add scripts/verify.mjs scripts/build-wastewater-model.mjs \
  scripts/tests/wastewater-plant-model.test.mjs
git commit -m "test(wwtp): gate asset generation and verification"
```

---

## Task 10: Regenerate, Inspect and Clean Up

**Files:**
- Modify: `projects/wastewater-plant/**`
- Modify: `projects/README.md`
- Modify: `docs/README.md`

**Interfaces:**
- Consumes: complete generator and validation pipeline.
- Produces: updated persistent project, documented scale and final visual proofs.

- [x] **Step 1: Regenerate the project**

Run:

```bash
node scripts/build-wastewater-model.mjs --workspace ./projects
```

Expected: the command exits 0, reports 300,000 to 500,000 effective triangles, no validation
errors, and replaces `projects/wastewater-plant` as one complete atomic project.

- [x] **Step 2: Verify the generated project through the API**

Start the project:

```bash
STUDIO_SESSION_TOKEN=wastewater-review \
STUDIO_VIEWER_DIR="$PWD/apps/viewer/dist" \
./target/release/studio-cli serve \
  --workspace ./projects \
  --project wastewater-plant \
  --port 8150 \
  --write-mode review-required
```

Query:

```bash
curl -fsS -X POST \
  -H 'content-type: application/json' \
  -H 'authorization: Bearer wastewater-review' \
  -d '{"jsonrpc":"2.0","id":1,"method":"evaluation.stats","params":{}}' \
  http://127.0.0.1:8150/rpc
```

Expected: `effectiveTriangles` is between 300000 and 500000, `uniqueTriangles` is at most 150000,
and the response contains no error.

- [x] **Step 3: Capture Chrome visual acceptance**

In system Google Chrome, inspect these URLs:

```text
http://127.0.0.1:8150/?token=wastewater-review
http://127.0.0.1:8150/?token=wastewater-review&view=35,32,260&target=220,0,10
http://127.0.0.1:8150/?token=wastewater-review&view=28,24,210&target=150,0,140
http://127.0.0.1:8150/?token=wastewater-review&view=22,28,230&target=420,0,70
http://127.0.0.1:8150/?token=wastewater-review&view=40,25,220&target=320,0,-125
http://127.0.0.1:8150/?token=wastewater-review&view=35,20,180&target=470,0,-10
```

Capture only two final images:

```bash
# Save the full-plant viewport.
output/wwtp-after-optimization.png

# Save the biological-treatment close-up.
output/wwtp-detail-biological.png
```

Acceptance requires zero console errors, zero failed requests, readable flow direction, visible
pipes and fittings, no obvious floating geometry, no black faces, and a clear distinction between
concrete, metal, water, roads and landscape.

- [x] **Step 4: Update docs and run the complete suite**

Update `projects/README.md` with final component, instance, connection, material and triangle
counts. Update `docs/README.md`:

```markdown
| `ICE3-STUDIO-SPEC-012` | [工程评审级污水处理厂资产](specs/2026-11-01-realistic-wastewater-plant-asset-design.md) | Implemented | 原创通用厂区、真实构筑物、管线、材质、场地与视觉验收 |
| `ICE3-STUDIO-PLAN-015` | [工程评审级污水处理厂资产实施计划](plans/2026-11-01-realistic-wastewater-plant-asset.md) | Implemented | 工程化拆分、拓扑校验、细节组件、管线支撑、材质与 Chrome 验收 |
```

Run:

```bash
npm run verify
```

Expected: every verification gate passes.

- [x] **Step 5: Clean temporary artifacts and commit**

Confirm no stray worktree files remain:

```bash
git status --short
find . -maxdepth 2 -type d -name '.playwright-cli' -o -name 'test-results'
rg --files scripts/wastewater-plant projects/wastewater-plant |
  rg '\.(png|jpe?g|webp|gif)$' || true
```

Only the persistent project, the two approved screenshots, source modules, tests and docs may
remain as intentional changes. The final media scan must return no image files inside the source
module or generated project. Remove temporary browser state and intermediate screenshots without
touching source or user-owned outputs.

```bash
git add scripts/build-wastewater-model.mjs scripts/wastewater-plant \
  scripts/tests/wastewater-plant-model.test.mjs scripts/verify.mjs \
  projects/wastewater-plant projects/README.md docs/README.md
git commit -m "feat(wwtp): deliver engineering-grade treatment plant asset"
```

`output/` 按仓库规则保持 Git 忽略；两张最终截图作为本地验收证据保留，不强制加入版本库。
