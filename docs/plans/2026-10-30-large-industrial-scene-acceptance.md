# Large Industrial Scene Acceptance Implementation Plan

> Document ID: ICE3-STUDIO-PLAN-007
>
> Status: Historical
>
> Owner: ICE3 Studio maintainers

> **历史状态：** 本文保留已完成实施过程的审计记录，不代表当前生产依赖；当前能力以
> [Asset Pipeline](../reference/asset-pipeline.md) 和 [能力清单](../reference/capabilities.md)
> 为准。

> **For agentic workers:** REQUIRED SUB-SKILL: Use `executing-plans` to implement this plan
> task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a deterministic 5.5M–7.5M-triangle generic industrial scene through the public
Studio API, then verify object scale, topology, shared geometry, incremental cache behavior,
source GLB export, and measurable performance.

**Architecture:** A standalone deterministic AI-client fixture composes high-level Component,
Instance, InstanceSet, and Connection Operations from a generic equipment catalogue. Studio Core
remains the source of truth and evaluator; a new lightweight `evaluation.stats` API exposes derived
counts without transferring the full 5M-triangle payload to the benchmark.

**Tech Stack:** Rust 1.98, serde, schemars, ts-rs, SHA-256, glam; Node.js 24.22.1 with ESM and
`node:test`; JSON-RPC; Playwright; Toolchain sidecar.

**Spec:** [`docs/specs/2026-10-30-large-scene-authoring-design.md`](../specs/2026-10-30-large-scene-authoring-design.md)

## 1. Global Constraints

- Build the acceptance scene through typed public Operations; do not write project snapshots directly.
- Keep Core and API vocabulary industry-neutral. Equipment names and areas belong to fixture metadata.
- Target 200–240 primary selectable equipment or facilities, including four primary structures,
  six process trains, and 10–12 secondary structures.
- Target 5,500,000–7,500,000 effective source triangles and approximately `1000 x 550` metres.
- Equivalent definitions and parameter sets must share one geometry cache entry and one GLB mesh.
- Do not create invisible geometry, random duplication, or meaningless subdivision to reach budgets.
- Do not commit generated GLBs, project caches, customer assets, benchmark outputs, or absolute paths.
- Use Node.js `24.22.1`; do not introduce Vite or Vitest.
- Maintain `damoqiongqiu` as Git author and committer.
- Every behavior change follows RED-GREEN-REFACTOR and ends in a focused commit.
- Root acceptance is `npm run verify`; the large benchmark is an additional release gate.

## 2. Review Focus

- A catalogue or layout change must not drift below 200 or above 240 primary objects, lose one of
  the four primary structures, vary from six process trains, or leave fewer than ten secondary
  structures; the fixture test must pin every count.
- An AI-facing plan must contain only public generic Operation names; a fixture accidentally using
  an internal or industry-specific operation must fail before contacting the server.
- Triangle totals must come from evaluated, visible geometry and must reject hidden/degenerate
  filler; the budget test must compare effective and unique triangles and assert mesh-sharing.
- Missing ports, media mismatches, unresolved set ordinals, and dangling topology must reject the
  whole candidate without changing the committed revision.
- Reordering catalog or map iteration must not alter IDs, operation order, source GLB digest, or
  cache keys; determinism tests must generate the plan twice and compare canonical output.

---

### 2.1 Task 1: Add a lightweight evaluated-scene statistics API

**Files:**
- Modify: `crates/studio-server/src/rpc.rs`
- Modify: `crates/studio-cli/src/schema.rs`
- Test: `crates/studio-server/tests/large_scene_rpc.rs`
- Test: `crates/studio-server/tests/rpc_contract.rs`

**Interfaces:**
- Consumes: `evaluate_session_scene(&Session)` and `LargeSceneStats`.
- Produces: JSON-RPC method `evaluation.stats`.
- Produces: public fields `sourceNodes`, `expandedNodes`, `uniqueMeshes`, `effectiveTriangles`,
  `uniqueTriangles`, `cacheHits`, `cacheMisses`, and `generatedMeshes`.

- [ ] **Step 1: Add a failing large-scene stats test**

Append to `crates/studio-server/tests/large_scene_rpc.rs`:

