# Studio ChangeSet and Approval API Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Use checkbox (`- [ ]`) syntax for tracking.

> Document ID: ICE3-STUDIO-PLAN-002
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

**Goal:** Add a persistent, digest-bound ChangeSet review layer and split proposal/approval
capabilities without weakening the existing atomic revision model.

**Architecture:** `studio-core` evaluates ordered Operations on a clone and computes a
deterministic candidate. `studio-project` persists ChangeSets and reconstructs them from an
immutable base revision. `studio-server` exposes the ChangeSet lifecycle, candidate previews,
ordered events, and review-required approval credentials. A later Workbench plan consumes this
API.

**Tech Stack:** Rust 1.98, edition 2024, serde, schemars, ts-rs, SHA-256, Tokio, Axum, atomic
filesystem persistence, the existing Toolchain adapter, and Node's built-in test tooling for
protocol fixtures.

**Spec:** [`ICE3-STUDIO-SPEC-002`](../specs/2026-10-30-studio-web-workbench-design.md)

## Global Constraints

- Studio Core remains Rust; Node.js must not own mutation, validation, persistence, or revision
  semantics.
- The public API remains the only mutation boundary.
- A ChangeSet never mutates the committed Document before `changeset.commit`.
- Approval binds to both `expectedVersion` and the exact `approvedDigest`.
- Multiple ChangeSets may exist, but only one commit based on a revision can win.
- `review-required` is the default write mode.
- Agent proposal capability must not include commit, approval, or direct `document.apply`.
- The Workbench approval capability is established only through a loopback bootstrap flow and
  stored in a short-lived `HttpOnly`, `SameSite=Strict` cookie.
- Candidate Documents and previews are derived caches, never committed source data.
- Committed revisions retain stable IDs, deterministic ordering, and generated schemas.
- Domain-specific concepts are forbidden in Core and fixture-independent tests.
- Every behavior change begins with a failing test and ends with the complete relevant suite.
- Git author and committer must be `damoqiongqiu <damoqiongqiu@126.com>`.

## Review Focus

- A stale or modified candidate is approved after its reviewed preview: commit must reject unless
  the freshly evaluated digest equals `approvedDigest`.
- Proposal credentials attempt a direct Document write or approval: review-required mode must
  reject both without changing state.
- Two ChangeSets commit from the same base: exactly one revision commits; the other becomes
  `stale`.
- A damaged ChangeSet file or preview exists during project open: current revision remains
  loadable and the damaged draft is isolated with a diagnostic.
- A preview adapter fails after a previous valid candidate exists: the prior preview remains
  readable and no revision or ChangeSet digest changes.

---

### Task 1: Evaluate Ordered Operations Without Mutating the Base

**Files:**

- Create: `crates/studio-core/src/changeset.rs`
- Modify: `crates/studio-core/src/apply.rs`
- Modify: `crates/studio-core/src/lib.rs`
- Create: `crates/studio-core/tests/changeset_evaluation.rs`

**Interfaces:**

- Consumes: existing `Document`, `Operation`, `DocumentDiff`, `Diagnostic`, and `TransactionError`.
- Produces: `evaluate_operations(base: &Document, operations: &[Operation]) -> Result<CandidateEvaluation, TransactionError>`.
- Produces: `CandidateEvaluation { document: Document, diff: DocumentDiff, diagnostics: Vec<Diagnostic> }`.
- Produces: `candidate_digest(base: &Document, operations: &[Operation]) -> Result<String, TransactionError>`.

- [ ] **Step 1: Write failing candidate evaluation tests**

