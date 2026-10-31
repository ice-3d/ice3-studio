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

## Start the Fixture

From the repository root:

```bash
npm --prefix apps/viewer run build
```

Then start the server:

```bash
STUDIO_TOOLCHAIN_CLI=scripts/preview-example.mjs \
ICE3_TOOLCHAIN_CLI=../ice3-toolchain/packages/cli/dist/main.js \
STUDIO_VIEWER_DIR=apps/viewer/dist \
cargo run -p studio-cli -- serve \
  --workspace . \
  --project wastewater-mini \
  --fixture fixtures/wastewater-mini \
  --native-gpu cpu
```

The server listens only on `127.0.0.1`, defaults to port `8120`, and prints its address, proposal
token, and a one-time browser bootstrap URL. The fixture is seeded at revision 2.
`--native-gpu cpu` is the release default. Experiment with `auto` or forced `gpu` only after
running `npm run benchmark:gpu` on the target host; the host mode is a startup policy and is never
controlled by an API request.

For iterative frontend work, keep the server running and start watch output in another terminal:

```bash
npm --prefix apps/viewer run dev
```

The watch build updates `apps/viewer/dist`; open the printed bootstrap URL to establish the
human approval cookie. The Workbench and API share `http://127.0.0.1:8120`.

The Workbench shows the current revision, generic object tree, validation diagnostics, form
inspection, and the prepared preview. A revision event triggers a fresh document read and preview
load without replacing the canvas.

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
