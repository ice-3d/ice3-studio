# Revision-Scoped GPU Scene Session Implementation Plan

> Document ID: ICE3-STUDIO-PLAN-011
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Keep derived scene geometry, topology, and instance state resident for an open project,
then update only the resources affected by each committed revision.

**Architecture:** The CPU remains authoritative. A server-owned scene source index maps
`ApplyResult.diff` to a backend-neutral `ScenePatch`; a native GPU scene session reuses topology and
attribute resources, updates SoA instance ranges, and publishes a complete revision view only after
its GPU work succeeds. CPU and browser fallbacks remain unchanged and default-on.

**Tech Stack:** Rust 2024, `wgpu`, Tokio, JSON-RPC, TypeScript/React Viewer, Node.js 24.22.1,
Playwright, and the repository's native test and benchmark scripts.

**Spec:** `docs/specs/2026-10-31-gpu-scene-session-design.md`

## Global Constraints

- CPU always owns Document, revision, validation, approval, persistence, and canonical GLB output.
- GPU scene state is derived, process-local, and disposable.
- Native GPU scene sessions are experimental and default-off.
- Native compute remains CPU-selected by default; WebGL2 remains the default browser renderer.
- Browser WebGPU and preview packets remain explicit experimental capabilities.
- Scene sessions are enabled only when host configuration enables them.
- The Viewer never gains Document write authority.
- All production and test code follows RED-GREEN-REFACTOR; no implementation step precedes its
  failing test.
- Rust Core must not depend on Node.js, Toolchain, Viewer, or Engine.
- Use Node.js `24.22.1`; do not introduce Vite or Vitest.
- Do not use competitor or vendor names in code, comments, identifiers, documentation, links, or
  commits.
- Git author and committer name is `damoqiongqiu`.
- Preserve unrelated dirty files in the Engine working tree.
- Update machine-readable performance output whenever behavior or report semantics change.
- Root verification is `PATH=$HOME/.nvm/versions/node/v24.22.1/bin:$PATH npm run verify`.

## Review Focus

- A transform arrives while an earlier attribute update is still pending; the target revision must
  not overtake, mix with, or publish partial output from the earlier patch.
- A component definition changes while multiple direct and nested instances reference it; every
  affected expanded node must update and unrelated geometry must remain reused.
- Device loss occurs after a patch is queued but before a consumer waits; the previous complete view
  must remain identified as previous, and CPU/WebGL2 fallback must remain usable.
- Shared topology is referenced by published and pending views while the byte budget is reached;
  eviction must fail closed rather than release a referenced buffer.
- A GPU revision fails while a preview is requested; the request must not expose a target-revision
  partial packet or mutate the committed Document revision.

---

## File Structure

- `crates/studio-project/src/layered_geometry_cache.rs`: memory-first cache wrapper over the durable
  disk cache.
- `crates/studio-core/src/scene_contract.rs`: backend-neutral scene patch, dirty range, session, and
  status contracts.
- `crates/studio-core/src/large_scene/evaluate.rs`: reusable selected-node scene evaluation.
- `crates/studio-server/src/scene_session.rs`: CPU source index, patch planning, and cached-scene
  snapshot maintenance.
- `crates/studio-gpu/src/residency/resources.rs`: shared topology and replaceable attribute
  resources.
- `crates/studio-gpu/src/scene/instances.rs`: SoA instance records and range updates.
- `crates/studio-gpu/src/scene/queue.rs`: ordered pending revisions and safe coalescing.
- `crates/studio-gpu/src/scene/session.rs`: GPU scene publication, waits, status, and resource
  lifecycle.
- `crates/studio-gpu/src/scene/mod.rs`: scene module exports.
- `crates/studio-gpu/src/device.rs`: scene-session methods on the native backend.
- `crates/studio-server/src/compute.rs`: host enablement, public scene API, status, and fallback.
- `crates/studio-server/src/rpc.rs`: project and revision lifecycle integration.
- `apps/viewer/src/api/StudioClient.ts`: scene status declarations.
- `apps/viewer/src/compute/ComputePanel.tsx`: pending and fallback presentation.
- `benchmarks/gpu-scene-session.mjs`: real large-scene CPU versus warm GPU revision measurements.
- `scripts/verify.mjs`: benchmark-script syntax gate.
- `README.md`, `docs/architecture.md`, `docs/reference/api.md`, `docs/README.md`: implemented
  behavior, flag, status, and evidence.

## Task 1: Retain CPU Scene Geometry In Memory

**Files:**
- Create: `crates/studio-project/src/layered_geometry_cache.rs`
- Modify: `crates/studio-project/src/lib.rs`
- Modify: `crates/studio-server/src/rpc.rs:1368-1391`
- Modify: `crates/studio-server/src/rpc.rs:439-505`
- Test: `crates/studio-project/tests/layered_geometry_cache.rs`

**Interfaces:**
- Consumes: `GeometryStore`, `GeometryDiskCache`, `GeometryCacheEntry`, `MemoryGeometryStore`.
- Produces: `LayeredGeometryCache::new(project_dir, max_bytes) -> Result<Self,
  GeometryCacheError>` and a `GeometryStore` implementation.

- [ ] **Step 1: Write failing cache tests**

```rust
use studio_core::geometry::{Bounds, EvaluatedMesh, Vertex};
use studio_core::gpu_contract::{GeometryBackendKind, ResidentGeometryId};
use studio_core::large_scene::cache::{GeometryCacheEntry, GeometryCacheMetadata, GeometryStore};
use studio_core::math::Vec3;
use studio_project::layered_geometry_cache::LayeredGeometryCache;

fn entry() -> GeometryCacheEntry {
    GeometryCacheEntry {
        mesh: EvaluatedMesh {
            vertices: vec![Vertex {
                position: [1.0, 2.0, 3.0],
                normal: [0.0, 1.0, 0.0],
            }],
            indices: vec![0, 0, 0],
            bounds: Bounds {
                min: Vec3 { x: 1.0, y: 2.0, z: 3.0 },
                max: Vec3 { x: 1.0, y: 2.0, z: 3.0 },
            },
        },
        metadata: GeometryCacheMetadata {
            backend: GeometryBackendKind::Gpu,
            kernel_version: "geometry-kernel-v1".into(),
            parity_status: "passed".into(),
        },
        resident_geometry_id: Some(ResidentGeometryId("resident-1".into())),
    }
}

#[test]
fn memory_hit_survives_removal_of_the_disk_entry() {
    let temp = tempfile::tempdir().unwrap();
    let mut cache = LayeredGeometryCache::new(temp.path(), 1024 * 1024).unwrap();
    cache.store_entry("scene-cache".into(), entry());
    std::fs::remove_file(temp.path().join("cache/geometry/scene-cache.mesh")).unwrap();

    let loaded = cache.load_entry("scene-cache").unwrap();
    assert_eq!(loaded.mesh.vertices[0].position, [1.0, 2.0, 3.0]);
    assert_eq!(loaded.resident_geometry_id.unwrap().0, "resident-1");
}

#[test]
fn cold_load_populates_memory_from_disk() {
    let temp = tempfile::tempdir().unwrap();
    let mut first = LayeredGeometryCache::new(temp.path(), 1024 * 1024).unwrap();
    first.store_entry("scene-cache".into(), entry());
    drop(first);

    let mut second = LayeredGeometryCache::new(temp.path(), 1024 * 1024).unwrap();
    assert!(second.load_entry("scene-cache").is_some());
}
```

- [ ] **Step 2: Run tests and observe the missing-module failure**

Run:

```bash
cargo test -p studio-project --test layered_geometry_cache
```

Expected: compilation fails because `studio_project::layered_geometry_cache` and
`LayeredGeometryCache` do not exist.

- [ ] **Step 3: Implement the memory-first store**

```rust
use std::path::Path;

use studio_core::geometry::EvaluatedMesh;
use studio_core::large_scene::cache::{
    GeometryCacheEntry, GeometryStore, MemoryGeometryStore,
};

use crate::geometry_cache::{GeometryCacheError, GeometryDiskCache};

pub struct LayeredGeometryCache {
    memory: MemoryGeometryStore,
    disk: GeometryDiskCache,
}

impl LayeredGeometryCache {
    pub fn new(project_dir: impl AsRef<Path>, max_bytes: u64) -> Result<Self, GeometryCacheError> {
        Ok(Self {
            memory: MemoryGeometryStore::default(),
            disk: GeometryDiskCache::with_max_bytes(project_dir, max_bytes)?,
        })
    }
}

impl GeometryStore for LayeredGeometryCache {
    fn load(&self, key: &str) -> Option<EvaluatedMesh> {
        self.load_entry(key).map(|entry| entry.mesh)
    }

    fn store(&mut self, key: String, mesh: EvaluatedMesh) {
        self.store_entry(key, GeometryCacheEntry {
            mesh,
            metadata: studio_core::large_scene::cache::GeometryCacheMetadata::cpu(),
            resident_geometry_id: None,
        });
    }

    fn load_entry(&self, key: &str) -> Option<GeometryCacheEntry> {
        if let Some(entry) = self.memory.load_entry(key) {
            return Some(entry);
        }
        let entry = self.disk.load_entry(key)?;
        // The disk loader intentionally drops process-local resident IDs.
        Some(entry)
    }

    fn store_entry(&mut self, key: String, entry: GeometryCacheEntry) {
        self.memory.store_entry(key.clone(), entry.clone());
        self.disk.store_entry(key, entry);
    }
}
```

Make `layered_geometry_cache` public from `studio-project`. Add
`scene_cache: LayeredGeometryCache` to `Session`. Initialize it in `initialize_state` from
`store.project_dir()`. Reinitialize it in both `project.create` and `project.open`. Change
`evaluate_session_scene(session: &mut Session)` to pass `&mut session.scene_cache`; every existing
caller already holds a mutable `Session`.