```rust
use studio_core::changeset::{candidate_digest, evaluate_operations};
use studio_core::document::Document;
use studio_core::ids::Revision;
use studio_core::operation::Operation;

#[test]
fn evaluates_a_clone_without_mutating_the_base() {
    let base = Document::new("candidate-test");
    let operations = vec![Operation::NodeCreate {
        node_id: "node-000001".into(),
        parent_id: None,
        name: "Assembly".into(),
        transform: Default::default(),
        content: Default::default(),
        material_id: None,
        metadata: Default::default(),
    }];

    let candidate = evaluate_operations(&base, &operations).unwrap();

    assert!(base.scene.nodes().is_empty());
    assert_eq!(candidate.document.revision, Revision::new(0));
    assert_eq!(candidate.document.scene.nodes().len(), 1);
    assert!(candidate.diagnostics.is_empty());
}

#[test]
fn candidate_digest_changes_with_operation_content() {
    let base = Document::new("digest-test");
    let first = vec![Operation::NodeCreate {
        node_id: "node-000001".into(),
        parent_id: None,
        name: "A".into(),
        transform: Default::default(),
        content: Default::default(),
        material_id: None,
        metadata: Default::default(),
    }];
    let mut second = first.clone();
    if let Operation::NodeCreate { name, .. } = &mut second[0] {
        *name = "B".into();
    }

    assert_eq!(
        candidate_digest(&base, &first).unwrap(),
        candidate_digest(&base, &first).unwrap()
    );
    assert_ne!(
        candidate_digest(&base, &first).unwrap(),
        candidate_digest(&base, &second).unwrap()
    );
}
```

- [ ] **Step 2: Run the tests and verify RED**

Run: `cargo test -p studio-core --test changeset_evaluation`

Expected: compilation fails because `studio_core::changeset` and its functions do not exist.

- [ ] **Step 3: Extract operation application from transaction commit**

In `apply.rs`, add one internal evaluation path used by both commit and ChangeSet:

```rust
pub struct OperationEvaluation {
    pub document: Document,
    pub diff: DocumentDiff,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn evaluate_operations(
    base: &Document,
    operations: &[Operation],
) -> Result<OperationEvaluation, TransactionError> {
    let mut candidate = base.clone();
    let mut diff = DiffBuilder::default();
    for (index, operation) in operations.iter().cloned().enumerate() {
        apply_operation(&mut candidate, operation, &mut diff, index)?;
    }
    let diagnostics = validate_document(&candidate);
    Ok(OperationEvaluation {
        document: candidate,
        diff: diff.finish(),
        diagnostics,
    })
}
```

Refactor `apply_transaction` to call `evaluate_operations`, reject nonempty diagnostics with
`TransactionError::ValidationFailed`, then create the revision and idempotency record exactly as
before.

- [ ] **Step 4: Add the candidate digest and re-export the API**

In `changeset.rs`, compute SHA-256 over a versioned canonical tuple containing the base ID,
revision, canonical base Document, and ordered Operations:

```rust
pub fn candidate_digest(
    base: &Document,
    operations: &[Operation],
) -> Result<String, TransactionError> {
    let bytes = serde_json::to_vec(&("ice3-changeset-v1", base, operations))
        .map_err(TransactionError::Serialization)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
```

Re-export `evaluate_operations` as `changeset::evaluate_operations`, and add `pub mod changeset;`
to `lib.rs`.

- [ ] **Step 5: Run both the focused and complete core suites**

Run: `cargo test -p studio-core --test changeset_evaluation`

Run: `cargo test -p studio-core`

Expected: both commands pass.

- [ ] **Step 6: Commit**

```bash
git add crates/studio-core/src/apply.rs crates/studio-core/src/changeset.rs \
  crates/studio-core/src/lib.rs crates/studio-core/tests/changeset_evaluation.rs
git commit -m "feat(studio-core): evaluate candidate change sets"
```

---

### Task 2: Define and Persist the ChangeSet Domain

**Files:**

- Create: `crates/studio-project/src/changeset.rs`
- Create: `crates/studio-project/src/changeset_store.rs`
- Modify: `crates/studio-project/src/lib.rs`
- Modify: `crates/studio-project/src/store.rs`
- Create: `crates/studio-project/tests/changeset_store.rs`

**Interfaces:**

- Consumes: `CandidateEvaluation`, `candidate_digest`, `Operation`, `Revision`, and `ProjectStore`.
- Produces: `ChangeSetId`, `ChangeSetStatus`, `ChangeSetSource`,
  `ChangeSetEntry`, `ChangeSet`, `ChangeSetEvaluation`.
- Produces: monotonically increasing ChangeSet `version: u64`.
- Produces: `ChangeSetStore::save(&ChangeSet)`, `load(&ChangeSetId)`, `list()`.
- Produces: `ProjectStore::change_set_store()` and `ProjectStore::load_revision(Revision)`.

- [ ] **Step 1: Write failing persistence tests**

