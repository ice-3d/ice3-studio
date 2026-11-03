# 能力清单

> Document ID: ICE3-STUDIO-REF-001
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

本文件是 Studio 对外能力的**代码镜像**。内容逐项来自 `crates/` 与 `apps/viewer/src/`，
实现变更时同步更新；与其它文档冲突时以本文件和代码为准。

`API_VERSION` 当前为 `1.0`，但项目仍是 pre-release。历史 API 曾包含
`system.capabilities.toolchain`，Phase 5B 随 Sidecar 退役删除了该字段；当前 schema
不含 `toolchain`。capabilities 调用方必须按当前 generated schema 解析，不能把版本
字符串视为兼容性承诺。

## 1. 传输与鉴权

- JSON-RPC 2.0 over HTTP `POST /rpc`，另有 WebSocket 推送 revision 与 ChangeSet 事件。
- 只监听 `127.0.0.1`。
- 写方法需要 `Authorization: Bearer <STUDIO_SESSION_TOKEN>`。
- 提交 ChangeSet 需要审批 Cookie `ice3_studio_approval=<STUDIO_APPROVAL_TOKEN>`。
- `review-required` 模式下 Agent 令牌不能直接 `document.apply` 或 `changeset.commit`；
  `autonomous` 模式下允许直接写入。

## 2. JSON-RPC 方法（固化 38，运行时 40）

`schemas/studio-api.schema.json` 固化 38 个方法。Server 的 `system.capabilities` 额外
公布两个实验性 GPU scene 方法 `evaluation.waitScene` 和
`evaluation.visibleInstances`，因此运行时能力清单共 40 个。

### 2.1 system（3）

| 方法 | 写 | 参数 | 返回 |
|---|---:||---|---|
| `system.health` | 否 | 无 | `apiVersion`、`schemaVersion`、`status` |
| `system.capabilities` | 否 | 无 | 方法清单、compute 证据、写模式与限制 |
| `system.agentManifest` | 否 | 无 | Agent 工具、当前令牌权限、调用上限、工作流提示 |

### 2.2 project（5）

| 方法 | 写 | 参数 | 返回 |
|---|---:||---|---|
| `project.create` | 是 | `{ projectId }` | `{ opened: true }` |
| `project.open` | 是 | `{ projectId }` | `{ opened: true }` |
| `project.get` | 否 | 无 | 当前 Document |
| `project.save` | 是 | 无 | 当前已持久化 revision |
| `project.close` | 是 | 无 | 释放写锁并关闭项目 |

### 2.3 document（5）

| 方法 | 写 | 参数 | 返回 |
|---|---:||---|---|
| `document.get` | 否 | 无 | 当前 Document |
| `document.apply` | 是 | `ApplyRequest` | `ApplyResult` |
| `document.undo` | 是 | 无 | 恢复后的文档（作为新 revision） |
| `document.redo` | 是 | 无 | 同上 |
| `document.history` | 否 | 无 | 操作历史 |

### 2.4 scene（5）

| 方法 | 写 | 参数 | 返回 |
|---|---:||---|---|
| `scene.get` | 否 | 无 | 当前场景 |
| `scene.query` | 否 | 过滤条件 | 确定性顺序的匹配节点 |
| `scene.context` | 否 | 无 | 有界项目摘要（计数、bounds、facet、权限、上限） |
| `scene.search` | 否 | `SceneSearchRequest` | revision 绑定的 source/derived 节点页 |
| `scene.bounds` | 否 | 无 | 按 node ID 的世界空间 bounds |

### 2.5 component（1）

| 方法 | 写 | 参数 | 返回 |
|---|---:||---|---|
| `component.get` | 否 | `ComponentGetParams` | 组件定义有界摘要：参数、端口、部件（≤100 条 + truncation 标志）、实例计数 |

### 2.6 validation（1）

| 方法 | 写 | 参数 | 返回 |
|---|---:||---|---|
| `validation.run` | 否 | `ValidationRunRequest`（`includeOverlaps` 默认 false） | 诊断数组；重叠审计为 `ICE3_STUDIO_GEOMETRY_OVERLAP` warning（轴向深度 + 体积占比双阈值，重合上报） |

### 2.7 evaluation（5）

