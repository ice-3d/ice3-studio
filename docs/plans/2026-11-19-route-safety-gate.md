# 管线工程安全门禁实施计划

> Document ID: ICE3-STUDIO-PLAN-030
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 Document 写入 revision 前，对管线中心线执行实体碰撞、净空面、端点接入和
支撑布局校验，并让污水厂资产满足这些硬门禁。

**Architecture:** 新 `route_safety` Core 模块复用正式路由中心线，以 AABB sweep 做宽相位，
只对候选执行 segment-to-triangle 距离窄相位。行业语义仅通过字符串 metadata 进入，
支持横梁继续作为资产节点存在。

**Tech Stack:** Rust 1.93、Cargo、Node.js 24.22.1、JavaScript ESM、Node Test、
Studio JSON-RPC、React/WebGL2 Viewer、Playwright、Google Chrome

**Spec:** `docs/specs/2026-11-19-route-safety-gate-design.md`

## 1. Global Constraints

- `API_VERSION` 最终必须为 `1.4`。
- Studio Core 不增加道路、池体、泵站或其它行业业务类型。
- Document 和 operation history 是唯一真源；路由、诊断和 GLB 均为派生结果。
- 所有生产代码先写失败测试，再写最小实现。
- Git author 和 committer 必须为 `damoqiongqiu <damoqiongqiu@126.com>`。
- 不推送远端。
- 障碍 owner 上限 `50,000`、路由段上限 `50,000`、窄相位上限 `1,000,000` 对。
- 单条连接最多报告 `32` 条碰撞。
- 端点接入豁免弧长为 `1.5m`；支撑默认最大间距为 `6m`；投影容差为 `1m`。
- 污水厂预检 p95 不超过 `150ms`；大型 fixture 不超过 `250ms`；额外内存不超过
  `64 MiB`。
- Web 测试、截图和视觉检查只使用 Google Chrome。
- Schema 与 bindings 是生成物，必须按仓库顺序同步，不手改汇总 TypeScript。

## 2. Review Focus

- 端点所属设备的合法插入不能掩盖路线后段再次穿入同一设备。
- 旋转、凹形和 Cut 部件不能让 AABB 宽相位把真实碰撞漏成 clean。
- 支撑 `lineId` 拼错、离线路面或转弯附近缺失时，不能因为存在一个任意支撑而通过。
- 预算耗尽不能被当作“没有碰撞”；必须返回 error Diagnostic。
- 道路净空面、ignored 附属件和默认 solid 分类不能让路面检查或实体检查互相旁路。

## 3. File Structure

### 3.1 Core

- Modify: `crates/studio-core/src/geometry/clash.rs`
- Modify: `crates/studio-core/src/geometry/mod.rs`
- Create: `crates/studio-core/src/large_scene/route_safety.rs`
- Modify: `crates/studio-core/src/large_scene/mod.rs`
- Modify: `crates/studio-core/src/large_scene/validate.rs`
- Modify: `crates/studio-core/src/validation.rs`
- Modify: `crates/studio-core/src/lib.rs`
- Test: `crates/studio-core/tests/route_safety.rs`
- Test: `crates/studio-core/tests/transactions.rs`

### 3.2 Asset and benchmark

- Modify: `scripts/wastewater-plant/site-plan.mjs`
- Modify: `scripts/wastewater-plant/model-kit.mjs`
- Modify: `scripts/wastewater-plant/validate.mjs`
- Modify: `scripts/tests/wastewater-plant-model.test.mjs`
- Create: `scripts/benchmark-route-safety.mjs`
- Create: `scripts/tests/route-safety-benchmark.test.mjs`
- Modify: `scripts/run-large-industrial-fixture.mjs`
- Modify: `package.json`

### 3.3 Generated data and docs

- Regenerate: `crates/*/bindings/`
- Regenerate: `schemas/`
- Regenerate through CLI: `projects/wastewater-plant/`
- Modify: `projects/README.md`
- Modify: `docs/reference/api.md`
- Modify: `docs/reference/capabilities.md`
- Modify: `docs/reference/diagnostics.md`
- Modify spec status after acceptance.

---

### Task 1: Segment-to-Triangle Distance

**Files:**
- Modify: `crates/studio-core/src/geometry/clash.rs`
- Modify: `crates/studio-core/src/geometry/mod.rs`

**Interfaces:**
- Consumes: existing `segment_triangle` intersection predicate and `Bounds`.
- Produces:
  - `pub fn segment_triangle_distance(segment: [[f32; 3]; 2], triangle: [[f32; 3]; 3]) -> f32`
  - `pub fn segment_bounds_distance(segment: [[f32; 3]; 2], bounds: &Bounds) -> f32`

- [ ] **Step 1: Write the failing distance tests**

Add to `geometry/clash.rs` tests:

