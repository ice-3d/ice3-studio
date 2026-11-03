# ICE3 Studio Toolchain Sidecar Implementation Plan

> Document ID: ICE3-STUDIO-PLAN-005
>
> Status: Historical
>
> Owner: ICE3 Studio maintainers

> **历史状态：** Sidecar 已退役，本文只保留迁移和审计证据；当前生产链见
> [Asset Pipeline](../reference/asset-pipeline.md)。

> **历史说明：** Sidecar 已由 Studio 原生 preview 和 asset pipeline 取代。
> 本文仅记录退役前的设计，不得作为当前部署或接口依据。

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Studio's one-shot preview command convention with a pinned, versioned,
long-lived Toolchain sidecar while keeping `studio-core` independent.

**Architecture:** A shared v1 contract defines JSON-RPC messages and asset results. Toolchain
implements a stdio sidecar. A new Rust `studio-toolchain-client` owns process launch, NDJSON
framing, request multiplexing, timeouts, capabilities, and shutdown. `studio-server` uses that
client for preview and publishes capability/evidence state.

**Tech Stack:** Rust 1.98, Tokio process primitives where useful, TypeScript 7, Node.js 24,
JSON-RPC 2.0 NDJSON, JSON Schema fixtures, esbuild React Workbench.

**Spec:** [`ICE3-STUDIO-SPEC-004`](../specs/2026-10-30-studio-toolchain-sidecar-design.md)

## Global Constraints

- `studio-core` must not depend on Toolchain, Engine, Viewer, or Node.js.
- The Studio product ships a pinned Toolchain sidecar; production does not depend on
  `STUDIO_TOOLCHAIN_CLI`.
- stdout from the sidecar contains protocol messages only; logs go to stderr.
- Requests contain workspace-relative paths and cannot inject commands or environment values.
- Toolchain outputs are derived assets and can never mutate Document, revision, approval, or
  candidate identity.
- Sidecar failure must not block health, document reads, Review, commit, or source GLB export.
- JSON-RPC request and response size is capped at 8 MiB.
- Initial method set: `toolchain.capabilities`, `asset.validate`, `asset.prepare`,
  `asset.preview`, `asset.partition`, and `$/cancel`.
- Contract version starts at `1.0.0`; capability negotiation reports the package version
  separately.
- Final output uses temp-write plus atomic rename; failed/cancelled jobs leave no published file.
- Both repositories keep their configured Git identity and use dedicated feature branches.
- Existing unrelated worktree changes must remain unstaged.

## Review Focus

- An incompatible sidecar contract must disable only Toolchain-backed features, not project
  editing or revision operations.
- Absolute paths, `..` traversal, and symlink escapes must be rejected before source access or
  output allocation.
- A sidecar crash mid-preview must not hang the request, publish a partial GLB, or mutate the
  current revision.
- Diagnostic output accidentally written to stdout must be detected rather than interpreted as a
  protocol response.
- Cancelled or timed-out partition jobs must clean temporary files and leave prior cache content
  intact.

---

## Cross-Repository File Map

## Execution Setup

- Use branch `feat/toolchain-sidecar-contract` in `ice3-doc`.
- Use `ice3-toolchain`'s committed `d4a4796` baseline and create branch `feat/studio-sidecar` in
  the same checkout; the earlier large-plant work is now a clean prerequisite.
- Use the existing `feat/toolchain-sidecar` branch in `ice3-studio`.
- Record the exact base commit of each repository in the execution ledger before Task 1.
- Stage only files named by the active task; never use repository-wide staging from the shared
  parent workspace.

### `ice3-doc`

- Create `docs/contracts/toolchain-sidecar-contract-v1.md`: authority for protocol semantics,
  versions, safety, and asset result fields.
- Create `docs/contracts/toolchain-sidecar-v1.schema.json`: machine-readable envelope and payload
  schema.
- Create `scripts/fixtures/toolchain-sidecar/capabilities-v1.json`: frozen capability response.
- Create `scripts/fixtures/toolchain-sidecar/preview-request-v1.json`: frozen request.
- Create `scripts/fixtures/toolchain-sidecar/preview-result-v1.json`: frozen result.
- Create `scripts/toolchain-sidecar-contract.test.mjs`: validates fixtures and version fields.
- Modify `docs/README.md`: links `ICE3-DOC-CTR-002`.
- Modify `package.json`: includes the new contract test.

### `ice3-toolchain`

- Create `packages/cli/src/sidecar/protocol.ts`: message types, size limits, NDJSON parser.
- Create `packages/cli/src/sidecar/workspacePaths.ts`: canonical workspace-relative path
  validation.
- Create `packages/cli/src/sidecar/operations.ts`: validate/prepare/preview/partition execution.
- Create `packages/cli/src/sidecar/server.ts`: handshake, request dispatch, cancellation, and
  stdout discipline.
- Create `tests/sidecar/protocol.test.ts`: parser and malformed-message tests.
- Create `tests/sidecar/operations.test.ts`: asset operation and path-safety tests.
- Create `tests/sidecar/server.test.ts`: end-to-end NDJSON process test.
- Modify `packages/cli/src/main.ts`: `sidecar` command entry.
- Modify `packages/cli/package.json`: export the sidecar module and describe the command.
- Modify `packages/cli/README.md`: development command and stdout contract.