| 方法 | 写 | 参数 | 返回 |
|---|---:||---|---|
| `evaluation.evaluate` | 否 | 无 | 按 node ID 的求值网格 |
| `evaluation.query` | 否 | `EvaluationQueryRequest` | 有界派生几何摘要与可选负载 |
| `evaluation.stats` | 否 | 无 | 有效三角形、唯一网格、缓存命中统计 |
| `evaluation.waitScene` | 否 | `{ revision? }` | 实验性；已发布的 GPU 场景状态 |
| `evaluation.visibleInstances` | 否 | `{ targetRevision, planes[6][4] }` | 实验性；可见实例索引与稳定 node ID |

### 2.8 export（2）

| 方法 | 写 | 参数 | 返回 |
|---|---:||---|---|
| `export.sourceGlb` | 是 | 无 | Source GLB 导出元数据 |
| `export.runtimePreview` | 是 | `RuntimePreviewRequest` | 已提交或候选的预处理预览元数据 |

### 2.9 changeset（10）

| 方法 | 写 | 参数 | 返回 |
|---|---:||---|---|
| `changeset.create` | 是 | `{ baseRevision, title }` | ChangeSet |
| `changeset.addOperations` | 是 | `{ changeSetId, expectedVersion, entries }` | 更新后的 ChangeSet |
| `changeset.removeOperations` | 是 | `{ changeSetId, expectedVersion, ... }` | 更新后的 ChangeSet |
| `changeset.evaluate` | 是 | `{ changeSetId, expectedVersion, includeCandidate }` | 状态、诊断、candidateDigest |
| `changeset.commit` | 是 | `{ changeSetId, expectedVersion, approvedDigest, idempotencyKey }` | 提交结果 |
| `changeset.get` | 否 | `{ changeSetId }` | ChangeSet |
| `changeset.getCandidate` | 否 | `{ changeSetId }` | 候选文档 |
| `changeset.impact` | 否 | `{ changeSetId }` | 影响范围与几何重建分类 |
| `changeset.list` | 否 | 无 | ChangeSet 列表 |
| `changeset.discard` | 是 | `{ changeSetId }` | 丢弃结果 |

### 2.10 referenceAsset（3）

| 方法 | 写 | 参数 | 返回 |
|---|---:||---|---|
| `referenceAsset.register` | 是 | `{ sourcePath, preparedPath, tilesetPath, bindingManifestPath }` | assetId 与有界描述符 |
| `referenceAsset.get` | 否 | `{ assetId }` | 描述符与流式资产 URL |
| `referenceAsset.bindings` | 否 | `{ assetId, cursor?, limit? }` | 稳定设备绑定页（limit 1..=100） |

## 3. Operation 类型（29）

`document.apply` 与 `changeset.addOperations` 接受同一组操作。

| 域 | 操作 |
|---|---|
| node（6） | `node.create`、`node.update`、`node.reparent`、`node.delete`、`primitive.update`、`mesh.replace` |
| component（4） | `component.define`、`component.update`、`component.patch`、`component.delete` |
| instance（4） | `instance.create`、`instance.update`、`instance.patch`、`instance.delete` |
| instanceSet（3） | `instanceSet.create`、`instanceSet.update`、`instanceSet.delete` |
| connection（4） | `connection.create`、`connection.update`、`connection.patch`、`connection.delete` |
| material（3） | `material.upsert`、`material.assign`、`material.patch` |
| relation（3） | `relation.create`、`relation.update`、`relation.delete` |
| constraint（2） | `constraint.upsert`、`constraint.delete` |

`*.patch` 为稀疏字段修改，只提交声明的字段，但仍经过完整校验、diff、幂等和 revision
冲突检查。操作不能提交路径、URL、shell 命令、脚本或任意表达式。

Component parameter 与 part binding 使用 untagged wire format：Number 直接写数字，
Vector3 直接写 `{x,y,z}`，Color 直接写四元数组，Enumeration 直接写字符串。部件字段
字面量直接写值，参数引用写 `{param:"name"}`，派生标量写 `{expr:"radius - wall / 2"}`。

Material 的 `opacity < 1` 导出为 alpha blend；`shading` 可为 `directional`、`flat` 或
`water`，省略时等价于 `directional`。

## 4. ChangeSet 状态（7）

