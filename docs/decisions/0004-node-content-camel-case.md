# 0004：NodeContent 字段统一为 camelCase

> Document ID: ICE3-STUDIO-ADR-004
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers
>
> Date: 2026-11-04

## 1. 背景

[ADR-003](0003-wire-format-camel-case.md) 统一了 `Operation` 的字段命名，但 Operation 的
负载里还有一处不一致：`node.create` 的 `content` 使用 `NodeContent`，而它与
`PartContent` 对同一语义的 `sweep` 给出两种拼写。

| 类型 | serde 配置 | `sweep` 字段输出 |
|---|---|---|
| `NodeContent`（`document.rs`） | `#[serde(tag = "type", rename_all = "snake_case")]` | `radial_segments` |
| `PartContent`（`large_scene/component.rs`） | `#[serde(tag = "kind", rename_all = "snake_case", rename_all_fields = "camelCase")]` | `radialSegments` |

在 `schemas/studio-api.schema.json` 中选择性计数可以复现：`radial_segments` 出现 16 次，
全部来自 `NodeContent`；`radialSegments` 出现 18 次，全部来自 `PartContent`。

这与 ADR-003 要解决的问题同源：同一个语义字段在两条入口上有两种拼写，模型必须记住
"node.create 用下划线、component 定义用驼峰"，拼错时被 `deny_unknown_fields` 硬拒绝。

**明确排除：** `geometry::Primitive` 不需要改动。它的四个变体字段都是单个单词
（`width`、`height`、`depth`、`radius`、`segments`、`rings`），`snake_case` 与
`camelCase` 序列化结果完全相同，不存在实际不一致。变体名（`box`、`plane`、
`cylinder`、`sphere`）在两种风格下也一致。

## 2. 决策

1. `NodeContent` 增加 `rename_all_fields = "camelCase"`。
2. `radial_segments` 字段增加 `#[serde(alias = "radial_segments")]`，保证历史
   revision 与既有 fixture 仍可读。
3. **变体名保持不变**（`group`、`primitive`、`custom_mesh`、`sweep`、
   `component_instance`、`instance_set`、`connection`、`reference_instance`）。
   枚举值继续使用 snake_case，与 `SceneContentKind`、`PartContent` 的 `kind` 值一致；
   ADR-003 只统一字段名，不改变枚举值。
4. `deny_unknown_fields` 保持开启。

## 3. 影响

| 角色 | 影响 |
|---|---|
| 历史工程 `revisions/*.json` | 不受影响，别名保证可读 |
| `fixtures/wastewater-mini/ai-plan.json` | 迁移 `radial_segments` → `radialSegments` |
| Agent 写请求 | 不破坏：旧拼写仍被接受 |
| Agent 读回显 | 字段名变化，随 `API_VERSION` 1.1 一并通告（ADR-003 已递增） |
| Web Viewer | 无影响：`apps/viewer` 与 `scripts/` 均不消费该字段 |
| `schemas/` 与各 crate `bindings/` | 重新生成，`NodeContent` 的 schema 字段名变化 |

由于 ADR-003 已经把 `API_VERSION` 递增到 `1.1` 并在同一批变更内，本决策不再次递增。

## 4. 验收

- `schemas/studio-api.schema.json` 中 `radial_segments` 计数为 0，
  `radialSegments` 覆盖 `NodeContent` 与 `PartContent` 两处。
- 历史 `radial_segments` JSON 仍可解析，`cargo test --workspace` 全绿。
- 变体名（`sweep`、`custom_mesh` 等）在 schema 中保持 snake_case 不变。
- `npm run verify` 全部门禁通过。

## 5. 关联文档

- `ICE3-STUDIO-ADR-003` — [Operation wire format 统一为 camelCase](0003-wire-format-camel-case.md)
- `ICE3-STUDIO-IDX-002` — [架构决策索引](README.md)
- `ICE3-STUDIO-REF-002` — [本地 API](../reference/api.md)
