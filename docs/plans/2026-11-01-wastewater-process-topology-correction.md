# Wastewater Process Topology Correction Implementation Plan

> Document ID: ICE3-STUDIO-PLAN-016
>
> Status: Historical
>
> Owner: ICE3 Studio maintainers

> **历史状态：** 本文保留已完成实施过程的审计记录，不代表当前生产依赖；当前能力以
> [Asset Pipeline](../reference/asset-pipeline.md) 和 [能力清单](../reference/capabilities.md)
> 为准。

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [x]`) syntax for tracking.

**Goal:** 通过 Studio 类型化 Operation 补全污水处理厂案例的初沉污泥、RAS、二沉池分流、
沼气、泥饼输送和生化池设备拓扑，并通过 Studio API 重建可运行的最新资产。

**Architecture:** 保持 Studio Core 的数据边界不变，本次只修改案例资产源码、资产业务校验
和生成入口。资产通过 `project.create` 与 `document.apply` 进入 Studio；Document 仍是唯一
真源，GLB、bounds、诊断和 Viewer 场景都从新 revision 派生。

**Tech Stack:** Node.js 24.22.1、JavaScript ESM、Node Test、Rust Studio Core/Server、
JSON-RPC、React Viewer、Playwright、Google Chrome

**Spec:** `docs/specs/2026-11-01-wastewater-process-piping-correction-design.md`

## Global Constraints

- 使用 Node.js 24.22.1 LTS 和 npm 11 或更高版本。
- Studio Core 不增加污水处理业务术语或案例专用类型。
- Document、Operation history 和 ChangeSet 是唯一可持久修改路径。
- 不直接编辑派生 GLB、Viewer 场景对象、revision 快照或 cache 文件。
- 所有生产行为先写失败测试，再写最小实现。
- Git author 和 committer 必须为 `damoqiongqiu <damoqiongqiu@126.com>`。
- 不推送远端；当前任务只产生本地提交。
- 新增资产源码只进入 `scripts/wastewater-plant/`，测试只进入现有 `scripts/tests/`。
- 生成工程必须恢复 300,000 至 500,000 有效三角形、最多 150,000 唯一三角形、
  15,000 展开节点、500 实例和 48 材质。
- 不增加 Studio Core、Viewer 或 Toolchain 的运行时依赖。
- 截图只写现有 `output/`，临时浏览器状态在交付前清理。

## Review Focus

- 工艺图缺少 RAS、初沉污泥或沼气链路时，构建必须失败，不能发布“看起来完整”的厂区。
- 同一普通端口被多条连接复用、端口介质不匹配或连接直径失衡时，必须产生具体诊断。
- 厌氧和缺氧池含曝气扩散器、好氧池缺少曝气系统时，资产契约测试必须失败。
- 集合井、分流井、RAS 泵站和气体节点新增后，节点 ID、端口方向和实例世界坐标必须稳定。
- 重建失败时旧 revision 必须保持不变；重建成功后 Viewer 必须加载新 revision，不能继续
  缓存旧工程。

---

## File Structure

### New files

- `scripts/wastewater-plant/process-nodes.mjs`：分流井、集泥井、RAS 泵站、气体节点、
  火炬和泥饼输送机的参数化几何。
- `docs/plans/2026-11-01-wastewater-process-topology-correction.md`：本实施计划。

### Modified files

- `scripts/wastewater-plant/components.mjs`：按厌氧、缺氧、好氧模式生成生化池设备。
- `scripts/wastewater-plant/site-plan.mjs`：组件定义、端口、站点节点和完整连接图。
- `scripts/wastewater-plant/model-kit.mjs`：新增介质材质映射和材料搬运关系 helper。
- `scripts/wastewater-plant/validate.mjs`：工艺可达性、端口和介质完整性校验。
- `scripts/tests/wastewater-plant-model.test.mjs`：失败关闭的资产业务测试。
- `scripts/build-wastewater-model.mjs`：保持 API 重建入口，必要时输出工艺摘要。
- `projects/wastewater-plant/**`：由 Studio API 重新生成的工程真源。
- `projects/README.md`：更新工艺链接、统计和已知限制。
- `docs/README.md`：计划完成后将 SPEC-013 与 PLAN-016 标为 Implemented。

---

### Task 1: Add Generic Process Reachability Validation

**Files:**
- Modify: `scripts/wastewater-plant/validate.mjs`
- Test: `scripts/tests/wastewater-plant-model.test.mjs`

**Interfaces:**
- Consumes: `plan.operations` containing `instance.create`, `relation.create`,
  `connection.create`, and `node.create` operations.
- Produces:
  `validateProcessGraph(plan, requirements) => { ok: boolean, issues: Array<{ code: string, message: string }> }`
- Produces requirement shape:
  `{ code: string, fromTags: string[], toTags: string[], media: string[] }`.

- [x] **Step 1: Write failing reachability tests**

Add to `scripts/tests/wastewater-plant-model.test.mjs`:

```js
import {
  validateProcessGraph,
} from '../../scripts/wastewater-plant/validate.mjs';

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
```

- [x] **Step 2: Run the focused test and verify RED**

Run:

```bash
node --test --test-name-pattern="process graph validation" \
  scripts/tests/wastewater-plant-model.test.mjs
```

Expected: FAIL because `validateProcessGraph` is not exported.

- [x] **Step 3: Implement bounded graph reachability**

Add to `scripts/wastewater-plant/validate.mjs`:

```js
export function validateProcessGraph(plan, requirements) {
  const tagByNode = new Map();
  for (const operation of plan.operations) {
    if (operation.type !== 'instance.create') continue;
    const tag = operation.metadata?.tag;
    if (typeof tag === 'string') tagByNode.set(operation.node_id, tag);
  }
  const edges = plan.operations
    .filter(({ type }) => type === 'connection.create')
    .map(({ connection, name }) => ({
      from: connection.from.nodeId,
      to: connection.to.nodeId,
      medium: connection.medium,
      name,
    }));
  const issues = [];
  for (const requirement of requirements) {
    const starts = [...tagByNode]
      .filter(([, tag]) => requirement.fromTags.includes(tag))
      .map(([nodeId]) => nodeId);
    const reached = new Set(starts);
    const queue = [...starts];
    while (queue.length > 0) {
      const current = queue.shift();
      for (const edge of edges) {
        if (edge.from !== current) continue;
        if (!requirement.media.includes(edge.medium)) continue;
        if (reached.has(edge.to)) continue;
        reached.add(edge.to);
        queue.push(edge.to);
      }
    }
    const targets = [...tagByNode]
      .filter(([nodeId, tag]) => (
        requirement.toTags.includes(tag) && reached.has(nodeId)
      ))
      .map(([, tag]) => tag);
    const missing = requirement.toTags.filter((tag) => !targets.includes(tag));
    if (missing.length > 0) {
      issues.push({
        code: requirement.code,
        message: `${requirement.fromTags.join(',')} cannot reach ` +
          `${missing.join(',')} via ${requirement.media.join(',')}`,
      });
    }
  }
  return { ok: issues.length === 0, issues };
}
```

- [x] **Step 4: Run the focused and complete asset tests**

Run:

```bash
node --test scripts/tests/wastewater-plant-model.test.mjs
```

Expected: PASS 28 tests.

- [x] **Step 5: Commit**

```bash
git add scripts/wastewater-plant/validate.mjs \
  scripts/tests/wastewater-plant-model.test.mjs
git commit -m "test(wwtp): validate process media reachability"
```

---

### Task 2: Split Anaerobic, Anoxic, and Aerobic Basin Components

**Files:**
- Modify: `scripts/wastewater-plant/components.mjs`
- Modify: `scripts/wastewater-plant/site-plan.mjs`
- Test: `scripts/tests/wastewater-plant-model.test.mjs`

**Interfaces:**
- Consumes: `aerationBasin(b, prefix, options)`.
- Produces:
  `aerationBasin(b, prefix, { mode: 'anaerobic' | 'anoxic' | 'aerobic', lanes, airLines, posts })`.
- Produces component keys:
  `anaerobic-basin`, `anoxic-basin`, `aerobic-basin`.

- [x] **Step 1: Write failing basin-mode tests**

Add:

```js
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
```

- [x] **Step 2: Run the focused test and verify RED**

Run:

```bash
node --test --test-name-pattern="process-correct equipment" \
  scripts/tests/wastewater-plant-model.test.mjs
```

Expected: FAIL because the three process-correct definitions do not exist.

- [x] **Step 3: Add mode-specific equipment**

Change `aerationBasin` to accept the `mode` option and gate the existing air block:

```js
function aerationBasin(
  b,
  prefix,
  { lanes = 3, airLines = 30, posts = 11, mode = 'aerobic' } = {},
) {
  // Existing slab, walls, baffles, water, walkways and guardrails stay shared.
  const parts = [];
  // ...unchanged common parts...

  if (mode === 'aerobic') {
    // Existing air-header, air-drop and air-diffuser loop.
  } else {
    for (const lane of [-1, 0, 1]) {
      parts.push(
        b.part(
          `${prefix}-mixer-column-${lane + 2}`,
          b.cylinder(0.22, 4.2, 16),
          { expr: 'length / 2 - 4' },
          2.4,
          { expr: `${lane} * width / 3` },
          'stainless',
        ),
        b.part(
          `${prefix}-mixer-propeller-${lane + 2}`,
          b.cylinder(0.85, 0.32, 18),
          { expr: 'length / 2 - 4' },
          0.9,
          { expr: `${lane} * width / 3` },
          'steel-dark',
        ),
      );
    }
  }
  return parts;
}
```

Define the three components with mode-specific names and ports:

```js
b.define('anaerobic-basin', 'Anaerobic basin',
  aerationBasin(b, 'ae', { mode: 'anaerobic' }), [
    b.port('inlet', 'ae-wall-w', -21, 4.0, 0, vec(1, 0, 0), 'settled-water', 0.9),
    b.port('outlet', 'ae-wall-e', 21, 4.0, 0, vec(1, 0, 0), 'mixed-liquor', 0.9),
  ], basinParameters);

b.define('anoxic-basin', 'Anoxic basin',
  aerationBasin(b, 'ae', { mode: 'anoxic' }), [
    b.port('inlet', 'ae-wall-w', -21, 4.0, 0, vec(1, 0, 0), 'mixed-liquor', 0.9),
    b.port('return-inlet', 'ae-wall-n', 5, 4.0, -10, vec(0, 0, 1), 'return-sludge', 0.35),
    b.port('outlet', 'ae-wall-e', 21, 4.0, 0, vec(1, 0, 0), 'mixed-liquor', 0.9),
  ], basinParameters);

b.define('aerobic-basin', 'Aerobic basin',
  aerationBasin(b, 'ae', { mode: 'aerobic' }), [
    b.port('inlet', 'ae-wall-w', -21, 4.0, 0, vec(1, 0, 0), 'mixed-liquor', 0.9),
    b.port('outlet', 'ae-wall-e', 21, 4.0, 0, vec(1, 0, 0), 'mixed-liquor', 0.9),
    b.port('air-in', 'ae-wall-n', 0, 4.6, -10, vec(0, 0, -1), 'air', 0.8),
  ], basinParameters);
```

Use `anaerobic-basin` for AE-x01, `anoxic-basin` for AE-x02, and
`aerobic-basin` for AE-x03.

- [x] **Step 4: Run asset tests**

Run:

```bash
node --test scripts/tests/wastewater-plant-model.test.mjs
```

Expected: PASS; existing tests may be updated only where the old component names
were an implementation detail.

- [x] **Step 5: Commit**

```bash
git add scripts/wastewater-plant/components.mjs \
  scripts/wastewater-plant/site-plan.mjs \
  scripts/tests/wastewater-plant-model.test.mjs
git commit -m "refactor(wwtp): separate biological basin modes"
```

---

### Task 3: Add Process Nodes and Complete Port Graph

**Files:**
- Create: `scripts/wastewater-plant/process-nodes.mjs`
- Modify: `scripts/wastewater-plant/site-plan.mjs`
- Modify: `scripts/wastewater-plant/validate.mjs`
- Test: `scripts/tests/wastewater-plant-model.test.mjs`

**Interfaces:**
- Produces component factories:
  `distributionBox`, `sludgeCollectionWell`, `rasPumpStation`, `gasCollector`,
  `gasFlare`, and `cakeConveyor`.
- Produces definition keys:
  `distribution-box`, `sludge-collection-well`, `ras-pump-station`, `gas-collector`,
  `gas-flare`, and `cake-conveyor`.
- Produces stable instance keys:
  `dd-g`, `dd-a`, `dd-b`, `fb-1`, `scw-1`, `rws-1`, `gcl-1`, `gf-1`,
  `cvc-1`, `cvc-2`.
- Produces `requiredProcessGraph` in `site-plan.mjs`.

- [x] **Step 1: Write failing complete-topology tests**

Add:

```js
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
```

- [x] **Step 2: Run the focused test and verify RED**

Run:

```bash
node --test --test-name-pattern="complete sludge" \
  scripts/tests/wastewater-plant-model.test.mjs
```

Expected: FAIL because process nodes and required connections do not exist.

- [x] **Step 3: Implement process node geometry**

Create `scripts/wastewater-plant/process-nodes.mjs`:

```js
import { box, cylinder } from './model-kit.mjs';

export function distributionBox(b, prefix, { length = 8, width = 6, depth = 2.6 } = {}) {
  return [
    b.part(`${prefix}-base`, box(length, 0.4, width), 0, 0.2, 0, 'concrete-old'),
    b.part(`${prefix}-wall-n`, box(length, depth, 0.35), 0, 0.5 + depth / 2, -width / 2, 'concrete-new'),
    b.part(`${prefix}-wall-s`, box(length, depth, 0.35), 0, 0.5 + depth / 2, width / 2, 'concrete-new'),
    b.part(`${prefix}-wall-w`, box(0.35, depth, width), -length / 2, 0.5 + depth / 2, 0, 'concrete-new'),
    b.part(`${prefix}-wall-e`, box(0.35, depth, width), length / 2, 0.5 + depth / 2, 0, 'concrete-new'),
    b.part(`${prefix}-cover`, box(length - 0.7, 0.12, width - 0.7), 0, 0.58 + depth, 0, 'walkway'),
  ];
}

export function sludgeCollectionWell(b, prefix, { radius = 2.6, depth = 3.2 } = {}) {
  return [
    b.part(`${prefix}-shell`, cylinder(radius, depth, 28), 0, depth / 2, 0, 'concrete-new'),
    b.part(`${prefix}-cover`, cylinder(radius + 0.15, 0.16, 28), 0, depth + 0.08, 0, 'steel-dark'),
  ];
}

export function rasPumpStation(b, prefix, { length = 18, width = 10, height = 4.8 } = {}) {
  return [
    b.part(`${prefix}-slab`, box(length + 1, 0.5, width + 1), 0, 0.25, 0, 'concrete'),
    b.part(`${prefix}-wall-n`, box(length, height, 0.4), 0, 0.5 + height / 2, -width / 2, 'concrete-new'),
    b.part(`${prefix}-wall-s`, box(length, height, 0.4), 0, 0.5 + height / 2, width / 2, 'concrete-new'),
    b.part(`${prefix}-pump-a`, cylinder(0.9, 1.8, 20), -4, 1.4, 0, 'stainless'),
    b.part(`${prefix}-pump-b`, cylinder(0.9, 1.8, 20), 0, 1.4, 0, 'stainless'),
    b.part(`${prefix}-pump-c`, cylinder(0.9, 1.8, 20), 4, 1.4, 0, 'stainless'),
  ];
}

export function gasCollector(b, prefix, { length = 10 } = {}) {
  return [
    b.part(`${prefix}-vessel`, cylinder(0.7, length, 20), 0, 3.0, 0, 'stainless'),
    b.part(`${prefix}-support-a`, box(0.25, 3, 0.25), -3.5, 1.5, 0, 'steel-dark'),
    b.part(`${prefix}-support-b`, box(0.25, 3, 0.25), 3.5, 1.5, 0, 'steel-dark'),
  ];
}

export function gasFlare(b, prefix, { height = 12 } = {}) {
  return [
    b.part(`${prefix}-stack`, cylinder(0.35, height, 20), 0, height / 2, 0, 'steel-dark'),
    b.part(`${prefix}-base`, cylinder(1.3, 0.4, 24), 0, 0.2, 0, 'concrete'),
  ];
}

export function cakeConveyor(b, prefix, { length = 14 } = {}) {
  return [
    b.part(`${prefix}-trough`, box(length, 0.5, 0.9), 0, 1.2, 0, 'steel'),
    b.part(`${prefix}-support-a`, box(0.2, 1.2, 0.6), -length / 3, 0.6, 0, 'steel-dark'),
    b.part(`${prefix}-support-b`, box(0.2, 1.2, 0.6), length / 3, 0.6, 0, 'steel-dark'),
  ];
}
```

- [x] **Step 4: Wire explicit process ports and connections**

Add a `relate` helper before using material handling:

```js
const relate = (fromKey, toKey, relationType, metadata = {}) => {
  const from = state.nodes.get(fromKey);
  const to = state.nodes.get(toKey);
  if (!from || !to) throw new Error(`missing relation endpoint ${fromKey} -> ${toKey}`);
  state.ops.push({
    type: 'relation.create',
    relation: {
      id: id('relation'),
      relationType,
      fromNodeId: from,
      toNodeId: to,
      metadata,
    },
  });
};
```

Define ports:

```js
// Primary clarifier
b.port('primary-sludge', 'pc-base', 0, 0.3, 0, vec(0, -1, 0), 'primary-sludge', 0.3)

// Secondary clarifier
b.port('return-sludge', 'sc-base', -3, 0.3, 0, vec(0, -1, 0), 'return-sludge', 0.3),
b.port('waste-sludge', 'sc-base', 3, 0.3, 0, vec(0, -1, 0), 'waste-sludge', 0.3)

// Digester
b.port('gas-out', 'dg-dome', 0, 18.5, 0, vec(0, 1, 0), 'biogas', 0.35)

// Gas holder
b.port('gas-in', 'gh-shell', -4, 5.5, 0, vec(-1, 0, 0), 'biogas', 0.35),
b.port('gas-out', 'gh-shell', 4, 5.5, 0, vec(1, 0, 0), 'biogas', 0.35)
```

Create the two distribution boxes and route each train through one:

```js
b.instance('distribution-box', `dd-${key}`, `DD-${series}01 二沉分流井`,
  366, 0, z, { stage: 'secondary', tag: `DD-${series}01` });
b.connect(`ae-${key}3`, 'outlet', `dd-${key}`, 'inlet',
  `AE-${series}03 → DD-${series}01`, 'mixed-liquor', 0.9);
for (let index = 1; index <= 3; index += 1) {
  b.connect(`dd-${key}`, `outlet-${index}`, `sc-${key}${index}`, 'inlet',
    `DD-${series}01 → SC-${series}0${index}`, 'mixed-liquor', 0.9);
}
```

Create one RAS station with six inlets and two outlets, then connect each secondary
clarifier and anoxic basin through it.

Create one sludge collection well fed by both primary clarifiers and connected to
the thickener route. Create the gas collector, two digester gas feeds, holder feed,
and holder-to-flare connection. Replace cake pipe connections with two conveyor
instances and `material-handling` relations.

Add a grit distribution box `dd-g` with one inlet and two outlets, and a filter
collection box `fb-1` with six clarifier inlets and one outlet. Update water-side
connections so no ordinary process port is reused.

- [x] **Step 5: Add complete process requirements and fail closed**

Export and pass:

```js
const requiredProcessGraph = [
  { code: 'MISSING_PRIMARY_SLUDGE', fromTags: ['PC-101', 'PC-201'], toTags: ['TH-101', 'TH-102'], media: ['primary-sludge'] },
  { code: 'MISSING_RAS', fromTags: ['SC-101', 'SC-102', 'SC-103', 'SC-201', 'SC-202', 'SC-203'], toTags: ['AE-102', 'AE-202'], media: ['return-sludge'] },
  { code: 'MISSING_GAS', fromTags: ['DG-101', 'DG-102'], toTags: ['GH-101'], media: ['biogas'] },
  { code: 'MISSING_DIGESTED_SLUDGE', fromTags: ['DG-101', 'DG-102'], toTags: ['DW-101-C1', 'DW-101-C2'], media: ['digested-sludge'] },
];
const graph = validateProcessGraph(plan, requiredProcessGraph);
if (!graph.ok) throw new Error(graph.issues.map(({ code, message }) => `${code}: ${message}`).join('; '));
```

- [x] **Step 6: Run asset tests**

Run:

```bash
node --test scripts/tests/wastewater-plant-model.test.mjs
```

Expected: PASS with the complete topology test and all earlier tests.

- [x] **Step 7: Commit**

```bash
git add scripts/wastewater-plant/process-nodes.mjs \
  scripts/wastewater-plant/site-plan.mjs \
  scripts/wastewater-plant/model-kit.mjs \
  scripts/wastewater-plant/validate.mjs \
  scripts/tests/wastewater-plant-model.test.mjs
git commit -m "feat(wwtp): complete process topology"
```

---

### Task 4: Correct Material Semantics and Port Usage

**Files:**
- Modify: `scripts/wastewater-plant/model-kit.mjs`
- Modify: `scripts/wastewater-plant/site-plan.mjs`
- Modify: `scripts/wastewater-plant/validate.mjs`
- Test: `scripts/tests/wastewater-plant-model.test.mjs`

**Interfaces:**
- Consumes: `PIPE_MATERIAL` and connection operations from Task 3.
- Produces:
  `validateConnectionPorts(plan) => { ok: boolean, issues: Array<{ code: string, message: string }> }`.

- [x] **Step 1: Write failing material and port-usage tests**

Extend the validator import:

```js
import {
  validateConnectionPorts,
} from '../../scripts/wastewater-plant/validate.mjs';
```

Add:

```js
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
  const result = validateConnectionPorts(plan);
  assert.equal(result.ok, true);
  assert.deepEqual(result.issues, []);
});
```

Add a second mutation test that duplicates a connection from a non-header port and
expects `PORT_REUSED`.

Add a third mutation test that changes a connection diameter to `20` and expects
`PIPE_DIAMETER_MISMATCH`.

- [x] **Step 2: Run focused tests and verify RED**

Run:

```bash
node --test --test-name-pattern="explicit material" \
  scripts/tests/wastewater-plant-model.test.mjs
```

Expected: FAIL because media/material mappings and port validator are missing.

- [x] **Step 3: Extend material mapping and port validation**

Add explicit materials in `site-plan.mjs` and map every medium:

```js
const PIPE_MATERIAL = {
  'primary-sludge': 'pipe-primary-sludge',
  'return-sludge': 'pipe-return-sludge',
  'waste-sludge': 'pipe-sludge',
  'thickened-sludge': 'pipe-sludge',
  'digested-sludge': 'pipe-sludge',
  biogas: 'pipe-biogas',
  chemical: 'pipe-chemical',
  // Retain the existing water and air mappings.
};
```

Implement:

```js
export function validateConnectionPorts(plan) {
  const issues = [];
  const usage = new Map();
  const definitions = new Map(
    plan.operations
      .filter(({ type }) => type === 'component.define')
      .map(({ definition }) => [definition.id, definition]),
  );
  const definitionByNode = new Map(
    plan.operations
      .filter(({ type }) => type === 'instance.create')
      .map((operation) => [
        operation.node_id,
        operation.instance.definitionId,
      ]),
  );
  for (const operation of plan.operations) {
    if (operation.type !== 'connection.create') continue;
    for (const endpoint of [operation.connection.from, operation.connection.to]) {
      const key = `${endpoint.nodeId}/${endpoint.port}`;
      usage.set(key, (usage.get(key) ?? 0) + 1);
    }
  }
  const headerPorts = new Set(
    plan.operations
      .filter(({ type }) => type === 'component.define')
      .flatMap(({ definition }) => (
        definition.ports.map(({ name, metadata }) => (
          metadata?.allowMultiple
            ? `${definition.id}/${name}`
            : null
        ))
      ))
      .filter(Boolean),
  );
  for (const [key, count] of usage) {
    const separator = key.indexOf('/');
    const nodeId = key.slice(0, separator);
    const portName = key.slice(separator + 1);
    const definitionId = definitionByNode.get(nodeId);
    const portKey = `${definitionId}/${portName}`;
    if (count > 1 && !headerPorts.has(portKey)) {
      issues.push({
        code: 'PORT_REUSED',
        message: `${key} is used by ${count} connections`,
      });
    }
  }
  for (const operation of plan.operations) {
    if (operation.type !== 'connection.create') continue;
    const connection = operation.connection;
    for (const endpoint of [connection.from, connection.to]) {
      const definitionId = definitionByNode.get(endpoint.nodeId);
      const port = definitions.get(definitionId)?.ports
        .find(({ name }) => name === endpoint.port);
      if (!port) continue;
      const ratio = connection.diameter / port.diameter;
      if (ratio < 0.8 || ratio > 1.2) {
        issues.push({
          code: 'PIPE_DIAMETER_MISMATCH',
          message: `${operation.name}: ${endpoint.nodeId}/${endpoint.port} ` +
            `diameter ${connection.diameter} ` +
            `does not match port diameter ${port.diameter}`,
        });
      }
    }
  }
  return { ok: issues.length === 0, issues };
}
```

Because `b.port` does not yet accept metadata, extend its final signature to
`port(name, partId, x, y, z, direction, medium, diameter, metadata = null)` and
include `metadata` only when provided. Header/collection ports that intentionally
accept multiple branches must set `metadata.allowMultiple = true`.

- [x] **Step 4: Run tests**

Run:

```bash
node --test scripts/tests/wastewater-plant-model.test.mjs
```

Expected: PASS.

- [x] **Step 5: Commit**

```bash
git add scripts/wastewater-plant/model-kit.mjs \
  scripts/wastewater-plant/site-plan.mjs \
  scripts/wastewater-plant/validate.mjs \
  scripts/tests/wastewater-plant-model.test.mjs
git commit -m "fix(wwtp): validate media and port usage"
```

---

### Task 5: Rebuild Through Studio API and Visually Verify

**Files:**
- Generated: `projects/wastewater-plant/**`
- Modify: `projects/README.md`
- Modify: `docs/README.md`
- Output: `output/wwtp-topology-overview.png`
- Output: `output/wwtp-topology-ras-close.png`
- Output: `output/wwtp-topology-sludge-close.png`

**Interfaces:**
- Consumes: `buildPlan('wastewater-plant')` and the existing API bootstrap script.
- Produces: a new Revision 1 project generated by `project.create` and
  `document.apply`, plus visual acceptance screenshots.

- [x] **Step 1: Regenerate the project through Studio**

Run:

```bash
node scripts/build-wastewater-model.mjs --workspace ./projects
```

Expected: `revision: 1`, no diagnostics, all process graph requirements pass, and
`projects/wastewater-plant/**` is rewritten deterministically.

- [x] **Step 2: Verify the active project through the RPC API**

Restart the local Studio server, then verify:

```bash
curl -s -X POST -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"system.health","params":{}}' \
  http://127.0.0.1:8150/rpc
```

Expected: `status: "ok"`.

Query `scene.context` and confirm the new component, instance and connection counts,
the configured bounds, and revision 1. Query `evaluation.stats` and confirm the
budget remains valid.

- [x] **Step 3: Capture the three Chrome acceptance views**

Use the existing screenshot script with fixed targets around:

```text
overview:    view=20,34,540
RAS:         target=390,1,0 view=32,30,125
sludge/gas:  target=350,1,-125 view=30,30,125
```

Save the screenshots at the exact paths listed in this task. Expected:

- No black viewport.
- No empty response or console error.
- RAS pump station visibly connects clarifiers and anoxic basins.
- Gas collector connects digesters to holder and flare.
- No dewatered-cake pipe sweep.

- [x] **Step 4: Update generated statistics and status**

Update `projects/README.md` with final component, instance, connection, material,
node and triangle counts and the corrected process description. In `docs/README.md`,
change both SPEC-013 and PLAN-016 to `Implemented` only after every gate passes.

- [x] **Step 5: Run complete verification**

Run:

```bash
npm run verify
```

Expected: all Studio gates pass.

- [x] **Step 6: Commit**

```bash
git add projects/wastewater-plant projects/README.md \
  docs/README.md output/wwtp-topology-overview.png \
  output/wwtp-topology-ras-close.png \
  output/wwtp-topology-sludge-close.png
git commit -m "feat(wwtp): rebuild correct process topology"
```

---

### Task 6: Final Review of Process Semantics

**Files:**
- Read: `scripts/wastewater-plant/**`
- Read: `projects/README.md`
- Read: `docs/specs/2026-11-01-wastewater-process-piping-correction-design.md`
- Test: `scripts/tests/wastewater-plant-model.test.mjs`

**Interfaces:**
- Consumes: the completed asset and its validation report.
- Produces: a review result that names any remaining Stage 1 defect before Stage 2.

- [x] **Step 1: Run the full asset suite again from a clean process**

Run:

```bash
node --test scripts/tests/wastewater-plant-model.test.mjs
npm run verify
```

Expected: PASS.

- [x] **Step 2: Audit the process graph independently**

List every connection by service, medium and endpoint. Confirm:

- both primary clarifiers reach a sludge destination;
- all six secondary clarifiers have RAS and WAS paths;
- RAS reaches both anoxic stages;
- each aerobic outlet reaches three clarifiers through a distribution box;
- both digesters reach the gas holder;
- the holder reaches flare;
- no cake material uses `connection.create`;
- no non-header port has multiple connections.

- [x] **Step 3: Review the three screenshots at full resolution**

Reject the stage if any required process node is invisible, floating, intersecting
the wrong structure, or connected by a visibly disconnected pipe.

- [x] **Step 4: Record any remaining Stage 2-only issue**

If visual pipe corners still look angular but topology is correct, record them as
Stage 2 work rather than hiding them with extra decorative geometry.

- [x] **Step 5: Commit any review-only correction**

```bash
git add scripts/wastewater-plant scripts/tests/wastewater-plant-model.test.mjs \
  projects projects/README.md docs/README.md output
git commit -m "fix(wwtp): close process topology review"
```

Only create this commit when the review found and fixed a concrete defect. Do not
create an empty review commit.

---

## Stage 2 交接

本计划只交付 SPEC-013 的阶段一。阶段二另写独立实施计划，覆盖 Core 连续弯头、路径
碰撞、支吊架和配件对齐。阶段一全部验收通过前，不开始阶段二。
