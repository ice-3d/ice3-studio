# Agent Iteration Impact Analysis Implementation Plan

> **Status:** Implemented

**Goal:** Add `changeset.impact` so an Agent can quantify whether a proposed iterative change is
local, targeted, broad, or requires a full scene rebuild before requesting approval.

**Architecture:** Studio Core defines the bounded impact result contract. Studio Server evaluates a
ChangeSet through the existing candidate path, runs the existing incremental scene planner against
the candidate Document, and returns counts only. Existing `changeset.evaluate` and
`changeset.commit` contracts remain unchanged.

**Tech Stack:** Rust 2024, Tokio, JSON-RPC 2.0, `serde`, `schemars`, `ts-rs`, TypeScript/React
Viewer contracts, Node.js 24.22.1, and Playwright.

**Spec:** `docs/specs/2026-10-31-agent-iteration-impact-design.md`

## Global Constraints

- `changeset.impact` never mutates the committed Document or advances its revision.
- Existing `changeset.evaluate`, `changeset.commit`, and public response shapes remain unchanged.
- Candidate validation uses the existing ChangeSet atomic transaction path.
- The analysis may populate content-addressed derived geometry cache entries but must not merge
  the candidate scene into the active scene snapshot or emit revision events.
- `changeset.impact` requires proposal authority in review-required mode.
- Impact responses contain counts and reasons only; never complete node, derived-node, or geometry
  arrays.
- Blocked and stale candidates return `scope: "unavailable"` and do not return partial scene
  forecasts.
- A successful impact response returns the same candidate digest that commit will verify.
- Use Node.js `24.22.1`; do not introduce Vite or Vitest.
- Do not use competitor or vendor names in code, comments, tests, documentation, links, or commits.
- Git author and committer name is `damoqiongqiu`.
- Root verification is
  `PATH=/Users/felix/.nvm/versions/node/v24.22.1/bin:$PATH npm run verify`.
- Use TDD for every production behavior change and commit each completed task.

## Review Focus

- A caller can evaluate a candidate, alter the ChangeSet, then request impact with a stale version;
  the request must fail instead of mixing versions.
- Impact analysis must not replace `scene_snapshot`, publish a scene patch, or emit a revision event.
- A blocked or stale candidate must report `unavailable`, never misleading local/zero-impact data.
- Component and relation changes must not be misclassified as a one-node local edit.
- Impact cache and generated-geometry counts must match post-commit scene statistics for the same
  warm cache state and must not include unrelated derived nodes.

---

## File Structure

- `crates/studio-core/src/agent_contract.rs`: backend-neutral impact enums and result structures.
- `crates/studio-core/tests/agent_contract.rs`: serialization and generated TypeScript checks.
- `crates/studio-server/src/iteration_impact.rs`: source diff counting, scene dependency analysis,
  scope classification, and impact reason derivation.
- `crates/studio-server/src/changeset.rs`: public request/result contracts, evaluation invocation,
  and `changeset.impact` dispatch.
- `crates/studio-server/src/rpc.rs`: mutation classification for the new proposal method.
- `crates/studio-server/src/capabilities.rs`: authorization classification if the proposal method
  list needs updating.
- `crates/studio-server/src/agent.rs`: manifest tool and workflow advertisement.
- `crates/studio-cli/src/schema.rs`: generated API schema registration.
- `crates/studio-server/tests/agent_iteration_impact.rs`: end-to-end impact, cache, full-rebuild,
  stale, blocked, permissions, and digest tests.
- `crates/studio-server/tests/agent_api.rs`: manifest schema coverage.
- `docs/reference/api.md`, `docs/reference/change-sets.md`, `docs/reference/diagnostics.md`,
  `docs/README.md`: public behavior and workflow documentation.

### Task 1: Define Impact Contracts

**Files:**
- Modify: `crates/studio-core/src/agent_contract.rs`
- Test: `crates/studio-core/tests/agent_contract.rs`

**Interfaces:**
- Consumes: `serde`, `schemars`, and `ts_rs` patterns already used by Agent contracts.
- Produces: `ChangeImpactCounts`, `IterationImpactScope`, `IterationImpactReason`,
  and `IterationImpact`.

