# ICE3 Studio GPU Resident Hot-Path Redesign

> Document ID: ICE3-STUDIO-SPEC-007
>
> Status: Experimental
>
> Owner: ICE3 Studio maintainers

## 1. Summary

The resident implementation is functionally complete, but its measured hot path is dominated by
CPU work. At 2M triangles, a resident lookup takes about 221 ms while a snapshot that also performs
GPU copy and readback takes about 4.16 ms. The inversion shows that the current path does not yet
exercise the GPU's steady-state capability.

This design removes whole-mesh hashing and transport from resident lookups, introduces stable
derived-geometry handles, batches compatible work into fewer submissions, and separates GPU kernel,
submission, synchronization, snapshot, and transport evidence.

CPU remains authoritative for Document state, validation, revision, approval, persistence, and
canonical GLB. GPU residency remains a replaceable derived cache.

## 2. Measured Problem

The current resident path calls `ResidencyKey::for_content` on every evaluation. For custom meshes,
this serializes the complete content and hashes every index and attribute byte. Resident hit
processing therefore scales with mesh size even though no upload or kernel dispatch is required.

The current resident batch API also iterates heterogeneous nodes sequentially. It records one
submission result but does not encode all compatible nodes into one command stream. Cold creation
submits work and immediately waits. Preview packet generation still evaluates a CPU scene and labels
the resulting packet as CPU-derived.

## 3. Goals

1. Compute canonical content and dependency digests once when immutable content enters the compute
   service.
2. Resolve repeated resident work through stable handles without serializing or hashing mesh payloads.
3. Keep source snapshot, topology residency, attribute residency, and GPU result identity separate.
4. Encode compatible nodes for one submission and avoid per-node fences.
5. Defer completion waits until a consumer requires CPU-visible completion.
6. Attribute wall time to key lookup, planning, upload, command encoding, submission, GPU kernel,
   synchronization, snapshot, packet assembly, and browser upload.
7. Let preview packet generation consume the resident source view and GPU evidence while preserving
   explicit readback boundaries for any GPU-derived attribute not present in the source snapshot.
8. Preserve CPU fallback, device-loss recovery, revision semantics, and canonical export.

## 4. Non-Goals

- Moving Document, revision, approval, undo, or validation authority to GPU memory.
- Persisting GPU handles as semantic object identities.
- Sharing native process GPU memory directly with the browser WebGPU sandbox.
- Compiling arbitrary Geometry Graphs or adding GPU mesh booleans in this milestone.
- Making native GPU or preview packets default-enabled before the measured release gates pass.

## 5. Architecture

```text
Immutable resolved content
        |
        v
CPU content digest + validation (once)
        |
        v
ResidentGeometryHandle
  geometry_id
  content_digest
  dependency_digest
  source_snapshot
  gpu_resource_generation
        |
        v
ComputeService resident index
        |
        v
ResidentPlan
  compatible kernel groups
  dirty ranges
  completion policy
        |
        v
One command encoder per compatible group
        |
        v
GPU resident buffers
        |
        +----> completion handle / GPU consumers
        |
        +----> explicit snapshot
        |
        +----> preview packet source view
```

The resident index is keyed by immutable digests and resource generation, never by object addresses
or by reserializing the full content.

## 6. Data Contracts

### 6.1 `GeometrySourceDigest`

Contains:

- topology digest;
- attribute digest;
- dependency digest;
- kernel contract version;
- parity contract version.

Digest calculation happens in the CPU ingestion boundary and is cached with the immutable source
snapshot.

### 6.2 `ResidentGeometryHandle`

Contains:

- opaque `ResidentGeometryId`;
- stable geometry ID used by preview transport;
- digest set;
- vertex and triangle counts;
- device resource generation;
- state: `ready`, `pending`, `lost`, or `evicted`.

The handle is not a scene object ID and is not part of semantic candidate identity.

### 6.3 `ResidentEvaluationRequest`

Contains either:

- an existing handle plus a dirty plan; or
- an immutable source digest plus the source snapshot needed for first residency.

The steady-state request does not contain a serialized NodeContent payload.

### 6.4 `GeometryPhaseEvidence`

Records separate values for:

- key lookup;
- plan construction;
- CPU validation;
- upload/partial update bytes;
- command encoding;
- queue wait;
- GPU kernel;
- submitted-work wait;
- snapshot copy/readback;
- packet assembly;
- browser upload.

