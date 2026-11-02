# ICE3 Studio GPU-Resident Geometry Design

> Document ID: ICE3-STUDIO-SPEC-006
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

## 1. Summary

ICE3 Studio currently evaluates derived geometry through both CPU and native GPU backends, but the
native GPU path constructs adjacency data, allocates buffers, compiles or binds pipelines, waits,
and reads every result back to the CPU for each evaluation. The Workbench then receives a complete
Toolchain GLB and the browser uploads newly created geometry objects again.

This design keeps derived geometry resident on both sides of the process boundary:

1. the native GPU backend reuses pipelines, topology, buffers, and derived outputs across
   evaluations and candidates;
2. the server distinguishes GPU-resident work from explicit CPU snapshots;
3. the browser receives a compact revision-aware geometry packet and updates existing WebGL2 or
   WebGPU buffers instead of replacing the complete scene.

The Document, operation history, validation, approval, revision rules, persistence, and canonical
source GLB remain CPU-authoritative. GPU residency is a replaceable cache of derived data.

## 2. Motivation

The current native GPU path is correct but does not expose the hardware's strengths:

- each custom-mesh evaluation builds the vertex-to-triangle adjacency again;
- each evaluation creates transient buffers and command resources;
- shader and compute pipeline lifecycle is not persistent;
- every result is staged, mapped, and assembled into a CPU mesh;
- the browser replaces a complete GLB and creates new geometry objects after each revision;
- unchanged topology and attributes are uploaded repeatedly.

Dense parallel computation cannot repay those repeated setup, synchronization, and transfer
costs. The measured 500k and 2M workloads pass CPU parity but remain slower end to end than the CPU
reference on the current integrated adapter. The optimization must therefore target resource
lifecycle and data movement, not only shader instruction count.

## 3. Goals

1. Reuse shader modules, pipeline layouts, compute pipelines, bind groups, and common buffers for
   the lifetime of a healthy device.
2. Keep topology, derived normals, bounds, and reusable output buffers resident on the native GPU.
3. Update only geometry channels affected by a document or parameter change.
4. Execute compatible nodes in one command stream and defer synchronization until a required
   result boundary.
5. Separate GPU-resident evaluation from explicit CPU-readable snapshots.
6. Deliver revision-aware geometry packets to the Workbench without replacing unchanged browser
   GPU buffers.
7. Preserve CPU canonical evaluation, parity checks, deterministic export, and safe fallback.
8. Quantify cold, warm-topology, resident, preview, and snapshot latency separately.
9. Enforce host-owned memory, queue, concurrency, and cancellation policy.

## 4. Non-Goals

- Giving GPU caches authority over the Document or approved ChangeSet identity.
- Treating cache hits or backend identity as part of semantic candidate digests.
- Sharing native process GPU allocations directly with the browser.
- Requiring browser WebGPU; WebGL2 remains the production browser default.
- Making GPU mandatory for project access, validation, commit, or export.
- Using arbitrary request-supplied shaders, device IDs, pointers, or backend commands.
- Moving CPU validation, authorization, persistence, or transaction control to GPU code.
- Implementing boolean geometry, sculpting, simulation, or external asset import.
- Replacing the Toolchain's prepared-asset, partition, or LOD responsibilities.

## 5. Design Principles

### 5.1 CPU owns semantics

CPU code resolves parameters, validates structure, assigns identities, controls revisions and
approval, chooses canonical export input, and decides whether a GPU failure is recoverable.

### Implementation status

The resident backend, explicit snapshots, server scheduler and packet endpoints, `ICE3PGP1`
transport, browser parser, stable geometry identity, WebGL2/WebGPU range uploads, and Workbench
fallback are implemented. The hot-path redesign removed whole-mesh serialization and hashing from
repeat lookups: at 500k and 2M triangles the handle hit recorded zero extra full hashes, uploads,
and fences, while cold GPU remained 11.78 ms and 31.31 ms versus CPU 2.68 ms and 9.17 ms. The
resident handle timing is below timer resolution and is cache-hit evidence, not a new kernel-speed
claim. Cold and warm paths remain slower, so native residency and preview packets remain
host-opt-in and `auto` stays on CPU.

