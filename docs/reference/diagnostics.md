# Diagnostics Reference

> Document ID: ICE3-STUDIO-REF-004
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

Studio Diagnostic 使用稳定的机器可读 code。code 标识失败类别，message 提供人类可读上下文。
Diagnostic 永远不包含凭据、stack trace 或配置 workspace 之外的路径。

JSON-RPC error 通过 `error.diagnosticCode` 暴露该 code；Document validation 结果使用相同的
`code` 字段。

面向 Agent 的失败还可包含 `error.data`，其中提供 `category`、`retryable`、
operation/object 位置、expected/actual 值和简短恢复建议。这些字段是附加信息，稳定的
Diagnostic code 仍是主要机器契约。

## 1. Diagnostic Fields

| Field | Meaning |
|---|---|
| `code` | Stable `ICE3_STUDIO_*` failure code |
| `message` | Human-readable explanation |
| `operationIndex` | Zero-based position in an atomic transaction, when applicable |
| `objectId` | Stable object or project identifier, when applicable |
| `pointer` | RFC 6901 JSON Pointer to the invalid document field |

## 2. Code Inventory

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
| `ICE3_STUDIO_RUNTIME_PREVIEW_FAILED` | Studio native Runtime preview export or atomic publication failed |
| `ICE3_STUDIO_PERSISTENCE_FAILED` | Atomic project persistence failed |
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
| `ICE3_STUDIO_GEOMETRY_OVERLAP` | Two scene entities overlap in world space (opt-in warning audit; not a transaction failure) |

## 3. 源导入 CLI 代码

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
| `ICE3_STUDIO_IMPORT_STEP_SCHEMA_UNSUPPORTED` | STEP schema 不是 AP242 |
| `ICE3_STUDIO_IMPORT_STEP_UNSUPPORTED_ENTITY` | STEP representation 或产品内容被跳过 |
| `ICE3_STUDIO_IMPORT_IFC_SCHEMA_UNSUPPORTED` | IFC schema 不是 IFC4X3 |
| `ICE3_STUDIO_IMPORT_IFC_UNIT_UNSUPPORTED` | IFC 项目缺少唯一、可解析且有效的长度单位 |
| `ICE3_STUDIO_IMPORT_IFC_DUPLICATE_GLOBAL_ID` | IFC GlobalId 重复，Selection 使用 Express ID 消歧 |
| `ICE3_STUDIO_IMPORT_IFC_UNSUPPORTED_RESOURCE` | IFC camera、light、skeleton、volume 或 texture 被跳过 |
| `ICE3_STUDIO_IMPORT_IFC_METADATA_TRUNCATED` | IFC metadata 超出深度或数组上限而被省略 |
| `ICE3_STUDIO_IMPORT_JT_CONVERTER_UNAVAILABLE` | `ICE3_JT_CONVERTER` 未配置、不是可执行 regular file |
| `ICE3_STUDIO_IMPORT_JT_CONVERT_FAILED` | JT converter 启动失败、非零退出或进程清理失败 |
| `ICE3_STUDIO_IMPORT_JT_EXPORT_INVALID` | Converter 输出不是合法的 regular-file GLB |
| `ICE3_STUDIO_IMPORT_JT_GEOMETRY_INVALID` | JT 层级、transform、bounds 或 metadata 映射无效 |
| `ICE3_STUDIO_IMPORT_JT_TIMEOUT` | JT converter/worker 超过配置期限并被终止 |

## 4. Asset Prepare 与 Partition CLI 代码

这些码由 `ice3-studio import/build/inspect/partition` 返回，或者作为 build warning 进入
`ice3-asset-build-report`。它们不属于 JSON-RPC `DiagnosticCode` enum。