### `ice3-studio`

- Create `crates/studio-toolchain-client/Cargo.toml`: adapter crate manifest.
- Create `crates/studio-toolchain-client/src/lib.rs`: public exports.
- Create `crates/studio-toolchain-client/src/protocol.rs`: typed v1 messages.
- Create `crates/studio-toolchain-client/src/workspace_path.rs`: request path validation.
- Create `crates/studio-toolchain-client/src/client.rs`: process lifecycle and multiplexing.
- Create `crates/studio-toolchain-client/tests/fixtures/fake-sidecar.mjs`: deterministic protocol
  worker.
- Create `crates/studio-toolchain-client/tests/client_contract.rs`: handshake/timeouts/errors.
- Modify `Cargo.toml` and `Cargo.lock`: workspace crate.
- Modify `crates/studio-server/Cargo.toml`: adapter dependency.
- Modify `crates/studio-server/src/preview.rs`: Toolchain-backed preview.
- Modify `crates/studio-server/src/rpc.rs`: sidecar lifecycle and capability state.
- Modify `crates/studio-server/src/changeset.rs`: candidate preview through the shared client.
- Modify `crates/studio-server/src/lib.rs`: public adapter status types if needed.
- Create `crates/studio-server/tests/toolchain_sidecar.rs`: server integration and fallback tests.
- Create `toolchain/manifest.template.json`: managed bundle metadata.
- Create `scripts/stage-toolchain.mjs`: verify/copy package and generate manifest hashes.
- Create `scripts/toolchain-launch.mjs`: development launcher using the generated manifest.
- Create `scripts/verify-toolchain-bundle.mjs`: release artifact verification.
- Create `scripts/verify-toolchain-sidecar.mjs`: cross-repository live sidecar smoke test.
- Modify `scripts/verify.mjs`: CPU-only contract tests plus optional live sidecar gate.
- Modify `apps/viewer/src/api/StudioClient.ts`, `SessionStore.ts`, and Workbench components for
  Toolchain status.
- Modify `docs/README.md`, `docs/architecture.md`, `docs/reference/api.md`, and
  `docs/reference/diagnostics.md`.
- Modify the spec and this plan to `Implemented` only after both repository gates pass.

---

### Task 1: Freeze the Shared Contract and Examples

**Files:**

- Create: `<workspace>/ice3-doc/docs/contracts/toolchain-sidecar-contract-v1.md`
- Create: `<workspace>/ice3-doc/docs/contracts/toolchain-sidecar-v1.schema.json`
- Create: `<workspace>/ice3-doc/scripts/fixtures/toolchain-sidecar/capabilities-v1.json`
- Create: `<workspace>/ice3-doc/scripts/fixtures/toolchain-sidecar/preview-request-v1.json`
- Create: `<workspace>/ice3-doc/scripts/fixtures/toolchain-sidecar/preview-result-v1.json`
- Create: `<workspace>/ice3-doc/scripts/toolchain-sidecar-contract.test.mjs`
- Modify: `<workspace>/ice3-doc/docs/README.md`
- Modify: `<workspace>/ice3-doc/package.json`

**Interfaces:**

- Produces: contract ID `ICE3-DOC-CTR-002`.
- Produces: contract semantic version `1.0.0`.
- Produces: JSON fields `contractVersion`, `packageVersion`, `operations`, `features`, `limits`,
  `assetId`, `contentHash`, `manifestPath`, `primaryPath`, `byteLength`, `diagnostics`,
  `warnings`.

Before implementation, create a new `ice3-doc` feature branch from `main`; do not alter unrelated
worktrees.

- [ ] **Step 1: Write the failing contract test**

Create `scripts/toolchain-sidecar-contract.test.mjs`:

```js
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

const fixture = (name) => readFile(
  new URL(`./fixtures/toolchain-sidecar/${name}`, import.meta.url),
  'utf8',
).then(JSON.parse);

test('freezes contract version and required methods', async () => {
  const capabilities = await fixture('capabilities-v1.json');
  assert.equal(capabilities.contractVersion, '1.0.0');
  assert.equal(capabilities.packageVersion, '0.2.0');
  assert.deepEqual(capabilities.operations, [
    'toolchain.capabilities',
    'asset.validate',
    'asset.prepare',
    'asset.preview',
    'asset.partition',
    '$/cancel',
  ]);
});

test('preview request and result are path-confined', async () => {
  const request = await fixture('preview-request-v1.json');
  const result = await fixture('preview-result-v1.json');
  assert.equal(request.params.sourcePath, 'cache/preview/00000002-source.glb');
  assert.equal(result.contentHash.startsWith('sha256:'), true);
  assert.equal(result.primaryPath.startsWith('cache/toolchain/'), true);
  assert.equal(result.primaryPath.includes('..'), false);
});

test('schema defines all frozen payload families', async () => {
  const schema = JSON.parse(await readFile(
    new URL('../docs/contracts/toolchain-sidecar-v1.schema.json', import.meta.url),
    'utf8',
  ));
  assert.deepEqual(
    Object.keys(schema.$defs).sort(),
    ['assetResult', 'capabilities', 'errorResult', 'previewRequest', 'progressNotification'],
  );
});
```

- [ ] **Step 2: Verify RED**

