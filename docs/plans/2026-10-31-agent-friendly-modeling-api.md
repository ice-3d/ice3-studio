# Agent-Friendly Modeling API Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a bounded, permission-aware Agent capability layer for project inspection, derived
scene discovery, targeted geometry queries, and sparse model updates without changing the
authoritative Document or existing client contracts.

**Architecture:** New backend-neutral Agent contracts live in Studio Core. The Server implements
context, source/derived search, explicit evaluated-geometry query, and a tool manifest over the
existing Document and cached evaluated scene. Sparse patch variants join the existing Operation
enum so every mutation continues through atomic transactions and ChangeSet approval.

**Tech Stack:** Rust 2024, Tokio, JSON-RPC 2.0, `serde`, `schemars`, `ts-rs`, React/TypeScript
Viewer contracts, Node.js 24.22.1, and Playwright.

**Spec:** `docs/specs/2026-10-31-agent-friendly-modeling-api-design.md`

## Global Constraints

- The Document remains the only durable modeling source of truth.
- CPU owns validation, revisions, approval, persistence, and canonical GLB export.
- Derived scene state remains disposable and may be rebuilt.
- Studio Core must not depend on Node.js, Toolchain, Viewer, Engine, or an Agent runtime.
- Proposal tokens cannot directly apply, commit, or discard.
- Existing public RPC response shapes must remain unchanged.
- New request types use camelCase JSON and `deny_unknown_fields`.
- Existing WebGL2, CPU incremental scene caching, native GPU default-off, and experimental preview
  packet decisions remain unchanged.
- Use Node.js `24.22.1`; do not introduce Vite or Vitest.
- Do not use competitor or vendor names in code, comments, tests, documentation, links, or commits.
- Git author and committer name is `damoqiongqiu`.
- Preserve unrelated dirty files in the Engine working tree.
- Root verification is `PATH=/Users/felix/.nvm/versions/node/v24.22.1/bin:$PATH npm run verify`.
- Use TDD for every production behavior change and commit each completed task.

## Review Focus

- A cursor from an older revision or a different normalized query must fail closed instead of
  returning a mixed page; Task 5 tests both cases.
- A sparse instance patch must retain omitted parameters, metadata, overrides, transform, and
  material values; Task 2 tests both retention and explicit clearing.
- Manifest permissions must match dispatch-time authorization for proposal, approval, and
  autonomous tokens; Task 4 tests all three identities.
- Derived geometry query must reject more than 32 nodes and must not return positions unless
  explicitly requested; Task 6 tests both payload rules.
- Context and default search must remain bounded on a 1,938-node scene; Task 7 records byte
  ceilings and fails the benchmark when exceeded.

## File Structure

- `crates/studio-core/src/agent_contract.rs`: Agent-facing read, manifest, search, evaluation,
  access, limit, and structured-error contracts.
- `crates/studio-core/src/patch.rs`: typed sparse patch request payloads.
- `crates/studio-core/src/operation.rs`: Operation variants that carry sparse patches.
- `crates/studio-core/src/apply.rs`: sparse patch mutation semantics and operation errors.
- `crates/studio-server/src/agent.rs`: manifest construction, project context, source/derived
  scene search, cursor handling, and evaluated geometry query.
- `crates/studio-server/src/capabilities.rs`: request authority classification and manifest access.
- `crates/studio-server/src/rpc.rs`: request-authority plumbing and new method dispatch.
- `crates/studio-core/tests/agent_contract.rs`: contract serialization and schema tests.
- `crates/studio-core/tests/agent_patches.rs`: patch semantics and atomic validation.
- `crates/studio-server/tests/agent_api.rs`: manifest, context, search, query, cursor, and permission
  integration tests.
- `crates/studio-server/tests/agent_workflow.rs`: full proposal-to-approval Agent workflow.
- `benchmarks/large-industrial-scene.mjs`: machine-readable context/search byte and paging evidence.
- `README.md`, `docs/architecture.md`, `docs/reference/api.md`,
  `docs/reference/diagnostics.md`: implemented behavior and examples.

---

### Task 1: Define Agent-Facing Contracts

**Files:**
- Create: `crates/studio-core/src/agent_contract.rs`
- Modify: `crates/studio-core/src/lib.rs`
- Test: `crates/studio-core/tests/agent_contract.rs`

**Interfaces:**
- Consumes: `Bounds`, `NodeContent`, `ComponentId`, `MaterialId`, `Revision`.
- Produces: `AgentManifest`, `SceneContext`, `SceneSearchRequest`,
  `SceneSearchResult`, `SceneSearchItem`, `EvaluationQueryRequest`,
  `EvaluationQueryResult`, `AgentErrorData`, and their supporting enums and structs.

