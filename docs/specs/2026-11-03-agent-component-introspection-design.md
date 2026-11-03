# Agent 组件定义内省设计

> Document ID: ICE3-STUDIO-SPEC-020
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

## 1. Summary

Agent 读接口已经覆盖场景摘要（`scene.context`）、节点检索（`scene.search`）和有界几何查询
（`evaluation.query`），但缺少组件定义的内省通道：`scene.search` 只返回
`componentDefinitionId`，没有任何方法读回该定义的参数 schema 与端口清单。Agent 想知道
"这个组件有哪些参数、取值范围是多少、能在哪个端口接管"，只能拉取整个 `document.get`
——默认工程快照约 600 KB，超出任何 LLM 的上下文预算，实际退化为盲试探。

本设计新增一个有界读方法 `component.get`，按 ID 返回单个组件定义的参数、端口和部件
摘要，不引入第二真源、不改任何既有方法形状。

## 2. Goals

- Agent 在拿到 `componentDefinitionId` 后，一次调用即可获得：
  - 参数名、类型、范围/枚举值、默认值和单位；
  - 端口名、所属部件、位置、方向、介质与管径；
  - 部件层级摘要（id、父部件、几何类别、repeat、可见性）；
  - 该定义在当前 revision 的实例计数。
- 响应字节预算与 `scene.context` 同一量级（大场景验收 fixture 上目标 16 KiB）。
- 只读 token 即可调用；read facade 模式与现有 Agent 读方法一致。
- 复用既有 wire 类型（`ComponentParameter`、`ComponentPort`），不复制 schema。

## 3. Non-Goals

- 不返回部件的完整 binding（transform 表达式、subtract、材质绑定）。构造新定义走
  `component.define`，不需要从摘要重建旧定义。
- 不做 material、constraint 的同类内省；`material.get` 留给后续设计。
- 不改 `document.get`、`scene.search` 的任何响应形状。
- 不引入筛选、分页或 cursor；单定义响应天然有界。

## 4. Method Contract

### 4.1 Method

`component.get`

### 4.2 Request

```text
ComponentGetParams {
  componentId: string
}
```

`componentId` 必须是当前 Document 中存在的 `ComponentDefinition` ID。

### 4.3 Response

```text
ComponentGetResult {
  revision: number
  component: ComponentDefinitionSummary
}
```

```text
ComponentDefinitionSummary {
  id: string
  name: string
  parameters: ComponentParameter[]
  ports: ComponentPort[]
  parts: ComponentPartSummary[]
  partCount: number
  partsTruncated: boolean
  instanceCount: number
  metadata: Record<string, string>
}
```

`ComponentParameter` 与 `ComponentPort` 复用既有类型与 wire 格式。
`ComponentPartSummary`：

```text
{
  id: string
  parentId: string | null
  kind: "group" | "box" | "plane" | "cylinder" | "sphere"
      | "custom_mesh" | "sweep" | "component_reference"
  referencedDefinitionId: string | null   // 仅 component_reference
  repeat: number | null                   // repeat 展开前的声明值
  visible: boolean
}
```

- `partCount` 是定义中部件条目的总数（`repeat` 不展开）；`parts` 至多
  `MAX_PART_SUMMARIES = 100` 条，超出时 `partsTruncated: true`。
- `instanceCount` 统计当前 Document 中以该定义为 `definitionId` 的
  `component_instance` 节点与 `instance_set` 节点之和。
- 部件按定义顺序返回；参数、端口按定义顺序返回，均不做排序重写。

### 4.4 Errors

未知 `componentId` 返回 `ICE3_STUDIO_REFERENCE_INVALID`，`error.data` 包含
`category: "not_found"`、`retryable: false`、`objectId` 与建议（重新执行
`scene.search` 获取有效 ID）。空参数校验失败沿用 `ICE3_STUDIO_API_INVALID`。

## 5. Manifest 与 Capabilities

- `system.agentManifest` 新增工具：

```text
name: component.get
sideEffect: read
requiresApproval: false
idempotency: none
inputSchemaRef: ...#/types/ComponentGetParams
outputSchemaRef: ...#/types/ComponentGetResult
```

- `inspect` workflow 扩展为
  `scene.context → scene.search → component.get | evaluation.query`：检索命中组件实例后，
  先内省定义再决定参数 patch 或连接提案。
- `system.capabilities.methods` 与静态 schema 的 methods 列表同步加入
  `component.get`（静态方法 37 → 38，含实验 GPU 方法总数 39 → 40）。

## 6. Authorization

读方法，无需写入令牌：read-only、proposal、approval、autonomous 均可调用，与
`scene.context`、`scene.search` 一致。方法不评估场景、不写缓存、不发布事件。

## 7. Performance And Context Bounds

- 大场景验收 fixture 上单次响应目标不超过 16 KiB；这是目标值，不是对任意
  metadata/参数数量的硬承诺（与 `scene.context` 同一口径）。
- 实现只读 `session.document.components` 与 scene 节点遍历，不触发几何求值。
- `MAX_PART_SUMMARIES = 100` 为硬上限，保证最坏情况部件摘要段不超过约 8 KiB。

## 8. Testing

### 8.1 Core Contract

- `ComponentGetParams` / `ComponentGetResult` camelCase 序列化与
  `deny_unknown_fields`；
- TypeScript 声明将可缺省字段标记为 optional；
- 部件摘要 kind 枚举 snake_case 序列化。

### 8.2 Server Contract

- 播种含参数（带范围与枚举）、端口、多部件与 `component_reference` 部件的定义：
  `component.get` 返回完整参数 schema 与端口清单；
- `instanceCount` 与播种的实例数一致；
- 未知 ID 返回结构化 not_found 错误；
- 超过 100 部件的定义返回 `partsTruncated: true` 与 `partCount` 全量计数；
- read-only（无 token）调用成功；
- manifest 工具列表包含 `component.get` 且 schema 引用有效；
- 既有方法响应形状不变。

## 9. Documentation

- `docs/reference/api.md` 方法表加入 `component.get`（读、`ComponentGetParams`、
  `ComponentGetResult`），方法计数更新；
- `README.md` API 章节计数 39 → 40；
- `docs/reference/capabilities.md` 方法计数同步；
- 生成式 JSON Schema 与 TypeScript 声明随 `studio-cli schema` 更新。

## 10. Delivery Order

1. `studio-core::agent_contract` 类型与常量；
2. `studio-server::agent` 的 `component_get` facade 与 manifest 工具；
3. `rpc.rs` dispatch、capabilities 方法列表、`studio-cli schema` methods/types；
4. 核心契约测试与 server 契约测试；
5. 文档与 schema 再生成。

每步独立可测；不依赖实验 GPU 路径。

## 11. 关联文档

- `ICE3-STUDIO-SPEC-009` — [Agent-Friendly Modeling API](2026-10-31-agent-friendly-modeling-api-design.md)
- `ICE3-STUDIO-SPEC-010` — [Agent Iteration Impact Analysis](2026-10-31-agent-iteration-impact-design.md)
- `ICE3-STUDIO-REF-002` — [本地 API](../reference/api.md)
