# ICE3 Studio 文档索引

> Document ID: ICE3-STUDIO-IDX-001
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

## 从这里开始

| ID | 标题 | Status | 用途 |
|---|---|---|---|
| `ICE3-STUDIO-RM-001` | [Studio README](../README.md) | Implemented | 项目定位、当前状态和文档入口 |
| `ICE3-STUDIO-GUIDE-START-001` | [Getting Started](guides/getting-started.md) | Implemented | 本地启动、API 工作流、验证和性能基线 |
| `ICE3-STUDIO-ARC-001` | [Architecture](architecture.md) | Implemented | 组件边界、真源、安全和延期范围 |
| `ICE3-STUDIO-REF-API-001` | [API Reference](reference/api.md) | Implemented | JSON-RPC、Operation、revision 和导出契约 |
| `ICE3-STUDIO-REF-DIAG-001` | [Diagnostics](reference/diagnostics.md) | Implemented | 稳定错误码和恢复规则 |
| `ICE3-STUDIO-REF-CHANGESET-001` | [ChangeSet Review](reference/change-sets.md) | Implemented | 候选变更、digest 审批、权限和恢复 |
| `ICE3-STUDIO-REF-TOOL-001` | [Toolchain Sidecar](reference/toolchain-sidecar.md) | Implemented | 固定版本 Sidecar、能力协商、缓存与故障降级 |
| `ICE3-STUDIO-ADR-0001` | [Document as Source of Truth](decisions/0001-document-as-source-of-truth.md) | Implemented | Document 与派生 Geometry 的决策 |
| `ICE3-STUDIO-ADR-0002` | [Local API Boundary](decisions/0002-local-api-boundary.md) | Implemented | 本地 API 与 Viewer 边界决策 |
| `ICE3-STUDIO-SPEC-002` | [Studio Web Workbench 设计](specs/2026-10-30-studio-web-workbench-design.md) | Implemented | AI-native 审查、表单编辑、ChangeSet 和通用工作台设计 |
| `ICE3-STUDIO-SPEC-003` | [Native GPU Compute 设计](specs/2026-10-30-native-gpu-compute-design.md) | Proposed | Native `wgpu` 后端、调度、增量缓存和量化验收 |
| `ICE3-STUDIO-SPEC-004` | [Toolchain Sidecar Integration 设计](specs/2026-10-30-studio-toolchain-sidecar-design.md) | Implemented | Studio 与 Toolchain 的版本化本地 Sidecar、能力协商和资产边界 |
| `ICE3-STUDIO-SPEC-005` | [Large-Scene Authoring 设计](specs/2026-10-30-large-scene-authoring-design.md) | Proposed | 参数化组件、实例、连接、GPU 求值、分区发布和大型场景验收 |
| `ICE3-STUDIO-PLAN-006` | [Large-Scene Authoring Foundation 实施计划](plans/2026-10-30-large-scene-authoring-foundation.md) | Implemented | M1 文档 schema、组件、实例、连接、CPU 求值和缓存 |
| `ICE3-STUDIO-PLAN-005` | [Toolchain Sidecar Integration 实施计划](plans/2026-10-30-studio-toolchain-sidecar.md) | Implemented | 共享契约、Sidecar、Rust 客户端、Server 集成和发布验收 |
| `ICE3-STUDIO-PLAN-002` | [ChangeSet 与审批 API 实施计划](plans/2026-10-30-studio-changeset-api.md) | Implemented | 候选变更集、持久化、双权限、预览和恢复 |
| `ICE3-STUDIO-PLAN-003` | [Studio Web Workbench 实施计划](plans/2026-10-30-studio-web-workbench-ui.md) | Implemented | React 工作台、只读画布、通用表单和 Agent 界面 |
| `ICE3-STUDIO-SPEC-001` | [AI-Native Modeling MVP 设计](specs/2026-10-30-ai-native-studio-mvp-design.md) | Implemented | Rust Core、API、数据模型、Viewer 和验收设计 |
| `ICE3-STUDIO-PLAN-001` | [AI-Native Modeling MVP 实施计划](plans/2026-10-30-ai-native-studio-mvp.md) | Implemented | Rust Core、API、Viewer、AI 验收和性能的实施步骤 |
| `ICE3-STUDIO-POL-001` | [仓库规则](../AGENTS.md) | Implemented | 架构、交付和命名边界 |

## 状态说明

- `Proposed`：设计待审阅，尚无对应实现。
- `Implemented`：实现、测试和用户入口均已交付。

## 关联文档

- `ICE3-DOC-ARC-001` — ICE3 家族架构
- `ICE3-IND-RM-001` — Industrial Semantic Core
- `ICE3-TOOL-RM-001` — Toolchain
- `ICE3-ENG-RM-001` — Engine
- `ICE3-AGENT-RM-001` — Agent Console
