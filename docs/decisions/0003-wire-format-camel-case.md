# 0003：Operation wire format 统一为 camelCase

> Document ID: ICE3-STUDIO-ADR-003
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers
>
> Date: 2026-11-04

## 1. 背景

Studio 的公开 JSON-RPC 契约里，`Operation` 的字段名存在**两种风格并存**：

| 范围 | 风格 | 示例 |
|---|---|---|
| Agent 读接口、`*.patch` 操作、`ChangeSet*`、`Scene*`、`Agent*` | camelCase | `nodeId`、`setParameters`、`approvedDigest` |
| `node.*`、`primitive.update`、`mesh.replace`、`component.*`、`instance.*`（非 patch）、`connection.*`（非 patch）、`material.*`、`relation.*`、`constraint.*`、`instanceSet.*` | snake_case | `node_id`、`base_color`、`parent_id`、`definition_id` |

根因是 `Operation` enum 只声明了 `#[serde(tag = "type", deny_unknown_fields)]`，
没有给变体字段加 `rename_all`；而 `instance.patch` 等四个变体直接复用了 `patch.rs` 中
带 `#[serde(rename_all = "camelCase")]` 的结构体。两者在同一个反序列化入口里合流。

这不是文档瑕疵，是**契约内部不一致**，实证如下：

- `schemas/studio-api.schema.json` 中 `node_id` 出现 192 次、`nodeId` 51 次；
  `material_id` 42 次、`materialId` 34 次。
- `projects/wastewater-plant/revisions/00000004.json` 是 camelCase（126 处 `nodeId`），
  同目录 `history/operations.jsonl` 是 snake_case（452 条 `node.create`）。
- `apps/viewer/src/forms/operationCompiler.ts` 仍在生成 `node_id`。

对一个由非确定性调用者驱动的接口，这违反 ACI 的确定性判据：同一个语义字段在不同端点
有两种拼写，模型必须记住"哪种操作配哪种写法"，且拼错时的失败是 `deny_unknown_fields`
的硬拒绝，恢复成本高。

**本次范围外（已知但不在本决策内）：** `NodeContent` 仍只声明
`#[serde(tag = "type", rename_all = "snake_case")]`，没有 `rename_all_fields`，因此
`node.create` 的 `sweep` 内容仍输出 `radial_segments`，而同语义的 `PartContent.sweep`
已输出 `radialSegments`。该处由 [ADR-004](0004-node-content-camel-case.md) 单独处理，
避免与本决策的迁移窗口重叠。

`geometry::Primitive` 不受影响，无需改动：它的四个变体字段都是单个单词（`width`、
`radius`、`segments`、`rings`），两种命名风格输出完全相同。

## 2. 决策

**统一到 camelCase**，且采用"读双接受、写单输出"的过渡策略：

1. 序列化只输出 camelCase。
2. 反序列化同时接受 camelCase 与历史 snake_case 别名。
3. JSON Schema 与 TypeScript 声明只描述 camelCase。
4. 别名保留到 0.2.0 之后的下一个 minor，届时移除并在 release note 中通告。

选择 camelCase 而非反向统一，理由是：JSON-RPC 信封、Agent 读接口、ChangeSet、Scene、
Diagnostics 已经全部是 camelCase，只有 29 个 operation 变体中的一部分是历史遗留；
反向统一会改动更大的已发布面，且与生成的 TypeScript 声明风格冲突。

## 3. 实现方式

首选 `#[serde(rename_all_fields = "camelCase")]` 作用于 `Operation` enum，再给每个
带历史名的字段加 `#[serde(alias = "旧的 snake_case 名")]`。例如：

```rust
#[serde(tag = "type", deny_unknown_fields, rename_all_fields = "camelCase")]
pub enum Operation {
    #[serde(rename = "material.upsert")]
    MaterialUpsert {
        #[serde(alias = "material_id")]
        material_id: MaterialId,
        #[serde(alias = "base_color")]
        base_color: [f32; 4],
        // ...
    },
}
```

约束：

