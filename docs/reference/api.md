# ICE3 Studio 本地 API

> Document ID: ICE3-STUDIO-REF-002
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

Studio Server 对外部 Agent、脚本和 Web Viewer 暴露同一组公开 API。Viewer 没有私有写入
路径；所有修改都必须进入事务或 ChangeSet。

`API_VERSION` 当前保持 `1.0`。项目仍处于 pre-release，Phase 5B 已删除
`system.capabilities.toolchain`，因此该形状变化不承诺向后兼容；客户端必须按当前
generated schema 解析 capabilities，不能把 `1.0` 当作兼容性承诺。

## Transport

- `GET /health`：无需认证的进程和 contract health。
- `POST /rpc`：JSON-RPC 2.0。
- `GET /ws?token=...`：revision 和 ChangeSet event stream。
- `GET /schemas/studio-api.schema.json`：生成 JSON Schema。
- `GET /schemas/studio-types.ts`：生成 TypeScript declaration。
- `GET /previews/{revision}`：已提交 revision 的 Studio 原生 GLB preview。
- `GET /preview-packets/{revision}`：host policy 开启时的 `ICE3PGP1` preview packet。
- `GET /preview-packets/candidates/{digest}`：candidate preview packet。
- `GET /reference-assets/{assetId}/tileset.json`：Reference Asset Tileset。
- `GET /reference-assets/{assetId}/tiled/{path}`：不可变粗模或细模 tile。
- `GET /reference-assets/{assetId}/prepared.glb`：Prepared GLB fallback。
- `GET /reference-assets/{assetId}/bindings.json`：binding manifest。

默认 origin 是 `http://127.0.0.1:8120`。写入需要
`Authorization: Bearer <token>`；ChangeSet commit 还需要审批 Cookie。

## JSON-RPC envelope

```json
{
  "jsonrpc": "2.0",
  "id": "request-1",
  "method": "document.get",
  "params": {}
}
```

成功返回 `result`。失败返回 `error.code`、`error.message`、
`error.diagnosticCode` 和可选 `error.data`。解析错误使用 `-32700`，未知方法使用
`-32601`。

## 方法清单

静态 Schema 固化 37 个方法；运行中的 `system.capabilities` 另有两个实验性 GPU scene
方法，共 39 个。

| 方法 | 写 | 参数 | 返回 |
|---|---:|---|---|
| `system.health` | 否 | 无 | API/schema version 和状态 |
| `system.capabilities` | 否 | 无 | 方法、compute、权限与限制 |
| `system.agentManifest` | 否 | 无 | Agent tools、authority 和 workflow |
| `project.create` | 是 | `{ projectId }` | `{ opened: true }` |
| `project.open` | 是 | `{ projectId }` | `{ opened: true }` |
| `project.get` | 否 | 无 | 当前 Document |
| `project.save` | 是 | 无 | 当前已持久化 revision |
| `project.close` | 是 | 无 | 释放 writer lock |
| `document.get` | 否 | 无 | 当前 Document |
| `document.apply` | 是 | `ApplyRequest` | `ApplyResult` |
| `document.undo` | 是 | 无 | 新 revision |
| `document.redo` | 是 | 无 | 新 revision |
| `document.history` | 否 | 无 | operation history |
| `scene.get` | 否 | 无 | 当前 Scene |
| `scene.query` | 否 | filters | 确定性顺序的 Node |
| `scene.context` | 否 | 无 | 有界项目摘要 |
| `scene.search` | 否 | `SceneSearchRequest` | revision-bound page |
| `scene.bounds` | 否 | 无 | 按 Node ID 的 world bounds |
| `validation.run` | 否 | 无 | Diagnostic |
| `referenceAsset.register` | 是 | 四个 workspace-relative path | asset ID 和 descriptor |
| `referenceAsset.get` | 否 | `{ assetId }` | descriptor 和 URL |
| `referenceAsset.bindings` | 否 | `{ assetId, cursor?, limit? }` | binding page，limit `1..=100` |
| `evaluation.evaluate` | 否 | 无 | 按 Node ID 的 evaluated Mesh |
| `evaluation.query` | 否 | `EvaluationQueryRequest` | 有界 Geometry summary/payload |
| `evaluation.stats` | 否 | 无 | 三角、Mesh 和 cache 统计 |
| `evaluation.waitScene` | 否 | `{ revision? }` | 实验性 GPU scene status |
| `evaluation.visibleInstances` | 否 | `{ targetRevision, planes[6][4] }` | 可见实例与稳定 Node ID |
| `export.sourceGlb` | 是 | 无 | Source GLB metadata |
| `export.runtimePreview` | 是 | `RuntimePreviewRequest` | committed/candidate preview metadata |
| `changeset.create` | 是 | `{ baseRevision, title }` | 新 draft ChangeSet |
| `changeset.addOperations` | 是 | `{ changeSetId, expectedVersion, entries }` | 更新后的 ChangeSet |
| `changeset.removeOperations` | 是 | `{ changeSetId, expectedVersion, entryIds }` | 更新后的 ChangeSet |
| `changeset.evaluate` | 是 | `{ changeSetId, expectedVersion, includeCandidate? }` | 状态、Diagnostic、digest |
| `changeset.commit` | 是 | `{ changeSetId, expectedVersion, approvedDigest, idempotencyKey }` | committed revision |
| `changeset.get` | 否 | `{ changeSetId }` | 持久 ChangeSet |
| `changeset.getCandidate` | 否 | `{ changeSetId }` | candidate Document |
| `changeset.impact` | 否 | `{ changeSetId, expectedVersion }` | 影响范围与 Geometry 复用证据 |
| `changeset.list` | 否 | 无 | ChangeSet 列表 |
| `changeset.discard` | 是 | `{ changeSetId }` | 丢弃结果 |