The `load_entry` cache-fill behavior must be implemented before the first test passes. Use
`self.disk.load_entry(key)`, store a clone into memory, then return that clone. Disk-loaded entries
retain `resident_geometry_id: None`; never synthesize a device handle from disk metadata.

- [ ] **Step 4: Run project and server tests**

Run:

```bash
cargo test -p studio-project --test layered_geometry_cache
cargo test -p studio-server --test large_scene_rpc
cargo test -p studio-server
```

Expected: all tests pass, including repeated `evaluation.stats` without recreating
`GeometryDiskCache`.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-project/src/lib.rs \
  crates/studio-project/src/layered_geometry_cache.rs \
  crates/studio-project/tests/layered_geometry_cache.rs \
  crates/studio-server/src/rpc.rs
git commit -m "perf(studio-server): retain scene geometry in memory"
```

## Task 2: Define Scene Patch And Dirty-Range Contracts

**Files:**
- Create: `crates/studio-core/src/scene_contract.rs`
- Modify: `crates/studio-core/src/lib.rs`
- Modify: `crates/studio-core/src/gpu_contract.rs`
- Test: `crates/studio-core/tests/scene_contract.rs`

**Interfaces:**
- Consumes: `NodeContent`, `GeometryDigestSet`, `GeometryDirtyMask`, `Transform`, `MaterialId`.
- Produces: `ElementRange`, `SceneGeometryChange`, `SceneInstanceChange`, `ScenePatch`,
  `SceneSessionId`, `SceneSessionState`, `SceneSessionStatus`, `SceneSubmission`, and
  `changed_position_ranges`.

- [ ] **Step 1: Write failing contract and range tests**

```rust
use studio_core::document::NodeContent;
use studio_core::geometry::TriangleMesh;
use studio_core::gpu_contract::{GeometryDirtyChannel};
use studio_core::scene_contract::{
    ElementRange, changed_position_ranges, scene_content_dirty,
};

#[test]
fn changed_positions_are_compressed_into_contiguous_ranges() {
    let before = TriangleMesh {
        positions: vec![[0.0; 3], [1.0; 3], [2.0; 3], [3.0; 3]],
        indices: vec![0, 1, 2, 1, 2, 3],
    };
    let after = TriangleMesh {
        positions: vec![[0.0; 3], [4.0; 3], [5.0; 3], [3.0; 3]],
        indices: before.indices.clone(),
    };

    assert_eq!(
        changed_position_ranges(&before, &after),
        Some(vec![ElementRange { start: 1, count: 2 }])
    );
}

#[test]
fn topology_changes_do_not_claim_an_incremental_range() {
    let before = TriangleMesh {
        positions: vec![[0.0; 3]; 3],
        indices: vec![0, 1, 2],
    };
    let after = TriangleMesh {
        positions: vec![[0.0; 3]; 4],
        indices: vec![0, 1, 2],
    };

    assert_eq!(changed_position_ranges(&before, &after), None);
}