Run in `ice3-doc`:

```bash
node --test scripts/toolchain-sidecar-contract.test.mjs
```

Expected: failure because the fixture files do not exist.

- [ ] **Step 3: Add frozen fixtures and schema**

`capabilities-v1.json` must contain:

```json
{
  "contractVersion": "1.0.0",
  "packageVersion": "0.2.0",
  "operations": [
    "toolchain.capabilities",
    "asset.validate",
    "asset.prepare",
    "asset.preview",
    "asset.partition",
    "$/cancel"
  ],
  "inputFormats": ["glb", "gltf"],
  "features": {
    "lod": true,
    "partition": true,
    "tileset": true,
    "renderBatching": true,
    "cancellation": true
  },
  "limits": {
    "maxMessageBytes": 8388608,
    "maxConcurrentJobs": 2,
    "maxSourceBytes": 2147483648
  }
}
```

`preview-request-v1.json` must use a workspace-relative source/output path and include
`assetId`, `budget`, and an integer JSON-RPC ID. `preview-result-v1.json` must contain all
asset-result fields from the spec and a `sha256:<64 lowercase hex>` content hash.

The JSON Schema must define the envelope, capabilities response, preview request/result, and
error result; reject unknown properties in request params; and enforce the 8 MiB transport limit
as documented metadata.

- [ ] **Step 4: Write the contract document**

Add `toolchain-sidecar-contract-v1.md` with:

- Document ID, Active status, owner, semantic version, consumers, and compatibility window.
- NDJSON framing and stdout/stderr rules.
- Exact capabilities, request, success, progress, and error shapes.
- Relative-path and atomic-publication rules.
- Contract-major compatibility and capability-gated feature rules.
- Stable error fields `code`, `message`, `retryable`, and optional `data`.
- Links to the four fixtures and schema.

Add `ICE3-DOC-CTR-002` to the contracts section in `docs/README.md`.

- [ ] **Step 5: Wire and run the contract gate**

Update `package.json`:

```json
"test": "node --test scripts/check-docs.test.mjs scripts/toolchain-sidecar-contract.test.mjs"
```

Run:

```bash
npm test
npm run docs:check
```

Expected: all tests pass and the new contract appears in the generated/indexed documentation.

- [ ] **Step 6: Commit in `ice3-doc`**

```bash
git add docs/README.md docs/contracts/toolchain-sidecar-contract-v1.md \
  docs/contracts/toolchain-sidecar-v1.schema.json \
  scripts/fixtures/toolchain-sidecar scripts/toolchain-sidecar-contract.test.mjs package.json
git commit -m "docs: define toolchain sidecar contract"
```

---

### Task 2: Implement Toolchain Sidecar Protocol and Capabilities

**Files:**

- Create: `ice3-toolchain/packages/cli/src/sidecar/protocol.ts`
- Create: `ice3-toolchain/packages/cli/src/sidecar/server.ts`
- Create: `ice3-toolchain/tests/sidecar/protocol.test.ts`
- Create: `ice3-toolchain/tests/sidecar/server.test.ts`
- Modify: `ice3-toolchain/packages/cli/src/main.ts`
- Modify: `ice3-toolchain/packages/cli/package.json`
- Modify: `ice3-toolchain/packages/cli/README.md`

**Interfaces:**

- Consumes: contract version `1.0.0`.
- Produces:
  `serveSidecar({ input, output, errorOutput, workspaceRoot, packageVersion }): Promise<void>`.
- Produces: `createSidecarHandler(options): SidecarHandler`.
- Produces: `SidecarHandler.handle(message: unknown): Promise<SidecarResponse | undefined>`.
- Produces CLI command: `ice3-tool sidecar --workspace <absolute-path>`.
- Produces: `toolchain.capabilities` response matching Task 1.

Create a dedicated Toolchain branch from the desired base while preserving its existing dirty
worktree; stage only files named in this task.

- [ ] **Step 1: Write failing parser and handshake tests**

`tests/sidecar/protocol.test.ts`:

```ts
import assert from 'node:assert/strict';
import { test } from '../node-test.js';
import { decodeLine, encodeMessage, SidecarProtocolError } from '../../packages/cli/src/sidecar/protocol.js';

test('decodes one complete JSON-RPC request', () => {
  assert.deepEqual(decodeLine('{"jsonrpc":"2.0","id":1,"method":"toolchain.capabilities","params":{}}'), {
    jsonrpc: '2.0',
    id: 1,
    method: 'toolchain.capabilities',
    params: {},
  });
});

test('rejects oversized protocol lines', () => {
  assert.throws(
    () => decodeLine('x'.repeat(8 * 1024 * 1024 + 1)),
    (error: unknown) => error instanceof SidecarProtocolError
      && error.code === 'ICE3_TOOL_SIDECAR_MESSAGE_TOO_LARGE',
  );
});

test('encodes exactly one newline-terminated message', () => {
  const line = encodeMessage({ jsonrpc: '2.0', id: 1, result: { ok: true } });
  assert.equal(line.endsWith('\n'), true);
  assert.equal(line.slice(0, -1).includes('\n'), false);
});
```

`tests/sidecar/server.test.ts`:

```ts
import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from '../node-test.js';
import { createSidecarHandler } from '../../packages/cli/src/sidecar/server.js';

test('reports exact capabilities', async () => {
  const workspace = await mkdtemp(join(tmpdir(), 'ice3-sidecar-'));
  try {
    const handler = createSidecarHandler({
      workspaceRoot: workspace,
      packageVersion: '0.2.0',
    });
    const response = await handler.handle({
      jsonrpc: '2.0',
      id: 'capabilities',
      method: 'toolchain.capabilities',
      params: {},
    });
    assert.equal(response?.result.contractVersion, '1.0.0');
    assert.equal(response?.result.packageVersion, '0.2.0');
  } finally {
    await rm(workspace, { recursive: true, force: true });
  }
});
```

- [ ] **Step 2: Verify RED**

Run in `ice3-toolchain`:

```bash
node --import tsx --test tests/sidecar/protocol.test.ts tests/sidecar/server.test.ts
```

Expected: module-not-found failures for `sidecar/protocol` and `sidecar/server`.

- [ ] **Step 3: Implement framing and handshake**

`protocol.ts` must:

- Export `MAX_MESSAGE_BYTES = 8 * 1024 * 1024`.
- Reject empty lines, invalid JSON, non-object messages, missing `jsonrpc: "2.0"`, missing
  string methods, and lines over the size cap.
- Serialize without raw newlines using `JSON.stringify`.
- Map protocol failures to `SidecarProtocolError` with stable `ICE3_TOOL_SIDECAR_*` codes.

`server.ts` must start a readline interface over stdin, call the handler concurrently, and write
exactly one NDJSON response line per request. It must never use stdout for logging.

- [ ] **Step 4: Add the sidecar command**

In `main.ts`, check `process.argv[2] === 'sidecar'` before `runCli`. Parse exactly one
`--workspace <absolute-path>`, canonicalize it, and start `serveSidecar`. Reject missing,
relative, non-directory, and multiple workspace arguments with exit code 1 and stderr output.

Add `./sidecar` to `packages/cli/package.json` exports pointing at the built
`dist/sidecar/server.js` and `dist/sidecar/server.d.ts`.

- [ ] **Step 5: Verify GREEN**

Run:

```bash
node --import tsx --test tests/sidecar/protocol.test.ts tests/sidecar/server.test.ts
npm run typecheck
```

Expected: PASS and no TypeScript errors.

- [ ] **Step 6: Commit in `ice3-toolchain`**

```bash
git add packages/cli/src/main.ts packages/cli/src/sidecar packages/cli/package.json \
  packages/cli/README.md tests/sidecar/protocol.test.ts tests/sidecar/server.test.ts
git commit -m "feat(toolchain): add sidecar protocol"
```

---

### Task 3: Implement Sidecar Asset Operations and Safety

**Files:**

- Create: `ice3-toolchain/packages/cli/src/sidecar/workspacePaths.ts`
- Create: `ice3-toolchain/packages/cli/src/sidecar/operations.ts`
- Create: `ice3-toolchain/tests/sidecar/operations.test.ts`
- Modify: `ice3-toolchain/packages/cli/src/sidecar/server.ts`

**Interfaces:**

- Consumes: `readSource`, `buildPreparedGLB`, `validatePreparedGLB`, `partitionAsset`, and the
  existing source-import registry.
- Produces: `resolveWorkspacePath(root, relativePath, mode): Promise<string>`.
- Produces:
  `executeSidecarOperation(operation, params, context): Promise<SidecarMethodResult>`.
- Produces: `SidecarMethodResult = ToolchainAssetResult | ToolchainValidationResult |
  ToolchainPartitionResult`.
- Produces handshake flags `lod`, `partition`, `tileset`, `renderBatching`, and `cancellation`.

- [ ] **Step 1: Write failing operation and path-safety tests**

Use `tests/fixtures/minimal.glb` for successful validate/prepare/preview. Tests must:

```ts
test('rejects parent traversal before reading a source', async () => {
  await assert.rejects(
    resolveWorkspacePath(workspace, '../outside.glb', 'read'),
    (error: unknown) => error instanceof SidecarProtocolError
      && error.code === 'ICE3_TOOL_SIDECAR_PATH_INVALID',
  );
});

test('publishes a prepared preview atomically', async () => {
  const result = await executeSidecarOperation('asset.preview', {
    sourcePath: 'fixtures/minimal.glb',
    outputPath: 'cache/toolchain/result/asset.glb',
    assetId: 'sidecar-preview',
    budget: 'unrestricted',
  }, context);
  assert.equal(result.primaryPath, 'cache/toolchain/result/asset.glb');
  assert.equal(result.contentHash.startsWith('sha256:'), true);
  assert.equal((await readFile(join(workspace, result.primaryPath))).subarray(0, 4).toString(), 'glTF');
});
```

Add a symlink escape test, validate test, and partition test using
`tests/fixtures/writePartitionGLTF.mjs`. The partition test asserts every returned path begins
with `cache/toolchain/` and no `.tmp` file remains.

Add an idempotency test that sends the same preview request twice. The second result must have the
same `contentHash`, `primaryPath`, and byte length. A corrupt cached GLB must be removed and
regenerated once.

- [ ] **Step 2: Verify RED**

```bash
node --import tsx --test tests/sidecar/operations.test.ts
```