```rust
#[tokio::test]
async fn reports_large_scene_statistics_without_returning_geometry() {
    let server = start_test_server(ServerConfig::for_tests("large-stats")).await;
    apply_two_shared_instances(&server).await;

    let stats = rpc_with_bearer(
        &server,
        server.token(),
        "evaluation.stats",
        serde_json::json!({}),
    )
    .await;
    assert_eq!(stats["result"]["sourceNodes"], 2);
    assert_eq!(stats["result"]["expandedNodes"], 2);
    assert_eq!(stats["result"]["uniqueMeshes"], 1);
    assert_eq!(stats["result"]["effectiveTriangles"], 24);
    assert_eq!(stats["result"]["uniqueTriangles"], 12);
    assert!(stats["result"].get("vertices").is_none());
}
```

Extract the existing component/instance fixture in that test file into:

```rust
async fn apply_two_shared_instances(server: &TestServer) {
    let component = serde_json::json!({
        "id": "component-000001",
        "name": "Equipment",
        "parameters": [],
        "parts": [{
            "id": "body",
            "parentId": null,
            "transform": {
                "translation": { "x": 0, "y": 0, "z": 0 },
                "rotation": [0, 0, 0, 1],
                "scale": { "x": 1, "y": 1, "z": 1 }
            },
            "content": {
                "kind": "box",
                "width": 1,
                "height": 1,
                "depth": 1
            },
            "materialRole": null,
            "metadata": {},
            "visible": true
        }],
        "ports": [],
        "metadata": {}
    });
    let instance = serde_json::json!({
        "definitionId": "component-000001",
        "parameters": {},
        "overrides": {},
        "metadata": {}
    });
    let applied = rpc_with_bearer(
        server,
        server.token(),
        "document.apply",
        serde_json::json!({
            "expectedRevision": 0,
            "dryRun": false,
            "idempotencyKey": "large-stats",
            "operations": [
                { "type": "component.define", "definition": component },
                {
                    "type": "instance.create",
                    "node_id": "node-000001",
                    "parent_id": null,
                    "name": "Equipment A",
                    "transform": {
                        "translation": { "x": 0, "y": 0, "z": 0 },
                        "rotation": [0, 0, 0, 1],
                        "scale": { "x": 1, "y": 1, "z": 1 }
                    },
                    "instance": instance,
                    "material_id": null,
                    "metadata": {}
                },
                {
                    "type": "instance.create",
                    "node_id": "node-000002",
                    "parent_id": null,
                    "name": "Equipment B",
                    "transform": {
                        "translation": { "x": 4, "y": 0, "z": 0 },
                        "rotation": [0, 0, 0, 1],
                        "scale": { "x": 1, "y": 1, "z": 1 }
                    },
                    "instance": instance,
                    "material_id": null,
                    "metadata": {}
                }
            ]
        }),
    )
    .await;
    assert!(applied["result"]["revision"].is_number(), "{applied}");
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test -p studio-server --test large_scene_rpc reports_large_scene_statistics
```

Expected: FAIL because `evaluation.stats` is not implemented.

- [ ] **Step 3: Implement the stats method**

Add a serializable response type in `rpc.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EvaluatedSceneStats {
    pub source_nodes: usize,
    pub expanded_nodes: usize,
    pub unique_meshes: usize,
    pub effective_triangles: usize,
    pub unique_triangles: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub generated_meshes: usize,
}
```

Add this match arm beside `evaluation.evaluate`:

```rust
"evaluation.stats" => {
    let (evaluated, _) = evaluate_session_scene(session)?;
    serde_json::to_value(EvaluatedSceneStats {
        source_nodes: evaluated.stats.source_nodes,
        expanded_nodes: evaluated.stats.expanded_nodes,
        unique_meshes: evaluated.stats.unique_meshes,
        effective_triangles: evaluated.stats.effective_triangles,
        unique_triangles: evaluated.stats.unique_triangles,
        cache_hits: evaluated.stats.cache_hits,
        cache_misses: evaluated.stats.cache_misses,
        generated_meshes: evaluated.stats.generated_meshes,
    })
    .map_err(serialize_error)
}
```

Add `"evaluation.stats"` to `system.capabilities` and the schema method list. Add the response type
to `studio-cli` schema generation as `EvaluatedSceneStats`.

- [ ] **Step 4: Run the focused tests**

Run:

```bash
cargo test -p studio-server --test large_scene_rpc reports_large_scene_statistics
cargo test -p studio-server --test rpc_contract publishes_generated_api_schemas
cargo test -p studio-server
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-server/src/rpc.rs crates/studio-server/tests/large_scene_rpc.rs \
  crates/studio-server/tests/rpc_contract.rs crates/studio-cli/src/schema.rs
git commit -m "feat(studio-server): expose evaluated scene statistics"
```

---

### 2.2 Task 2: Define the generic fixture catalogue and scale invariants

