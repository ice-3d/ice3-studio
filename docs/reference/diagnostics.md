# Diagnostics Reference

> Document ID: ICE3-STUDIO-REF-004
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
| `ICE3_STUDIO_REFERENCE_EXPORT_UNSUPPORTED` | Authored reference overrides cannot be composed into the unchanged source GLB |
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

## 源导入 CLI 代码

这些码由 `ice3-studio import` 返回或作为 warning 进入命令结果，不属于 JSON-RPC
`DiagnosticCode` enum。

| 代码 | 含义 |
|---|---|
| `ICE3_STUDIO_IMPORT_UNSUPPORTED` | 自动 adapter 无法根据扩展名选择格式 |
| `ICE3_STUDIO_IMPORT_ADAPTER_UNAVAILABLE` | 显式 adapter 不存在或与扩展名不匹配 |
| `ICE3_STUDIO_IMPORT_REGISTRY_INVALID` | adapter ID 或扩展名重复/无效 |
| `ICE3_STUDIO_IMPORT_INVALID_SOURCE` | Source、OBJ face 或 MTL 内容无效 |
| `ICE3_STUDIO_IMPORT_EMPTY_GEOMETRY` | Source 不包含可导入三角形 |
| `ICE3_STUDIO_IMPORT_RESOURCE_IGNORED` | 不安全或缺失资源被忽略 |
| `ICE3_STUDIO_IMPORT_LIMIT_EXCEEDED` | node、triangle 或 decoded-memory 预算越界 |
| `ICE3_STUDIO_IMPORT_TIMEOUT` | native source worker 超过硬 timeout 并被终止 |
| `ICE3_STUDIO_IMPORT_WORKER_PROTOCOL_INVALID` | worker response 版本、长度或 bincode 无效 |
| `ICE3_STUDIO_IMPORT_WORKER_FAILED` | worker 异常退出或崩溃 |
| `ICE3_STUDIO_IMPORT_WORKER_OUTPUT_TOO_LARGE` | response 或 stderr 超过上限 |
| `ICE3_STUDIO_IMPORT_IO` | Source 或输出文件 I/O 失败 |
| `ICE3_STUDIO_IMPORT_OBJ_UNIT_ASSUMED_METERS` | OBJ 没有单位信息，按米解释 |
| `ICE3_STUDIO_IMPORT_OBJ_VERTEX_COLORS_UNSUPPORTED` | OBJ 顶点色未导入 |
| `ICE3_STUDIO_IMPORT_OBJ_TEXTURE_UNSUPPORTED` | MTL 纹理未导入 |
| `ICE3_STUDIO_IMPORT_OBJ_RESOURCE_IGNORED` | MTL 路径越界或不安全 |
| `ICE3_STUDIO_IMPORT_OBJ_MATERIAL_UNAVAILABLE` | 引用的 MTL 或 material 不可用 |
| `ICE3_STUDIO_IMPORT_PRIMITIVE_UNSUPPORTED` | glTF 非三角形 primitive 被跳过 |
| `ICE3_STUDIO_IMPORT_RESOURCE_UNSUPPORTED` | glTF animation、skin、texture 等资源被跳过 |
| `ICE3_STUDIO_IMPORT_FBX_*` | FBX animation、skin、camera、light、texture、NURBS 或 decoder warning |
| `ICE3_STUDIO_IMPORT_USD_UNSUPPORTED_RESOURCE` | USD camera、light、skeleton、volume 等资源被跳过 |

## Recovery Rules

- Validation or operation failures leave the current revision unchanged.
- A revision conflict requires re-reading the current revision before resubmitting.
- A stale or mismatched Agent search cursor requires restarting `scene.search` without the cursor.
- A blocked or stale `changeset.impact` request returns `scope: "unavailable"` with no candidate
  digest and no partial geometry forecast; fix validation or restart from the current revision.
- Agent context, search, and evaluation limit failures include the invalid JSON Pointer and whether
  retrying with corrected parameters can succeed.
- An idempotency key can be retried with the same semantic request; reusing it for different
  content is rejected.
- A runtime preview failure keeps the previous valid preview.
- Reference export with authored overrides fails closed with
  `ICE3_STUDIO_REFERENCE_EXPORT_UNSUPPORTED`; remove/reset the overrides or export the source
  baseline instead.
- A persistence failure keeps the previous snapshot loadable.
- An explicit GPU-only failure returns its stable GPU diagnostic; automatic mode retries eligible
  work on the CPU and keeps the current revision unchanged.
- A lost or failed GPU device is removed from scheduling until a later process restart or explicit
  host reinitialization; cached CPU and GPU geometry remain separately keyed.

ChangeSet version conflicts and stale approved digests use `ICE3_STUDIO_OPERATION_INVALID` with a
specific message. A stale base revision uses `ICE3_STUDIO_REVISION_CONFLICT`.