- [ ] **Step 1: Write the failing contract test**

```rust
use std::collections::BTreeMap;

use studio_core::agent_contract::{
    AgentLimits, EvaluationInclude, EvaluationQueryRequest, SceneSearchField,
    SceneSearchRequest, SceneSearchScope,
};

#[test]
fn agent_requests_serialize_with_camel_case_and_bounded_defaults() {
    let request = SceneSearchRequest {
        scope: SceneSearchScope::Derived,
        query: None,
        node_id: None,
        owner_id: None,
        parent_id: None,
        metadata: BTreeMap::new(),
        content_kind: None,
        component_definition_id: None,
        material_id: None,
        visible: None,
        relation_type: None,
        related_node_id: None,
        bounds: None,
        fields: None,
        cursor: None,
        limit: 25,
    };
    let value = serde_json::to_value(request).unwrap();
    assert_eq!(value["scope"], "derived");
    assert_eq!(value["limit"], 25);
    assert!(value.get("node_id").is_none());

    let fields = serde_json::to_value([SceneSearchField::ComponentDefinitionId]).unwrap();
    assert_eq!(fields[0], "componentDefinitionId");

    let evaluation = EvaluationQueryRequest {
        node_ids: vec!["node-000001/body".into()],
        include: vec![EvaluationInclude::Summary],
    };
    let value = serde_json::to_value(evaluation).unwrap();
    assert_eq!(value["nodeIds"][0], "node-000001/body");
    assert_eq!(AgentLimits::default().search_max_limit, 100);
}
```

- [ ] **Step 2: Run the test and observe the missing module**

Run:

```bash
cargo test -p studio-core --test agent_contract
```

Expected: compilation fails because `studio_core::agent_contract` does not exist.

- [ ] **Step 3: Implement the contracts**

Use these concrete top-level definitions:

```rust
pub const DEFAULT_SEARCH_LIMIT: usize = 25;
pub const MAX_SEARCH_LIMIT: usize = 100;
pub const MAX_EVALUATION_NODES: usize = 32;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentAccess {
    pub can_read: bool,
    pub can_propose: bool,
    pub can_commit: bool,
    pub can_apply_directly: bool,
    pub can_discard: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentLimits {
    pub search_default_limit: usize,
    pub search_max_limit: usize,
    pub evaluation_max_nodes: usize,
    pub max_rpc_body_bytes: u64,
}

impl Default for AgentLimits {
    fn default() -> Self {
        Self {
            search_default_limit: DEFAULT_SEARCH_LIMIT,
            search_max_limit: MAX_SEARCH_LIMIT,
            evaluation_max_nodes: MAX_EVALUATION_NODES,
            max_rpc_body_bytes: 64 * 1024 * 1024,
        }
    }
}
```

Define the remaining enums and structs with camelCase serde/TS attributes:

```rust
#[serde(rename_all = "snake_case")]
pub enum SceneContentKind {
    Group,
    Primitive,
    Mesh,
    Sweep,
    ComponentInstance,
    InstanceSet,
    Connection,
}

#[serde(rename_all = "snake_case")]
pub enum SceneSearchScope {
    Source,
    Derived,
    Both,
}

#[serde(rename_all = "camelCase")]
pub enum SceneSearchField {
    Id,
    Scope,
    Kind,
    Name,
    ParentId,
    OwnerId,
    Visible,
    ComponentDefinitionId,
    MaterialId,
    GeometryId,
    Bounds,
    Metadata,
}

#[serde(rename_all = "snake_case")]
pub enum EvaluationInclude {
    Summary,
    Positions,
    Indices,
}
```

`SceneSearchRequest` contains exactly the request fields from the spec and implements
`Default` with source scope, empty metadata, and limit 25. Mark the request
`deny_unknown_fields`.

Define `SceneCounts`, `SceneContentKindCount`, `RelationTypeCount`, `SceneFacets`,
`SceneCoordinateSystem`, `SceneContext`, `SceneSearchItem`, `SceneSearchResult`,
`EvaluationQueryRequest`, `EvaluationQueryItem`, `EvaluationQueryResult`,
`AgentTool`, `AgentSideEffect`, `AgentIdempotency`, `AgentWorkflow`, and
`AgentManifest`. Use `Bounds` and `serde_json::Value` where the spec says `bounds`
and `expected`/`actual`.

Define `AgentErrorCategory` with snake_case serialization and
`AgentErrorData` with camelCase fields and `deny_unknown_fields`.

Export all named types from `studio-core` using:

```rust
pub mod agent_contract;
```

Add a TS export test for each exported type with the same
`#[test] fn export_bindings_<snake_name>` pattern used by existing contract
tests.

