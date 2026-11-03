# Native GPU Compute Backend Implementation Plan

> Document ID: ICE3-STUDIO-PLAN-008
>
> Status: Historical
>
> Owner: ICE3 Studio maintainers

> **历史状态：** 本文保留已完成实施过程的审计记录，不代表当前生产依赖；当前能力以
> [Asset Pipeline](../reference/asset-pipeline.md) 和 [能力清单](../reference/capabilities.md)
> 为准。

> **For agentic workers:** REQUIRED SUB-SKILL: Use `executing-plans` to implement this plan
> task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a native GPU backend that accelerates large derived-geometry batches while preserving
CPU-authoritative documents, validation, caching, canonical export, and safe fallback.

**Architecture:** Define a backend-neutral geometry contract in `studio-core`, implement device
lifecycle and WGSL kernels in a new `studio-gpu` crate, and schedule CPU/GPU work from
`studio-server` using measured workload and memory thresholds. The current 216-object fixture and
synthetic 500k/2M-triangle workloads provide parity and performance evidence.

**Tech Stack:** Rust 1.98, `wgpu` 30.0.1, WGSL, `async-trait`, `bytemuck`, `pollster`, serde,
schemars, ts-rs, Tokio; Node.js 24.22.1 benchmarks.

**Spec:** [`docs/specs/2026-10-30-native-gpu-compute-design.md`](../specs/2026-10-30-native-gpu-compute-design.md)

**Implementation status:** Experimental. Functional tasks are implemented and tested. The release
performance gate fails on the current integrated Metal GPU; automatic selection therefore remains
on CPU until a measured crossover is established.

## 1. Global Constraints

- GPU output is derived geometry and never owns Document, validation, approval, or revision state.
- CPU evaluation remains the canonical reference and required fallback.
- `studio-core` must not depend on `wgpu`, Viewer, Toolchain, Server, or device APIs.
- No API client can submit WGSL, choose an arbitrary adapter, or bypass host policy.
- Backend selection must be measured; detecting a GPU is not proof that every workload benefits.
- CPU/GPU parity requires identical topology and bounded position, normal, and bounds error.
- Canonical source GLB remains CPU-generated.
- Existing WebGL2 and experimental browser WebGPU behavior must not regress.
- Node.js is `24.22.1`; no Vite or Vitest.
- Git author and committer remain `damoqiongqiu`.
- Every behavior change follows RED-GREEN-REFACTOR and ends in a focused commit.
- Root acceptance is `npm run verify`; performance acceptance additionally runs both benchmark
  suites.

## 2. Review Focus

- Systems without an adapter, with incompatible limits, or with a lost device must continue through
  CPU evaluation and preserve the last committed revision; tests must force each path.
- Empty, NaN, oversized-index, and zero-sized geometry must fail before GPU allocation and must not
  rely on driver-specific behavior.
- 500k and 2M triangle workloads must compare actual end-to-end transfer plus kernel time, not
  isolated shader duration; a GPU path without stable gain must not be selected.
- Cache keys must include resolved geometry content, dependency digests, backend, and kernel
  version; moving an instance may invalidate routes without invalidating shared part geometry.
- Concurrent interactive and background jobs must preserve priority, cancellation, bounded
  memory, and a responsive health endpoint; tests must exercise queue pressure and device loss.

---

### 2.1 Task 1: Define backend-neutral GPU contracts in Studio Core

**Files:**
- Create: `crates/studio-core/src/gpu_contract.rs`
- Modify: `crates/studio-core/src/lib.rs`
- Test: `crates/studio-core/tests/gpu_contract.rs`

**Interfaces:**
- Produces: `GeometryBackendKind::{Cpu, Gpu}`
- Produces: `GeometryBackendMode::{Auto, Cpu, Gpu}`
- Produces: `GeometryJobPriority::{Interactive, Throughput, Background}`
- Produces: `GeometryBatch`, `GeometryJobNode`, `GeometryBatchResult`,
  `GeometryExecutionStats`, and `BackendCapabilities`
