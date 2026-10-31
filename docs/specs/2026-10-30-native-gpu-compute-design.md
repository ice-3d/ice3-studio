# ICE3 Studio Native GPU Compute Design

> Document ID: ICE3-STUDIO-SPEC-003
>
> Status: Experimental
>
> Owner: ICE3 Studio maintainers

## 1. Summary

ICE3 Studio adds a native GPU compute backend for derived geometry work. The backend uses Rust
and `wgpu` to access the local GPU without a browser, making large modeling jobs, incremental
ChangeSet evaluation, and parallel AI candidate comparison faster.

The committed Document, Operations, validation results, revisions, permissions, and persistence
remain controlled by CPU code. GPU output is a derived, replaceable Geometry result. It is never
the source of truth.

The existing browser WebGPU renderer remains separate. Rendering with GPU hardware does not count
as Studio Core GPU acceleration.

### Implementation Status

The backend-neutral contract, native `wgpu` device lifecycle, Box/Plane/Cylinder/Sphere/Sweep
geometry kernels, custom-mesh normals and bounds, CPU/GPU scheduler, safe fallback, cache identity,
resident geometry, public capability evidence, and Workbench panel are implemented. The latest
mode-specific benchmark measured 500k triangles at 2.68 ms CPU, 11.78 ms cold GPU, and a sub-timer
resolution resident-handle hit; 2M triangles measured 9.17 ms CPU, 31.31 ms cold GPU, and a
sub-timer resolution resident-handle hit. The resident path reported zero additional full hashes,
uploads, or fences after registration. All tested workloads passed topology, finite-value, normal,
and bounds parity. Cold and warm paths remain slower than CPU on the current integrated adapter,
so automatic dispatch remains on the CPU reference and native GPU execution remains experimental.

Resource-resident execution, dirty-channel updates, batched submission, and incremental browser
delivery are defined separately by `ICE3-STUDIO-SPEC-006`.

Delivered host behavior:

- `cpu` never initializes or invokes native GPU work.
- `gpu` requests native compute and returns a structured `ICE3_STUDIO_GPU_*` diagnostic when the
  request cannot be satisfied.
- `auto` requires a measured crossover, a healthy adapter, parity, and a job below both host and
  adapter memory budgets. Current defaults intentionally keep its threshold above practical jobs
  until a positive crossover is measured.
- GPU admission is limited to two concurrent jobs, orders Interactive before Throughput before
  Background, waits outside the async request loop, and falls back to CPU after auto-mode failure.
- Supported kernels are Box, Plane, Cylinder, Sphere, Sweep, and custom indexed mesh face-normal
  accumulation plus per-workgroup bounds reduction.
- Cache keys include resolved geometry content, part and dependency digests, backend, kernel
  version, and parity state. CPU and GPU entries are independently keyed.
- `npm run benchmark:gpu` writes `output/native-gpu-benchmark.json` and fails closed unless 500k
  and 2M workloads improve p50 by at least 10% while GPU p95 remains within 5% of CPU.

## 2. Goals

1. Use a local native GPU for high-volume geometry evaluation.
2. Improve AI feedback loops by reducing Operation-to-candidate-preview latency.
3. Evaluate multiple ChangeSet candidates efficiently through shared GPU resources.
4. Reuse unchanged geometry and upload only incremental changes.
5. Preserve CPU reference behavior, data integrity, and deterministic committed output.
6. Expose measurable backend and kernel evidence to API clients and the Workbench.
7. Select CPU or GPU from measured thresholds instead of labels or assumptions.

## 3. Non-Goals

- Giving GPU memory or asynchronous jobs authority over the Document.
- Making GPU availability mandatory for project access or revision commits.
- Moving persistence, authentication, validation policy, or transaction control to GPU code.
- Using browser WebGPU rendering as a substitute for native compute.
- Supporting arbitrary request-supplied shaders, device IDs, or backend commands.
- Requiring bit-identical floating-point output across different hardware vendors.
- Implementing boolean geometry, sculpting, NURBS, or simulation in this phase.
- Changing the external Toolchain's Runtime LOD and partitioning responsibilities.

## 4. Design Principles

### 4.1 CPU owns semantics

CPU code owns IDs, document mutation, validation, revision rules, approval, persistence, and
canonical source GLB export.

### 4.2 GPU owns parallel derived work

GPU kernels generate and transform derived geometry, normals, bounds, visibility, and future
large-scale spatial operations.

### 4.3 Measured dispatch