```rust
#[test]
fn segment_triangle_distance_reports_crossing_and_clearance() {
    let triangle = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    assert_eq!(
        segment_triangle_distance([[0.2, 0.2, -1.0], [0.2, 0.2, 1.0]], triangle),
        0.0,
    );
    let near = segment_triangle_distance(
        [[0.2, 0.2, 0.5], [0.8, 0.2, 0.5]],
        triangle,
    );
    assert!((near - 0.5).abs() < 1e-5, "{near}");

    let bounds = Bounds {
        min: Vec3 { x: 0.0, y: 0.0, z: -1.0 },
        max: Vec3 { x: 1.0, y: 1.0, z: 1.0 },
    };
    assert_eq!(
        segment_bounds_distance([[0.2, 0.2, -2.0], [0.2, 0.2, 2.0]], &bounds),
        0.0,
    );
}
```

- [ ] **Step 2: Run the test and verify RED**

Run:

```bash
cargo test -p studio-core segment_triangle_distance_reports_crossing_and_clearance
```

Expected: FAIL because `segment_triangle_distance` does not exist.

- [ ] **Step 3: Implement the minimal distance primitive**

Add helpers in `clash.rs` for point-triangle and segment-segment distance.

`segment_bounds_distance` returns `0.0` when any part of the segment is inside the AABB.
Otherwise it returns the shortest distance from either endpoint to the AABB or from the
segment to the nearest AABB face.

`segment_triangle_distance` returns `0.0` when `segment_triangle` is true; otherwise returns
the minimum of:

- both segment endpoints to the triangle plane/region;
- each segment endpoint to each triangle edge;
- each triangle vertex to the segment.

```rust
pub fn segment_triangle_distance(
    segment: [[f32; 3]; 2],
    triangle: [[f32; 3]; 3],
) -> f32 {
    if segment_triangle(segment, triangle) {
        return 0.0;
    }
    let mut distance = f32::INFINITY;
    for point in segment {
        distance = distance.min(point_triangle_distance(point, triangle));
    }
    for vertex in triangle {
        distance = distance.min(point_segment_distance(vertex, segment));
    }
    for triangle_edge in triangle_edges(triangle) {
        distance = distance.min(segment_segment_distance(segment, triangle_edge));
    }
    distance
}
```

- [ ] **Step 4: Export and verify GREEN**

Add both distance functions to `geometry/mod.rs` re-exports, then run:

```bash
cargo test -p studio-core segment_triangle_distance_reports_crossing_and_clearance
cargo test -p studio-core --test clashes
```

Expected: both commands PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-core/src/geometry/clash.rs \
  crates/studio-core/src/geometry/mod.rs
git commit -m "feat(geometry): measure segment triangle distance"
```

### Task 2: Routing Contract, Limits and Diagnostics

**Files:**
- Create: `crates/studio-core/src/large_scene/route_safety.rs`
- Modify: `crates/studio-core/src/large_scene/mod.rs`
- Modify: `crates/studio-core/src/validation.rs`
- Modify: `crates/studio-core/src/lib.rs`
- Modify: `crates/studio-server/tests/rpc_contract.rs`
- Regenerate: `crates/*/bindings/`, `schemas/`

**Interfaces:**
- Consumes: `ConnectionIntent.metadata`, `RoutePolicy`, `ComponentDefinition`.
- Produces:
  - `RouteSafetyStats` with public `usize` fields `obstacle_owners`, `route_segments`,
    `broad_phase_candidates`, `narrow_phase_pairs`, and `triangle_pairs`.
  - routing constants listed below.
  - five `DiagnosticCode` variants: `RouteCollision`, `RouteClearanceInsufficient`,
    `RouteSupportMissing`, `RouteSupportSpacing`, `RouteSafetyBudget`.

- [ ] **Step 1: Write failing contract and metadata tests**

Add tests in `route_safety.rs`:

```rust
#[test]
fn routing_limits_and_defaults_are_stable() {
    assert_eq!(DEFAULT_MAX_SUPPORT_SPACING_M, 6.0);
    assert_eq!(ENDPOINT_EXCLUSION_M, 1.5);
    assert_eq!(MAX_OBSTACLE_OWNERS, 50_000);
    assert_eq!(MAX_ROUTE_SEGMENTS, 50_000);
    assert_eq!(MAX_TRIANGLE_PAIRS, 1_000_000);
}

#[test]
fn metadata_parser_rejects_unknown_obstacle_values() {
    let mut metadata = BTreeMap::new();
    metadata.insert("routeObstacle".into(), "mystery".into());
    assert_eq!(
        obstacle_class(&metadata),
        Err("routeObstacle must be solid, ignored, or clearance-surface"),
    );
}
```

Add to `rpc_contract.rs`:

```rust
#[test]
fn health_reports_route_safety_api_version() {
    assert_eq!(studio_core::API_VERSION, "1.4");
}
```

- [ ] **Step 2: Run tests and verify RED**

Run:

```bash
cargo test -p studio-core routing_limits_and_defaults_are_stable
cargo test -p studio-server --test rpc_contract health_reports_route_safety_api_version
```

Expected: FAIL because the module, constants and API version are absent.

- [ ] **Step 3: Add constants, metadata parser and DiagnosticCode variants**

Add:

```rust
pub const ENDPOINT_EXCLUSION_M: f32 = 1.5;
pub const DEFAULT_MAX_SUPPORT_SPACING_M: f32 = 6.0;
pub const SUPPORT_PROJECTION_TOLERANCE_M: f32 = 1.0;
pub const TURN_THRESHOLD_RADIANS: f32 = 15.0_f32.to_radians();
pub const MAX_OBSTACLE_OWNERS: usize = 50_000;
pub const MAX_ROUTE_SEGMENTS: usize = 50_000;
pub const MAX_TRIANGLE_PAIRS: usize = 1_000_000;
pub const MAX_COLLISIONS_PER_ROUTE: usize = 32;

pub(crate) enum ObstacleClass {
    Solid,
    Ignored,
    ClearanceSurface { clearance: f32 },
}
```

Add `DiagnosticCode` variants:

```text
ICE3_STUDIO_ROUTE_COLLISION
ICE3_STUDIO_ROUTE_CLEARANCE_INSUFFICIENT
ICE3_STUDIO_ROUTE_SUPPORT_MISSING
ICE3_STUDIO_ROUTE_SUPPORT_SPACING
ICE3_STUDIO_ROUTE_SAFETY_BUDGET
```

Set `API_VERSION` to `1.4`.

- [ ] **Step 4: Regenerate bindings and verify GREEN**

Refresh the generated TypeScript copies through the existing `ts-rs` export tests, then:

```bash
cargo run -p studio-cli -- schema --output schemas
cargo test -p studio-core routing_limits_and_defaults_are_stable
cargo test -p studio-server --test rpc_contract health_reports_route_safety_api_version
cargo run -p studio-cli -- schema --output schemas --check
```

Expected: all PASS and generated files show only `1.4` plus new Diagnostic codes.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-core crates/studio-server/tests/rpc_contract.rs \
  crates/*/bindings schemas