| 代码 | 含义 |
|---|---|
| `ICE3_STUDIO_ASSET_INVALID_OPTIONS` | budget 路径、crease angle、inspect option 或 output/report 组合无效 |
| `ICE3_STUDIO_ASSET_INVALID_BUDGET` | custom budget 无法读取、schema 无效或 limit 非法 |
| `ICE3_STUDIO_ASSET_BUDGET_WARNING` | soft budget 或 warning 在 `--warnings-as-errors` 下被提升为失败 |
| `ICE3_STUDIO_ASSET_BUDGET_EXCEEDED` | 一项 hard budget 越界，未发布 GLB/report |
| `ICE3_STUDIO_ASSET_OUTPUT_EXISTS` | output 或 report 已存在且未指定 `--force` |
| `ICE3_STUDIO_ASSET_INVALID_GEOMETRY` | primitive、index、edge generation 或 Prepared profile 几何无效 |
| `ICE3_STUDIO_ASSET_INVALID_PROFILE` | AssetProfile 的 budget、LOD、proxy 或 render 组合无效 |
| `ICE3_STUDIO_ASSET_TILESET_INVALID` | Tileset、partition GLB、URI、stats 或不支持的 Runtime profile 无效 |
| `ICE3_STUDIO_ASSET_REPORT_WRITE_FAILED` | report 序列化、temporary 写入或发布失败 |
| `ICE3_STUDIO_ASSET_IO` | GLB/report 原子发布、rollback 或文件系统操作失败 |

## 5. 恢复规则

- validation 或 operation 失败保持当前 revision 不变。
- revision 冲突要求先重新读取当前 revision，再重新提交。
- Agent search cursor 过期或不匹配时，必须去掉 cursor 后重启 `scene.search`。
- blocked 或 stale 的 `changeset.impact` 返回 `scope: "unavailable"`，不提供 candidate
  digest 或部分几何预测；修复 validation 或从当前 revision 重新开始。
- Agent context、search 和 evaluation 限额失败会包含无效 JSON Pointer，以及修正参数后是否
  可重试。
- 相同语义请求可以使用同一 idempotency key 重试；不同内容复用该 key 会被拒绝。
- Runtime preview 失败时保留上一份有效 preview。
- 带 authored override 的 reference export 以
  `ICE3_STUDIO_REFERENCE_EXPORT_UNSUPPORTED` 失败关闭；移除/重置 override，或导出 source
  baseline。
- persistence 失败时上一份 snapshot 仍可加载。
- Asset Prepare 的 source import、edge generation、hard budget 或 profile 校验失败时不发布
  GLB/report；无 `--force` 遇到已有目标时两者都保持不变。
- `build --force` 发布失败会恢复上一份 GLB 和 report；若 rollback 本身失败，错误会列出
  两个路径的恢复状态，调用方应先检查目标完整性再重试。
- `ICE3_STUDIO_ASSET_INVALID_BUDGET` 要求修正 JSON schema 或 limit；空 profile、重复
  metric、soft limit 大于 hard limit 或非法整数值都不会进入 Prepare。
- Prepared GLB inspect 不允许 `--crease-angle`、`--skip-edges` 或 `--budget`；这些选项只
  适用于重新准备 source。
- `partition` 的 source import、Prepare、edge generation、GLB validation 或 directory
  publish 任一步失败时不留下最终输出目录；临时 staging 会清理。
- `partition --force` 替换失败时恢复上一份目录；若 rollback 自身失败，应检查错误中的
  primary failure 和 backup 位置。
- `partition --output` 不能是文件系统根目录或当前目录 `.`；这防止 `--force` 替换
  workspace/root。
- `ICE3_STUDIO_ASSET_TILESET_INVALID` 表示 Tileset/part/coarse/profile 契约不完整，
  应修正参数、profile 或输出实现；不要发布部分目录。
- Prepare performance gate 的 peak RSS 超过 512 MiB 时失败；应减少 static batch
  选区、降低 partition 上限或优化中间内存，不能删除该门禁或静默放宽。
- explicit GPU-only 失败返回稳定 GPU Diagnostic；automatic 模式会把可执行工作退回 CPU，
  并保持当前 revision 不变。
- GPU device 丢失或失败后不再进入调度，直到进程重启或 host 显式重新初始化；CPU/GPU
  geometry cache 继续分别 keyed。

ChangeSet version 冲突和过期 approved digest 使用 `ICE3_STUDIO_OPERATION_INVALID` 返回具体
message；base revision 过期使用 `ICE3_STUDIO_REVISION_CONFLICT`。
