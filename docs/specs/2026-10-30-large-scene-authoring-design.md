# ICE3 Studio Large-Scene Authoring Design

> Document ID: ICE3-STUDIO-SPEC-005
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

## Implementation Status

M1 scale-out authoring and M2 large-scene acceptance are implemented: Document schema v2
migration, validated parameterized components, explicit instances, deterministic instance sets,
typed connections, shared CPU evaluation, persistent geometry caching, GLB mesh sharing, API
projections, parameter forms, a 216-object / 6.1M-triangle fixture, topology checks, and
machine-readable performance evidence.

M3 native GPU evaluation and M4 Runtime publication remain pending. The system therefore remains
`Proposed` as a whole.

## 1. Summary

ICE3 Studio needs a scale-out authoring model for large industrial scenes rather than a larger
collection of one-off modeling primitives. An AI client must be able to describe reusable
equipment, place it repeatedly, connect it through typed ports, evaluate the resulting geometry,
publish a Runtime asset, and inspect the same model in the Workbench without issuing one
operation per triangle or per internal part.

This design adds four generic concepts to the existing Document:

- reusable parameterized component definitions;
- component instances;
- deterministic instance sets;
- routable connections between component ports.

The Document remains the source of truth. Component expansion, shared meshes, world transforms,
connection sweeps, source GLB, partitions, and LODs remain derived results. CPU code retains
transaction, validation, approval, persistence, and canonical-export authority. A native GPU
backend accelerates bounded geometry work and falls back safely.

The first acceptance target is a fictional generic industrial site with the same scale as the
existing large process-plant demonstration:

- approximately `1000 x 550` metres;
- 200 to 240 primary selectable equipment and facility objects;
- four primary treatment structures;
- six parallel process trains;
- ten to twelve secondary treatment structures;
- `5,500,000` to `7,500,000` effective source triangles;
- tens to hundreds of fine Runtime partitions and coarse LOD content.

The example validates generic Studio capabilities. Wastewater terminology, equipment tags, and
process topology belong to fixture metadata and fixture authoring tools, not Core operation
names.

## 2. Goals

1. Let AI build large scenes with high-level typed Operations instead of raw mesh submission.
2. Reuse component geometry across many instances without duplicating Document data.
3. Preserve stable instance, part, connection, and Runtime Selection identity.
4. Make local changes invalidate only the affected derived geometry.
5. Accelerate large geometry batches with a native GPU when measured workload justifies it.
6. Keep CPU reference evaluation and canonical source export authoritative.
7. Publish large scenes through Toolchain partitioning, LOD, and render batching.
8. Stream Runtime assets into the Workbench with coarse-to-fine coverage.
9. Expose machine-readable evidence for construction, evaluation, GPU, caching, publishing,
   and browser loading.
10. Keep the modeling vocabulary generic across industries.

## 3. Non-Goals

- Industry-specific operation names or built-in process-plant semantics.
- Arbitrary scripts, expressions, shader submission, or executable templates.
- Direct manipulation of geometry in the Workbench.
- Boolean operations, sculpting, free-form NURBS, texture authoring, animation, or simulation.
- Per-instance replacement of arbitrary internal shared geometry.
- Making GPU availability mandatory for editing, validation, persistence, or canonical export.
- Moving approvals, revisions, persistence, or security enforcement to GPU or Toolchain code.
- Replacing Toolchain's LOD, partition, tileset, or render-batching implementation.
- Loading the complete high-detail source asset before the browser can show useful content.

## 4. Confirmed Decisions

- The first milestone targets scale-out authoring, not isolated GPU microbenchmarks.
- The large-scene acceptance scale matches the existing large process-plant demonstration.
- Document structure uses parameterized components, instances, and layout rules.
- Instance overrides are limited to typed parameters, transform, material assignment,
  visibility, and metadata.