git commit -m "feat(api): define route safety diagnostics"
```

### Task 3: Route Facts and Static Obstacle Broad Phase

**Files:**
- Modify: `crates/studio-core/src/large_scene/route_safety.rs`
- Modify: `crates/studio-core/src/large_scene/validate.rs`
- Create: `crates/studio-core/tests/route_safety.rs`

**Interfaces:**
- Consumes: `route_connection`, `shared_pump_document`, `segment_bounds_distance`.
- Produces: `validate_route_safety(document, definitions) -> (Vec<Diagnostic>, RouteSafetyStats)`.

- [ ] **Step 1: Write a failing static-obstacle collision test**

Use `shared_pump_document()`, add pipe/inlet ports at the two pump bodies, add a direct
connection from `node-000001` to `node-000002`, and add a static box at `x=6`.

```rust
#[test]
fn direct_route_through_a_static_box_is_rejected() {
    let document = collision_document();
    let definitions = definitions(&document);
    let (diagnostics, stats) = validate_route_safety(&document, &definitions);
    assert!(diagnostics.iter().any(|diagnostic| (
        diagnostic.code == DiagnosticCode::RouteCollision
        && diagnostic.object_id.as_deref() == Some("node-000003")
    )));
    assert_eq!(stats.obstacle_owners, 3);
    assert!(stats.route_segments > 0);
}
```

Add shared helpers in the same test file:

```rust
fn definitions(
    document: &Document,
) -> BTreeMap<ComponentId, &ComponentDefinition> {
    document.components.iter()
        .map(|definition| (definition.id.clone(), definition))
        .collect()
}

fn run_route_safety(document: &Document) -> (Vec<Diagnostic>, RouteSafetyStats) {
    let definitions = definitions(document);
    validate_route_safety(document, &definitions)
}

fn route_safety_codes(document: &Document) -> Vec<DiagnosticCode> {
    run_route_safety(document).0
        .into_iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}
```

- [ ] **Step 2: Run the test and verify RED**

Run:

```bash
cargo test -p studio-core --test route_safety direct_route_through_a_static_box_is_rejected
```

Expected: FAIL because `validate_route_safety` does not exist.

- [ ] **Step 3: Implement route facts and static owner AABBs**

Build `RouteFact` from every `NodeContent::Connection`:

```rust
struct RouteFact {
    node_index: usize,
    node_id: NodeId,
    from_owner: NodeId,
    to_owner: NodeId,
    radius: f32,
    policy: RoutePolicy,
    line_id: Option<String>,
    max_support_spacing: f32,
    points: Vec<[f32; 3]>,
    cumulative: Vec<f32>,
}
```

For this task, build obstacle owners from static scene nodes using
`crate::geometry::world_bounds`. Sort owners by node ID and count them against
`MAX_OBSTACLE_OWNERS`.

Define the stats result used by all later benchmark and budget tasks:

```rust
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteSafetyStats {
    pub obstacle_owners: usize,
    pub route_segments: usize,
    pub broad_phase_candidates: usize,
    pub narrow_phase_pairs: usize,
    pub triangle_pairs: usize,
}
```

- [ ] **Step 4: Integrate and verify GREEN**

Call `route_safety::validate_route_safety` at the end of `validate_large_scene`.
Skip a connection when existing structural checks prevent port resolution.

```bash
cargo test -p studio-core --test route_safety direct_route_through_a_static_box_is_rejected
cargo test -p studio-core --test connections
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-core/src/large_scene/route_safety.rs \
  crates/studio-core/src/large_scene/validate.rs \
  crates/studio-core/tests/route_safety.rs