Derive `Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS` and export TypeScript
for all four types. Use camelCase serialization and TypeScript names.

```rust
pub struct ChangeImpactCounts {
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
}

pub enum IterationImpactScope {
    Local,
    Targeted,
    Broad,
    FullRebuild,
    Unavailable,
}

#[serde(rename_all = "snake_case")]
pub enum IterationImpactReason {
    NodeContent,
    NodeTransform,
    InstanceParameters,
    MaterialAssignment,
    ComponentDefinition,
    RelationTopology,
    Constraint,
    FullRebuild,
    ValidationBlocked,
}

pub struct IterationImpact {
    pub scope: IterationImpactScope,
    pub full_scene_rebuild_required: bool,
    pub source_nodes: ChangeImpactCounts,
    pub derived_nodes: ChangeImpactCounts,
    pub relations: ChangeImpactCounts,
    pub components: ChangeImpactCounts,
    pub materials: ChangeImpactCounts,
    pub affected_source_nodes: usize,
    pub affected_derived_nodes: usize,
    pub reused_geometry: usize,
    pub generated_geometry: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub reasons: Vec<IterationImpactReason>,
}
```

- [ ] **Step 1: Write the failing contract test**

Add to `crates/studio-core/tests/agent_contract.rs`:

```rust
#[test]
fn iteration_impact_serializes_with_counts_and_snake_case_reasons() {
    let impact = IterationImpact {
        scope: IterationImpactScope::Local,
        full_scene_rebuild_required: false,
        source_nodes: ChangeImpactCounts::default(),
        derived_nodes: ChangeImpactCounts::default(),
        relations: ChangeImpactCounts::default(),
        components: ChangeImpactCounts::default(),
        materials: ChangeImpactCounts::default(),
        affected_source_nodes: 1,
        affected_derived_nodes: 64,
        reused_geometry: 64,
        generated_geometry: 0,
        cache_hits: 64,
        cache_misses: 0,
        reasons: vec![IterationImpactReason::InstanceParameters],
    };
    let value = serde_json::to_value(impact).unwrap();

    assert_eq!(value["scope"], "local");
    assert_eq!(value["fullSceneRebuildRequired"], false);
    assert_eq!(value["affectedSourceNodes"], 1);
    assert_eq!(value["generatedGeometry"], 0);
    assert_eq!(value["reasons"][0], "instance_parameters");
}
```

- [ ] **Step 2: Run the test and observe the missing types**

Run:

```bash
cargo test -p studio-core --test agent_contract iteration_impact_serializes
```

Expected: compilation fails because the four impact types are not defined.

- [ ] **Step 3: Implement the contracts**

Place the definitions near the other Agent result structures. Implement `Default` for
`ChangeImpactCounts` by deriving it. Do not add optional or nullable fields; every field in the
result is always present.

- [ ] **Step 4: Run contract tests**

Run:

```bash
cargo test -p studio-core --test agent_contract
cargo test -p studio-core export_bindings
```

Expected: all tests pass and the four new TypeScript files exist.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-core/src/agent_contract.rs \
  crates/studio-core/tests/agent_contract.rs \
  crates/studio-core/bindings/ChangeImpactCounts.ts \
  crates/studio-core/bindings/IterationImpactScope.ts \
  crates/studio-core/bindings/IterationImpactReason.ts \
  crates/studio-core/bindings/IterationImpact.ts