### 5.2 Runtime resources are long-lived

Pipelines and topology-specific resources are created once per compatible device and input
generation, then reused until invalidation, budget pressure, device loss, or shutdown.

### 5.3 Derived data may remain GPU-resident

Internal operations that consume GPU outputs can continue on GPU. A CPU mesh is a projection
created only at an explicit snapshot, export, persistence, or compatibility boundary.

### 5.4 Measured modes

Cold execution, warm-topology execution, resident execution, preview synchronization, and CPU
snapshot each have independent performance evidence. A fast resident path does not justify
selecting GPU for cold work that remains slower.

### 5.5 Atomic visibility

A revision or candidate becomes visible only after all required GPU work and parity checks finish.
Partial GPU writes, failed uploads, or device loss leave the previous complete representation
active.

### 5.6 Stable identities survive transport

Geometry IDs, node IDs, topology digests, and revision relationships remain stable across the
native-to-browser boundary. Cache identity is independent of memory addresses and process-local
handles.

## 6. Architecture

```text
Document / resolved operations
             |
             v
CPU semantic authority + validation + cache identity
             |
             v
ComputeService scheduler
             |
             v
GPU resource manager
  pipelines + buffer pool + staging ring
             |
             v
GpuGeometryResidency
  topology + attributes + normals + bounds
             |
             +----> GPU consumers (bounds, culling, future LOD)
             |
             +----> explicit CPU snapshot / canonical export
             |
             +----> PreviewGeometryPacket
                         |
                         v
                Server revision endpoint
                         |
                         v
               Browser geometry residency
                         |
                         v
                  WebGL2 / WebGPU
```

Native and browser GPU buffers are not zero-copy shared. The process boundary carries a compact
delta packet. Both sides independently reuse their device-local buffers.

## 7. Native GPU Resource Management

### 7.1 Device resource set

`studio-gpu` owns one device-scoped resource set containing:

- shader modules;
- explicit bind-group layouts;
- pipeline layouts;
- compute pipelines for each kernel and stride variant;
- common empty/dummy buffers;
- a bounded staging ring;
- reusable capacity-class buffers;
- the residency index.

Resources are initialized once and warmed in the background. First-use compilation is a fallback,
not the expected path.

### 7.2 Buffer classes

Buffers declare one of these roles:

- `StaticTopology`: positions, indices, adjacency offsets/lists, and other data reused until
  topology changes.
- `DynamicAttribute`: positions or attributes updated by a partial write.
- `DeviceOutput`: normals, bounds, visibility, and future derived data normally consumed on GPU.
- `ReadbackStaging`: host-mapped staging used only for snapshots or preview transport.
- `Command`: indirect dispatch and render command data.

The resource manager may suballocate compatible ranges from larger allocations. Reuse is protected
by GPU completion tracking so a released range is not overwritten while still in flight.

### 7.3 Capacity classes

Allocation sizes are rounded into bounded classes rather than creating arbitrary per-job sizes.
The initial policy uses 1 MiB, 4 MiB, 16 MiB, 64 MiB, and 256 MiB classes, with an oversized
single-allocation path subject to the current host and adapter budget.

### 7.4 Staging ring

Readback uses a ring of two or three staging slots. A submission records the slot, completion
fence, job ID, revision, and payload range. The slot cannot be reused until completion. Resident
jobs do not reserve a staging slot.

## 8. Residency Model

### 8.1 Keys

Residency uses separate keys for independent invalidation:

- `topologyKey`: operation kind, topology-defining parameters, canonical indices, dependency
  topology digest, kernel name, and kernel version.
- `attributeKey`: numeric dimensions, resolved parameters, positions digest, material-independent
  source data digest, and kernel version.
- `resultKey`: topology key, attribute key, backend compatibility version, and parity version.
- `presentationKey`: result key plus browser transport version and export profile.