- All mutations continue through ChangeSets and `document.apply`.
- CPU remains the semantic and canonical-export reference.
- GPU dispatch is based on measured workload and device pressure.
- Small scenes may still use a single Prepared GLB.
- Large scenes use Toolchain-generated Tilesets and the existing camera-driven Runtime loader.
- WebGL2 remains the default browser renderer; WebGPU remains an explicit experimental choice.

## 5. System Context

```text
External AI / deterministic fixture client
                 |
                 | typed high-level Operations
                 v
          Studio Server / ChangeSets
                 |
                 v
       Studio Core source Document
  components + instances + sets
       + connections + relations
                 |
                 v
      backend-neutral evaluation
          /                 \
     CPU reference       Native GPU
          \                 /
      derived geometry cache
                 |
                 +----> Workbench evidence
                 |
                 +----> canonical source GLB
                              |
                              v
                    Toolchain Sidecar
              validate / prepare / partition
                              |
                              v
             immutable Runtime asset package
                              |
                              v
         Workbench GLB or camera-driven Tileset
```

The dependency boundaries remain unchanged:

- `studio-core` does not depend on Server, Viewer, Toolchain, native graphics, or an AI SDK.
- `studio-gpu` does not read project files or control revisions.
- `studio-server` orchestrates CPU/GPU evaluation and the managed Toolchain sidecar.
- Viewer uses public read APIs and artifact URLs only.

## 6. Document Model

### 6.1 Schema Version

The extended model introduces a new explicit Document schema version. The existing schema is
read without reinterpretation and is migrated only through a validated revision transaction.
Once a project is written in the new schema, the stored revision remains self-describing and
can be opened deterministically.

The migration must:

1. Preserve every existing Node, Material, Relation, Constraint, stable ID, revision, and
   idempotency record.
2. Add empty component, instance-set, and connection collections.
3. Preserve the existing `wastewater-mini` Document semantics byte-for-byte after canonical
   reserialization at the new schema version, excluding the schema-version field.
4. Reject unknown future schema versions rather than guessing.

### 6.2 Component Definition

A `ComponentDefinition` is a reusable, parameterized DAG of local parts. It is source data, not
a Web UI component and not an external modeling-application document.

```text
ComponentDefinition
├── id
├── name
├── parameters[]
├── parts[]
├── ports[]
├── metadata
└── visible
```

Each parameter declares:

- stable name;
- scalar, vector, color, or enum type;
- default value;
- optional minimum and maximum;
- optional unit label;
- display metadata for generated forms.

Each definition part declares:

- stable part ID local to the definition;
- optional parent part ID;
- local transform;
- content;
- material role;
- parameter bindings used by dimensions, transforms, and material selection;
- metadata;
- visibility.

Allowed part content is:

- group;
- box, plane, cylinder, or sphere;
- custom triangle mesh;
- polyline sweep;
- a reference to another component definition.

Component references form a DAG. Cycles, unresolved references, duplicate part IDs, duplicate
port names, non-finite values, excessive nesting, and invalid parameter bindings are rejected.

First-version parameter bindings are references to typed instance values. Arbitrary arithmetic
expressions, conditionals, loops, file access, URLs, scripts, and executable code are not
supported.

### 6.3 Component Instances

`SceneNode.content` gains a component-instance variant that references a ComponentDefinition.

```text
ComponentInstance
├── definitionId
├── parameters
├── overrides
└── metadata
```

The containing Node continues to own:

- stable instance Node ID;
- display name;
- parent;
- placement transform;
- assigned material;
- visibility;
- metadata.

Overrides are keyed by definition part ID and support only:

- additional typed parameter values scoped to that instance;
- material-role assignment;
- visibility;
- metadata.

An instance cannot replace the internal mesh of a shared part. A requirement for structurally
different geometry produces a different ComponentDefinition or explicit custom-mesh Node.

### 6.4 Instance Sets

An `InstanceSet` is a SceneNode containing a deterministic placement rule:

```text
InstanceSet
├── definitionId
├── layout
├── sharedParameters
├── itemOverrides
└── metadata
```

Supported layout kinds:

- `linear`: origin, direction, spacing, and count;
- `grid`: origin, two axis directions, row/column counts, and spacing;
- `radial`: centre, radius, start angle, angular direction, and count;
- `path`: ordered placement frames and selectable spacing mode.

An instance set is intended for repeated objects whose ordinal identity is meaningful. Unique
business objects that require independent names, ports, relations, and topology use explicit
component-instance Nodes.

Instance-set appearance identity is derived from the set ID and stable ordinal. Reordering a set
therefore changes the semantic identity of its items and must invalidate dependent connections.
The API reports the concrete endpoint identities produced by a set.

### 6.5 Connections

`SceneNode.content` gains a connection-intent variant:

```text
Connection
├── from: InstancePortRef
├── to: InstancePortRef
├── medium
├── diameter
├── routePolicy
├── relationId
└── metadata
```

An instance/port reference may address an explicit component instance or an instance-set ordinal.
The route policy initially supports:

- `direct`;
- `orthogonal`;
- `buried`;
- `rack`.

The evaluator generates a deterministic polyline and sweep from the port frames, route policy,
and routing clearance metadata. The generated point list is derived data and is not written back
to the Document.

`connection.create` atomically creates the connection Node and its generic process Relation. The
two identities remain linked through `relationId`; deleting or replacing the connection applies
the same transaction semantics to its Relation.

Validation rejects:

- missing instances or ports;
- incompatible port media;
- duplicate or zero-length connection identity;
- self-connections unless explicitly allowed by future policy;
- non-positive radius;
- routes that cannot meet minimum clearance;
- unresolved instance-set ordinals.

### 6.6 Stable Identity

The source-of-truth identities are:

- ComponentDefinition ID;
- definition-local part ID;
- component-instance Node ID;
- InstanceSet Node ID and stable ordinal;
- Connection Node ID and Relation ID;
- existing scene, material, relation, constraint, and revision IDs.

Derived Runtime identities use a canonical representation such as:

```text
<instance-id>/<part-id>
<instance-set-id>/<ordinal>/<part-id>
<connection-id>
```

The final encoding is fixed by the API schema. Evaluation order, HashMap order, partition order,
and cache state cannot alter an identity.

Component definitions and internal parts are not top-level selectable Runtime objects by default.
Selection resolves to the owning instance, InstanceSet item, connection, or explicitly declared
selectable part.

### 6.7 Ownership and Nested Components

A ComponentDefinition may reference another definition, but ownership remains explicit:

- definitions are project-scoped and cannot reference another project;
- IDs remain unique within their namespace;
- source metadata does not contain local filesystem paths;
- definition deletion is rejected while instances or nested references remain;
- instance deletion is rejected while connections or required Relations still reference it.

## 7. Public API

The API remains JSON-RPC over loopback HTTP with WebSocket events. All writes enter
`document.apply`; dry-run, idempotency, revision conflict, schema validation, and ChangeSet
approval semantics do not change.

### 7.1 Operations

| Operation | Purpose |
|---|---|
| `component.define` | Create a component definition |
| `component.update` | Replace validated definition content |
| `component.delete` | Delete an unreferenced definition |
| `instance.create` | Create an explicit component-instance Node |
| `instance.update` | Update placement, parameters, or supported overrides |
| `instance.delete` | Delete an unreferenced instance |
| `instanceSet.create` | Create a deterministic repeated placement |
| `instanceSet.update` | Update layout, shared parameters, or item overrides |
| `instanceSet.delete` | Delete an unreferenced instance set |
| `connection.create` | Create a routed connection and Relation |
| `connection.update` | Update endpoints, route policy, or metadata |
| `connection.delete` | Delete a connection and its linked Relation |