git commit -m "feat(studio-core): define agent iteration impact contracts"
```

### Task 2: Analyze Scene Impact

**Files:**
- Create: `crates/studio-server/src/iteration_impact.rs`
- Modify: `crates/studio-server/src/lib.rs`
- Modify: `crates/studio-server/src/scene_session.rs:33-51`
- Test: `crates/studio-server/tests/agent_iteration_impact.rs`

**Interfaces:**

- Consumes: `Session`, `DocumentDiff`, `Operation`, `IterationImpact`,
  `SceneSourceIndex`, `build_scene_patch_with_scene`, and `ComputeEvaluator`.
- Produces:
  `pub(crate) fn analyze_impact(session: &mut Session, candidate: &Document, diff:
  &DocumentDiff, operations: &[Operation]) -> Result<IterationImpact, RpcError>`.

Add `cache_hits`, `cache_misses`, and `generated_meshes` to `BuiltScenePatch` by copying the
corresponding fields from the selected or full `EvaluatedScene.stats`. This avoids discarding
evaluation evidence before the server merges a committed scene.

```rust
pub struct BuiltScenePatch {
    pub patch: ScenePatch,
    pub scene: EvaluatedScene,
    pub affected_source_nodes: Option<BTreeSet<NodeId>>,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub generated_meshes: usize,
}
```

Populate those fields in both return paths of `build_scene_patch_with_scene`. Update
`merge_scene_snapshot` to use the `BuiltScenePatch` counters instead of reading selected scene
stats indirectly.

Impact analysis must:

1. call `ensure_scene_snapshot(session)`;
2. borrow the previous snapshot and `SceneSourceIndex`;
3. call `build_scene_patch_with_scene` with the candidate Document, diff, current cache, and
   `ComputeEvaluator`;
4. derive counts without modifying `session.scene_snapshot`;
5. classify the result with the deterministic rules from the spec.

Use these reason rules:

- `InstancePatch.transform.is_some()` or `NodeUpdate.transform.is_some()`:
  `NodeTransform`;
- `InstancePatch` with set/remove parameters: `InstanceParameters`;
- `InstancePatch.material_id` or `clear_material`: `MaterialAssignment`;
- `MaterialPatch`, `MaterialUpsert`, or `MaterialAssign`: `MaterialAssignment`;
- component operations: `ComponentDefinition`;
- connection or relation operations: `RelationTopology`;
- constraint operations: `Constraint`;
- all other source-node changes: `NodeContent`.

Scope classification:

```rust
let source_change_count = diff.added_node_ids.len()
    + diff.updated_node_ids.len()
    + diff.removed_node_ids.len();
let structural_change = !diff.added_component_ids.is_empty()
    || !diff.updated_component_ids.is_empty()
    || !diff.removed_component_ids.is_empty()
    || !diff.added_relation_ids.is_empty()
    || !diff.updated_relation_ids.is_empty()
    || !diff.removed_relation_ids.is_empty()
    || !diff.added_constraint_ids.is_empty()
    || !diff.updated_constraint_ids.is_empty()
    || !diff.removed_constraint_ids.is_empty();
```

`affected_source_nodes == None` means `FullRebuild`. Otherwise use `Local` when
`source_change_count <= 1 && !structural_change`, `Broad` when affected derived nodes are greater
than 1,000, and `Targeted` for the remainder.

Derived counts compare the candidate fragment IDs against the source index's previous expanded IDs:

- added: candidate IDs not in the previous affected closure;
- updated: candidate IDs in both sets;
- removed: previous affected IDs not in the candidate ID set.

- [ ] **Step 1: Write failing cache-evidence tests**

Create `crates/studio-server/tests/agent_iteration_impact.rs` with helpers that create a
parameterized component scene, write it through `document.apply` using the approval cookie, warm
`evaluation.stats`, and build a ChangeSet containing one `instance.patch`.

The first test:

```rust
#[tokio::test]
async fn impact_reuses_unchanged_part_geometry_for_local_patch() {
    let server = start_test_server(ServerConfig::review_required("impact-local")).await;
    seed_parameterized_box(&server).await;
    warm_stats(&server).await;

    let changes = create_change_set_with_operations(
        &server,
        vec![instance_patch_transform("node-000001")],
    )
    .await;
    let impact = rpc_with_bearer(
        &server,
        server.agent_token(),
        "changeset.impact",
        serde_json::json!({
            "changeSetId": changes["id"],
            "expectedVersion": changes["version"]
        }),
    )
    .await;

    assert_eq!(impact["result"]["impact"]["scope"], "local", "{impact}");
    assert_eq!(impact["result"]["impact"]["generatedGeometry"], 0, "{impact}");
    assert!(
        impact["result"]["impact"]["cacheHits"].as_u64().unwrap() > 0,
        "{impact}"
    );
}
```

Add the unbound-parameter and full-rebuild cases:

```rust
#[tokio::test]
async fn impact_reuses_geometry_for_an_unbound_parameter_change() {
    let server = start_test_server(ServerConfig::review_required("impact-parameter")).await;
    seed_parameterized_box(&server).await;
    warm_stats(&server).await;

    let changes = create_change_set_with_operations(
        &server,
        vec![instance_patch_parameter("node-000001", "unused", 2.0)],
    )
    .await;
    let impact = request_impact(&server, &changes).await;

    assert_eq!(impact["scope"], "local", "{impact}");
    assert_eq!(impact["generatedGeometry"], 0, "{impact}");
    assert!(impact["cacheHits"].as_u64().unwrap() > 0, "{impact}");
    assert_eq!(impact["reasons"][0], "instance_parameters", "{impact}");
}