| 状态 | 含义 |
|---|---|
| `draft` | 已创建，操作可增删 |
| `evaluating` | 求值进行中 |
| `ready` | 求值通过，持有可提交的 candidateDigest |
| `blocked` | 求值或预览失败，带诊断 |
| `stale` | 基线 revision 已前进，需重建 |
| `committed` | 已提交为 revision |
| `discarded` | 已丢弃 |

只有 `ready` 状态且提交时 digest 与审批时一致才会落为 revision。

## 5. 诊断码（28）

全部以 `ICE3_STUDIO_` 为前缀，稳定且不复用。

| 域 | 诊断码 |
|---|---|
| API | `ICE3_STUDIO_API_INVALID`、`ICE3_STUDIO_API_VERSION_UNSUPPORTED`、`ICE3_STUDIO_AUTH_REQUIRED` |
| 项目 | `ICE3_STUDIO_PROJECT_NOT_FOUND`、`ICE3_STUDIO_PROJECT_LOCKED`、`ICE3_STUDIO_PERSISTENCE_FAILED`、`ICE3_STUDIO_REVISION_CONFLICT` |
| 操作 | `ICE3_STUDIO_OPERATION_INVALID`、`ICE3_STUDIO_COMPONENT_INVALID`、`ICE3_STUDIO_INSTANCE_INVALID`、`ICE3_STUDIO_INSTANCE_SET_INVALID`、`ICE3_STUDIO_CONNECTION_INVALID`、`ICE3_STUDIO_PARAMETER_INVALID`、`ICE3_STUDIO_CONSTRAINT_FAILED` |
| 几何 | `ICE3_STUDIO_GEOMETRY_INVALID`、`ICE3_STUDIO_GEOMETRY_CACHE_FAILED`、`ICE3_STUDIO_BUDGET_EXCEEDED`、`ICE3_STUDIO_GEOMETRY_OVERLAP` |
| 引用资产 | `ICE3_STUDIO_REFERENCE_INVALID`、`ICE3_STUDIO_REFERENCE_EXPORT_UNSUPPORTED` |
| 导出与预览 | `ICE3_STUDIO_EXPORT_FAILED`、`ICE3_STUDIO_RUNTIME_PREVIEW_FAILED` |
| GPU | `ICE3_STUDIO_GPU_UNAVAILABLE`、`ICE3_STUDIO_GPU_INCOMPATIBLE`、`ICE3_STUDIO_GPU_DEVICE_LOST`、`ICE3_STUDIO_GPU_KERNEL_FAILED`、`ICE3_STUDIO_GPU_PARITY_FAILED`、`ICE3_STUDIO_GPU_BUDGET_EXCEEDED` |

完整说明与恢复规则见 [Diagnostics](diagnostics.md)。

Source Import CLI 另有稳定码：
`ICE3_STUDIO_IMPORT_UNSUPPORTED`、`ICE3_STUDIO_IMPORT_ADAPTER_UNAVAILABLE`、
`ICE3_STUDIO_IMPORT_REGISTRY_INVALID`、`ICE3_STUDIO_IMPORT_INVALID_SOURCE`、
`ICE3_STUDIO_IMPORT_EMPTY_GEOMETRY`、`ICE3_STUDIO_IMPORT_RESOURCE_IGNORED` 和
`ICE3_STUDIO_IMPORT_IO`。OBJ warning 使用 `ICE3_STUDIO_IMPORT_OBJ_*` 命名空间。
JT 另有
`ICE3_STUDIO_IMPORT_JT_CONVERTER_UNAVAILABLE`、
`ICE3_STUDIO_IMPORT_JT_CONVERT_FAILED`、
`ICE3_STUDIO_IMPORT_JT_EXPORT_INVALID`、
`ICE3_STUDIO_IMPORT_JT_GEOMETRY_INVALID` 和
`ICE3_STUDIO_IMPORT_JT_TIMEOUT`。

## 6. CLI

