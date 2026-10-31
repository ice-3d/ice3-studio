# Getting Started

> Document ID: ICE3-STUDIO-GUIDE-START-001
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

ICE3 Studio MVP runs locally. Rust owns the modeling document, operations, validation,
revisions, persistence, and GLB export. The browser Viewer only reads and displays through
the public API.

## Prerequisites

- Rust 1.98 with Cargo.
- Node.js 24.22.1 and npm 11 or newer for Viewer build tooling and Toolchain integration.
- A browser with WebGL2 for the current Viewer.

Install Viewer dependencies once:

```bash
npm --prefix apps/viewer ci
```

## Start the Server

The default development scene is the full wastewater reference plant: 204 semantically bound
equipment items over 5.76M effective triangles, streamed from a layered reference asset.

```bash
npm --prefix apps/viewer run build   # once, and after Viewer changes
npm run dev
```

`npm run dev` starts `studio-cli` on `127.0.0.1:8149` with project `wwtp-reference`. The first
run registers the reference asset and commits the 204 bindings (about two minutes); every later
run reuses the stored project and is ready in about two seconds. The command prints the Viewer
URL and keeps serving until `Ctrl+C`.

Open the printed URL to load the scene. The URL carries the agent token, and the approval token
is `approval-wwtp-reference`. Set `ICE3_WWTP_PROJECT_ID` to serve an isolated copy of the scene,
and `STUDIO_ONE_SHOT=1` to run the example once without staying in the foreground.

### Small smoke fixture

`wastewater-mini` is a 16 KB JSON fixture (11 nodes) used by `npm run verify`, the E2E server,
and the MVP benchmark. It exists for fast contract checks, not as a demo asset:

```bash
npm run dev:mini
```

The server listens only on `127.0.0.1` and defaults to port `8120`. `--native-gpu cpu` is the
release default. Experiment with `auto` or forced `gpu` only after running
`npm run benchmark:gpu` on the target host; the host mode is a startup policy and is never
controlled by an API request.

For iterative frontend work, keep the server running and start watch output in another terminal:

```bash
npm --prefix apps/viewer run dev
```

The watch build updates `apps/viewer/dist`; reload the browser tab to pick it up. The Workbench
and API share the same origin.

The Workbench shows the current revision, the source/derived object tree, the 3D viewport,
typed property forms, the Agent panel, ChangeSet review, validation diagnostics, and the
prepared preview. A revision event triggers a fresh document read and preview load without
replacing the canvas, so selection and camera survive a commit.

## Model Through the API

All model changes use `POST /rpc` with JSON-RPC 2.0 and a bearer token. A write must include an
expected revision and an idempotency key:

```json
{
  "jsonrpc": "2.0",
  "id": "add-pump",
  "method": "document.apply",
  "params": {
    "expectedRevision": 2,
    "dryRun": false,
    "idempotencyKey": "add-pump-001",
    "operations": [
      {
        "type": "node.create",
        "node_id": "node-000012",
        "parent_id": null,
        "name": "Standby Pump",
        "transform": {
          "translation": { "x": 0, "y": 1, "z": 0 },
          "rotation": [0, 0, 0, 1],
          "scale": { "x": 1, "y": 1, "z": 1 }
        },
        "content": {
          "type": "primitive",
          "kind": "cylinder",
          "radius": 1.2,
          "height": 1.8,
          "segments": 20
        },
        "material_id": null,
        "metadata": { "role": "pump" }
      }
    ]
  }
}
```

Use `dryRun: true` to validate and inspect the diff without creating a revision. If a
transaction is rejected, nothing from that batch is persisted.

## Review a ChangeSet

AI proposals and human form edits both go through a ChangeSet. In the default `review-required`
mode an agent token can stage and evaluate, but cannot commit:

1. `changeset.create { baseRevision, title }`
2. `changeset.addOperations { changeSetId, expectedVersion, entries }`
3. `changeset.evaluate { changeSetId, expectedVersion }` → status `ready` plus a candidate digest
4. `changeset.commit { changeSetId, expectedVersion, approvedDigest, idempotencyKey }` carrying
   the approval cookie

The commit is rejected when the digest no longer matches the approved candidate, when the
ChangeSet is not `ready`, or when the caller presents only the agent token. `changeset.impact`
reports whether a proposal stays local or forces a wider rebuild, so the reviewer can judge the
cost before approving. See [ChangeSet 审阅](../reference/change-sets.md).

## Verify and Benchmark

Run every repository gate:

```bash
npm run verify
```

Run the release benchmark:

```bash
npm run benchmark
```

The benchmark writes machine-readable p50/p95 results to
`output/studio-mvp-benchmark.json`. It fails if a specification performance limit is exceeded.

## Stop and Recover

Stop the server with `Ctrl-C`. Project snapshots are written atomically before a revision
event is published. If the process stops during a write, the previous current revision remains
loadable. A lock left by a dead process is recovered on the next open.
