# ICE3 Studio Revision-Scoped GPU Scene Session

> Document ID: ICE3-STUDIO-SPEC-008
>
> Status: Historical
>
> Owner: ICE3 Studio maintainers

> **历史状态：** 本文保留已完成设计过程的审计记录，不代表当前生产依赖；当前能力以
> [Asset Pipeline](../reference/asset-pipeline.md) 和 [能力清单](../reference/capabilities.md)
> 为准。

## 1. Summary

ICE3 Studio currently keeps individual evaluated meshes resident, but the large-scene path still
reconstructs scene state for each consumer request, reads the complete geometry set from disk, and
materializes every changed mesh back to the CPU. Consequently, a large industrial revision remains
dominated by CPU planning, cache I/O, snapshots, and synchronization rather than GPU work.

This design adds a revision-scoped GPU scene session. The session retains shared topology, derived
attributes, and an instance table across committed revisions. An `ApplyResult` diff drives a
`ScenePatch` that updates only affected resources. Multiple patches are encoded into one command
submission when possible, and completion waits occur only at an explicit consumer boundary.

The Document remains the only durable source of truth. The GPU scene is a replaceable derived view,
and CPU fallback remains available for every failure.

## 2. Measured Problem

The current large industrial fixture contains:

- 6,125,878 effective triangles;
- 716,891 unique triangles;
- 233 unique meshes;
- 1,938 expanded scene nodes;
- 210 connections.

On the current target adapter, warm `evaluation.stats` and a transform-only update each take about
1.87 seconds. The update does not become materially cheaper because the server creates a fresh disk
cache reader and evaluates the complete scene for each request.

The mesh benchmark separately shows 2M triangles at about 11.36 ms on the CPU reference and
39.24 ms with the current cold GPU path. Resident handle lookup is below timer resolution because it
is a cache hit; it is not evidence of a faster geometry kernel. The benchmark therefore reports
resident cache-hit success separately from cold GPU crossover and keeps the overall performance gate
failed closed.

## 3. Goals

1. Keep one GPU scene session alive for each open project and GPU device generation.
2. Reuse shared topology and unchanged attributes across revisions.
3. Convert committed revision diffs into precise scene and dirty-range updates.
4. Update transforms and materials without geometry dispatch, snapshot, or readback.
5. Preserve the previous complete GPU scene until a replacement revision is complete.
6. Coalesce pending derived updates and submit each scheduled batch once where safe.
7. Defer synchronization until preview, diagnostics, export, or another CPU consumer requires it.
8. Read back only changed ranges required by browser transport.
9. Preserve CPU authority, revision atomicity, undo/redo, project switching, resource limits, and
   canonical GLB export.
10. Measure real revision workflows rather than cache-hit lookup in isolation.

## 4. Non-Goals

- Moving Document, validation, approval, or persistence authority into GPU memory.
- Making GPU scene sessions, native GPU execution, preview packets, or browser WebGPU default-on.
- Exposing GPU buffers, WGSL, or device handles through the public API.
- GPU mesh booleans, topology editing, sculpting, or arbitrary geometry graph compilation.
- Sharing native GPU memory directly with the browser process.
- Persisting GPU resource handles across server restarts.
- Reimplementing browser instancing or frustum culling, which already exist.

## 5. Architecture

```text
CPU authority
  Document + revision + approval + validation + canonical GLB
                         |
                  ApplyResult.diff
                         |
                         v
Scene source index
  node/component dependencies
  stable identities and digests
  previous derived metadata
                         |
                         v
                    ScenePatch
  geometry add/update/remove
  instance add/remove/update
  dirty channels and ranges
                         |
                         v
Revision-scoped GpuSceneSession
  revision sequencer
  GeometryPool
  InstanceTable
  resource generations
  completion and evidence
                         |
          +--------------+---------------+
          |                              |
    GPU consumers                 packet/export consumers
          |                              |
    no CPU readback               explicit readback boundary
```

The native scene session and browser renderer are separate GPU domains. They exchange stable IDs,
revision numbers, and explicit CPU-readable packet data only.

## 6. Data Contracts

### 6.1 `SceneSessionId`

An opaque, process-local identifier scoped to:

- project identity;
- active session generation;
- native GPU device generation.

It is not a Document ID and is never persisted.

### 6.2 `SceneSourceIndex`

The CPU-owned index contains only metadata needed to plan an update:

- committed revision;
- node IDs and resolved instance ownership;
- node, component, and geometry digests;
- component-to-instance dependency edges;
- previous transforms, material IDs, visibility, bounds, and resource references;
- geometry topology and attribute associations.

It does not duplicate Document mutation authority and does not serialize the full Document for each
update.

### 6.3 `ScenePatch`

A patch is derived from one or more consecutive committed revisions and contains:

- `baseRevision`;
- `targetRevision`;
- geometry resource additions, replacements, dirty-range updates, and removals;
- instance additions, updates, and removals;
- material and visibility updates;
- dirty channel masks;
- required completion policy;
- evidence labels for each touched dependency.

A patch cannot mutate the Document or become durable project state.

### 6.4 Geometry resource identity

Geometry resources are split into:

- topology resources keyed by topology digest: indices, connectivity, adjacency, and immutable
  topology metadata;
- attribute resources keyed by attribute digest: positions, normals, and bounds;
- result handles keyed by the combined topology, attribute, dependency, kernel, and parity digests.

Instances reference result handles. Topology and attribute resources have independent reference
counts, allowing changed positions to reuse unchanged connectivity.

### 6.5 Instance table

The instance table uses structure-of-arrays buffers:

- stable scene node ID;
- result handle;
- world transform;
- material ID;
- visibility;
- local and world bounds when GPU consumers require them.

The table is never the source for semantic identity. Stable node and geometry IDs remain defined by
the Document and derived-scene contracts.

### 6.6 Scene revision state

The session exposes:

- `publishedRevision`: newest fully committed GPU scene view;
- `targetRevision`: newest queued or running CPU revision;
- `state`: `idle`, `pending`, `failed`, or `lost`;
- pending patch and submission counts;
- shared geometry, instance, topology, attribute, and resident-byte counts;
- last phase evidence and fallback reason.

The Document revision may be newer than `publishedRevision` while GPU work is pending. Consumers
must not present the old GPU view as the target revision.

## 7. Revision Update Flow

1. The CPU validates and commits a Document transaction.
2. The server maps `ApplyResult.diff` through `SceneSourceIndex`; unchanged nodes and dependencies
   are not scanned as new geometry.
3. The server constructs a `ScenePatch` for the new target revision.
4. The session validates revision order and reserves bounded memory for required resources.
5. The patch planner groups work by topology, attribute, instance, and semantic updates.
6. Compatible work is encoded into a shared command stream and submitted.
7. The previous complete scene remains available while the new view is pending.
8. The target becomes published only after all required resources and parity checks succeed.
9. Older resources are retired only after no published or pending consumer references them.

No GPU update occurs for dry-run transactions or rejected operations.

## 8. Dirty Update Semantics

### 8.1 Transform-only

- Update only the changed instance transform ranges.
- Run no geometry kernel.
- Perform no geometry upload or readback.
- Produce a preview delta with no geometry chunk.

### 8.2 Material and visibility-only

- Update only affected instance records.
- Do not invalidate topology, attributes, normals, or bounds.
- Do not create a geometry snapshot.

### 8.3 Parameter change with stable topology

- Reuse topology, indices, and adjacency.
- Compute changed position ranges on the CPU before upload.
- Upload only those ranges.
- Recompute normals and bounds for the affected topology neighborhood.
- Replace only the resulting attribute resource after successful completion.

### 8.4 Topology change

- Allocate a new topology resource.
- Build and upload only the changed topology and dependent attributes.
- Keep the previous topology generation alive until the update and its consumers are complete.
- Garbage-collect unreferenced generations after retirement.

### 8.5 Component definition change

- Use the dependency index to find directly and transitively affected component instances.
- Re-evaluate only that dependency closure.
- If the affected closure cannot be proven complete, fall back to a conservative full derived-scene
  rebuild while preserving revision and CPU authority.

### 8.6 Additions, removals, and structural changes

- Allocate or remove only affected instance slots.
- Update geometry reference counts.
- Remove unreferenced derived resources through normal retirement.
- Never mutate scene semantics based only on a GPU allocation outcome.

### 8.7 Batching and coalescing

Consecutive patches may be coalesced while no consumer has pinned an intermediate revision and no
identity-dependent operation prevents it. Coalescing uses the final derived state and must preserve
stable identities.

All compatible work in a scheduled batch uses one command encoder and one queue submission.
Batching may not cross priority, device generation, resource-that-cannot-be-shared, or atomic scene
visibility boundaries.

## 9. Synchronization and Preview

- Submission returns a completion handle; no fence wait is implicit.
- Status and statistics can report pending work without waiting.
- A consumer requesting `targetRevision` waits once for all preceding patches required by that
  revision.
- Multiple revisions may remain queued in order; a newer revision cannot overtake an older one.
- Transform-only and material-only previews do not require GPU completion.
- Attribute updates request readback only for ranges missing from the browser packet.
- Canvas preview, diagnostics, and source export are explicit CPU-readable boundaries.
- Source GLB export always renders from canonical CPU Document state, never from a stale GPU scene.

GPU kernel timing uses timestamp queries when supported. When unavailable, kernel timing is `null`;
wall-clock submission or wait time is never relabeled as kernel time.

