# ICE3 Studio AI-Native Modeling MVP Implementation Plan

> Document ID: ICE3-STUDIO-PLAN-001
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver a local-first, Rust-backed, API-first modeling system that can construct, validate, revise, persist, export, and display a small wastewater treatment module without any external modeling application.

**Architecture:** A dependency-directed Rust workspace separates deterministic domain modeling, local project persistence, JSON-RPC serving, and CLI orchestration. A thin TypeScript viewer consumes preview GLBs and API events. An external deterministic AI test client exercises the same public API available to any future agent.

**Tech Stack:** Rust 1.98.0, edition 2024, JSON Schema, JSON-RPC 2.0, HTTP/WebSocket, glTF/GLB, TypeScript 7.0.2, Vite, Node.js 24.22.1 for viewer build tooling only, and the existing ICE3 Engine and Toolchain packages.

**Spec:** [`ICE3-STUDIO-SPEC-001`](../specs/2026-10-30-ai-native-studio-mvp-design.md)

## Global Constraints

- Studio Core is Rust. Node.js must not own document mutation, geometry evaluation, validation, persistence, or export.
- The public API is the product boundary; the Web Viewer uses the same API and has no private mutation path.
- External AI owns natural-language understanding; Studio contains no model credentials, model SDK, or agent policy.
- Document plus append-only operation history is the source of truth; evaluated Geometry is derived and cacheable.
- Public units are meters and radians; coordinates are right-handed Y-up.
- Every write is an atomic transaction with `expectedRevision`, `idempotencyKey`, and optional `dryRun`.
- Stable IDs and deterministic ordering are mandatory.
- Local server binds loopback only and requires a short-lived session token for writes.
- Project paths must remain inside the configured workspace root.
- Studio exports standard GLB and leaves Runtime LOD and spatial partitioning to the existing Toolchain.
- The only MVP acceptance fixture is `wastewater-mini`.
- Source, tests, documentation, diagnostics, links, and commits must not contain competitor, vendor, or unapproved commercial product names.
- Every behavior change starts with a failing test and ends with the complete relevant suite passing.
- Git author and committer must be `damoqiongqiu <damoqiongqiu@126.com>`.

## Review Focus

- Invalid geometry: non-finite coordinates, reversed or out-of-range indices, zero dimensions, and singular transforms must fail before persistence.
- Revision conflict and duplicate submission: stale revisions must leave the project unchanged; the same idempotency key must not create multiple revisions.
- Partial write: an operation failure or disk failure in the middle of a batch must leave the previous revision loadable.
- Path and target injection: project IDs, resource references, and export requests must not escape the configured workspace.
- Preview failure: Toolchain failure must retain the last valid viewer asset and expose a structured diagnostic.

---

### Task 1: Bootstrap the Rust Workspace

**Files:**
- Create: `Cargo.toml`
- Create: `Cargo.lock`
- Create: `rustfmt.toml`
- Create: `.gitignore`
- Create: `crates/studio-core/Cargo.toml`
- Create: `crates/studio-core/src/lib.rs`
- Create: `crates/studio-core/tests/workspace_contract.rs`
- Modify: `README.md`

**Interfaces:**
- Consumes: the approved `ICE3-STUDIO-SPEC-001`.
- Produces: `studio_core::PROJECT_SCHEMA_VERSION: u32` and `studio_core::API_VERSION: &str`.
- Produces: `cargo test --workspace`, `cargo fmt --check`, and `cargo clippy --workspace --all-targets -- -D warnings`.

- [ ] **Step 1: Write the failing workspace contract test**

```rust
// crates/studio-core/tests/workspace_contract.rs
use studio_core::{API_VERSION, PROJECT_SCHEMA_VERSION};

#[test]
fn exposes_the_initial_contract_versions() {
    assert_eq!(PROJECT_SCHEMA_VERSION, 1);
    assert_eq!(API_VERSION, "1.0");
}
```

- [ ] **Step 2: Run the test and verify RED**

Run: `cargo test -p studio-core --test workspace_contract`

Expected: compilation fails because the package and exported constants do not exist.

- [ ] **Step 3: Create the workspace and minimal core crate**

```toml
# Cargo.toml
[workspace]
members = [
  "crates/studio-core",
  "crates/studio-project",
  "crates/studio-server",
  "crates/studio-cli",
]
resolver = "3"

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "MIT"
rust-version = "1.98"

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
sha2 = "0.10"
schemars = "1"
ts-rs = "11"
glam = { version = "0.30", features = ["serde"] }
axum = { version = "0.8", features = ["ws"] }
tokio = { version = "1", features = ["fs", "macros", "net", "rt-multi-thread", "sync", "time"] }
tempfile = "3"
reqwest = { version = "0.12", features = ["json"] }
clap = { version = "4", features = ["derive"] }
rand = "0.9"
```

```rust
// crates/studio-core/src/lib.rs
pub const PROJECT_SCHEMA_VERSION: u32 = 1;
pub const API_VERSION: &str = "1.0";
```

Create empty `lib.rs`, `main.rs`, and `Cargo.toml` skeletons for the other three crates.

```gitignore
/target/
/apps/viewer/node_modules/
/apps/viewer/dist/
/.studio/
*.tmp
```

- [ ] **Step 4: Run all foundation checks**

