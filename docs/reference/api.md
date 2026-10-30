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
| `system.capabilities` | No | none | API version and method inventory |
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
| `export.sourceGlb` | Yes | none | Source GLB export metadata |
| `export.runtimePreview` | Yes | `{ revision? }` | Prepared preview metadata |

`export.runtimePreview` always prepares the current document revision. A commit is persisted
before its revision event is published.

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

IDs, node order, material order, relation order, and diagnostic order are deterministic.
Lengths are meters, angles are radians, coordinates are right-handed Y-up, and material colors
are sRGB.

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
writes a standard glTF 2.0 binary. Runtime optimization, LOD, and spatial partitioning remain
outside Studio Core and are performed by the configured Toolchain adapter.

See [Diagnostics](diagnostics.md) for stable failure codes.

## ChangeSet Review

AI proposals and human form edits use the persistent ChangeSet lifecycle described in
[ChangeSet Review](change-sets.md). Candidate approval binds `expectedVersion` to
`approvedDigest`; no draft operation affects the current revision before commit.