git commit -m "feat(core): preflight routes against static obstacles"
```

### Task 4: Endpoint Exemption and Candidate Narrow Phase

**Files:**
- Modify: `crates/studio-core/src/large_scene/route_safety.rs`
- Modify: `crates/studio-core/tests/route_safety.rs`

**Interfaces:**
- Consumes: Task 1 `segment_triangle_distance`, `ResolvedPort`, `route_connection`.
- Produces: owner-aware collision filtering and shared `RouteSafetyStats`.

- [ ] **Step 1: Write failing endpoint and component-obstacle tests**

```rust
#[test]
fn endpoint_owner_contact_is_ignored_but_reentry_is_not() {
    assert!(!route_safety_codes(&endpoint_contact_document())
        .contains(&DiagnosticCode::RouteCollision));
    assert!(route_safety_codes(&endpoint_reentry_document())
        .contains(&DiagnosticCode::RouteCollision));
}

#[test]
fn rotated_component_part_uses_narrow_phase_after_box_overlap() {
    let (diagnostics, stats) = run_route_safety(&rotated_part_clearance_document());
    assert!(!diagnostics.iter().any(|diagnostic| (
        diagnostic.code == DiagnosticCode::RouteCollision
    )));
    assert!(stats.narrow_phase_pairs < stats.broad_phase_candidates);
}
```

- [ ] **Step 2: Verify RED**

Run:

```bash
cargo test -p studio-core --test route_safety endpoint_owner_contact_is_ignored_but_reentry_is_not
cargo test -p studio-core --test route_safety rotated_component_part_uses_narrow_phase_after_box_overlap
```

Expected: first FAIL because endpoint owners are not excluded; second FAIL because
component obstacles are not built.

- [ ] **Step 3: Implement owner expansion and endpoint windows**

For each connection, resolve the actual endpoint node ID from `InstancePortRef`
(`set_node_id` for set items). Ignore only collisions whose source owner equals an
endpoint and whose nearest arc position is within `ENDPOINT_EXCLUSION_M` of that end.

Build component owner AABBs from resolved parameters, nested Part transforms and instance
transform. Do not evaluate Part triangles until an owner enters the broad-phase candidate
set.

- [ ] **Step 4: Implement narrow phase and verify GREEN**

Evaluate only candidate owner mesh variants, transform triangles to world space, and call
`segment_triangle_distance`. Count each triangle comparison in
`RouteSafetyStats.triangle_pairs`.

```bash
cargo test -p studio-core --test route_safety endpoint_owner_contact_is_ignored_but_reentry_is_not
cargo test -p studio-core --test route_safety rotated_component_part_uses_narrow_phase_after_box_overlap
cargo test -p studio-core --test route_safety
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-core/src/large_scene/route_safety.rs \
  crates/studio-core/tests/route_safety.rs
git commit -m "feat(core): narrow route collisions with endpoint exemptions"
```

### Task 5: Clearance Surfaces

**Files:**
- Modify: `crates/studio-core/src/large_scene/route_safety.rs`
- Modify: `crates/studio-core/tests/route_safety.rs`

**Interfaces:**
- Consumes: `routeObstacle=clearance-surface`, `routeClearance`.
- Produces: `ICE3_STUDIO_ROUTE_CLEARANCE_INSUFFICIENT`.

- [ ] **Step 1: Write failing clearance tests**

```rust
#[test]
fn clearance_surface_rejects_a_low_crossing() {
    for (surface_y, clearance, expected) in [
        (0.0, 5.0, true),
        (4.0, 1.0, true),
        (3.9, 0.5, false),
    ] {
        let diagnostics = clearance_diagnostics(surface_y, clearance);
        assert_eq!(
            diagnostics.iter().any(|diagnostic| (
                diagnostic.code == DiagnosticCode::RouteClearanceInsufficient
            )),
            expected,
        );
    }
}
```

- [ ] **Step 2: Verify RED**

```bash
cargo test -p studio-core --test route_safety clearance_surface_rejects_a_low_crossing
```

Expected: FAIL because clearance surfaces are not evaluated.

- [ ] **Step 3: Implement footprint crossing and vertical clearance**

Project each route segment onto XZ. When it intersects the surface AABB footprint, compare
the segment's minimum center Y minus route radius with `surface.max.y + clearance`.
`routeClearance` parsing accepts finite values `>= 0`; invalid values emit
`DiagnosticCode::OperationInvalid`.

- [ ] **Step 4: Verify GREEN**

```bash
cargo test -p studio-core --test route_safety clearance_surface_rejects_a_low_crossing
cargo test -p studio-core --test route_safety
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-core/src/large_scene/route_safety.rs \
  crates/studio-core/tests/route_safety.rs