Run: `cargo fmt --check`

Run: `cargo test --workspace`

Run: `cargo clippy --workspace --all-targets -- -D warnings`

Expected: all commands pass.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock rustfmt.toml .gitignore crates README.md
git commit -m "build(studio): bootstrap rust workspace"
```

---

### Task 2: Define Stable IDs, Units, and the Document Model

**Files:**
- Create: `crates/studio-core/src/ids.rs`
- Create: `crates/studio-core/src/math.rs`
- Create: `crates/studio-core/src/document.rs`
- Create: `crates/studio-core/tests/document_model.rs`
- Modify: `crates/studio-core/src/lib.rs`

**Interfaces:**
- Consumes: `PROJECT_SCHEMA_VERSION` and `API_VERSION`.
- Produces: `NodeId`, `MaterialId`, `RelationId`, `ConstraintId`, `Revision`.
- Produces: `Transform`, `Vec2`, `Vec3`, `Document`, `SceneNode`, `NodeContent`, `Material`, `Relation`, `Constraint`.
- Produces: `NodeId::new(prefix: &str, ordinal: u64) -> Result<NodeId, IdError>`.

- [ ] **Step 1: Write failing document model tests**

```rust
use studio_core::document::{Document, Material, NodeContent, SceneNode};
use studio_core::ids::{NodeId, Revision};
use studio_core::math::Transform;

#[test]
fn creates_deterministic_ids_and_default_document() {
    assert_eq!(NodeId::new("node", 7).unwrap().as_str(), "node-000007");
    assert_eq!(Revision::new(0).as_u64(), 0);

    let document = Document::new("wwtp-mini");
    assert_eq!(document.revision, Revision::new(0));
    assert_eq!(document.project_id, "wwtp-mini");
    assert_eq!(document.schema_version, 1);
    assert!(document.scene.nodes().is_empty());
}

#[test]
fn rejects_invalid_ids() {
    assert!(NodeId::new("", 1).is_err());
    assert!(NodeId::new("node space", 1).is_err());
}

#[test]
fn stores_identity_independently_from_display_name() {
    let id = NodeId::new("node", 1).unwrap();
    let node = SceneNode::group(id.clone(), "Ground");
    assert_eq!(node.id, id);
    assert_eq!(node.name, "Ground");
    assert_eq!(node.transform, Transform::IDENTITY);
    assert!(matches!(node.content, NodeContent::Group));
}
```

- [ ] **Step 2: Run the tests and verify RED**

Run: `cargo test -p studio-core --test document_model`

Expected: compilation fails with missing `document`, `ids`, and `math` modules.

- [ ] **Step 3: Implement the model with strict construction**

```rust
// crates/studio-core/src/ids.rs
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(String);

impl NodeId {
    pub fn new(prefix: &str, ordinal: u64) -> Result<Self, IdError> {
        if prefix.is_empty()
            || !prefix.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'
            })
        {
            return Err(IdError::InvalidPrefix);
        }
        Ok(Self(format!("{prefix}-{ordinal:06}")))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
```

Implement `MaterialId`, `RelationId`, `ConstraintId`, and `Revision` with the same
validated-prefix pattern. Implement `Document::new`, ordered vectors for scene nodes,
materials, relations, and constraints, and deterministic lookup methods.

```rust
// crates/studio-core/src/math.rs
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    pub translation: Vec3,
    pub rotation: [f32; 4],
    pub scale: Vec3,
}

impl Transform {
    pub const IDENTITY: Self = Self {
        translation: Vec3 { x: 0.0, y: 0.0, z: 0.0 },
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: Vec3 { x: 1.0, y: 1.0, z: 1.0 },
    };
}
```

Use `f64` only for cumulative validation calculations where needed; public Scene values
remain finite `f32` meters and radians.

- [ ] **Step 4: Run model tests and the full crate suite**

Run: `cargo test -p studio-core`

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-core
git commit -m "feat(studio-core): define stable document model"
```

---

### Task 3: Implement Primitive Geometry and Sweep Generation

**Files:**
- Create: `crates/studio-core/src/geometry/mod.rs`
- Create: `crates/studio-core/src/geometry/primitives.rs`
- Create: `crates/studio-core/src/geometry/sweep.rs`
- Create: `crates/studio-core/src/geometry/evaluate.rs`
- Create: `crates/studio-core/tests/geometry_evaluation.rs`
- Modify: `crates/studio-core/src/lib.rs`

**Interfaces:**
- Consumes: `NodeContent`, `Transform`, and ID types from Task 2.
- Produces: `Primitive`, `TriangleMesh`, `Sweep`, `EvaluatedMesh`, `GeometryError`.
- Produces: `evaluate_node_content(content: &NodeContent) -> Result<EvaluatedMesh, GeometryError>`.
- Produces: `vertices: Vec<Vertex>`, `indices: Vec<u32>`, and `bounds: Bounds` in every evaluated mesh.

- [ ] **Step 1: Write failing geometry tests**

