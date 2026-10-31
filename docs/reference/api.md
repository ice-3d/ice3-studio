# Local API Reference

> Document ID: ICE3-STUDIO-REF-API-001
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

The Studio Server exposes the same API to external AI clients, scripts, and the Web Viewer.
The Web Viewer has no private mutation path.

## Transport

- `GET /health`: unauthenticated process and contract health.
- `POST /rpc`: JSON-RPC 2.0.
- `GET /ws?token=...`: revision event stream.
- `GET /schemas/studio-api.schema.json`: generated JSON Schema.
- `GET /schemas/studio-types.ts`: generated TypeScript declarations.
- `GET /previews/{revision}`: prepared GLB for a committed revision.
- `GET /preview-packets/{revision}`: revisioned `ICE3PGP1` geometry packet when host policy enables
  preview packets.
- `GET /preview-packets/candidates/{digest}`: candidate packet with the same media type.
- `GET /reference-assets/{assetId}/tileset.json`: streamed reference tileset with relative tile
  content rewritten beneath `/tiled/`.
- `GET /reference-assets/{assetId}/tiled/{path}`: immutable coarse/fine GLB tiles.
- `GET /reference-assets/{assetId}/prepared.glb`: immutable Prepared GLB fallback.
- `GET /reference-assets/{assetId}/equipment.json`: immutable equipment manifest.

The default origin is `http://127.0.0.1:8120`. Server startup generates a short-lived token;
mutating methods require `Authorization: Bearer <token>`.

## JSON-RPC Envelope

```json
{
  "jsonrpc": "2.0",
  "id": "request-1",
  "method": "document.apply",
  "params": {}
}
```

Success returns `result`; failure returns `error.code`, `error.message`,
`error.diagnosticCode`, and optional structured `error.data`. Unknown methods use JSON-RPC code
`-32601`. Parse failures use `-32700` with a null request ID.

## Methods

| Method | Write | Parameters | Result |
|---|---:|---|---|
| `system.health` | No | none | Version and status |
| `system.capabilities` | No | none | API version, method inventory, Toolchain state, and native compute evidence |
| `system.agentManifest` | No | none | Agent tools, current token authority, limits, and workflow hints |
| `project.create` | Yes | `{ projectId }` | `{ opened: true }` |
| `project.open` | Yes | `{ projectId }` | `{ opened: true }` |
| `project.get` | No | none | Current document |
| `project.save` | Yes | none | Current persisted revision |
| `project.close` | Yes | none | Release the writer lock and close the active project |
| `document.get` | No | none | Current document |
| `document.apply` | Yes | `ApplyRequest` | `ApplyResult` |
| `document.undo` | Yes | none | Restored document as a new revision |
| `document.redo` | Yes | none | Restored document as a new revision |
| `document.history` | No | none | Operation history |
| `scene.get` | No | none | Current scene |
| `scene.query` | No | See below | Matching nodes in deterministic scene order |
| `scene.context` | No | none | Bounded project counts, bounds, facets, access, and limits |
| `scene.search` | No | `SceneSearchRequest` | Revision-bound source/derived node pages |
| `scene.bounds` | No | none | World-space bounds by node ID |
| `validation.run` | No | none | Diagnostic array |
| `referenceAsset.register` | Yes | `ReferenceAssetRegisterParams` | Immutable asset ID and bounded descriptor |
| `referenceAsset.get` | No | `{ assetId }` | Descriptor and streamed asset URLs |
| `referenceAsset.bindings` | No | `{ assetId, cursor?, limit? }` | Stable equipment page, limit `1..=100` |
| `evaluation.evaluate` | No | none | Evaluated mesh by node ID |
| `evaluation.query` | No | `EvaluationQueryRequest` | Bounded derived geometry summaries and optional payloads |
| `evaluation.waitScene` | No | `{ revision? }` | Published experimental GPU scene status |
| `evaluation.visibleInstances` | No | `{ targetRevision, planes[6][4] }` | Visible GPU buffer indices and stable node IDs; GPU compute when experimental scene sessions are ready, CPU fallback otherwise |
| `export.sourceGlb` | Yes | none | Source GLB export metadata |
| `export.runtimePreview` | Yes | `RuntimePreviewRequest` | Prepared committed or candidate preview metadata |