```rust
use studio_core::document::Document;
use studio_core::ids::Revision;
use studio_project::changeset::{
    ChangeSet, ChangeSetId, ChangeSetSource, ChangeSetStatus,
};
use studio_project::{ProjectStore, WorkspacePath};

#[test]
fn persists_and_lists_change_sets_without_creating_a_revision() {
    let root = tempfile::tempdir().unwrap();
    let path = WorkspacePath::new(root.path(), "generic-project").unwrap();
    let store = ProjectStore::create(path).unwrap();
    store.write_snapshot(&Document::new("generic-project")).unwrap();
    let change_sets = store.change_set_store();
    let change_set = ChangeSet::new(
        ChangeSetId::parse("changeset-0123456789abcdef").unwrap(),
        "generic-project",
        Revision::new(0),
        "Adjust assembly",
        1_793_359_200_000,
    );
    change_sets.save(&change_set).unwrap();

    assert_eq!(change_sets.list().unwrap(), vec![change_set.clone()]);
    assert_eq!(store.load_current().unwrap().revision, Revision::new(0));
    assert_eq!(change_set.status, ChangeSetStatus::Draft);
}

#[test]
fn rejects_change_set_ids_with_path_characters() {
    assert!(ChangeSetId::parse("../../escape").is_err());
}

#[test]
fn isolates_a_corrupt_change_set_file() {
    let root = tempfile::tempdir().unwrap();
    let path = WorkspacePath::new(root.path(), "corrupt-draft").unwrap();
    let store = ProjectStore::create(path).unwrap();
    store.write_snapshot(&Document::new("corrupt-draft")).unwrap();
    let change_sets = store.change_set_store();
    std::fs::create_dir_all(change_sets.directory()).unwrap();
    std::fs::write(
        change_sets.directory().join("changeset-bad.json"),
        "{",
    )
    .unwrap();

    assert!(change_sets.list().unwrap().is_empty());
    assert_eq!(store.load_current().unwrap().project_id, "corrupt-draft");
    assert_eq!(change_sets.diagnostics().len(), 1);
}
```

- [ ] **Step 2: Run the tests and verify RED**

Run: `cargo test -p studio-project --test changeset_store`

Expected: compilation fails because the ChangeSet project modules do not exist.

- [ ] **Step 3: Implement the domain types**

Use validated opaque IDs and explicit status transitions:

```rust
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ChangeSetId(String);

impl ChangeSetId {
    pub fn parse(value: &str) -> Result<Self, ChangeSetError> {
        let Some(hex) = value.strip_prefix("changeset-") else {
            return Err(ChangeSetError::InvalidId);
        };
        if hex.len() == 16 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            Ok(Self(value.into()))
        } else {
            Err(ChangeSetError::InvalidId)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeSetStatus {
    Draft,
    Evaluating,
    Ready,
    Blocked,
    Stale,
    Committed,
    Discarded,
}
```

Define `ChangeSet` with the exact fields from spec section 9. `add_entries` and
`remove_entries` require `expected_version`, replace candidate fields with `None`, increment
version, and update `updated_at`.

- [ ] **Step 4: Implement atomic ChangeSet storage and recovery isolation**

Create `changesets/` during project creation and open. `ChangeSetStore::save` writes canonical
JSON through a sibling temporary file, `sync_all`, and rename. `list` reads each file
independently; malformed files are recorded in `diagnostics()` and skipped. Reject IDs or paths
outside the project changeset directory.

Add `ProjectStore::load_revision(revision)` using the revision path and schema validation. It must
load historical base revisions after the current revision advances.

- [ ] **Step 5: Run project and workspace tests**

Run: `cargo test -p studio-project`

Run: `cargo test --workspace`

Expected: all tests pass and the current revision remains unchanged after ChangeSet persistence.

- [ ] **Step 6: Commit**

```bash
git add crates/studio-project
git commit -m "feat(studio-project): persist candidate change sets"
```

---

### Task 3: Expose the ChangeSet Lifecycle Through JSON-RPC

**Files:**

- Create: `crates/studio-server/src/changeset.rs`
- Create: `crates/studio-server/tests/changeset_rpc.rs`
- Modify: `crates/studio-server/src/rpc.rs`
- Modify: `crates/studio-server/src/lib.rs`
- Modify: `crates/studio-server/src/events.rs`
- Modify: `crates/studio-cli/src/schema.rs`
- Modify: `schemas/studio-api.schema.json`
- Modify: `schemas/studio-types.ts`

