# Diagnostics Reference

> Document ID: ICE3-STUDIO-REF-DIAG-001
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

Studio diagnostics use stable machine-readable codes. A code identifies the failure category;
the message provides human-readable context. Diagnostics never include credentials, stack
traces, or paths outside the configured workspace.

JSON-RPC errors expose the code as `error.diagnosticCode`. Document validation results expose the
same value as `code`.

Agent-facing failures may also include `error.data` with `category`, `retryable`, operation/object
location, expected/actual values, and short recovery suggestions. These fields are additive; the
stable diagnostic code remains the primary machine contract.

## Diagnostic Fields

| Field | Meaning |
|---|---|
| `code` | Stable `ICE3_STUDIO_*` failure code |
| `message` | Human-readable explanation |
| `operationIndex` | Zero-based position in an atomic transaction, when applicable |
| `objectId` | Stable object or project identifier, when applicable |
| `pointer` | RFC 6901 JSON Pointer to the invalid document field |

## Code Inventory

| Code | Meaning |
|---|---|
| `ICE3_STUDIO_API_INVALID` | JSON-RPC envelope or method parameters are invalid |
| `ICE3_STUDIO_API_VERSION_UNSUPPORTED` | The requested API version is incompatible |
| `ICE3_STUDIO_AUTH_REQUIRED` | A write request lacks a valid local session token |
| `ICE3_STUDIO_PROJECT_NOT_FOUND` | The project does not exist or cannot be accessed |
| `ICE3_STUDIO_PROJECT_LOCKED` | Another writer holds the project lock |
| `ICE3_STUDIO_REVISION_CONFLICT` | `expectedRevision` is stale |
| `ICE3_STUDIO_OPERATION_INVALID` | An operation type or operation input is invalid |
| `ICE3_STUDIO_REFERENCE_INVALID` | A node, material, relation, or constraint reference is invalid |
| `ICE3_STUDIO_GEOMETRY_INVALID` | A primitive, mesh, index, or transform is invalid |
| `ICE3_STUDIO_CONSTRAINT_FAILED` | A relationship or spatial constraint is violated |
| `ICE3_STUDIO_BUDGET_EXCEEDED` | A configured project budget is exceeded |
| `ICE3_STUDIO_EXPORT_FAILED` | Source GLB export failed |
| `ICE3_STUDIO_RUNTIME_PREVIEW_FAILED` | Toolchain preview processing failed |
| `ICE3_STUDIO_PERSISTENCE_FAILED` | Atomic project persistence failed |
| `ICE3_STUDIO_TOOLCHAIN_UNAVAILABLE` | The managed Toolchain sidecar is not configured or cannot start |
| `ICE3_STUDIO_TOOLCHAIN_INCOMPATIBLE` | The sidecar contract major does not match Studio |
| `ICE3_STUDIO_TOOLCHAIN_TIMEOUT` | A Toolchain operation exceeded its host timeout |
| `ICE3_STUDIO_TOOLCHAIN_PROTOCOL_INVALID` | Sidecar stdout violated the JSON-RPC contract or the process crashed |
| `ICE3_STUDIO_TOOLCHAIN_JOB_FAILED` | Toolchain rejected or failed an asset job |
| `ICE3_STUDIO_COMPONENT_INVALID` | A component definition, part graph, binding, or reference is invalid |
| `ICE3_STUDIO_PARAMETER_INVALID` | A component parameter or override is missing, mistyped, or out of range |
| `ICE3_STUDIO_INSTANCE_INVALID` | A component instance or part override is invalid |
| `ICE3_STUDIO_INSTANCE_SET_INVALID` | An instance layout, count, placement, or item override is invalid |
| `ICE3_STUDIO_CONNECTION_INVALID` | A connection endpoint, port, medium, relation, or route is invalid |
| `ICE3_STUDIO_GEOMETRY_CACHE_FAILED` | Derived geometry cache access or recovery failed |
| `ICE3_STUDIO_GPU_UNAVAILABLE` | Native GPU compute was requested but no compatible ready backend is available |
| `ICE3_STUDIO_GPU_INCOMPATIBLE` | The native GPU adapter or feature set cannot satisfy the compute contract |
| `ICE3_STUDIO_GPU_BUDGET_EXCEEDED` | A derived-geometry job exceeds the configured GPU memory budget |
| `ICE3_STUDIO_GPU_KERNEL_FAILED` | A native GPU compute kernel failed or returned incomplete output |
| `ICE3_STUDIO_GPU_DEVICE_LOST` | The native GPU device was lost during derived-geometry evaluation |
| `ICE3_STUDIO_GPU_PARITY_FAILED` | GPU output failed finite-value, topology, bounds, or CPU parity validation |

## Recovery Rules

- Validation or operation failures leave the current revision unchanged.
- A revision conflict requires re-reading the current revision before resubmitting.
- A stale or mismatched Agent search cursor requires restarting `scene.search` without the cursor.
- Agent context, search, and evaluation limit failures include the invalid JSON Pointer and whether
  retrying with corrected parameters can succeed.
- An idempotency key can be retried with the same semantic request; reusing it for different
  content is rejected.
- A runtime preview failure keeps the previous valid preview.
- A persistence failure keeps the previous snapshot loadable.
- An explicit GPU-only failure returns its stable GPU diagnostic; automatic mode retries eligible
  work on the CPU and keeps the current revision unchanged.
- A lost or failed GPU device is removed from scheduling until a later process restart or explicit
  host reinitialization; cached CPU and GPU geometry remain separately keyed.

ChangeSet version conflicts and stale approved digests use `ICE3_STUDIO_OPERATION_INVALID` with a
specific message. A stale base revision uses `ICE3_STUDIO_REVISION_CONFLICT`.