```rust
use studio_core::geometry::{Primitive, evaluate_node_content};
use studio_core::document::NodeContent;

#[test]
fn evaluates_a_box_with_outward_triangles() {
    let content = NodeContent::Primitive(Primitive::Box {
        width: 2.0,
        height: 1.0,
        depth: 4.0,
    });
    let mesh = evaluate_node_content(&content).unwrap();
    assert_eq!(mesh.vertices.len(), 24);
    assert_eq!(mesh.indices.len(), 36);
    assert_eq!(mesh.bounds.min.x, -1.0);
    assert_eq!(mesh.bounds.max.z, 2.0);
}

#[test]
fn sweeps_a_polyline_into_connected_rings() {
    let content = NodeContent::Sweep {
        points: vec![
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [2.0, 0.0, 3.0],
        ],
        radius: 0.25,
        radial_segments: 8,
    };
    let mesh = evaluate_node_content(&content).unwrap();
    assert_eq!(mesh.vertices.len(), 24);
    assert!(mesh.indices.len() > 60);
}

#[test]
fn rejects_zero_sized_and_non_finite_primitives() {
    assert!(evaluate_node_content(&NodeContent::Primitive(Primitive::Cylinder {
        radius: 0.0,
        height: 2.0,
        segments: 16,
    })).is_err());
    assert!(evaluate_node_content(&NodeContent::Primitive(Primitive::Box {
        width: f32::NAN,
        height: 1.0,
        depth: 1.0,
    })).is_err());
}
```

- [ ] **Step 2: Run the tests and verify RED**

Run: `cargo test -p studio-core --test geometry_evaluation`

Expected: compilation fails because geometry types and evaluator do not exist.

- [ ] **Step 3: Implement primitive and sweep evaluation**

```rust
// crates/studio-core/src/geometry/mod.rs
#[derive(Debug, Clone)]
pub enum Primitive {
    Box { width: f32, height: f32, depth: f32 },
    Plane { width: f32, depth: f32 },
    Cylinder { radius: f32, height: f32, segments: u32 },
    Sphere { radius: f32, segments: u32, rings: u32 },
}

#[derive(Debug, Clone)]
pub struct TriangleMesh {
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

pub fn evaluate_node_content(content: &NodeContent) -> Result<EvaluatedMesh, GeometryError> {
    match content {
        NodeContent::Group => Ok(EvaluatedMesh::empty()),
        NodeContent::Primitive(primitive) => evaluate_primitive(primitive),
        NodeContent::CustomMesh(mesh) => validate_and_clone_mesh(mesh),
        NodeContent::Sweep { points, radius, radial_segments } => {
            evaluate_sweep(points, *radius, *radial_segments)
        }
    }
}
```

Use deterministic ring orientation and stable triangle winding. Generate normals from
triangle winding. Reject fewer than two Sweep points, non-positive radii, fewer than
three radial segments, repeated consecutive points, and non-finite input.

- [ ] **Step 4: Run geometry, model, and workspace tests**

Run: `cargo test -p studio-core`

Run: `cargo test --workspace`

Expected: both commands pass.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-core
git commit -m "feat(studio-core): evaluate modeling primitives"
```

---

### Task 4: Add Atomic Document Operations and Validation

**Files:**
- Create: `crates/studio-core/src/operation.rs`
- Create: `crates/studio-core/src/apply.rs`
- Create: `crates/studio-core/src/validation.rs`
- Create: `crates/studio-core/src/diff.rs`
- Create: `crates/studio-core/tests/transactions.rs`
- Modify: `crates/studio-core/src/lib.rs`
- Modify: `crates/studio-core/src/document.rs`

**Interfaces:**
- Consumes: `Document`, IDs, primitives, materials, relations, and constraints from Tasks 2–3.
- Produces: `Operation` enum with the spec's operation names.
- Produces: `ApplyRequest`, `ApplyResult`, `Diagnostic`, `DiagnosticCode`, and `DocumentDiff`.
- Produces: `apply_transaction(document: &mut Document, request: ApplyRequest) -> Result<ApplyResult, TransactionError>`.

- [ ] **Step 1: Write failing transaction tests**

```rust
use studio_core::apply::{ApplyRequest, apply_transaction};
use studio_core::document::Document;
use studio_core::ids::Revision;
use studio_core::operation::Operation;

#[test]
fn applies_a_batch_and_increments_revision_once() {
    let mut document = Document::new("demo");
    let result = apply_transaction(&mut document, ApplyRequest {
        expected_revision: Revision::new(0),
        dry_run: false,
        idempotency_key: "batch-1".into(),
        operations: vec![
            Operation::MaterialUpsert {
                material_id: "mat-000001".into(),
                name: "Concrete".into(),
                base_color: [0.4, 0.39, 0.36, 1.0],
                opacity: 1.0,
                edge_color: None,
            },
            Operation::NodeCreate {
                node_id: "node-000001".into(),
                parent_id: None,
                name: "Ground".into(),
                transform: Default::default(),
                content: Default::default(),
                material_id: Some("mat-000001".into()),
            },
        ],
    }).unwrap();

    assert_eq!(result.previous_revision, Revision::new(0));
    assert_eq!(result.revision, Revision::new(1));
    assert_eq!(document.scene.nodes().len(), 1);
}