```
ice3-studio schema --output <dir> [--check]
ice3-studio pack --project <dir> --output <file.ice3>
ice3-studio unpack <file.ice3> --into <dir>
ice3-studio import SOURCE --output FILE --asset-id ID
                        [--adapter auto|gltf|obj|fbx|usd|step|ifc|jt]
                        [--warnings-as-errors] [--force]
ice3-studio build SOURCE --output FILE --asset-id ID
                       [--adapter auto|gltf|obj|fbx|usd|step|ifc|jt]
                       [--crease-angle DEGREES] [--skip-edges]
                       [--budget interactive-desktop-v1|unrestricted|FILE]
                       [--report FILE] [--warnings-as-errors] [--force] [--json]
ice3-studio inspect SOURCE [--adapter auto|gltf|obj|fbx|usd|step|ifc|jt]
                          [--crease-angle DEGREES] [--skip-edges]
                          [--budget interactive-desktop-v1|unrestricted|FILE] [--json]
ice3-studio partition SOURCE --output <dir> --asset-id ID
                          [--adapter auto|gltf|obj|fbx|usd|step|ifc|jt]
                          [--max-triangles N] [--max-bytes N] [--crease-angle DEGREES]
                          [--skip-edges] [--budget PROFILE|FILE]
                          [--render-batch off|static] [--lod]
                          [--lod-ratio R] [--lod-allocation proportional|adaptive]
                          [--lod-max-triangles N] [--lod-min-unit-triangles N]
                          [--lod-curve-samples N] [--lod-max-memory-bytes N]
                          [--semantic-proxy off|auto]
                          [--coarse-chunk-max-bytes N]
                          [--warnings-as-errors] [--force] [--json]
ice3-studio validate FILE
ice3-studio export tileset --project <dir> --output <dir>
                         [--profile agent-console-v1] [--force]
ice3-studio serve --workspace <dir> --project <id>
                  [--fixture <dir>] [--port <u16>] [--write-mode <mode>]
                  [--native-gpu <cpu|auto|gpu>]
```

| 参数 | 默认 | 说明 |
|---|---|---|
| `--workspace` | 必填 | 项目与资产的根目录，路径限制在此 |
| `--project` | 必填 | 项目 ID，对应 workspace 下的目录 |
| `--fixture` | 无 | 首次打开时用于播种的 fixture 目录 |
| `--port` | `8120` | 仅绑定 loopback |
| `--write-mode` | `review-required` | 或 `autonomous` |
| `--native-gpu` | `cpu` | 启动期策略，不能被 API 请求覆盖 |

`import` 支持 glTF/GLB、OBJ/MTL、FBX ASCII/Binary、USD ASCII/Binary、STEP AP242、
IFC4X3 和 JT。
adapter 生成
统一 `SourceScene`，canonical writer 生成 `ice3-prepared` GLB。OBJ/MTL、glTF 和 USD
外部资源被限制在源文件同目录；OBJ 假定米制并发出 warning。FBX/USD/STEP/IFC/JT 通过
Studio 自带 worker 解析，不需要外部 DCC。JT 使用 host-owned `ICE3_JT_CONVERTER`，固定
`--input`/`--output` 参数并验证 GLB。worker 默认限制 1,000,000 nodes、20,000,000 triangles、
512 MiB estimated decoded memory 和 120 秒硬 timeout；response 与 stderr 上限分别为
512 MiB 和 1 MiB。内存预算在主要 geometry/metadata 分配前预检，并在 worker response
解码后重新记账；该值不是进程 RSS 的字节级硬上限。FBX/USD/STEP/IFC/JT 是默认启用的 Cargo
features，可分别关闭。超时、崩溃、协议污染和越界资源
失败关闭。USD composition 额外限制深度 32、总 layer 数 1,024，并拒绝绝对路径、URI、
`..` 与 symlink escape。STEP 只接受 AP242，保留 assembly/product 层级、共享实例、
面颜色、Selection 和 metadata，并使用共享 source origin。
IFC 只接受 IFC4X3 和可解析的唯一长度单位，保留空间层级、GlobalId Selection、
Pset/Qto/System、共享实例、材质和 shared source origin。
JT 默认 converter timeout 为 300 秒，外层 worker 另有 5 秒清理宽限；输出上限 512 MiB，
保留层级、共享实例、材质、object/assembly metadata、custom properties、稳定
Selection 和 shared source origin。Converter 必须先经过 supervisor 握手，Studio 登记
process group 后才开始执行；批准 converter 不得通过新 session、daemon 或等价方式主动
逃逸进程组。违反该 host trust contract 的 converter 不受支持。