GPU kernel time uses timestamp queries when supported. Wall-clock measurements remain available as
lower-fidelity evidence.

## 7. Execution

### 7.1 First residency

1. Validate and fingerprint immutable content on the CPU once.
2. Build topology and adjacency outside the interactive hot path where possible.
3. Upload capacity-class buffers.
4. Encode all required compute passes.
5. Return a pending or ready handle according to completion policy.
6. Record resource generation and evidence.

### 7.2 Resident hit

1. Look up the handle by digest and generation.
2. Validate that the device generation still matches.
3. Update LRU/pin state.
4. Return the resident view.

No mesh serialization, SHA calculation, upload, dispatch, fence, or readback is allowed.

### 7.3 Dirty update

1. Validate affected channels.
2. Write only dirty buffer ranges.
3. Run only dependent kernels.
4. Keep topology and unaffected outputs resident.
5. Publish the new result generation atomically.

### 7.4 Batching

Compatible handles are grouped by:

- kernel family;
- pipeline and binding layout;
- dirty channels;
- priority;
- result visibility revision;
- memory admission class.

Each group uses one command encoder and one queue submission. Partial failure publishes no ready
entries from that group.

### 7.5 Completion

`submit` returns a completion handle. CPU waits only for:

- canonical snapshot/export;
- diagnostics that require CPU comparison;
- explicit caller synchronization.

Preview metadata and GPU-only consumers use the completion handle without a full CPU readback.

## 8. Preview Boundary

Preview packet construction receives:

- immutable source positions and topology;
- stable node and geometry IDs;
- resident digest and evidence;
- explicit GPU-derived attributes only when readback materializes them.

Source-only transforms and material changes create deltas without readback or geometry packet data.
Any GPU-derived normals or bounds not present in the source snapshot remain an explicit snapshot
boundary. Native and browser GPU buffers are not shared.

## 9. Compatibility and Failure Handling

- Existing evaluation APIs remain available as compatibility wrappers.
- CPU mode never initializes the GPU.
- Auto mode selects GPU only after digest, budget, health, and measured threshold checks pass.
- Device loss changes every handle generation to `lost`, clears GPU resources, and preserves the
  committed revision and last complete browser scene.
- Handle expiration causes CPU reference evaluation or a GLB fallback, never stale reuse.
- Packet generation remains host-enabled and falls back to GLB on invalid data or unsupported
  transport.
- Canonical source GLB remains CPU-authoritative.

## 10. Testing

Unit and integration coverage must prove:

1. A repeated handle lookup performs no whole-mesh serialization or SHA.
2. A stale device generation never reuses a resident handle.
3. Transform-only and material-only requests submit no geometry work.
4. Compatible nodes produce one submission and publish atomically.
5. Incompatible nodes preserve deterministic priority and budget behavior.
6. Snapshot remains the only path requiring CPU-visible geometry output.
7. Preview packet source views preserve stable digest identity across revisions.
8. Device loss and cancellation leave Document revision unchanged.
9. CPU and resident paths preserve existing tolerance parity.
10. Key lookup, submit, fence, snapshot, and transport evidence remain distinguishable.

## 11. Acceptance Gates

Functional gates:

- all existing Rust, Server, Viewer, Engine, and E2E functional tests pass;
- no full-mesh serialization or SHA occurs on a resident hit;
- compatible heterogeneous nodes use one submission;
- no readback occurs for transform-only preview updates;
- source export remains CPU-canonical.

Performance gates on 500k and 2M indexed triangles:

- resident p50 is at least 10% below CPU p50;
- resident p95 is no more than 5% above CPU p95;
- resident hit key lookup p50 is below 0.1 ms;
- warm dirty updates do not rebuild topology;
- GPU kernel time is reported independently from CPU preparation and synchronization.

If any gate fails, native GPU and packet transport remain experimental and Auto remains on CPU.

## 12. Rollout

1. Land digest/handle and phase-evidence changes behind the existing experimental resident policy.
2. Land compatible batching and deferred completion.
3. Route preview packet source views through resident handles while preserving explicit snapshot
   boundaries.
4. Re-run all mode-specific benchmarks.
5. Enable only capabilities that independently pass functional and performance gates.