Existing Node, Primitive, mesh, material, Relation, constraint, ChangeSet, undo, and redo
Operations remain available.

The API returns created identities, expanded counts, affected identities, and validation
diagnostics. Bulk high-level Operations are preferred for AI authoring, but no Operation accepts
arbitrary scripts or merged binary payloads.

### 7.2 Read Models

The existing `scene.get`, `scene.query`, `scene.bounds`, `evaluation.evaluate`, and ChangeSet APIs
gain generic projections for:

- component definitions and references;
- concrete instance identities generated by sets;
- resolved ports and connection endpoints;
- derived geometry summaries;
- CPU/GPU and cache evidence.

Derived projections must never mutate source Document data.

### 7.3 Derived Asset Jobs

Large Runtime publication uses asynchronous jobs:

| Method | Purpose |
|---|---|
| `derivedAsset.request` | Request GLB or Tileset publication for a revision or approved candidate |
| `derivedAsset.status` | Read job state, progress, and evidence |
| `derivedAsset.cancel` | Request cancellation |

Job states are:

```text
queued -> exporting -> preparing -> partitioning -> ready
                                                 \-> failed
queued | exporting | preparing | partitioning -> cancelled
```

Small scenes may return an immediately ready GLB descriptor. The public descriptor has a stable
shape:

```text
DerivedAssetDescriptor
├── jobId
├── kind: "glb" | "tileset"
├── assetId
├── revision
├── contentHash
├── url
├── bounds
├── triangleCount
├── partitionCount
├── byteLength
├── cacheStatus
└── diagnostics[]
```

The URL is a Studio-owned artifact URL. Absolute filesystem paths and Sidecar workspace paths do
not cross the public boundary.

### 7.4 Diagnostic Codes

New stable codes include:

- `ICE3_STUDIO_COMPONENT_INVALID`;
- `ICE3_STUDIO_PARAMETER_INVALID`;
- `ICE3_STUDIO_INSTANCE_INVALID`;
- `ICE3_STUDIO_INSTANCE_SET_INVALID`;
- `ICE3_STUDIO_CONNECTION_INVALID`;
- `ICE3_STUDIO_GEOMETRY_CACHE_FAILED`;
- `ICE3_STUDIO_GPU_UNAVAILABLE`;
- `ICE3_STUDIO_GPU_INCOMPATIBLE`;
- `ICE3_STUDIO_GPU_BUDGET_EXCEEDED`;
- `ICE3_STUDIO_GPU_KERNEL_FAILED`;
- `ICE3_STUDIO_GPU_PARITY_FAILED`;
- `ICE3_STUDIO_DERIVED_ASSET_FAILED`;
- `ICE3_STUDIO_DERIVED_ASSET_CANCELLED`.

Diagnostics identify the operation, definition, instance, or connection and include JSON
Pointers. They never expose credentials or out-of-project paths.

## 8. Evaluation

### 8.1 Pipeline

Evaluation proceeds in a fixed order:

1. Validate Document structure and all source references.
2. Compile ComponentDefinitions into immutable evaluation blueprints.
3. Resolve explicit instances and InstanceSet ordinals.
4. Resolve parameters and supported overrides.
5. Compute dependency digests and cache keys.
6. Evaluate reusable part geometry on CPU or GPU.
7. Compute instance world matrices and world bounds.
8. Resolve connection endpoints and generate routed sweeps.
9. Build the derived scene and GLB-sharing plan.
10. Validate budget, parity, and export prerequisites.

No derived result can be committed as Document truth.

### 8.2 Shared Geometry

Instances with the same definition, parameter values, part state, and kernel identity share one
mesh result. GLB export may reference the same mesh from multiple standard glTF Nodes.

Effective triangle count is the sum visible through instance placement. Unique geometry bytes and
unique triangle count are reported separately, proving that repeated equipment reuses storage and
computation.

### 8.3 Cache Layers

The evaluator uses these deterministic layers:

| Layer | Key inputs | Reuse |
|---|---|---|
| Blueprint | Definition content, parameter schema, compiler version | All instances of a definition |
| Part geometry | Blueprint, resolved parameters, kernel and backend version | All equivalent parts |
| Instance world data | Instance transform, parent chain, bounds | Transform-only changes |
| Instance set | Definition, layout, parameters, item overrides | Unchanged ordinals |
| Connection route | Resolved port frames, medium, diameter, route policy | Unchanged endpoints |
| Derived export | Source digest, export rules, profile version | Unchanged revision export |

In-memory caches are process-scoped. Durable caches live under the project's
`cache/geometry/` directory and are content-addressed. A cache write is atomic. Corrupt entries are
quarantined and regenerated. Cached files are never included in Git or returned as authoritative
Document data.

### 8.4 Incremental Invalidation

| Change | Required invalidation |
|---|---|
| Instance transform or parent | World matrix, world bounds, dependent connection routes |
| Material assignment | Material binding only; no triangle rebuild |
| Visibility | Derived visibility and dependent bounds summaries |
| Instance parameter | Affected shared geometry key and dependent routes |
| Definition primitive or part | Blueprint and bodies using it |
| InstanceSet layout | Changed ordinal placements and dependent connections |
| InstanceSet item override | That ordinal and dependent routes |
| Connection endpoint or route policy | That connection route only |
| Metadata only | Metadata projection; no geometry |

The implementation must prefer incremental caches but may perform a bounded full fallback after
device loss or cache corruption. Fallback reason and work performed are reported as evidence.

## 9. Native GPU Evaluation

### 9.1 Boundary

The native GPU is a backend for derived geometry. It does not own Documents, revisions,
approvals, persistence, authorization, or canonical export semantics.

`studio-core` exposes backend-neutral asynchronous evaluation requests and results. `studio-gpu`
implements execution, device lifecycle, memory accounting, and parity measurement. `studio-server`
performs scheduling and evidence recording.

### 9.2 Initial Kernels

The first GPU implementation supports:

- box generation;
- plane generation;
- cylinder generation;
- sphere generation;
- polyline sweep generation;
- normals accumulation;
- bounds reduction.

Component DAG validation, parameter validation, identity allocation, connection semantics, and
canonical export rules remain on CPU.

### 9.3 Scheduling

Host-selected modes are:

- `auto`: choose CPU or GPU per job from measured conditions;
- `cpu`: force the reference backend;
- `gpu`: require the native backend and fail explicitly when unavailable.

`auto` considers:

- expanded part and triangle counts;
- upload and readback estimates;
- independent batch size;
- current queue depth;
- GPU memory pressure;
- backend health;
- measured crossover data.

Interactive candidate work has priority over background publication. Oversized jobs are
chunked, use CPU fallback in `auto` mode, or fail with a budget diagnostic. GPU work never blocks
health checks, reads, revision events, or ChangeSet approval.

### 9.4 Correctness and Fallback

CPU reference geometry remains normative. GPU parity checks require:

- identical indexed triangle topology for deterministic kernels;
- vertex and normal error below `1e-5` after normalization;
- relative bounds error below `1e-5` at scene scale;
- finite positions, normals, and bounds.

Device loss invalidates GPU caches and fails in-flight jobs with a structured diagnostic.
Committed revisions and disk data remain unchanged. `auto` mode may rerun failed work on CPU.

### 9.5 Evidence

Evaluation evidence includes:

- selected backend and fallback reason;
- adapter acceleration tier without machine-identifying details;
- instance, expanded part, unique mesh, and triangle counts;
- CPU time, kernel time, queue time, upload bytes, and readback bytes;
- cache hit and miss counts;
- estimated and peak GPU memory;
- parity result.

## 10. Publication and Streaming

### 10.1 Publication Pipeline

Large-scene publication proceeds through:

```text
approved revision or candidate
  -> Studio source GLB
  -> Toolchain asset validation
  -> Prepared GLB
  -> partition + LOD + render batching
  -> asset manifest verification
  -> immutable content-addressed publication
  -> Workbench artifact URL
```

Toolchain remains the owner of Runtime LOD, spatial partitioning, coarse content, tileset
generation, and static render batches.

### 10.2 Small and Large Asset Selection

- Small scenes use the existing single GLB path.
- Large scenes use a Toolchain Tileset and the existing camera-driven Runtime loader.
- The Server chooses the descriptor type from output capabilities and measured scene size.
- API callers receive the selected kind and evidence; they do not provide executable or Toolchain
  paths.

### 10.3 Artifact Layout

The published package preserves Toolchain-relative structure:

```text
cache/toolchain/<content-key>/
├── manifest.json
├── tileset.json
├── parts/
└── coarse/
```

Studio serves it through a read-only versioned artifact route. Requests containing absolute paths,
parent traversal, or symlink escapes are rejected. Content types are explicit. Immutable assets
may use content-digest caching headers.

### 10.4 Job Scheduling and Cancellation

- Interactive candidate previews take priority over background full-scene publication.
- At most one publication job for the same project and revision is active.
- A superseded revision lowers or cancels queued work.
- Cancellation leaves no partially visible publication.
- Previously ready assets remain available until replacement is complete.
- Sidecar failure does not change the Document or remove the last valid artifact.

### 10.5 Workbench Loading

The Workbench owns exactly one active asset handle:

- `ICEAssetHandle` for GLB;
- `ICETilesetHandle` for Tileset.

On a new descriptor:

1. Start loading the replacement while retaining the old asset.
2. Fit the camera from the new descriptor bounds after the first usable content commits.
3. Atomically switch the active scene.
4. Dispose the previous asset only after the replacement is usable.

Failures retain the previous visible asset and show structured diagnostics. The Workbench exposes
tileset state, resident Tile count, GPU bytes, cache activity, LOD fallback, and loading progress.

WebGL2 remains the default. WebGPU can be selected only through the existing explicit
experimental control and capability probe.

## 11. Large Industrial Acceptance Scene

The acceptance fixture is project-scoped and generic:

- invented site layout with no customer or proprietary data;
- roughly `1000 x 550` metres;
- 200 to 240 primary selectable equipment and facility objects;
- at least eight functional areas;
- four primary treatment structures;
- six parallel process trains with shared and branch-specific topology;
- ten to twelve secondary structures;
- supporting chemical, utility, solids, control, maintenance, and access infrastructure;
- visible ports, routed pipes, walkways, railings, access ways, and equipment assemblies;
- `5,500,000` to `7,500,000` effective source triangles;
- generated entirely through typed Studio Operations;
- Toolchain-prepared Tileset with fine partitions and coarse LOD.

Industry names and colors live in fixture metadata and material data. Core code contains only
generic component, instance, set, connection, relation, and geometry concepts.

The deterministic AI fixture client must:

1. define reusable equipment templates;
2. create explicit unique instances and repeated instance sets;
3. create ports, process relations, and routes;
4. run successful and rejected dry-runs;
5. commit the scene through ChangeSets;
6. evaluate, export, partition, reload, undo, and redo;
7. prove stable identity across all stages.

## 12. Performance and Verification

### 12.1 Benchmark Suites

The existing small-scene benchmark remains. A new large-scene suite records:

- high-level Operation count and Document JSON bytes;
- blueprint, unique mesh, instance, expanded part, and effective triangle counts;
- full evaluation and 1% incremental evaluation p50/p95;
- CPU and GPU kernel latency, transfer bytes, and crossover selection;
- geometry cache hit ratio and regenerated bytes;
- source GLB time and size;
- Toolchain preparation, partition, and LOD time;
- first visible partition, tileset-ready, and full working-set load time;
- resident Tile count, GPU bytes, evictions, and coarse fallback;
- fixed-camera FPS, CPU frame time, GPU frame time, Selection, and Pick p50/p95.

