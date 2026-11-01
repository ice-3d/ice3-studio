# Agent 主导的 Studio 工作台设计

> Document ID: ICE3-STUDIO-SPEC-014
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

## 1. 目标

让 Web Viewer 的默认入口体现“AI Agent 主导建模、人类辅助审阅”的产品分工，同时保留
现有三栏工作台和全部人工审查能力。

本次不引入独立的 Agent 产品模式。Objects、3D Scene 和右侧 Review dock 的布局保持
不变，只调整默认落点、上下文表达和人工接手路径。

## 2. 信息架构

| 区域 | 职责 |
|---|---|
| 左侧 Objects | 搜索、分组和定位 source/derived object，为 Agent 和人工操作提供上下文 |
| 中央 3D Scene | 展示当前 revision 或候选预览，并以完整业务实体响应 Selection |
| 右侧 Review dock | 默认显示 Agent；Inspect 和 Changes 保留人工精细操作与变更审查 |
| 底部 Evidence bar | 暴露 Operations、Timeline 和 Diagnostics |

`Agent` 是默认 tab。选择对象不会强制离开 Agent；用户可点击“Inspect selected object”
显式进入人工检查。

## 3. Agent 任务控制区

Agent 面板显示：

- Gateway 连接状态。
- 当前 project、revision 和焦点 object。
- 连接后的运行目标输入、计划、执行消息和错误。
- Gateway 离线时的人工接手说明和操作入口。

Gateway 不可用时不得伪装为已连接。人工表单、ChangeSet 审查和提交继续可用。

## 4. Object 与实体语义

- 对象搜索保留匹配项的祖先路径，并显示匹配数/总数。
- Selection 以完整业务实体或 source object 为单位，不把派生面片当成独立实体。
- 参考资产中的派生 selection 会映射回其业务 binding。
- 参数表单只提交实际改变的字段；API 原始 scalar、vector、color 和 enumeration 值按
  untagged wire format 处理。

## 5. 视觉与响应式

视觉继续使用工业控制台和 CAD 工作台语言。Agent 区域通过层级、状态和上下文表达获得
首要注意力，不增加大规模聊天画布或隐藏人工工具。

900px 以下按 Scene、Objects、Review dock 顺序纵向排列。对象列表使用独立滚动区域，
触控按钮至少 44px 高，页面不得出现横向溢出。

## 6. 验证

- E2E 验证默认 Agent tab、project/revision 上下文、焦点 object 和人工接手。
- E2E 验证搜索、实体选择、参数变更、candidate review 和 commit。
- Chrome 验证 1440px、1024px、768px 和 375px 布局。
- Viewer test、typecheck、production build 和依赖审计必须通过。

## 7. 非目标

- 不新增 Agent 模式切换。
- 不隐藏或删除 Objects、Inspect、Changes。
- 不修改 Agent Gateway 协议、模型供应商或凭据策略。
- 不改变 Document、Operation、ChangeSet 或 Toolchain 的权威边界。

## 8. 关联文档

- `ICE3-STUDIO-SPEC-002` — [Studio Web Workbench](2026-10-30-studio-web-workbench-design.md)
- `ICE3-STUDIO-SPEC-014` — 本设计
- `ICE3-DOC-ADR-006` — [Agent 主导的 Studio 工作台](https://github.com/ice-3d/ice3-doc/blob/main/docs/decisions/0006-agent-led-studio-workbench.md)
- `ICE3-STUDIO-PLAN-018` — [Agent 主导工作台实施计划](../plans/2026-11-01-agent-led-workbench.md)