**Files:**
- Create: `fixtures/large-industrial-site/catalog.mjs`
- Create: `scripts/tests/large-industrial-fixture.test.mjs`

**Interfaces:**
- Produces: `EQUIPMENT_GROUPS`, `PROCESS_TRAINS`, `AREAS`, `FACILITIES`, and
  `EXPECTED_SCENE`.
- Produces: `validateCatalogue(catalogue) -> { equipmentCount, primaryClarifiers,
  biologicalReactors, secondaryClarifiers, facilities, areas }`.
- Later tasks consume the exact group keys and counts.

- [ ] **Step 1: Write failing catalogue tests**

Create `scripts/tests/large-industrial-fixture.test.mjs`:

```javascript
import assert from 'node:assert/strict';
import test from 'node:test';
import {
  AREAS,
  EQUIPMENT_GROUPS,
  FACILITIES,
  PROCESS_TRAINS,
  validateCatalogue,
} from '../../fixtures/large-industrial-site/catalog.mjs';

test('catalogue preserves the accepted large-scene scale', () => {
  const counts = validateCatalogue({
    areas: AREAS,
    equipmentGroups: EQUIPMENT_GROUPS,
    facilities: FACILITIES,
    processTrains: PROCESS_TRAINS,
  });
  assert.equal(counts.equipmentCount + counts.facilities, 216);
  assert.equal(counts.primaryClarifiers, 4);
  assert.equal(counts.biologicalReactors, 18);
  assert.equal(counts.secondaryClarifiers, 12);
  assert.equal(counts.areas, 8);
  assert.equal(counts.facilities, 12);
  assert.equal(counts.processTrains, 6);
});

test('catalogue keys are unique and industry-neutral at the API boundary', () => {
  const keys = EQUIPMENT_GROUPS.map((group) => group.kind);
  assert.equal(new Set(keys).size, keys.length);
  assert.equal(keys.some((key) => /wastewater|sludge|clarifier/i.test(key)), false);
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
source $HOME/.nvm/nvm.sh && nvm use 24.22.1 >/dev/null
node --test scripts/tests/large-industrial-fixture.test.mjs
```

Expected: module-not-found failure.

- [ ] **Step 3: Implement the catalogue**

Create `fixtures/large-industrial-site/catalog.mjs` with:

```javascript
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

export function validateCatalogue({ areas, equipmentGroups, facilities, processTrains }) {
  const equipmentCount = equipmentGroups.reduce((sum, group) => sum + group.count, 0);
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
```

`biologicalReactors === 18` represents six trains with three generic reactor zones each.

- [ ] **Step 4: Run the tests**

Run:

```bash
node --test scripts/tests/large-industrial-fixture.test.mjs
```

Expected: PASS, 2/2.

- [ ] **Step 5: Commit**

```bash
git add fixtures/large-industrial-site/catalog.mjs scripts/tests/large-industrial-fixture.test.mjs
git commit -m "test(studio-fixture): define large industrial scene catalogue"
```

---

### 2.3 Task 3: Build reusable detailed equipment templates

**Files:**
- Create: `fixtures/large-industrial-site/templates.mjs`
- Create: `fixtures/large-industrial-site/geometry.mjs`
- Modify: `scripts/tests/large-industrial-fixture.test.mjs`

**Interfaces:**
- Consumes: catalogue `kind` keys.
- Produces: `buildComponentDefinitions() -> ComponentDefinition[]`.
- Produces: `createEquipmentMesh(kind, ordinal) -> TriangleMesh`.
- Produces: `TEMPLATE_TRIANGLE_BUDGETS: Record<string, number>`.

- [ ] **Step 1: Add failing template tests**

Append:

```javascript
import { TEMPLATE_TRIANGLE_BUDGETS, buildComponentDefinitions } from '../../fixtures/large-industrial-site/templates.mjs';

test('every equipment kind has a reusable bounded template', () => {
  const generated = buildComponentDefinitions();
  assert.equal(generated.length, EQUIPMENT_GROUPS.length + 1);
  for (const group of EQUIPMENT_GROUPS) {
    const definition = generated.find((candidate) => candidate.name === group.kind);
    assert.ok(definition, group.kind);
    assert.equal(definition.parts.length >= 8, true);
    assert.equal(definition.ports.length >= 2, true);
    assert.ok(TEMPLATE_TRIANGLE_BUDGETS[group.kind] >= 8_000);
  }
  assert.ok(generated.find((candidate) => candidate.name === 'process-building'));
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
node --test scripts/tests/large-industrial-fixture.test.mjs
```

