# ICE3 Studio Toolchain Sidecar Integration Design

> Document ID: ICE3-STUDIO-SPEC-004
>
> Status: Historical
>
> Owner: ICE3 Studio maintainers

> **历史说明：** Sidecar 已由 Studio 原生 preview 和 asset pipeline 取代。
> 本文仅记录退役前的设计，不得作为当前部署或接口依据。

## 1. Summary

ICE3 Studio should formally depend on ICE3 Toolchain for asset preparation capabilities while
keeping the Rust modeling core independent. The dependency is a pinned, local, versioned
sidecar service, not a TypeScript import into `studio-core` and not a developer-only executable
path convention.

Studio supplies modeling intent, document semantics, ChangeSets, approvals, and canonical source
GLB. Toolchain supplies source import, geometry preparation, validation, LOD, partitioning,
render batching, tileset generation, and Runtime preview generation.

```text
studio-core
    no dependency on Toolchain

studio-server
    -> studio-toolchain-client (Rust)
        -> pinned local Toolchain sidecar
            -> @ice3/toolchain
```

## 2. Goals

1. Make the existing implicit Toolchain integration explicit, versioned, and testable.
2. Reuse Toolchain import, preparation, validation, LOD, partition, and preview capabilities
   without moving modeling semantics out of Studio.
3. Preserve local-first and offline operation after installation.
4. Keep Toolchain failures from blocking document editing, review, commit, or source GLB export.
5. Provide capability negotiation so Studio disables only unsupported features.
6. Run Toolchain as a long-lived local process to avoid per-request startup cost.
7. Keep the dependency compatible with Studio's requirement that Node.js is not the modeling
   kernel or Runtime.

## 3. Non-Goals

- Moving Document, Operation, candidate digest, approval, or persistence into Toolchain.
- Calling Toolchain from `studio-core`.
- Allowing API clients to supply executable paths, command arguments, environment variables, or
  arbitrary scripts.
- Requiring a network service or runtime package download.
- Replacing Toolchain's GLB/Tiles output contract with a Studio-private asset format.
- Supporting every Toolchain input format in the first integration milestone.
- Defining boolean geometry, sculpting, or an import UI in this design.

## 4. Design Principles

### 4.1 Product dependency, Core isolation

The Studio product depends on a managed Toolchain distribution. The Rust modeling core does not.
Only the Server-side adapter knows how to start and communicate with the sidecar.

### 4.2 Assets are derived

All output produced through Toolchain is a derived asset. It never mutates the Document or
determines approval identity. Outputs are addressable by content hash and replaceable.

### 4.3 One formal contract

Studio and Toolchain communicate only through a versioned, machine-readable contract. Source
imports, command-line warnings, stderr text, and filesystem conventions are not protocol.

### 4.4 Local by default

The sidecar runs on the same machine, uses project-confined files, requires no credentials, and
continues to work without Internet access.

### 4.5 Capability-scoped degradation

If the sidecar is missing, incompatible, busy, or failed, Studio reports a structured degraded
state. Core editing and source export remain available.

## 5. Current State

Studio Server currently launches a Toolchain executable for Runtime preview. Configuration is
provided through `STUDIO_TOOLCHAIN_CLI`, commands are assembled inside the preview adapter, and
the test harness substitutes local scripts.

That implementation proves the workflow but has weaknesses:

- dependency availability is implicit;
- no package or contract version is negotiated;
- process startup occurs in request paths;
- stdout/stderr and exit-status handling is bespoke;
- capabilities are not reported to clients;
- packaging and production discovery are undefined;
- concurrency, cancellation, timeouts, and long-running jobs have no shared model.

This design replaces those conventions with the sidecar contract below.

## 6. Dependency Classification