#[test]
fn dry_run_reports_changes_without_mutating_the_document() {
    let mut document = Document::new("demo");
    let before = document.clone();
    let result = apply_transaction(&mut document, ApplyRequest {
        expected_revision: Revision::new(0),
        dry_run: true,
        idempotency_key: "preview-1".into(),
        operations: vec![Operation::NodeCreate {
            node_id: "node-000001".into(),
            parent_id: None,
            name: "Ground".into(),
            transform: Default::default(),
            content: Default::default(),
            material_id: None,
        }],
    }).unwrap();
    assert_eq!(result.revision, Revision::new(0));
    assert_eq!(document, before);
}

#[test]
fn rejects_stale_revision_without_partial_changes() {
    let mut document = Document::new("demo");
    let before = document.clone();
    assert!(apply_transaction(&mut document, ApplyRequest {
        expected_revision: Revision::new(2),
        dry_run: false,
        idempotency_key: "conflict".into(),
        operations: vec![],
    }).is_err());
    assert_eq!(document, before);
}

#[test]
fn returns_the_original_result_for_a_repeated_idempotency_key() {
    let mut document = Document::new("demo");
    let request = ApplyRequest {
        expected_revision: Revision::new(0),
        dry_run: false,
        idempotency_key: "same-batch".into(),
        operations: vec![Operation::NodeCreate {
            node_id: "node-000001".into(),
            parent_id: None,
            name: "Ground".into(),
            transform: Default::default(),
            content: Default::default(),
            material_id: None,
        }],
    };
    let first = apply_transaction(&mut document, request.clone()).unwrap();
    let second = apply_transaction(&mut document, request).unwrap();
    assert_eq!(second.revision, first.revision);
    assert_eq!(second.changed_node_ids, first.changed_node_ids);
    assert_eq!(document.revision, Revision::new(1));
}
```

- [ ] **Step 2: Run the tests and verify RED**

Run: `cargo test -p studio-core --test transactions`

Expected: compilation fails because apply and operation types do not exist.

- [ ] **Step 3: Implement transactional apply**

```rust
pub fn apply_transaction(
    document: &mut Document,
    request: ApplyRequest,
) -> Result<ApplyResult, TransactionError> {
    if let Some(previous) = document.idempotency_record(&request.idempotency_key) {
        if previous.request_digest != request.semantic_digest()? {
            return Err(TransactionError::IdempotencyKeyReused);
        }
        return Ok(previous.result.clone());
    }
    if request.expected_revision != document.revision {
        return Err(TransactionError::RevisionConflict {
            expected: request.expected_revision,
            actual: document.revision,
        });
    }
    if request.operations.is_empty() {
        return Err(TransactionError::EmptyTransaction);
    }

    let mut candidate = document.clone();
    let mut diff = DocumentDiff::default();
    for operation in request.operations {
        apply_operation(&mut candidate, operation, &mut diff)?;
    }
    validate_document(&candidate)?;

    if request.dry_run {
        return Ok(ApplyResult::dry_run(document.revision, diff));
    }

    let previous_revision = document.revision;
    candidate.revision = previous_revision.next();
    let result = ApplyResult::committed(previous_revision, candidate.revision, diff);
    candidate.idempotency_keys.push(
        request.idempotency_record(candidate.revision, &result),
    );
    *document = candidate;
    Ok(result)
}
```

Use an ordered `IdempotencyRecord` map backed by a vector in serialized form so output is
deterministic. Reapplying an existing key returns the original result and does not mutate.
Implement every operation listed in the spec in its own match arm.

- [ ] **Step 4: Implement validation and stable diagnostics**

Validation rejects duplicate IDs, missing parents/materials, hierarchy cycles, missing
Relation endpoints, non-finite transforms, non-positive Primitive dimensions, invalid
indices, and violated `no_overlap` constraints. Diagnostics carry stable code, operation
index, object ID, JSON Pointer, and message.

Run: `cargo test -p studio-core --test transactions`

Run: `cargo test -p studio-core`

Expected: all tests pass and failed transactions leave `document` unchanged.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-core
git commit -m "feat(studio-core): apply atomic modeling transactions"
```

---

### Task 5: Persist Projects, Revisions, and History

**Files:**
- Create: `crates/studio-project/Cargo.toml`
- Create: `crates/studio-project/src/lib.rs`
- Create: `crates/studio-project/src/path.rs`
- Create: `crates/studio-project/src/store.rs`
- Create: `crates/studio-project/src/lock.rs`
- Create: `crates/studio-project/tests/project_store.rs`

**Interfaces:**
- Consumes: serializable `Document`, `Revision`, and `ApplyResult`.
- Produces: `ProjectStore::create(path)`, `open`, `write_snapshot`, `load_current`,
  `undo`, `redo`, and `history`.
- Produces: `WorkspacePath` and `ProjectError`.

- [ ] **Step 1: Write failing persistence tests**