实验方法必须先从 `system.capabilities.methods` 确认，再由调用方准备 CPU fallback。

## Transaction

```json
{
  "expectedRevision": 2,
  "dryRun": false,
  "idempotencyKey": "change-042",
  "operations": []
}
```

- `expectedRevision` 拒绝 stale write。
- 相同 `idempotencyKey` 和相同语义请求返回原结果；复用于不同内容会被拒绝。
- `dryRun` 执行完整校验、求值和 diff，但不持久化。
- 一个 Operation 无效会拒绝整批修改。
- 成功事务只增加一个 revision；失败事务和 dry run 不发布 event。

## Operation

当前有 29 种 Operation：

| 域 | 类型 |
|---|---|
| Node | `node.create`、`node.update`、`node.reparent`、`node.delete`、`primitive.update`、`mesh.replace` |
| Component | `component.define`、`component.update`、`component.patch`、`component.delete` |
| Instance | `instance.create`、`instance.update`、`instance.patch`、`instance.delete` |
| InstanceSet | `instanceSet.create`、`instanceSet.update`、`instanceSet.delete` |
| Connection | `connection.create`、`connection.update`、`connection.patch`、`connection.delete` |
| Material | `material.upsert`、`material.assign`、`material.patch` |
| Relation | `relation.create`、`relation.update`、`relation.delete` |
| Constraint | `constraint.upsert`、`constraint.delete` |

`*.patch` 只修改显式字段，但仍执行完整校验、diff、幂等和 revision 冲突检查。传统
Operation variant 顶层字段使用 snake_case（例如 `node_id`、`material_id`）；`*.patch`
payload 和返回的 Document 使用 camelCase（例如 `nodeId`、`setParameters`）。

## Component 与参数

Component Parameter 支持 Number、Vector3、Color 和 Enumeration。API wire format 是
untagged：

```json
{
  "radius": 15,
  "centre": { "x": 0, "y": 2, "z": 0 },
  "tint": [0.2, 0.4, 0.6, 1.0],
  "mode": "clarifier"
}
```

Number 可直接声明 inclusive `minimum` 和 `maximum`；Color 必须包含四个 `0.0..=1.0`
分量。不要发送 `{kind:"number",value:15}` 一类包装对象。

Part scalar 和 vector 字面量同样直接写值：

```json
{
  "width": 12,
  "height": { "param": "depth" },
  "depth": { "expr": "radius - wall / 2" }
}
```

`{param:"name"}` 引用实例参数；`{expr:"..."}` 使用受控四则运算、括号和白名单数学函数。
禁止脚本、环境访问和任意代码执行。

实例、实例集、嵌套组件的安全上限由 server 校验：16 层组件引用、单组件 4096 parts、
单实例集 100,000 items。

## Material

```json
{
  "type": "material.upsert",
  "material_id": "material-000001",
  "name": "Process water",
  "base_color": [0.24, 0.52, 0.62, 1.0],
  "opacity": 0.72,
  "edge_color": null,
  "shading": "water"
}
```

`opacity < 1` 导出为 glTF alpha blend。`shading` 允许 `directional`、`flat` 和 `water`；
省略时等价于 `directional`。`material.patch` 使用显式 `clearEdgeColor` 清除边缘色。

## Agent context 与搜索

`system.agentManifest` 返回 Agent tools、当前 token authority、limits 和 inspection、
proposal、review、commit、recovery workflow。manifest 只描述能力，实际权限仍由 RPC
边界执行。

`scene.context` 返回 counts、world bounds、单位/坐标系、source facets、最多 50 个
relation type 和当前权限，不返回完整 Node 或 Geometry 集合。

`scene.search` 支持：

- `scope: source | derived | both`
- metadata、content、component、material、visibility 和 bounds filter
- name/ID query
- field projection
- `limit 1..=100`