**Interfaces:**

- Consumes: ChangeSet project persistence and Core candidate evaluation.
- Produces: `changeset.create`, `list`, `get`, `addOperations`, `removeOperations`, `evaluate`,
  `getCandidate`, `commit`, and `discard` RPC methods.
- Produces: `ChangeSetEvent { event, changeSetId, version, status, candidateDigest }`.

- [ ] **Step 1: Write failing RPC lifecycle tests**

```rust
#[tokio::test]
async fn stages_evaluates_and_commits_one_change_set() {
    let server = start_test_server(ServerConfig::for_tests("generic-project")).await;
    let created = rpc(&server, "changeset.create", serde_json::json!({
        "baseRevision": 0,
        "title": "Add assembly"
    })).await;
    let change_set_id = created["result"]["id"].as_str().unwrap();

    let added = rpc(&server, "changeset.addOperations", serde_json::json!({
        "changeSetId": change_set_id,
        "expectedVersion": 0,
        "entries": [{
            "source": { "kind": "agent", "label": "test-agent" },
            "operation": {
                "type": "node.create",
                "node_id": "node-000001",
                "parent_id": null,
                "name": "Assembly",
                "transform": {
                    "translation": { "x": 0, "y": 0, "z": 0 },
                    "rotation": [0, 0, 0, 1],
                    "scale": { "x": 1, "y": 1, "z": 1 }
                },
                "content": { "type": "group" },
                "material_id": null,
                "metadata": {}
            }
        }]
    })).await;
    assert_eq!(added["result"]["version"], 1);

    let evaluated = rpc(&server, "changeset.evaluate", serde_json::json!({
        "changeSetId": change_set_id,
        "expectedVersion": 1,
        "includeCandidate": true
    })).await;
    assert_eq!(evaluated["result"]["status"], "ready");
    let digest = evaluated["result"]["candidateDigest"].as_str().unwrap();

    let committed = rpc(&server, "changeset.commit", serde_json::json!({
        "changeSetId": change_set_id,
        "expectedVersion": 1,
        "approvedDigest": digest,
        "idempotencyKey": "approve-add-assembly"
    })).await;
    assert_eq!(committed["result"]["revision"], 1);

    let current = rpc(&server, "document.get", serde_json::json!({})).await;
    assert_eq!(current["result"]["scene"]["nodes"].as_array().unwrap().len(), 1);
}
```

- [ ] **Step 2: Verify RED**

Run: `cargo test -p studio-server --test changeset_rpc`

Expected: compilation fails because the ChangeSet RPC module and methods do not exist.

- [ ] **Step 3: Implement the ChangeSet service and RPC methods**

`changeset.create` loads the exact base revision and allocates a random 16-hex ID. `addOperations`
generates server-owned entry IDs and rejects unknown source kinds. `evaluate` calls
`evaluate_operations`; validation diagnostics set `blocked`, while valid candidates set `ready`,
store the digest, and expose the derived Document only when requested.

`commit` must perform these operations under the existing session mutex:

```rust
if change_set.version != request.expected_version {
    return Err(RpcError::with_diagnostic(
        -32016,
        "ICE3_STUDIO_OPERATION_INVALID",
        "ICE3_STUDIO_OPERATION_INVALID: ChangeSet version conflict".into(),
        None,
    ));
}
if current.revision != change_set.base_revision {
    change_set.status = ChangeSetStatus::Stale;
    change_sets.save(&change_set)?;
    return Err(RpcError::with_diagnostic(
        -32010,
        "ICE3_STUDIO_REVISION_CONFLICT",
        "ICE3_STUDIO_REVISION_CONFLICT".into(),
        None,
    ));
}
let candidate = evaluate_change_set(&base, &change_set)?;
if candidate.digest != request.approved_digest {
    return Err(RpcError::with_diagnostic(
        -32017,
        "ICE3_STUDIO_OPERATION_INVALID",
        "ICE3_STUDIO_OPERATION_INVALID: approved candidate digest is stale".into(),
        None,
    ));
}
let apply = ApplyRequest {
    expected_revision: change_set.base_revision,
    dry_run: false,
    idempotency_key: request.idempotency_key,
    operations: change_set.operations(),
};
let result = apply_transaction(&mut document, apply)?;
let history_event = serde_json::json!({
    "kind": "changeset.commit",
    "changeSetId": change_set.id,
    "approvedDigest": request.approved_digest,
    "entryCount": change_set.entries.len()
});
store.write_snapshot_with_history(&document, &history_event)?;
change_set.mark_committed(result.revision);
change_sets.save(&change_set)?;
let _ = session.events.send(ServerEvent::ChangeSetCommitted {
    change_set_id: change_set.id.clone(),
    version: change_set.version,
    revision: result.revision,
    candidate_digest: request.approved_digest,
});
let _ = session.events.send(ServerEvent::RevisionCommitted {
    revision: result.revision,
});
```