| Layer | Depends on Toolchain | Rationale |
|---|---:|---|
| `studio-core` | No | Owns modeling and document semantics |
| Viewer | No direct dependency | Uses Server APIs only |
| `studio-server` | Through adapter | Orchestrates local capabilities |
| `studio-toolchain-client` | Yes | Owns process, protocol, versioning, and mapping |
| Desktop/local product bundle | Yes | Ships a pinned sidecar with enhanced features |

The dependency must not be introduced as a Cargo dependency on generated JavaScript or as an npm
package linked into the modeling kernel.

## 7. System Context

```text
AI Agent / Web Workbench
          |
          | public Studio API
          v
    Studio Server
      |        |
      |        +---- studio-core ---- Document / Operations / source GLB
      |
      +---- studio-toolchain-client
                  |
                  | stdio JSON-RPC
                  v
          Toolchain Sidecar
                  |
                  +---- import / prepare / validate
                  +---- LOD / partition / render batches
                  +---- tileset / Runtime preview
                  |
                  v
       project-confined derived assets
```

## 8. Sidecar Protocol

### 8.1 Transport

- JSON-RPC 2.0 messages are newline-delimited on child process stdio pipes.
- stdout carries protocol messages only.
- stderr carries bounded diagnostics and operational logs.
- Every production process starts from the pinned manifest and fixed allowlisted arguments.
- The adapter never invokes a shell.

### 8.2 Handshake

The first request is:

```json
{
  "jsonrpc": "2.0",
  "id": "capabilities-1",
  "method": "toolchain.capabilities",
  "params": {}
}
```

The response includes:

```json
{
  "contractVersion": "1.0.0",
  "packageVersion": "0.2.0",
  "operations": ["asset.validate", "asset.prepare", "asset.partition", "asset.preview"],
  "inputFormats": ["glb", "gltf"],
  "features": {
    "lod": true,
    "partition": true,
    "tileset": true,
    "renderBatching": true,
    "cancellation": true
  },
  "limits": {
    "maxConcurrentJobs": 2,
    "maxSourceBytes": 2147483648
  }
}
```

The adapter caches capabilities for the process lifetime. An incompatible contract marks the
sidecar unavailable but does not terminate Studio.

### 8.3 Initial Methods

| Method | Purpose |
|---|---|
| `toolchain.capabilities` | Contract/package version and feature discovery |
| `asset.validate` | Validate a Studio-produced source GLB |
| `asset.prepare` | Produce normalized Prepared GLB |
| `asset.partition` | Produce partition and LOD assets |
| `asset.preview` | Produce Runtime preview metadata and GLB |
| `$/cancel` | Cancel an in-flight request by JSON-RPC request ID |

Source-format import methods may be added only after the contract version that defines their
input/output shape is available.

### 8.4 Job Behavior

- Long operations emit bounded `job.progress` notifications.
- The protocol reader remains available while a worker executes.
- Cancellation is cooperative and must leave no final partial asset.
- The adapter applies host-controlled timeout and concurrency policy.
- Interactive preview jobs run before queued background partition jobs.

## 9. Workspace and File Contract

The sidecar starts with one canonical project workspace root supplied by Studio Server.

- Requests contain workspace-relative paths only.
- Canonicalization rejects absolute paths, `..` traversal, and symlink escapes.
- Temporary outputs stay under `cache/toolchain/tmp`.
- Final outputs stay under `cache/toolchain/<content-key>`.
- Publication uses write-then-atomic-rename.
- Failed or cancelled jobs remove temporary outputs.
- Client source files and generated assets never enter Git.

The sidecar cannot access another project through this process.

## 10. Output Contract

Successful asset operations return:

```json
{
  "assetId": "prepared-<hash>",
  "contentHash": "sha256:<hex>",
  "profileVersion": "ice3-prepared-glb-v1",
  "manifestPath": "cache/toolchain/<hash>/manifest.json",
  "primaryPath": "cache/toolchain/<hash>/asset.glb",
  "byteLength": 123456,
  "diagnostics": [],
  "warnings": []
}
```

