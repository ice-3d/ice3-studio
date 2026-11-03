# GPU-Resident Geometry Implementation Plan

> Document ID: ICE3-STUDIO-PLAN-009
>
> Status: Historical
>
> Owner: ICE3 Studio maintainers

> **历史状态：** 本文保留已完成实施过程的审计记录，不代表当前生产依赖；当前能力以
> [Asset Pipeline](../reference/asset-pipeline.md) 和 [能力清单](../reference/capabilities.md)
> 为准。

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Keep derived geometry resident across native GPU evaluations and browser preview revisions, update only dirty channels, batch GPU work, and preserve CPU-authoritative validation, snapshots, export, and fallback.

**Architecture:** Extend the backend-neutral contract with explicit evaluation modes and opaque resident geometry IDs. Add device-scoped pipelines, pooled buffers, staging slots, and a native residency store in `studio-gpu`; integrate those through `ComputeService`; publish a revision-aware binary preview packet; then teach the browser engine to reuse WebGL2/WebGPU geometry by stable geometry ID and apply dirty ranges atomically.

**Tech Stack:** Rust 1.98, `wgpu` 30.0.1, WGSL, Tokio, serde, JSON Schema, ts-rs, Node.js 24.22.1, TypeScript, WebGL2, WebGPU, Node's built-in test runner, Playwright.

**Spec:** `docs/specs/2026-10-31-gpu-resident-geometry-design.md`

## Global Constraints

- Rust remains the modeling kernel and semantic authority.
- Document, validation, approval, revision, persistence, and canonical source GLB remain CPU-owned.
- GPU residency is replaceable derived state and never the source of truth.
- Native GPU remains experimental until all release performance gates pass.
- The CLI and test-server default remains `--native-gpu cpu`.
- Browser WebGL2 remains the production default; browser WebGPU remains explicitly enabled.
- Node.js is fixed to `24.22.1`.
- Do not use Vite or Vitest.
- Do not add request-controlled shaders, adapter IDs, buffer pointers, paths, or backend commands.
- Host and adapter memory budgets are mandatory before allocation.
- Source GLB remains CPU-generated; preview packets are transport data only.
- Every task follows RED-GREEN-REFACTOR and ends in a focused commit.
- Git author and committer must be `damoqiongqiu`.
- Repository documentation, code, comments, identifiers, examples, and commit messages use neutral
  technical names and contain no competitor or vendor names.
- Existing WebGL2, experimental browser WebGPU, GLB, Toolchain, and current API behavior must not
  regress.
- Current host defaults: two concurrent GPU jobs, 2 GiB host budget capped by adapter budget, and
  effective automatic GPU threshold remains disabled until measured crossover is positive.

## Review Focus

- A parameter-only change with unchanged topology must reuse native adjacency and browser
  topology buffers; a wrong implementation would rebuild both and lose the intended gain.
- A transform-only or material-only change must perform no geometry kernel, readback, or browser
  geometry upload; a wrong implementation would show a new image while silently paying full cost.
- Device loss, cancellation, corrupt packets, and failed partial uploads must retain the previous
  complete revision and release in-flight references; leaking or partially publishing is not
  acceptable.
- Pinned current-revision residency and in-flight buffers must survive budget pressure while
  unpinned LRU entries are evicted; a use-after-free or failed eviction breaks correctness.
- Any unsupported packet, browser backend, or Toolchain-dependent capability must fall back to the
  existing GLB path without changing revision or selection identity.

---

## File Structure

### Native Rust workspace

- `crates/studio-core/src/gpu_contract.rs`: backend-neutral modes, dirty channels, opaque resident
  IDs, resident batch results, snapshot requests, and evidence. No `wgpu` types.
- `crates/studio-core/tests/gpu_residency_contract.rs`: serialization and default-trait behavior.
- `crates/studio-gpu/src/resources/mod.rs`: device-scoped resource facade.
- `crates/studio-gpu/src/resources/pipeline_cache.rs`: persistent kernel pipelines and layouts.
- `crates/studio-gpu/src/resources/buffer_pool.rs`: capacity classes, suballocation, LRU, and safe
  reuse.
- `crates/studio-gpu/src/resources/staging_ring.rs`: bounded readback slots and completion tracking.
- `crates/studio-gpu/src/residency/mod.rs`: native residency facade.
- `crates/studio-gpu/src/residency/key.rs`: topology, attribute, result, and presentation keys.
- `crates/studio-gpu/src/residency/dirty.rs`: channel masks and kernel-specific invalidation.
- `crates/studio-gpu/src/residency/store.rs`: resident entries, pinning, generations, and eviction.
- `crates/studio-gpu/src/kernels/primitives.rs`: kernel execution against persistent resources.
- `crates/studio-gpu/src/kernels/mesh.rs`: custom-mesh resident execution and snapshot modes.
- `crates/studio-gpu/src/device.rs`: device resource initialization and residency integration.
- `crates/studio-server/src/compute.rs`: multi-mode scheduler and evidence.
- `crates/studio-server/src/preview_packet.rs`: packet model, encoder, delta builder, and validator.
- `crates/studio-server/src/rpc.rs`: packet endpoints, capability negotiation, and GLB fallback.

### Browser repository

Browser work occurs in `../ice3-engine` and is committed there separately.

- `packages/assets/src/ICEPreviewGeometryPacket.ts`: binary packet parser and validation.
- `packages/core/src/geometry/ICEGeometry.ts`: stable geometry identity and revision metadata.
- `packages/core/src/geometry/ICEIndexedGeometry.ts`: construction from packet geometry records.
- `packages/renderer/src/WebGL2GeometryCache.ts`: stable-ID lookup and partial range updates.
- `packages/renderer/src/WebGPUGeometryCache.ts`: stable-ID lookup, generation swap, and dirty range
  updates.
- `packages/runtime/src/asset/ICEPreviewPacketLoader.ts`: packet-to-scene publication.
- `packages/ice3/src/index.ts`: public packet loader facade.
- In Studio `apps/viewer/src/viewport/SceneAdapter.ts` and `StudioClient.ts`: packet preference,
  fallback, and evidence.
- In Studio `apps/viewer/src/compute/ComputePanel.tsx`: resident and packet evidence.

---

### Task 1: Extend the Core contract with residency modes