Add capability checks in Task 5; for this task, keep the existing token guard and mark the exact
guard replacement point.

- [ ] **Step 4: Implement ordered ChangeSet events**

Replace the revision-only broadcast payload with a serializable enum. Existing revision events
serialize to the same JSON shape:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum ServerEvent {
    #[serde(rename = "revision.committed")]
    RevisionCommitted { revision: Revision },
    #[serde(rename = "changeset.created")]
    ChangeSetCreated {
        change_set_id: ChangeSetId,
        version: u64,
        status: ChangeSetStatus,
    },
    #[serde(rename = "changeset.evaluated")]
    ChangeSetEvaluated {
        change_set_id: ChangeSetId,
        version: u64,
        status: ChangeSetStatus,
        candidate_digest: Option<String>,
    },
    #[serde(rename = "changeset.committed")]
    ChangeSetCommitted {
        change_set_id: ChangeSetId,
        version: u64,
        revision: Revision,
        candidate_digest: String,
    },
}
```

`changeset.commit` emits `changeset.committed` before `revision.committed`; tests assert revision
numbers and event order.

- [ ] **Step 5: Generate schemas and run focused/full suites**

Run: `cargo test -p studio-server --test changeset_rpc`

Run: `cargo test --workspace`

Run: `cargo run -q -p studio-cli -- schema --output schemas`

Run: `cargo run -q -p studio-cli -- schema --output schemas --check`

Expected: RPC tests pass, current revision remains unchanged after draft operations, and generated
schemas are deterministic.

- [ ] **Step 6: Commit**

```bash
git add crates/studio-server crates/studio-cli schemas
git commit -m "feat(studio-server): expose change set review api"
```

---

### Task 4: Generate and Cache Candidate Previews

**Files:**

- Modify: `crates/studio-server/src/changeset.rs`
- Modify: `crates/studio-server/src/preview.rs`
- Modify: `crates/studio-server/src/rpc.rs`
- Create: `crates/studio-server/tests/common/mod.rs`
- Modify: `crates/studio-server/tests/changeset_rpc.rs`
- Modify: `crates/studio-server/tests/preview_pipeline.rs`

**Interfaces:**

- Consumes: candidate Document, digest, and existing `PreviewAdapter`.
- Produces: `ServerConfig.toolchain_cli: Option<PathBuf>` and
  `TestServer::set_toolchain_cli(PathBuf)` so preview tests do not mutate process environment.
- Produces: `ExportRuntimePreviewRequest.candidateDigest` support.
- Produces: `GET /previews/candidates/{digest}`.
- Produces: `CandidatePreview { assetId, digest, url }`.
- Produces: shared test helpers `start_with_toolchain`, `replace_toolchain_body`,
  `rpc_with_bearer`, `rpc_with_approval_cookie`, `prepare_valid_candidate_preview`,
  `prepare_invalid_preview_candidate`, and the `SUCCESS_TOOLCHAIN`/`FAILED_TOOLCHAIN` fixtures.

- [ ] **Step 1: Write failing candidate preview tests**

```rust
#[tokio::test]
async fn caches_candidate_preview_by_digest() {
    let server = start_with_toolchain(
        "candidate-preview",
        "import fs from 'node:fs'; const a=process.argv.slice(2); const i=a.indexOf('--output'); fs.copyFileSync(a[1], a[i+1]);",
    ).await;
    let evaluated = evaluate_one_node_change_set(&server).await;
    let digest = evaluated["candidateDigest"].as_str().unwrap();

    let first = rpc(&server, "export.runtimePreview", serde_json::json!({
        "candidateDigest": digest
    })).await;
    let second = rpc(&server, "export.runtimePreview", serde_json::json!({
        "candidateDigest": digest
    })).await;

    assert_eq!(first["result"]["assetId"], second["result"]["assetId"]);
    assert_eq!(first["result"]["url"], format!("/previews/candidates/{digest}"));
}