git commit -m "feat(core): enforce route clearance surfaces"
```

### Task 6: Support Missing, Spacing and Turns

**Files:**
- Modify: `crates/studio-core/src/large_scene/route_safety.rs`
- Modify: `crates/studio-core/tests/route_safety.rs`

**Interfaces:**
- Consumes: `role=pipe-support`, `supportPart=beam`, `lineId`, `maxSupportSpacing`.
- Produces: support-missing and support-spacing diagnostics.

- [ ] **Step 1: Write failing support tests**

```rust
#[test]
fn support_layout_rejects_missing_sparse_and_off_route_stations() {
    assert_support_code(missing_support_document(), DiagnosticCode::RouteSupportMissing);
    assert_support_code(sparse_support_document(), DiagnosticCode::RouteSupportSpacing);
    assert_support_code(off_route_support_document(), DiagnosticCode::RouteSupportSpacing);
    assert_support_code(turn_without_support_document(), DiagnosticCode::RouteSupportSpacing);
    assert_eq!(buried_without_support_codes(), vec![]);
}
```

- [ ] **Step 2: Verify RED**

```bash
cargo test -p studio-core --test route_safety support_layout_rejects_missing_sparse_and_off_route_stations
```

Expected: FAIL because support stations are not indexed.

- [ ] **Step 3: Implement support projection and spacing**

Build a `BTreeMap<String, Vec<SupportStation>>` keyed by `lineId`. Reject orphan
support IDs with `DiagnosticCode::OperationInvalid`. For non-Buried routes:

- project each beam center to the nearest route segment;
- reject horizontal projection distance above `SUPPORT_PROJECTION_TOLERANCE_M`;
- sort arc lengths;
- include `0` and route length;
- reject any gap above `maxSupportSpacing`;
- reject a turn when no valid station exists within
  `max(1.0, 4.0 * diameter)` of the turn.

- [ ] **Step 4: Verify GREEN**

```bash
cargo test -p studio-core --test route_safety support_layout_rejects_missing_sparse_and_off_route_stations
cargo test -p studio-core --test route_safety
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-core/src/large_scene/route_safety.rs \
  crates/studio-core/tests/route_safety.rs
git commit -m "feat(core): validate pipe support layout"
```

### Task 7: Budgets and Transaction Failure

**Files:**
- Modify: `crates/studio-core/src/large_scene/route_safety.rs`
- Modify: `crates/studio-core/tests/route_safety.rs`
- Modify: `crates/studio-core/tests/transactions.rs`

**Interfaces:**
- Consumes: `RouteSafetyStats`, `apply_transaction`.
- Produces: deterministic budget failure and pre-persistence rejection.

- [ ] **Step 1: Write failing budget and transaction tests**

```rust
#[test]
fn route_safety_budget_exhaustion_fails_closed() {
    let (diagnostics, _) = validate_with_segment_limit(MAX_ROUTE_SEGMENTS - 1);
    assert!(diagnostics.iter().any(|diagnostic| (
        diagnostic.code == DiagnosticCode::RouteSafetyBudget
    )));
}

#[test]
fn route_collision_does_not_advance_revision() {
    let mut document = collision_document();
    let before = document.revision;
    let error = apply_transaction(&mut document, collision_apply_request()).unwrap_err();
    assert!(matches!(error, TransactionError::ValidationFailed(_)));
    assert_eq!(document.revision, before);
}
```

- [ ] **Step 2: Verify RED**

```bash
cargo test -p studio-core --test route_safety route_safety_budget_exhaustion_fails_closed
cargo test -p studio-core --test transactions route_collision_does_not_advance_revision
```

Expected: first FAIL because limits are not enforced; second FAIL because routing validation
is not yet part of `validate_document`.

- [ ] **Step 3: Enforce every limit before allocation**

Check obstacle owners, route segments and triangle pairs before extending vectors. Return
`RouteSafetyBudget` and stop further route checks. Keep already-produced diagnostics in
stable order and append the budget failure last.

- [ ] **Step 4: Verify GREEN and deterministic ordering**

```bash
cargo test -p studio-core --test route_safety route_safety_budget_exhaustion_fails_closed
cargo test -p studio-core --test transactions route_collision_does_not_advance_revision
cargo test -p studio-core --test route_safety
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-core/src/large_scene/route_safety.rs \
  crates/studio-core/tests/route_safety.rs \
  crates/studio-core/tests/transactions.rs