- [ ] **Step 4: Run contract and schema tests**

Run:

```bash
cargo test -p studio-core --test agent_contract
cargo test -p studio-cli --test schema_output
```

Expected: both commands pass; generated bindings show keys such as `nodeId`,
`componentDefinitionId`, and `evaluationMaxNodes`.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-core/src/agent_contract.rs \
  crates/studio-core/src/lib.rs \
  crates/studio-core/tests/agent_contract.rs \
  crates/studio-core/bindings schemas/studio-types.ts
git commit -m "feat(studio-core): define agent modeling contracts"
```

---

### Task 2: Add Sparse Instance And Material Patches

**Files:**
- Create: `crates/studio-core/src/patch.rs`
- Modify: `crates/studio-core/src/lib.rs`
- Modify: `crates/studio-core/src/operation.rs`
- Modify: `crates/studio-core/src/apply.rs`
- Test: `crates/studio-core/tests/agent_patches.rs`

**Interfaces:**
- Consumes: `ComponentId`, `MaterialId`, `ParameterValue`, `Transform`.
- Produces: `InstancePatch`, `MaterialPatch`, `Operation::InstancePatch`, and
  `Operation::MaterialPatch`.

- [ ] **Step 1: Write failing patch tests**

Create a component with one Number parameter and one instance with:

```text
capacity=4, metadata { role: pump, area: A }, overrides {}
material-000001 assigned
```

Then apply this patch:

```rust
Operation::InstancePatch(InstancePatch {
    node_id: "node-000001".into(),
    name: Some("Pump 2".into()),
    transform: None,
    visible: Some(false),
    definition_id: None,
    set_parameters: [(
        "capacity".into(),
        ParameterValue::Number(6.0),
    )]
    .into_iter()
    .collect(),
    remove_parameters: Vec::new(),
    set_metadata: [("service".into(), "standby".into())]
        .into_iter()
        .collect(),
    remove_metadata_keys: vec!["area".into()],
    material_id: None,
    clear_material: true,
});
```

Assert:

- name is `Pump 2`;
- capacity is `6.0`;
- metadata contains `role` and `service`, not `area`;
- overrides, transform, and all unspecified parameters are unchanged;
- `material_id` is `None`;
- result diff updates only `node-000001`.

Add a second test applying `MaterialPatch` to `opacity` only and assert
base color, name, and edge color remain unchanged.

Add a third test where `clear_material` and `material_id` are both supplied;
expect `ICE3_STUDIO_OPERATION_INVALID`.

- [ ] **Step 2: Run the tests and observe missing variants**

Run:

```bash
cargo test -p studio-core --test agent_patches
```

Expected: compilation fails because `InstancePatch` and the new Operation variants do not exist.

- [ ] **Step 3: Implement patch payloads**

In `patch.rs` define:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct InstancePatch {
    pub node_id: NodeId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<Transform>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition_id: Option<ComponentId>,
    #[serde(default)]
    pub set_parameters: BTreeMap<String, ParameterValue>,
    #[serde(default)]
    pub remove_parameters: Vec<String>,
    #[serde(default)]
    pub set_metadata: BTreeMap<String, String>,
    #[serde(default)]
    pub remove_metadata_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material_id: Option<MaterialId>,
    #[serde(default)]
    pub clear_material: bool,
}
```

Define the analogous `MaterialPatch` with `name`, `base_color`, `opacity`,
`edge_color`, and `clear_edge_color`.

Add a `has_input()` method to each patch that returns true when at least one field,
map, list, or clear flag is present.

Add Operation variants:

```rust
#[serde(rename = "instance.patch")]
#[ts(rename = "instance.patch")]
InstancePatch(InstancePatch),

#[serde(rename = "material.patch")]
#[ts(rename = "material.patch")]
MaterialPatch(MaterialPatch),
```

- [ ] **Step 4: Implement sparse mutation semantics**

In `apply_operation`, match the new variants.

For `InstancePatch`:

1. Reject `!patch.has_input()`.
2. Reject overlap between
   `set_parameters.keys()` and `remove_parameters`.
3. Reject non-null `material_id` with `clear_material`.
4. Find a `ComponentInstance` node.
5. Apply only present fields.
6. Apply metadata set then remove.
7. Set or clear root material after the content borrow ends.
8. Record `diff.node_updated(node_id)`.

Use this exact overlap check:

```rust
if patch
    .remove_parameters
    .iter()
    .any(|name| patch.set_parameters.contains_key(name))
{
    return Err(operation_error(
        operation_index,
        &patch.node_id,
        "/removeParameters",
        "a parameter cannot be set and removed in one patch",
    ));
}
```

