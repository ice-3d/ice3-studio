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
| `ICE3-STUDIO-GUIDE-001` | [污水模块 AI 验收夹具](guides/wastewater-fixture.md) | Implemented | `wastewater-mini` 契约 fixture 的确定性 AI 建模计划 |
| `ICE3-STUDIO-GUIDE-002` | [Large Industrial Fixture](guides/large-industrial-fixture.md) | Implemented | 576 万三角形参考资产、204 个语义绑定及程序化 API fixture |
| `ICE3-STUDIO-ARC-001` | [Architecture](architecture.md) | Implemented | 组件边界、真源、安全和延期范围 |
| `ICE3-STUDIO-REF-CAP-001` | [能力清单](reference/capabilities.md) | Implemented | 39 个方法、27 种 Operation、7 个 ChangeSet 状态、诊断码、CLI 与环境变量的代码镜像 |
| `ICE3-STUDIO-REF-API-001` | [API Reference](reference/api.md) | Implemented | JSON-RPC、Operation、revision 和导出契约 |
| `ICE3-STUDIO-REF-DIAG-001` | [Diagnostics](reference/diagnostics.md) | Implemented | 稳定错误码和恢复规则 |
| `ICE3-STUDIO-REF-CHANGESET-001` | [ChangeSet Review](reference/change-sets.md) | Implemented | 候选变更、digest 审批、权限和恢复 |
| `ICE3-STUDIO-REF-TOOL-001` | [Toolchain Sidecar](reference/toolchain-sidecar.md) | Implemented | 固定版本 Sidecar、能力协商、缓存与故障降级 |
| `ICE3-STUDIO-ADR-0001` | [Document as Source of Truth](decisions/0001-document-as-source-of-truth.md) | Implemented | Document 与派生 Geometry 的决策 |
| `ICE3-STUDIO-ADR-0002` | [Local API Boundary](decisions/0002-local-api-boundary.md) | Implemented | 本地 API 与 Viewer 边界决策 |
| `ICE3-STUDIO-SPEC-002` | [Studio Web Workbench 设计](specs/2026-10-30-studio-web-workbench-design.md) | Implemented | AI-native 审查、表单编辑、ChangeSet 和通用工作台设计 |
| `ICE3-STUDIO-SPEC-003` | [Native GPU Compute 设计](specs/2026-10-30-native-gpu-compute-design.md) | Experimental | Native `wgpu` 实现完整，当前设备未达到 release 性能门禁 |
| `ICE3-STUDIO-SPEC-006` | [GPU-Resident Geometry 设计](specs/2026-10-31-gpu-resident-geometry-design.md) | Experimental | 功能已交付，但 resident 性能门禁未通过；CPU 与 WebGL2 仍为默认 |
| `ICE3-STUDIO-SPEC-007` | [GPU Resident Hot-Path Redesign](specs/2026-10-31-gpu-hot-path-redesign-design.md) | Experimental | 稳定 handle、一次性 digest、异步批处理和分阶段 GPU 证据 |
| `ICE3-STUDIO-SPEC-008` | [Revision-Scoped GPU Scene Session](specs/2026-10-31-gpu-scene-session-design.md) | Experimental | CPU 增量场景默认启用；GPU 增量无额外性能优势，保持实验性 |
| `ICE3-STUDIO-SPEC-009` | [Agent-Friendly Modeling API](specs/2026-10-31-agent-friendly-modeling-api-design.md) | Implemented | 面向外部 Agent 的有界上下文、分页发现、派生几何查询与稀疏 patch 契约 |
| `ICE3-STUDIO-SPEC-010` | [Agent Iteration Impact Analysis](specs/2026-10-31-agent-iteration-impact-design.md) | Implemented | 局部修改影响范围、几何重建与缓存复用契约 |
| `ICE3-STUDIO-SPEC-011` | [Large Reference Asset Editing](specs/2026-10-31-large-reference-assets-design.md) | Implemented | 外部大型资产、轻量语义绑定、流式预览和稳定取景 |
| `ICE3-STUDIO-PLAN-014` | [Large Reference Asset Editing 实施计划](plans/2026-10-31-large-reference-assets.md) | Implemented | 引用资产存储、绑定编辑、流式 Viewer 与真实大模型验收 |
| `ICE3-STUDIO-PLAN-011` | [Revision-Scoped GPU Scene Session 实施计划](plans/2026-10-31-gpu-scene-session.md) | Experimental | CPU 增量场景、GPU residency、preview delta 和量化门禁 |
| `ICE3-STUDIO-PLAN-012` | [Agent-Friendly Modeling API 实施计划](plans/2026-10-31-agent-friendly-modeling-api.md) | Implemented | Agent manifest、上下文查询、派生场景发现和稀疏 patch 的 TDD 交付步骤 |
| `ICE3-STUDIO-PLAN-013` | [Agent Iteration Impact Analysis 实施计划](plans/2026-10-31-agent-iteration-impact.md) | Implemented | 局部修改影响、缓存证据、权限、Schema 和当前 Agent 验收 |
| `ICE3-STUDIO-PLAN-010` | [GPU Resident Hot-Path Redesign Implementation Plan](plans/2026-10-31-gpu-hot-path-redesign.md) | Experimental | 从 resident digest/handle 到批处理、证据和生产预览接线 |
| `ICE3-STUDIO-PLAN-009` | [GPU-Resident Geometry 实施计划](plans/2026-10-31-gpu-resident-geometry.md) | Experimental | Native residency、增量 packet、浏览器 buffer 复用和量化验收 |
| `ICE3-STUDIO-PLAN-008` | [Native GPU Compute Backend 实施计划](plans/2026-10-30-native-gpu-compute-backend.md) | Experimental | GPU 契约、设备生命周期、内核、调度、证据和 fail-closed 性能门禁 |
| `ICE3-STUDIO-SPEC-004` | [Toolchain Sidecar Integration 设计](specs/2026-10-30-studio-toolchain-sidecar-design.md) | Implemented | Studio 与 Toolchain 的版本化本地 Sidecar、能力协商和资产边界 |
| `ICE3-STUDIO-SPEC-005` | [Large-Scene Authoring 设计](specs/2026-10-30-large-scene-authoring-design.md) | Implemented | 参数化组件、实例、连接、GPU 求值、分区发布和大型场景验收 |
| `ICE3-STUDIO-PLAN-006` | [Large-Scene Authoring Foundation 实施计划](plans/2026-10-30-large-scene-authoring-foundation.md) | Implemented | M1 文档 schema、组件、实例、连接、CPU 求值和缓存 |
| `ICE3-STUDIO-PLAN-007` | [Large Industrial Scene Acceptance 实施计划](plans/2026-10-30-large-industrial-scene-acceptance.md) | Implemented | M2 大规模场景目录、布局、拓扑、公共 API 生成和性能验收 |
| `ICE3-STUDIO-PLAN-005` | [Toolchain Sidecar Integration 实施计划](plans/2026-10-30-studio-toolchain-sidecar.md) | Implemented | 共享契约、Sidecar、Rust 客户端、Server 集成和发布验收 |
| `ICE3-STUDIO-PLAN-002` | [ChangeSet 与审批 API 实施计划](plans/2026-10-30-studio-changeset-api.md) | Implemented | 候选变更集、持久化、双权限、预览和恢复 |
| `ICE3-STUDIO-PLAN-003` | [Studio Web Workbench 实施计划](plans/2026-10-30-studio-web-workbench-ui.md) | Implemented | React 工作台、只读画布、通用表单和 Agent 界面 |
| `ICE3-STUDIO-SPEC-001` | [AI-Native Modeling MVP 设计](specs/2026-10-30-ai-native-studio-mvp-design.md) | Implemented | Rust Core、API、数据模型、Viewer 和验收设计 |
| `ICE3-STUDIO-PLAN-001` | [AI-Native Modeling MVP 实施计划](plans/2026-10-30-ai-native-studio-mvp.md) | Implemented | Rust Core、API、Viewer、AI 验收和性能的实施步骤 |
| `ICE3-STUDIO-POL-001` | [仓库规则](../AGENTS.md) | Implemented | 架构、交付和命名边界 |

## 状态说明

- `Proposed`：设计待审阅，尚无对应实现。
- `Implemented`：实现、测试和用户入口均已交付。
- `Experimental`：实现和功能验收已交付，但量化发布门禁尚未通过；默认不得启用。

## 关联文档

- `ICE3-DOC-ARC-001` — ICE3 家族架构
- `ICE3-IND-RM-001` — Industrial Semantic Core
- `ICE3-TOOL-RM-001` — Toolchain
- `ICE3-ENG-RM-001` — Engine
- `ICE3-AGENT-RM-001` — Agent Console