- Produces: `trait GeometryBackend`
- Later tasks use these exact names.

- [ ] **Step 1: Write failing contract tests**

Create `crates/studio-core/tests/gpu_contract.rs`:

```rust
use studio_core::gpu_contract::{
    BackendCapabilities, GeometryBackendKind, GeometryBackendMode, GeometryJobPriority,
};

#[test]
fn serializes_stable_backend_contract_values() {
    assert_eq!(
        serde_json::to_string(&GeometryBackendKind::Gpu).unwrap(),
        "\"gpu\""
    );
    assert_eq!(
        serde_json::to_string(&GeometryBackendMode::Auto).unwrap(),
        "\"auto\""
    );
    assert_eq!(
        serde_json::to_string(&GeometryJobPriority::Interactive).unwrap(),
        "\"interactive\""
    );
}

#[test]
fn reports_safe_capability_defaults() {
    let capabilities = BackendCapabilities::cpu();
    assert_eq!(capabilities.kind, GeometryBackendKind::Cpu);
    assert_eq!(capabilities.health, "ready");
    assert_eq!(capabilities.accelerated, false);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test -p studio-core --test gpu_contract
```

Expected: module-not-found failure.

- [ ] **Step 3: Add the contracts**

Add `async-trait.workspace = true` and implement:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum GeometryBackendKind {
    Cpu,
    Gpu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum GeometryBackendMode {
    Auto,
    Cpu,
    Gpu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum GeometryJobPriority {
    Background,
    Throughput,
    Interactive,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeometryBatch {
    pub job_id: String,
    pub priority: GeometryJobPriority,
    pub budget_bytes: u64,
    pub nodes: Vec<GeometryJobNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeometryJobNode {
    pub node_id: NodeId,
    pub content: NodeContent,
    pub content_digest: String,
    pub dependency_digest: String,
    pub cache_key: String,
}

#[derive(Debug, Clone, Default)]
pub struct GeometryExecutionStats {
    pub cpu_ms: f64,
    pub upload_bytes: u64,
    pub kernel_ms: f64,
    pub readback_bytes: u64,
    pub queue_wait_ms: f64,
    pub gpu_memory_estimate_bytes: u64,
    pub cache_hits: usize,
    pub fallback_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GeometryBatchResult {
    pub job_id: String,
    pub backend: GeometryBackendKind,
    pub meshes: BTreeMap<NodeId, EvaluatedMesh>,
    pub stats: GeometryExecutionStats,
    pub diagnostics: Vec<Diagnostic>,
}

#[async_trait::async_trait]
pub trait GeometryBackend: Send + Sync {
    async fn initialize(&self) -> Result<BackendCapabilities, BackendError>;
    async fn evaluate(&self, batch: GeometryBatch) -> Result<GeometryBatchResult, BackendError>;
    async fn cancel(&self, job_id: &str) -> Result<(), BackendError>;
    fn stats(&self) -> GeometryExecutionStats;
}
```

Define `BackendError` with stable variants for unavailable, incompatible, budget, kernel, device,
and parity failures. Add `BackendCapabilities::cpu()` with `health: "ready"`,
`accelerated: false`, no adapter identity, and zero device-memory budget.

`BackendCapabilities` fields are `kind`, `health`, `accelerated`, `adapterTier`,
`backendFamily`, `memoryBudgetBytes`, and `fallbackReason`; adapter names and machine identifiers
are never serialized.

- [ ] **Step 4: Run focused tests and clippy**

Run:

```bash
cargo test -p studio-core --test gpu_contract
cargo clippy -p studio-core --all-targets -- -D warnings
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/studio-core/src/lib.rs \
  crates/studio-core/src/gpu_contract.rs crates/studio-core/tests/gpu_contract.rs
git commit -m "feat(studio-core): define native GPU geometry contract"
```

---

### 2.2 Task 2: Add the native GPU crate and device lifecycle

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/studio-gpu/Cargo.toml`
- Create: `crates/studio-gpu/src/lib.rs`
- Create: `crates/studio-gpu/src/device.rs`
- Create: `crates/studio-gpu/tests/device_contract.rs`

**Interfaces:**
- Consumes: `GeometryBackend` and capability types from Task 1.
- Produces: `NativeGpuBackend::new(mode: GeometryBackendMode)`
- Produces: `NativeGpuBackend::discover()`, `NativeGpuBackend::capabilities()`,
  `NativeGpuBackend::for_test_gpu()`, and `CapabilityProbe`
- Produces bounded adapter selection and device-loss state.

- [ ] **Step 1: Write failing discovery tests**

Create `crates/studio-gpu/tests/device_contract.rs`:

```rust
use studio_core::gpu_contract::{GeometryBackendKind, GeometryBackendMode};
use studio_gpu::NativeGpuBackend;

#[test]
fn cpu_mode_never_requires_a_gpu_adapter() {
    let backend = NativeGpuBackend::new(GeometryBackendMode::Cpu);
    let capabilities = pollster::block_on(backend.capabilities()).unwrap();
    assert_eq!(capabilities.kind, GeometryBackendKind::Cpu);
    assert_eq!(capabilities.accelerated, false);
}

#[test]
fn gpu_discovery_is_bounded_and_returns_structured_state() {
    let backend = NativeGpuBackend::new(GeometryBackendMode::Gpu);
    let result = pollster::block_on(backend.initialize());
    match result {
        Ok(capabilities) => {
            assert_eq!(capabilities.kind, GeometryBackendKind::Gpu);
            assert!(capabilities.health == "ready");
        }
        Err(error) => assert!(
            error.to_string().contains("unavailable")
                || error.to_string().contains("incompatible")
        ),
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test -p studio-gpu --test device_contract
```

Expected: package/module-not-found failure.

- [ ] **Step 3: Add the crate and bounded initialization**

Add workspace dependencies:

```toml
wgpu = "30.0.1"
bytemuck = { version = "1", features = ["derive"] }
pollster = "0.4"
async-trait.workspace = true
```

Add `crates/studio-gpu` to workspace members and implement:

```rust
pub struct NativeGpuBackend {
    mode: GeometryBackendMode,
    state: Mutex<DeviceState>,
    limits: GpuHostLimits,
}

#[derive(Debug, Clone)]
pub struct GpuHostLimits {
    pub max_buffer_bytes: u64,
    pub max_job_memory_bytes: u64,
    pub max_concurrent_jobs: usize,
}
```

`discover()` must:

1. Create a native `wgpu::Instance`.
2. Request a high-performance adapter with fallback allowed.
3. Inspect adapter info, features, limits, and backend.
4. Request a device with only capability-discovery, timestamp-query when available, and default
   limits capped by host policy.
5. Run a one-triangle parity warm-up.
6. return structured unavailable/incompatible errors on failure.

Do not expose raw adapter identifiers. Report only a sanitized acceleration tier and backend
family.

- [ ] **Step 4: Verify**

Run:

```bash
cargo test -p studio-gpu --test device_contract
cargo clippy -p studio-gpu --all-targets -- -D warnings
```

Expected: PASS on CPU-only and GPU-capable machines without panicking.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock crates/studio-gpu
git commit -m "feat(studio-gpu): add bounded native device lifecycle"
```

---

### 2.3 Task 3: Implement box and plane compute kernels with CPU parity

**Files:**
- Create: `crates/studio-gpu/src/kernels/mod.rs`
- Create: `crates/studio-gpu/src/kernels/box_plane.wgsl`
- Create: `crates/studio-gpu/src/kernels/primitives.rs`
- Modify: `crates/studio-gpu/src/lib.rs`
- Test: `crates/studio-gpu/tests/primitive_parity.rs`

**Interfaces:**
- Consumes: initialized device and queues.
- Produces: `evaluate_box` and `evaluate_plane`.
- Produces: `PrimitiveGpuParams` and `MeshReadback`.

- [ ] **Step 1: Add failing parity tests**

Create `crates/studio-gpu/tests/primitive_parity.rs`:

```rust
use studio_core::geometry::{Primitive, evaluate_node_content};
use studio_core::document::NodeContent;
use studio_gpu::{NativeGpuBackend, PrimitiveJob};

fn assert_parity(primitive: Primitive) {
    let backend = NativeGpuBackend::for_test_gpu();
    let Some(backend) = backend else { return };
    let gpu = backend.evaluate_primitive(PrimitiveJob { primitive: primitive.clone() }).unwrap();
    let cpu = evaluate_node_content(&NodeContent::Primitive(primitive)).unwrap();
    assert_eq!(gpu.indices, cpu.indices);
    assert_eq!(gpu.vertices.len(), cpu.vertices.len());
    for (gpu_vertex, cpu_vertex) in gpu.vertices.iter().zip(&cpu.vertices) {
        for axis in 0..3 {
            assert!((gpu_vertex.position[axis] - cpu_vertex.position[axis]).abs() <= 1e-5);
            assert!((gpu_vertex.normal[axis] - cpu_vertex.normal[axis]).abs() <= 1e-5);
        }
    }
}

#[test]
fn box_and_plane_match_cpu_reference() {
    assert_parity(Primitive::Box { width: 3.0, height: 2.0, depth: 4.0 });
    assert_parity(Primitive::Plane { width: 8.0, depth: 6.0 });
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test -p studio-gpu --test primitive_parity
```

Expected: missing GPU primitive API.

- [ ] **Step 3: Implement compute pipelines**

Use storage buffers for parameters, vertices, and indices. WGSL kernels must generate:

- box: six independent faces, 24 vertices, 36 indices, per-face normals;
- plane: four vertices and two triangles on XZ with `+Y` normals.

Use fixed `@workgroup_size(64)` and reject zero groups before dispatch. Create staging readback
buffers with `MAP_READ`, map asynchronously, validate finite values, and unmap before returning.
The CPU implementation remains the parity oracle.

- [ ] **Step 4: Run parity and all existing geometry tests**

Run:

```bash
cargo test -p studio-gpu --test primitive_parity
cargo test -p studio-core --test geometry_evaluation
```

Expected: PASS when GPU is available; tests skip cleanly when it is unavailable.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-gpu/src/kernels crates/studio-gpu/src/lib.rs \
  crates/studio-gpu/tests/primitive_parity.rs
git commit -m "feat(studio-gpu): compute boxes and planes natively"
```

---

### 2.4 Task 4: Implement cylinder, sphere, and sweep kernels

**Files:**
- Create: `crates/studio-gpu/src/kernels/radial.wgsl`
- Modify: `crates/studio-gpu/src/kernels/mod.rs`
- Modify: `crates/studio-gpu/src/kernels/primitives.rs`
- Create: `crates/studio-gpu/tests/radial_parity.rs`

**Interfaces:**
- Produces: `evaluate_cylinder`, `evaluate_sphere`, and `evaluate_sweep`.
- Reuses the readback and parity validation from Task 3.

- [ ] **Step 1: Add failing radial and sweep parity tests**

Test representative values:

```rust
assert_radial_parity(Primitive::Cylinder {
    radius: 2.0,
    height: 5.0,
    segments: 64,
});
assert_radial_parity(Primitive::Sphere {
    radius: 3.0,
    segments: 96,
    rings: 48,
});
assert_sweep_parity(&[
    [0.0, 0.0, 0.0],
    [5.0, 0.0, 0.0],
    [5.0, 0.0, 5.0],
    [10.0, 0.0, 5.0],
], 0.35, 24);
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test -p studio-gpu --test radial_parity
```

Expected: missing radial GPU implementation.

- [ ] **Step 3: Implement radial WGSL kernels**

Match CPU winding and segment/ring limits exactly. Generate side walls and end caps with stable
ring orientation. Sweep kernels compute deterministic tangent frames from neighboring points,
generate parallel-transport-compatible rings, and cap both ends.

Reject invalid point counts, duplicate adjacent points, non-positive radius, non-finite values,
and unsupported segment counts before creating buffers.

- [ ] **Step 4: Verify parity and existing tests**

Run:

```bash
cargo test -p studio-gpu --test radial_parity
cargo test -p studio-core --test geometry_evaluation
```

Expected: PASS or capability skip, as in Task 3.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-gpu/src/kernels crates/studio-gpu/tests/radial_parity.rs
git commit -m "feat(studio-gpu): compute radial primitives and sweeps"
```

---

### 2.5 Task 5: Accelerate normals and bounds for custom triangle meshes

**Files:**
- Create: `crates/studio-gpu/src/kernels/mesh.wgsl`
- Modify: `crates/studio-gpu/src/kernels/mod.rs`
- Modify: `crates/studio-gpu/src/lib.rs`
- Test: `crates/studio-gpu/tests/mesh_parity.rs`

**Interfaces:**
- Produces: `evaluate_custom_mesh(mesh: &TriangleMesh) -> Result<EvaluatedMesh, BackendError>`.
- Preserves input positions and indices; computes accumulated normals and bounds.

- [ ] **Step 1: Add failing mesh parity tests**

```rust
#[test]
fn custom_mesh_normals_and_bounds_match_cpu_reference() {
    let mesh = synthetic_mesh(500_000);
    let backend = NativeGpuBackend::for_test_gpu();
    let Some(backend) = backend else { return };
    let gpu = backend.evaluate_custom_mesh(&mesh).unwrap();
    let cpu = validate_mesh(&mesh).unwrap();
    assert_eq!(gpu.indices, cpu.indices);
    assert_bounds_within_relative_error(gpu.bounds, cpu.bounds, 1e-5);
    assert_normals_within(gpu.vertices, cpu.vertices, 1e-5);
}
```

Use a deterministic indexed grid rather than unindexed triangle soup.

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test -p studio-gpu --test mesh_parity
```

Expected: missing mesh kernel.

- [ ] **Step 3: Implement normal and bounds kernels**

Upload validated positions and indices. Accumulate face normals into an output normal buffer,
normalize on readback, and compute bounds through bounded per-workgroup partial reductions.
Never mutate input buffers. Reject non-indexed or out-of-range indices before dispatch.

- [ ] **Step 4: Run parity at 50k, 500k, and 2M triangles**

Run:

```bash
cargo test -p studio-gpu --test mesh_parity --release
```

Expected: PASS on the reference GPU; capability-dependent skips are recorded in test output.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-gpu/src/kernels crates/studio-gpu/src/lib.rs \
  crates/studio-gpu/tests/mesh_parity.rs
git commit -m "feat(studio-gpu): accelerate mesh normals and bounds"
```

---

### 2.6 Task 6: Integrate measured CPU/GPU scheduling and safe fallback

**Files:**
- Create: `crates/studio-server/src/compute.rs`
- Modify: `crates/studio-server/src/lib.rs`
- Modify: `crates/studio-server/src/rpc.rs`
- Modify: `crates/studio-core/src/large_scene/evaluate.rs`
- Modify: `crates/studio-project/src/geometry_cache.rs`
- Test: `crates/studio-server/tests/compute_scheduler.rs`

**Interfaces:**
- Produces server `ComputeService`
- Adds `Session.compute: Arc<ComputeService>`
- Adds `system.capabilities.compute: ComputeStatus`
- Adds cache metadata `backend: cpu|gpu`, `kernelVersion`, `parityStatus`
- Existing CPU evaluation remains default when GPU is unavailable.

- [ ] **Step 1: Add failing scheduler tests**

Test with a deterministic fake backend:

- `cpu` never invokes the GPU;
- `auto` uses CPU below the configured triangle threshold;
- `auto` uses GPU above threshold;
- forced `gpu` returns `ICE3_STUDIO_GPU_UNAVAILABLE`;
- a GPU kernel failure in `auto` reruns on CPU and returns `fallbackReason`;
- device loss preserves the committed revision.

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test -p studio-server --test compute_scheduler
```

Expected: missing `ComputeService` and capability fields.

- [ ] **Step 3: Implement scheduler policy**

Use one mutex-free read-mostly scheduler with:

```rust
pub const INITIAL_GPU_THRESHOLD_TRIANGLES: usize = 250_000;
pub const MAX_CONCURRENT_GPU_JOBS: usize = 2;
pub const DEFAULT_GPU_BUDGET_BYTES: u64 = 2 * 1024 * 1024 * 1024;
```

Priority ordering is Interactive, Throughput, Background. The scheduler checks estimated:

- triangles;
- upload/readback bytes;
- available device memory;
- queue depth;
- backend health;
- measured crossover data.

Do not run GPU work on the async request loop. Evaluation enters bounded blocking workers.

- [ ] **Step 4: Integrate into large-scene evaluation**

Add a backend-neutral optional evaluator hook used by `evaluate_large_scene_with_cache`. The hook
only accelerates geometry-producing part content; CPU remains responsible for parameter
resolution, identities, connections, validation, and canonical GLB.

Cache metadata records backend, kernel version, and parity state. CPU and GPU cache entries never
silently mix.

- [ ] **Step 5: Verify scheduler and server**

Run:

```bash
cargo test -p studio-server --test compute_scheduler
cargo test -p studio-server
cargo test -p studio-core
```

Expected: PASS, including forced fallback.

- [ ] **Step 6: Commit**

```bash
git add crates/studio-server/src/compute.rs crates/studio-server/src/lib.rs \
  crates/studio-server/src/rpc.rs crates/studio-core/src/large_scene/evaluate.rs \
  crates/studio-project/src/geometry_cache.rs crates/studio-server/tests/compute_scheduler.rs
git commit -m "feat(studio-server): schedule native GPU geometry work"
```

---

### 2.7 Task 7: Expose compute evidence and benchmarks

**Files:**
- Create: `benchmarks/native-gpu.mjs`
- Modify: `package.json`
- Modify: `scripts/verify.mjs`
- Modify: `apps/viewer/src/api/StudioClient.ts`
- Create: `apps/viewer/src/compute/ComputePanel.tsx`
- Modify: `apps/viewer/src/app/Workbench.tsx`
- Test: `apps/viewer/tests/compute-status.test.ts`
- Test: `crates/studio-server/tests/compute_scheduler.rs`

**Interfaces:**
- Produces `npm run benchmark:gpu`
- Produces `output/native-gpu-benchmark.json`
- Displays backend, tier, kernel time, transfer bytes, cache hits, memory, queue, and fallback.

- [ ] **Step 1: Add failing UI status test**

The formatter returns:

```javascript
{
  title: 'Native compute · GPU',
  detail: 'full · kernel 12.4 ms · 5120 KiB readback',
  state: 'ready'
}
```

for a ready GPU status, and a CPU fallback reason when unavailable.

- [ ] **Step 2: Implement status display**

Add a compact compute section beside Toolchain status. Display the selected backend, adapter tier,
memory budget, and fallback reason. Do not add controls that choose arbitrary adapters.

- [ ] **Step 3: Add the native benchmark**

The benchmark uses deterministic indexed meshes at 50k, 500k, and 2M triangles. Run two warmups and
nine measured samples per backend. Record:

- p50/p95 end-to-end latency;
- kernel time;
- queue wait;
- upload/readback bytes;
- cache hits;
- peak estimated memory;
- parity outcome;
- selected backend and tier.

Fail release when:

- topology or parity differs;
- either 500k or 2M fails to show at least 10% lower p50 end-to-end latency;
- GPU p95 is more than 5% worse than CPU;
- forced GPU failure changes committed state;
- memory or queue bounds are exceeded.

Add:

```json
"benchmark:gpu": "node benchmarks/native-gpu.mjs"
```

- [ ] **Step 4: Run UI and performance tests**

Run:

```bash
source $HOME/.nvm/nvm.sh && nvm use 24.22.1 >/dev/null
npm --prefix apps/viewer test
npm --prefix apps/viewer run typecheck
npm run benchmark:gpu
```

Expected: PASS with a native GPU; CPU-only CI verifies status and skip reporting.

- [ ] **Step 5: Commit**

```bash
git add benchmarks/native-gpu.mjs package.json scripts/verify.mjs \
  apps/viewer/src/api/StudioClient.ts apps/viewer/src/compute \
  apps/viewer/src/app/Workbench.tsx apps/viewer/tests/compute-status.test.ts \
  crates/studio-server/tests/compute_scheduler.rs
git commit -m "feat(studio-workbench): expose native GPU evidence"
```

---

### 2.8 Task 8: Update documentation and run the release gate

**Files:**
- Modify: `docs/specs/2026-10-30-native-gpu-compute-design.md`
- Modify: `docs/architecture.md`
- Modify: `docs/reference/api.md`
- Modify: `docs/reference/diagnostics.md`
- Modify: `docs/README.md`
- Modify: `docs/plans/2026-10-30-native-gpu-compute-backend.md`

- [ ] **Step 1: Document implemented behavior**

Document:

- supported kernels and their CPU parity;
- `auto`, `cpu`, and `gpu` host modes;
- thresholds and memory budgets;
- cache/backend identity;
- fallback and device-loss behavior;
- benchmark commands and report path;
- the distinction between native compute and browser rendering.

Update the native GPU spec to `Implemented` only when all acceptance tests and GPU benchmarks pass.

- [ ] **Step 2: Run all gates**

Run:

```bash
source $HOME/.nvm/nvm.sh && nvm use 24.22.1 >/dev/null
npm run verify
npm run benchmark
npm run benchmark:large-industrial
npm run benchmark:gpu
```

Expected: all functional gates pass and all performance gates fail closed when gains are absent.

- [ ] **Step 3: Audit and commit**

```bash
git status --short
git diff --check
rg -n "TBD|TODO|FIXME|PLACEHOLDER" docs crates apps tests benchmarks scripts || true
git add Cargo.toml Cargo.lock crates/studio-gpu docs apps benchmarks package.json scripts
git diff --cached --check
git commit -m "docs(studio): document native GPU compute"
```

Expected: no generated benchmark output, cache, customer asset, device identifier, or absolute path
is staged.

---

## 3. Plan Self-Review

### 3.1 Spec Coverage

| Spec area | Plan task |
|---|---|
| Backend-neutral asynchronous contract | 1 |
| Adapter/device lifecycle and structured failure | 2 |
| Initial primitive kernels | 3, 4 |
| Mesh normals and bounds | 5 |
| CPU parity and canonical CPU export | 3–5, 8 |
| CPU/GPU scheduling and fallback | 6 |
| Priorities, memory, cancellation, device loss | 6, 7 |
| Public capability and Workbench evidence | 7 |
| 50k, 500k, 2M and incremental benchmarks | 7 |
| Security and host-owned policy | 1, 2, 6 |

### 3.2 Type Consistency

- `GeometryBackendKind`, `GeometryBackendMode`, and batch types are introduced once in Task 1.
- Every later task uses `EvaluatedMesh` and `Diagnostic` from Core.
- Cache APIs remain in `studio-project`; Core continues to define backend-neutral types.
- `ComputeService` is the only Server-side scheduler and does not leak into Core.

### 3.3 Scope Check

This plan ends with independently usable native GPU compute, fallback, evidence, and performance
gates. Runtime Tileset publication remains M4.