#[tokio::test]
async fn relation_change_requires_full_scene_rebuild() {
    let server = start_test_server(ServerConfig::review_required("impact-relation")).await;
    seed_parameterized_box(&server).await;
    warm_stats(&server).await;

    let changes = create_change_set_with_operations(
        &server,
        vec![relation_update("relation-000001")],
    )
    .await;
    let impact = request_impact(&server, &changes).await;

    assert_eq!(impact["scope"], "full_rebuild", "{impact}");
    assert_eq!(impact["fullSceneRebuildRequired"], true, "{impact}");
    assert!(impact["reasons"]
        .as_array()
        .unwrap()
        .iter()
        .any(|reason| reason == "full_rebuild"));
}
```

Define `request_impact` in the test module:

```rust
async fn request_impact(
    server: &TestServer,
    change_set: &serde_json::Value,
) -> serde_json::Value {
    let response = rpc_with_bearer(
        server,
        server.agent_token(),
        "changeset.impact",
        serde_json::json!({
            "changeSetId": change_set["id"],
            "expectedVersion": change_set["version"]
        }),
    )
    .await;
    response["result"]["impact"].clone()
}
```

- [ ] **Step 2: Run the tests and verify RED**

Run:

```bash
cargo test -p studio-server --test agent_iteration_impact
```

Expected: the `changeset.impact` calls fail because the method is unknown.

- [ ] **Step 3: Implement `BuiltScenePatch` counters and impact analysis**

Create `iteration_impact.rs`, export it from `lib.rs`, and implement the analysis rules above.
Do not call `merge_scene_snapshot`, `apply_scene_patch`, or any event emitter.

- [ ] **Step 4: Run focused tests**

Run:

```bash
cargo test -p studio-server --test agent_iteration_impact
cargo test -p studio-server --test scene_patch
cargo test -p studio-server --test scene_session_rpc
```

Expected: all tests pass once the RPC method is wired in Task 3.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-server/src/iteration_impact.rs \
  crates/studio-server/src/lib.rs \
  crates/studio-server/src/scene_session.rs \
  crates/studio-server/tests/agent_iteration_impact.rs
git commit -m "feat(studio-server): analyze incremental scene impact"
```

### Task 3: Expose `changeset.impact`

**Files:**
- Modify: `crates/studio-server/src/changeset.rs`
- Modify: `crates/studio-server/src/rpc.rs`
- Modify: `crates/studio-server/src/capabilities.rs`
- Modify: `crates/studio-server/src/agent.rs`
- Modify: `crates/studio-server/tests/agent_iteration_impact.rs`
- Modify: `crates/studio-server/tests/agent_api.rs`
- Modify: `crates/studio-server/tests/changeset_rpc.rs`

**Interfaces:**

- Consumes: Task 1 `IterationImpact` and Task 2 `analyze_impact`.
- Produces:
  `ChangeSetImpactParams`, `ChangeSetImpactResult`, and RPC method
  `changeset.impact`.

```rust
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChangeSetImpactParams {
    change_set_id: ChangeSetId,
    expected_version: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ChangeSetImpactResult {
    change_set_id: ChangeSetId,
    version: u64,
    base_revision: Revision,
    status: ChangeSetStatus,
    candidate_digest: Option<String>,
    diagnostic_count: usize,
    impact: IterationImpact,
}
```