For `MaterialPatch`, reject empty input and conflicting edge-color fields, find by
material ID, apply only `Some` fields, and call `diff.material_updated`.

Leave parameter type/range/unknown-name validation to the existing whole-document
validation pass so the transaction remains atomic.

- [ ] **Step 5: Run focused and full core tests**

Run:

```bash
cargo test -p studio-core --test agent_patches
cargo test -p studio-core
```

Expected: all tests pass, including malformed/empty patch rejection.

- [ ] **Step 6: Commit**

```bash
git add crates/studio-core/src/patch.rs \
  crates/studio-core/src/lib.rs \
  crates/studio-core/src/operation.rs \
  crates/studio-core/src/apply.rs \
  crates/studio-core/tests/agent_patches.rs \
  crates/studio-core/bindings schemas/studio-types.ts
git commit -m "feat(studio-core): patch instances and materials sparsely"
```

---

### Task 3: Add Sparse Component And Connection Patches

**Files:**
- Modify: `crates/studio-core/src/patch.rs`
- Modify: `crates/studio-core/src/operation.rs`
- Modify: `crates/studio-core/src/apply.rs`
- Test: `crates/studio-core/tests/agent_patches.rs`

**Interfaces:**
- Consumes: Task 2 sparse-patch error and metadata rules.
- Produces: `ComponentPatch`, `ConnectionPatch`, and their Operation variants.

- [ ] **Step 1: Write failing tests**

Add tests that:

- patch a component name, one parameter default, one metadata value, and one
  metadata removal; assert parts, ports, parameter types, and other defaults are
  unchanged;
- patch a connection diameter and metadata; assert endpoints, medium, route policy,
  relation ID, and node transform are unchanged;
- patch endpoint and route policy in one operation;
- reject an empty component patch and an empty connection patch.

- [ ] **Step 2: Run the tests and observe missing variants**

Run:

```bash
cargo test -p studio-core --test agent_patches component_patch
cargo test -p studio-core --test agent_patches connection_patch
```

Expected: compilation fails because the new patch types and variants are absent.

- [ ] **Step 3: Implement patch payloads and variants**

Define:

```rust
ComponentPatch {
    component_id: ComponentId,
    name: Option<String>,
    set_parameter_defaults: BTreeMap<String, ParameterValue>,
    set_metadata: BTreeMap<String, String>,
    remove_metadata_keys: Vec<String>,
}

ConnectionPatch {
    node_id: NodeId,
    from: Option<InstancePortRef>,
    to: Option<InstancePortRef>,
    medium: Option<String>,
    diameter: Option<f32>,
    route_policy: Option<RoutePolicy>,
    set_metadata: BTreeMap<String, String>,
    remove_metadata_keys: Vec<String>,
}
```

Add Operation variants with `type` names `component.patch` and `connection.patch`.

- [ ] **Step 4: Implement mutations**

For `ComponentPatch`, update only definition name/defaults/metadata and use
`diff.component_updated`. Do not modify parts or ports.

For `ConnectionPatch`, run existing `ensure_connection_relation` after applying
endpoint changes but before considering the operation successful. Update only
connection content and node metadata, then use `diff.node_updated`.

Set/remove overlap and empty-input rules apply.

- [ ] **Step 5: Run tests**

Run:

```bash
cargo test -p studio-core --test agent_patches
cargo test -p studio-core --test large_scene_transactions
```

Expected: both pass.

- [ ] **Step 6: Commit**

```bash
git add crates/studio-core/src/patch.rs \
  crates/studio-core/src/operation.rs \
  crates/studio-core/src/apply.rs \
  crates/studio-core/tests/agent_patches.rs \
  crates/studio-core/bindings schemas/studio-types.ts
git commit -m "feat(studio-core): patch components and connections sparsely"
```

---

### Task 4: Add Request Authority And Agent Manifest

**Files:**
- Modify: `crates/studio-server/src/capabilities.rs`
- Create: `crates/studio-server/src/agent.rs`
- Modify: `crates/studio-server/src/lib.rs`
- Modify: `crates/studio-server/src/rpc.rs`
- Test: `crates/studio-server/tests/agent_api.rs`

**Interfaces:**
- Consumes: Task 1 `AgentManifest`, `AgentAccess`, `AgentLimits`, `AgentTool`,
  and `AgentWorkflow`.
- Produces: `RequestAuthority`, `classify_request_authority`,
  `agent_access`, and `{agent,manifest}`.

- [ ] **Step 1: Write failing manifest tests**

Start a review-required server and call `system.agentManifest` with:

1. no credentials;
2. the agent bearer token;
3. the approval cookie.

Assert:

```text
read-only: canRead=true, canPropose=false, canCommit=false
agent:     canRead=true, canPropose=true, canCommit=false
approval:  canRead=true, canPropose=true, canCommit=true
```

For an autonomous server, agent access has `canApplyDirectly=true`,
`canCommit=true`, and `canDiscard=true`.

Assert the manifest includes `scene.context`, `scene.search`,
`evaluation.query`, and every patch tool, with nonempty schema references.

- [ ] **Step 2: Run tests and observe unknown method**

Run:

```bash
cargo test -p studio-server --test agent_api manifest_access
```

Expected: `system.agentManifest` returns `-32601`.

- [ ] **Step 3: Add authority classification**

In `capabilities.rs` add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestAuthority {
    ReadOnly,
    AgentProposal,
    HumanApproval,
    AutonomousAgent,
}

pub fn classify_request_authority(
    mode: WriteMode,
    agent_token: Option<&str>,
    approval_cookie: Option<&str>,
    expected_agent: &str,
    expected_approval: &str,
) -> RequestAuthority {
    let agent_valid = agent_token
        .is_some_and(|token| constant_time_eq(token.as_bytes(), expected_agent.as_bytes()));
    let approval_valid = approval_cookie
        .is_some_and(|token| constant_time_eq(token.as_bytes(), expected_approval.as_bytes()));
    match mode {
        WriteMode::ReviewRequired if agent_valid => RequestAuthority::AgentProposal,
        WriteMode::ReviewRequired if approval_valid => RequestAuthority::HumanApproval,
        WriteMode::Autonomous if agent_valid => RequestAuthority::AutonomousAgent,
        _ => RequestAuthority::ReadOnly,
    }
}

pub fn agent_access(authority: RequestAuthority) -> AgentAccess {
    match authority {
        RequestAuthority::ReadOnly => AgentAccess {
            can_read: true,
            can_propose: false,
            can_commit: false,
            can_apply_directly: false,
            can_discard: false,
        },
        RequestAuthority::AgentProposal => AgentAccess {
            can_read: true,
            can_propose: true,
            can_commit: false,
            can_apply_directly: false,
            can_discard: false,
        },
        RequestAuthority::HumanApproval => AgentAccess {
            can_read: true,
            can_propose: true,
            can_commit: true,
            can_apply_directly: true,
            can_discard: true,
        },
        RequestAuthority::AutonomousAgent => AgentAccess {
            can_read: true,
            can_propose: true,
            can_commit: true,
            can_apply_directly: true,
            can_discard: true,
        },
    }
}
```

Keep `authorize_write` as the dispatch-time gate; do not replace its enforcement
with manifest data.

- [ ] **Step 4: Pass authority into dispatch**

In the RPC handler, classify authority from the same headers and session values
already used by `authorize_write`. Change the internal signature to:

```rust
fn dispatch(
    session: &mut Session,
    request: &RpcRequest,
    authority: RequestAuthority,
) -> Result<serde_json::Value, RpcError>
```

Pass the captured value through `spawn_blocking`. Update all direct internal calls
and tests to pass an appropriate authority.

- [ ] **Step 5: Implement `system.agentManifest`**

In `agent.rs`, build a constant tool list with these names:

```text
system.agentManifest
scene.context
scene.search
evaluation.query
changeset.create
changeset.addOperations
changeset.evaluate
changeset.getCandidate
changeset.commit
document.apply
```

Use schema references:

```text
/schemas/studio-api.schema.json#/$defs/SceneSearchRequest
/schemas/studio-api.schema.json#/$defs/EvaluationQueryRequest
/schemas/studio-api.schema.json#/$defs/ApplyRequest
```

Mark commit/discard/direct apply availability from `agent_access(authority)`.
Add `system.agentManifest` to method dispatch and the capabilities method list.

Add these additive fields to `Capabilities`:

```rust
pub access: AgentAccess,
pub limits: AgentLimits,
```

Populate them using the same request authority and `AgentLimits::default()`.

- [ ] **Step 6: Run tests**

Run:

```bash
cargo test -p studio-server --test agent_api manifest_access
cargo test -p studio-server --test changeset_rpc
cargo test -p studio-server --test rpc_contract
```

Expected: all pass; proposal token still cannot direct apply or commit.

- [ ] **Step 7: Commit**

```bash
git add crates/studio-server/src/capabilities.rs \
  crates/studio-server/src/agent.rs \
  crates/studio-server/src/lib.rs \
  crates/studio-server/src/rpc.rs \
  crates/studio-server/tests/agent_api.rs \
  crates/studio-server/bindings crates/studio-core/bindings \
  schemas/studio-types.ts