Expected: missing `resolveWorkspacePath` and `executeSidecarOperation`.

- [ ] **Step 3: Implement path confinement**

`resolveWorkspacePath` must:

- reject absolute paths;
- normalize and canonicalize the workspace root;
- reject any input containing a resolved path outside the root;
- canonicalize existing ancestors for write targets;
- reject symlink components that escape the root;
- return a path inside the requested project root.

- [ ] **Step 4: Implement asset operations**

`asset.validate` reads a workspace file and returns the `validatePreparedGLB` report.

`asset.prepare` and `asset.preview`:

1. resolve source and output paths;
2. read the source GLB/glTF;
3. call `buildPreparedGLB`;
4. write to `<output>.tmp`;
5. validate the temporary bytes;
6. atomically rename to the final path;
7. return content hash, byte length, diagnostics, and warnings.

`asset.partition` accepts `assetId`, `maxTriangles`, `maxBytes`, and optional LOD options;
it uses the existing partition pipeline and publishes files only after all outputs validate.

Cancellation must remove temp files. The server tracks active request IDs and returns
`ICE3_TOOL_SIDECAR_CANCELLED` for a canceled operation.

Cache keys include source SHA-256, normalized options, contract version, package version, and
profile/budget version. Cache hits return the stored validated result without rewriting the final
file. The dispatcher queues background partition work behind interactive preview when active jobs
reach `maxConcurrentJobs`.

- [ ] **Step 5: Run integration tests**

Spawn the real compiled sidecar command in `server.test.ts`, send a capability request and one
preview request through stdin, parse stdout as NDJSON, and assert no other stdout lines exist.

Run:

```bash
npm run build
npm run test -- tests/sidecar
```

Expected: parser, handshake, operations, path safety, cancellation cleanup, and real process tests
all pass.

- [ ] **Step 6: Commit in `ice3-toolchain`**

```bash
git add packages/cli/src/sidecar tests/sidecar
git commit -m "feat(toolchain): execute sidecar asset jobs"
```

---

### Task 4: Add the Rust Toolchain Client

**Files:**

- Create: `ice3-studio/crates/studio-toolchain-client/Cargo.toml`
- Create: `ice3-studio/crates/studio-toolchain-client/src/lib.rs`
- Create: `ice3-studio/crates/studio-toolchain-client/src/protocol.rs`
- Create: `ice3-studio/crates/studio-toolchain-client/src/workspace_path.rs`
- Create: `ice3-studio/crates/studio-toolchain-client/src/client.rs`
- Create: `ice3-studio/crates/studio-toolchain-client/tests/fixtures/fake-sidecar.mjs`
- Create: `ice3-studio/crates/studio-toolchain-client/tests/client_contract.rs`
- Modify: `ice3-studio/Cargo.toml`
- Modify: `ice3-studio/Cargo.lock`

**Interfaces:**

- Consumes: sidecar NDJSON protocol from Tasks 1–3.
- Produces: `ToolchainClient::spawn(ToolchainLaunchConfig) -> Result<Self, ToolchainClientError>`.
- Produces: `capabilities(&self) -> Result<ToolchainCapabilities, ToolchainClientError>`.
- Produces: `asset_validate -> ToolchainValidationResult`,
  `asset_prepare/asset_preview -> ToolchainAssetResult`, and
  `asset_partition -> ToolchainPartitionResult`.
- Produces:
  `request<T: DeserializeOwned>(&self, method: &str, params: Value, timeout: Duration) -> Result<T,
  ToolchainClientError>`.
- Produces: `cancel(&self, request_id: u64)`.
- Produces: `ToolchainClientError::{Unavailable, Incompatible, Timeout, Protocol, Rejected,
  Crashed}`.

- [ ] **Step 1: Add a failing fake-sidecar contract test**

Create a Node fixture that reads stdin with `readline`, responds to `toolchain.capabilities`, and
can be configured by environment to exit or emit invalid JSON. In Rust:

```rust
#[test]
fn negotiates_contract_and_reports_versions() {
    let client = test_client("normal");
    let capabilities = client.capabilities().unwrap();
    assert_eq!(capabilities.contract_version, "1.0.0");
    assert_eq!(capabilities.package_version, "0.2.0");
    assert!(capabilities.operations.contains(&"asset.preview".to_string()));
}

#[test]
fn rejects_an_incompatible_major_contract() {
    let client = test_client("incompatible");
    assert!(matches!(
        client.capabilities(),
        Err(ToolchainClientError::Incompatible { .. })
    ));
}
```

- [ ] **Step 2: Verify RED**

Run:

```bash
cargo test -p studio-toolchain-client --test client_contract
```

Expected: package and client type do not exist.

- [ ] **Step 3: Implement typed protocol and path validation**

Serde types include:

```rust
pub struct ToolchainCapabilities {
    pub contract_version: String,
    pub package_version: String,
    pub operations: Vec<String>,
    pub input_formats: Vec<String>,
    pub features: ToolchainFeatures,
    pub limits: ToolchainLimits,
}

pub struct ToolchainAssetResult {
    pub asset_id: String,
    pub content_hash: String,
    pub profile_version: String,
    pub manifest_path: String,
    pub primary_path: String,
    pub byte_length: u64,
    pub diagnostics: Vec<ToolchainDiagnostic>,
    pub warnings: Vec<String>,
}

pub struct ToolchainValidationResult {
    pub status: String,
    pub diagnostics: Vec<ToolchainDiagnostic>,
    pub warnings: Vec<String>,
}

pub struct ToolchainPartitionResult {
    pub asset_id: String,
    pub content_hash: String,
    pub tileset_path: String,
    pub files: Vec<ToolchainOutputFile>,
    pub diagnostics: Vec<ToolchainDiagnostic>,
    pub warnings: Vec<String>,
}

pub struct ToolchainOutputFile {
    pub path: String,
    pub content_hash: String,
    pub byte_length: u64,
}

pub enum ToolchainState {
    Starting,
    Ready,
    Busy,
    Unavailable,
    Incompatible,
    Failed,
}
```

Use camelCase serde names. Reject a response larger than 8 MiB and reject asset paths that are
absolute or contain parent traversal.

- [ ] **Step 4: Implement lifecycle and request multiplexing**

`ToolchainClient` owns:

- the child process;
- a mutex-protected stdin writer;
- a background stdout reader;
- a pending-request map keyed by monotonically increasing `u64` IDs;
- stderr draining with a bounded ring buffer;
- timeout and shutdown flags.

Each `request` writes one newline, waits for its matching response or timeout, and returns a typed
result. A reader thread dispatches responses to request channels so two blocking Server workers
can share one sidecar. On drop, close stdin, wait briefly, then kill the child.

- [ ] **Step 5: Add failure tests**

Test malformed JSON, stdout log contamination, missing response, timeout, child exit before
response, and oversized line. Each must map to a distinct stable error variant and include no raw
environment values.

Run:

```bash
cargo test -p studio-toolchain-client
```

Expected: all fake-sidecar tests pass.

- [ ] **Step 6: Commit in `ice3-studio`**

```bash
git add Cargo.toml Cargo.lock crates/studio-toolchain-client
git commit -m "feat(studio-toolchain): add sidecar client"
```

---

### Task 5: Migrate Server Preview and Publish Toolchain Status

**Files:**

- Modify: `ice3-studio/crates/studio-server/Cargo.toml`
- Modify: `ice3-studio/crates/studio-server/src/lib.rs`
- Modify: `ice3-studio/crates/studio-server/src/rpc.rs`
- Modify: `ice3-studio/crates/studio-server/src/preview.rs`
- Modify: `ice3-studio/crates/studio-server/src/changeset.rs`
- Create: `ice3-studio/crates/studio-server/tests/toolchain_sidecar.rs`
- Modify: `ice3-studio/crates/studio-server/tests/common/mod.rs`
- Modify: `ice3-studio/crates/studio-server/tests/verification_contract.rs`
- Modify: `ice3-studio/crates/studio-core/src/validation.rs`
- Modify: `ice3-studio/docs/reference/diagnostics.md`

**Interfaces:**

- Consumes: `ToolchainClient`.
- Produces: `ToolchainStatus { state, contract_version, package_version, capabilities, reason }`.
- Produces: `Capabilities.toolchain`.
- Produces: Runtime preview through `asset.preview`.
- Produces diagnostics:
  `ICE3_STUDIO_TOOLCHAIN_UNAVAILABLE`, `ICE3_STUDIO_TOOLCHAIN_INCOMPATIBLE`,
  `ICE3_STUDIO_TOOLCHAIN_TIMEOUT`, `ICE3_STUDIO_TOOLCHAIN_PROTOCOL_INVALID`,
  `ICE3_STUDIO_TOOLCHAIN_JOB_FAILED`.

```rust
pub struct ToolchainStatus {
    pub state: ToolchainState,
    pub contract_version: Option<String>,
    pub package_version: Option<String>,
    pub capabilities: Vec<String>,
    pub reason: Option<String>,
}
```

- [ ] **Step 1: Write failing server integration tests**

Use a fake sidecar script injected through test-only `ServerConfig` construction. Test:

```rust
#[tokio::test]
async fn preview_uses_the_sidecar_and_preserves_revision() {
    let server = start_server_with_fake_sidecar("preview-ok").await;
    let before = current_revision(&server).await;
    let preview = rpc(&server, "export.runtimePreview", json!({})).await;
    assert_eq!(preview["result"]["assetId"], "prepared-fixture");
    assert_eq!(current_revision(&server).await, before);
}

#[tokio::test]
async fn incompatible_sidecar_disables_preview_but_not_document_reads() {
    let server = start_server_with_fake_sidecar("contract-v2").await;
    assert_eq!(rpc(&server, "document.get", json!({})).await["error"], json!(null));
    assert_eq!(
        rpc(&server, "export.runtimePreview", json!({})).await["error"]["diagnosticCode"],
        "ICE3_STUDIO_TOOLCHAIN_INCOMPATIBLE"
    );
}
```

- [ ] **Step 2: Verify RED**

```bash
cargo test -p studio-server --test toolchain_sidecar
```

Expected: no `toolchain` capability and no sidecar-backed preview path.

- [ ] **Step 3: Replace one-shot preview execution**

`PreviewAdapter` stores `Arc<ToolchainClient>` and calls `asset_preview` with relative source and
output paths. Candidate preview uses the same client and cache key behavior. Delete direct
`Command::new(node)` construction from `preview.rs`.