`export.runtimePreview` prepares the current document revision when `candidateDigest` is absent.
When a candidate digest is provided, it prepares that persisted ChangeSet candidate for review.
A commit is persisted before its revision event is published.

`system.capabilities.toolchain` reports `state`, contract/package versions, enabled operations,
and a sanitized failure reason. Runtime preview responses include the derived `assetId` and
content hash. Toolchain failures do not change the current revision or the previously valid
preview.

`system.capabilities.compute` reports host `mode`, the last selected backend, sanitized adapter
tier and backend family, effective memory budget, health and fallback reason, queue depth,
concurrency limit, crossover threshold, and the latest kernel/upload/readback/cache evidence.
It also reports `lastEvaluationMode`, resident count/bytes, peak bytes, topology and resident
hits, evictions, pipeline warm-up, preview packet version/bytes, browser upload bytes, snapshot
readback bytes, `residentHotPathHashes`, `handleLookupMs`, phase-specific lookup/plan/validation/
encode/submit/kernel/wait/snapshot/transport evidence, and `previewGeometryPacket: "1"` when packet
transport is enabled.
Native compute and browser rendering are separate: this status does not describe the Viewer's
WebGL2 or experimental browser WebGPU renderer.

The current default is CPU reference evaluation because the integrated Metal benchmark did not
meet the native GPU crossover. `cpu`, `auto`, and explicit `gpu` host modes exist in the scheduler;
`auto` falls back to CPU when GPU health, parity, budget, or device state is unsuitable, while
explicit GPU-only failure returns a stable `ICE3_STUDIO_GPU_*` diagnostic.

`export.runtimePreview` returns `packetUrl` beside the GLB fallback when
`STUDIO_PREVIEW_PACKETS=1` or the equivalent host configuration is enabled. The packet media type
is `application/vnd.ice3.preview-geometry; version=1`; clients must validate the whole packet and
atomically fall back to the GLB route on unsupported, invalid, stale, or over-budget responses.

After `project.close`, document and scene methods return `ICE3_STUDIO_PROJECT_NOT_FOUND`
until a project is opened again.

## Reference Assets

`referenceAsset.register` copies the source GLB, optional Prepared GLB, tileset directory, and
equipment manifest into the active project as immutable files. Registration is idempotent for the
same hashes and requires approval in `review-required` mode. Paths are relative to the configured
workspace; absolute paths, `..`, symlink components, malformed GLBs, and embedded digest mismatches
are rejected before publication.

The descriptor contains only bounded metadata: hashes, world bounds, source/mesh/material/
equipment counts, and one timestamp. Positions, indices, buffers, credentials, and absolute
filesystem paths are not returned. `document.get` stores one `reference_instance` node per editable
semantic binding:

```json
{
  "type": "reference_instance",
  "assetId": "asset-000001",
  "selectionId": "ice3-virtual-wwtp/equipment/AE-101"
}
```

Binding transform, visibility, material, and metadata edits use normal `node.update`,
`material.assign`, `node.delete`, and ChangeSet history. Impact analysis reports these edits as
local and reports zero generated reference geometry. `scene.context` and `evaluation.stats` add the
asset baseline triangle/mesh counts once per referenced digest.

For a baseline-only project, `export.sourceGlb` returns the registered source digest and path
without rebuilding geometry. If a binding has an authored transform, material, or visibility
override, export returns `ICE3_STUDIO_REFERENCE_EXPORT_UNSUPPORTED` rather than silently discarding
the overlay. Composing reference overrides into a new monolithic GLB is outside this phase.

The on-disk layout below the project is:

```text
references/<assetId>/
  asset.json
  source.glb
  prepared.glb
  equipment.json
  tiled/tileset.json
```

Viewer sessions request the small Document, descriptor, and 100-item binding pages. An
`ICETilesetLoader` streams coarse-to-fine tiles; imported selection IDs map back to Studio node
IDs, and the previous ready scene remains mounted until a replacement coarse tile is available.

Reproduce the real-asset gate with:

```bash
PATH=/Users/felix/.nvm/versions/node/v24.22.1/bin:$PATH \
  node scripts/run-large-reference-acceptance.mjs
```