#[tokio::test]
async fn failed_candidate_preview_preserves_previous_candidate() {
    let server = start_with_toolchain("candidate-failure", SUCCESS_TOOLCHAIN).await;
    let valid_digest = prepare_valid_candidate_preview(&server).await;
    replace_toolchain_body(&server, FAILED_TOOLCHAIN).await;
    let bad_digest = prepare_invalid_preview_candidate(&server).await;

    let response = rpc(&server, "export.runtimePreview", serde_json::json!({
        "candidateDigest": bad_digest
    })).await;
    assert_eq!(
        response["error"]["diagnosticCode"],
        "ICE3_STUDIO_RUNTIME_PREVIEW_FAILED"
    );
    assert!(reqwest::get(format!(
        "{}/previews/candidates/{valid_digest}",
        server.base_url()
    )).await.unwrap().status().is_success());
}
```

- [ ] **Step 2: Verify RED**

Run: `cargo test -p studio-server --test changeset_rpc caches_candidate_preview_by_digest`

Expected: test fails because candidate digest requests and candidate preview routes do not exist.

- [ ] **Step 3: Implement content-addressed candidate previews**

Move Toolchain selection from request-time environment lookup into `Session.toolchain_cli`,
initialized from `STUDIO_TOOLCHAIN_CLI`. Add the test-only setter declared in the Interfaces block.
Implement `tests/common/mod.rs` so toolchain helpers create a script inside
`TestServer::workspace_root()`, call `set_toolchain_cli`, and RPC helpers send either a bearer
token or the approval cookie. No test helper mutates process environment.

Store candidate source and prepared GLBs under:

```text
cache/
├── candidates/<digest>-source.glb
└── preview/<digest>.glb
```

Only accept digests currently registered by a loaded ChangeSet evaluation. Recompute the
candidate before exporting. Use a temporary preview filename plus rename. Serve `/previews/candidates/{digest}`
with `Content-Type: model/gltf-binary`.

- [ ] **Step 4: Preserve failure isolation**

On Toolchain failure, delete only the new temporary output, retain older digest files, leave the
ChangeSet `blocked`, and return `ICE3_STUDIO_RUNTIME_PREVIEW_FAILED`.

- [ ] **Step 5: Run preview and complete suites**

Run: `cargo test -p studio-server --test changeset_rpc`

Run: `cargo test -p studio-server --test preview_pipeline`

Run: `cargo test --workspace`

Expected: all tests pass and current revision remains unchanged.

- [ ] **Step 6: Commit**

```bash
git add crates/studio-server
git commit -m "feat(studio-server): preview exact change set candidates"
```

---

### Task 5: Enforce Proposal and Approval Capabilities

**Files:**

- Create: `crates/studio-server/src/capabilities.rs`
- Modify: `crates/studio-server/src/auth.rs`
- Modify: `crates/studio-server/src/rpc.rs`
- Modify: `crates/studio-server/src/lib.rs`
- Modify: `crates/studio-server/src/main.rs`
- Modify: `crates/studio-cli/src/main.rs`
- Modify: `crates/studio-server/tests/rpc_contract.rs`
- Modify: `crates/studio-server/tests/changeset_rpc.rs`

**Interfaces:**

- Consumes: existing loopback server and session token.
- Produces: `WriteMode::{ReviewRequired, Autonomous}`.
- Produces: proposal bearer token and approval cookie capabilities.
- Produces: bootstrap endpoint `GET /session/bootstrap?token=...`.
- Produces: `403` for missing capability and stable diagnostics.

- [ ] **Step 1: Write failing authorization tests**

```rust
#[tokio::test]
async fn proposal_token_cannot_commit_or_apply_directly() {
    let server = start_test_server(ServerConfig::review_required("generic-project")).await;
    let change_set = create_and_evaluate_change_set(&server, &server.agent_token()).await;

    let commit = rpc_with_bearer(
        &server,
        server.agent_token(),
        "changeset.commit",
        commit_params(&change_set),
    ).await;
    assert_eq!(commit["error"]["diagnosticCode"], "ICE3_STUDIO_AUTH_REQUIRED");

    let direct = rpc_with_bearer(
        &server,
        server.agent_token(),
        "document.apply",
        valid_apply_params(),
    ).await;
    assert_eq!(direct["error"]["diagnosticCode"], "ICE3_STUDIO_AUTH_REQUIRED");
}