Reports are machine-readable JSON. Device-identifying data, credentials, and absolute paths are
excluded.

### 12.2 Quantitative Gates

The large-scene release gate requires:

1. The scene contains 200 to 240 primary selectable objects and 5.5M to 7.5M effective source
   triangles.
2. The Document remains a high-level representation; it does not persist flattened part
   instances or connection triangle buffers.
3. Equivalent instances share one cached geometry entry.
4. A change confined to one small component reuses at least 90% of evaluable geometry cache
   entries and does not rebuild unaffected unique meshes.
5. GPU and CPU geometry parity passes every supported kernel.
6. On the development reference machine, each backend runs two warm-up batches followed by at
   least nine measured batches for the 500k- and 2M-triangle workloads. `auto` mode demonstrates
   at least a 10% p50 improvement in end-to-end job latency for each size, and GPU p95 is no more
   than 5% worse than CPU when GPU is selected. The report records exact values and fails when
   either condition is not met.
7. First usable Runtime content comes from the Tileset or coarse partition path; the browser does
   not fetch the entire source GLB before rendering.
8. Replacing a revision preserves the previous valid asset until the replacement is ready.
9. Selection IDs remain stable across source export, partition, LOD, and browser reload.
10. Existing small-scene latency limits and WebGL2 behavior continue to pass.

Absolute time thresholds for the 5.5M to 7.5M scene are established from the first reproducible
release baseline rather than invented before implementation. Every revision to those thresholds
is recorded with the benchmark result and hardware-neutral tier description.

### 12.3 Correctness Evidence

Before performance is accepted:

- every Operation has schema and negative-path tests;
- component DAG and parameter validation cover cycles, depth, limits, and non-finite values;
- stable IDs survive reorder-insensitive maps, save/reopen, undo, redo, and partition;
- shared mesh and effective triangle counters are independently verified;
- connection endpoints resolve to real ports and all required routes are connected;
- partition coverage and Selection identity match the source scene;
- CPU/GPU parity and fallback tests pass;
- cancellation and failure never expose partial assets or mutate committed revisions;
- the form and API schemas expose the same constrained parameter contract.

## 13. Security

- Component definitions are declarative data and cannot execute code.
- Parameters are bounded by type, size, count, and numeric limits.
- Definition nesting, part count, instance count, connection count, and document bytes have host
  policy limits.
- API clients cannot select GPU adapters, shaders, Toolchain executables, or environment values.
- Artifact paths remain project-confined; traversal and symlink escapes are rejected.
- Cache and publication manifests are verified before use.
- GPU or Toolchain failure cannot bypass validation, ChangeSet approval, revision checks, or
  persistence semantics.
- Diagnostics do not expose credentials, Agent policy, or out-of-workspace paths.

## 14. Testing Strategy

### 14.1 Core Unit Tests

- Component schema, parameter types, overrides, and bounded validation.
- DAG cycle detection and nested-reference invalidation.
- Linear, grid, radial, and path instance-set determinism.
- Port resolution and connection routing.
- Stable derived identities.
- Shared geometry keys and incremental invalidation.
- Schema migration and canonical reserialization.

### 14.2 API and Transaction Tests

- Every new Operation in a ChangeSet and direct `document.apply`.
- Dry-run equivalence and idempotent retry.
- Revision conflicts and reference-safe deletion.
- Bulk response and generated identity mapping.
- Derived asset job state transitions, cancellation, and stale tasks.
- Artifact URL containment and content headers.

### 14.3 GPU Tests

- Capability and device initialization.
- Per-kernel CPU parity.
- 500k and 2M synthetic batch benchmarks.
- Cache reuse and upload/readback accounting.
- Memory admission, pressure, and CPU fallback.
- Device loss and recovery without committed-state changes.