## 10. Resource Lifecycle

- Memory is reserved against the host and adapter budgets before allocation.
- Published, pending, and pinned resources are not eviction candidates.
- Unreferenced resources use deterministic least-recently-used retirement.
- A replacement scene cannot evict resources still reachable from its base scene.
- Device loss immediately marks every session handle lost and prevents further publication.
- Project close, project switch, and server shutdown release the full session.
- Dropping a failed patch releases its staged resources without changing the previous published
  scene.

## 11. Failure Handling

- **Unsupported or disabled:** the existing CPU-derived path runs and reports the reason.
- **Revision conflict:** reject the patch, keep the previous scene, and rebuild from the current
  authoritative revision before accepting more GPU work.
- **Budget exceeded:** fail the scene update without partial publication and fall back according to
  the host policy.
- **Kernel or parity failure:** keep the prior complete scene, report a stable GPU diagnostic, and
  do not expose target-revision partial output.
- **Device loss:** invalidate handles, clear GPU resources, retain Document state, and select the
  existing CPU/WebGL2 fallback.
- **Consumer timeout:** do not mutate, roll back, or reorder the Document or queued revisions.
- **Cancelled or disconnected client:** completed Document revisions remain committed; cancellation
  only affects unpublished GPU work that no current revision requires.

GPU work runs on blocking workers. Locks are not held across asynchronous waits, and GPU activity
must not block health checks, revision delivery, or read-only Document APIs.

## 12. Public Behavior and Observability

The existing `ComputeStatus` gains a scene-session section containing:

- enabled and state;
- published and target revisions;
- pending patches and submissions;
- geometry, topology, attribute, and instance counts;
- resident bytes and peak bytes;
- dirty-range and readback byte counts;
- per-phase timing;
- last fallback reason.

No public operation can submit shader code, choose an adapter, select a process path, or obtain a
raw GPU buffer.

The Viewer reports pending target revisions and fallback state. It consumes existing full and delta
preview packets through public APIs and does not gain Document write authority.

## 13. Compatibility and Rollout

- The existing CPU evaluator remains the automatic default.
- Native GPU scene sessions are host-enabled, experimental, and default-off.
- Browser WebGL2 remains the default renderer.
- Browser WebGPU and preview packets remain explicit experimental capabilities.
- Existing mesh-evaluation and resident-handle APIs remain compatible wrappers while the scene path
  is incremental.
- A failed benchmark or functional gate keeps the scene session experimental and default-off.

## 14. Testing

Unit and integration coverage includes:

- patch generation for node, component, material, connection, add, remove, undo, and redo diffs;
- dependency closure for nested components and instance sets;
- topology reuse for stable-topology parameter changes;
- position dirty ranges and affected-normal dispatch;
- zero geometry work for transform, material, and visibility updates;
- shared topology and attribute reference counting;
- ordered multi-revision queues, coalescing, and consumer waits;
- atomic publication and rollback to the previous complete scene;
- memory admission and reference-safe eviction;
- device loss, project switching, and CPU fallback;
- full and delta packet parity with CPU reference geometry;
- status and evidence serialization through generated schemas;
- Viewer display of target revision, pending state, and fallback.

## 15. Performance and Acceptance Gates

The large industrial acceptance project compares the CPU-reference path and the experimental scene
session on the same adapter and machine in the same run.

Functional gates:

- CPU and GPU scene views pass the existing geometry parity tolerances;
- transform-only updates perform zero geometry dispatch, upload, snapshot, and readback;
- material and visibility updates perform zero geometry work;
- stable-topology parameter changes do not rebuild or re-upload unchanged topology;
- one scheduled batch uses one GPU submission;
- Document revision and canonical GLB remain valid under every failure;
- all existing Rust, API, Viewer, Engine, and E2E verification gates remain green.

Performance gates for the 6.1M-effective-triangle fixture:

- transform-only revision p50 is at least 5 times faster than the CPU-reference revision workflow;
- transform-only p95 does not regress the existing baseline;
- a single-instance parameter update uploads less than 5% of full unique-scene geometry bytes;
- transform and material updates report zero snapshot and readback bytes;
- queue coalescing reports exactly one submission for a tested multi-revision batch;
- geometry count and memory remain within configured bounds;
- statistics do not recreate or decode the full disk geometry cache.

Initial scene construction and cold GPU work are reported separately and do not inherit results from
warm incremental updates. If cold GPU execution remains slower than CPU, automatic mode remains on
the CPU reference and the feature remains experimental.

## 16. Documentation and Decision Record

On implementation, update:

- architecture and public API documentation;
- benchmark instructions and machine-readable output;
- Workbench status documentation;
- generated schemas and declarations for any added status fields.

The implementation must not change the default CPU/WebGL2 behavior without a separate measured
decision.
