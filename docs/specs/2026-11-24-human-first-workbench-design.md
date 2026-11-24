# 人本优先 Studio 工作台设计

> Document ID: ICE3-STUDIO-SPEC-040
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

## 1. 摘要

Studio Viewer 是给人使用的 3D 建模工作台。Agent 通过公开 RPC、MCP 和 ChangeSet API
操作 Studio，不需要嵌入 Viewer 的专用页面。Viewer 不再显示 Agent tab、目标输入、
Gateway 连接状态、运行进度或 Agent 对话。

```text
外部 Agent → Studio RPC / MCP → typed Operations → ChangeSet
人 → Objects / 3D Scene / Inspect → ChangeSet review → human approval
```

## 2. 目标

- 让人工建模成为 Workbench 的默认和首要路径。
- 让 Agent 保持为机器 API 使用者，而不是 Viewer 的界面角色。
- 删除无人使用的前端 Agent Store、Transport、Provider、Panel 和 Gateway 测试夹具。
- 保留 ChangeSet 作为人类审阅 Agent 或人工提案的统一边界。
- 保持对象搜索、实体 Selection、视口编辑、Evidence 和人工提交可用。

## 3. 非目标

- 不删除公开 RPC、MCP、Agent 工具清单或 ChangeSet API。
- 不改变人工审批和提交权限。
- 不在 Viewer 中增加新的 Agent、Prompt、模型配置或策略界面。

## 4. Workbench

Review dock 只包含：

- `Inspect`：默认 tab，Transform、图元、组件参数、材质与本地草稿。
- `Changes`：候选 diff、诊断、审阅与人工批准。

Objects、3D Scene、Inspect、Changes 和 Evidence 是完整的人工工作流。选择对象后直接
进入 Inspect 编辑，不再经过 Agent 焦点或人工接手按钮。

移动端顺序为 3D Scene → Objects → Review dock，优先呈现模型和对象管理。

## 5. Agent 边界

Agent 继续通过 Studio 的公开 API 工作：

- 查询 scene、context、material 和 route inspection。
- 创建 ChangeSet 并追加 typed Operations。
- 求值候选、读取 impact 和 diagnostics。
- 将候选交给人类审阅。

Agent 不能自动审批或提交。Viewer 不连接独立的 Agent Gateway，也不包含模型凭据、
Prompt 或 Agent policy。

## 6. 验证

- Viewer 默认打开 Inspect，页面不存在 Agent tab。
- 人工编辑、审阅、提交和移动端布局通过 Chrome E2E。
- `/agent/ws`、Agent 前端状态机和 Gateway fixture 已移除。
- MCP、Agent API 和 ChangeSet 审批测试继续通过。
