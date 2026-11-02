# GPU Resident Hot-Path Redesign Implementation Plan

> Document ID: ICE3-STUDIO-PLAN-010
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate whole-mesh hashing and per-node synchronization from resident work, then measure and route the production preview path through stable resident handles without weakening CPU authority or fallback behavior.

**Architecture:** Immutable content is fingerprinted once when it enters the server and is represented by a device-generation-scoped resident handle. Repeated work looks up that handle, batches compatible updates into one command submission, and waits only at explicit snapshot or CPU-consumer boundaries. GPU buffers and metadata remain derived state; CPU Document state and canonical GLB remain authoritative.

**Tech Stack:** Rust, Tokio, `wgpu`, serde/JSON Schema, JSON-RPC, TypeScript, Node `node:test`

**Spec:** `docs/specs/2026-10-31-gpu-hot-path-redesign-design.md`

## Global Constraints

- Node.js is `24.22.1`; do not use Vite or Vitest.
- Use native execution; do not dispatch subagents.
- CPU remains authoritative for Document, revision, validation, approval, persistence, and canonical GLB.
- Native GPU and preview packets remain experimental and default-off until every release gate passes.
- Browser WebGL2 remains default; browser WebGPU remains explicit and experimental.
- Resident hit must not serialize a mesh, calculate a whole-mesh SHA, upload, dispatch, fence, or read back.
- A GPU handle is not a semantic node identity and must never enter candidate identity.
- Native and browser GPU buffers are separate; preserve packet and GLB boundaries.
- Keep CPU parity tolerance at `1e-5` for positions, normals, and bounds.
- Do not push, merge, or release as part of this plan unless explicitly requested.

## Review Focus

- A stale or evicted handle must never return geometry from a previous device generation.
- A digest collision or dependency mismatch must force CPU fallback rather than stale reuse.
- A mixed compatible batch must publish all entries or none after a failure.
- Cancellation before publication must not expose partial resident entries.
- Transform-only preview must not read back or re-upload geometry.

---

### Task 1: Add Digest and Resident Handle Contracts

**Files:**
- Modify: `crates/studio-core/src/gpu_contract.rs`
- Test: `crates/studio-core/tests/gpu_residency_contract.rs`
- Modify: generated `crates/studio-core/bindings/`

**Interfaces:**
- Produces: `GeometryDigestSet`
- Produces: `ResidentGeometryHandle`
- Produces: `ResidentGeometryState`
- Produces: `GeometryPhaseEvidence`
- Produces: `GeometryJobNode.digests: Option<GeometryDigestSet>`

- [ ] **Step 1: Write failing contract tests**

```rust
#[test]
fn digest_set_round_trips_and_does_not_contain_payload() {
    let digests = GeometryDigestSet {
        topology: "a".repeat(64),
        attribute: "b".repeat(64),
        dependency: "c".repeat(64),
        kernel_version: "geometry-kernel-v1".into(),
        parity_version: "parity-1e-5-v1".into(),
    };
    let encoded = serde_json::to_value(&digests).unwrap();
    assert_eq!(encoded["topology"], "a".repeat(64));
    assert!(encoded.get("positions").is_none());
}

#[test]
fn resident_handle_serializes_device_generation_and_state() {
    let handle = ResidentGeometryHandle {
        id: ResidentGeometryId("geometry-1".into()),
        geometry_id: "component/body".into(),
        digests: sample_digests(),
        vertex_count: 3,
        triangle_count: 1,
        device_generation: 4,
        state: ResidentGeometryState::Ready,
    };
    let encoded = serde_json::to_value(handle).unwrap();
    assert_eq!(encoded["deviceGeneration"], 4);
    assert_eq!(encoded["state"], "ready");
}
```

- [ ] **Step 2: Run tests to verify RED**

Run: `cargo test -p studio-core --test gpu_residency_contract`

Expected: compile failure because `GeometryDigestSet`, `ResidentGeometryHandle`, and `ResidentGeometryState` do not exist.