**Files:**
- Modify: `crates/studio-core/src/gpu_contract.rs`
- Test: `crates/studio-core/tests/gpu_residency_contract.rs`

**Interfaces:**
- Consumes: existing `GeometryBatch`, `GeometryBatchResult`, `GeometryExecutionStats`, and
  `GeometryBackend`.
- Produces: `GeometryEvaluationMode`, `GeometryDirtyChannel`, `GeometryDirtyMask`,
  `ResidentGeometryId`, `GeometryResidentEntry`, `GeometryResidentBatchResult`,
  `GeometryEvaluationEvidence`, `GeometrySnapshotRequest`, and default trait methods
  `GeometryBackend::evaluate_resident` and `GeometryBackend::snapshot`.

- [ ] **Step 1: Write failing contract tests**

```rust
use studio_core::gpu_contract::{
    BackendCapabilities, BackendError, GeometryBackend, GeometryBatch, GeometryBatchResult,
    GeometryDirtyChannel, GeometryDirtyMask, GeometryEvaluationMode, GeometryExecutionStats,
};

#[test]
fn dirty_channels_round_trip_as_a_stable_mask() {
    let mut mask = GeometryDirtyMask::empty();
    mask.insert(GeometryDirtyChannel::Positions);
    mask.insert(GeometryDirtyChannel::Normals);
    mask.insert(GeometryDirtyChannel::Bounds);
    assert_eq!(mask.bits(), 0b0001_1010);
    assert!(mask.contains(GeometryDirtyChannel::Normals));
    assert!(!mask.contains(GeometryDirtyChannel::Topology));
    assert_eq!(serde_json::to_string(&GeometryEvaluationMode::Resident).unwrap(), "\"resident\"");
}

#[test]
fn resident_backend_defaults_are_explicitly_unsupported() {
    struct Reference;
    #[async_trait::async_trait]
    impl GeometryBackend for Reference {
        async fn initialize(&self) -> Result<BackendCapabilities, BackendError> {
            panic!("unused")
        }
        async fn evaluate(
            &self,
            _batch: GeometryBatch,
        ) -> Result<GeometryBatchResult, BackendError> {
            panic!("unused")
        }
        async fn cancel(&self, _job_id: &str) -> Result<(), BackendError> {
            Ok(())
        }
        fn stats(&self) -> GeometryExecutionStats {
            Default::default()
        }
    }

    let error = pollster::block_on(Reference.evaluate_resident(
        GeometryBatch {
            job_id: "resident".into(),
            priority: studio_core::gpu_contract::GeometryJobPriority::Interactive,
            budget_bytes: 1024,
            nodes: Vec::new(),
        },
    ))
    .unwrap_err();
    assert!(error.to_string().contains("resident"));
}
```

- [ ] **Step 2: Run the contract tests to verify RED**

Run: `cargo test -p studio-core --test gpu_residency_contract`

Expected: compile failure because the residency types and methods do not exist.