#[tokio::test]
async fn approval_cookie_commits_the_exact_candidate() {
    let server = start_test_server(ServerConfig::review_required("generic-project")).await;
    let change_set = create_and_evaluate_change_set(&server, &server.agent_token()).await;
    let response = rpc_with_approval_cookie(
        &server,
        "changeset.commit",
        commit_params(&change_set),
    ).await;
    assert_eq!(response["result"]["revision"], 1);
}
```

- [ ] **Step 2: Verify RED**

Run: `cargo test -p studio-server --test changeset_rpc proposal_token_cannot_commit_or_apply_directly`

Expected: test fails because proposal and approval capabilities do not exist.

- [ ] **Step 3: Implement capability checking**

Create explicit capability methods:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    Read,
    Propose,
    Approve,
    AutonomousWrite,
}

pub fn authorize_write(
    mode: WriteMode,
    method: &str,
    agent_token: Option<&str>,
    approval_cookie: Option<&str>,
    expected_agent: &str,
    expected_approval: &str,
) -> Result<Capability, RpcError> {
    let proposal_method = matches!(
        method,
        "project.get"
            | "document.get"
            | "scene.get"
            | "scene.query"
            | "scene.bounds"
            | "validation.run"
            | "evaluation.evaluate"
            | "export.sourceGlb"
            | "export.runtimePreview"
            | "changeset.create"
            | "changeset.list"
            | "changeset.get"
            | "changeset.addOperations"
            | "changeset.removeOperations"
            | "changeset.evaluate"
            | "changeset.getCandidate"
    );
    let approval_method = matches!(
        method,
        "project.create"
            | "project.open"
            | "project.save"
            | "project.close"
            | "document.apply"
            | "document.undo"
            | "document.redo"
            | "changeset.commit"
            | "changeset.discard"
    );
    let agent_valid = agent_token.is_some_and(|token| {
        constant_time_eq(token.as_bytes(), expected_agent.as_bytes())
    });
    let approval_valid = approval_cookie.is_some_and(|token| {
        constant_time_eq(token.as_bytes(), expected_approval.as_bytes())
    });

    match mode {
        WriteMode::ReviewRequired if proposal_method && agent_valid => Ok(Capability::Propose),
        WriteMode::ReviewRequired if approval_method && approval_valid => Ok(Capability::Approve),
        WriteMode::Autonomous if agent_valid => Ok(Capability::AutonomousWrite),
        _ => Err(RpcError::with_diagnostic(
            -32001,
            "ICE3_STUDIO_AUTH_REQUIRED",
            "ICE3_STUDIO_AUTH_REQUIRED".into(),
            None,
        )),
    }
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (a, b)| difference | (a ^ b))
        == 0
}
```

Do not place credentials in query parameters after the bootstrap redirect. The bootstrap token is
single-use and expires after 60 seconds. Cookie attributes are `HttpOnly; SameSite=Strict; Path=/`.

- [ ] **Step 4: Add CLI write-mode configuration**

Add:

```text
--write-mode review-required|autonomous
```

Default to `review-required`. In review-required mode, print a one-time bootstrap URL to stdout;
write the agent proposal token to a file path supplied by `STUDIO_AGENT_TOKEN_FILE` or print it
only when no file path is configured. In autonomous mode, preserve a single trusted writer token.

- [ ] **Step 5: Run authorization and full tests**

Run: `cargo test -p studio-server --test changeset_rpc`

Run: `cargo test -p studio-server --test rpc_contract`

Run: `cargo test --workspace`

Expected: proposal and approval capabilities are isolated; all existing autonomous tests pass when
explicitly configured for autonomous mode.

- [ ] **Step 6: Commit**