- [ ] **Step 3: Add the contracts**

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GeometryDigestSet {
    pub topology: String,
    pub attribute: String,
    pub dependency: String,
    pub kernel_version: String,
    pub parity_version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ResidentGeometryState {
    Pending,
    Ready,
    Lost,
    Evicted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ResidentGeometryHandle {
    pub id: ResidentGeometryId,
    pub geometry_id: String,
    pub digests: GeometryDigestSet,
    pub vertex_count: usize,
    pub triangle_count: usize,
    pub device_generation: u64,
    pub state: ResidentGeometryState,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GeometryPhaseEvidence {
    pub lookup_ms: f64,
    pub planning_ms: f64,
    pub validation_ms: f64,
    pub encode_ms: f64,
    pub submit_ms: f64,
    pub gpu_kernel_ms: Option<f64>,
    pub wait_ms: f64,
    pub snapshot_ms: f64,
    pub transport_ms: f64,
    pub browser_upload_ms: f64,
}
```

Add `pub digests: Option<GeometryDigestSet>` to `GeometryJobNode` and `pub phases: GeometryPhaseEvidence` to `GeometryEvaluationEvidence`.

- [ ] **Step 4: Verify GREEN**

Run: `cargo test -p studio-core --test gpu_residency_contract`

Expected: PASS, including the original dirty-mask and unsupported-backend tests.

- [ ] **Step 5: Generate bindings and commit**

```bash
cargo test -p studio-core
cargo run -p studio-cli -- schema
git add crates/studio-core/src/gpu_contract.rs \
  crates/studio-core/tests/gpu_residency_contract.rs \
  crates/studio-core/bindings schemas/studio-api.schema.json schemas/studio-types.ts
git commit -m "feat(studio-core): define resident geometry handles"
```

---

### Task 2: Resolve Resident Hot Paths by Precomputed Digests

**Files:**
- Modify: `crates/studio-gpu/src/residency/key.rs`
- Modify: `crates/studio-gpu/src/residency/store.rs`
- Modify: `crates/studio-gpu/src/device.rs`
- Test: `crates/studio-gpu/tests/resident_evaluation.rs`

**Interfaces:**
- Consumes: `GeometryDigestSet`, `ResidentGeometryHandle`
- Produces: `ResidencyKey::from_digests`
- Produces: `GpuGeometryResidency::evaluate_with_digests`
- Produces: `GpuGeometryResidency::lookup_handle`
- Produces: `NativeGpuBackend::evaluate_resident_handle`

- [ ] **Step 1: Write the failing no-hash hot-path test**

```rust
#[test]
fn repeated_digest_handle_lookup_does_not_reserialize_mesh() {
    let Some(backend) = NativeGpuBackend::for_test_gpu() else { return };
    let mesh = triangle_mesh();
    let digests = backend
        .prepare_geometry_digests(&NodeContent::CustomMesh(mesh.clone()))
        .unwrap();
    let first = backend
        .evaluate_with_digests(&mesh, digests.clone(), "component/body")
        .unwrap();
    let before = backend.resident_phase_counters().unwrap();

    for _ in 0..16 {
        backend.evaluate_resident_handle(&first.id).unwrap();
    }

    let after = backend.resident_phase_counters().unwrap();
    assert_eq!(after.full_hashes, before.full_hashes);
    assert_eq!(after.upload_bytes, before.upload_bytes);
    assert_eq!(after.fence_waits, before.fence_waits);
}
```

- [ ] **Step 2: Run test to verify RED**

Run: `cargo test -p studio-gpu --test resident_evaluation repeated_digest_handle_lookup --release`

Expected: compile failure because digest preparation, handle lookup, and phase counters do not exist.

- [ ] **Step 3: Implement digest conversion and handles**

```rust
impl ResidencyKey {
    pub fn from_digests(digests: &GeometryDigestSet) -> Self {
        let result = hash_parts(&[
            b"geometry-result-v2",
            digests.topology.as_bytes(),
            digests.attribute.as_bytes(),
            digests.dependency.as_bytes(),
            digests.kernel_version.as_bytes(),
            digests.parity_version.as_bytes(),
        ]);
        Self {
            topology: digests.topology.clone(),
            attribute: digests.attribute.clone(),
            result,
        }
    }
}
```

Store handles in a map keyed by `ResidentGeometryId`. `evaluate_resident_handle` performs only:

1. device generation validation;
2. handle state validation;
3. LRU update;
4. evidence construction.

Keep the existing `evaluate_custom_mesh_resident` compatibility path, which may compute a digest once before first insertion.

- [ ] **Step 4: Verify GREEN**

Run: `cargo test -p studio-gpu --test resident_evaluation --release`

Expected: PASS; all 16 repeated handle lookups report zero additional full hashes, uploads, and waits.

- [ ] **Step 5: Test stale generations and commit**

```rust
#[test]
fn stale_device_generation_rejects_the_handle() {
    let Some(backend) = NativeGpuBackend::for_test_gpu() else { return };
    let handle = insert_sample_handle(&backend);
    let stale = ResidentGeometryHandle {
        device_generation: handle.device_generation.saturating_sub(1),
        ..handle
    };
    assert!(backend.evaluate_resident_handle_checked(&stale).is_err());
}
```

Run: `cargo test -p studio-gpu --test resident_evaluation --release`

Expected: PASS.

```bash
git add crates/studio-gpu/src/residency/key.rs \
  crates/studio-gpu/src/residency/store.rs crates/studio-gpu/src/device.rs \
  crates/studio-gpu/tests/resident_evaluation.rs
git commit -m "feat(studio-gpu): resolve resident work by stable handles"
```

---

### Task 3: Record Phase-Specific Evidence

**Files:**
- Modify: `crates/studio-gpu/src/device.rs`
- Modify: `crates/studio-gpu/src/kernels/mesh.rs`
- Test: `crates/studio-gpu/tests/resident_evaluation.rs`
- Test: `crates/studio-gpu/tests/native_gpu_benchmark.rs`

**Interfaces:**
- Consumes: `GeometryPhaseEvidence`
- Produces: `NativeGpuBackend::resident_phase_counters`
- Produces: timestamp-query kernel latency when supported

- [ ] **Step 1: Write failing phase-evidence tests**

```rust
#[test]
fn resident_hit_reports_zero_gpu_and_transfer_phases() {
    let Some(backend) = NativeGpuBackend::for_test_gpu() else { return };
    let handle = insert_resident_triangle(&backend);
    let before = backend.resident_phase_counters().unwrap();
    let entry = backend.evaluate_resident_handle(&handle.id).unwrap();
    let after = backend.resident_phase_counters().unwrap();
    assert_eq!(after.upload_bytes, before.upload_bytes);
    assert_eq!(after.fence_waits, before.fence_waits);
    assert_eq!(entry.evidence.phases.gpu_kernel_ms, None);
}
```

- [ ] **Step 2: Run test to verify RED**

Run: `cargo test -p studio-gpu --test resident_evaluation resident_hit_reports_zero --release`

Expected: compile failure because phase counters and `entry.evidence.phases` do not exist.

- [ ] **Step 3: Instrument boundaries**

Record counters and timings at these exact boundaries:

```text
lookup start/end
planning start/end
validation start/end
encode start/end
submit start/end
wait start/end
snapshot mapping start/end
```

Use `wgpu::Features::TIMESTAMP_QUERY` when available. If unavailable, set `gpu_kernel_ms` to `None`; do not estimate it from wall time.

- [ ] **Step 4: Verify GREEN**

Run:

```bash
cargo test -p studio-gpu --test resident_evaluation --release
cargo test -p studio-gpu --test native_gpu_benchmark
```

Expected: PASS; benchmark report includes `phases` under every mode.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-gpu/src/device.rs crates/studio-gpu/src/kernels/mesh.rs \
  crates/studio-gpu/tests/resident_evaluation.rs \
  crates/studio-gpu/tests/native_gpu_benchmark.rs
git commit -m "feat(studio-gpu): attribute resident GPU latency"
```

---

### Task 4: Encode Heterogeneous Compatible Nodes in One Submission

**Files:**
- Modify: `crates/studio-gpu/src/residency/batch.rs`
- Modify: `crates/studio-gpu/src/residency/store.rs`
- Modify: `crates/studio-gpu/src/kernels/mesh.rs`
- Modify: `crates/studio-gpu/src/device.rs`
- Test: `crates/studio-gpu/tests/resident_batch.rs`

**Interfaces:**
- Consumes: precomputed digest handles and phase evidence
- Produces: one command encoder and one queue submission for compatible distinct nodes
- Produces: `poll_resident` without implicit completion waits

- [ ] **Step 1: Write the failing heterogeneous batch test**

```rust
#[test]
fn distinct_compatible_meshes_share_one_submission() {
    let Some(backend) = NativeGpuBackend::for_test_gpu() else { return };
    let first = triangle_mesh_with_offset(0.0);
    let second = triangle_mesh_with_offset(1.0);
    let result = backend
        .evaluate_resident(three_node_batch(&[first, second]))
        .unwrap();
    assert_eq!(result.entries.len(), 3);
    assert_eq!(result.evidence.submission_count, 1);
    assert_eq!(result.evidence.phases.wait_ms, 0.0);
}
```

- [ ] **Step 2: Run test to verify RED**

Run: `cargo test -p studio-gpu --test resident_batch distinct_compatible_meshes --release`

Expected: `submission_count` is greater than one because content is admitted sequentially.

- [ ] **Step 3: Split allocation and encoding**

Refactor resident mesh creation into:

```rust
pub(crate) fn prepare_resident_custom_mesh(
    device: &wgpu::Device,
    resources: &DeviceResources,
    mesh: &TriangleMesh,
) -> Result<PendingResidentMesh, BackendError>;

pub(crate) fn encode_resident_custom_mesh(
    encoder: &mut wgpu::CommandEncoder,
    resources: &DeviceResources,
    pending: &PendingResidentMesh,
);
```

Allocate every compatible node first. On allocation failure, release every temporary lease and
publish no entries. Encode all bind groups into one encoder, call `queue.submit` once, and return a
pending completion handle.

- [ ] **Step 4: Verify GREEN**

Run: `cargo test -p studio-gpu --test resident_batch --release`

Expected: PASS; compatible distinct nodes report one submission and no CPU wait.

- [ ] **Step 5: Test cancellation and commit**

```rust
#[test]
fn cancellation_after_encoding_publishes_no_entries() {
    let Some(backend) = NativeGpuBackend::for_test_gpu() else { return };
    let job = backend.submit_resident_handle_batch(batch_with_id("cancel-me"));
    backend.cancel("cancel-me").unwrap();
    assert!(backend.wait_resident_handle(job).is_err());
    assert!(backend.poll_resident("cancel-me").is_none());
}
```

Run: `cargo test -p studio-gpu --test resident_batch --release`

Expected: PASS.

```bash
git add crates/studio-gpu/src/residency/batch.rs \
  crates/studio-gpu/src/residency/store.rs crates/studio-gpu/src/kernels/mesh.rs \
  crates/studio-gpu/src/device.rs crates/studio-gpu/tests/resident_batch.rs
git commit -m "feat(studio-gpu): batch heterogeneous resident nodes"
```

---

### Task 5: Use Stable Handles in ComputeService

**Files:**
- Modify: `crates/studio-server/src/compute.rs`
- Modify: `crates/studio-server/tests/resident_scheduler.rs`
- Modify: `crates/studio-server/bindings/`

**Interfaces:**
- Consumes: `GeometryDigestSet`, `ResidentGeometryHandle`
- Produces: `ComputeService::register_geometry`
- Produces: `ComputeService::evaluate_handle`
- Produces: `ComputeService::snapshot_handle`
- Preserves existing `evaluate`, `evaluate_for`, and `snapshot_for` as compatibility wrappers

- [ ] **Step 1: Write failing service tests**

```rust
#[tokio::test]
async fn repeated_handle_evaluation_skips_content_hashing() {
    let gpu = FakeGpu::new(FakeOutcome::Success);
    let service = resident_service(gpu.clone());
    let handle = service
        .register_geometry("shared".into(), box_content())
        .await
        .unwrap();
    let first = service.evaluate_handle(&handle).await.unwrap();
    let hashes = service.status().resident_hot_path_hashes;
    let second = service.evaluate_handle(&first.handle).await.unwrap();
    assert_eq!(service.status().resident_hot_path_hashes, hashes);
    assert_eq!(second.mode, "resident");
}

#[tokio::test]
async fn stale_handle_falls_back_without_mutating_revision() {
    let gpu = FakeGpu::new(FakeOutcome::Success);
    let service = resident_service(gpu);
    let handle = service
        .register_geometry("shared".into(), box_content())
        .await
        .unwrap();
    let document = component_document();
    let revision = document.revision;
    let stale = ResidentGeometryHandle {
        device_generation: handle.device_generation + 1,
        ..handle
    };
    assert!(service.evaluate_handle(&stale).await.is_err());
    assert_eq!(document.revision, revision);
}
```

- [ ] **Step 2: Run tests to verify RED**

Run: `cargo test -p studio-server --test resident_scheduler repeated_handle_evaluation -- --nocapture`

Expected: compile failure because handle methods and evidence fields do not exist.

- [ ] **Step 3: Add service registration and handle lookup**

Registration computes topology, attribute, dependency, kernel, and parity digests once and retains
an immutable source snapshot. `evaluate_handle` uses only the handle ID and generation. Compatibility
wrappers register content once and then delegate to handle evaluation.

Add status counters:

```rust
pub resident_hot_path_hashes: usize,
pub handle_lookup_ms: f64,
pub last_geometry_phases: GeometryPhaseEvidence,
```

- [ ] **Step 4: Verify GREEN**

Run:

```bash
cargo test -p studio-server --test resident_scheduler
cargo test -p studio-server
```

Expected: PASS; old APIs remain source-compatible.

- [ ] **Step 5: Generate bindings and commit**

```bash
cargo run -p studio-cli -- schema
git add crates/studio-server/src/compute.rs crates/studio-server/tests/resident_scheduler.rs \
  crates/studio-server/bindings schemas/studio-api.schema.json schemas/studio-types.ts
git commit -m "feat(studio-server): evaluate resident geometry handles"
```

---

### Task 6: Route Preview Source Views Through Resident Handles

**Files:**
- Modify: `crates/studio-server/src/preview.rs`
- Modify: `crates/studio-server/src/rpc.rs`
- Modify: `crates/studio-server/src/preview_packet.rs`
- Modify: `crates/studio-server/tests/preview_packet.rs`
- Modify: `crates/studio-server/tests/resident_scheduler.rs`

**Interfaces:**
- Consumes: `ComputeService::register_geometry`, `evaluate_handle`, and `snapshot_handle`
- Produces: `PreviewGeometrySourceView`
- Produces: packet backend/residency evidence derived from actual resident mode

- [ ] **Step 1: Write failing preview integration tests**

```rust
#[tokio::test]
async fn transform_only_preview_reuses_resident_source_without_readback() {
    let gpu = FakeGpu::new(FakeOutcome::Success);
    let service = resident_service(gpu.clone());
    let before = gpu.snapshot_calls();
    let preview = build_resident_preview(&service, box_content(), Transform::IDENTITY)
        .await
        .unwrap();
    assert_eq!(preview.packet.manifest.residencyMode, "resident");
    assert_eq!(preview.packet.binary.len(), 0);
    assert_eq!(gpu.snapshot_calls(), before);
}

#[test]
fn gpu_derived_attribute_requests_explicit_snapshot() {
    let source = PreviewGeometrySourceView {
        positions: Arc::new(vec![[0.0, 0.0, 0.0]; 3]),
        indices: Arc::new(vec![0, 1, 2]),
        needs_gpu_normals: true,
    };
    assert!(source.requires_snapshot());
}
```

- [ ] **Step 2: Run test to verify RED**

Run: `cargo test -p studio-server --test preview_packet transform_only_preview -- --nocapture`

Expected: compile failure because resident preview source views do not exist.

- [ ] **Step 3: Build packets from resident source views**

Define:

```rust
pub struct PreviewGeometrySourceView {
    pub handle: ResidentGeometryHandle,
    pub positions: Arc<Vec<[f32; 3]>>,
    pub indices: Arc<Vec<u32>>,
    pub needs_gpu_normals: bool,
}

impl PreviewGeometrySourceView {
    pub fn requires_snapshot(&self) -> bool {
        self.needs_gpu_normals
    }
}
```

For positions and topology that already exist in the immutable source snapshot, build packet ranges
without GPU readback. Use the handle's geometry ID and digests. Transform-only and material-only
revisions emit no binary chunk. Request `snapshot_handle` only when the packet consumer requires a
GPU-derived attribute absent from source.

Update `PreviewAdapter` to accept `Arc<ComputeService>` through an explicit builder used by the
server; direct adapter tests keep CPU behavior.

- [ ] **Step 4: Verify GREEN**

Run:

```bash
cargo test -p studio-server --test preview_packet
cargo test -p studio-server --test resident_scheduler
cargo test -p studio-server
```

Expected: PASS; transform-only packet binary is empty and GPU snapshot calls do not increase.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-server/src/preview.rs crates/studio-server/src/rpc.rs \
  crates/studio-server/src/preview_packet.rs \
  crates/studio-server/tests/preview_packet.rs \
  crates/studio-server/tests/resident_scheduler.rs
git commit -m "feat(studio-server): build previews from resident source views"
```

---

### Task 7: Re-run Mode-Specific Gates and Document Results

**Files:**
- Modify: `crates/studio-gpu/tests/native_gpu_benchmark.rs`
- Modify: `benchmarks/native-gpu.mjs`
- Modify: `apps/viewer/src/compute/ComputePanel.tsx`
- Modify: `apps/viewer/tests/compute-status.test.ts`
- Modify: `docs/README.md`
- Modify: `docs/reference/api.md`
- Modify: `README.md`

**Interfaces:**
- Consumes: phase evidence and stable handles
- Produces: benchmark fields for lookup, plan, encode, submit, kernel, wait, snapshot, transport

- [ ] **Step 1: Write failing evidence test**

```ts
test('reports resident phase evidence separately', () => {
  const view = formatComputeStatus({
    ...readyStatus,
    lastEvaluationMode: 'resident',
    handleLookupMs: 0.02,
    lastGpuKernelMs: 1.3,
    lastWaitMs: 0,
    residentBytes: 1234,
  });
  assert.match(view.metrics, /lookup 0\.02 ms/);
  assert.match(view.metrics, /kernel 1\.3 ms/);
  assert.match(view.metrics, /wait 0 ms/);
});
```

- [ ] **Step 2: Run test to verify RED**

Run: `npm --prefix apps/viewer test`

Expected: resident phase metrics are absent.

- [ ] **Step 3: Add phase output and release checks**

The benchmark must reject a report when:

```text
resident 500k or 2M p50 >= 0.90 * CPU p50
resident 500k or 2M p95 > 1.05 * CPU p95
resident hit key lookup p50 >= 0.1 ms
resident hit full_hashes > 0
transform-only browser upload bytes > 0
```

If any check fails, print the failed mode and leave Auto on CPU.

- [ ] **Step 4: Run all gates**

Run:

```bash
PATH=/Users/felix/.nvm/versions/node/v24.22.1/bin:$PATH npm run verify
PATH=/Users/felix/.nvm/versions/node/v24.22.1/bin:$PATH npm run benchmark
PATH=/Users/felix/.nvm/versions/node/v24.22.1/bin:$PATH npm run benchmark:large-industrial
PATH=/Users/felix/.nvm/versions/node/v24.22.1/bin:$PATH npm run benchmark:gpu
```

Expected: functional commands pass; `benchmark:gpu` passes only if all performance gates pass.

- [ ] **Step 5: Update documentation from measured output and commit**

```bash
git add crates/studio-gpu/tests/native_gpu_benchmark.rs benchmarks/native-gpu.mjs \
  apps/viewer/src/compute/ComputePanel.tsx apps/viewer/tests/compute-status.test.ts \
  README.md docs/README.md docs/reference/api.md
git commit -m "docs(studio): record resident hot-path evidence"
```

## Self-Review

- Spec coverage: digest/handle contract in Task 1; no-hash lookup in Task 2; phase evidence in Task 3;
  batching/completion in Task 4; server integration in Task 5; source-only preview transport in Task
  6; measured release gates in Task 7.
- Placeholder scan: no deferred implementation language remains; every task names its concrete API,
  test, command, and commit.
- Type consistency: `GeometryDigestSet`, `ResidentGeometryHandle`, `GeometryPhaseEvidence`, and method
  names are introduced once and reused unchanged.
- Review focus mapping: stale generations in Task 2; digest/dependency mismatch in Task 5; atomic
  mixed-batch publication and cancellation in Task 4; transform-only readback in Task 6.