The scheduler selects a backend from workload size, current device state, memory pressure, and
measured crossover thresholds. Detecting a GPU does not imply every operation uses it.

### 4.4 Safe fallback

Unavailable, incompatible, or failed GPU execution returns a structured diagnostic and uses CPU
reference evaluation unless the caller explicitly requested GPU-only execution.

### 4.5 Exact review identity

GPU behavior cannot change a ChangeSet's semantic candidate digest. Geometry cache metadata must
identify the backend and backend-versioned digest without contaminating Document identity.

## 5. System Context

```text
AI Agent / Workbench
        |
        | ChangeSet and Operation API
        v
Rust Studio Server
        |
        +------------------------+
        |                        |
        v                        v
CPU reference evaluator      Native GPU backend
canonical geometry           wgpu adapter/device
        |                        |
        +-----------+------------+
                    |
                    v
          derived Geometry cache
                    |
                    +----> candidate preview
                    |
                    +----> Workbench backend evidence
                    |
                    +----> optional GPU-assisted export

Committed Document / validation / revision / persistence remain on CPU.
```

## 6. Crate Boundaries

### 6.1 `studio-gpu`

New Rust crate responsible for:

- local adapter/device initialization;
- backend capability discovery;
- memory and feature budgets;
- shader/kernel registration;
- command submission and result readback;
- device-loss recovery;
- backend performance metrics;
- GPU resource pooling;
- CPU/GPU parity checks.

`studio-gpu` does not read project files, accept network requests, sign sessions, commit
transactions, or depend on Viewer code.

### 6.2 `studio-core`

Continues to own:

- Document and Operation types;
- CPU geometry reference implementation;
- validation;
- candidate digest semantics;
- canonical GLB export input.

It exposes a backend-neutral geometry request/result contract without depending on `wgpu`.

### 6.3 `studio-server`

Owns the scheduler integration:

- estimates work before evaluation;
- chooses CPU/GPU/default mode;
- batches compatible jobs;
- applies priorities and cancellation;
- records backend evidence in ChangeSet evaluation;
- reports GPU status and fallback diagnostics.

### 6.4 Workbench

Displays:

- selected backend;
- adapter name and acceleration tier;
- kernel and upload/readback time;
- GPU memory pressure;
- fallback reason;
- current queue state for parallel candidates.

The Workbench cannot choose arbitrary adapters or submit shader code.

## 7. Geometry Backend Contract

The backend-neutral contract is asynchronous:

```rust
#[async_trait]
pub trait GeometryBackend {
    async fn initialize(&self) -> Result<BackendCapabilities, BackendError>;
    async fn evaluate(&self, batch: GeometryBatch) -> Result<GeometryBatchResult, BackendError>;
    async fn cancel(&self, job_id: JobId) -> Result<(), BackendError>;
    fn stats(&self) -> BackendStats;
}
```

### 7.1 `GeometryBatch`

```rust
pub struct GeometryBatch {
    pub job_id: JobId,
    pub priority: JobPriority,
    pub budget_bytes: u64,
    pub nodes: Vec<GeometryJobNode>,
}

pub struct GeometryJobNode {
    pub node_id: NodeId,
    pub content: NodeContent,
    pub content_digest: String,
    pub dependency_digest: String,
    pub cache_key: String,
}
```

### 7.2 `GeometryBatchResult`

```rust
pub struct GeometryBatchResult {
    pub job_id: JobId,
    pub backend: GeometryBackendKind,
    pub meshes: BTreeMap<NodeId, EvaluatedMesh>,
    pub stats: GeometryExecutionStats,
    pub diagnostics: Vec<Diagnostic>,
}
```

`GeometryExecutionStats` includes CPU time, upload bytes, kernel time, readback bytes, queue wait,
GPU memory estimate, cache hits, and fallback reason.

### 7.3 Initial Kernels

The first GPU implementation covers:

- Box generation and normals.
- Plane generation and normals.
- Cylinder side walls and caps.
- Sphere generation and normals.
- Polyline Sweep ring generation, normals, and caps.
- Mesh normal accumulation.
- Mesh bounds reduction.

TriangleMesh index validation and structural document validation remain on CPU.

## 8. Scheduling

### 8.1 Job Priorities

- `Interactive`: active ChangeSet candidate, lowest latency.
- `Throughput`: AI-generated parallel proposals.
- `Background`: full preview preparation, export cache, and prewarming.

Interactive jobs preempt or cancel queued background work when memory or queue capacity is
constrained.