- `deny_unknown_fields` 必须保持开启；别名只放宽"已知字段的另一种拼写"，不放宽未知字段。
- **已验证**（2026-11-04，隔离 spike，serde 1.0.229 + schemars 1）：`rename_all_fields`
  与 `alias` 可以组合使用。camelCase 与历史 snake_case 均可读，序列化只输出 camelCase，
  同一对象内混用两种风格也能接受，`deny_unknown_fields` 仍拒绝未知字段；schemars 生成的
  schema 只出现 camelCase 名（`materialId` 出现、`material_id` 不出现）。因此不需要退回
  "抽独立 struct" 的备选方案。
- 四个 `*.patch` 变体不改结构，它们本来就是 camelCase。
- 不允许引入"运行时键名归一化中间层"：那会削弱 `deny_unknown_fields` 的防呆价值。

迁移分四步，每步独立可验证：

1. **spike**：验证 serde 与 schemars 对 `rename_all_fields` + `alias` 的行为，确认
   schema 只输出 camelCase、旧 JSON 仍可解析。
2. **读兼容**：加别名，保持序列化不变，全量测试通过。此时 schema 尚未变。
3. **写切换**：启用 camelCase 输出，重新生成 schema 与 bindings，更新 Viewer 的
   `operationCompiler.ts` 与 `instanceParameters.ts`，迁移 `fixtures/*/ai-plan.json`。
4. **收尾**：更新 `docs/reference/api.md` 的示例与 `capabilities.md` 的说明；跑通
   `npm run verify`；在 release note 标注别名移除时间。

## 4. 影响与兼容性

| 角色 | 影响 |
|---|---|
| 已有工程数据（`history/operations.jsonl`、`revisions/`） | 不受影响，别名保证可读 |
| 外部 Agent 写请求 | 不破坏：旧 snake_case 仍被接受，但 schema 只提示 camelCase |
| 外部 Agent 读回显（`changeset.get` 的 `entries[].operation`） | **破坏性**：字段名从 snake_case 变为 camelCase，需在 release note 通告 |
| Web Viewer | 需同步改 `operationCompiler.ts`、`instanceParameters.ts` 及其单测 |
| `schemas/studio-types.ts` 与各 crate `bindings/` | 重新生成，字段名变化 |

由于读回显是破坏性的，`API_VERSION` 需要小幅递增（`1.0` → `1.1`），并在
`system.capabilities` 中如实反映；不能静默改变响应形状。

## 5. 替代方案

| 方案 | 结论 |
|---|---|
| 统一到 snake_case | 拒绝。改动面覆盖 Agent 读接口、ChangeSet、Scene、Diagnostics 与全部生成绑定，远大于现状 |
| 保持双风格，只在文档里说明 | 拒绝。文档无法降低模型的拼写错误率，且 `deny_unknown_fields` 会让错误变成硬失败 |
| 引入运行时键名归一化（递归转 camelCase） | 拒绝。会静默接受本应被拒绝的输入，破坏防呆，且难以在 schema 中表达 |
| 一次性破坏性切换，不保留别名 | 拒绝。已有 452 条 `node.create` 历史操作与多个 fixture 会立刻不可读 |

## 6. 验收

- `schemas/studio-api.schema.json` 中不再出现 snake_case 的 operation 字段名
  （`node_id`、`material_id`、`base_color` 等计数为 0）。
- 同一份 history 与 fixtures 在迁移前后均可读，`cargo test --workspace` 全绿。
- 新增契约测试：同一 operation 用两种拼写均可解析，混用风格（如 `nodeId` + `base_color`）
  在同一对象内仍被接受；拼错字段名仍返回 `ICE3_STUDIO_OPERATION_INVALID`。
- `npm run verify` 32 道门禁通过。
- `docs/reference/api.md` 的 operation 示例全部为 camelCase，且 front-matter 与索引同步。

## 7. 关联文档

- `ICE3-STUDIO-IDX-002` — [架构决策索引](README.md)
- `ICE3-STUDIO-REF-002` — [本地 API](../reference/api.md)
- `ICE3-STUDIO-REF-001` — [能力清单](../reference/capabilities.md)
- `ICE3-STUDIO-ADR-002` — [本地 API 是产品边界](0002-local-api-boundary.md)