返回 `{revision, items, nextCursor}`。Cursor 绑定 revision 和 normalized filter；revision
或 filter 变化后必须重新搜索。

`evaluation.query` 接受 1 至 32 个唯一派生 Node ID。默认返回 vertex/index count、world
bounds 和 Geometry digest；只有显式请求时才返回 positions/indices。

## ChangeSet

AI 提案和人工表单修改使用
[ChangeSet Review](change-sets.md) 中的生命周期。`changeset.impact` 返回 candidate
digest 和有界影响证据，scope 可为 `local`、`targeted`、`broad`、`full_rebuild` 或
`unavailable`，但不发布 candidate Scene。

Commit 要求：

- `expectedVersion` 与 `approvedDigest` 匹配。
- ChangeSet 处于 `ready` 且基线未前进。
- 调用方拥有审批权限。
- `idempotencyKey` 唯一，或重试与原请求完全一致。

## Reference Asset

`referenceAsset.register` 把 source GLB、可选 Prepared GLB、Tileset 和 binding manifest
复制到活动项目并作为不可变文件发布。路径相对 workspace；绝对路径、`..`、symlink escape、
无效 GLB 和 digest mismatch 在发布前失败。

Descriptor 只返回 hash、world bounds、source/mesh/material/binding count 和 timestamp。
Document 为每个可编辑 semantic binding 保存一个 `reference_instance` Node：

```json
{
  "type": "reference_instance",
  "assetId": "asset-000001",
  "selectionId": "asset/binding/AE-101"
}
```

Binding transform、visibility、material 和 metadata 修改使用普通 `node.update`、
`material.assign`、`node.delete` 和 ChangeSet。带 authored override 的 baseline export
返回 `ICE3_STUDIO_REFERENCE_EXPORT_UNSUPPORTED`，不会静默丢弃 overlay。

## Runtime Asset 导出

Runtime Asset 是 CLI 产品入口，不是额外的 HTTP 写入方法：

```bash
ice3-studio export tileset \
  --project PROJECT \
  --output DIRECTORY \
  --profile agent-console-v1 \
  [--force]
```

命令只读取 committed revision；Studio Server 持锁时使用 read-only project reader，不改写
Document、ChangeSet 或 revision。Pipeline 依次执行 Prepared GLB validation、atomic
partition unit 分析、空间切分、LOD/coarse chunk、Tileset 和 binding aggregation。

发布目录：

```text
DIRECTORY/
├── tileset.json
├── parts/
├── coarse/
├── bindings.json
└── export.json
```

`export.json` 记录 `revision`、`profile`、`sourceDigest`、`tilesetDigest`、
`bindingsDigest`、`bundleDigest`、文件相对路径/长度/digest 和统计，包括
`edgeSegmentCount`。Parts 与 coarse GLB 都包含真实 render edges。所有 Tileset content
URI 均相对发布目录。完整 source GLB 只写入项目 `cache/preview/`，不进入 bundle。

`bindings.json` 以 profile `1.0` 发布。带 `metadata.tag` 的节点聚合为
`plant/equipment/<tag>`；无 tag 时使用 source owner identity。设备内
`nodeSelectionIds` 稳定排序，world bounds 取并集，`area` 优先使用 `train`，否则使用
`stage`。

发布是失败关闭的：输出已存在时默认返回错误；`--force` 先完成全部 staging 写入，再把
旧目录 rename 为 backup、staging rename 为目标目录。替换失败会恢复旧目录。
`scripts/compare-partition-imports.mjs` 连续执行 direct partition、LOD、semantic proxy、
render batch、coarse chunk 和 skip-edges case，检查 URI、bounds、Selection、edge
统计/material、byte budget 和字节确定性；该脚本不进入 Studio runtime。JT Source
Import 由 Studio isolated worker 调用 host-owned converter 完成，不在
`export tileset` 的生产调用图中。

Engine/Agent Console 的消费契约由 `scripts/verify-prepare-engine.mjs` 单独验证：
`readPreparedGLB` 恢复 edges 和 Selection，`readTileset` 解析 relative content URI，
并逐 part 验证 Prepared profile。Engine 不通过 API 请求 Studio 重算几何，也不链接
Studio Rust crate。

## Event 与 Revision

认证 WebSocket 按顺序发送 ChangeSet 和 revision event，例如：

```json
{
  "event": "revision.committed",
  "revision": 3
}
```

客户端收到 revision event 后重新读取 Document 和 preview。Document 刷新不要求替换 canvas
或 Engine instance。

## 关联文档

- `ICE3-STUDIO-ARC-001` — [Studio 架构](../architecture.md)
- `ICE3-STUDIO-REF-001` — [能力清单](capabilities.md)
- `ICE3-STUDIO-REF-003` — [ChangeSet 审阅](change-sets.md)
- `ICE3-STUDIO-REF-004` — [Diagnostics](diagnostics.md)