Partition and preview operations may add typed derived paths and counts. Studio validates that
all paths remain inside the project cache before publishing them through API responses.

Toolchain diagnostics retain their stable `ICE3_TOOL_*` codes. Studio API errors wrap them with a
Studio-side diagnostic and preserve sanitized Toolchain details.

## 11. Version Compatibility

- The protocol has an independent semantic version, starting at `1.0.0`.
- Studio declares supported contract ranges, not exact patch equality.
- The installed Toolchain package version is reported separately.
- Studio uses the newest common capability set.
- A major contract mismatch disables Toolchain-backed features and reports
  `ICE3_STUDIO_TOOLCHAIN_CONTRACT_INCOMPATIBLE`.
- A missing feature disables only that method.
- Every capability/evidence response includes contract and package versions.

The authoritative schema should live in the shared contract repository and be vendored into both
implementations during release builds.

## 12. Packaging and Local Installation

### 12.1 Release Product

- Studio release bundles a pinned Toolchain package and compatible Node.js runtime.
- The launcher reads a SHA-256-hashed manifest that records package version, contract version,
  runtime version, platform, architecture, and artifact digest.
- Studio does not download Toolchain at operation time.
- The installation is relocatable and works from read-only application directories; mutable
  state goes under project cache and user application data.

### 12.2 Development

- A sibling Toolchain checkout may be selected for development, but it must satisfy the same
  capabilities handshake.
- Environment overrides are development-only and unavailable through the public API.
- Test adapters may be injected only into test server construction.

### 12.3 Upgrade

- Studio upgrade installs a new managed sidecar atomically.
- An active server keeps its current process until jobs finish or the server restarts.
- Old sidecar directories are removed only after no manifest or active process references them.

## 13. Process Lifecycle

- Start lazily on the first Toolchain operation, or during startup prewarm when configured.
- Perform capability handshake before accepting jobs.
- Reap the process on Server shutdown.
- Restart after a crash with bounded backoff and a maximum of two attempts per job.
- Disable the sidecar after repeated startup or protocol failures.
- An idle process may stop after a configurable interval; startup cost is measured separately
  from operation latency.

## 14. Concurrency and Resource Control

- Host configuration controls maximum concurrent jobs and memory budget.
- Interactive preview has priority over background partition and LOD.
- Oversized jobs fail before Toolchain allocates large temporary files.
- Adapter queue depth is visible in capabilities/evidence.
- A slow or failed Toolchain job cannot block health checks, document reads, or revision events.
- Work runs outside the async event loop.

## 15. Cache and Idempotency

Toolchain cache keys include:

- source content hash;
- operation and normalized options;
- Toolchain contract and package version;
- runtime profile and budget version;
- output-affecting platform limits where applicable.

Cache writes are atomic and verify output content hashes. A cache hit returns the same manifest
and primary-asset identity without starting a job. Cache identity is derived and does not alter
ChangeSet candidate identity.

## 16. Failure and Degradation

| Failure | Studio behavior |
|---|---|
| Sidecar missing | Editing remains available; enhanced assets unavailable |
| Contract incompatible | Disable Toolchain methods; report exact supported/required range |
| Invalid source | Return structured import/preparation diagnostic |
| Timeout | Cancel process work if supported; remove partial output |
| Crash | Retry within policy, then mark unavailable and fall back |
| Invalid output path | Reject result and report protocol violation |
| Corrupt cache entry | Quarantine entry and regenerate |

Canonical source GLB export and committed preview availability remain governed by Studio. A
Toolchain failure never changes the current revision.

## 17. Security

- No request-supplied executable, shell command, environment variable, or arbitrary argument.
- Fixed method allowlist and fixed launch manifest.
- Workspace-confined filesystem access and output path validation.
- No network requirement and no credential forwarding by default.
- Protocol messages and logs have size limits.
- Child process receives a minimal environment.
- Symlink escapes and temporary-file races are rejected.
- Toolchain cannot access Studio approval tokens or Agent credentials.