`dispatch` loads and validates the ChangeSet through the existing `evaluate` helper with
`include_candidate: true`. For `Ready`, invoke `analyze_impact` on the candidate and diff. For
`Blocked` and `Stale`, return an unavailable impact containing zero counts, the diagnostic count,
and `ValidationBlocked`; do not run scene analysis. Return the candidate digest unchanged.

Add `changeset.impact` to:

- `changeset::dispatch`;
- the mutating method list in `rpc.rs`;
- the proposal-method authorization list;
- `system.capabilities.methods`;
- Agent manifest tools with proposal access, input `ChangeSetImpactParams`, and output
  `ChangeSetImpactResult`;
- the manifest `propose` workflow.

- [ ] **Step 1: Extend the failing impact tests**

Add the digest and stale-version test:

```rust
#[tokio::test]
async fn impact_digest_matches_evaluate_and_version_drift_fails_closed() {
    let server = start_test_server(ServerConfig::review_required("impact-digest")).await;
    seed_parameterized_box(&server).await;
    warm_stats(&server).await;
    let changes = create_change_set_with_operations(
        &server,
        vec![instance_patch_transform("node-000001")],
    )
    .await;

    let impact = rpc_with_bearer(
        &server,
        server.agent_token(),
        "changeset.impact",
        serde_json::json!({
            "changeSetId": changes["id"],
            "expectedVersion": changes["version"]
        }),
    )
    .await;
    let evaluated = rpc_with_bearer(
        &server,
        server.agent_token(),
        "changeset.evaluate",
        serde_json::json!({
            "changeSetId": changes["id"],
            "expectedVersion": changes["version"],
            "includeCandidate": false
        }),
    )
    .await;

    assert_eq!(
        impact["result"]["candidateDigest"],
        evaluated["result"]["candidateDigest"],
        "{impact}\n{evaluated}"
    );

    rpc_with_bearer(
        &server,
        server.agent_token(),
        "changeset.addOperations",
        serde_json::json!({
            "changeSetId": changes["id"],
            "expectedVersion": changes["version"],
            "entries": [{
                "source": { "kind": "agent", "label": "drift" },
                "operation": instance_patch_transform("node-000001")
            }]
        }),
    )
    .await;
    let stale = rpc_with_bearer(
        &server,
        server.agent_token(),
        "changeset.impact",
        serde_json::json!({
            "changeSetId": changes["id"],
            "expectedVersion": changes["version"]
        }),
    )
    .await;
    assert_eq!(stale["error"]["diagnosticCode"], "ICE3_STUDIO_OPERATION_INVALID");
}
```

Add the blocked test:

```rust
#[tokio::test]
async fn blocked_candidate_impact_is_unavailable() {
    let server = start_test_server(ServerConfig::review_required("impact-blocked")).await;
    seed_parameterized_box(&server).await;
    let changes = create_change_set_with_operations(
        &server,
        vec![missing_instance("node-999999")],
    )
    .await;
    let result = request_impact_result(&server, &changes).await;

    assert_eq!(result["status"], "blocked", "{result}");
    assert_eq!(result["impact"]["scope"], "unavailable", "{result}");
    assert_eq!(result["impact"]["generatedGeometry"], 0, "{result}");
    assert!(result["candidateDigest"].is_null(), "{result}");
}
```

Extend `manifest_access_matches_request_authority` with:

```rust
assert!(!tool_available(&read_only, "changeset.impact"), "{read_only}");
assert!(tool_available(&agent, "changeset.impact"), "{agent}");
assert!(tool_available(&approval, "changeset.impact"), "{approval}");
```

Extend the expected tool and schema-property loops with:

```rust
("changeset.impact", "expectedVersion")
```

Extend the capabilities test in `changeset_rpc.rs` with:

```rust
let caps = rpc(&server, "system.capabilities", serde_json::json!({})).await;
assert!(caps["result"]["methods"]
    .as_array()
    .unwrap()
    .iter()
    .any(|method| method == "changeset.impact"));
```

- [ ] **Step 2: Run the focused tests and verify RED**

Run:

```bash
cargo test -p studio-server --test agent_iteration_impact
cargo test -p studio-server --test agent_api manifest_access
```

Expected: the impact method is missing or unavailable and the manifest does not contain it.

- [ ] **Step 3: Implement dispatch, authorization, and manifest entry**

The impact call must reuse the existing evaluation path rather than duplicating transaction
validation. Preserve existing method behavior and response schemas.

- [ ] **Step 4: Run server tests**

Run:

```bash
cargo test -p studio-server --test agent_iteration_impact
cargo test -p studio-server --test agent_api
cargo test -p studio-server --test changeset_rpc
cargo test -p studio-server --test scene_session_rpc
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-server/src/changeset.rs \
  crates/studio-server/src/rpc.rs \
  crates/studio-server/src/capabilities.rs \
  crates/studio-server/src/agent.rs \
  crates/studio-server/tests/agent_iteration_impact.rs \
  crates/studio-server/tests/agent_api.rs \
  crates/studio-server/tests/changeset_rpc.rs
git commit -m "feat(studio-server): expose agent iteration impact"
```

### Task 4: Generate Contracts And Documentation

**Files:**
- Modify: `crates/studio-cli/src/schema.rs`
- Modify: `docs/reference/api.md`
- Modify: `docs/reference/change-sets.md`
- Modify: `docs/reference/diagnostics.md`
- Modify: `docs/specs/2026-10-31-agent-iteration-impact-design.md`
- Modify: `docs/plans/2026-10-31-agent-iteration-impact.md`
- Modify: `docs/README.md`
- Generated: `schemas/studio-api.schema.json`
- Generated: `schemas/studio-types.ts`
- Generated: `crates/studio-server/bindings/*.ts`
- Generated: `crates/studio-core/bindings/*.ts`

**Interfaces:**

- Consumes: all public impact contracts from Tasks 1 and 3.
- Produces: generated JSON Schema and TypeScript declarations plus public usage documentation.

- [ ] **Step 1: Add a failing schema assertion**

Extend `crates/studio-cli/tests/schema_output.rs`:

```rust
assert!(text.contains("changeset.impact"));
let schema: serde_json::Value = serde_json::from_slice(&first).unwrap();
assert!(schema["types"].get("ChangeSetImpactParams").is_some());
assert!(schema["types"].get("ChangeSetImpactResult").is_some());
assert!(schema["types"].get("IterationImpact").is_some());
```

- [ ] **Step 2: Run the schema test and verify RED**

Run:

```bash
cargo test -p studio-cli --test schema_output
```

Expected: failure because the method and types are absent.

- [ ] **Step 3: Register types and generate bindings**

Add `ChangeSetImpactParams`, `ChangeSetImpactResult`, and `IterationImpact` to
`crates/studio-cli/src/schema.rs`. Run:

```bash
cargo test -p studio-core export_bindings
cargo test -p studio-server export_bindings
cargo run -q -p studio-cli -- schema --output schemas
```

- [ ] **Step 4: Document the workflow**

In the API reference, show an Agent sequence:

```text
changeset.create
changeset.addOperations
changeset.impact
changeset.commit
```

Document that `changeset.impact` persists candidate evaluation state, returns counts rather than
complete diff arrays, and may warm the content-addressed cache. Document post-commit
`evaluation.stats` as the exact execution evidence for the committed revision.

Set the spec and plan status to `Implemented` only after the final acceptance run succeeds.

- [ ] **Step 5: Run schema and documentation checks**

Run:

```bash
cargo test -p studio-cli --test schema_output
rg -n "TBD|TODO|FIXME|PLACEHOLDER" docs/reference docs/specs docs/plans || true
```

Expected: schema test passes and the placeholder scan prints nothing.

- [ ] **Step 6: Commit**

```bash
git add crates/studio-cli/src/schema.rs \
  crates/studio-cli/tests/schema_output.rs \
  crates/studio-core/bindings crates/studio-server/bindings \
  schemas/studio-api.schema.json schemas/studio-types.ts \
  docs/reference/api.md docs/reference/change-sets.md \
  docs/reference/diagnostics.md docs/README.md \
  docs/specs/2026-10-31-agent-iteration-impact-design.md \
  docs/plans/2026-10-31-agent-iteration-impact.md
git commit -m "docs(studio): document agent iteration impact"
```