```rust
use studio_core::document::Document;
use studio_project::{ProjectStore, WorkspacePath};

#[test]
fn creates_and_reopens_deterministic_project_state() {
    let root = tempfile::tempdir().unwrap();
    let path = WorkspacePath::new(root.path(), "wwtp-mini").unwrap();
    let store = ProjectStore::create(path.clone()).unwrap();
    store.write_snapshot(&Document::new("wwtp-mini")).unwrap();
    drop(store);

    let reopened = ProjectStore::open(path).unwrap();
    let document = reopened.load_current().unwrap();
    assert_eq!(document.project_id, "wwtp-mini");
    assert_eq!(document.schema_version, 1);
}

#[test]
fn rejects_project_paths_outside_the_workspace() {
    let root = tempfile::tempdir().unwrap();
    assert!(WorkspacePath::new(root.path(), "../escape").is_err());
}

#[test]
fn failed_second_write_keeps_the_previous_revision() {
    let root = tempfile::tempdir().unwrap();
    let path = WorkspacePath::new(root.path(), "safe").unwrap();
    let store = ProjectStore::create(path.clone()).unwrap();
    store.write_snapshot(&Document::new("safe")).unwrap();
    let before = store.load_current().unwrap();
    std::fs::create_dir(
        root.path().join("safe/revisions/00000001.json"),
    ).unwrap();

    let mut next = before.clone();
    next.revision = next.revision.next();
    assert!(store.write_snapshot(&next).is_err());
    assert_eq!(store.load_current().unwrap(), before);
}

#[test]
fn undo_and_redo_restore_document_content() {
    let root = tempfile::tempdir().unwrap();
    let path = WorkspacePath::new(root.path(), "history").unwrap();
    let store = ProjectStore::create(path).unwrap();
    let mut first = Document::new("history");
    store.write_snapshot(&first).unwrap();
    first.revision = first.revision.next();
    first.metadata.insert("phase".into(), "built".into());
    store.write_snapshot(&first).unwrap();

    let undone = store.undo().unwrap();
    assert_eq!(undone.metadata.get("phase"), None);
    let redone = store.redo().unwrap();
    assert_eq!(redone.metadata.get("phase").map(String::as_str), Some("built"));
}
```

- [ ] **Step 2: Run tests and verify RED**

Run: `cargo test -p studio-project --test project_store`

Expected: compilation fails because the persistence crate does not exist.

- [ ] **Step 3: Implement the project store**

Create `project.json`, `revisions/NNNNNNNN.json`, `history/operations.jsonl`, `resources/`,
and `cache/preview/`. Serialize canonical JSON and write via a sibling temporary file plus
rename. Compute a SHA-256 digest for every snapshot. Open validates schema, current revision,
snapshot digest, and operation history continuity.

Undo restores the preceding snapshot as the next revision and records an `undo` history
entry; redo restores the following snapshot the same way. A new write after undo truncates
the redo branch. Object IDs and content remain stable across undo and redo.

- [ ] **Step 4: Implement locking and failure restoration**

Create an exclusive lock file that records process ID and acquisition time. Reject a second
writer. On a commit failure, remove only that attempt's temporary file and preserve the
previous current revision. Do not accept absolute resource paths.

Run: `cargo test -p studio-project`

Expected: all tests pass, including escape rejection and failed-write recovery.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-project Cargo.toml Cargo.lock
git commit -m "feat(studio-project): persist revisions atomically"
```

---

### Task 6: Expose the Local JSON-RPC API and Generated Schemas

**Files:**
- Create: `crates/studio-server/Cargo.toml`
- Create: `crates/studio-server/src/lib.rs`
- Create: `crates/studio-server/src/main.rs`
- Create: `crates/studio-server/src/rpc.rs`
- Create: `crates/studio-server/src/auth.rs`
- Create: `crates/studio-server/src/events.rs`
- Create: `crates/studio-server/tests/rpc_contract.rs`
- Create: `crates/studio-cli/src/main.rs`
- Create: `crates/studio-cli/src/schema.rs`
- Create: `crates/studio-cli/tests/schema_output.rs`
- Create: `schemas/studio-api.schema.json`
- Create: `schemas/studio-types.ts`
- Modify: `crates/studio-core/src/lib.rs`

**Interfaces:**
- Consumes: project store and transaction APIs from Tasks 4–5.
- Produces: `POST /rpc`, `GET /health`, `GET /ws?token=...`, and `GET /schemas/{name}`.
- Produces: `RpcRequest`, `RpcResponse`, `Capabilities`, and ordered `RevisionEvent`.
- Produces: static schemas generated from Rust types.

- [ ] **Step 1: Write failing API contract tests**

```rust
use studio_server::{ServerConfig, start_test_server};

#[tokio::test]
async fn health_reports_api_and_schema_versions() {
    let server = start_test_server(ServerConfig::for_tests("wwtp-mini")).await;
    let body: serde_json::Value = reqwest::get(server.health_url())
        .await.unwrap()
        .json().await.unwrap();
    assert_eq!(body["status"], "ok");
    assert_eq!(body["apiVersion"], "1.0");
    assert_eq!(body["schemaVersion"], 1);
}

#[tokio::test]
async fn write_requires_the_local_session_token() {
    let server = start_test_server(ServerConfig::for_tests("wwtp-mini")).await;
    let response = reqwest::Client::new()
        .post(server.rpc_url())
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "document.apply",
            "params": {}
        }))
        .send().await.unwrap();
    assert_eq!(response.status(), 401);
}

#[tokio::test]
async fn unknown_methods_return_a_stable_json_rpc_error() {
    let server = start_test_server(ServerConfig::for_tests("wwtp-mini")).await;
    let response = reqwest::Client::new()
        .post(server.rpc_url())
        .bearer_auth(server.token())
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "unknown.method",
            "params": {}
        }))
        .send().await.unwrap();
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["error"]["code"], -32601);
}