Expected: module-not-found failure.

- [ ] **Step 3: Implement deterministic presentation-grade templates**

Create `geometry.mjs` with deterministic mesh helpers:

```javascript
export function createEquipmentMesh(kind, ordinal) {
  const seed = [...kind].reduce((value, character) => value * 31 + character.charCodeAt(0), ordinal);
  const positions = [];
  const indices = [];
  addTaperedShell(positions, indices, seed);
  addServiceDeck(positions, indices, seed);
  addInstrumentation(positions, indices, seed);
  return { positions, indices };
}

function addTaperedShell(positions, indices, seed) {
  const segments = 64 + (seed % 4) * 16;
  for (let ring = 0; ring <= 24; ring += 1) {
    const y = ring * 0.25;
    const radius = 3.2 + Math.sin(ring / 24 * Math.PI) * 0.4;
    for (let segment = 0; segment <= segments; segment += 1) {
      const angle = segment / segments * Math.PI * 2;
      positions.push([Math.cos(angle) * radius, y, Math.sin(angle) * radius]);
    }
  }
  const stride = segments + 1;
  for (let ring = 0; ring < 24; ring += 1) {
    for (let segment = 0; segment < segments; segment += 1) {
      const a = ring * stride + segment;
      const b = a + stride;
      indices.push(a, b, a + 1, a + 1, b, b + 1);
    }
  }
}

function addServiceDeck(positions, indices, seed) {
  const count = 96 + seed % 64;
  const base = positions.length;
  positions.push([0, 6.1, 0]);
  for (let index = 0; index <= count; index += 1) {
    const angle = index / count * Math.PI * 2;
    positions.push([Math.cos(angle) * 4.2, 6.1, Math.sin(angle) * 4.2]);
  }
  for (let index = 0; index < count; index += 1) {
    indices.push(base, base + index + 1, base + index + 2);
  }
}

function addInstrumentation(positions, indices, seed) {
  for (let item = 0; item < 24 + seed % 17; item += 1) {
    const angle = item * 2.399963;
    const base = positions.length;
    const x = Math.cos(angle) * 3.6;
    const z = Math.sin(angle) * 3.6;
    positions.push([x, 4.0, z], [x + 0.12, 4.4, z], [x, 4.8, z], [x - 0.12, 4.4, z]);
    indices.push(base, base + 1, base + 2, base, base + 2, base + 3);
  }
}
```

Create `templates.mjs` with the exact triangle-budget table:

```javascript
export const TEMPLATE_TRIANGLE_BUDGETS = Object.freeze({
  'inlet-structure': 10_000,
  'bar-screen': 15_000,
  'lift-pump': 18_000,
  'grit-basin': 25_000,
  'primary-structure': 70_000,
  'distribution-well': 18_000,
  'biological-reactor': 55_000,
  'mixer': 10_000,
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
  valve: 6_000,
  'process-building': 25_000,
});
```

`buildComponentDefinitions()` creates one stable `component-xxxxxx` per kind whose
`PartContent::CustomMesh` comes from `createEquipmentMesh`. Each definition includes two ports,
stable metadata, and a `"detailTriangleBudget"` value. Enlarge each helper with deterministic
structural rings, braces, decks, nozzles, rails, or inspection platforms until the evaluated
triangle count reaches the declared budget. Do not add invisible or degenerate triangles. The
equipment template loop uses `EQUIPMENT_GROUPS`; after it, append one `process-building` definition
using the same builder and the declared facility budget.

- [ ] **Step 4: Add and run a total-budget test**

Add:

```javascript
test('catalogue weighted template budget targets the large scene band', () => {
  const weighted = EQUIPMENT_GROUPS.reduce(
    (sum, group) => sum + group.count * TEMPLATE_TRIANGLE_BUDGETS[group.kind],
    0,
  );
  assert.ok(weighted >= 5_500_000, `weighted triangles: ${weighted}`);
  assert.ok(weighted <= 7_500_000, `weighted triangles: ${weighted}`);
});
```

Run:

```bash
node --test scripts/tests/large-industrial-fixture.test.mjs
```

Expected: PASS, 4/4.

- [ ] **Step 5: Commit**

```bash
git add fixtures/large-industrial-site/geometry.mjs \
  fixtures/large-industrial-site/templates.mjs scripts/tests/large-industrial-fixture.test.mjs
git commit -m "feat(studio-fixture): build reusable industrial equipment templates"
```

---

### 2.4 Task 4: Generate the factory layout and high-level Operation plan