## 18. Observability

Studio records:

- sidecar state and restart count;
- contract/package/runtime versions;
- operation kind and cache key;
- queue wait, execution time, output bytes, and exit status;
- sanitized Toolchain diagnostics;
- timeout, cancellation, and fallback count.

The Workbench exposes read-only Toolchain availability and versions separately from browser
rendering and native GPU compute.

## 19. Testing

### 19.1 Contract Tests

- Handshake accepts compatible ranges and rejects major mismatches.
- Unknown methods and malformed messages fail with stable diagnostics.
- stdout protocol contamination is detected.
- Capability gaps disable only affected UI/API actions.

### 19.2 Security Tests

- Absolute paths, traversal, and symlink escape are rejected.
- Request fields cannot inject command arguments or environment values.
- Oversized messages and outputs are bounded.
- Temporary files are removed after cancellation and failure.

### 19.3 Lifecycle Tests

- Lazy start, concurrent startup, crash restart, timeout, shutdown, and idle exit.
- Queue fairness prioritizes interactive preview.
- Health and revision events remain responsive during slow Toolchain work.

### 19.4 Integration Tests

- Existing Runtime preview behavior migrates with no API regression.
- Prepared GLB and partition outputs match Toolchain's own conformance tests.
- Identical requests hit the content cache deterministically.
- Candidate approval digest is unchanged by sidecar versions or cache state.

## 20. Migration Plan Boundary

The first implementation milestone should:

1. Add the shared contract schema and fixture server.
2. Add `studio-toolchain-client` to `studio-server`.
3. Replace the current one-shot preview command with the sidecar adapter.
4. Add Toolchain capability/evidence to `system.capabilities`.
5. Bundle a managed development/release sidecar and remove production reliance on
   `STUDIO_TOOLCHAIN_CLI`.
6. Add validation, timeout, cancellation, restart, and cache tests.

Source-format import and advanced LOD/partition UI follow only after this baseline is stable.

## 21. Acceptance Criteria

1. `studio-core` still has no Toolchain, Engine, Viewer, or Node dependency.
2. A Studio release runs with its pinned Toolchain sidecar without user path configuration.
3. Preview operations use a long-lived sidecar and do not start one process per request.
4. Capabilities expose contract, package, runtime versions, and supported features.
5. Unsupported or failed Toolchain features degrade without blocking core editing.
6. No API request can inject commands, executable paths, or arbitrary environment values.
7. Sidecar outputs are project-confined, content-addressed, atomic, and cacheable.
8. Existing preview, ChangeSet, and revision workflows pass without regression.
9. A slow or crashed Toolchain process cannot block health, reads, or revision events.
10. Tests cover compatible/incompatible contracts, path escape, timeout, cancellation, crash,
    cache corruption, and partial-output cleanup.

## 22. Risks

| Risk | Mitigation |
|---|---|
| Duplicate dependencies and package size | Pin one managed sidecar; defer unused import formats |
| Node process cold start | Persistent sidecar and startup prewarm |
| Contract drift across repositories | Shared versioned schema and cross-repo conformance fixtures |
| Sidecar crash or leak | Supervisor, bounded restart, idle shutdown, process-tree cleanup |
| Large temporary assets | Admission control, quotas, project cache cleanup |
| Misinterpreting derived output as document truth | Separate asset APIs and content hashes; no mutation capability |
| Windows/macOS/Linux packaging differences | Manifest-driven launcher and platform conformance tests |

## 23. Decisions

- Studio product depends on Toolchain; Studio Core does not.
- Sidecar communication uses a fixed JSON-RPC contract over stdio.
- Toolchain runs as a long-lived, locally bundled process.
- Capability negotiation and explicit degradation are mandatory.
- Toolchain outputs remain derived, content-addressed assets.
- Production behavior does not depend on `STUDIO_TOOLCHAIN_CLI`.