Cache keys never include process addresses, adapter serial numbers, local paths, or revision
timestamps.

### 8.2 Dirty channels

Each residency records a channel bitset:

- `topology`
- `positions`
- `indices`
- `normals`
- `bounds`
- `visibility`
- `material`
- `transform`

The scheduler computes this set from operation semantics and dependency digests before allocating
or uploading.

### 8.3 Kernel invalidation

Initial dirty-channel rules are:

| Change | Topology | Positions | Normals | Bounds |
|---|---:|---:|---:|---:|
| Box dimensions | unchanged | update | unchanged | update |
| Plane dimensions | unchanged | update | unchanged | update |
| Cylinder radius/height | unchanged | update | unchanged | update |
| Cylinder segments | rebuild | rebuild | rebuild | update |
| Sphere radius | unchanged | update | unchanged | update |
| Sphere segments/rings | rebuild | rebuild | rebuild | update |
| Sweep radius | unchanged | update | rebuild | update |
| Sweep points/segments | rebuild as required | rebuild | rebuild | update |
| Custom-mesh positions | unchanged | update | rebuild | update |
| Custom-mesh indices | rebuild | unchanged | rebuild | unchanged |
| Node transform | unchanged | unchanged | unchanged | world bounds only |
| Material | unchanged | unchanged | unchanged | unchanged |

These rules are performance hints only. Structural validation and parity checks remain mandatory.

### 8.4 Residency states

- `loading`: buffers are being created or updated; not externally visible.
- `ready`: complete and eligible for GPU consumers.
- `stale`: a dependency changed and the entry is no longer selectable.
- `evicting`: no new users; waiting for in-flight references.
- `lost`: invalidated by device loss.

### 8.5 Ownership and eviction

- Current revision and active candidate entries are pinned.
- In-flight jobs add temporary references.
- Unpinned entries follow LRU eviction inside the configured byte budget.
- Oversized entries are rejected before allocation.
- Eviction never changes Document or revision state.

## 9. Evaluation Modes

### 9.1 Cold

No compatible topology or attribute residency exists. The backend:

1. validates input on CPU;
2. creates or reuses capacity-class buffers;
3. uploads topology and attributes;
4. builds adjacency where required;
5. executes all compatible GPU passes;
6. stores outputs as resident.

Cold mode is measured separately and may remain slower than CPU.

### 9.2 Warm topology

A compatible topology entry exists, but attributes changed. The backend performs partial buffer
writes, reruns only necessary kernels, and updates bounds or other dependent outputs.

### 9.3 Resident

Required inputs and outputs are already resident. A job submits GPU work without CPU readback.
Consumers may use the result handle directly.

### 9.4 Snapshot

An explicit caller requests a CPU-readable mesh. The backend copies only required stages into
staging, waits on a fence, validates topology and parity metadata, and returns `EvaluatedMesh`.
Snapshot timing is reported separately and never counted as resident kernel latency.

## 10. Command Execution

### 10.1 Batch planning

`ComputeService` groups jobs by:

- kernel family;
- compatible pipeline and buffer layout;
- priority;
- result visibility revision;
- memory admission class.

Compatible jobs share command encoding, pipeline bindings, and staging completion where possible.

### 10.2 Pass ordering

For custom meshes:

1. upload topology or changed positions;
2. face-normal generation;
3. vertex-normal accumulation;
4. bounds reduction;
5. optional GPU consumers;
6. one readback only if a snapshot is requested.

Intermediate passes use GPU memory barriers and remain in the same encoded command sequence.

### 10.3 Non-blocking completion

Resident jobs record completion and publish readiness through a fence or channel. They do not
block the request thread. Snapshot and export jobs wait only at the minimum required boundary.

### 10.4 Dispatch scaling

Logical work is mapped to one-, two-, or three-dimensional dispatch grids within adapter limits.
Indirect dispatch is introduced for GPU-produced work counts and batch consumers. Known-size
primitive kernels may continue with direct dispatch where it is simpler and measured faster.

### 10.5 Cancellation

