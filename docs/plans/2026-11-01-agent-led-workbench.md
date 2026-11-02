# Agent 主导工作台实施计划

> Document ID: ICE3-STUDIO-PLAN-018
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

**目标：** 在不改变现有三栏结构和公开 API 的前提下，让 Agent 成为 Viewer 的默认工作
入口，并保留人工定位、检查、修改和审批路径。

**设计：** [Agent 主导的 Studio 工作台设计](../specs/2026-11-01-agent-led-workbench-design.md)

## 实施结果

- [x] 将 Review dock 默认 tab 从 `Inspect` 改为 `Agent`。
- [x] 在 Agent 面板显示 project、revision、焦点 object 和 Gateway 状态。
- [x] 将对象搜索、过滤、层级保留和匹配计数接入 Objects 面板。
- [x] 保留 source/derived identity，并将 Selection 收敛到完整业务实体。
- [x] 增加 Agent 到 Inspect 的显式人工接手入口。
- [x] 保留 ChangeSet 审查、candidate preview 和人工提交流程。
- [x] 按 untagged API format 处理直接 scalar、vector、color 和 enumeration 参数。
- [x] 统一工作台 token、状态卡、表单、对象树和响应式布局。

## 验证结果

```bash
PLAYWRIGHT_CHANNEL=chrome npm run verify
```

- 18 道仓库门禁通过。
- Viewer 单元测试 34 项通过。
- Chrome E2E 8 项通过。
- Production build 通过。
- 依赖审计为 0 个漏洞。

浏览器验收覆盖 1440px、1024px、768px 和 375px；页面无横向溢出，Agent 人工接手按钮
在窄屏下达到 44px 触控高度。

## 关联文档

- `ICE3-STUDIO-SPEC-014` — [Agent 主导的 Studio 工作台设计](../specs/2026-11-01-agent-led-workbench-design.md)
- `ICE3-STUDIO-PLAN-003` — [原 Studio Web Workbench 实施计划](2026-10-30-studio-web-workbench-ui.md)
- `ICE3-STUDIO-ARC-001` — [Studio 架构](../architecture.md)