#[tokio::test]
async fn publishes_a_revision_event_after_a_successful_commit() {
    let server = start_test_server(ServerConfig::for_tests("wwtp-mini")).await;
    let mut events = server.subscribe().await;
    server.apply_fixture_batch().await.unwrap();
    let event = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        events.recv(),
    ).await.unwrap().unwrap();
    assert_eq!(event.revision, 1);
    assert_eq!(event.event, "revision.committed");
}
```

- [ ] **Step 2: Run tests and verify RED**

Run: `cargo test -p studio-server --test rpc_contract`

Expected: compilation fails because the server crate and test server do not exist.

- [ ] **Step 3: Implement the server**

Use `axum` and `tokio`. Bind only to `127.0.0.1`, generate a random 32-byte session token on
startup, print the token to stdout, and require `Authorization: Bearer <token>` for mutating
methods. Read-only requests remain unauthenticated for local discovery.

Add these workspace dependencies and pin them in `Cargo.lock`:

```toml
axum = { version = "0.8", features = ["ws"] }
tokio = { version = "1", features = ["fs", "macros", "net", "rt-multi-thread", "sync", "time"] }
tempfile = "3"
reqwest = { version = "0.12", features = ["json"] }
clap = { version = "4", features = ["derive"] }
```

`studio-server` depends on `studio-core`, `studio-project`, `serde`, `serde_json`, `axum`,
`tokio`, `thiserror`, `sha2`, `schemars`, and `ts-rs`. Its dev dependencies are `reqwest` and
`tempfile`. `studio-cli` depends on `studio-core`, `studio-server`, `clap`, `serde_json`,
`schemars`, and `ts-rs`. These packages are selected and pinned during Task 1; run
`cargo tree --workspace` during review and reject an unexpected transitive addition without
the active task requiring it.

Define exact RPC envelopes:

```rust
#[derive(Serialize, Deserialize)]
pub struct RpcRequest {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    pub method: String,
    pub params: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
pub struct RpcResponse {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    pub result: Option<serde_json::Value>,
    pub error: Option<RpcError>,
}
```

Implement `system.health`, `system.capabilities`, `project.create`, `project.open`,
`project.get`, `project.save`, `project.close`, `document.get`, `document.apply`,
`document.undo`, `document.redo`, `document.history`, `scene.get`, `scene.query`,
`scene.bounds`,
`validation.run`, `evaluation.evaluate`, `export.sourceGlb`, and
`export.runtimePreview`.

- [ ] **Step 4: Generate and verify schemas**

```bash
cargo run -p studio-cli -- schema --output schemas
cargo test -p studio-server
```

Expected: API integration tests pass; generated schema files include every RPC method and
Operation. A second generation must produce byte-identical output.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-server schemas Cargo.toml Cargo.lock
git commit -m "feat(studio-server): expose local ai modeling api"
```

---

### Task 7: Export Standard GLB and Orchestrate Runtime Preview

**Files:**
- Create: `crates/studio-core/src/export/glb.rs`
- Create: `crates/studio-server/src/preview.rs`
- Create: `crates/studio-server/tests/preview_pipeline.rs`
- Create: `scripts/preview-example.mjs`
- Modify: `crates/studio-core/src/lib.rs`
- Modify: `crates/studio-server/src/rpc.rs`

**Interfaces:**
- Consumes: validated Document and evaluated meshes.
- Produces: `export_glb(document: &Document) -> Result<Vec<u8>, ExportError>`.
- Produces: `RuntimePreview { revision, source_digest, preview_path, diagnostics }`.

- [ ] **Step 1: Write failing GLB and preview tests**

```rust
use studio_core::document::Document;
use studio_core::export::glb::export_glb;

#[test]
fn exports_a_standard_glb_header_and_json_chunk() {
    let bytes = export_glb(&Document::new("empty")).unwrap();
    assert_eq!(&bytes[0..4], b"glTF");
    assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 2);
    let json_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
    assert!(json_len > 0);
}

#[test]
fn exports_stable_bytes_for_the_same_revision() {
    let document = Document::new("stable");
    assert_eq!(
        export_glb(&document).unwrap(),
        export_glb(&document).unwrap(),
    );
}
```

- [ ] **Step 2: Run tests and verify RED**

Run: `cargo test -p studio-core --test glb_export`

Expected: compilation fails because GLB export does not exist.

- [ ] **Step 3: Implement deterministic standard GLB export**

Write a glTF 2.0 JSON chunk followed by a BIN chunk. Emit stable node order, matrices,
triangle indices, positions, materials, and `extras.ice3` metadata. Every buffer view names
its buffer explicitly. The exporter must not write the proprietary Prepared profile.

- [ ] **Step 4: Add Toolchain preview orchestration**

`studio-server/src/preview.rs` writes a source GLB into the current revision cache and
invokes only the approved local Toolchain command. The server requires
`STUDIO_TOOLCHAIN_CLI` to contain an existing JavaScript file under the configured
workspace; it never accepts a command path through the request:

```text
node "$STUDIO_TOOLCHAIN_CLI" build <source> --output <preview> \
  --asset-id <project-id> --budget unrestricted
```

The Node.js command is a peer build-time adapter; it is not linked into Studio Core. The
server validates command paths, captures stdout/stderr without secrets, checks output
existence and digest, and publishes the preview path only after success.

Run: `cargo test -p studio-core --test glb_export`

Run: `cargo test -p studio-server --test preview_pipeline`

Expected: valid source GLB, successful fixture preview, and previous preview retained on
adapter failure.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-core/src/export crates/studio-server scripts
git commit -m "feat(studio): export previewable glb assets"
```

---

### Task 8: Add the Minimal Wastewater Fixture and Deterministic AI Client

**Files:**
- Create: `fixtures/wastewater-mini/project.json`
- Create: `fixtures/wastewater-mini/ai-plan.json`
- Create: `scripts/run-ai-fixture.mjs`
- Create: `crates/studio-server/tests/ai_fixture.rs`
- Modify: `docs/guides/wastewater-fixture.md`

**Interfaces:**
- Consumes: public JSON-RPC methods and generated Schema.
- Produces: a reproducible fixture transaction sequence with no model credentials.
- Produces: one dry-run rejection, one corrected transaction, undo/redo, reload, and export.

- [ ] **Step 1: Write the failing end-to-end test**

```rust
use studio_server::{ServerConfig, start_test_server};

#[tokio::test]
async fn deterministic_ai_client_builds_and_reloads_wastewater_fixture() {
    let server = start_test_server(ServerConfig::for_fixture("wastewater-mini")).await;
    let result = server.run_fixture("fixtures/wastewater-mini/ai-plan.json").await;
    assert_eq!(result.health.status, "ok");
    assert_eq!(result.rejected_operations, 1);
    assert_eq!(result.final_revision, 2);
    assert_eq!(result.nodes["aeration_tanks"], 2);
    assert_eq!(result.nodes["secondary_clarifiers"], 2);
    assert_eq!(result.relations["process_connections"], 4);
    assert!(result.preview_ready);
}
```

- [ ] **Step 2: Run test and verify RED**

Run: `cargo test -p studio-server --test ai_fixture`

Expected: fixture and client runner do not exist.

- [ ] **Step 3: Author the deterministic AI transaction plan**

The plan contains material creation, ground, two aeration tanks, two secondary clarifiers,
two pumps, and at least four typed process connections. Every node and relation has an
explicit stable ID and metadata. The first attempt includes one missing target and must be
rejected. The second attempt fixes only that target.

- [ ] **Step 4: Run the AI fixture and inspect diagnostics**

Run: `cargo test -p studio-server --test ai_fixture`

Run: `node scripts/run-ai-fixture.mjs --fixture fixtures/wastewater-mini`

Expected: one structured rejection, one successful revision, undo/redo parity, reload
identity parity, preview GLB creation, and no credentials.

- [ ] **Step 5: Commit**

```bash
git add fixtures scripts/run-ai-fixture.mjs crates/studio-server/tests/ai_fixture.rs docs/guides
git commit -m "test(studio): prove ai-driven wastewater modeling"
```

---

### Task 9: Build the Thin TypeScript Viewer

**Files:**
- Create: `apps/viewer/package.json`
- Create: `apps/viewer/tsconfig.json`
- Create: `apps/viewer/vite.config.ts`
- Create: `apps/viewer/index.html`
- Create: `apps/viewer/src/main.ts`
- Create: `apps/viewer/src/api.ts`
- Create: `apps/viewer/src/scene.ts`
- Create: `apps/viewer/src/panel.ts`
- Create: `apps/viewer/tests/api.test.mjs`
- Create: `apps/viewer/e2e/viewer.spec.ts`
- Create: `apps/viewer/playwright.config.ts`
- Modify: `README.md`

**Interfaces:**
- Consumes: `/health`, `/ws`, `scene.get`, `validation.run`, and `export.runtimePreview`.
- Produces: Viewer showing the latest prepared preview, revision, tree, selection, diagnostics.
- Produces: `npm run dev`, `npm run build`, `npm test`, `npm run test:e2e`, and `npm run verify`.

- [ ] **Step 1: Write failing API and viewer tests**

```ts
// apps/viewer/e2e/viewer.spec.ts
import { expect, test } from '@playwright/test';

test('shows current revision and loads the generated preview', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('status')).toContainText('Revision 2');
  await expect(page.getByRole('heading', { name: 'wastewater-mini' })).toBeVisible();
  await expect(page.locator('canvas[data-layer="scene"]')).toBeVisible();
});