- Queued jobs remove their pending reservation immediately.
- Running jobs stop publishing results when cancellation is observed.
- Lower-priority background jobs release residency references after the current GPU packet.
- The API never reports a cancelled partial result as ready.

## 11. Server Integration

### 11.1 ComputeService

`ComputeService` remains the only scheduling authority. It:

- estimates triangle and byte counts;
- selects cold, warm, or resident GPU mode;
- applies host and adapter budgets;
- pins the current revision and active candidates;
- maintains priority and concurrency limits;
- decides when a failed GPU path may rerun on CPU;
- records mode-specific evidence.

### 11.2 Core hook

The existing backend-neutral evaluator hook continues to consume resolved `NodeContent`.
Document mutation, parameter resolution, topology validation, dependency digests, and cache
identity remain outside `studio-gpu`.

### 11.3 Snapshot policy

CPU meshes are produced for:

- canonical source GLB export;
- persistence or tests that explicitly require an evaluated mesh;
- a preview transport that has no compatible resident path;
- explicit diagnostics requiring CPU comparison.

Normal preview preparation consumes resident metadata and output buffers without requesting a
full CPU mesh.

## 12. Preview Geometry Packet

### 12.1 Purpose

The existing preview path serializes the complete scene to GLB, runs Toolchain preparation, and
constructs new browser geometry objects. The optimized path adds a separate local preview packet
for Workbench consumption. GLB remains the interoperable and canonical asset path.

### 12.2 Container

`ICE3PGP1` is a local binary container containing:

- a JSON manifest;
- one aligned binary chunk;
- optional delta chunks;
- a content digest.

The manifest includes:

- schema and transport version;
- revision and source digest;
- backend and residency mode evidence;
- node records with stable IDs, matrices, materials, and geometry IDs;
- geometry records with topology and attribute digests;
- element counts, index width, byte ranges, and dirty ranges;
- a parent packet revision when incremental.

### 12.3 Delta rules

- Unchanged geometry IDs are omitted from binary payloads.
- Transform-only changes send node matrices only.
- Material-only changes send material records only.
- Attribute changes send dirty byte ranges.
- Topology changes send a complete replacement geometry record.
- Removal is explicit and acknowledged; omission never implies deletion.

### 12.4 Browser behavior

The browser runtime keyed by stable `geometryId` rather than object address:

1. validates packet schema, ranges, digests, and limits;
2. acquires or creates a browser geometry record;
3. writes changed ranges with `bufferSubData` or `queue.writeBuffer`;
4. swaps a new buffer generation only when capacity or topology changes;
5. publishes the new scene atomically after every required upload succeeds.

On validation or upload failure, the previous complete scene remains active.

### 12.5 Compatibility

The server advertises packet support through capabilities. The Workbench requests it explicitly
through a versioned media type. Unsupported clients, Toolchain-only features, invalid packets, or
browser budget failures use the existing GLB preview path.

## 13. Failure and Recovery

### 13.1 Device loss

- Stop new native GPU submissions.
- Mark all residency handles `lost`.
- Release pipelines, bind groups, buffers, and staging slots after safe teardown.
- Preserve Document, revision, approved candidate, and last complete browser packet.
- Recreate device resources once if policy allows.
- Rerun affected candidates on CPU when mode is `auto`.

### 13.2 Partial upload

A packet becomes visible only after all dirty ranges and metadata validate. Failed partial upload
rolls back to the previous geometry generation.

### 13.3 Cache corruption

Invalid cache keys, digest mismatch, impossible counts, invalid indices, non-finite values, or
parity mismatch evict the entry and force CPU reference evaluation.

### 13.4 Budget pressure

Admission first cancels unpinned background work, then evicts LRU residency, then runs eligible
auto-mode jobs on CPU. Oversized forced-GPU jobs fail with
`ICE3_STUDIO_GPU_BUDGET_EXCEEDED`.

### 13.5 Browser unsupported

The Workbench reports packet incompatibility as non-fatal and uses GLB. Existing WebGL2 and
experimental browser WebGPU behavior must remain unchanged.