The command writes `output/large-reference-acceptance.json`, an initial canvas screenshot, and a
rotated/zoomed selection screenshot beside it.

## Transaction Contract

```json
{
  "expectedRevision": 2,
  "dryRun": false,
  "idempotencyKey": "change-042",
  "operations": []
}
```

- `expectedRevision` rejects stale writes.
- `idempotencyKey` returns the original result when the same semantic request is retried.
- `dryRun` performs the same operation application, validation, and diff without persistence.
- A batch is atomic. One invalid operation rejects all mutations in the batch.
- Failed transactions and dry runs publish no revision event.

## Operations

| Type | Purpose |
|---|---|
| `node.create` | Create a group, primitive, mesh, or sweep node |
| `node.update` | Update name, transform, visibility, or metadata |
| `node.reparent` | Change a node parent |
| `node.delete` | Delete an unreferenced node |
| `primitive.update` | Replace primitive parameters |
| `mesh.replace` | Replace a triangle mesh |
| `material.upsert` | Create or update a material |
| `material.assign` | Assign or clear a node material |
| `relation.create` | Create a typed directed relationship |
| `relation.update` | Update relationship fields or metadata |
| `relation.delete` | Delete a relationship |
| `constraint.upsert` | Create or update a spatial constraint |
| `constraint.delete` | Delete a spatial constraint |
| `component.define` | Create an unreferenced component definition |
| `component.update` | Replace an existing component definition |
| `component.delete` | Delete a component definition with no live references |
| `instance.create` | Create an explicit component-instance Node |
| `instance.update` | Replace instance parameters and supported overrides |
| `instance.delete` | Delete an instance with no live connection references |
| `instanceSet.create` | Create a deterministic repeated component placement |
| `instanceSet.update` | Replace an instance-set layout and parameter overrides |
| `instanceSet.delete` | Delete an unreferenced instance set |
| `connection.create` | Create a routed connection for an existing Relation |
| `connection.update` | Replace connection endpoints or routing policy |
| `connection.delete` | Delete a connection and its linked Relation |
| `instance.patch` | Update only named instance fields, parameters, and metadata |
| `component.patch` | Update only definition name, parameter defaults, and metadata |
| `connection.patch` | Update only named connection fields and metadata |
| `material.patch` | Update only named material fields |

IDs, node order, material order, relation order, and diagnostic order are deterministic.
Lengths are meters, angles are radians, coordinates are right-handed Y-up, and material colors
are sRGB.

## Agent Context And Search

`system.agentManifest` reports `access`, `limits`, tool schema references, and short inspection,
proposal, review, commit, and recovery workflows. Manifest availability is descriptive; mutation
authorization remains enforced by the RPC boundary.

`scene.context` returns fixed-size counts, world bounds, units/coordinate system, source content
facets, up to 50 relation types, and effective access. It never returns complete node or geometry
collections.

`scene.search` accepts `scope: source | derived | both`, metadata/content/component/material/
visibility/bounds filters, an optional name/ID query, field projection, and a limit from 1 to 100.
It returns `{ revision, items, nextCursor }`. Cursor pages are bound to the revision and normalized
filters; using a stale cursor or a cursor with different filters returns
`ICE3_STUDIO_REVISION_CONFLICT`.

`evaluation.query` requires 1 to 32 unique derived node IDs. It returns vertex/index counts,
world bounds, and geometry digests by default. Positions and indices appear only when requested
with `summary`.

Agent proposal iteration uses:

```text
changeset.create
changeset.addOperations
changeset.impact
changeset.commit
```

`changeset.impact` returns the existing candidate digest plus bounded impact counts instead of a
complete scene payload. Its `scope` is `local`, `targeted`, `broad`, `full_rebuild`, or
`unavailable`. Local and targeted reports include affected source/derived counts, reused and
generated geometry, cache hits/misses, and reason codes. It may warm content-addressed geometry
entries but does not publish the candidate scene. Use post-commit `evaluation.stats` for exact
execution evidence on the committed revision.

Sparse patch payloads use camelCase fields. Omitted fields remain unchanged. Metadata set/remove
keys and parameter set/remove names may not overlap. Material and edge-color clearing use explicit
`clearMaterial` and `clearEdgeColor` flags.