test('shows a revision event without unmounting the scene', async ({ page }) => {
  await page.goto('/');
  const canvas = page.locator('canvas[data-layer="scene"]');
  const handle = await canvas.elementHandle();
  await page.evaluate(() => window.__studioTest.publishRevision(3));
  await expect(page.getByRole('status')).toContainText('Revision 3');
  expect(await canvas.elementHandle()).toBe(handle);
});
```

- [ ] **Step 2: Run tests and verify RED**

Run: `cd apps/viewer && npm run test:e2e`

Expected: Viewer package and DOM contracts do not exist.

- [ ] **Step 3: Implement the API client and scene adapter**

Use this exact package dependency set:

```json
{
  "name": "@ice3/studio-viewer",
  "private": true,
  "type": "module",
  "engines": {
    "node": "^24.0.0 || >=26.0.0",
    "npm": ">=11"
  },
  "scripts": {
    "dev": "vite",
    "build": "vite build",
    "typecheck": "tsc --noEmit",
    "test": "node --test tests/*.test.mjs",
    "test:e2e": "playwright test",
    "verify": "npm run typecheck && npm test && npm run build"
  },
  "dependencies": {
    "ice3": "file:../../../ice3-engine/packages/ice3"
  },
  "devDependencies": {
    "@playwright/test": "1.64.1",
    "typescript": "7.0.2",
    "vite": "7.1.7"
  }
}
```

`api.ts` validates JSON-RPC responses and exposes:

```ts
export type StudioClient = {
  health(): Promise<{ apiVersion: string; schemaVersion: number }>;
  scene(): Promise<SceneSnapshot>;
  validation(): Promise<ValidationReport>;
  preview(revision: number): Promise<{ url: string }>;
  subscribe(onRevision: (revision: number) => void): () => void;
};
```

`scene.ts` owns the existing ICE3 Engine canvas and loads only the preview URL returned by
Studio. `panel.ts` renders project ID, revision, node tree, selected node metadata, and
diagnostics. It never constructs an `Operation` directly.

- [ ] **Step 4: Implement revision-safe preview updates**

Keep the current canvas mounted. When a new revision arrives, fetch its preview, load the
new scene off-stage, and replace the scene only after success. On failure, retain the prior
preview and show `ICE3_STUDIO_RUNTIME_PREVIEW_FAILED`.

`playwright.config.ts` starts `studio-cli serve --fixture wastewater-mini` on
`127.0.0.1:8120`, starts Vite on `127.0.0.1:8121`, and makes the test client load the fixture
preview. The test server uses a fixed token supplied only through the test process
environment.

Run: `cd apps/viewer && npm run typecheck`

Run: `cd apps/viewer && npm test`

Run: `cd apps/viewer && npm run build`

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add apps/viewer README.md
git commit -m "feat(studio-viewer): display api-driven models"
```