### Task 5: Current Agent Acceptance

**Files:**
- Create: `scripts/agent-iteration-impact-check.mjs`
- Create: `scripts/tests/agent-iteration-impact.test.mjs`
- Modify: `scripts/run-ai-fixture.mjs`
- Modify: `scripts/verify.mjs`
- Create during acceptance: `output/agent-iteration-impact.json`
- Create during acceptance: `output/agent-iteration-impact.png`

**Interfaces:**

- Consumes: public `changeset.impact` method and existing large wastewater acceptance fixture.
- Produces:
  `assertImpactInvariants(result, { expectedScope, maxGeneratedGeometry })` and
  machine-readable evidence that the current Agent can repeat local changes without full rebuilds.

- [ ] **Step 1: Write a failing script-level regression test**

Create the test importing a helper that does not exist yet:

```javascript
import assert from 'node:assert/strict';
import test from 'node:test';
import { assertImpactInvariants } from '../agent-iteration-impact-check.mjs';

test('accepts a cached local material or parameter iteration', () => {
  assert.doesNotThrow(() => assertImpactInvariants({
    scope: 'local',
    fullSceneRebuildRequired: false,
    generatedGeometry: 0,
    cacheHits: 64,
    reasons: ['instance_parameters'],
  }, {
    expectedScope: 'local',
    maxGeneratedGeometry: 0,
  }));
});

test('rejects a full rebuild reported as local', () => {
  assert.throws(() => assertImpactInvariants({
    scope: 'local',
    fullSceneRebuildRequired: true,
    generatedGeometry: 0,
    cacheHits: 64,
    reasons: ['instance_parameters'],
  }, {
    expectedScope: 'local',
    maxGeneratedGeometry: 0,
  }));
});
```

- [ ] **Step 2: Run the test and observe the missing helper export**

Run:

```bash
node --test scripts/tests/agent-iteration-impact.test.mjs
```

Expected: failure because `scripts/agent-iteration-impact-check.mjs` does not exist.

- [ ] **Step 3: Implement the assertion helper and wire it into verification**

Create the helper with these exact invariants:

```javascript
import assert from 'node:assert/strict';

export function assertImpactInvariants(result, options) {
  assert.equal(result.scope, options.expectedScope);
  assert.equal(
    result.fullSceneRebuildRequired,
    result.scope === 'full_rebuild',
  );
  assert.ok(result.generatedGeometry <= options.maxGeneratedGeometry);
  assert.ok(Array.isArray(result.reasons));

  if (result.scope === 'local' || result.scope === 'targeted') {
    assert.equal(result.fullSceneRebuildRequired, false);
  }
  if (result.scope === 'local' && !result.reasons.includes('node_content')) {
    assert.ok(result.cacheHits > 0);
  }
  if (result.scope === 'unavailable') {
    assert.equal(result.candidateDigest, null);
  }
}
```

Call the helper from the live Agent acceptance script for every valid round:

```javascript
assertImpactInvariants(round.impact, {
  expectedScope: round.scope,
  maxGeneratedGeometry: round.maxGeneratedGeometry,
});
```

Wire the helper into the existing AI fixture gate without another server startup:

```javascript
import { assertImpactInvariants } from './agent-iteration-impact-check.mjs';

assertImpactInvariants({
  scope: 'local',
  fullSceneRebuildRequired: false,
  generatedGeometry: 0,
  cacheHits: 64,
  candidateDigest: 'fixture-digest',
  reasons: ['instance_parameters'],
}, {
  expectedScope: 'local',
  maxGeneratedGeometry: 0,
});
```

Place that block at module initialization in `scripts/run-ai-fixture.mjs`. The existing
`scripts/verify.mjs` command already runs this file, so the invariant becomes part of the 14-gate
suite without adding another heavy gate.

- [ ] **Step 4: Run deterministic verification**

Run:

```bash
node --test scripts/tests/agent-iteration-impact.test.mjs
PATH=/Users/felix/.nvm/versions/node/v24.22.1/bin:$PATH npm run verify
```

