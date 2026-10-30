# Architecture

> Document ID: ICE3-STUDIO-ARC-001
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

ICE3 Studio is a local-first authoring system designed for external AI operation. Natural
language understanding stays outside Studio. Studio receives typed modeling intent through a
public API and turns it into deterministic document revisions.

## Boundary

```text
external AI or script
        |
        | JSON-RPC 2.0
        v
Studio Server (loopback)
        |-------------------+-------------------+
        |                   |                   |
        v                   v                   v
Rust Studio Core
  document + operations + validation + geometry + export
        |             Native GPU Compute
        |               bounded wgpu kernels + CPU parity
        |                   |
        +-------------------+
        |
        +----> revision snapshots and history
        |
        +----> source GLB
        |
        +----> revision event ----> thin Web Viewer
        |
        +----> studio-toolchain-client ----> pinned Toolchain Sidecar
                                                  |
                                                  +----> prepared GLB / preview / partition
```

Studio Core has no dependency on the Viewer, Engine, Toolchain, or an AI provider SDK. Node.js
is used only for Viewer build tooling and the out-of-process Toolchain adapter.

## Components

### `studio-core`

Owns stable types, generic modeling operations, primitive and sweep evaluation, materials,
relations, constraints, diagnostics, document diffs, and standard GLB serialization. It has no
file, process, network, or UI access.

The `large_scene` module owns generic component definitions, instances, deterministic instance
sets, typed parameter resolution, port-aware connections, and CPU reference expansion. It emits
shared evaluated meshes and stable derived identities without becoming industry-specific.

### `studio-project`

Owns project paths, writer locks, atomic snapshots, revision history, undo/redo, and snapshot
integrity. It also owns the bounded content-addressed geometry cache under
`cache/geometry/`. A snapshot is persisted before the new revision becomes externally visible.

### `studio-gpu`

Owns the local adapter/device lifecycle, bounded `wgpu` buffers, WGSL kernels, CPU/GPU parity
checks, and backend statistics for native geometry compute. It does not read projects, serve HTTP,
approve revisions, or replace browser WebGPU rendering. CPU remains the canonical evaluator and is
the default automatic path until adapter-specific benchmarks demonstrate a positive crossover.

### `studio-server`

Owns the local HTTP and WebSocket boundary. It validates authentication and parameters, applies
one transaction at a time, persists accepted revisions, and publishes revision events. Blocking
filesystem and Toolchain work runs on Tokio blocking workers so health and event delivery remain
responsive. Scene bounds and evaluation read from the same cached derived scene so a request does
not evaluate equivalent component instances twice. `ComputeService` enforces GPU memory,
concurrency, priority, cancellation-safe admission, backend health, and automatic CPU fallback.

### `studio-toolchain-client` and Toolchain sidecar

Owns the pinned sidecar process, capability handshake, newline-delimited JSON-RPC request map,
timeout, bounded stderr capture, restart policy, and project-relative asset paths. Toolchain
prepares derived assets and never receives document mutation authority.

### `studio-cli`

Starts the local server and generates deterministic schemas and declarations.

### Viewer

Consumes public read APIs and revision events, displays the prepared GLB, and reports selection
and validation state. It does not mutate the document directly.

## Source of Truth

The serializable `Document` plus its operation history is durable truth. Component definitions,
instances, instance-set rules, and connection intents are source data. Triangle meshes, derived
part identities, world bounds, diagnostics, GLB buffers, and Toolchain previews are derived
values. This keeps AI operations explainable, replayable, and diffable.

Every committed transaction increments the revision once. Stable object IDs survive renames,
reloads, undo, and redo. Atomic snapshots and idempotency records make retries safe within the
local project boundary.

## Industry Layering

Core primitives and operations are generic. Industrial concepts use metadata, relations, and
assemblies above the Core boundary. `wastewater-mini` is an acceptance fixture, not a
hard-coded process type in the modeling API.

## Security Posture

- HTTP and WebSocket bind to `127.0.0.1`.
- Writes require a process-scoped bearer token.
- Project IDs and resources are confined to the configured workspace.
- Operations cannot submit paths, URLs, shell commands, scripts, or arbitrary expressions.
- The Toolchain executable comes from a host-owned pinned manifest, never from an API request.
- GPU requests cannot submit WGSL, select an arbitrary adapter, or bypass host budgets.
- Diagnostics exclude credentials and out-of-workspace paths.

## Deferred Scope

Boolean geometry, free-form curves, sculpting, collaborative editing, cloud tenancy, large
static asset import, and persistent GPU buffer pooling are outside this MVP. Runtime LOD and
spatial partitioning remain Toolchain responsibilities. Native GPU execution remains experimental
until the release benchmark shows a stable latency gain.
