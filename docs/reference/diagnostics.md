# Diagnostics Reference

> Document ID: ICE3-STUDIO-REF-DIAG-001
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

Studio diagnostics use stable machine-readable codes. A code identifies the failure category;
the message provides human-readable context. Diagnostics never include credentials, stack
traces, or paths outside the configured workspace.

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

## Recovery Rules

- Validation or operation failures leave the current revision unchanged.
- A revision conflict requires re-reading the current revision before resubmitting.
- An idempotency key can be retried with the same semantic request; reusing it for different
  content is rejected.
- A runtime preview failure keeps the previous valid preview.
- A persistence failure keeps the previous snapshot loadable.