---

### Task 10: Complete Verification, Performance Evidence, and Documentation

**Files:**
- Create: `scripts/verify.mjs`
- Create: `benchmarks/studio-mvp.mjs`
- Create: `docs/guides/getting-started.md`
- Create: `docs/reference/api.md`
- Create: `docs/reference/diagnostics.md`
- Create: `docs/architecture.md`
- Create: `docs/decisions/0001-document-as-source-of-truth.md`
- Create: `docs/decisions/0002-local-api-boundary.md`
- Modify: `docs/README.md`
- Modify: `README.md`
- Modify: `AGENTS.md`

**Interfaces:**
- Consumes: all previous tasks.
- Produces: one root `npm run verify` orchestration command that invokes Rust and Viewer gates.
- Produces: machine-readable benchmark JSON with p50/p95 for project open, transaction, evaluation, and export.

- [ ] **Step 1: Write failing verification contract test**

```rust
// crates/studio-server/tests/verification_contract.rs
#[test]
fn documents_every_public_diagnostic_code() {
    let document = include_str!("../../../docs/reference/diagnostics.md");
    for code in studio_core::validation::DIAGNOSTIC_CODES {
        assert!(document.contains(code), "missing diagnostic: {code}");
    }
}
```

- [ ] **Step 2: Run test and verify RED**

Run: `cargo test -p studio-server --test verification_contract`

Expected: reference documentation and exported diagnostic codes do not exist.

- [ ] **Step 3: Implement the root verification and benchmark scripts**

`scripts/verify.mjs` runs sequentially:

```text
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p studio-cli -- schema --check
node --test apps/viewer/tests
cd apps/viewer && npm run typecheck && npm run build && npm run test:e2e
```

`benchmarks/studio-mvp.mjs` starts a release server, creates/opens the fixture, applies 100
operations, evaluates, exports, and writes JSON with p50/p95. It must fail if the spec's
performance limits are exceeded.

- [ ] **Step 4: Complete documentation and run the full gate**

Document startup, the AI API workflow, operation semantics, diagnostics, revision recovery,
and the difference between source GLB and Runtime preview. Run:

```bash
node scripts/verify.mjs
node benchmarks/studio-mvp.mjs
```

Expected: all checks pass and the benchmark report is written under `output/`.

- [ ] **Step 5: Commit**

```bash
git add scripts benchmarks docs README.md AGENTS.md
git commit -m "docs(studio): verify ai-native mvp workflow"
```

---

## Plan Self-Review

- Spec coverage: Rust Core, generic primitives, materials, relations, constraints, atomic
  revisions, local persistence, JSON-RPC, schema generation, GLB export, Toolchain preview,
  Viewer, security, error mapping, deterministic AI fixture, and performance evidence each
  have an owning task.
- Type consistency: IDs, `Document`, `NodeContent`, `Operation`, `ApplyRequest`,
  `ApplyResult`, `ProjectStore`, RPC methods, and `RuntimePreview` are introduced before
  consumers.
- Failure coverage: invalid geometry, stale revisions, duplicate idempotency keys, partial
  writes, path escape, invalid tokens, and preview failure have explicit tests.
- Scope check: the plan produces one coherent vertical slice. Boolean operations, advanced
  curves, sculpting, collaboration, cloud deployment, and incremental GPU transport remain
  outside this MVP.