## Large-Scene Authoring

Component parameters support finite Number, Vector3, Color, and Enumeration values. A Number may
declare inclusive minimum and maximum values; Colors require four components in `0.0..=1.0`.
Initial safety limits are a 16-level component-reference depth, 4096 parts per component, and
100,000 items in one instance set.

Instances may override typed parameters plus part material, visibility, and metadata. They cannot
replace arbitrary shared internal geometry. Definitions form an acyclic graph and cannot be
deleted while an instance or nested definition still references them.

Derived identities are deterministic:

```text
<instance-id>/<part-id>
<instance-set-id>/<six-digit-ordinal>/<part-id>
<connection-id>/route
```

These identities appear in `evaluation.evaluate`, `scene.bounds`, GLB extras, and Runtime
Selection. Component definitions and internal parts remain Document source data.

Example Operations:

```json
[
  {
    "type": "component.define",
    "definition": {
      "id": "component-000001",
      "name": "Equipment",
      "parameters": [],
      "parts": [{
        "id": "body",
        "parentId": null,
        "transform": {
          "translation": { "kind": "literal", "value": { "x": 0, "y": 0, "z": 0 } },
          "rotation": [0, 0, 0, 1],
          "scale": { "kind": "literal", "value": { "x": 1, "y": 1, "z": 1 } }
        },
        "content": {
          "kind": "box",
          "width": { "kind": "literal", "value": 1 },
          "height": { "kind": "literal", "value": 1 },
          "depth": { "kind": "literal", "value": 1 }
        },
        "materialRole": null,
        "metadata": {},
        "visible": true
      }],
      "ports": [],
      "metadata": {}
    }
  },
  {
    "type": "instance.create",
    "node_id": "node-000001",
    "parent_id": null,
    "name": "Equipment A",
    "transform": {
      "translation": { "x": 0, "y": 0, "z": 0 },
      "rotation": [0, 0, 0, 1],
      "scale": { "x": 1, "y": 1, "z": 1 }
    },
    "instance": {
      "definitionId": "component-000001",
      "parameters": {},
      "overrides": {},
      "metadata": {}
    },
    "material_id": null,
    "metadata": {}
  }
]
```

Dry-run and committed batches use the same validation, derived identities, and diff. A failed
dry-run is not the basis for a subsequent partial commit.

## Scene Queries

All filters are optional and combine with AND semantics:

```json
{
  "nodeId": "node-000004",
  "parentId": "node-000001",
  "metadata": { "role": "pump" },
  "relationType": "process",
  "relatedNodeId": "node-000002",
  "bounds": {
    "min": { "x": -10, "y": 0, "z": -10 },
    "max": { "x": 10, "y": 5, "z": 10 }
  }
}
```

`bounds` selects nodes whose world-space bounds intersect the query box. Relation filters select
nodes participating in a matching relation.

## History

`document.history` returns ordered entries:

```json
{
  "revision": 3,
  "digest": "<sha256>",
  "event": {
    "kind": "apply",
    "request": {
      "expectedRevision": 2,
      "dryRun": false,
      "idempotencyKey": "ai-batch-0042",
      "operations": []
    }
  }
}
```

Revision 0 records the initial snapshot. Successful apply, undo, and redo events are appended
without rewriting earlier entries.

## Revision Events

The authenticated WebSocket emits ordered JSON messages:

```json
{
  "event": "revision.committed",
  "revision": 3
}
```

On receipt, a client should read the latest document and request its current preview. Replacing
the document does not require replacing the canvas or rendering engine instance.

## Persistence and Export

`document.apply`, undo, and redo persist revision snapshots atomically. `export.sourceGlb`
writes a standard glTF 2.0 binary and reuses identical evaluated meshes for equivalent component
instances. Runtime optimization, LOD, and spatial partitioning remain outside Studio Core and are
performed by the configured Toolchain adapter.

See [Diagnostics](diagnostics.md) for stable failure codes.

## ChangeSet Review

AI proposals and human form edits use the persistent ChangeSet lifecycle described in
[ChangeSet Review](change-sets.md). Candidate approval binds `expectedVersion` to
`approvedDigest`; no draft operation affects the current revision before commit.