Do not hold the Server session mutex while waiting for the sidecar. Clone the required document,
project path, and adapter handle first.

- [ ] **Step 4: Add capability and degradation state**

Add `toolchain` to `system.capabilities`. On startup, Studio starts the managed sidecar and
performs the handshake. If unavailable, it records the reason and still starts the Server.

Map client errors to the diagnostics above. A Toolchain failure must not modify `session.document`
or persist a revision.

Add lifecycle tests for startup handshake failure, graceful shutdown, one restart after a child
crash, and idle shutdown followed by a successful lazy restart. Add a concurrency test proving
health and `document.get` remain responsive while a fake sidecar delays its preview response.

- [ ] **Step 5: Run focused and full gates**

```bash
cargo test -p studio-server --test toolchain_sidecar
cargo test --workspace
cargo run -q -p studio-cli -- schema --output schemas --check
```

Expected: all pass after schema regeneration.

- [ ] **Step 6: Commit in `ice3-studio`**

```bash
git add crates/studio-server crates/studio-core/src/validation.rs docs/reference/diagnostics.md \
  schemas crates/studio-server/bindings crates/studio-core/bindings
git commit -m "feat(studio-server): integrate toolchain sidecar"
```

---

### Task 6: Add Managed Sidecar Discovery and Release Verification

**Files:**

- Create: `ice3-studio/toolchain/manifest.template.json`
- Create: `ice3-studio/scripts/stage-toolchain.mjs`
- Create: `ice3-studio/scripts/toolchain-launch.mjs`
- Create: `ice3-studio/scripts/verify-toolchain-bundle.mjs`
- Create: `ice3-studio/scripts/verify-toolchain-sidecar.mjs`
- Create: `ice3-studio/scripts/tests/toolchain-bundle.test.mjs`
- Modify: `ice3-studio/package.json`
- Modify: `ice3-studio/scripts/verify.mjs`
- Modify: `ice3-studio/crates/studio-toolchain-client/src/client.rs`

**Interfaces:**

- Consumes: built `@ice3/toolchain` package and compatible Node executable.
- Produces: `dist/toolchain/manifest.json`.
- Produces: manifest fields `contractVersion`, `packageVersion`, `nodeVersion`,
  `entrypoint`, `sha256`, `platform`, `architecture`.
- Produces: `ToolchainLaunchConfig::from_manifest(application_root)`.

- [ ] **Step 1: Write failing bundle-manifest tests**

The test creates a temporary fake Toolchain package and fake runtime, runs:

```js
node scripts/stage-toolchain.mjs \
  --package /tmp/fake-toolchain \
  --node /tmp/fake-node \
  --output /tmp/bundle
```

Then asserts:

```js
assert.equal(manifest.contractVersion, '1.0.0');
assert.equal(manifest.packageVersion, '0.2.0');
assert.match(manifest.sha256, /^[a-f0-9]{64}$/);
assert.equal(manifest.entrypoint, 'runtime/node');
assert.equal(manifest.packageEntry, 'toolchain/dist/main.js');
```

A corrupted file must make `verify-toolchain-bundle.mjs` exit non-zero.

Add manifest tests for mismatched platform, mismatched architecture, an entrypoint outside the
bundle, and duplicate required paths.

- [ ] **Step 2: Verify RED**

```bash
node --test scripts/tests/toolchain-bundle.test.mjs
```

Expected: scripts do not exist.

- [ ] **Step 3: Implement staging and verification**

`stage-toolchain.mjs`:

- copies only the release package and required runtime files;
- writes a temporary directory;
- computes SHA-256 for every required file;
- writes `manifest.json`;
- atomically replaces the final bundle only after verification.

`verify-toolchain-bundle.mjs` rejects missing files, changed hashes, unsupported contract majors,
and entrypoints that resolve outside the bundle. Runtime discovery prefers this manifest; the
environment override remains test/development-only.

- [ ] **Step 4: Run a live sidecar smoke test**

`verify-toolchain-sidecar.mjs` starts the staged sidecar with a temporary workspace, asserts
contract/package versions, sends one validate request for a synthetic fixture, awaits cancellation
of a deliberately slow background job, and confirms no temporary file remains.

Run:

```bash
node scripts/stage-toolchain.mjs --package ../ice3-toolchain/release/toolchain \
  --node "$(command -v node)" --output output/toolchain-bundle
node scripts/verify-toolchain-bundle.mjs --bundle output/toolchain-bundle
node scripts/verify-toolchain-sidecar.mjs --bundle output/toolchain-bundle
```

Expected: all commands exit 0 and stdout contains only protocol output from the sidecar itself.

- [ ] **Step 5: Commit in `ice3-studio`**

```bash
git add toolchain scripts/stage-toolchain.mjs scripts/toolchain-launch.mjs \
  scripts/verify-toolchain-bundle.mjs scripts/verify-toolchain-sidecar.mjs \
  scripts/tests/toolchain-bundle.test.mjs package.json scripts/verify.mjs \
  crates/studio-toolchain-client/src/client.rs
git commit -m "build(studio): bundle toolchain sidecar"
```

---

### Task 7: Add Workbench Status, Cross-Repository Conformance, and Release Docs

**Files:**