- [ ] **Step 3: Add the backend-neutral types and default methods**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum GeometryEvaluationMode {
    Cpu,
    Cold,
    Warm,
    Resident,
    Snapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum GeometryDirtyChannel {
    Topology,
    Positions,
    Indices,
    Normals,
    Bounds,
    Visibility,
    Material,
    Transform,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeometryDirtyMask(pub u16);

impl GeometryDirtyMask {
    pub const fn empty() -> Self { Self(0) }
    pub fn insert(&mut self, channel: GeometryDirtyChannel) {
        self.0 |= 1 << channel as u16;
    }
    pub fn contains(self, channel: GeometryDirtyChannel) -> bool {
        self.0 & (1 << channel as u16) != 0
    }
    pub const fn bits(self) -> u16 { self.0 }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResidentGeometryId(pub String);

#[derive(Debug, Clone, PartialEq)]
pub struct GeometryResidentEntry {
    pub id: ResidentGeometryId,
    pub node_id: NodeId,
    pub mode: GeometryEvaluationMode,
    pub dirty: GeometryDirtyMask,
    pub bounds: crate::geometry::Bounds,
    pub vertex_count: usize,
    pub triangle_count: usize,
}

#[derive(Debug, Clone)]
pub struct GeometryResidentBatchResult {
    pub job_id: String,
    pub backend: GeometryBackendKind,
    pub entries: Vec<GeometryResidentEntry>,
    pub evidence: GeometryEvaluationEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct GeometryEvaluationEvidence {
    pub mode: GeometryEvaluationMode,
    pub stats: GeometryExecutionStats,
    pub submission_count: usize,
    pub resident_geometries: usize,
    pub resident_bytes: u64,
    pub peak_resident_bytes: u64,
    pub topology_hits: usize,
    pub resident_hits: usize,
    pub evictions: usize,
    pub preview_packet_bytes: u64,
    pub browser_upload_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct GeometrySnapshotRequest {
    pub geometry_ids: Vec<ResidentGeometryId>,
}
```

Add trait defaults that return `BackendError::Incompatible` with reason containing
`"resident geometry is unsupported"` and `"geometry snapshots are unsupported"`.

- [ ] **Step 4: Run focused and full Core tests**

Run: `cargo test -p studio-core --test gpu_residency_contract`

Expected: PASS, `2 passed`.

Run: `cargo test -p studio-core`

Expected: all existing Core tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-core/src/gpu_contract.rs crates/studio-core/tests/gpu_residency_contract.rs \
  crates/studio-core/bindings
git commit -m "feat(studio-core): define resident geometry contract"
```

---

### Task 2: Add persistent pipelines and pooled buffers

**Files:**
- Create: `crates/studio-gpu/src/resources/mod.rs`
- Create: `crates/studio-gpu/src/resources/pipeline_cache.rs`
- Create: `crates/studio-gpu/src/resources/buffer_pool.rs`
- Create: `crates/studio-gpu/src/resources/staging_ring.rs`
- Modify: `crates/studio-gpu/src/lib.rs`
- Modify: `crates/studio-gpu/src/device.rs`
- Test: `crates/studio-gpu/tests/resource_lifecycle.rs`

**Interfaces:**
- Consumes: healthy `wgpu::Device` and `wgpu::Queue` from the existing native backend lifecycle.
- Produces: `DeviceResources::create(device, queue, limits)`,
  `PipelineKey::{Box, Plane, Cylinder, Sphere, Sweep, MeshFaceNormals, MeshNormals, MeshBounds}`,
  `PipelineCache`,
  `BufferClass::for_bytes(u64)`, `BufferPool::{acquire, release, total_bytes}`, and
  `StagingRing::{acquire, complete, ready_count}`,
  `NativeGpuBackend::{resource_stats, warm_pipelines_for_test}`.

- [ ] **Step 1: Write failing lifecycle tests**

```rust
use studio_gpu::{BufferClass, NativeGpuBackend};

#[test]
fn capacity_classes_round_up_without_unbounded_allocations() {
    assert_eq!(BufferClass::for_bytes(1), BufferClass::OneMiB);
    assert_eq!(BufferClass::for_bytes(1_048_577), BufferClass::FourMiB);
    assert_eq!(
        BufferClass::for_bytes(80 * 1024 * 1024),
        BufferClass::TwoHundredFiftySixMiB,
    );
}

#[test]
fn repeated_device_evaluations_reuse_pipelines_and_buffers() {
    let Some(backend) = NativeGpuBackend::for_test_gpu() else { return };
    let first = backend.resource_stats();
    for _ in 0..3 {
        backend.warm_pipelines_for_test().unwrap();
    }
    let second = backend.resource_stats();
    assert_eq!(first.pipeline_count, second.pipeline_count);
    assert_eq!(first.buffer_allocation_count, second.buffer_allocation_count);
}
```

- [ ] **Step 2: Run to verify RED**

Run: `cargo test -p studio-gpu --test resource_lifecycle`

Expected: compile failure for missing resource types.

- [ ] **Step 3: Implement explicit pipelines**

Create `PipelineCache` with a `BTreeMap<PipelineKey, Arc<ComputePipeline>>` behind `RwLock`.
`DeviceResources::create` compiles Box/Plane, Cylinder/Sphere/Sweep, face-normal, normal
accumulation, and bounds pipelines once. All kernels accept `&PipelineCache` instead of creating a
pipeline internally.

Expose test-only counters through:

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResourceStats {
    pub pipeline_count: usize,
    pub buffer_allocation_count: usize,
    pub pooled_bytes: u64,
    pub peak_resident_bytes: u64,
}
```

- [ ] **Step 4: Implement bounded buffer pooling**

`BufferPool` allocates only in the capacity classes from the spec. Each lease records role,
capacity, offset, generation, and in-flight count. `release` returns a zero-user lease to the
matching class; non-zero leases enter a delayed-release queue. The pool must reject a lease if the
requested bytes exceed the effective host/adapter cap.

- [ ] **Step 5: Implement the staging ring**

`StagingRing` starts with three host-visible slots. `acquire` rejects when all slots are busy.
`complete(submission_index)` releases slots whose GPU work is complete. No resident evaluation may
acquire a staging slot.

- [ ] **Step 6: Wire resources into device discovery**

`CapabilityProbe` owns `Arc<DeviceResources>`. `NativeGpuBackend` stores that same instance in
`DeviceState`. Re-initialization and device loss replace the whole resource set.

- [ ] **Step 7: Run focused and full tests**

Run: `cargo test -p studio-gpu --test resource_lifecycle`

Expected: PASS.

Run: `cargo test -p studio-gpu`

Expected: all parity tests pass; exact allocations are unchanged.

- [ ] **Step 8: Commit**

```bash
git add crates/studio-gpu/src/resources crates/studio-gpu/src/device.rs crates/studio-gpu/src/lib.rs \
  crates/studio-gpu/tests/resource_lifecycle.rs
git commit -m "feat(studio-gpu): pool native device resources"
```

---

### Task 3: Implement residency keys and dirty-channel planning

**Files:**
- Create: `crates/studio-gpu/src/residency/mod.rs`
- Create: `crates/studio-gpu/src/residency/key.rs`
- Create: `crates/studio-gpu/src/residency/dirty.rs`
- Modify: `crates/studio-gpu/src/lib.rs`
- Test: `crates/studio-gpu/tests/residency_keys.rs`

**Interfaces:**
- Consumes: `NodeContent`, `GeometryDirtyMask`, and content/dependency digests.
- Produces: `ResidencyKey`, `DirtyPlan::for_transition(previous, next)`, and
  `DirtyPlan::requires_topology_rebuild()`.

- [ ] **Step 1: Write failing dirty-channel tests**

```rust
use studio_core::document::NodeContent;
use studio_core::geometry::Primitive;
use studio_core::gpu_contract::{GeometryDirtyChannel, GeometryDirtyMask};
use studio_gpu::DirtyPlan;

#[test]
fn box_dimension_change_reuses_topology_and_normals() {
    let before = NodeContent::Primitive(Primitive::Box {
        width: 1.0, height: 1.0, depth: 1.0,
    });
    let after = NodeContent::Primitive(Primitive::Box {
        width: 3.0, height: 1.0, depth: 1.0,
    });
    let plan = DirtyPlan::for_transition(Some(&before), &after);
    assert!(!plan.dirty.contains(GeometryDirtyChannel::Topology));
    assert!(plan.dirty.contains(GeometryDirtyChannel::Positions));
    assert!(!plan.dirty.contains(GeometryDirtyChannel::Normals));
    assert!(plan.dirty.contains(GeometryDirtyChannel::Bounds));
}

#[test]
fn cylinder_segment_change_rebuilds_topology() {
    let before = NodeContent::Primitive(Primitive::Cylinder {
        radius: 1.0, height: 2.0, segments: 16,
    });
    let after = NodeContent::Primitive(Primitive::Cylinder {
        radius: 1.0, height: 2.0, segments: 32,
    });
    assert!(DirtyPlan::for_transition(Some(&before), &after)
        .requires_topology_rebuild());
}
```

- [ ] **Step 2: Run to verify RED**

Run: `cargo test -p studio-gpu --test residency_keys`

Expected: compile failure for missing `DirtyPlan`.

- [ ] **Step 3: Implement deterministic hashing**

Use SHA-256 over versioned serde tuples. Define:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResidencyKey {
    pub topology: String,
    pub attribute: String,
    pub result: String,
}
```

No hash may include addresses, absolute paths, adapter serials, or timestamps.

- [ ] **Step 4: Implement kernel transition rules**

Encode the invalidation matrix from the spec in typed match arms. Unknown transitions mark all
geometry channels dirty. Transform-only transitions set `Transform` and `Bounds`; material-only
transitions set `Material` only.

- [ ] **Step 5: Verify**

Run: `cargo test -p studio-gpu --test residency_keys`

Expected: PASS, including all primitive families.

Run: `cargo clippy -p studio-gpu --all-targets -- -D warnings`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/studio-gpu/src/residency crates/studio-gpu/src/lib.rs \
  crates/studio-gpu/tests/residency_keys.rs
git commit -m "feat(studio-gpu): plan resident geometry invalidation"
```

---

### Task 4: Implement native residency store and resident evaluation

**Files:**
- Create: `crates/studio-gpu/src/residency/store.rs`
- Modify: `crates/studio-gpu/src/residency/mod.rs`
- Modify: `crates/studio-gpu/src/kernels/primitives.rs`
- Modify: `crates/studio-gpu/src/kernels/mesh.rs`
- Modify: `crates/studio-gpu/src/device.rs`
- Test: `crates/studio-gpu/tests/resident_evaluation.rs`

**Interfaces:**
- Consumes: resource manager from Task 2, keys/dirty planner from Task 3, and current kernels.
- Produces: `GpuGeometryResidency`, `ResidentGeometryRecord`, resident evaluation methods on
  `NativeGpuBackend`, `NativeGpuBackend::resident_stats() -> GeometryEvaluationEvidence`, and
  explicit `snapshot_geometry`.
- Produces exact methods
  `evaluate_custom_mesh_resident(&TriangleMesh) -> Result<GeometryResidentEntry, BackendError>`
  and
  `snapshot_geometry(&ResidentGeometryId) -> Result<EvaluatedMesh, BackendError>`.

- [ ] **Step 1: Write failing residency tests**

```rust
use studio_core::gpu_contract::GeometryEvaluationMode;

#[test]
fn second_evaluation_reuses_topology_without_readback() {
    let Some(backend) = NativeGpuBackend::for_test_gpu() else { return };
    let mesh = synthetic_mesh(500_000);
    let first = backend.evaluate_custom_mesh_resident(&mesh).unwrap();
    let second = backend.evaluate_custom_mesh_resident(&mesh).unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(second.mode, GeometryEvaluationMode::Resident);
    assert_eq!(second.evidence.mode, GeometryEvaluationMode::Resident);
    assert_eq!(second.evidence.stats.readback_bytes, 0);
    assert!(second.evidence.resident_hits >= 1);
}

#[test]
fn snapshot_is_the_only_custom_mesh_path_that_reads_back() {
    let Some(backend) = NativeGpuBackend::for_test_gpu() else { return };
    let mesh = synthetic_mesh(50_000);
    let resident = backend.evaluate_custom_mesh_resident(&mesh).unwrap();
    let before = backend.resident_stats().stats.readback_bytes;
    let snapshot = backend.snapshot_geometry(&resident.id).unwrap();
    assert_eq!(snapshot.vertices.len(), mesh.positions.len());
    assert!(
        backend.resident_stats().stats.readback_bytes - before
            >= snapshot.vertices.len() as u64 * 16
    );
}
```

- [ ] **Step 2: Run to verify RED**

Run: `cargo test -p studio-gpu --test resident_evaluation`

Expected: missing resident APIs.

- [ ] **Step 3: Implement the store**

`GpuGeometryResidency` uses `RwLock<BTreeMap<String, Arc<ResidentGeometryRecord>>>`. An entry stores
keys, generation, topology CSR, typed buffer leases, output buffers, bounds, counts, pinned
references, last-used generation, and cached evidence. Pin/unpin and eviction use RAII guards.

- [ ] **Step 4: Refactor kernels to resident workspaces**

Primitive kernels write into existing output leases when topology/counts match. Custom meshes reuse
positions, indices, offsets, incidence, face normals, normals, and bounds buffers when their keys
match. A topology change destroys the old generation only after a replacement succeeds.

- [ ] **Step 5: Separate resident completion from snapshots**

Resident evaluation returns opaque IDs and evidence without mapping a staging buffer. Snapshot maps
the requested geometry through Task 2's staging ring, validates parity, and returns
`EvaluatedMesh`.

- [ ] **Step 6: Verify**

Run: `cargo test -p studio-gpu --test resident_evaluation --release`

Expected: PASS; second evaluation reports zero readback.

Run: `cargo test -p studio-gpu`

Expected: all CPU parity and legacy tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/studio-gpu/src/residency crates/studio-gpu/src/kernels \
  crates/studio-gpu/src/device.rs crates/studio-gpu/tests/resident_evaluation.rs
git commit -m "feat(studio-gpu): keep derived geometry resident"
```

---

### Task 5: Batch submissions and add asynchronous completion

**Files:**
- Create: `crates/studio-gpu/src/residency/batch.rs`
- Modify: `crates/studio-gpu/src/device.rs`
- Modify: `crates/studio-gpu/src/residency/mod.rs`
- Test: `crates/studio-gpu/tests/resident_batch.rs`

**Interfaces:**
- Consumes: resident records and device resources.
- Produces: `ResidentBatchPlan`, `ResidentBatchSubmission`,
  `NativeGpuBackend::poll_resident`,
  `NativeGpuBackend::wait_resident`,
  `NativeGpuBackend::evaluate_resident(GeometryBatch) ->
  Result<GeometryResidentBatchResult, BackendError>`, and cancellation checkpoints.

- [ ] **Step 1: Write failing batch tests**

```rust
#[test]
fn compatible_nodes_share_one_submission_and_one_completion() {
    let Some(backend) = NativeGpuBackend::for_test_gpu() else { return };
    let batch = box_batch(32);
    let result = backend.evaluate_resident(batch).unwrap();
    assert_eq!(result.entries.len(), 32);
    assert_eq!(result.evidence.submission_count, 1);
    assert_eq!(result.evidence.stats.readback_bytes, 0);
}

#[test]
fn cancelled_background_batch_never_publishes_entries() {
    let Some(backend) = NativeGpuBackend::for_test_gpu() else { return };
    backend.cancel("background-batch").unwrap();
    let result = backend.evaluate_resident(background_batch("background-batch"));
    assert!(result.is_err());
}
```

- [ ] **Step 2: Run to verify RED**

Run: `cargo test -p studio-gpu --test resident_batch`

Expected: missing submission and cancellation behavior.

- [ ] **Step 3: Implement batch planning and one encoded submission**

Group nodes by kernel family and compatible layout. Encode all passes into one
`CommandEncoder`. Submit once and return a submission handle. Publish resident entries only after
the completion worker observes success for the whole submission.

- [ ] **Step 4: Implement polling and cancellation**

`poll_resident` checks completion without blocking. `wait_resident` blocks a dedicated worker, not
the async request loop. Cancellation marks queued jobs cancelled before encoding and checked jobs
cancelled before publication.

- [ ] **Step 5: Verify**

Run: `cargo test -p studio-gpu --test resident_batch`

Expected: PASS.

Run: `cargo test -p studio-gpu`

Expected: all existing tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/studio-gpu/src/residency crates/studio-gpu/src/device.rs \
  crates/studio-gpu/tests/resident_batch.rs
git commit -m "feat(studio-gpu): batch resident geometry submissions"
```

---

### Task 6: Integrate resident modes and evidence into the server

**Files:**
- Modify: `crates/studio-server/src/compute.rs`
- Modify: `crates/studio-server/src/rpc.rs`
- Modify: `crates/studio-core/src/large_scene/evaluate.rs`
- Modify: `crates/studio-server/tests/compute_scheduler.rs`
- Test: `crates/studio-server/tests/resident_scheduler.rs`

**Interfaces:**
- Consumes: resident batch APIs and snapshot requests from Tasks 4–5.
- Produces: `GeometryEvaluationMode` evidence, `SnapshotPolicy`, `residentGeometry` capability
  fields, and source-specific snapshot behavior.
- Produces `GeometryEvaluationPurpose::{Preview, Candidate, SourceExport, Diagnostics}` and exact
  `ComputeService` methods `evaluate_transform_only`, `evaluate_for`, and `snapshot_for`.

- [ ] **Step 1: Write failing scheduler tests**

```rust
#[tokio::test]
async fn transform_only_change_performs_no_geometry_work() {
    let gpu = FakeGpu::new(FakeOutcome::Success);
    let service = resident_service(gpu.clone());
    service.evaluate(box_content_named("shared")).await.unwrap();
    service
        .evaluate_transform_only("shared", Transform::IDENTITY)
        .await
        .unwrap();
    assert_eq!(gpu.evaluate_calls(), 1);
    assert_eq!(gpu.snapshot_calls(), 0);
}

#[tokio::test]
async fn export_requests_snapshot_while_preview_uses_residency() {
    let service = resident_service(FakeGpu::new(FakeOutcome::Success));
    service.evaluate(box_content()).await.unwrap();
    service
        .evaluate_for(GeometryEvaluationPurpose::Preview)
        .await
        .unwrap();
    assert_eq!(service.status().last_evaluation_mode, "resident");
    service
        .snapshot_for(GeometryEvaluationPurpose::SourceExport)
        .await
        .unwrap();
    assert_eq!(service.status().last_evaluation_mode, "snapshot");
}
```

- [ ] **Step 2: Run to verify RED**

Run: `cargo test -p studio-server --test resident_scheduler`

Expected: compile failure for missing modes and purposes.

- [ ] **Step 3: Add scheduler mode selection**

Extend `ComputePolicy` with `resident_enabled` and `preview_packet_enabled`, both false by default
until measured gates pass. Mode selection uses cold/warm/resident evidence plus health, budget, and
priority. Snapshot policy is determined by API purpose, not by request-controlled backend flags.

- [ ] **Step 4: Extend large-scene evaluation**

Large-scene expansion stores opaque resident IDs in evaluation metadata alongside mesh keys.
Transform-only nodes update node matrices without re-evaluating part geometry. Material-only
changes update materials without invoking the geometry evaluator.

- [ ] **Step 5: Expand capabilities and diagnostics**

Add resident counts, bytes, mode, packet version, pipeline warm-up state, topology hits, and eviction
counters. Device loss invalidates resident IDs and reports
`ICE3_STUDIO_GPU_DEVICE_LOST` without altering committed revision.

- [ ] **Step 6: Verify**

Run: `cargo test -p studio-server --test resident_scheduler`

Expected: PASS.

Run: `cargo test -p studio-server`

Expected: all existing scheduler, ChangeSet, Toolchain, and RPC tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/studio-server/src/compute.rs crates/studio-server/src/rpc.rs \
  crates/studio-core/src/large_scene/evaluate.rs \
  crates/studio-server/tests/compute_scheduler.rs \
  crates/studio-server/tests/resident_scheduler.rs crates/studio-server/bindings
git commit -m "feat(studio-server): schedule resident geometry modes"
```

---

### Task 7: Encode and serve revision-aware preview packets

**Files:**
- Create: `crates/studio-server/src/preview_packet.rs`
- Modify: `crates/studio-server/src/lib.rs`
- Modify: `crates/studio-server/src/rpc.rs`
- Modify: `crates/studio-server/src/preview.rs`
- Test: `crates/studio-server/tests/preview_packet.rs`

**Interfaces:**
- Consumes: resident IDs and snapshot policy from Task 6.
- Produces: `PreviewGeometryPacket`,
  `PreviewGeometryManifest`, `PreviewGeometryRecord`, `encode_preview_packet`,
  `decode_preview_packet`, `build_preview_delta`, `/preview-packets/{revision}`, and
  `/preview-packets/candidates/{digest}`.

- [ ] **Step 1: Write failing packet tests**

```rust
#[test]
fn packet_round_trip_preserves_dirty_ranges_and_geometry_identity() {
    let packet = sample_packet();
    let bytes = encode_preview_packet(&packet);
    let decoded = decode_preview_packet(&bytes).unwrap();
    assert_eq!(decoded.manifest.revision, 4);
    assert_eq!(decoded.manifest.geometries[0].geometry_id, "component/body");
    assert_eq!(decoded.manifest.geometries[0].ranges[0].byte_length, 48);
}

#[test]
fn transform_only_delta_contains_no_geometry_chunk() {
    let base = sample_packet();
    let next = transform_only_successor(&base);
    let delta = build_preview_delta(&base, &next);
    assert_eq!(delta.binary.len(), 0);
    assert_eq!(delta.manifest.geometries.len(), 0);
    assert_eq!(delta.manifest.nodes.len(), 1);
}

#[test]
fn invalid_ranges_are_rejected_before_upload() {
    let mut bytes = encode_preview_packet(&sample_packet());
    bytes[40..48].copy_from_slice(&u64::MAX.to_le_bytes());
    assert!(decode_preview_packet(&bytes).is_err());
}
```

- [ ] **Step 2: Run to verify RED**

Run: `cargo test -p studio-server --test preview_packet`

Expected: missing module and packet endpoints.

- [ ] **Step 3: Implement packet encoding**

Write the `ICE3PGP1` header, aligned JSON manifest, and binary chunk. Validate counts, alignment,
digest, ranges, finite metadata, and a default 256 MiB transport cap before allocation.

- [ ] **Step 4: Implement full and delta packet generation**

Use stable node and geometry IDs. Full packets contain create records and payloads. Deltas contain
only changed nodes, materials, dirty ranges, replacements, and explicit removals. A transform-only
delta has an empty binary chunk.

- [ ] **Step 5: Add endpoints and negotiation**

Advertise `previewGeometryPacket: "1"` in compute capabilities. Return a `packetUrl` beside the GLB
preview URL when enabled. Packet endpoints require existing local authentication when writes or
temporary authorization are involved and serve a versioned local media type.

- [ ] **Step 6: Verify**

Run: `cargo test -p studio-server --test preview_packet`

Expected: PASS.

Run: `cargo test -p studio-server`

Expected: all existing preview and GLB tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/studio-server/src/preview_packet.rs crates/studio-server/src/preview.rs \
  crates/studio-server/src/rpc.rs crates/studio-server/src/lib.rs \
  crates/studio-server/tests/preview_packet.rs crates/studio-server/bindings
git commit -m "feat(studio-server): publish incremental preview packets"
```

---

### Task 8: Parse geometry packets and preserve identity in the browser core

**Files:**
- Create: `../ice3-engine/packages/assets/src/ICEPreviewGeometryPacket.ts`
- Modify: `../ice3-engine/packages/assets/src/index.ts`
- Modify: `../ice3-engine/packages/core/src/geometry/ICEGeometry.ts`
- Modify: `../ice3-engine/packages/core/src/geometry/ICEIndexedGeometry.ts`
- Modify: `../ice3-engine/packages/runtime/src/asset/ICEAssetSceneBuilder.ts`
- Test: `../ice3-engine/packages/assets/tests/PreviewGeometryPacket.test.ts`
- Create: `../ice3-engine/packages/assets/tests/fixtures/previewGeometry.ts`
- Test: `../ice3-engine/packages/core/tests/GeometryIdentity.test.ts`

**Interfaces:**
- Consumes: `ICE3PGP1` bytes from Task 7.
- Produces: `ICEPreviewGeometryPacket`, `ICEPreviewGeometryDelta`,
  `parsePreviewGeometryPacket`, `applyPreviewGeometryDelta`, and optional
  `ICEGeometryIdentity { geometryId, topologyDigest, attributeDigest }`.
- Produces: `ICEGeometryBufferUpdate { positionRanges }` and
  `ICEGeometryByteRange { byteOffset, byteLength }`.
- Produces test fixtures `fullPacketFixture()`, `transformOnlyDeltaFixture()`, and
  `attributeDeltaFixture()` in the fixture file above.

- [ ] **Step 1: Write failing parser and identity tests**

```ts
it('applies a transform-only delta without geometry payload', () => {
  const base = parsePreviewGeometryPacket(fullPacketFixture());
  const next = applyPreviewGeometryDelta(base, transformOnlyDeltaFixture());
  expect(next.geometries).toEqual(base.geometries);
  expect(next.nodes[0]?.matrix[12]).toBe(4);
});

it('preserves stable geometry identity across packet revisions', () => {
  const base = parsePreviewGeometryPacket(fullPacketFixture());
  const next = applyPreviewGeometryDelta(base, attributeDeltaFixture());
  expect(next.geometries[0]?.geometryId).toBe(base.geometries[0]?.geometryId);
  expect(next.geometries[0]?.attributeDigest).not.toBe(
    base.geometries[0]?.attributeDigest,
  );
});
```

- [ ] **Step 2: Run to verify RED**

Run in `../ice3-engine`: `npm test -- --test-name-pattern PreviewGeometryPacket`

Expected: parser module not found.

- [ ] **Step 3: Implement parser and validation**

Validate magic, versions, manifest length, binary length, count caps, index widths, ranges,
digests, and finite metadata before creating typed arrays. Reject a delta whose parent revision does
not match the current packet.

- [ ] **Step 4: Add geometry identity metadata**

Add optional immutable identity fields to `ICEGeometry` and `ICEIndexedGeometry`. Existing
constructors remain valid with identity defaulting from `id` for compatibility.

- [ ] **Step 5: Implement packet-to-scene conversion**

Create or reuse `ICEIndexedGeometry` objects by `geometryId`. Reuse unchanged arrays; create new
typed views only for dirty ranges or replacements.

- [ ] **Step 6: Verify**

Run in `../ice3-engine`: `npm test`

Expected: all browser package tests pass.

Run in `../ice3-engine`: `npm run typecheck`

Expected: PASS.

- [ ] **Step 7: Commit in the browser repository**

```bash
git add packages/assets/src/ICEPreviewGeometryPacket.ts packages/assets/src/index.ts \
  packages/assets/tests/PreviewGeometryPacket.test.ts \
  packages/core/src/geometry/ICEGeometry.ts \
  packages/core/src/geometry/ICEIndexedGeometry.ts \
  packages/core/tests/GeometryIdentity.test.ts packages/runtime/src/asset/ICEAssetSceneBuilder.ts
git commit -m "feat(engine): parse resident geometry packets"
```

---

### Task 9: Reuse and partially update browser GPU geometry

**Files:**
- Modify: `../ice3-engine/packages/renderer/src/WebGL2GeometryCache.ts`
- Modify: `../ice3-engine/packages/renderer/src/WebGPUGeometryCache.ts`
- Create: `../ice3-engine/packages/runtime/src/asset/ICEPreviewPacketLoader.ts`
- Modify: `../ice3-engine/packages/runtime/src/index.ts`
- Modify: `../ice3-engine/packages/ice3/src/index.ts`
- Test: `../ice3-engine/packages/renderer/tests/GeometryResidency.test.ts`
- Test: `../ice3-engine/packages/runtime/tests/PreviewPacketLoader.test.ts`
- Modify: `apps/viewer/src/viewport/SceneAdapter.ts`
- Modify: `apps/viewer/src/api/StudioClient.ts`
- Test: `apps/viewer/tests/preview-packet-client.test.ts`

**Interfaces:**
- Consumes: packet parser and stable identity from Task 8.
- Produces: `ICEPreviewPacketLoader`, `SceneAdapter.showPreview(preview)`,
  `geometryResidency.uploadCount`, `geometryResidency.updateBytes`,
  `pin(geometry, update?: ICEGeometryBufferUpdate)`,
  cache-entry `attributeDigest`, and GLB fallback.
- Produces `ICEPreviewPacketLoader.load(url, current?) ->
  Promise<{ kind: 'packet'; packet } | { kind: 'fallback'; reason: string }>`.

- [ ] **Step 1: Write failing cache behavior tests**

```ts
import { ICEIndexedGeometry } from '@ice3/core';

it('does not upload or recreate buffers for a transform-only revision', () => {
  const cache = new WebGPUGeometryCache({ device });
  const first = new ICEIndexedGeometry({
    id: 'shared',
    positions: new Float32Array([0, 0, 0, 1, 0, 0, 0, 1, 0]),
    indices: new Uint16Array([0, 1, 2]),
  });
  cache.pin(first);
  const before = cache.uploadCount;
  const revision = new ICEIndexedGeometry({
    id: first.id,
    positions: first.positions,
    indices: first.indices,
  });
  cache.pin(revision);
  expect(cache.uploadCount).toBe(before);
  expect(cache.updateBytes).toBe(0);
});

it('updates only changed position ranges', () => {
  const cache = new WebGL2GeometryCache();
  const first = new ICEIndexedGeometry({
    id: 'shared',
    positions: new Float32Array([0, 0, 0, 1, 0, 0, 0, 1, 0]),
    indices: new Uint16Array([0, 1, 2]),
  });
  const revision = new ICEIndexedGeometry({
    id: first.id,
    positions: new Float32Array([0, 0, 0, 2, 0, 0, 0, 1, 0]),
    indices: first.indices,
  });
  cache.pin(gl, first);
  const before = cache.updateBytes;
  cache.pin(gl, revision, {
    positionRanges: [{ byteOffset: 12, byteLength: 12 }],
  });
  expect(cache.updateBytes - before).toBe(12);
});

it('keeps the previous generation when a partial write fails', () => {
  const cache = new WebGL2GeometryCache();
  const first = triangleGeometry('shared', 1);
  const revision = triangleGeometry('shared', 2);
  cache.pin(gl, first);
  expect(() => cache.pin(failingWebGL2(), revision, {
    positionRanges: [{ byteOffset: 12, byteLength: 12 }],
  })).toThrow(/ICE3_GPU_RESOURCE_FAILED/);
  expect(cache.retain(gl, first).attributeDigest).toBe(first.attributeDigest);
});

it('reports GLB fallback when a packet is malformed', async () => {
  const loader = new ICEPreviewPacketLoader(fetchReturning(new Uint8Array([1, 2, 3])));
  const result = await loader.load('/preview-packets/4');
  expect(result).toEqual({ kind: 'fallback', reason: 'packet-invalid' });
});
```

`triangleGeometry` and `failingWebGL2` are local helpers in `GeometryResidency.test.ts`; they
construct the literal three-vertex geometry above and wrap `fakeWebGL2()` with a
`bufferSubData` method that throws. `fetchReturning` is a local helper in
`PreviewPacketLoader.test.ts` that returns a successful response with the supplied bytes.

- [ ] **Step 2: Run to verify RED**

Run in `../ice3-engine`: `npm test -- --test-name-pattern GeometryResidency`

Expected: stable-ID reuse and range update APIs do not exist.

- [ ] **Step 3: Key browser caches by geometry identity**

Use `geometryId` as the stable key and retain `topologyDigest`. Reuse the current entry when only
attributes change. Allocate a replacement generation when topology or index format changes, then
swap atomically after upload succeeds. For legacy geometry without `ICEGeometryIdentity`, use the
concrete indexed-geometry `id`; if neither identity exists, retain the previous object-identity
behavior.

- [ ] **Step 4: Implement partial updates**

WebGL2 uses `bufferSubData` for position/color ranges. WebGPU packs dirty vertex ranges into the
existing interleaved layout and uses `queue.writeBuffer`; it still avoids any write for
transform-only changes. Expose uploaded and updated bytes in frame stats.

- [ ] **Step 5: Add the public packet loader**

`ICEPreviewPacketLoader.load(url, current?)` fetches, parses, applies a delta, builds or reuses the
scene, pins changed geometries, and publishes atomically. It exposes a typed fallback result if the
packet is unsupported or invalid.

- [ ] **Step 6: Integrate Workbench**

`StudioClient.preview` prefers `packetUrl` when advertised. `SceneAdapter.showPreview` uses the
packet loader when available and falls back to `ICEAssetLoader` GLB loading. Preserve selection IDs,
camera behavior, and disposal semantics.

- [ ] **Step 7: Verify**

Run in `../ice3-engine`: `npm run verify`

Expected: PASS.

Run in the Studio worktree: `npm --prefix apps/viewer test`

Expected: all unit tests pass.

Run in the Studio worktree: `npm --prefix apps/viewer run typecheck`

Expected: PASS.

- [ ] **Step 8: Commit**

Commit browser changes in `../ice3-engine`:

```bash
git add packages/renderer/src/WebGL2GeometryCache.ts \
  packages/renderer/src/WebGPUGeometryCache.ts \
  packages/runtime/src/asset/ICEPreviewPacketLoader.ts \
  packages/runtime/src/index.ts packages/ice3/src/index.ts \
  packages/renderer/tests/GeometryResidency.test.ts \
  packages/runtime/tests/PreviewPacketLoader.test.ts
git commit -m "feat(engine): reuse resident browser geometry"
```

Commit Studio changes separately:

```bash
git add apps/viewer/src/viewport/SceneAdapter.ts apps/viewer/src/api/StudioClient.ts \
  apps/viewer/tests/preview-packet-client.test.ts
git commit -m "feat(studio-workbench): consume resident preview packets"
```

---

### Task 10: Add mode-specific evidence, benchmarks, and final documentation

**Files:**
- Modify: `benchmarks/native-gpu.mjs`
- Modify: `crates/studio-gpu/tests/native_gpu_benchmark.rs`
- Modify: `apps/viewer/src/compute/ComputePanel.tsx`
- Modify: `apps/viewer/tests/compute-status.test.ts`
- Modify: `docs/specs/2026-10-31-gpu-resident-geometry-design.md`
- Modify: `docs/specs/2026-10-30-native-gpu-compute-design.md`
- Modify: `docs/architecture.md`
- Modify: `docs/reference/api.md`
- Modify: `docs/README.md`
- Modify: `README.md`

**Interfaces:**
- Consumes: all prior tasks.
- Produces: machine-readable cold/warm/resident/snapshot/preview evidence and final capability
  documentation.

- [ ] **Step 1: Write failing evidence tests**

```ts
test('reports resident mode and partial browser upload evidence', () => {
  const view = formatComputeStatus({
    ...readyStatus,
    lastEvaluationMode: 'resident',
    residentBytes: 1234,
    lastUploadBytes: 0,
    lastReadbackBytes: 0,
    lastCacheHits: 8,
    previewPacketBytes: 512,
    browserUploadBytes: 0,
  });
  expect(view.detail).toContain('resident');
  expect(view.detail).toContain('readback 0');
  expect(view.metrics).toContain('packet 512 B');
});
```

```rust
#[test]
fn benchmark_report_contains_every_evaluation_mode() {
    let report = benchmark_report();
    for mode in ["cold", "warm", "resident", "snapshot", "preview"] {
        assert!(report["workloads"][0]["modes"].get(mode).is_some(), "{mode}");
    }
}
```

- [ ] **Step 2: Run to verify RED**

Run: `cargo test -p studio-gpu --test native_gpu_benchmark --no-run`

Expected: missing mode-specific benchmark API.

Run in the Studio worktree: `npm --prefix apps/viewer test`

Expected: compute formatter lacks residency metrics.

- [ ] **Step 3: Extend compute status and benchmark**

Report mode, resident bytes, peak bytes, topology hits, resident hits, evictions, packet bytes,
browser upload bytes, and snapshot bytes. Benchmark cold, warm-topology, resident, snapshot, and
preview for 50k, 500k, and 2M meshes.

- [ ] **Step 4: Enforce release gates**

Keep `benchmark:gpu` fail-closed. Resident 500k/2M p50 must improve at least 10%; GPU p95 must be
within 105% of CPU; transform-only preview must upload zero geometry; single-parameter deltas must
be smaller than full packets.

- [ ] **Step 5: Run all functional and performance gates**

Run in the Studio worktree:

```bash
source $HOME/.nvm/nvm.sh && nvm use 24.22.1 >/dev/null
npm run verify
npm run benchmark
npm run benchmark:large-industrial
npm run benchmark:gpu
```

Expected: functional gates pass. Performance commands pass only when their documented gates pass;
otherwise the command exits nonzero and leaves Auto on CPU.

- [ ] **Step 6: Verify browser repository**

Run in `../ice3-engine`:

```bash
source $HOME/.nvm/nvm.sh && nvm use 24.22.1 >/dev/null
npm run verify
npm run benchmark:renderers
```

Expected: existing WebGL2 and experimental WebGPU behavior passes.

- [ ] **Step 7: Finalize documentation**

Update implementation status only from measured evidence. The new spec becomes `Implemented` only
after every functional and release performance gate passes. Otherwise it remains `Experimental`
with separate cold, warm, resident, preview, and snapshot results.

- [ ] **Step 8: Commit**

Commit browser documentation and evidence changes in each repository separately with:

```bash
git commit -m "docs(studio): document resident geometry evidence"
```

---

## Plan Self-Review

### Spec Coverage

| Spec area | Plan tasks |
|---|---|
| Backend-neutral resident IDs, modes, dirty channels, snapshots | 1 |
| Persistent pipelines, buffer classes, capacity pool, staging ring | 2 |
| Topology/attribute/result keys and kernel invalidation | 3 |
| Native residency, pinning, LRU, partial updates, outputs | 4 |
| Batched command stream, async completion, cancellation | 5 |
| Scheduler modes, snapshot policy, evidence, device loss | 6 |
| Binary full/delta preview packet and server endpoints | 7 |
| Browser packet validation, stable identity, scene construction | 8 |
| Browser geometry reuse, partial upload, Workbench fallback | 9 |
| Mode-specific benchmarks, UI evidence, documentation, release gates | 10 |
| CPU authority, canonical GLB, security, compatibility | Global constraints and Tasks 1, 6, 7, 9, 10 |

### Type Consistency

- `GeometryEvaluationMode`, `GeometryDirtyChannel`, and `ResidentGeometryId` are defined once in
  Task 1 and reused by later tasks.
- `DeviceResources`, `PipelineCache`, `BufferPool`, and `StagingRing` are device-scoped types from
  Task 2.
- `ResidencyKey` and `DirtyPlan` are produced by Task 3 and consumed unchanged by Task 4.
- `GeometryResidentBatchResult` is the only resident result crossing from `studio-gpu` to
  `studio-server`.
- `ICE3PGP1` manifest field names used by Rust packets and TypeScript parsing are identical.
- Browser `geometryId` is stable across Tasks 7–9 and is distinct from node ID.

### Review Focus Coverage

- Parameter-only native and browser reuse: Tasks 4 and 9.
- Transform-only and material-only zero-work paths: Tasks 3, 6, 7, and 9.
- Device loss, cancellation, corrupt packet, partial upload: Tasks 4–7 and 9.
- Pinning, in-flight buffers, LRU and budget pressure: Tasks 2, 4, and 6.
- Unsupported packet/backend fallback: Tasks 7, 9, and 10.