git commit -m "feat(studio-server): advertise agent capabilities"
```

---

### Task 5: Implement Context And Source Search

**Files:**
- Modify: `crates/studio-server/src/agent.rs`
- Modify: `crates/studio-server/src/rpc.rs`
- Test: `crates/studio-server/tests/agent_api.rs`

**Interfaces:**
- Consumes: Task 1 context/search contracts, existing `evaluate_session_scene`, and
  `scene_snapshot`.
- Produces: `{agent,context}`, `{agent,source_search}`, and
  `{agent,normalize_query}`.

- [ ] **Step 1: Write failing context and search tests**

Use the existing component-instance fixture and assert:

- context has matching source/derived/component/instance counts, Y-up metadata,
  access, limits, and source content facets;
- source search finds the instance by `query: "Pump"` and by metadata;
- `limit: 1` returns one item and a cursor;
- the second page returns the remaining item and `nextCursor: null`;
- changing the query while reusing a cursor returns
  `ICE3_STUDIO_REVISION_CONFLICT`;
- committing a revision before requesting page two returns
  `ICE3_STUDIO_REVISION_CONFLICT`;
- a projection omitting `metadata` does not serialize `metadata`;
- unknown projection names return `ICE3_STUDIO_API_INVALID`.

- [ ] **Step 2: Run tests and observe unknown methods**

Run:

```bash
cargo test -p studio-server --test agent_api context
cargo test -p studio-server --test agent_api source_search
```

Expected: `scene.context` and `scene.search` return `-32601`.

- [ ] **Step 3: Implement deterministic query fingerprint**

In `agent.rs` add:

```rust
fn normalize_query(request: &SceneSearchRequest) -> Result<String, RpcError> {
    let mut fields = request
        .fields
        .clone()
        .unwrap_or_else(|| default_search_fields());
    fields.sort_by_key(|field| format!("{field:?}"));
    fields.dedup();
    let bytes = serde_json::to_vec(&(
        &request.scope,
        &request.query,
        &request.node_id,
        &request.owner_id,
        &request.parent_id,
        &request.metadata,
        &request.content_kind,
        &request.component_definition_id,
        &request.material_id,
        request.visible,
        &request.relation_type,
        &request.related_node_id,
        request.bounds,
        fields,
    ))
    .map_err(serialize_error)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
```

Use cursor format:

```text
v1.<revision>.<offset>.<query-digest>
```

Validate exactly four dot-separated parts. The format contains only URL-safe
characters.

- [ ] **Step 4: Implement `scene.context`**

Call `ensure_scene_snapshot` and compute source bounds. Build:

- source content-kind counts in enum order;
- relation-type counts sorted by `(count descending, type ascending)` and capped
  at 50;
- derived count and world bounds from the cached evaluated scene;
- `AgentAccess` from request authority and `AgentLimits::default()`.

Return errors through the existing `large_scene_error` mapping. Do not clone or
serialize complete node/material/component collections.

- [ ] **Step 5: Implement source `scene.search`**

Apply filters in this order:

```text
node id -> owner (invalid for source) -> parent -> metadata -> query
-> content kind -> component definition -> material -> visibility
-> relation filters -> bounds
```

Sort by source node ID and slice `[offset..offset + limit]`.

Build `SceneSearchItem` with source world bounds and content kind. For source,
`ownerId` is null and `geometryId` is null unless the source content is a custom
mesh or primitive represented by a cached mesh.

When `fields` is present, require `id`, `scope`, and `kind`; serialize a projected
item with omitted optional fields absent rather than zero/false placeholders.

- [ ] **Step 6: Wire RPC dispatch and test**

Add both method names to capabilities and dispatch:

```rust
"scene.context" => crate::agent::context(session, authority),
"scene.search" => crate::agent::search(session, authority, params),
```

Run:

```bash
cargo test -p studio-server --test agent_api context
cargo test -p studio-server --test agent_api source_search
```

Expected: all focused tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/studio-server/src/agent.rs \
  crates/studio-server/src/rpc.rs \
  crates/studio-server/tests/agent_api.rs \
  crates/studio-server/bindings crates/studio-core/bindings \
  schemas/studio-types.ts
git commit -m "feat(studio-server): expose bounded agent scene context"
```

---

### Task 6: Add Derived Search And Evaluated Geometry Query

**Files:**
- Modify: `crates/studio-server/src/agent.rs`
- Modify: `crates/studio-server/src/rpc.rs`
- Test: `crates/studio-server/tests/agent_api.rs`

**Interfaces:**
- Consumes: Task 5 cursor/search engine and existing evaluated scene snapshot.
- Produces: derived-scope handling in `scene.search` and
  `evaluation.query`.

- [ ] **Step 1: Write failing derived tests**

Create one component instance and one instance set. Assert:

- derived search finds `node-000001/body` and
  `node-000010/000000/body`;
- derived items contain deterministic `ownerId` and world `bounds`;
- derived search omits positions and indices;
- `evaluation.query` returns the same order as input `nodeIds`;
- default include returns neither positions nor indices;
- include positions returns positions but no indices;
- include positions and indices returns both;
- 33 unique node IDs are rejected;
- duplicate node IDs are rejected;
- one unknown node ID rejects the whole request.

- [ ] **Step 2: Run tests and observe missing behavior**

Run:

```bash
cargo test -p studio-server --test agent_api derived_search
cargo test -p studio-server --test agent_api evaluation_query
```

Expected: derived scope returns no items or unresolved nodes, and
`evaluation.query` returns `-32601`.

- [ ] **Step 3: Implement derived search**

Ensure the evaluated scene once per request. Build derived items from
`EvaluatedSceneNode`.

Determine content kind:

```rust
fn derived_kind(node: &EvaluatedSceneNode) -> SceneContentKind {
    if node
        .mesh_key
        .as_deref()
        .is_some_and(|key| key.starts_with("connection-"))
    {
        SceneContentKind::Connection
    } else if node.mesh_key.is_some() {
        SceneContentKind::Mesh
    } else {
        SceneContentKind::Group
    }
}
```

Map a derived node to its owning source node by choosing the longest source node ID
that equals `ownerId` or is a path prefix of the derived ID. Use that source for
`componentDefinitionId` and source material fallback.

Reject relation filters when `scope=derived`.

For `scope=both`, build source results then derived results and sort by
`(scope, id)` before applying the cursor offset and limit.

- [ ] **Step 4: Implement `evaluation.query`**

Reject duplicate IDs:

```rust
let unique = request.node_ids.iter().collect::<BTreeSet<_>>();
if unique.len() != request.node_ids.len() {
    return Err(agent_operation_error(
        "/nodeIds",
        "evaluation node IDs must be unique",
        None,
    ));
}
```

Reject lengths outside `1..=MAX_EVALUATION_NODES`.

Use the evaluated scene node map. For a node with a mesh:

- return geometry ID, vertex/index counts, world bounds, and digests;
- include cloned positions/indices only for explicit include values;
- do not include a field when not requested.

For a node without a mesh, return zero counts, null geometry/digest/bounds, and
null payload fields.

- [ ] **Step 5: Wire dispatch and run tests**

Add `evaluation.query` to manifest and capabilities, then dispatch it.

Run:

```bash
cargo test -p studio-server --test agent_api
cargo test -p studio-server --test large_scene_rpc
```

Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add crates/studio-server/src/agent.rs \
  crates/studio-server/src/rpc.rs \
  crates/studio-server/tests/agent_api.rs \
  crates/studio-server/bindings crates/studio-core/bindings \
  schemas/studio-types.ts
git commit -m "feat(studio-server): query derived agent scene data"
```

---

### Task 7: Add Agent Workflow And Large-Scene Evidence

**Files:**
- Create: `crates/studio-server/tests/agent_workflow.rs`
- Modify: `scripts/run-ai-fixture.mjs`
- Modify: `benchmarks/large-industrial-scene.mjs`
- Modify: `README.md`
- Modify: `docs/architecture.md`
- Modify: `docs/reference/api.md`
- Modify: `docs/reference/diagnostics.md`
- Modify: `docs/README.md`
- Test: `scripts/tests/gpu-scene-report.test.mjs` if benchmark semantics move into an exported helper

**Interfaces:**
- Consumes: all previous tasks.
- Produces: a deterministic proposal/approval workflow and machine-readable
  context/search byte evidence.

- [ ] **Step 1: Write the failing Agent workflow test**

Use a review-required `TestServer`. As the agent token:

1. call `system.agentManifest`;
2. call `scene.context`;
3. call `scene.search` with `query: "Pump"`;
4. call `evaluation.query` for one derived part;
5. create a ChangeSet;
6. add an `instance.patch`;
7. evaluate and assert updated node IDs;
8. attempt commit with the agent token and assert HTTP 403;
9. commit with the approval cookie and approved digest;
10. reopen the project and assert the patched metadata/value survived.

Do not require a Toolchain for the core workflow. If capabilities report Toolchain
ready, also request candidate preview and assert a packet or GLB URL.

- [ ] **Step 2: Run the workflow and observe failure**

Run:

```bash
cargo test -p studio-server --test agent_workflow
```

Expected: failure until the fixture operations and generated fields are complete.

- [ ] **Step 3: Implement the workflow fixture**

Use the project's `apply_instance_and_set` pattern or a focused local fixture.
The patch must change only:

```json
{
  "type": "instance.patch",
  "node_id": "node-000001",
  "set_parameters": {
    "capacity": { "kind": "number", "value": 6.0 }
  },
  "set_metadata": { "agentReviewed": "true" },
  "clearMaterial": false
}
```

Assert unchanged parts, overrides, and transform after commit.

- [ ] **Step 4: Extend the deterministic AI fixture script**

Before its existing first write, call:

```js
const manifest = await rpc(101, 'system.agentManifest', {});
const context = await rpc(102, 'scene.context', {});
const search = await rpc(103, 'scene.search', { limit: 10 });
```

Assert the new methods succeeded, context revision is 0 before writes, and search
returns an items array. Include these evidence fields in the final JSON output:

```json
{
  "agentManifestTools": 10,
  "initialSourceNodes": 0,
  "initialSearchItems": 0
}
```

Use the actual returned lengths rather than hardcoding expected tools.

- [ ] **Step 5: Add large-scene context/search gates**

In `benchmarks/large-industrial-scene.mjs`, after `coldStats`, call:

```js
const context = await rpc('scene.context');
const contextBytes = Buffer.byteLength(JSON.stringify(context));
const firstPage = await rpc('scene.search', { scope: 'both', limit: 25 });
const firstPageBytes = Buffer.byteLength(JSON.stringify(firstPage));
const secondPage = firstPage.nextCursor
  ? await rpc('scene.search', { scope: 'both', limit: 25, cursor: firstPage.nextCursor })
  : null;
```

Add to the result:

```js
agentContext: {
  contextBytes,
  searchPageBytes: firstPageBytes,
  firstPageItems: firstPage.items.length,
  secondPageItems: secondPage?.items.length ?? 0,
}
```

Fail the benchmark when:

```js
if (contextBytes > 16 * 1024) failures.push('agentContextBytes');
if (firstPageBytes > 128 * 1024) failures.push('agentSearchPageBytes');
if (firstPage.items.length > 25) failures.push('agentSearchLimit');
```

The three-way scene comparison must preserve the new Agent evidence in every mode.

- [ ] **Step 6: Update user-facing documentation**

Update:

- `README.md`: Agent inspect/propose/review workflow and measured context bounds.
- `docs/architecture.md`: read facade and one authoritative mutation path.
- `docs/reference/api.md`: methods, request/response fields, cursor recovery,
  patch semantics, limits.
- `docs/reference/diagnostics.md`: Agent error categories and recovery examples.
- `docs/README.md`: change SPEC-009 to Implemented only after all gates pass.

Do not mark the feature Implemented while any new benchmark or permission test is
disabled or returning untruthful zero samples.

- [ ] **Step 7: Run focused verification**

Run:

```bash
cargo test -p studio-core --test agent_contract
cargo test -p studio-core --test agent_patches
cargo test -p studio-server --test agent_api
cargo test -p studio-server --test agent_workflow
PATH=/Users/felix/.nvm/versions/node/v24.22.1/bin:$PATH npm run benchmark:gpu-scene
```

Expected: all tests pass; benchmark failures are empty and Agent byte evidence is
within limits.

- [ ] **Step 8: Run the complete verification gate**

Run:

```bash
PATH=/Users/felix/.nvm/versions/node/v24.22.1/bin:$PATH npm run verify
```

Expected: all 14 gates pass after schema and binding regeneration.

- [ ] **Step 9: Commit**

```bash
git add crates/studio-server/tests/agent_workflow.rs \
  scripts/run-ai-fixture.mjs benchmarks/large-industrial-scene.mjs \
  README.md docs/architecture.md docs/reference/api.md \
  docs/reference/diagnostics.md docs/README.md
git commit -m "test(studio): verify agent modeling workflow"
```

## Plan Self-Review

- Spec coverage: Tasks 1-6 cover contracts, four patches, manifest, access,
  context, source/derived search, cursors, and explicit geometry query. Task 7
  covers Agent workflow, byte ceilings, documentation, and full verification.
- Placeholder scan: no unfinished implementation placeholders remain.
- Type consistency: `SceneSearchRequest`, `SceneSearchResult`,
  `EvaluationQueryRequest`, `InstancePatch`, `ComponentPatch`,
  `ConnectionPatch`, and `MaterialPatch` are introduced in Tasks 1-3 and consumed
  unchanged in Tasks 4-7.
- Review Focus: stale/mismatched cursors are tested in Task 5; omitted-field
  retention and null/clear semantics in Task 2; authority mismatch in Task 4;
  payload and node limits in Task 6; large-scene byte ceilings in Task 7.
