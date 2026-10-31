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
| `scene.bounds` | No | none | World-space bounds by node ID |
| `validation.run` | No | none | Diagnostic array |
| `evaluation.evaluate` | No | none | Evaluated mesh by node ID |
| `evaluation.waitScene` | No | `{ revision? }` | Published experimental GPU scene status |
| `evaluation.visibleInstances` | No | `{ targetRevision, planes[6][4] }` | Visible GPU buffer indices and stable node IDs; GPU compute when experimental scene sessions are ready, CPU fallback otherwise |
| `export.sourceGlb` | Yes | none | Source GLB export metadata |
| `export.runtimePreview` | Yes | `{ revision? }` | Prepared preview metadata |

`export.runtimePreview` always prepares the current document revision. A commit is persisted
before its revision event is published.

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

IDs, node order, material order, relation order, and diagnostic order are deterministic.
Lengths are meters, angles are radians, coordinates are right-handed Y-up, and material colors
are sRGB.

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