```bash
git add crates/studio-server crates/studio-cli
git commit -m "feat(studio-server): separate propose and approve capabilities"
```

---

### Task 6: Prove Recovery, Concurrency, and Performance

**Files:**

- Modify: `crates/studio-server/tests/changeset_rpc.rs`
- Modify: `crates/studio-project/tests/changeset_store.rs`
- Modify: `benchmarks/studio-mvp.mjs`
- Create: `docs/reference/change-sets.md`
- Modify: `docs/reference/api.md`
- Modify: `docs/reference/diagnostics.md`
- Modify: `docs/README.md`
- Modify: `README.md`
- Modify: `AGENTS.md`

**Interfaces:**

- Consumes: complete ChangeSet and approval implementation.
- Produces: documented lifecycle, API, security model, recovery behavior, and benchmark metrics.

- [ ] **Step 1: Write failing restart and concurrency tests**

```rust
#[tokio::test]
async fn restart_restores_active_change_set_and_candidate() {
    let root = tempfile::tempdir().unwrap();
    let first = start_test_server_at(root.path()).await;
    let prepared = create_and_evaluate_change_set(&first, &first.agent_token()).await;
    first.stop().await;

    let second = start_test_server_at(root.path()).await;
    let restored = rpc(&second, "changeset.get", serde_json::json!({
        "changeSetId": prepared["id"]
    })).await;
    assert_eq!(restored["result"]["version"], prepared["version"]);
    assert_eq!(restored["result"]["candidateDigest"], prepared["candidateDigest"]);
}

#[tokio::test]
async fn concurrent_commits_from_one_base_have_one_winner() {
    let server = start_test_server(ServerConfig::review_required("concurrent")).await;
    let first = create_and_evaluate_change_set(&server, &server.agent_token()).await;
    let second = create_and_evaluate_change_set(&server, &server.agent_token()).await;

    let results = tokio::join!(
        commit_candidate(&server, &first),
        commit_candidate(&server, &second),
    );
    assert_eq!(
        results.iter().filter(|value| value["result"]["revision"] == 1).count(),
        1
    );
}
```

- [ ] **Step 2: Verify RED**

Run: `cargo test -p studio-server --test changeset_rpc restart_restores_active_change_set_and_candidate`

Expected: test infrastructure cannot stop/restart the server or restore the ChangeSet.

- [ ] **Step 3: Add restart test support and run concurrency**

Add `TestServer::stop(self)` that aborts the server task, waits briefly, and preserves its workspace
root. Add `start_test_server_at(path)` for restart. Ensure server startup loads ChangeSets but never
lets a damaged draft fail project open.

- [ ] **Step 4: Extend the benchmark**

Add p50/p95 metrics:

```js
changesetEvaluate100OperationsMs: 500,
changesetCandidatePreviewMs: 10_000,
```

Measure ten ChangeSet evaluations with 100 Operations each. Verify candidate evaluation does not
change the current revision. Keep the report at `output/studio-mvp-benchmark.json`.

- [ ] **Step 5: Complete documentation and run all gates**

Document every method, status transition, permission, cache path, digest rule, and recovery case.

Run: `npm run verify`

Run: `npm run benchmark`

Run: `cargo run -q -p studio-cli -- schema --output schemas --check`

Expected: all gates and performance limits pass.

- [ ] **Step 6: Commit**

```bash
git add benchmarks docs README.md AGENTS.md crates/studio-server/tests \
  crates/studio-project/tests
git commit -m "docs(studio): verify change set review workflow"
```

## Plan Self-Review

- Spec coverage: candidate evaluation, ChangeSet identity/version/status, persistence, exact
  approval digest, ordered events, candidate preview, write modes, restart, concurrency, security,
  diagnostics, performance, and documentation each have an owner.
- Placeholder scan: no deferred behavior or unnamed implementation steps.
- Type consistency: `ChangeSetId`, `version: u64`, `ChangeSetEntry`, `CandidateEvaluation`,
  `approvedDigest`, and `expectedVersion` are introduced before consumers.
- Review Focus coverage: digest race, permission bypass, concurrent commit, corrupt persistence,
  and preview failure have explicit tests.
- Scope check: this plan ends at a headless, independently testable API subsystem. It does not
  touch the React Workbench.