## 14. Correctness

- Input and output indices must remain valid and topologically identical to CPU reference output.
- Positions, normals, and bounds retain the existing finite-value and tolerance contracts.
- GPU output never changes semantic candidate identity.
- Dirty-channel acceleration may skip unaffected computation but cannot skip structural
  validation.
- Browser packets are derived transport data, not Document source.
- Canonical source GLB remains CPU-generated.
- CPU snapshot, GPU residency, disk cache, and browser cache have separate identities and cannot
  be silently mixed.

## 15. Observability

Compute status adds:

- latest evaluation mode: `cold`, `warm`, `resident`, or `snapshot`;
- resident geometry count and bytes;
- peak bytes and eviction count;
- pipeline compilation and warm-up state;
- queue wait and in-flight count;
- upload, partial-update, and readback bytes;
- resident cache hits and topology hits;
- preview packet full/delta bytes;
- browser upload/reuse counters where exposed by the Workbench.

No raw adapter identifiers, device handles, addresses, or filesystem paths are exposed.

## 16. Performance Evidence

### 16.1 Workloads

Use deterministic indexed meshes at 50k, 500k, and 2M triangles:

- two warm-up runs;
- nine measured samples;
- cold, warm-topology, resident, snapshot, and preview modes;
- identical CPU reference topology and value tolerances.

### 16.2 Release gates

- Resident GPU p50 is at least 10% lower than CPU at 500k and 2M triangles.
- Resident GPU p95 is no more than 5% worse than CPU.
- Cold mode may select CPU when it is slower; it must not regress Auto selection.
- Warm-topology upload bytes are lower than a cold upload.
- Transform-only and material-only updates cause no geometry readback.
- A one-parameter update produces a preview packet substantially smaller than a full GLB.
- Browser geometry upload count stays zero when only node transforms change.
- Browser geometry upload bytes reflect only dirty ranges when attributes change.
- Memory, queue, cancellation, parity, and device-loss gates pass.

### 16.3 Output

`npm run benchmark:gpu` writes machine-readable evidence under `output/`, including mode-specific
p50/p95, bytes, parity, cache state, and adapter tier. `npm run verify` retains functional gates;
performance release stays a separate fail-closed command.

## 17. Security and Policy

- Packet media types, schema versions, count limits, byte limits, and digest validation occur
  before allocation.
- No API request can select arbitrary adapters, shaders, pointers, cache paths, or binary layouts.
- Packet payloads never contain local absolute paths.
- Host mode and memory policy remain startup configuration.
- Existing bearer authentication and loopback binding govern packet endpoints.
- Toolchain remains the only prepared-asset process boundary.

## 18. Compatibility and Rollout

- Existing RPC methods and GLB URLs remain available.
- Native GPU remains experimental and CPU remains the default until all release gates pass.
- Browser WebGL2 remains default; browser WebGPU remains explicitly selected.
- Packet support is capability-negotiated and can be disabled by host policy.
- The implementation may enable resident execution independently from packet transport, but the
  final delivered milestone includes both.
- No schema migration is required for Document data.

## 19. Acceptance

The feature is complete when:

1. persistent pipelines and capacity-class buffers eliminate per-job resource construction;
2. topology and outputs survive multiple evaluations without re-upload;
3. dirty-channel updates pass CPU parity;
4. compatible batch jobs share one command stream and avoid per-node readback;
5. snapshot and resident modes have separate APIs and evidence;
6. device loss, cancellation, budget pressure, and cache corruption preserve the last committed
   revision;
7. Workbench receives and applies full and delta geometry packets;
8. transform-only revisions reuse browser geometry buffers;
9. attribute revisions update only dirty browser ranges;
10. GLB fallback and canonical export remain functional;
11. the documented resident, warm, preview, and browser performance gates pass;
12. root verification, existing benchmarks, browser tests, and dependency audit pass.

If resident performance still does not beat the CPU reference on a target adapter, Auto must keep
selecting CPU and the feature remains experimental rather than weakening the gate.