git commit -m "test(core): fail closed on route safety budgets"
```

### Task 8: Wastewater Metadata and Multi-Support Generation

**Files:**
- Modify: `scripts/wastewater-plant/site-plan.mjs`
- Modify: `scripts/wastewater-plant/model-kit.mjs`
- Modify: `scripts/wastewater-plant/validate.mjs`
- Modify: `scripts/tests/wastewater-plant-model.test.mjs`

**Interfaces:**
- Consumes: `buildPlan`, existing `connect`, `coarseRoute`.
- Produces: `planSupportStations(route, maxSpacing = 6)` and `validatePipeSupportSpacing`.

- [ ] **Step 1: Write failing asset tests**

```js
test('pipe supports cover every above-ground route at six metre intervals', () => {
  const result = validatePipeSupportSpacing(buildLegacyPlan('wastewater-plant'), {
    maxSpacing: 6,
    tolerance: 1,
  });
  assert.deepEqual(result.issues, []);
});

test('roads declare generic route clearance surfaces', () => {
  const roads = buildLegacyPlan('wastewater-plant').operations.filter((operation) => (
    operation.type === 'node.create' && operation.metadata?.roadClass
  ));
  assert.ok(roads.length > 0);
  for (const road of roads) {
    assert.equal(road.metadata.routeObstacle, 'clearance-surface');
    assert.equal(road.metadata.routeClearance, '5');
  }
});
```

- [ ] **Step 2: Verify RED**

```bash
node --test scripts/tests/wastewater-plant-model.test.mjs \
  --test-name-pattern='pipe supports cover|roads declare'
```

Expected: FAIL because `validatePipeSupportSpacing` and road metadata are absent.

- [ ] **Step 3: Add generic metadata and support stations**

For road `b.ground` calls in `site-plan.mjs`, merge:

```js
{
  roadClass: 'site-access',
  routeObstacle: 'clearance-surface',
  routeClearance: '5',
}
```

In `model-kit.mjs`, add:

```js
export function planSupportStations(route, maxSpacing = 6) {
  const total = route.slice(1).reduce((sum, point, index) => (
    sum + Math.hypot(
      point.x - route[index].x,
      point.y - route[index].y,
      point.z - route[index].z,
    )
  ), 0);
  const stations = [];
  for (let distance = maxSpacing; distance < total; distance += maxSpacing) {
    const sample = pointAtRouteDistance(route, distance);
    stations.push(sample);
  }
  return stations;
}