**Files:**
- Create: `fixtures/large-industrial-site/layout.mjs`
- Create: `fixtures/large-industrial-site/operations.mjs`
- Modify: `scripts/tests/large-industrial-fixture.test.mjs`

**Interfaces:**
- Consumes: `buildComponentDefinitions()` and catalogue counts.
- Produces: `buildIndustrialScenePlan() -> { projectId, transactions, expected }`.
- Produces: exactly `216` primary selectable instances/facilities and deterministic `node-*` IDs.
- Every Operation type starts with `component.`, `instance`, or `connection`, or is
  `relation.create`.

- [ ] **Step 1: Add failing operation-plan tests**

Append:

```javascript
import { buildIndustrialScenePlan } from '../../fixtures/large-industrial-site/operations.mjs';

test('builds a deterministic high-level operation plan', () => {
  const first = buildIndustrialScenePlan();
  const second = buildIndustrialScenePlan();
  assert.deepEqual(first, second);
  const operations = first.transactions.flatMap((transaction) => transaction.operations);
  assert.equal(first.expected.primaryObjects, 216);
  assert.equal(operations.filter((operation) => operation.type === 'instance.create').length, 216);
  assert.equal(first.expected.facilityObjects, 12);
  assert.equal(operations.filter((operation) => operation.type === 'component.define').length, 23);
  assert.equal(operations.some((operation) => (
    !['component.define', 'instance.create', 'instanceSet.create', 'relation.create', 'connection.create']
      .includes(operation.type)
  )), false);
});
```

The `204` equipment instances plus `12` facility instances equal `216`. Instance-set behavior is
covered by M1; this fixture keeps every primary object explicit so its stable identity is directly
addressable in topology and benchmark updates.

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
node --test scripts/tests/large-industrial-fixture.test.mjs
```

Expected: module-not-found failure.

- [ ] **Step 3: Implement layout and Operations**

`layout.mjs` exports:

```javascript
export const SITE_BOUNDS = Object.freeze({
  min: [0, 0, 0],
  max: [1000, 0, 550],
});

export const AREA_BOUNDS = Object.freeze({
  intake: [30, 30, 190, 490],
  primary: [210, 40, 130, 500],
  'biological-a': [350, 80, 110, 210],
  'biological-b': [350, 290, 110, 230],
  secondary: [480, 50, 160, 480],
  advanced: [650, 50, 150, 480],
  solids: [170, 360, 290, 170],
  utilities: [40, 300, 920, 100],
});

export function areaPosition(area, ordinal, total) {
  const [x, z, width, depth] = AREA_BOUNDS[area];
  const columns = Math.max(1, Math.ceil(Math.sqrt(total * width / depth)));
  const rows = Math.max(1, Math.ceil(total / columns));
  const column = ordinal % columns;
  const row = Math.floor(ordinal / columns);
  return {
    x: x + (columns === 1 ? width / 2 : column * width / (columns - 1)),
    y: 0,
    z: z + (rows === 1 ? depth / 2 : row * depth / (rows - 1)),
  };
}

export const FACILITY_SLOTS = Object.freeze(
  Array.from({ length: 12 }, (_, index) => [
    60 + index * 80,
    520,
  ]),
);