- Create: `ice3-studio/apps/viewer/src/toolchain/ToolchainPanel.tsx`
- Create: `ice3-studio/apps/viewer/tests/toolchain-status.test.ts`
- Create: `ice3-studio/docs/reference/toolchain-sidecar.md`
- Create: `ice3-toolchain/tests/conformance/studio-sidecar.test.ts`
- Modify: `ice3-studio/apps/viewer/src/api/StudioClient.ts`
- Modify: `ice3-studio/apps/viewer/src/session/SessionStore.ts`
- Modify: `ice3-studio/apps/viewer/src/app/Workbench.tsx`
- Modify: `ice3-studio/apps/viewer/src/app/studioTokens.css`
- Modify: `ice3-studio/apps/viewer/e2e/viewer.spec.ts`
- Modify: `ice3-studio/docs/README.md`
- Modify: `ice3-studio/docs/architecture.md`
- Modify: `ice3-studio/docs/reference/api.md`
- Modify: `ice3-studio/README.md`
- Modify: `ice3-studio/AGENTS.md`
- Modify: `ice3-studio/docs/specs/2026-10-30-studio-toolchain-sidecar-design.md`
- Modify: `ice3-studio/docs/plans/2026-10-30-studio-toolchain-sidecar.md`

**Interfaces:**

- Consumes: `Capabilities.toolchain`.
- Produces: read-only Toolchain state/versions in Workbench.
- Produces:
  `formatToolchainStatus(status: ToolchainStatus): { label: string; version: string; reason:
  string | null }`.
- Produces: cross-repository conformance test using the real built sidecar.

- [ ] **Step 1: Write failing UI and conformance tests**

```ts
test('formats Toolchain state separately from rendering and study compute', () => {
  const view = formatToolchainStatus({
    state: 'ready',
    contractVersion: '1.0.0',
    packageVersion: '0.2.0',
    capabilities: ['asset.preview', 'asset.partition'],
    reason: null,
  });
  assert.equal(view.label, 'Toolchain ready');
  assert.equal(view.version, 'contract 1.0.0 · package 0.2.0');
});
```

Playwright intercepts `system.capabilities` with an incompatible state and asserts
`Toolchain unavailable` while the document tree and raster canvas remain enabled.

The Toolchain conformance test starts the built sidecar, performs the handshake, runs a frozen
preview request, and asserts the result validates against the Task 1 schema.

- [ ] **Step 2: Verify RED**

```bash
npm --prefix apps/viewer test
cargo test --workspace
```

In `ice3-toolchain`:

```bash
node --import tsx --test tests/conformance/studio-sidecar.test.ts
```

Expected: UI formatter and cross-repository test do not exist.

- [ ] **Step 3: Implement Workbench status**

Add a compact read-only panel with states `starting`, `ready`, `busy`, `unavailable`,
`incompatible`, and `failed`. Display contract/package versions, enabled capabilities, and a
sanitized reason. Do not add a path picker, executable selector, or command field.

Keep the canvas and review controls mounted while Toolchain status changes.

- [ ] **Step 4: Complete cross-repository checks and docs**

Add a conformance script to the Toolchain test suite that imports the frozen fixtures from the
shared contract and compares normalized result fields. Document:

- how the managed sidecar is discovered;
- contract/package/runtime compatibility;
- cache and temporary paths;
- cancellation, crash, timeout, and degraded behavior;
- how to reproduce both repository gates.

Run from `ice3-toolchain`:

```bash
npm run verify
```

Run from `ice3-studio`:

```bash
npm run verify
node scripts/verify-toolchain-bundle.mjs --bundle output/toolchain-bundle
node scripts/verify-toolchain-sidecar.mjs --bundle output/toolchain-bundle
```

Expected: every command exits 0; all cross-repository sidecar and existing Studio tests pass.

- [ ] **Step 5: Mark implemented and commit**

Only after both repository gates and live sidecar checks pass, change the spec and plan status to
`Implemented`.

Commit in `ice3-toolchain`:

```bash
git add tests/conformance/studio-sidecar.test.ts packages/cli/src/sidecar \
  packages/cli/src/main.ts packages/cli/package.json packages/cli/README.md tests/sidecar
git commit -m "feat(toolchain): ship studio sidecar integration"
```

Commit in `ice3-studio`:

```bash
git add apps/viewer crates/studio-server crates/studio-toolchain-client toolchain scripts \
  Cargo.toml Cargo.lock package.json docs README.md AGENTS.md schemas crates/*/bindings
git commit -m "feat(studio): integrate managed toolchain sidecar"
```

---

## Plan Self-Review

- Spec coverage: shared contract, Toolchain framing/security/jobs, managed packaging, Rust
  lifecycle, Server preview, capabilities, Workbench evidence, tests, and cross-repository docs
  each have an owning task.
- Type consistency: contract methods and asset fields are defined in Task 1; Tasks 2–4 consume the
  same names; Task 5 maps the same client errors to public diagnostics.
- Security coverage: traversal, symlink escape, command injection, stdout contamination,
  oversized messages, cancellation cleanup, and credential isolation have explicit tests.
- Isolation coverage: `studio-core` remains outside every new dependency path.
- Release coverage: the plan does not mark either specification or plan implemented until both
  project gates and the live sidecar verification pass.