function pointAtRouteDistance(route, target) {
  let travelled = 0;
  for (let index = 1; index < route.length; index += 1) {
    const from = route[index - 1];
    const to = route[index];
    const length = Math.hypot(to.x - from.x, to.y - from.y, to.z - from.z);
    if (travelled + length >= target) {
      const ratio = (target - travelled) / length;
      return {
        point: {
          x: from.x + (to.x - from.x) * ratio,
          y: from.y + (to.y - from.y) * ratio,
          z: from.z + (to.z - from.z) * ratio,
        },
        run: { from, to },
      };
    }
    travelled += length;
  }
  const last = route[route.length - 1];
  return { point: last, run: { from: route.at(-2), to: last } };
}
```

Generate support groups for every returned station with unique
`supportGroup = `${lineId}-${index}``. Mark every pipe accessory metadata with
`routeObstacle: 'ignored'`. Add connection metadata `maxSupportSpacing: '6'`.

- [ ] **Step 4: Update alignment validation and verify GREEN**

Change `validatePipeAccessoryAlignment` to store an array of beam operations per `lineId`
and validate every station. Add:

```js
export function validatePipeSupportSpacing(
  plan,
  { maxSpacing = 6, tolerance = 1 } = {},
) {
  const definitions = new Map(
    plan.operations
      .filter(({ type }) => type === 'component.define')
      .map(({ definition }) => [definition.id, definition]),
  );
  const instances = new Map(
    plan.operations
      .filter(({ type }) => type === 'instance.create')
      .map((operation) => [operation.node_id, operation]),
  );
  const supports = new Map();
  for (const operation of plan.operations) {
    if (operation.type !== 'node.create') continue;
    if (operation.metadata?.role !== 'pipe-support') continue;
    if (operation.metadata.supportPart !== 'beam') continue;
    const lineId = operation.metadata.lineId;
    supports.set(lineId, [...(supports.get(lineId) ?? []), operation]);
  }
  const issues = [];
  for (const operation of plan.operations) {
    if (operation.type !== 'connection.create') continue;
    if (operation.connection.routePolicy.kind === 'buried') continue;
    const lineId = operation.connection.metadata?.lineId;
    const from = resolvePlanPort(instances, definitions, operation.connection.from);
    const to = resolvePlanPort(instances, definitions, operation.connection.to);
    if (!lineId || !from || !to) continue;
    const route = coarsePipeRoute(operation.connection, from, to);
    const projected = (supports.get(lineId) ?? []).map((support) => (
      projectPointToRoute(support.transform.translation, route)
    ));
    const valid = projected.filter(({ distance }) => distance <= tolerance);
    if (valid.length === 0) {
      issues.push({
        code: 'PIPE_SUPPORT_MISSING',
        message: `${operation.name} has no support on ${lineId}`,
      });
      continue;
    }
    if (valid.length !== projected.length) {
      issues.push({
        code: 'PIPE_SUPPORT_OFF_ROUTE',
        message: `${operation.name} has a support more than ${tolerance} m off-route`,
      });
    }
    const total = routeLength(route);
    const positions = [0, ...valid.map(({ arc }) => arc), total]
      .sort((left, right) => left - right);
    for (let index = 1; index < positions.length; index += 1) {
      if (positions[index] - positions[index - 1] > maxSpacing + 1e-5) {
        issues.push({
          code: 'PIPE_SUPPORT_SPACING',
          message: `${operation.name} support gap exceeds ${maxSpacing} m`,
        });
        break;
      }
    }
  }
  return { ok: issues.length === 0, issues };
}
```

Before this function, refactor the existing local `resolvePort`, route projection and route
length logic into module-level `resolvePlanPort`, `projectPointToRoute` and
`routeLength` helpers. `projectPointToRoute(point, route)` returns
`{ arc, distance }`; `routeLength(route)` sums every 3D segment.

```bash
node --test scripts/tests/wastewater-plant-model.test.mjs
```

Expected: all asset tests PASS.

- [ ] **Step 5: Commit**

```bash
git add scripts/wastewater-plant scripts/tests/wastewater-plant-model.test.mjs
git commit -m "feat(asset): generate route clearance and pipe supports"
```

### Task 9: Rebuild and Validate the Wastewater Project

**Files:**
- Regenerate: `projects/wastewater-plant/**`
- Modify: `projects/README.md`

**Interfaces:**
- Consumes: Task 8 generator.
- Produces: public-API-created project satisfying route safety.

- [ ] **Step 1: Generate into a temporary workspace**

```bash
route_workspace=$(mktemp -d)
node scripts/build-wastewater-model.mjs --workspace "$route_workspace"
printf '%s\n' "$route_workspace"
```

Expected: `$route_workspace/wastewater-plant/project.json` exists.

- [ ] **Step 2: Validate through Studio Server**

Start:

```bash
STUDIO_SESSION_TOKEN=route-safety-review \
STUDIO_VIEWER_DIR="$PWD/apps/viewer/dist" \
  target/release/studio-cli serve \
  --workspace "$route_workspace" \
  --project wastewater-plant \
  --port 8151 \
  --write-mode review-required
```

Call:

```bash
curl -s http://127.0.0.1:8151/rpc \
  -H 'authorization: Bearer route-safety-review' \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"validation.run","params":{}}'
```

Expected: `result` is an empty array.

- [ ] **Step 3: Update the tracked fixture through the generator**

```bash
node scripts/build-wastewater-model.mjs --workspace ./projects
```

Expected: only generated project files change; no revision JSON is hand-edited.

- [ ] **Step 4: Verify project statistics and document revision**

```bash
node --test scripts/tests/wastewater-plant-model.test.mjs
cargo test -p studio-core --test route_safety
```

Expected: PASS. Record old and new revision/statistics in the implementation report.

- [ ] **Step 5: Commit**

```bash
git add projects/wastewater-plant projects/README.md
git commit -m "asset(wastewater): satisfy route safety gate"
```

### Task 10: Performance Benchmarks

**Files:**
- Create: `crates/studio-cli/src/route_safety_report.rs`
- Create: `crates/studio-cli/tests/route_safety_report.rs`
- Modify: `crates/studio-cli/src/main.rs`
- Create: `scripts/benchmark-route-safety.mjs`
- Create: `scripts/tests/route-safety-benchmark.test.mjs`
- Modify: `scripts/run-large-industrial-fixture.mjs`
- Modify: `package.json`

**Interfaces:**
- Consumes: generated wastewater project and large-industrial fixture.
- Produces:
  - `studio-cli route-safety-report --project <dir>` with JSON
    `{ revision, diagnostics, stats }`.
  - `output/route-safety-benchmark.json` with p50/p95 and counters.

- [ ] **Step 1: Write failing benchmark report tests**

```js
test('route safety report enforces latency and memory limits', () => {
  const report = summarizeRouteSafetyRuns([
    { durationMs: 80, peakBytes: 20 * 1024 * 1024 },
    { durationMs: 100, peakBytes: 24 * 1024 * 1024 },
  ]);
  assert.equal(report.p95Ms, 100);
  assert.equal(report.withinLimits, true);
  assert.throws(
    () => summarizeRouteSafetyRuns([
      { durationMs: 260, peakBytes: 70 * 1024 * 1024 },
    ]),
    /route safety performance gate failed/,
  );
});
```

Also add in `crates/studio-cli/tests/route_safety_report.rs`:

```rust
#[test]
fn route_safety_report_serializes_counters_and_diagnostics() {
    let report = build_report(
        7,
        Vec::new(),
        studio_core::large_scene::route_safety::RouteSafetyStats {
            obstacle_owners: 12,
            route_segments: 34,
            broad_phase_candidates: 5,
            narrow_phase_pairs: 2,
            triangle_pairs: 100,
        },
    );
    assert_eq!(report["revision"], 7);
    assert_eq!(report["stats"]["obstacleOwners"], 12);
    assert_eq!(report["stats"]["trianglePairs"], 100);
    assert_eq!(report["diagnostics"], serde_json::json!([]));
}
```

- [ ] **Step 2: Verify RED**

```bash
node --test scripts/tests/route-safety-benchmark.test.mjs
```

Expected: FAIL because report helpers do not exist.

- [ ] **Step 3: Implement benchmark execution**

Export `summarizeRouteSafetyRuns`. The script must:

- generate a temporary wastewater project through
  `scripts/build-wastewater-model.mjs`;
- run `studio-cli route-safety-report --project <generated-project>` to capture counters;
- start `studio-cli serve` on port `0`;
- call `validation.run` 5 times for warmup and 20 times for measurement;
- record p50, p95 and the maximum preflight counters returned by the server;
- fail when wastewater p95 exceeds `150ms` or additional RSS exceeds `64 MiB`.

Add `"benchmark:route-safety": "node scripts/benchmark-route-safety.mjs"`.

The CLI report opens the project read-only through `ProjectStore`, calls
`validate_route_safety`, serializes the report to stdout and prints the same evidence to the
report file only when the benchmark script passes `--output`.

- [ ] **Step 4: Add the large-fixture gate and verify GREEN**

In `run-large-industrial-fixture.mjs`, time 20 `validation.run` calls after commit and fail
when p95 exceeds `250ms`. Close the server, run `route-safety-report` for counters, then
include both timing and counters in `output/large-industrial-fixture.json`.

```bash
node --test scripts/tests/route-safety-benchmark.test.mjs
npm run benchmark:route-safety
```

Expected: both PASS and machine-readable reports exist.

- [ ] **Step 5: Commit**

```bash
git add scripts/benchmark-route-safety.mjs \
  scripts/tests/route-safety-benchmark.test.mjs \
  scripts/run-large-industrial-fixture.mjs package.json
git commit -m "bench(route): gate route safety latency and memory"
```

### Task 11: Documentation, Chrome Acceptance and Status

**Files:**
- Modify: `docs/reference/api.md`
- Modify: `docs/reference/capabilities.md`
- Modify: `docs/reference/diagnostics.md`
- Modify: `docs/plans/2026-11-19-route-safety-gate.md`
- Modify: `docs/specs/2026-11-19-route-safety-gate-design.md`
- Modify: `docs/README.md`
- Output: `output/route-safety-*.png`

**Interfaces:**
- Consumes: completed implementation and benchmark.
- Produces: accurate Implemented status and Chrome evidence.

- [ ] **Step 1: Update references**

Document API `1.4`, five diagnostics, metadata contract, default `6m` spacing, endpoint
`1.5m` exemption, budget values and the read-only `route-safety-report` benchmark CLI.

- [ ] **Step 2: Run Chrome visual review**

Start the latest wastewater project with `npm run dev`, then inspect in the installed
Google Chrome:

```text
http://127.0.0.1:8149/?token=wastewater-review&target=300,4,0&view=45,26,360
http://127.0.0.1:8149/?token=wastewater-review&target=300,4,35&view=45,24,58
http://127.0.0.1:8149/?token=wastewater-review&target=190,2,20&view=35,18,40
http://127.0.0.1:8149/?token=wastewater-review&target=296,4,-125&view=25,16,70
```

Save screenshots as:

```text
output/route-safety-overview.png
output/route-safety-process.png
output/route-safety-supports.png
output/route-safety-rack.png
```

- [ ] **Step 3: Run full verification**

```bash
PLAYWRIGHT_CHANNEL=chrome npm run verify
```

Expected: all 34 existing gates plus route safety tests PASS; dependency audit reports zero
high-severity vulnerabilities.

- [ ] **Step 4: Close status and self-review**

Change PLAN-030 and SPEC-033 to `Implemented` only after Core, asset, benchmark and Chrome
gates pass. Update SPEC-013 and `projects/README.md` to state that stage 2B safety checks
are complete while fittings remain follow-up work.

Run:

```bash
npm run docs:check
git diff --check
```

Expected: documentation contract passes and no untracked test artifacts remain.

- [ ] **Step 5: Commit**

```bash
git add docs projects/README.md
git commit -m "docs(route): close route safety acceptance"
```

## 4. Final Verification

Run from the repository root:

```bash
PLAYWRIGHT_CHANNEL=chrome npm run verify
npm run benchmark:route-safety
git status --short --branch
git log --oneline --decorate -10
```

Expected:

- clean worktree;
- no remote push;
- every route safety test passes;
- both performance reports are inside their limits;
- author and committer are `damoqiongqiu <damoqiongqiu@126.com>`.