### 14.4 Integration and E2E

- Deterministic AI client builds the large industrial fixture using high-level Operations.
- Source export passes Toolchain validation and preparation.
- Partition output covers the source scene and preserves Selection IDs.
- Workbench loads through Tileset streaming.
- Revision switch, stale publication, toolchain failure, and tile failure preserve useful content.
- Undo, redo, save, reopen, and replay produce the same stable identities.
- Existing MVP and Toolchain sidecar suites show no regression.

## 15. Delivery Milestones

Each milestone has its own implementation plan and verification gate.

### M1: Scale-Out Authoring Foundation

- Document schema and migration.
- Component, instance, instance-set, and connection Operations.
- CPU reference evaluator and persistent cache.
- Public schemas and Workbench parameter forms.

### M2: Large-Scene Acceptance

- Generic large industrial fixture generated through Studio APIs.
- Deterministic high-level AI client.
- Source GLB, topology, identity, and incremental-cache verification.
- Large-scene benchmark baseline.

### M3: Native GPU Evaluation

- `studio-gpu` backend and initial kernels.
- Scheduler, parity, memory controls, fallback, and evidence.
- Measured CPU/GPU comparison and release gate.

### M4: Large Runtime Publication

- Asynchronous derived-asset jobs.
- Toolchain preparation, partition, LOD, and render-batching orchestration.
- Versioned artifact serving.
- Workbench Tileset streaming, lifecycle, and evidence.

## 16. Acceptance Criteria

This design is complete when:

1. An AI can build the specified large scene with generic high-level Operations.
2. The Document remains compact and parameterized rather than expanded into flat per-part rows.
3. Repeated components share evaluation and stored geometry.
4. CPU and GPU produce parity-approved derived geometry.
5. Measured `auto` scheduling selects GPU only when justified and demonstrates quantified gains
   on large batches.
6. Local modifications have bounded invalidation and measured cache reuse.
7. Source export and Toolchain publication preserve complete scene and Selection identity.
8. Workbench streams large content, preserves the last valid scene on failure, and exposes
   actionable evidence.
9. Existing MVP, ChangeSet, Toolchain, and WebGL2 workflows remain backward compatible.
10. All new capabilities remain generic and industry-neutral at the Core and API boundary.

## 17. Risks and Mitigations

| Risk | Mitigation |
|---|---|
| Component model becomes an accidental scripting language | Restrict to typed data and bindings; no expressions or code |
| Large Documents still grow too quickly | Reuse definitions and instance sets; enforce size and count budgets |
| Incremental invalidation is incomplete | Dependency digests, explicit invalidation matrix, and mutation tests |
| GPU overhead dominates small jobs | Measured dispatch and CPU reference path |
| Floating-point differences break identity | CPU canonical export and parity tolerance for derived geometry |
| Toolchain jobs block interaction | Async jobs, priorities, cancellation, and bounded workers |
| Browser loads the entire source scene | Coarse-to-fine Tileset streaming and first-visible evidence |
| Industry semantics leak into Core | Generic Operations; fixture-specific metadata and tooling |
| Partial publication becomes visible | Content-addressed staging, manifest verification, and atomic switch |
| Migration changes old projects | Explicit schema migration, golden tests, and revision-safe writes |

## 18. Reference Decisions

- [Document as Source of Truth](../decisions/0001-document-as-source-of-truth.md)
- [Local API Boundary](../decisions/0002-local-api-boundary.md)
- [AI-Native Modeling MVP Design](2026-10-30-ai-native-studio-mvp-design.md)
- [Studio Web Workbench Design](2026-10-30-studio-web-workbench-design.md)
- [Native GPU Compute Design](2026-10-30-native-gpu-compute-design.md)
- [Toolchain Sidecar Integration Design](2026-10-30-studio-toolchain-sidecar-design.md)
- [Architecture](../architecture.md)