#[test]
fn custom_mesh_attributes_mark_normals_and_bounds_dirty() {
    let before = NodeContent::CustomMesh(TriangleMesh {
        positions: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        indices: vec![0, 1, 2],
    });
    let after = NodeContent::CustomMesh(TriangleMesh {
        positions: vec![[0.0; 3], [2.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        indices: vec![0, 1, 2],
    });
    let dirty = scene_content_dirty(&before, &after);

    assert!(dirty.contains(GeometryDirtyChannel::Positions));
    assert!(dirty.contains(GeometryDirtyChannel::Normals));
    assert!(dirty.contains(GeometryDirtyChannel::Bounds));
    assert!(!dirty.contains(GeometryDirtyChannel::Topology));
}
```

- [ ] **Step 2: Run tests and observe missing contract types**

Run:

```bash
cargo test -p studio-core --test scene_contract
```

Expected: compilation fails because `scene_contract` does not exist.

- [ ] **Step 3: Implement the contracts and range compression**

Add these public types with `Debug, Clone, PartialEq`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElementRange {
    pub start: u32,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneGeometryChange {
    pub geometry_id: String,
    pub previous_geometry_id: Option<String>,
    pub content: NodeContent,
    pub digests: GeometryDigestSet,
    pub dirty: GeometryDirtyMask,
    pub position_ranges: Vec<ElementRange>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneInstanceChange {
    pub node_id: String,
    pub geometry_id: Option<String>,
    pub matrix: [f32; 16],
    pub material_id: Option<String>,
    pub visible: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScenePatch {
    pub base_revision: u64,
    pub target_revision: u64,
    pub full_rebuild: bool,
    pub geometry: Vec<SceneGeometryChange>,
    pub instances: Vec<SceneInstanceChange>,
    pub removed_geometry_ids: Vec<String>,
    pub removed_node_ids: Vec<String>,
}
```

Define `SceneSessionId(String)`, `SceneSessionState::{Idle, Pending, Failed, Lost}`, and a
schema-exported `SceneSessionStatus` containing every field from spec section 12 plus:

- `session_id: Option<String>`;
- cumulative `submission_count: usize`;
- cumulative `geometry_dispatch_count: usize`.

Define `SceneSubmission` with the session ID, target revision, submission count, geometry dispatch
count, instance range count, upload bytes, readback bytes, and `GeometryPhaseEvidence`.

Implement `changed_position_ranges` by returning `None` when vertex or index counts differ. Otherwise
walk both position arrays, mark changed vertex indices, and coalesce adjacent indices into
`ElementRange`. `scene_content_dirty` returns positions, normals, and bounds for same-topology
position changes; it returns topology, positions, indices, normals, and bounds for structural
changes.

Add optional scene methods with unsupported defaults to `GeometryBackend`:

```rust
async fn create_scene_session(
    &self,
    _project_id: &str,
) -> Result<SceneSessionId, BackendError> {
    Err(BackendError::Unavailable { reason: "scene sessions are unsupported".into() })
}

async fn apply_scene_patch(
    &self,
    _session: &SceneSessionId,
    _patch: ScenePatch,
) -> Result<SceneSubmission, BackendError> {
    Err(BackendError::Unavailable { reason: "scene sessions are unsupported".into() })
}

async fn wait_scene_revision(
    &self,
    _session: &SceneSessionId,
    _revision: u64,
) -> Result<SceneSessionStatus, BackendError> {
    Err(BackendError::Unavailable { reason: "scene sessions are unsupported".into() })
}

async fn close_scene_session(&self, _session: &SceneSessionId) -> Result<(), BackendError> {
    Err(BackendError::Unavailable { reason: "scene sessions are unsupported".into() })
}

fn scene_status(&self, _session: &SceneSessionId) -> Option<SceneSessionStatus> {
    None
}
```

- [ ] **Step 4: Run contract tests and generated binding tests**

Run:

```bash
cargo test -p studio-core --test scene_contract
cargo test -p studio-core --test gpu_contract
cargo test -p studio-cli --test schema_output
```

Expected: all pass; generated binding tests may first fail until the next schema-export command
regenerates `crates/studio-core/bindings`.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-core/src/lib.rs \
  crates/studio-core/src/scene_contract.rs \
  crates/studio-core/src/gpu_contract.rs \
  crates/studio-core/tests/scene_contract.rs \
  crates/studio-core/bindings
git commit -m "feat(studio-core): define revision scene patches"
```

## Task 3: Evaluate Only Affected Scene Sources

**Files:**
- Modify: `crates/studio-core/src/large_scene/evaluate.rs`
- Test: `crates/studio-core/tests/large_scene_incremental.rs`
- Test: `crates/studio-core/tests/large_scene_evaluation.rs`

**Interfaces:**
- Consumes: `Document`, `GeometryStore`, `GeometryEvaluator`, `NodeId`.
- Produces: `evaluate_large_scene_selected(document, store, evaluator,
  selected_source_nodes: &BTreeSet<NodeId>) -> Result<EvaluatedScene, LargeSceneError>`.

- [ ] **Step 1: Write a failing selected-node evaluation test**

Append to `crates/studio-core/tests/large_scene_incremental.rs`:

```rust
#[test]
fn selected_evaluation_expands_only_named_source_nodes() {
    let mut document = Document::new("selected-scene");
    document.scene.nodes_mut().extend([
        SceneNode {
            id: NodeId::from("box-000001"),
            name: "Primary".into(),
            parent_id: None,
            transform: Transform::IDENTITY,
            material_id: None,
            content: NodeContent::Primitive(Primitive::Box {
                width: 1.0,
                height: 1.0,
                depth: 1.0,
            }),
            metadata: BTreeMap::new(),
            visible: true,
        },
        SceneNode {
            id: NodeId::from("box-000002"),
            name: "Unrelated".into(),
            parent_id: None,
            transform: Transform::IDENTITY,
            material_id: None,
            content: NodeContent::Primitive(Primitive::Box {
                width: 2.0,
                height: 2.0,
                depth: 2.0,
            }),
            metadata: BTreeMap::new(),
            visible: true,
        },
    ]);
    let mut store = MemoryGeometryStore::default();
    let selected = [NodeId::from("box-000001")].into_iter().collect();
    let scene =
        evaluate_large_scene_selected(&document, &mut store, None, &selected).unwrap();

    assert_eq!(scene.nodes.len(), 1);
    assert_eq!(scene.nodes[0].id, "box-000001");
    assert_eq!(scene.meshes.len(), 1);
}
```

- [ ] **Step 2: Run the test and observe the missing function**

Run:

```bash
cargo test -p studio-core --test large_scene_incremental selected_evaluation -- --nocapture
```

Expected: compilation fails because `evaluate_large_scene_selected` is not exported.

- [ ] **Step 3: Extract one traversal and reuse it**

Move the current node loop from `evaluate_large_scene_with_cache_and_evaluator` into a shared
private function that takes `Option<&BTreeSet<NodeId>>`. Skip a source node only when a selection
exists and does not contain its source ID. Keep all definitions available to selected component
evaluation so nested references and connections resolve exactly as in a complete evaluation.

The complete evaluator calls the shared function with `None`; the selected evaluator calls it with
`Some(selected_source_nodes)`. Preserve stable ordering by iterating the original Document node
order. Do not recompute `sourceNodes` from the selection; `sourceNodes` remains the number of
document source nodes, while expanded and mesh statistics describe the returned selection.

- [ ] **Step 4: Run all large-scene tests**

Run:

```bash
cargo test -p studio-core --test large_scene_incremental
cargo test -p studio-core --test large_scene_evaluation
cargo test -p studio-core --test large_scene_transactions
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-core/src/large_scene/evaluate.rs \
  crates/studio-core/tests/large_scene_incremental.rs
git commit -m "feat(studio-core): evaluate selected scene sources"
```

## Task 4: Plan Server Scene Patches From Revision Diffs

**Files:**
- Create: `crates/studio-server/src/scene_session.rs`
- Modify: `crates/studio-server/src/lib.rs`
- Modify: `crates/studio-server/src/compute.rs`
- Test: `crates/studio-server/tests/scene_patch.rs`

**Interfaces:**
- Consumes: `Document`, `DocumentDiff`, `EvaluatedScene`, `ScenePatch`, and
  `evaluate_large_scene_selected`.
- Produces: `SceneSourceIndex::capture(document, scene, evaluator) -> SceneSourceIndex`,
  `SceneSourceIndex::affected_source_nodes(diff) -> Option<BTreeSet<NodeId>>`, and
  `build_full_scene_patch(document, scene, evaluator) -> Result<ScenePatch,
  LargeSceneError>`, and
  `build_scene_patch(current, diff, previous_scene, index, store, evaluator) ->
  Result<ScenePatch, LargeSceneError>`.

- [ ] **Step 1: Write failing planner tests**

```rust
use std::collections::BTreeMap;

use studio_core::document::{Document, NodeContent, SceneNode};
use studio_core::geometry::Primitive;
use studio_core::ids::{ComponentId, MaterialId, NodeId};
use studio_core::large_scene::component::{
    ComponentDefinition, ComponentInstance, ComponentPart, PartContent, PartTransform,
    ParameterValue, ScalarBinding,
};
use studio_core::large_scene::cache::MemoryGeometryStore;
use studio_core::large_scene::evaluate::evaluate_large_scene;
use studio_core::math::Transform;
use studio_server::scene_session::{SceneSourceIndex, build_scene_patch};

#[test]
fn transform_only_diff_contains_no_geometry_change() {
    let mut document = one_box_document();
    let previous_scene = evaluate_large_scene(&document).unwrap();
    let index = SceneSourceIndex::capture(&document, &previous_scene, None).unwrap();
    let node_id = document.scene.nodes()[0].id.clone();
    document.scene.nodes_mut()[0].transform.translation.x += 4.0;
    let diff = studio_core::diff::DocumentDiff {
        updated_node_ids: vec![node_id.clone()],
        ..Default::default()
    };
    let mut store = MemoryGeometryStore::default();
    let patch = build_scene_patch(
        &document,
        &diff,
        &previous_scene,
        &index,
        &mut store,
        None,
    )
    .unwrap();

    assert!(patch.geometry.is_empty());
    assert_eq!(patch.instances.len(), 1);
    assert_eq!(patch.instances[0].node_id, node_id.to_string());
}

#[test]
fn component_change_marks_every_direct_instance() {
    let (document, component_id, first_node, second_node) = two_instance_component_fixture();
    let previous_scene = evaluate_large_scene(&document).unwrap();
    let index = SceneSourceIndex::capture(&document, &previous_scene, None).unwrap();
    let diff = studio_core::diff::DocumentDiff {
        updated_component_ids: vec![component_id],
        ..Default::default()
    };

    let affected = index.affected_source_nodes(&diff).unwrap();
    assert_eq!(
        affected,
        [first_node, second_node]
            .into_iter()
            .collect()
    );
}

#[test]
fn unsupported_diff_requests_a_conservative_rebuild() {
    let document = one_box_document();
    let scene = evaluate_large_scene(&document).unwrap();
    let index = SceneSourceIndex::capture(&document, &scene, None).unwrap();
    let diff = studio_core::diff::DocumentDiff {
        updated_relation_ids: vec![studio_core::ids::RelationId::from("route-000001")],
        ..Default::default()
    };

    assert!(index.affected_source_nodes(&diff).is_none());
}

#[test]
fn material_change_selects_referencing_nodes_without_geometry() {
    let mut document = one_box_document();
    let material_id = MaterialId::from("material-000001");
    document.materials.push(studio_core::document::Material {
        id: material_id.clone(),
        name: "Steel".into(),
        base_color: [0.5, 0.5, 0.5, 1.0],
        opacity: 1.0,
        edge_color: None,
    });
    document.scene.nodes_mut()[0].material_id = Some(material_id.clone());
    let previous_scene = evaluate_large_scene(&document).unwrap();
    let index = SceneSourceIndex::capture(&document, &previous_scene, None).unwrap();
    let diff = studio_core::diff::DocumentDiff {
        updated_material_ids: vec![material_id],
        ..Default::default()
    };

    assert_eq!(
        index.affected_source_nodes(&diff).unwrap(),
        [NodeId::from("node-000001")].into_iter().collect()
    );
}

#[test]
fn nested_component_change_reaches_the_outer_instance() {
    let (document, leaf_component, node_id) = nested_component_fixture();
    let scene = evaluate_large_scene(&document).unwrap();
    let index = SceneSourceIndex::capture(&document, &scene, None).unwrap();
    let diff = studio_core::diff::DocumentDiff {
        updated_component_ids: vec![leaf_component],
        ..Default::default()
    };

    assert_eq!(
        index.affected_source_nodes(&diff).unwrap(),
        [node_id].into_iter().collect()
    );
}

#[test]
fn initial_patch_contains_every_derived_identity() {
    let mut document = one_box_document();
    document.revision = studio_core::ids::Revision::new(7);
    let scene = evaluate_large_scene(&document).unwrap();
    let patch = build_full_scene_patch(&document, &scene, None).unwrap();

    assert!(patch.full_rebuild);
    assert_eq!(patch.base_revision, 0);
    assert_eq!(patch.target_revision, 7);
    assert_eq!(patch.geometry.len(), 1);
    assert_eq!(patch.instances.len(), 1);
}
```

Define these helpers in `crates/studio-server/tests/scene_patch.rs`:

```rust
fn one_box_document() -> Document {
    let mut document = Document::new("scene-patch");
    document.scene.nodes_mut().push(SceneNode {
        id: NodeId::from("node-000001"),
        name: "Primary".into(),
        parent_id: None,
        transform: Transform::IDENTITY,
        material_id: None,
        content: NodeContent::Primitive(Primitive::Box {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        }),
        metadata: BTreeMap::new(),
        visible: true,
    });
    document
}

fn two_instance_component_fixture() -> (Document, ComponentId, NodeId, NodeId) {
    let mut document = Document::new("component-scene");
    let component_id = ComponentId::from("component-000001");
    document.components.push(ComponentDefinition {
        id: component_id.clone(),
        name: "Vessel".into(),
        parameters: Vec::new(),
        parts: vec![ComponentPart {
            id: "body".into(),
            parent_id: None,
            transform: PartTransform::default(),
            content: PartContent::Box {
                width: ScalarBinding::Literal(2.0),
                height: ScalarBinding::Literal(1.0),
                depth: ScalarBinding::Literal(1.0),
            },
            material_role: None,
            metadata: BTreeMap::new(),
            visible: true,
        }],
        ports: Vec::new(),
        metadata: BTreeMap::new(),
    });
    let first = NodeId::from("node-000001");
    let second = NodeId::from("node-000002");
    for (id, x) in [(first.clone(), 0.0), (second.clone(), 4.0)] {
        let mut node = SceneNode::group(id, "Vessel");
        node.transform.translation.x = x;
        node.content = NodeContent::ComponentInstance(ComponentInstance {
            definition_id: component_id.clone(),
            parameters: BTreeMap::new(),
            overrides: BTreeMap::new(),
            metadata: BTreeMap::new(),
        });
        document.scene.nodes_mut().push(node);
    }
    (document, component_id, first, second)
}

fn nested_component_fixture() -> (Document, ComponentId, NodeId) {
    let mut document = Document::new("nested-component-scene");
    let leaf = ComponentId::from("component-000001");
    document.components.push(ComponentDefinition {
        id: leaf.clone(),
        name: "Leaf".into(),
        parameters: Vec::new(),
        parts: vec![ComponentPart {
            id: "body".into(),
            parent_id: None,
            transform: PartTransform::default(),
            content: PartContent::Box {
                width: ScalarBinding::Literal(1.0),
                height: ScalarBinding::Literal(1.0),
                depth: ScalarBinding::Literal(1.0),
            },
            material_role: None,
            metadata: BTreeMap::new(),
            visible: true,
        }],
        ports: Vec::new(),
        metadata: BTreeMap::new(),
    });
    let outer = ComponentId::from("component-000002");
    document.components.push(ComponentDefinition {
        id: outer.clone(),
        name: "Outer".into(),
        parameters: Vec::new(),
        parts: vec![ComponentPart {
            id: "nested".into(),
            parent_id: None,
            transform: PartTransform::default(),
            content: PartContent::ComponentReference {
                definition_id: leaf.clone(),
                parameters: BTreeMap::<String, ParameterValue>::new(),
            },
            material_role: None,
            metadata: BTreeMap::new(),
            visible: true,
        }],
        ports: Vec::new(),
        metadata: BTreeMap::new(),
    });
    let node_id = NodeId::from("node-000003");
    let mut node = SceneNode::group(node_id.clone(), "Outer");
    node.content = NodeContent::ComponentInstance(ComponentInstance {
        definition_id: outer,
        parameters: BTreeMap::new(),
        overrides: BTreeMap::new(),
        metadata: BTreeMap::new(),
    });
    document.scene.nodes_mut().push(node);
    (document, leaf, node_id)
}
```

- [ ] **Step 2: Run tests and observe missing planner APIs**

Run:

```bash
cargo test -p studio-server --test scene_patch -- --nocapture
```

Expected: compilation fails because `studio_server::scene_session` does not exist.

- [ ] **Step 3: Implement source indexing and patch planning**

`SceneSourceIndex` stores no `Document` clone. It stores:

```rust
pub struct SceneSourceIndex {
    revision: u64,
    nodes: BTreeMap<NodeId, SourceNodeSnapshot>,
    component_instances: BTreeMap<ComponentId, BTreeSet<NodeId>>,
    material_nodes: BTreeMap<MaterialId, BTreeSet<NodeId>>,
    expanded_by_source: BTreeMap<NodeId, Vec<String>>,
}

struct SourceNodeSnapshot {
    content_digest: String,
    transform: Transform,
    material_id: Option<MaterialId>,
    visible: bool,
}
```

Build direct component-instance mappings from source `NodeContent::ComponentInstance` and
`NodeContent::InstanceSet`. Build nested dependency closure by following
`ComponentDefinition.parts` references and expanding each component's mapped source-node IDs.
Build a material-to-node reverse index from source node `material_id` values.

`affected_source_nodes` returns `None` for relations, constraints, unknown connection changes, or
any diff whose dependency closure cannot be proven. Otherwise return added, updated, and removed
source node IDs plus all instances depending on changed components. Added/removed nodes need no
previous content comparison.

For selected existing nodes, evaluate only affected source nodes with
`evaluate_large_scene_selected`. Compare the current source content digest with the prior digest:

```rust
if current_digest == prior.content_digest {
    // transform, material, visibility, name, and metadata changes only
    emit instance changes and no geometry changes
} else {
    emit geometry changes for each affected expanded part
}
```

Use `ResidencyKey::for_content` to create digest keys and resolved part IDs. Include
`previous_geometry_id` by looking up the corresponding derived node in `previous_scene`. If the
previous mapping is ambiguous, set `full_rebuild = true`.

`build_full_scene_patch` sets `full_rebuild = true`, uses base revision `0`, uses the Document
revision as target, and returns every derived geometry and instance from the complete current
scene. Initial open, project switch, and an unprovable diff use this function. Do not invent
partial deltas.

- [ ] **Step 4: Run planner and existing large-scene tests**

Run:

```bash
cargo test -p studio-server --test scene_patch
cargo test -p studio-server --test large_scene_rpc
cargo test -p studio-core --test large_scene_incremental
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-server/src/lib.rs \
  crates/studio-server/src/scene_session.rs \
  crates/studio-server/tests/scene_patch.rs \
  crates/studio-server/tests/fixtures
git commit -m "feat(studio-server): plan scene patches from revisions"
```

## Task 5: Split Shared Topology From Replaceable Attributes

**Files:**
- Create: `crates/studio-gpu/src/residency/resources.rs`
- Modify: `crates/studio-gpu/src/residency/mod.rs`
- Modify: `crates/studio-gpu/src/residency/store.rs`
- Modify: `crates/studio-gpu/src/kernels/mesh.rs`
- Modify: `crates/studio-gpu/src/device.rs`
- Test: `crates/studio-gpu/tests/scene_geometry_update.rs`

**Interfaces:**
- Consumes: `TriangleMesh`, `ResidencyKey`, `ElementRange`, `scene_content_dirty`.
- Produces: `NativeGpuBackend::update_resident_geometry(current_id, content, ranges) ->
  Result<GeometryResidentEntry, BackendError>`.

- [ ] **Step 1: Write failing GPU update tests**

```rust
use studio_core::document::NodeContent;
use studio_core::geometry::TriangleMesh;
use studio_core::gpu_contract::GeometryDirtyChannel;
use studio_core::scene_contract::changed_position_ranges;
use studio_gpu::residency::ResourceReferences;
use studio_gpu::NativeGpuBackend;

#[test]
fn stable_topology_update_reuses_indices_and_uploads_only_changed_positions() {
    let Some(backend) = NativeGpuBackend::for_test_gpu() else {
        return;
    };
    let first = mesh(1.0);
    let second = mesh(2.0);
    let seed = backend.evaluate_custom_mesh_resident(&first).unwrap();
    let ranges = changed_position_ranges(&first, &second).unwrap();
    let updated = backend
        .update_resident_geometry(
            &seed.id,
            &NodeContent::CustomMesh(second.clone()),
            ranges,
        )
        .unwrap();
    let snapshot = backend.snapshot_geometry(&updated.id).unwrap();

    assert_eq!(snapshot.indices, second.indices);
    assert_eq!(snapshot.vertices.len(), second.positions.len());
    assert!(updated.evidence.stats.upload_bytes < (second.positions.len() * 16) as u64);
    assert!(
        !updated
            .dirty
            .contains(GeometryDirtyChannel::Topology)
    );
}

#[test]
fn changed_index_count_allocates_new_topology() {
    let Some(backend) = NativeGpuBackend::for_test_gpu() else {
        return;
    };
    let first = mesh(1.0);
    let seed = backend.evaluate_custom_mesh_resident(&first).unwrap();
    let mut second = mesh(2.0);
    second.positions.push([4.0, 4.0, 0.0]);
    second.indices.extend([0, 4, 1]);
    let error = backend.update_resident_geometry(
        &seed.id,
        &NodeContent::CustomMesh(second),
        Vec::new(),
    );

    assert!(error.is_err());
}

#[test]
fn referenced_topology_is_never_an_eviction_candidate() {
    let mut references = ResourceReferences::default();
    references.add_geometry("geometry-1", "topology-1", "attribute-1");
    references.add_geometry("geometry-2", "topology-1", "attribute-2");
    references.remove_geometry("geometry-1");

    assert!(references.topology_is_referenced("topology-1"));
    assert_eq!(references.evictable_attributes(), vec!["attribute-1"]);
    assert!(references.evictable_topologies().is_empty());
}

fn mesh(width: f32) -> TriangleMesh {
    TriangleMesh {
        positions: vec![
            [-width, 0.0, 0.0],
            [width, 0.0, 0.0],
            [0.0, width, 0.0],
        ],
        indices: vec![0, 1, 2],
    }
}
```

- [ ] **Step 2: Run tests and observe the missing update API**

Run:

```bash
cargo test -p studio-gpu --test scene_geometry_update --release
```

Expected: compilation fails because `update_resident_geometry` does not exist.

- [ ] **Step 3: Implement reference-safe resource split**

Create `ResidentTopologyGpu` containing immutable indices, adjacency offsets, incidence, counts, and
workgroup metadata. Create `ResidentAttributeGpu` containing positions, face normals, vertex
normals, bounds, and the parameter buffer. Both own `Arc<DeviceResources>` so their `Drop`
implementations return leases exactly once.

Add a resource-reference table beside the pool. It maps result ID to topology ID and attribute ID,
tracks counts for both resource classes, and exposes eviction candidates only when their count is
zero. Removing a result decrements both counts but cannot release a topology still referenced by
another result.

Create `ResidentGeometryResources { topology: Arc<ResidentTopologyGpu>, attributes:
ResidentAttributeGpu }`. A stable-topology update keeps the topology `Arc`, allocates only a new
attribute set, writes changed position ranges through a transient aligned upload buffer, dispatches
the existing normal and bounds kernels, and publishes the new result only after success.

Reject incremental updates when index count, vertex count, or topology digest changes. Those
changes use normal cold creation. Never mutate a published attribute buffer in place.

Compute `GeometryExecutionStats.upload_bytes` from the actual aligned range bytes, not the complete
mesh size. Add the position range union to the returned `GeometryEvaluationEvidence.phases` by
setting `stats.upload_bytes` and the returned `dirty` mask.

- [ ] **Step 4: Run GPU residency, parity, and resource tests**

Run:

```bash
cargo test -p studio-gpu --test scene_geometry_update --release
cargo test -p studio-gpu --test resident_evaluation --release
cargo test -p studio-gpu --test resident_batch --release
cargo test -p studio-gpu --test resource_lifecycle
```

Expected: all pass. On a host with no adapter, adapter-dependent tests return without failure.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-gpu/src/device.rs \
  crates/studio-gpu/src/kernels/mesh.rs \
  crates/studio-gpu/src/residency/mod.rs \
  crates/studio-gpu/src/residency/resources.rs \
  crates/studio-gpu/src/residency/store.rs \
  crates/studio-gpu/tests/scene_geometry_update.rs
git commit -m "feat(studio-gpu): update resident attributes incrementally"
```

## Task 6: Add A SoA Instance Table

**Files:**
- Create: `crates/studio-gpu/src/scene/instances.rs`
- Create: `crates/studio-gpu/src/scene/mod.rs`
- Modify: `crates/studio-gpu/src/lib.rs`
- Test: `crates/studio-gpu/tests/instance_table.rs`

**Interfaces:**
- Consumes: `SceneInstanceChange`, `ScenePatch`.
- Produces: `InstanceTable::apply_changes(changes) -> InstanceTableUpdate`, `InstanceTable::records()`,
  and `InstanceTable::gpu_bytes()`.

- [ ] **Step 1: Write failing instance table tests**

```rust
use studio_core::scene_contract::SceneInstanceChange;
use studio_gpu::scene::instances::InstanceTable;

#[test]
fn transform_update_touches_only_the_matrix_range() {
    let mut table = InstanceTable::default();
    table.apply_changes(vec![instance("node-000001", 0.0)]);
    let update = table.apply_changes(vec![instance("node-000001", 3.0)]);

    assert_eq!(update.changed_ranges.len(), 1);
    assert_eq!(update.changed_ranges[0].start, 0);
    assert_eq!(update.changed_ranges[0].count, 1);
    assert_eq!(update.changed_bytes, 64);
    assert_eq!(table.records()[0].matrix[12], 3.0);
}

#[test]
fn stable_node_ids_reuse_the_same_instance_slot() {
    let mut table = InstanceTable::default();
    table.apply_changes(vec![
        instance("node-000001", 1.0),
        instance("node-000002", 2.0),
    ]);
    let update = table.apply_changes(vec![instance("node-000001", 5.0)]);

    assert_eq!(update.changed_ranges[0].start, 0);
    assert_eq!(table.records()[1].node_id, "node-000002");
}

fn instance(node_id: &str, x: f32) -> SceneInstanceChange {
    let mut matrix = [0.0; 16];
    matrix[0] = 1.0;
    matrix[5] = 1.0;
    matrix[10] = 1.0;
    matrix[15] = 1.0;
    matrix[12] = x;
    SceneInstanceChange {
        node_id: node_id.into(),
        geometry_id: Some("geometry-1".into()),
        matrix,
        material_id: None,
        visible: true,
    }
}
```

- [ ] **Step 2: Run tests and observe the missing module**

Run:

```bash
cargo test -p studio-gpu --test instance_table
```

Expected: compilation fails because `scene::instances` does not exist.

- [ ] **Step 3: Implement stable slot assignment and range planning**

Use a `BTreeMap<String, usize>` node index and parallel vectors for matrices, geometry IDs,
materials, and visibility. Preserve insertion order for existing slots. Removed slots are appended
to the free-slot queue and reused only after the patch that removes them has published.

`InstanceTableUpdate` contains:

```rust
pub struct InstanceTableUpdate {
    pub changed_ranges: Vec<studio_core::scene_contract::ElementRange>,
    pub changed_bytes: u64,
    pub removed_ranges: Vec<studio_core::scene_contract::ElementRange>,
}
```

Cover the current matrix stride plus stable semantic data when reporting changed bytes. A
transform-only update writes 64 bytes per changed instance. Material or visibility changes use the
same range claim but do not increment geometry dispatch.

- [ ] **Step 4: Run table tests**

Run:

```bash
cargo test -p studio-gpu --test instance_table
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-gpu/src/lib.rs \
  crates/studio-gpu/src/scene/mod.rs \
  crates/studio-gpu/src/scene/instances.rs \
  crates/studio-gpu/tests/instance_table.rs
git commit -m "feat(studio-gpu): track scene instances in SoA storage"
```

## Task 7: Implement Ordered GPU Scene Sessions

**Files:**
- Create: `crates/studio-gpu/src/scene/queue.rs`
- Create: `crates/studio-gpu/src/scene/session.rs`
- Modify: `crates/studio-gpu/src/scene/mod.rs`
- Modify: `crates/studio-gpu/src/device.rs`
- Test: `crates/studio-gpu/tests/scene_session.rs`

**Interfaces:**
- Consumes: scene contracts and tasks 5–6.
- Produces: `NativeGpuBackend::create_scene_session`, `apply_scene_patch`,
  `wait_scene_revision`, `scene_status`, and `close_scene_session`.

- [ ] **Step 1: Write failing session tests**

```rust
use studio_core::scene_contract::{
    SceneGeometryChange, SceneInstanceChange, ScenePatch, SceneSessionState,
};
use studio_gpu::NativeGpuBackend;

#[test]
fn pending_revisions_publish_in_order_and_transform_updates_skip_geometry() {
    let Some(backend) = NativeGpuBackend::for_test_gpu() else {
        return;
    };
    let session = pollster::block_on(backend.create_scene_session("scene-test")).unwrap();
    let first = pollster::block_on(backend.apply_scene_patch(
        &session,
        patch(1, 0, Some(geometry("geometry-1", 1.0)), instance("node-000001", 0.0)),
    ))
    .unwrap();
    assert_eq!(first.submission_count, 1);
    assert_eq!(
        backend.scene_status(&session).unwrap().state,
        SceneSessionState::Pending
    );

    let second = pollster::block_on(backend.apply_scene_patch(
        &session,
        patch(2, 1, None, instance("node-000001", 4.0)),
    ))
    .unwrap();
    assert_eq!(second.geometry_dispatch_count, 0);
    assert_eq!(second.readback_bytes, 0);

    let ready = pollster::block_on(backend.wait_scene_revision(&session, 2)).unwrap();
    assert_eq!(ready.published_revision, 2);
    assert_eq!(ready.target_revision, 2);
    assert_eq!(ready.state, SceneSessionState::Idle);
}

#[test]
fn device_failure_never_publishes_the_target_revision() {
    let Some(backend) = NativeGpuBackend::for_test_gpu() else {
        return;
    };
    let session = pollster::block_on(backend.create_scene_session("failure-test")).unwrap();
    let first = patch(1, 0, Some(geometry("geometry-1", 1.0)), instance("node-000001", 0.0));
    pollster::block_on(backend.apply_scene_patch(&session, first)).unwrap();
    pollster::block_on(backend.wait_scene_revision(&session, 1)).unwrap();

    backend.fail_scene_for_test(&session, "simulated device loss");
    let second = patch(2, 1, None, instance("node-000001", 3.0));
    let error = pollster::block_on(backend.apply_scene_patch(&session, second));
    assert!(error.is_err());
    let status = backend.scene_status(&session).unwrap();
    assert_eq!(status.published_revision, 1);
    assert_eq!(status.target_revision, 2);
    assert_eq!(status.state, SceneSessionState::Failed);
}
```

Keep helpers concrete: `geometry` creates a triangle `NodeContent` and `ResidencyKey`; `instance`
uses the same identity matrix construction shown in Task 6; `patch` sets base and target revision.
`fail_scene_for_test` is a `#[cfg(test)]` method or feature-gated test hook and cannot be reachable
from production RPC.

- [ ] **Step 2: Run tests and observe the missing session APIs**

Run:

```bash
cargo test -p studio-gpu --test scene_session --release
```

Expected: compilation fails because the scene-session methods do not exist.

- [ ] **Step 3: Implement the ordered session**

`NativeGpuBackend` owns `GpuSceneSessions` guarded by the existing device state mutex. A session
holds:

```rust
struct GpuSceneSession {
    project_id: String,
    device_generation: u64,
    published_revision: u64,
    target_revision: u64,
    state: SceneSessionState,
    instances: InstanceTable,
    geometries: BTreeMap<String, ResidentGeometryRecord>,
    topology_refs: BTreeMap<String, usize>,
    pending: SceneRevisionQueue,
    evidence: SceneSessionEvidence,
}
```

Validate `patch.base_revision == target_revision` and either
`patch.target_revision > patch.base_revision`, or `patch.full_rebuild` with base revision `0` for
the session's first publication. Reject stale, reordered, foreign-session, or post-device-loss
patches.

For a full rebuild, construct a staged instance table and staged geometry references. For an
incremental patch, apply only the supplied geometry and instance changes. Encode all compatible
geometry and instance writes into one `CommandEncoder`, call `queue.submit` once, and retain a
completion generation. Do not poll or wait during `apply_scene_patch`.

`wait_scene_revision` performs at most one wait for all pending submissions needed by the requested
revision, then atomically swaps staged instance and geometry references into the published view.
`scene_status` never waits and never changes publication state.

Implement coalescing in `SceneRevisionQueue`: while no wait has been requested, a later transform
for the same instance can replace the earlier pending transform, and an add-remove-add sequence may
collapse to the final add. Geometry updates with matching topology may replace their staged
attribute update. Never coalesce across component/geometry reference changes that alter identity or
across a requested consumer boundary.

- [ ] **Step 4: Run residency and scene tests**

Run:

```bash
cargo test -p studio-gpu --test scene_session --release
cargo test -p studio-gpu --test scene_geometry_update --release
cargo test -p studio-gpu --test resident_batch --release
cargo test -p studio-gpu --test resource_lifecycle
```

Expected: all pass; adapter-dependent tests may return early when no native adapter exists.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-gpu/src/device.rs \
  crates/studio-gpu/src/scene/mod.rs \
  crates/studio-gpu/src/scene/queue.rs \
  crates/studio-gpu/src/scene/session.rs \
  crates/studio-gpu/tests/scene_session.rs
git commit -m "feat(studio-gpu): keep revision scenes resident"
```

## Task 8: Expose Scene Sessions Through ComputeService

**Files:**
- Modify: `crates/studio-server/src/compute.rs`
- Modify: `crates/studio-server/src/rpc.rs:44-75`
- Modify: `crates/studio-cli/src/main.rs:25-70`
- Test: `crates/studio-server/tests/compute_scheduler.rs`

**Interfaces:**
- Consumes: `GeometryBackend` scene methods from Task 2 and status from Task 7.
- Produces: `ComputeService::open_scene_session(project_id)`, `apply_scene_patch`, `wait_scene`,
  `close_scene_session`, `scene_status`, and `ComputePolicy::scene_sessions_enabled`.

- [ ] **Step 1: Write failing ComputeService tests**

```rust
use studio_core::gpu_contract::{GeometryBackend, GeometryBackendMode};
use studio_server::compute::{ComputePolicy, ComputeService};

#[tokio::test]
async fn disabled_scene_sessions_do_not_initialize_backend_scene_state() {
    let gpu = FakeGpu::new(FakeOutcome::Success);
    let service = ComputeService::new(
        ComputePolicy {
            mode: GeometryBackendMode::Gpu,
            scene_sessions_enabled: false,
            ..ComputePolicy::default()
        },
        Some(gpu.clone() as std::sync::Arc<dyn GeometryBackend>),
    );

    assert!(service.open_scene_session("project-1").await.is_err());
    assert_eq!(gpu.scene_create_calls(), 0);
    assert!(!service.status().scene_session.enabled);
}

#[tokio::test]
async fn enabled_scene_sessions_report_pending_and_ready_revisions() {
    let gpu = FakeGpu::new(FakeOutcome::Success);
    let service = ComputeService::new(
        ComputePolicy {
            mode: GeometryBackendMode::Gpu,
            scene_sessions_enabled: true,
            ..ComputePolicy::default()
        },
        Some(gpu.clone() as std::sync::Arc<dyn GeometryBackend>),
    );
    let session = service.open_scene_session("project-1").await.unwrap();
    assert_eq!(gpu.scene_create_calls(), 1);
    assert_eq!(
        service.scene_status().unwrap().session_id,
        Some(session.0.clone())
    );
}
```

Extend the file-local `FakeGpu` used by `compute_scheduler.rs` with a
`scene_create_calls: AtomicUsize` field initialized to zero. Implement
`create_scene_session` by incrementing that counter and returning
`SceneSessionId("scene-1".into())`; implement `apply_scene_patch` and `wait_scene_revision` with the
deterministic pending and ready values used by these assertions. Do not create another Adapter or
duplicate the existing scheduler fake.

- [ ] **Step 2: Run tests and observe missing policy and methods**

Run:

```bash
cargo test -p studio-server --test compute_scheduler scene_
```

Expected: compilation fails because `scene_sessions_enabled`, scene methods, and scene status are
missing.

- [ ] **Step 3: Add the host gate and API**

Add `scene_sessions_enabled: false` to `ComputePolicy`. Extend `ComputeStatus` with:

```rust
pub scene_session: SceneSessionStatus,
```

The disabled status is concrete, not optional:

```rust
SceneSessionStatus {
    enabled: false,
    session_id: None,
    state: SceneSessionState::Idle,
    published_revision: 0,
    target_revision: 0,
    pending_patches: 0,
    pending_submissions: 0,
    geometry_count: 0,
    topology_count: 0,
    attribute_count: 0,
    instance_count: 0,
    resident_bytes: 0,
    peak_resident_bytes: 0,
    dirty_range_bytes: 0,
    readback_bytes: 0,
    submission_count: 0,
    geometry_dispatch_count: 0,
    fallback_reason: None,
    phases: GeometryPhaseEvidence::default(),
}
```

Store the active `SceneSessionId` behind the existing compute mutex discipline. `open_scene_session`
requires GPU mode, a healthy initialized backend, and the host gate. `close_scene_session` is
idempotent. `apply_scene_patch` maps unsupported, budget, device-loss, kernel, and parity errors to
existing `ComputeError` variants without mutating the active session ID.

Add CLI and server configuration:

```rust
scene_sessions_enabled: std::env::var("STUDIO_GPU_SCENE_SESSIONS")
    .is_ok_and(|value| value == "1"),
```

`--native-gpu cpu` must ignore the enable flag and keep the scene session disabled.

- [ ] **Step 4: Run scheduler and schema tests**

Run:

```bash
cargo test -p studio-server --test compute_scheduler scene_
cargo test -p studio-server --test compute_scheduler
cargo test -p studio-cli --test schema_output
```

Expected: all pass, and generated `ComputeStatus` declarations include `sceneSession`.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-server/src/compute.rs \
  crates/studio-server/src/rpc.rs \
  crates/studio-server/tests/compute_scheduler.rs \
  crates/studio-cli/src/main.rs \
  crates/studio-server/bindings schemas
git commit -m "feat(studio-server): expose experimental scene sessions"
```

## Task 9: Wire Scene Sessions Into Project And Revision Lifecycles

**Files:**
- Modify: `crates/studio-server/src/rpc.rs`
- Modify: `crates/studio-server/src/changeset.rs`
- Modify: `crates/studio-server/src/scene_session.rs`
- Test: `crates/studio-server/tests/scene_session_rpc.rs`

**Interfaces:**
- Consumes: Task 4 patch planner, Task 8 compute scene methods.
- Produces: committed revisions automatically staged after persistence; project open/close and
  undo/redo synchronize scene lifecycle.

- [ ] **Step 1: Write failing lifecycle tests**

```rust
mod common;

use studio_core::gpu_contract::GeometryBackendMode;
use studio_server::{ServerConfig, TestServer, start_test_server};

use common::rpc_with_bearer;

async fn rpc(
    server: &TestServer,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    rpc_with_bearer(server, server.token(), method, params).await
}

#[tokio::test]
async fn apply_returns_committed_revision_when_optional_scene_worker_fails() {
    let config = ServerConfig {
        compute_mode: GeometryBackendMode::Gpu,
        scene_sessions_enabled: true,
        ..ServerConfig::for_tests("scene-failure")
    };
    let server = start_test_server(config).await;
    let response = rpc(
        &server,
        "document.apply",
        serde_json::json!({
            "expected_revision": 0,
            "idempotency_key": "scene-create",
            "operations": [{
                "type": "node.create",
                "node_id": "node-000001",
                "parent_id": null,
                "name": "Primary",
                "transform": {
                    "translation": { "x": 0, "y": 0, "z": 0 },
                    "rotation": [0, 0, 0, 1],
                    "scale": { "x": 1, "y": 1, "z": 1 }
                },
                "content": {"type": "group"},
                "material_id": null,
                "metadata": {}
            }]
        }),
    )
    .await;

    assert_eq!(response["result"]["revision"], 1);
    assert_eq!(
        rpc(&server, "document.get", serde_json::json!({})).await["result"]["revision"],
        1
    );
}
```

Add a second test that closes and reopens the same project and asserts the scene status is disabled
in CPU mode and has a fresh target revision after a GPU-enabled reopen. Use the existing
`TestServer` RPC helper patterns. Add `scene_sessions_enabled: bool` to `ServerConfig`, defaulting
to `false` in `for_tests` and reading the host flag in `serve`.

Add a third test using `common::SUCCESS_TOOLCHAIN` and
`common::replace_toolchain_body(&server, common::SUCCESS_TOOLCHAIN)`. After the committed revision,
call `export.runtimePreview` and assert:

- the preview revision is `1`;
- a preview path or packet URL is returned;
- `system.capabilities.compute.sceneSession.state` is `failed` when the scene worker loses its
  device, or `disabled`/`idle` when no adapter is available;
- `document.get` still reports revision `1`.

This test exercises the CPU preview fallback after optional GPU scene failure.

- [ ] **Step 2: Run lifecycle tests and observe scene lifecycle gaps**

Run:

```bash
cargo test -p studio-server --test scene_session_rpc -- --nocapture
```

Expected: the close/reopen or failure-state test fails because scene sessions are not wired into
apply/open/close.

- [ ] **Step 3: Wire every committed revision path**

After a successful `write_snapshot_with_history` in `document.apply`:

1. Build the current CPU scene patch from the existing source index and `result.diff`.
2. Call `ComputeService::apply_scene_patch` only when scene sessions are enabled.
3. Record a scene scheduling failure in compute status and return the successful Document result.
4. Update `SceneSourceIndex` and the cached partial scene only after the CPU patch plan succeeds.
5. Do not wait for GPU completion in `document.apply`.

Store the planned `ScenePatch` as `last_scene_patch` on the session, keyed by target revision. It is
derived metadata and is replaced on the next committed revision; it is never persisted or returned
as Document truth. The preview path uses it to decide which geometry IDs need a bounded readback.

Apply the same scheduling helper to:

- `changeset.commit`;
- `document.undo`;
- `document.redo`.

For undo/redo, construct a `DocumentDiff` by comparing the previous and replacement Documents using
the existing ID collections. Treat a diff that cannot be reconstructed completely as a scene full
rebuild, not as a no-op.

On `project.create`, `project.open`, and successful project replacement:

- close the old scene session;
- reset the layered CPU cache;
- reset the source index and cached scene;
- open a scene session for the new project only when enabled.

After opening the new scene session, evaluate the authoritative Document through the layered cache,
capture `SceneSourceIndex`, build `build_full_scene_patch`, and enqueue that patch. This establishes
the GPU base for later revisions. Opening the project must still succeed if that optional GPU patch
fails; record the fallback status and leave CPU/WebGL2 fully usable.

On `project.close`, close the scene session before marking the session closed.

For transform-only patches, update the cached CPU scene's node matrix/material/visibility without
evaluating or cloning geometry. For attribute/topology patches, evaluate only affected source nodes
and merge their returned nodes and meshes into a new cached `EvaluatedScene`.

- [ ] **Step 4: Run RPC, changeset, and resident tests**

Run:

```bash
cargo test -p studio-server --test scene_session_rpc
cargo test -p studio-server --test rpc_contract
cargo test -p studio-server --test changeset_rpc
cargo test -p studio-server --test resident_scheduler
```

Expected: all pass; a GPU scheduling failure never changes a committed Document result.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-server/src/rpc.rs \
  crates/studio-server/src/changeset.rs \
  crates/studio-server/src/scene_session.rs \
  crates/studio-server/tests/scene_session_rpc.rs
git commit -m "feat(studio-server): schedule scene revisions"
```

## Task 10: Consume Dirty Scene Geometry At Preview Boundaries

**Files:**
- Modify: `crates/studio-core/src/scene_contract.rs`
- Modify: `crates/studio-core/src/gpu_contract.rs`
- Modify: `crates/studio-gpu/src/device.rs`
- Modify: `crates/studio-gpu/src/scene/session.rs`
- Modify: `crates/studio-server/src/compute.rs`
- Modify: `crates/studio-server/src/preview.rs`
- Modify: `crates/studio-server/src/preview_packet.rs`
- Modify: `crates/studio-server/src/rpc.rs`
- Test: `crates/studio-server/tests/scene_preview_packet.rs`

**Interfaces:**
- Consumes: published scene geometry IDs, topology/attribute digests, `ScenePatch`, and existing
  preview packet validation.
- Produces: `SceneGeometrySnapshotRequest`, `SceneGeometryReadback`,
  `GeometryBackend::snapshot_scene_geometries`, `ComputeService::snapshot_scene_geometries`, and
  `build_scene_preview_delta`.

- [ ] **Step 1: Write failing transform-only and attribute readback tests**

```rust
use std::collections::BTreeMap;

use studio_core::document::{Document, Material, NodeContent, SceneNode};
use studio_core::gpu_contract::{GeometryBackendKind, GeometryEvaluationMode};
use studio_core::ids::{MaterialId, NodeId};
use studio_core::large_scene::evaluate::{EvaluatedScene, EvaluatedSceneNode};
use studio_core::math::Transform;
use studio_core::scene_contract::SceneGeometryReadback;
use studio_server::preview_packet::{
    PreviewGeometryNode, PreviewGeometryPacket, build_full_preview_packet,
    build_scene_preview_delta, decode_preview_packet,
};

#[test]
fn transform_only_scene_delta_has_no_geometry_chunk() {
    let base = base_packet();
    let document = document_revision(2);
    let delta = build_scene_preview_delta(
        &base,
        &document,
        vec![node_delta("node-000001", [4.0, 0.0, 0.0, 1.0])],
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );

    assert!(delta.binary.is_empty());
    assert!(delta.manifest.geometries.is_empty());
    assert_eq!(delta.manifest.revision, 2);
    assert_eq!(delta.manifest.parent_revision, Some(1));
}

#[test]
fn attribute_scene_delta_contains_only_the_changed_geometry() {
    let base = base_packet();
    let document = document_revision(2);
    let readback = SceneGeometryReadback {
        geometry_id: "geometry-base".into(),
        positions: vec![[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 2.0, 0.0]],
        indices: vec![0, 1, 2],
        topology_changed: false,
    };
    let delta = build_scene_preview_delta(
        &base,
        &document,
        Vec::new(),
        vec![readback],
        Vec::new(),
        Vec::new(),
    );
    let decoded = decode_preview_packet(
        &studio_server::preview_packet::encode_preview_packet(&delta),
    )
    .unwrap();

    assert_eq!(decoded.manifest.geometries.len(), 1);
    assert_eq!(decoded.manifest.geometries[0].geometry_id, "geometry-base");
    assert_eq!(decoded.manifest.geometries[0].ranges.len(), 1);
    assert_eq!(decoded.binary.len(), 3 * 3 * 4);
}
```

Add these helpers immediately above the tests:

```rust
fn document_revision(revision: u64) -> Document {
    let mut document = Document::new("scene-preview");
    document.revision = studio_core::ids::Revision::new(revision);
    document.materials.push(Material {
        id: MaterialId::from("material-000001"),
        name: "Steel".into(),
        base_color: [0.5, 0.5, 0.5, 1.0],
        opacity: 1.0,
        edge_color: None,
    });
    let mut node = SceneNode::group(NodeId::from("node-000001"), "Primary");
    node.content = NodeContent::Primitive(studio_core::geometry::Primitive::Box {
        width: 1.0,
        height: 1.0,
        depth: 1.0,
    });
    node.material_id = Some(MaterialId::from("material-000001"));
    document.scene.nodes_mut().push(node);
    document
}

fn base_packet() -> PreviewGeometryPacket {
    let document = document_revision(1);
    let mesh = studio_core::geometry::evaluate_node_content(
        &document.scene.nodes()[0].content,
    )
    .unwrap();
    let mut meshes = BTreeMap::new();
    meshes.insert("geometry-base".into(), mesh);
    let scene = EvaluatedScene {
        nodes: vec![EvaluatedSceneNode {
            id: "node-000001".into(),
            owner_id: "node-000001".into(),
            name: "Primary".into(),
            parent_id: None,
            transform: Transform::IDENTITY,
            material_id: Some(MaterialId::from("material-000001")),
            mesh_key: Some("geometry-base".into()),
            resident_geometry_id: None,
            metadata: BTreeMap::new(),
            visible: true,
        }],
        meshes,
        resident_geometry_ids: BTreeMap::new(),
        stats: Default::default(),
    };
    build_full_preview_packet(
        &document,
        &scene,
        GeometryBackendKind::Cpu,
        GeometryEvaluationMode::Resident,
    )
}

fn node_delta(node_id: &str, translation: [f32; 4]) -> PreviewGeometryNode {
    let mut matrix = [0.0; 16];
    matrix[0] = 1.0;
    matrix[5] = 1.0;
    matrix[10] = 1.0;
    matrix[15] = 1.0;
    matrix[12..16].copy_from_slice(&translation);
    PreviewGeometryNode {
        node_id: node_id.into(),
        parent_id: None,
        name: "Primary".into(),
        matrix,
        material_id: Some("material-000001".into()),
        geometry_id: Some("geometry-base".into()),
        visible: true,
    }
}
```

- [ ] **Step 2: Run tests and observe missing scene readback APIs**

Run:

```bash
cargo test -p studio-server --test scene_preview_packet -- --nocapture
```

Expected: compilation fails because `SceneGeometryReadback` and `build_scene_preview_delta` do not
exist.

- [ ] **Step 3: Add bounded readback and direct scene delta assembly**

Define:

```rust
pub struct SceneGeometrySnapshotRequest {
    pub target_revision: u64,
    pub geometry_ids: Vec<String>,
}

pub struct SceneGeometryReadback {
    pub geometry_id: String,
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub topology_changed: bool,
}
```

Add a `snapshot_scene_geometries` default method to `GeometryBackend`. The native implementation
looks up only the requested published geometry result IDs, snapshots only those resident records,
and returns them in request order. Reject IDs that are not in the published scene. Count actual
readback bytes in the result; do not read the whole scene.

Add `build_scene_preview_delta` to `preview_packet.rs`. It:

- keeps `parent_revision = base.manifest.revision`;
- includes only changed nodes and removed IDs supplied by the scene patch;
- includes only the materials supplied by the patch;
- encodes vertex attributes for every requested readback;
- encodes indices only when `topology_changed` is true;
- validates the resulting packet with `decode_preview_packet` before publishing.

For transform/material/visibility-only patches, call this builder with an empty readback vector, so
the packet has no geometry chunk. For attribute/topology patches, request only geometry IDs whose
`SceneGeometryChange.dirty` mask contains attribute or topology channels.

In the preview path, when preview packets and scene sessions are enabled:

1. reuse the CPU cached scene's node metadata and materials,
2. request only the patch's changed geometry IDs from the scene session,
3. build and validate the delta,
4. write it to the existing revision packet path,
5. leave the previous packet in place if readback or validation fails.

If the scene session is unavailable, run the existing CPU full-packet path and record the reason.

- [ ] **Step 4: Run packet, preview, and scene tests**

Run:

```bash
cargo test -p studio-server --test scene_preview_packet
cargo test -p studio-server --test preview_packet
cargo test -p studio-server --test preview_pipeline
cargo test -p studio-gpu --test scene_session --release
```

Expected: all pass; transform-only packets contain zero geometry bytes and failed readback leaves
the previous packet intact.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-core/src/scene_contract.rs \
  crates/studio-core/src/gpu_contract.rs \
  crates/studio-gpu/src/device.rs \
  crates/studio-gpu/src/scene/session.rs \
  crates/studio-server/src/compute.rs \
  crates/studio-server/src/preview.rs \
  crates/studio-server/src/preview_packet.rs \
  crates/studio-server/src/rpc.rs \
  crates/studio-server/tests/scene_preview_packet.rs
git commit -m "feat(studio-server): stream scene preview deltas"
```

## Task 11: Report Scene State In The Viewer

**Files:**
- Modify: `apps/viewer/src/api/StudioClient.ts`
- Modify: `apps/viewer/src/compute/ComputePanel.tsx`
- Modify: `apps/viewer/tests/compute-status.test.ts`
- Modify: generated Viewer schemas and bindings

**Interfaces:**
- Consumes: generated `ComputeStatus.sceneSession`.
- Produces: pending and fallback display with published/target revision and transfer evidence.

- [ ] **Step 1: Write failing Viewer assertions**

Append to `apps/viewer/tests/compute-status.test.ts`:

```ts
import type { ComputeStatus } from '../src/api/StudioClient';

const baseStatus: ComputeStatus = {
  mode: 'auto',
  selectedBackend: 'cpu',
  adapter: 'reference',
  tier: 'cpu',
  memoryBudgetBytes: 2 * 1024 * 1024 * 1024,
  health: 'ready',
  fallbackReason: null,
  queueDepth: 0,
  maxConcurrentJobs: 2,
  gpuThresholdTriangles: 250000,
  lastKernelMs: 0,
  lastUploadBytes: 0,
  lastReadbackBytes: 0,
  lastCacheHits: 0,
  sceneSession: {
    enabled: false,
    sessionId: null,
    state: 'idle',
    publishedRevision: 0,
    targetRevision: 0,
    pendingPatches: 0,
    pendingSubmissions: 0,
    geometryCount: 0,
    topologyCount: 0,
    attributeCount: 0,
    instanceCount: 0,
    residentBytes: 0,
    peakResidentBytes: 0,
    dirtyRangeBytes: 0,
    readbackBytes: 0,
    submissionCount: 0,
    geometryDispatchCount: 0,
    fallbackReason: null,
    phases: {},
  },
};

test('reports pending scene revisions without calling them published', () => {
  const view = formatComputeStatus({
    ...baseStatus,
    selectedBackend: 'gpu',
    sceneSession: {
      enabled: true,
      sessionId: 'scene-1',
      state: 'pending',
      publishedRevision: 8,
      targetRevision: 9,
      pendingPatches: 1,
      pendingSubmissions: 1,
      geometryCount: 233,
      topologyCount: 233,
      attributeCount: 233,
      instanceCount: 1938,
      residentBytes: 4096,
      peakResidentBytes: 8192,
      dirtyRangeBytes: 0,
      readbackBytes: 0,
      submissionCount: 1,
      geometryDispatchCount: 0,
      fallbackReason: null,
      phases: {},
    },
  });

  assert.match(view.detail, /scene pending 8 → 9/);
  assert.match(view.metrics, /1938 instances/);
  assert.match(view.metrics, /dirty 0 B/);
});
```

Add the missing required fields to the existing `baseStatus` test fixture rather than weakening the
new assertion.

- [ ] **Step 2: Run Viewer tests and observe missing status fields**

Run:

```bash
PATH=$HOME/.nvm/versions/node/v24.22.1/bin:$PATH npm --prefix apps/viewer test
```

Expected: TypeScript compilation fails because `sceneSession` is missing from `ComputeStatus`.

- [ ] **Step 3: Add scene status types and formatting**

Add a `SceneSessionStatus` TypeScript type matching the generated Rust contract. Extend
`ComputeStatus` with required `sceneSession`.

In `formatComputeStatus`, retain the existing title and details, then append:

- `scene pending <published> → <target>` for pending;
- `scene ready r<revision>` for idle with a nonzero published revision;
- `scene failed` for failed;
- `scene lost` for lost;
- no scene clause when disabled.

Include geometry, topology, attribute, instance, dirty-range, readback, pending patch, and pending
submission counts in metrics. Use existing `formatBytes` and `formatMillis`; do not add a new
formatting library.

- [ ] **Step 4: Run Viewer checks**

Run:

```bash
PATH=$HOME/.nvm/versions/node/v24.22.1/bin:$PATH npm --prefix apps/viewer test
PATH=$HOME/.nvm/versions/node/v24.22.1/bin:$PATH npm --prefix apps/viewer run typecheck
PATH=$HOME/.nvm/versions/node/v24.22.1/bin:$PATH npm --prefix apps/viewer run build
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add apps/viewer/src/api/StudioClient.ts \
  apps/viewer/src/compute/ComputePanel.tsx \
  apps/viewer/tests/compute-status.test.ts \
  schemas crates/studio-server/bindings
git commit -m "feat(viewer): show pending scene revisions"
```

## Task 12: Add The Large-Scene Revision Benchmark And Gates

**Files:**
- Create: `benchmarks/gpu-scene-session.mjs`
- Create: `scripts/gpu-scene-gates.mjs`
- Modify: `package.json`
- Modify: `scripts/verify.mjs`
- Modify: `README.md`
- Modify: `docs/architecture.md`
- Modify: `docs/reference/api.md`
- Modify: `docs/README.md`
- Test: `scripts/tests/gpu-scene-report.test.mjs`
- Test: `benchmarks/gpu-scene-session.mjs`

**Interfaces:**
- Consumes: JSON-RPC server, `STUDIO_GPU_SCENE_SESSIONS=1`, the large industrial plan fixture.
- Produces: `output/gpu-scene-session-benchmark.json` and a fail-closed
  `npm run benchmark:gpu-scene`.

- [ ] **Step 1: Write failing report-gate tests**

Create `scripts/tests/gpu-scene-report.test.mjs`:

```js
import assert from 'node:assert/strict';
import test from 'node:test';

import { evaluateSceneBenchmarkGate } from '../gpu-scene-gates.mjs';

test('warm transform gate requires fivefold speedup and no readback', () => {
  const result = {
    transform: {
      cpuP50: 1800,
      cpuP95: 2100,
      gpuP50: 360,
      gpuP95: 1800,
      readbackBytes: 0,
      geometryDispatches: 0,
      submissions: 1,
    },
    parameter: { uploadBytes: 4096, fullGeometryBytes: 100000, snapshots: 0 },
    scene: { pendingAfterConsume: 0, publishedRevision: 9, targetRevision: 9 },
  };

  assert.deepEqual(evaluateSceneBenchmarkGate(result), {
    passed: true,
    failures: [],
  });
});

test('cache-hit-only evidence cannot pass the scene gate', () => {
  const result = {
    transform: {
      cpuP50: 1800,
      cpuP95: 2100,
      gpuP50: 1800,
      gpuP95: 2200,
      readbackBytes: 0,
      geometryDispatches: 0,
      submissions: 1,
    },
    parameter: { uploadBytes: 5000, fullGeometryBytes: 100000, snapshots: 1 },
    scene: { pendingAfterConsume: 0, publishedRevision: 9, targetRevision: 9 },
  };

  assert.deepEqual(evaluateSceneBenchmarkGate(result).failures, [
    'transformP50BelowFivefold',
    'transformP95Regression',
    'parameterUploadBelowFivePercent',
    'parameterSnapshotFree',
  ]);
});
```

- [ ] **Step 2: Run the test and observe the missing gate module**

Run:

```bash
PATH=$HOME/.nvm/versions/node/v24.22.1/bin:$PATH node --test scripts/tests/gpu-scene-report.test.mjs
```

Expected: module-not-found failure for `scripts/gpu-scene-gates.mjs`.

- [ ] **Step 3: Implement the benchmark and gate**

Create `scripts/gpu-scene-gates.mjs` with the exported pure function. Apply these exact conditions:

- `transform.gpuP50 <= transform.cpuP50 / 5`;
- `transform.gpuP95 <= transform.cpuP95`;
- `transform.readbackBytes === 0`;
- `transform.geometryDispatches === 0`;
- `transform.submissions === 1`;
- `parameter.uploadBytes < parameter.fullGeometryBytes * 0.05`;
- `parameter.snapshots === 0`;
- `scene.pendingAfterConsume === 0`;
- `scene.publishedRevision === scene.targetRevision`.

Create `benchmarks/gpu-scene-session.mjs` by importing
`buildIndustrialScenePlan()` and running two isolated temporary workspaces:

1. CPU baseline with `--native-gpu cpu`.
2. Experimental GPU scene with `--native-gpu gpu` and
   `STUDIO_GPU_SCENE_SESSIONS=1`.

For each server, apply the same plan, consume one full evaluation, then run exactly nine warm
transform-only revisions against the same secondary-structure target and one parameter revision.
Capture:

- initial scene build wall time;
- wall-clock p50/p95;
- published and target revisions;
- geometry dispatch and submission counts from `system.capabilities.compute.sceneSession`;
- dirty-range and readback bytes;
- one-instance parameter upload bytes and total unique geometry bytes;
- CPU fallback reason.

If no adapter is available, write a skipped report with reason and exit zero only for that
environmental case. If an adapter is present and any gate fails, write the report and exit nonzero.

Add to `package.json`:

```json
"benchmark:gpu-scene": "node benchmarks/gpu-scene-session.mjs"
```

Add the script syntax gate to `scripts/verify.mjs`:

```js
{
  label: 'GPU scene benchmark script',
  command: 'node',
  args: ['--check', 'benchmarks/gpu-scene-session.mjs'],
}
```

- [ ] **Step 4: Run the benchmark and full verification**

Run:

```bash
PATH=$HOME/.nvm/versions/node/v24.22.1/bin:$PATH node --test scripts/tests/gpu-scene-report.test.mjs
PATH=$HOME/.nvm/versions/node/v24.22.1/bin:$PATH npm run benchmark:gpu-scene
PATH=$HOME/.nvm/versions/node/v24.22.1/bin:$PATH npm run verify
```

Expected: gate unit tests pass. The benchmark emits
`output/gpu-scene-session-benchmark.json`; it passes only if the 5× transform and dirty-upload
gates pass. Root verification passes all gates.

After measuring, update the documents truthfully:

- if functional gates pass but the performance gate fails, mark the scene session `Experimental`
  and keep it default-off;
- if all gates pass, keep native GPU automatic selection on CPU because cold execution remains
  separate, but report scene-session evidence and keep WebGL2/WebGPU defaults unchanged;
- include exact measured values, the flag name, and the generated report path;
- document that status `publishedRevision` can lag `targetRevision`.

- [ ] **Step 5: Commit**

```bash
git add benchmarks/gpu-scene-session.mjs \
  scripts/gpu-scene-gates.mjs \
  scripts/tests/gpu-scene-report.test.mjs \
  scripts/verify.mjs \
  package.json README.md docs/architecture.md docs/reference/api.md docs/README.md
git commit -m "perf(studio): measure revision scene residency"
```

## Final Verification

- [ ] Run the complete Rust format and lint gate:

```bash
cargo fmt --all -- --check
cargo clippy -p studio-core -p studio-gpu -p studio-project -p studio-server --all-targets -- -D warnings
```

- [ ] Run all native GPU release tests that exercise this plan:

```bash
cargo test -p studio-gpu --test scene_geometry_update --release
cargo test -p studio-gpu --test scene_session --release
cargo test -p studio-gpu --test resident_evaluation --release
```

- [ ] Run the full repository verification:

```bash
PATH=$HOME/.nvm/versions/node/v24.22.1/bin:$PATH npm run verify
```

- [ ] Run the scene benchmark:

```bash
PATH=$HOME/.nvm/versions/node/v24.22.1/bin:$PATH npm run benchmark:gpu-scene
```

- [ ] Confirm the committed Document revision is unchanged by a simulated GPU scene failure and
  that the canonical source GLB still exports.

- [ ] Confirm `git status --short` contains no unintended Engine changes and every Studio change is
  committed with author `damoqiongqiu`.
