# Toolchain Sidecar

> Document ID: ICE3-STUDIO-REF-005
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

## Boundary

ICE3 Studio Server uses a managed local Toolchain sidecar for Runtime preview and prepared-asset
capabilities. `studio-core` has no Toolchain or Node dependency. The sidecar cannot mutate the
Document, approval state, candidate digest, or revision.

The canonical wire contract is
[`ICE3-DOC-CTR-002`](../../../ice3-doc/docs/contracts/toolchain-sidecar-contract-v1.md).

## Discovery

Release bundles place the sidecar under:

```text
toolchain/manifest.json
toolchain/dist/main.js
toolchain/node_modules/
runtime/node
```

`STUDIO_APP_ROOT` selects the installation root for local integration tests. Development may use
`STUDIO_TOOLCHAIN_CLI` to select a script inside the project workspace. The public API cannot
select an executable, adapter, environment variable, or shader.

Every discoverable bundle must pass:

```bash
npm run toolchain:verify -- --bundle output/toolchain-bundle
```

The verifier checks platform, architecture, contract major, path containment, per-file SHA-256,
and aggregate SHA-256.

## Operation

Studio starts one long-lived process, performs `toolchain.capabilities`, and sends newline-delimited
JSON-RPC requests over stdio. Runtime preview uses `asset.preview`; candidate preview uses the same
client and cache path. Interactive preview is queued before background partition work.

Toolchain writes final assets under the project's `cache/toolchain/` directory. Writes use a
temporary file or directory followed by an atomic rename. Corrupt cached GLBs are removed and
regenerated.

## Degradation

- Missing or stopped sidecar: editing still works; preview returns
  `ICE3_STUDIO_TOOLCHAIN_UNAVAILABLE`.
- Incompatible contract: only Toolchain-backed features are disabled.
- Timeout: request returns `ICE3_STUDIO_TOOLCHAIN_TIMEOUT`.
- Malformed stdout or crash: request returns `ICE3_STUDIO_TOOLCHAIN_PROTOCOL_INVALID`.
- Rejected job: request returns `ICE3_STUDIO_TOOLCHAIN_JOB_FAILED`.

Studio retries one crashed sidecar request after restarting the process. A failed preview never
changes the current revision or removes the previous valid preview.

## Verification

```bash
npm run verify
node scripts/verify-toolchain-bundle.mjs --bundle output/toolchain-bundle
node scripts/verify-toolchain-sidecar.mjs --bundle output/toolchain-bundle
```

Repository tests use deterministic fake sidecars for lifecycle and failure cases. The live smoke
test uses the pinned Toolchain package and a synthetic model.