`--warnings-as-errors` 在发布前拒绝 warning；默认拒绝已有 output，`--force` 才原子
替换。`validate` 接受 source glTF/GLB 与 Prepared GLB。

### 6.1 Asset Prepare

`import` 使用共享 `PrepareArtifact` 生成 `ice3-prepared` GLB、真实 metrics、
preparation metadata 和 budget，但默认 `edges=false`。已有 source import 入口行为不变，
现有 preview 用户不会因为 edges 自动增加 line primitive。

`build` 默认 `edges=true`、crease angle `30°`。edge generation 使用
`round(position * 100000)` welding，并输出 boundary + crease edges。每个 edge primitive
固定为 `mode=1`、`role="edges"`、`widthPx=3`，引用包含 `#e6f2ff` base color 的 edge
material；material `edgeColor` 可覆盖默认色，`edges=false` 的 material 不生成对应边。
`--skip-edges` 关闭 line primitive 和 edge segments，但保留已使用的 source materials 与
对应 edge material slots；reserved slots 顺序为 source、edge materials、`default`、
`default edges`、`vertex colors`。

`build --budget` 接受 `interactive-desktop-v1`、`unrestricted` 或受控 JSON profile。
Soft limit 产生 warning，hard limit 失败关闭；`--warnings-as-errors` 在发布前把 warning
转为失败。`--report` 写出 `ice3-asset-build-report` schema `1.0`，包含 source/content
digest、metrics、preparation、budget 和 warnings。

GLB 与 report 使用 sibling temporary files 成对发布。无 `--force` 时任一目标已存在即失败；
带 `--force` 时先保留 backup，任一 rename 失败会恢复上一份 GLB/report；成功后才清理
backup。`inspect` 不发布文件，对 source 执行同一 Prepare，对 Prepared GLB 直接读取
embedded profile 的 metrics、preparation 和 budget。

`partition` 接受任意已注册 Studio source adapter，并把同一 PrepareArtifact 交给
`studio_pipeline::build_with_profile`。默认值：

| 参数 | 默认 |
|---|---|
| `maxTriangles` / `maxBytes` | `500000` / `33554432` |
| edges / crease angle | on / `30°` |
| LOD / ratio / allocation | off / `0.15` / `proportional` |
| LOD max triangles / min unit triangles | `250000` / `8` |
| LOD curve samples / max memory | `8` / `67108864` |
| semantic proxy / render batch | off / off |
| coarse chunk max bytes | `maxBytes` |

`semantic-proxy=auto` 必须同时启用 `--lod` 和 adaptive allocation。LOD ratio 必须在
`(0,1)`，curve samples 必须为 `4..=32`，min unit triangles 不得大于 max triangles。
Fine、LOD、coarse、semantic proxy 和 render batch writer 均从各自最终几何生成真实
edges；edge bytes 计入 part/chunk budget。输出先写入同父目录 staging，整个 Tileset
通过 GLB/profile 校验后才 rename。默认拒绝已有目录；`--force` 失败时恢复旧目录。
`--output` 必须指向可命名的目录，不能使用文件系统根目录或当前目录 `.`，避免 `--force`
替换 workspace/root。Static render batch 烘焙 node transform 时保留 `COLOR_0`，并继续
通过 `_ICE3_SEMANTIC_INDEX` 映射原 Selection，不改变 fine geometry 的外观或业务身份。

### 6.2 Engine 消费

Engine `@ice3/assets` 直接读取 Studio 产物，不链接 Studio Prepare 实现：

- `verifyPreparedGLBIntegrity` 校验 `contentDigest` 覆盖的 BIN chunk；
- `readPreparedGLB` 恢复 triangle/edge geometry、edge material color、Preparation
  metadata、stats 和 node Selection；
- `readTileset` 解析 3D Tiles 1.1 hierarchy、relative GLB URI 和 bounds，再逐 part 调用
  `readPreparedGLB`。

`node scripts/verify-prepare-engine.mjs` 对 Studio `build` 和 direct partition 执行真实
Engine reader contract，结果写入 `output/prepare-engine-contract.json`。

### 6.3 Prepare 性能基线

`npm run benchmark:prepare` 对 large-industrial fixture 的五个 case 执行 warmup 1 次 +
测量 5 次：