export function facilityPosition(ordinal) {
  const [x, z] = FACILITY_SLOTS[ordinal];
  return { x, y: 0, z };
}
```

`operations.mjs` exports:

```javascript
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
  const areaByKind = new Map([
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
  const areaTotals = new Map();
  for (const group of EQUIPMENT_GROUPS) {
    const area = areaByKind.get(group.kind);
    areaTotals.set(area, (areaTotals.get(area) ?? 0) + group.count);
  }
  const areaOrdinals = new Map();
  const instances = [];
  let ordinal = 1;
  for (const group of EQUIPMENT_GROUPS) {
    const area = areaByKind.get(group.kind);
    for (let index = 0; index < group.count; index += 1) {
      const areaOrdinal = areaOrdinals.get(area) ?? 0;
      const position = areaPosition(area, areaOrdinal, areaTotals.get(area));
      instances.push({
        type: 'instance.create',
        node_id: `node-${String(ordinal).padStart(6, '0')}`,
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
      node_id: `node-${String(ordinal).padStart(6, '0')}`,
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
  return {
    projectId: 'large-industrial-site',
    transactions: chunkTransactions([...operations, ...instances], 80),
    expected: {
      primaryObjects: 216,
      facilityObjects: FACILITIES.length,
      primaryClarifiers: 4,
      processTrains: 6,
      secondaryClarifiers: 12,
      minTriangles: 5_500_000,
      maxTriangles: 7_500_000,
    },
  };
}
```

Define `chunkTransactions` in the same module:

```javascript
function chunkTransactions(operations, size) {
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
```

- [ ] **Step 4: Run plan tests and assert structural counts**

Add assertions that the individual equipment counters match Task 2, every `parent_id` references an
earlier Node, all translations are within `SITE_BOUNDS`, and every Operation list is ≤80 items.

Run:

```bash
node --test scripts/tests/large-industrial-fixture.test.mjs
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add fixtures/large-industrial-site/layout.mjs \
  fixtures/large-industrial-site/operations.mjs scripts/tests/large-industrial-fixture.test.mjs
git commit -m "feat(studio-fixture): generate large industrial layout operations"
```

---

### 2.5 Task 5: Add ports, process topology, and routed connections

**Files:**
- Create: `fixtures/large-industrial-site/topology.mjs`
- Modify: `fixtures/large-industrial-site/operations.mjs`
- Modify: `scripts/tests/large-industrial-fixture.test.mjs`

**Interfaces:**
- Consumes: generated instance and facility metadata.
- Produces: `buildTopology(instances) -> { relations, connections, validation }`.
- Produces deterministic IDs and route policies with no dangling endpoints.

- [ ] **Step 1: Add failing topology tests**

Append:

```javascript
test('connects every process train without dangling ports', () => {
  const plan = buildIndustrialScenePlan();
  const topology = plan.expected.topology;
  assert.equal(topology.processTrains, 6);
  assert.equal(topology.danglingPorts, 0);
  assert.equal(topology.invalidMedia, 0);
  assert.ok(topology.connectionCount >= 120);
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
node --test scripts/tests/large-industrial-fixture.test.mjs
```

Expected: `expected.topology` is undefined.

- [ ] **Step 3: Implement topology**

Add inlet/outlet/branch/utility ports to the generated definitions and build:

- six train inlet relations from a shared distribution instance to each first reactor;
- three reactor relations per train;
- six reactor-to-secondary relations;
- six RAS connections and six internal recycle connections;
- shared air headers for all six trains;
- solids, chemical, odor, and utility branches;
- facility service connections.

Use:

```javascript
const connection = (ordinal, fromNode, fromPort, toNode, toPort, medium, diameter) => ({
  type: 'connection.create',
  node_id: `node-${String(ordinal).padStart(6, '0')}`,
  parent_id: null,
  name: `connection-${id}`,
  transform: {
    translation: { x: 0, y: 0, z: 0 },
    rotation: [0, 0, 0, 1],
    scale: { x: 1, y: 1, z: 1 },
  },
  connection: {
    from: { kind: 'instance', node_id: fromNode, port: fromPort },
    to: { kind: 'instance', node_id: toNode, port: toPort },
    medium,
    diameter,
    route_policy: { kind: 'rack', elevation: 5.5 },
    relation_id: `relation-${String(ordinal).padStart(6, '0')}`,
    metadata: {},
  },
  metadata: {}
});

const relation = (ordinal, fromNode, toNode, medium) => ({
  type: 'relation.create',
  relation: {
    id: `relation-${String(ordinal).padStart(6, '0')}`,
    relation_type: 'process',
    from_node_id: fromNode,
    to_node_id: toNode,
    metadata: { medium }
  }
});
```

Connections and their Relations must be emitted in the same transaction, with the Relation before
the connection so `connection.create` can reference it.

- [ ] **Step 4: Run topology tests**

Run:

```bash
node --test scripts/tests/large-industrial-fixture.test.mjs
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add fixtures/large-industrial-site/topology.mjs \
  fixtures/large-industrial-site/operations.mjs scripts/tests/large-industrial-fixture.test.mjs
git commit -m "feat(studio-fixture): connect large industrial process topology"
```

---

### 2.6 Task 6: Run the plan through the public API and verify evaluated scale

**Files:**
- Create: `scripts/run-large-industrial-fixture.mjs`
- Modify: `package.json`
- Modify: `scripts/tests/large-industrial-fixture.test.mjs`

**Interfaces:**
- Produces CLI: `npm run fixture:large-industrial`.
- Consumes: all M1 Operations and `evaluation.stats`.
- Produces JSON evidence: `output/large-industrial-fixture.json`.

- [ ] **Step 1: Add a failing CLI test**

Append a test that runs the module's exported `verifyAppliedScene(stats, plan)`:

```javascript
import { verifyAppliedScene } from '../../scripts/run-large-industrial-fixture.mjs';

test('rejects evaluated output outside the accepted scale', () => {
  assert.throws(() => verifyAppliedScene({
    sourceNodes: 216,
    expandedNodes: 4000,
    uniqueMeshes: 22,
    effectiveTriangles: 5_000_000,
    uniqueTriangles: 500_000,
    cacheHits: 0,
    cacheMisses: 22,
    generatedMeshes: 22,
  }, {
    primaryObjects: 216,
    minTriangles: 5_500_000,
    maxTriangles: 7_500_000,
  }));
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
node --test scripts/tests/large-industrial-fixture.test.mjs
```

Expected: module-not-found failure.

- [ ] **Step 3: Implement the API runner**

Create `scripts/run-large-industrial-fixture.mjs` by extracting the server startup and RPC helper
pattern from `scripts/run-ai-fixture.mjs`. It must:

1. Build the release CLI with `cargo build --release -p studio-cli`.
2. Start a temporary Studio workspace and autonomous server.
3. Create `large-industrial-site`.
4. `dryRun` the complete first transaction and assert revision remains 0.
5. Apply all transactions with monotonic `expectedRevision`.
6. Read `document.get` and assert the 216 primary source objects.
7. Call `evaluation.stats` and pass the result to `verifyAppliedScene`.
8. Call `export.sourceGlb` and record file size/digest metadata.
9. Close, reopen, and verify revision, IDs, and stats remain stable.
10. Write the result atomically to `output/large-industrial-fixture.json`.

Export:

```javascript
export function verifyAppliedScene(stats, expected) {
  if (stats.sourceNodes < expected.primaryObjects) {
    throw new Error('source object count is below the accepted scale');
  }
  if (stats.uniqueMeshes > 40) {
    throw new Error('equipment definitions did not share evaluated meshes');
  }
  if (stats.effectiveTriangles < expected.minTriangles
    || stats.effectiveTriangles > expected.maxTriangles) {
    throw new Error(`effective triangle count outside budget: ${stats.effectiveTriangles}`);
  }
  if (!Number.isInteger(stats.uniqueTriangles) || stats.uniqueTriangles <= 0) {
    throw new Error('unique triangle count is invalid');
  }
}
```

Add:

```json
"fixture:large-industrial": "node scripts/run-large-industrial-fixture.mjs"
```

- [ ] **Step 4: Execute the complete fixture**

Run:

```bash
source $HOME/.nvm/nvm.sh && nvm use 24.22.1 >/dev/null
npm run fixture:large-industrial
```

Expected: exit 0 and a report containing 216 primary objects, 5.5M–7.5M effective triangles,
at most 40 unique meshes, and a valid source GLB path.

- [ ] **Step 5: Commit**

```bash
git add scripts/run-large-industrial-fixture.mjs package.json \
  scripts/tests/large-industrial-fixture.test.mjs
git commit -m "test(studio-fixture): verify large industrial scene through public API"
```

---

### 2.7 Task 7: Add quantified incremental and rendering-preparation benchmarks

**Files:**
- Create: `benchmarks/large-industrial-scene.mjs`
- Modify: `package.json`
- Modify: `scripts/verify.mjs`

**Interfaces:**
- Produces CLI: `npm run benchmark:large-industrial`.
- Produces `output/large-industrial-benchmark.json`.
- Compares cold build, warm build, full evaluation, one-instance update, source GLB export, and
  optional Runtime preview.

- [ ] **Step 1: Implement the benchmark with explicit failure gates**

The benchmark must use the fixture plan and public API, warm up twice, then collect at least seven
samples for:

- high-level plan construction;
- full document apply and validation;
- cold `evaluation.stats`;
- warm `evaluation.stats`;
- one-instance parameter/transform update followed by `evaluation.stats`;
- source GLB export;
- reopen;
- Runtime preview when Toolchain is available.

Record p50/p95, source/unique triangle counts, unique meshes, cache hits/misses, operation count,
document bytes, source GLB bytes, and output revision.

Release gates:

```javascript
if (stats.effectiveTriangles < 5_500_000 || stats.effectiveTriangles > 7_500_000) {
  failures.push('effectiveTriangles');
}
if (stats.uniqueMeshes > 40) {
  failures.push('uniqueMeshes');
}
if (incremental.cacheHits < 1 || incremental.generatedMeshes >= stats.uniqueMeshes) {
  failures.push('incrementalCacheReuse');
}
if (operations.length >= 100_000) {
  failures.push('operationCount');
}
```

The one-instance update changes only a secondary instance transform. Its `evaluation.stats` call
must reuse existing geometry and must not regenerate every mesh.

- [ ] **Step 2: Add scripts**

Add:

```json
"benchmark:large-industrial": "node benchmarks/large-industrial-scene.mjs"
```

Add a `Large industrial fixture` verification step after the existing AI fixture step:

```javascript
{
  label: 'Large industrial fixture',
  command: 'node',
  args: ['scripts/run-large-industrial-fixture.mjs'],
},
```

- [ ] **Step 3: Run verification and benchmark**

Run:

```bash
source $HOME/.nvm/nvm.sh && nvm use 24.22.1 >/dev/null
npm run fixture:large-industrial
npm run benchmark:large-industrial
```

Expected: both pass; the benchmark report includes exact p50/p95 values and exceeds no documented
memory or execution limit.

- [ ] **Step 4: Commit**

```bash
git add benchmarks/large-industrial-scene.mjs package.json scripts/verify.mjs
git commit -m "perf(studio-fixture): benchmark large industrial authoring"
```

---

### 2.8 Task 8: Document the fixture and run the final repository gate

**Files:**
- Create: `docs/guides/large-industrial-fixture.md`
- Modify: `docs/README.md`
- Modify: `docs/specs/2026-10-30-large-scene-authoring-design.md`
- Modify: `docs/plans/2026-10-30-large-industrial-scene-acceptance.md`

**Interfaces:**
- Consumes all M2 artifacts.
- Produces reproducible instructions and updates the spec's implementation status to M1+M2
  complete while M3/M4 remain pending.

- [ ] **Step 1: Write the guide**

Document:

- exact fixture scale and why it is fictional;
- `npm run fixture:large-industrial`;
- `npm run benchmark:large-industrial`;
- output locations and ignored status;
- deterministic AI-client path;
- triangle, object, mesh-sharing, topology, and incremental-cache acceptance;
- distinction between Studio source generation and Toolchain Runtime preparation.

- [ ] **Step 2: Update document status**

This step originally kept the design `Approved` until GPU and Runtime publication were complete.
Later GPU-scene and Toolchain integration plans delivered those capabilities; the design is now
`Implemented` and native GPU execution remains experimental and default-off.

- [ ] **Step 3: Run the full gate**

Run:

```bash
source $HOME/.nvm/nvm.sh && nvm use 24.22.1 >/dev/null
npm run verify
npm run benchmark:large-industrial
```

Expected: all repository gates pass; the generated scene remains within all scale and topology
budgets; no generated asset or cache appears in `git status`.

- [ ] **Step 4: Audit and commit**

Run:

```bash
git status --short
git diff --check
rg -n "TBD|TODO|FIXME|PLACEHOLDER" docs fixtures scripts benchmarks crates apps || true
git add docs
git diff --cached --check
git commit -m "docs(studio): document large industrial scene acceptance"
```

Expected: documentation-only commit; generated GLBs, benchmark output, and temporary projects are
not staged.

---

## 3. Plan Self-Review

### 3.1 Spec Coverage

| Spec requirement | Plan task |
|---|---|
| Generic high-level Operations | 2, 4 |
| 200–240 primary objects and required process counts | 2, 4, 6 |
| Four primary, six trains, 10–12 secondary | 2, 4, 6 |
| 5.5M–7.5M effective triangles | 3, 6, 7 |
| Shared definitions and unique mesh reuse | 3, 5, 6 |
| Typed ports and complete process topology | 5 |
| Dry-run, atomic apply, reload and stable IDs | 6 |
| Incremental cache evidence | 7 |
| Source GLB and optional Runtime preview | 6, 7 |
| Generic, industry-neutral Core boundary | 2, 5 |
| Machine-readable quantitative evidence | 1, 6, 7 |
| Documentation and full regression | 8 |

### 3.2 Type Consistency

- `EQUIPMENT_GROUPS` is the shared source for definitions and counts.
- Definitions are named by generic `kind`; display names and industry labels live in metadata.
- `ComponentDefinition.id`, `ComponentInstance.definitionId`, and generated `node-*` IDs match M1.
- `evaluation.stats` reports fields directly from `LargeSceneStats`.
- Connection endpoint objects use M1's `{ kind: "instance", node_id, port }` schema.
- Monetary or sector-specific terminology does not enter Core or public Operation names.

### 3.3 Scope Check

This plan produces a working, independently testable M2 authoring and benchmark milestone. Native
GPU execution and Toolchain Tileset publication remain in M3 and M4.