Expected: the focused test passes and all 14 verification gates pass.

- [ ] **Step 5: Run the current Agent acceptance workflow**

Start a review-required Studio server against the existing detailed wastewater project. As the
current AI Agent:

1. call `system.agentManifest`;
2. make a material-only ChangeSet and call `changeset.impact`;
3. modify one instance transform and unbound parameter and call `changeset.impact`;
4. add one pump and one pipe and call `changeset.impact`;
5. submit one invalid connection and verify `scope: "unavailable"`;
6. commit the three valid revisions with approval authority;
7. compare each reported `cacheHits`, `cacheMisses`, and `generatedGeometry` with
   `evaluation.stats`;
8. capture a screenshot and write `output/agent-iteration-impact.json`.

The acceptance JSON must include:

```json
{
  "modelRevision": 4,
  "rounds": 3,
  "totalRequestBytes": 0,
  "totalWallClockMs": 0,
  "localFullRebuildCount": 0,
  "unrelatedDerivedNodeChanges": 0,
  "metricsMatchedPostCommit": true
}
```

Populate the numeric fields from actual execution; the zero values above are schema examples only
and must not remain in the generated report.

- [ ] **Step 6: Commit**

```bash
git add scripts/agent-iteration-impact-check.mjs \
  scripts/tests/agent-iteration-impact.test.mjs scripts/run-ai-fixture.mjs scripts/verify.mjs
git commit -m "test(studio): verify local agent iteration impact"
```

### Task 6: Final Verification And Handoff

**Files:**
- Modify: `docs/specs/2026-10-31-agent-iteration-impact-design.md`
- Modify: `docs/plans/2026-10-31-agent-iteration-impact.md`

**Interfaces:**

- Consumes: all implementation, generated artifacts, and current Agent acceptance evidence.
- Produces: a final verified branch and review package.

- [ ] **Step 1: Run the full verification suite**

Run:

```bash
PATH=/Users/felix/.nvm/versions/node/v24.22.1/bin:$PATH npm run verify
```

Expected: all 14 verification gates pass.

- [ ] **Step 2: Re-run current Agent acceptance**

Run the workflow from Task 5 against a fresh detailed wastewater project and verify:

- `localFullRebuildCount` is `0`;
- unrelated derived node changes are `0`;
- material/parameter rounds generate no geometry after warm-up;
- topology round generates only local geometry;
- impact metrics equal post-commit `evaluation.stats`;
- all three valid revisions commit and reopen successfully.

- [ ] **Step 3: Mark documents implemented**

Change the spec and plan status from `Proposed` to `Implemented` only after Steps 1 and 2 pass.

- [ ] **Step 4: Commit final documentation**

```bash
git add docs/specs/2026-10-31-agent-iteration-impact-design.md \
  docs/plans/2026-10-31-agent-iteration-impact.md
git commit -m "docs(studio): mark agent iteration impact implemented"
```

- [ ] **Step 5: Produce whole-branch review evidence**

Run:

```bash
/Users/felix/.codex/skills/subagent-driven-development/scripts/review-package \
  1d5eccf HEAD
```

Expected: a review package is written under the active plan workspace. Complete one final review
before merge.

## Self-Review

- Spec coverage: Tasks 1-4 cover contracts, scene analysis, RPC authorization, manifest, schemas,
  documentation, blocked/stale behavior, cache evidence, and compatibility.
- Spec coverage: Task 5 covers repeated current-Agent validation, deliberate invalid input, approval,
  screenshot evidence, and post-commit metric comparison.
- Placeholder scan: no unfinished implementation steps or unnamed types remain. The zeros in the
  Task 5 acceptance JSON are explicitly marked as schema examples.
- Type consistency: `changeset.impact`, `ChangeSetImpactParams`,
  `ChangeSetImpactResult`, `IterationImpact`, `ChangeImpactCounts`,
  `IterationImpactScope`, and `IterationImpactReason` use the same names throughout.
- Review Focus coverage: stale versions, scene non-publication, blocked/stale status, structural
  classification, and metric parity each have explicit tests in Tasks 2, 3, or 5.