- `build` 的 edges 开启与关闭；
- 直接 partition；
- 自适应 LOD 与 semantic proxy；
- 静态 render batch。

基线记录 p50/p95、peak RSS、output bytes、triangle/edge/partition/batch/coarse count 和
确定性 digest，写入 `fixtures/parity/prepare-performance-v1.json`；运行报告写入
`output/prepare-performance.json`。Peak RSS 上限为 512 MiB，p95 上限为同参数本地归档
legacy artifact 的 2 倍。验证门禁只读取 checked-in baseline，不运行旧 Toolchain。

Prepare CLI 的稳定失败码：
`ICE3_STUDIO_ASSET_INVALID_OPTIONS`、`ICE3_STUDIO_ASSET_INVALID_BUDGET`、
`ICE3_STUDIO_ASSET_BUDGET_WARNING`、`ICE3_STUDIO_ASSET_BUDGET_EXCEEDED`、
`ICE3_STUDIO_ASSET_OUTPUT_EXISTS`、`ICE3_STUDIO_ASSET_INVALID_GEOMETRY`、
`ICE3_STUDIO_ASSET_INVALID_PROFILE`、`ICE3_STUDIO_ASSET_TILESET_INVALID`、
`ICE3_STUDIO_ASSET_REPORT_WRITE_FAILED` 和 `ICE3_STUDIO_ASSET_IO`。
完整恢复规则见 [Diagnostics](diagnostics.md)。

`export tileset` 读取当前 committed revision，生成：

```text
tileset.json
parts/
coarse/
bindings.json
export.json
```

source GLB 只保留在 `projects/<id>/cache/preview/`，不会复制到发布目录。输出目录默认
不可覆盖；`--force` 通过 sibling staging、backup 和 rename 原子替换，失败时恢复上一份
成功 bundle。`agent-console-v1` 固定使用 60,000 triangles/part、64 MiB/part、LOD ratio
0.5、adaptive allocation、120,000 triangles LOD ceiling、4 MiB coarse chunk、
`semanticProxy=off` 和 static render batch。Parts 和 coarse GLB 都包含真实 edges；
`export.json.stats.edgeSegmentCount` 来自实际编码产物。导出只读取 Studio committed
revision。

`bindings.json` 优先按 metadata `tag` 聚合完整设备，`selectionId` 为
`plant/equipment/<tag>`；无 tag 的独立场景对象使用 source owner ID。`nodeSelectionIds`
按稳定 ID 排序，bounds 使用 world-space union，`area` 优先取 `train`，否则取 `stage`。

## 7. 环境变量

| 变量 | 作用 |
|---|---|
| `STUDIO_VIEWER_DIR` | Viewer 构建产物目录；不设则只提供 API |
| `STUDIO_SESSION_TOKEN` | 写入令牌；不设则随机生成并在启动时打印 |
| `STUDIO_APPROVAL_TOKEN` | 审批令牌，写入 Cookie 用于提交 ChangeSet |
| `STUDIO_AGENT_TOKEN` | 供 Agent 使用的令牌 |
| `STUDIO_PREVIEW_PACKETS` | 设为 `1` 启用增量 preview packet（实验） |
| `STUDIO_GPU_SCENE_SESSIONS` | 设为 `1` 启用 GPU 场景会话（实验） |
| `STUDIO_INCREMENTAL_SCENE_CACHE` | 设为 `0` 关闭 CPU 增量场景缓存（默认开启） |
| `ICE3_JT_CONVERTER` | JT 转换器路径，由部署方提供 |
| `ICE3_JT_TIMEOUT_MS` | JT converter 正整数毫秒 timeout，默认 `300000` |

`ICE3_JT_CONVERTER` 由 Studio import worker 直接解析。相对路径基于 Studio 进程工作
目录，部署时推荐使用绝对路径。Studio Server 不再读取任何 Sidecar/Node runtime 配置。

## 8. 工程格式

工程是一个目录，语义层与几何层分离：

| 路径 | 内容 | 可重建 |
|---|---|---|
| `project.json` | 工程头：schemaVersion、projectId、currentRevision、snapshotDigest、undo/redo 栈 | 否 |
| `revisions/` | 文档快照，每个 revision 一份全量 | 否 |
| `changesets/` | 持久化候选变更集 | 否 |
| `history/operations.jsonl` | append-only 操作历史 | 否 |
| `references/` | 引用资产（体积可达数百 MB） | 从源资产重建 |
| `cache/` | 几何与预览缓存 | 是 |
| `resources/` | 派生预览：`summary.json`、`layout.svg` | 是 |