### 8.2 Backend Selection

Host mode:

- `auto`: execute each job on the estimated faster backend.
- `cpu`: force CPU reference behavior.
- `gpu`: request GPU and fail with a structured diagnostic if unavailable.

`auto` inputs:

- operation and output element counts;
- estimated upload/readback bytes;
- number of independent nodes;
- GPU queue depth and memory pressure;
- backend health;
- measured crossover thresholds for the current adapter.

Initial thresholds are configuration defaults. Release evidence updates them per adapter tier;
requests cannot override host policy.

### 8.3 Backpressure

When GPU work exceeds memory or queue budgets:

1. cancel lower-priority incomplete work;
2. reject oversized jobs with `ICE3_STUDIO_GPU_BUDGET_EXCEEDED`;
3. run eligible jobs on CPU when mode is `auto`;
4. never block loopback health, reads, or revision-event delivery.

## 9. Incremental Evaluation

Cache identity is a hash of:

- node content;
- content version;
- relevant transform values;
- parent-chain digest;
- kernel name and version;
- backend compatibility version.

Rules:

- Unchanged nodes reuse cached result metadata and GPU buffers.
- A changed primitive uploads parameters, not the unchanged scene.
- Parent changes invalidate descendant world bounds and dependent constraints.
- Material-only changes do not re-evaluate unchanged geometry.
- Multiple candidates from one base revision share immutable buffers.
- Cache eviction follows LRU within a configured memory budget.
- CPU and GPU caches are independently keyed and cannot be silently mixed.

## 10. Memory Model

- Host configuration defines total and per-job GPU budgets.
- Admission control estimates bytes before creating GPU buffers.
- Persistent shared scene buffers and transient job buffers are accounted separately.
- Readback staging buffers use a bounded pool.
- Oversized results are streamed in chunks or fall back to CPU.
- Device loss invalidates all GPU caches but preserves committed CPU state and disk data.
- GPU memory statistics are exposed in API evidence and Workbench diagnostics.

## 11. Device Lifecycle

Startup:

1. enumerate adapters;
2. request compatible features and limits;
3. initialize kernel pipelines;
4. run a small parity/health job;
5. publish capabilities and selected adapter.

Device loss:

1. mark active GPU jobs failed with `ICE3_STUDIO_GPU_DEVICE_LOST`;
2. invalidate GPU caches;
3. preserve current Document and committed previews;
4. reconnect using bounded backoff;
5. optionally rerun affected candidates on CPU;
6. publish recovery state.

No GPU operation may partially commit a ChangeSet.

## 12. Correctness and Determinism

### 12.1 CPU Reference

CPU evaluation remains the normative implementation for parity tests and canonical source GLB.

### 12.2 Geometry Parity

For each kernel:

- indexed triangle count must match;
- index topology must match;
- vertex and normal position error must remain below `1e-5` for normalized primitives;
- bounds error must remain below `1e-5` relative to scene scale;
- no NaN or infinite values are accepted.

### 12.3 Candidate Identity

ChangeSet candidate digest remains based on base revision and semantic Operations. Backend
selection, GPU adapter, queue timing, and derived mesh bytes do not alter approval identity.

### 12.4 Export

Candidate previews may use GPU results. Canonical source GLB export defaults to CPU reference
geometry for cross-device reproducibility. GPU-assisted canonical export requires a later explicit
decision with byte-stability evidence.

## 13. Diagnostics

New stable codes:

- `ICE3_STUDIO_GPU_UNAVAILABLE`
- `ICE3_STUDIO_GPU_INCOMPATIBLE`
- `ICE3_STUDIO_GPU_BUDGET_EXCEEDED`
- `ICE3_STUDIO_GPU_KERNEL_FAILED`
- `ICE3_STUDIO_GPU_DEVICE_LOST`
- `ICE3_STUDIO_GPU_PARITY_FAILED`

Diagnostics include backend, node IDs, kernel name, budget values, and recovery action, but never
credentials or out-of-workspace paths.

## 14. Public Status

Read-only capabilities include:

```json
{
  "mode": "auto",
  "selectedBackend": "gpu",
  "adapter": "system GPU",
  "tier": "full",
  "memoryBudgetBytes": 2147483648,
  "health": "ready",
  "fallbackReason": null
}
```

Candidate evaluation evidence adds:

```json
{
  "backend": "gpu",
  "cpuMs": 0.0,
  "uploadBytes": 0,
  "kernelMs": 0.0,
  "readbackBytes": 0,
  "queueWaitMs": 0,
  "cacheHits": 0,
  "fallbackReason": null
}
```

Actual adapter descriptions must not expose local machine identifiers beyond what is necessary for
diagnosis.

## 15. Performance Verification

Benchmark workloads:

| Workload | Purpose |
|---|---|
| 50k triangles | Measure launch and upload overhead |
| 500k triangles | Establish expected GPU crossover |
| 2M triangles | Measure kernel throughput and memory |
| 100 Operations, one candidate | End-to-end interaction latency |
| 10 parallel candidates | Candidate throughput and fairness |
| 1% incremental node change | Upload reuse and cache effectiveness |

Required metrics:

- CPU and GPU p50/p95;
- upload and readback bytes;
- kernel and queue time;
- cache hit ratio;
- peak GPU memory estimate;
- cancellation and fallback count;
- full Output JSON with adapter tier and backend version.

The release gate fails if:

- 500k or higher workloads show no stable end-to-end benefit under `auto`;
- GPU causes correctness parity failures;
- GPU failure changes committed state;
- parallel candidates starve interactive work;
- memory admission control is bypassed.

## 16. Security

- GPU kernels are compiled into Studio; requests cannot submit WGSL or source code.
- Adapter and backend choices come from host configuration.
- GPU diagnostics cannot reveal credentials or arbitrary filesystem paths.
- Device features and limits are capped by host policy.
- A GPU failure cannot bypass API authorization, validation, approval digest, or idempotency.
- Browser and Agent clients receive read-only backend evidence.

## 17. Testing

### 17.1 CPU-only Tests

- scheduler thresholds and mode behavior;
- cache-key and dependency invalidation;
- memory admission and backpressure;
- cancellation and stale-job handling;
- fallback diagnostics;
- API capability and evidence schemas.

### 17.2 GPU Tests

- adapter and device initialization;
- parity for every initial kernel;
- 500k and 2M synthetic mesh evaluation;
- incremental upload reuse;
- parallel candidate jobs;
- forced device-loss simulation;
- oversized job rejection and CPU fallback;
- memory budget accounting.

GPU tests detect capability at runtime. The current Apple Silicon development machine is a hard
release acceptance target; CPU-only CI continues to run scheduler and contract tests.

### 17.3 End-to-End

- AI creates a large generic project.
- The server selects GPU under `auto`.
- Candidate preview is produced from GPU geometry.
- Workbench displays GPU evidence.
- Approval remains bound to the semantic candidate digest.
- CPU reference export remains stable.
- Device loss leaves the committed project unchanged and recoverable.

## 18. Acceptance Criteria

1. Native Studio computes large geometry on GPU without a browser.
2. `auto` selects GPU only when measured workload and memory conditions justify it.
3. Initial geometry kernels pass CPU parity.
4. Large-scene preview latency and parallel candidate throughput improve measurably.
5. Current Document, validation, revision identity, approval, and persistence remain CPU-owned.
6. GPU failure falls back safely and never modifies committed state.
7. Workbench and API expose real backend/kernel evidence.
8. Release benchmark fails when expected large-workload gains are absent.
9. No request can inject shaders, select arbitrary adapters, or bypass security controls.
10. Existing browser rendering behavior remains independent and backward compatible.

## 19. Risks and Mitigations

| Risk | Mitigation |
|---|---|
| GPU slower for small jobs | measured crossover and CPU dispatch |
| Cross-device output differences | tolerance parity and CPU canonical export |
| Device loss corrupts in-flight work | jobs are derived; no GPU commit authority |
| Memory exhaustion | admission control, pooling, backpressure |
| Complexity leaks into Core | backend-neutral contract and isolated `studio-gpu` crate |
| GPU branch exists but is not used | benchmark evidence and automatic dispatch thresholds |
| Browser GPU mistaken for native compute | separate capability and status reporting |

## 20. Reference Decisions

- [Document as Source of Truth](../decisions/0001-document-as-source-of-truth.md)
- [Local API Boundary](../decisions/0002-local-api-boundary.md)
- [MVP Specification](2026-10-30-ai-native-studio-mvp-design.md)
- [Workbench Specification](2026-10-30-studio-web-workbench-design.md)
- [Architecture](../architecture.md)

## 21. Open Implementation Decisions

No product or architecture decision remains open. Exact kernel shader structure, runtime threshold
defaults, and wgpu version pinning belong to the implementation plan and measured release baseline.
