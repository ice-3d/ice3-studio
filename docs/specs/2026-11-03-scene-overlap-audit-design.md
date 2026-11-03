# 场景空间重叠审计设计

> Document ID: ICE3-STUDIO-SPEC-021
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

## 1. Summary

`no_overlap` 目前是 opt-in 约束：只有作者显式列出节点集才检查 AABB 相交。Agent 放置
设备时没有任何默认的空间冲突信号——已知深度重叠的设备对正是这类错误。本设计把空间
重叠审计升级为 `validation.run` 的可选全局体检：warning 级诊断，不 fail 事务，不改任何
既有响应形状。

## 2. Goals

- Agent 一次调用即可审计已提交 revision 的实体空间冲突。
- 通用几何规则，不引入场景语义（内核无任何行业词汇，红线不变）。
- 事务路径（`document.apply`、`changeset.evaluate`）完全不动，零回归。
- 误报可控：贴地、埋深、薄板、管线路由与父子层级不产生噪音。

## 3. Non-Goals

- 不改 `no_overlap` 约束的现有语义（显式约束仍 fail 事务）。
- 不对 ChangeSet candidate 做重叠预检（candidate 审计留给后续设计）。
- 不做精确碰撞（三角级）；AABB 级别信号已足够触发人工或 Agent 复查。
- 不自动修复、不阻断提交。

## 4. Method Contract

### 4.1 Request

`validation.run` 增加可选参数：

```text
ValidationRunRequest {
  includeOverlaps?: boolean = false
}
```

`deny_unknown_fields`、camelCase。`params: {}` 与省略 params 行为不变。

### 4.2 Response

响应仍是 `Diagnostic[]`。`includeOverlaps: true` 时，常规诊断之后追加重叠 warning：

```text
{
  "code": "ICE3_STUDIO_GEOMETRY_OVERLAP",
  "operationIndex": null,
  "objectId": "<nodeA>|<nodeB>",
  "pointer": "/scene",
  "message": "spatial overlap between <nameA> and <nameB>: x=<dx>m y=<dy>m z=<dz>m"
}
```

不新增 severity 字段；warning 语义由诊断码表达，与既有 27 码的扁平分类一致。

## 5. Audit Rules

实体 = 源节点（含其派生部件的 world bounds 并集，复用 `derived_source_bounds`）。

按顺序排除：

1. **Connection 节点**：路由在结构间穿行是其设计行为，由 RoutePolicy 管辖。
2. **Slab 实体**：最薄维度 < 0.5 m 的薄板（地面、道路、铺装、围栏、绿篱）——
   与竖向构件的边界接触和浅层交叠不构成冲突信号。
3. **祖先-后代对**：层级包含的 bounds 交叠是正常嵌套。
4. **单向包含**：AABB 单向包含（池内水面、建筑内设备）是嵌套不报；
   **双向包含（实体重合）是最严重的冲突，必须上报**。
5. **轴向深度**：三轴交叠深度均 > 0.1 m 才报告；接触（深度 0）不算。
6. **体积占比**：交叠体积占较大实体体积 < 15% 视为贴边或室内设备突出轮廓，
   不上报。AABB 级审计抓深嵌压叠，浅接触留给视觉与人工复查。

实测（681 节点污水厂工程）：无体积规则时 14 条 warning（其中过半为建筑×室内设备
噪声）；加规则后 1 条（检修棚罩泵组，AABB 无法与压叠区分的灰色地带，保留为复查
信号）。

上报规则：

- 每对实体一条诊断，按 `(min(idA,idB), max(idA,idB))` 确定性排序；
- 上限 `MAX_OVERLAP_DIAGNOSTICS = 50` 条，超出时追加一条汇总诊断报告总数。

## 6. Authorization 与性能

- `validation.run` 保持只读方法，全部 authority 可调用。
- `includeOverlaps` 触发一次场景求值（复用 large-scene 求值路径）；WWTP 级工程约
  数百毫秒，仅显式请求时发生。
- 事务校验（`validate_document`）不调用重叠审计。

## 7. Manifest

`system.agentManifest` 新增工具：

```text
name: validation.run
sideEffect: read
requiresApproval: false
idempotency: none
inputSchemaRef: ...#/types/ValidationRunRequest
outputSchemaRef: null   // Diagnostic[] 已在 schema types 中
```

`inspect` workflow 追加 `validation.run`：检索与内省之后做空间审计。

## 8. Testing

- Core：重叠对报告（构造两个相交箱体）与实体重合上报；连接节点、slab、父子、
  单向包含、接触、低于深度阈值各一条排除用例；50 条上限与汇总诊断。
- Server 契约：`params: {}` 响应与既有快照一致（无新诊断）；`includeOverlaps` 在
  重叠场景返回结构化 warning；unknown 字段被拒绝。
- 回归：`npm run verify` 全门禁通过，事务行为不变。

## 9. Documentation

- `docs/reference/diagnostics.md` 新增 `ICE3_STUDIO_GEOMETRY_OVERLAP`；
- `docs/reference/api.md` 的 `validation.run` 参数改为 `ValidationRunRequest`；
- `docs/reference/capabilities.md` 诊断码镜像 27 → 28；
- schema methods/types 更新（静态方法数不变，新增请求类型）。

## 10. 关联文档

- `ICE3-STUDIO-SPEC-020` — [Agent 组件定义内省](2026-11-03-agent-component-introspection-design.md)
- `ICE3-STUDIO-REF-003` — [Diagnostics](../reference/diagnostics.md)