撤销深度上限为 `MAX_UNDO_DEPTH`（200）。超出后最早的快照被回收，对应的撤销记录同时失效；`open` 只校验仍被引用的 revision 是否完整。语义层通常只有几百 KB，几何在 `references/` 下。

`.ice3` 容器把语义层打成单文件：

- 未压缩 tar（每块 512 字节），`tar -tf` 可直接查看内容。
- 根 `manifest.json` 记录 `magic`、`formatVersion`、`schemaVersion`、`projectId`、`revision`、内容 `digest` 和排除项。
- 默认排除 `cache/` 和 `references/`，因此容器通常只有几百 KB；引用资产需要单独分发。
- `pack` 会生成 `resources/summary.json` 与 `resources/layout.svg`（按区域着色的俯视散点图），便于不开编辑器时判断工程内容。

## 9. 参考资产清单

资产发布方提供一份清单，Studio 只按结构读取，不解释任何行业字段：

```json
{
  "assetId": "asset-000001",
  "materialSlots": [
    { "id": "tank-biological", "color": "#8ea6b2", "role": "tank" }
  ],
  "bindings": [
    {
      "selectionId": "asset/part/AE-101",
      "name": "Aeration tank A",
      "tag": "AE-101",
      "bounds": { "min": [0, 0, 0], "max": [20, 6, 13] },
      "slot": "tank-biological",
      "hidden": false,
      "metadata": { "stage": "biological", "area": "TrainA" }
    }
  ]
}
```

| 字段 | 作用 |
|---|---|
| `materialSlots` | 资产声明可覆盖的材质槽位与默认颜色 |
| `bindings[].slot` | 该绑定使用的槽位；缺省时保留资产原材质 |
| `bindings[].hidden` | 资产声明该构件不应显示（废弃件） |
| `bindings[].metadata` | 任意业务属性，内核不解释 |
| `bindings[].tag` / `stage` / `area` | 兼容字段，解析时并入 `metadata` |

Viewer 按槽位取色、按 `hidden` 隐藏，不解析节点名、不做行业推断。早期清单用
`equipment` 作为数组键，读取时兼容。

## 10. 原生预览

Studio Core 把文档求值成 GLB，committed preview 原子写入
`cache/preview/<revision>.glb`，candidate preview 写入
`cache/preview/candidates/<digest>.glb`。Viewer 通过 `/previews/<revision>` 和
`/previews/candidates/<digest>` 读取。响应 digest 从实际文件字节重新计算。

Preview 不启动外部进程，也没有可选优化器 fallback。预览失败不会修改当前 revision 或
已有有效 preview；`system.capabilities` 只报告 Studio 自身的能力。

## 11. Viewer 能力

`apps/viewer/src` 的实际模块：

| 模块 | 职责 |
|---|---|
| `app` | Workbench 主框架、Agent-first Review dock、表头与视觉 token |
| `viewport` | 视口、SceneAdapter、相机取景 |
| `navigation` | 对象搜索、层级过滤、对象树与树模型 |
| `forms` | 表单渲染、实例参数、表单到操作的编译 |
| `changeset` | 候选变更的 Provider、Store 与类型 |
| `agent` | Agent Mission Control、上下文、对话 Store 与传输层 |
| `session` | 会话与令牌管理 |
| `compute` | GPU/compute 证据展示 |
| `api`、`shared` | RPC 客户端与共享工具 |

Viewer 只调用公开读 API 并订阅 revision 事件，不持有任何私有修改通道。

Review dock 默认打开 Agent。Agent 面板显示 Gateway 状态、project、revision 和焦点
object；选择对象只更新 Agent 上下文，用户点击 `Inspect selected object` 后才进入人工
参数编辑。Agent Gateway 离线不会禁用对象搜索、Inspect、Changes、candidate review 或
人工提交。

Selection 以完整业务实体或 source object 为单位。参考资产中多个派生 selection 会归并到
同一 binding；视口和对象树使用同一实体身份，不把池壁、栏杆或其它面片当成独立实体。
