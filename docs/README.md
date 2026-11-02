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
| `ICE3-STUDIO-GUIDE-004` | [Getting Started](guides/getting-started.md) | Implemented | 本地启动、API 工作流、验证和性能基线 |
| `ICE3-STUDIO-GUIDE-001` | [污水模块 AI 验收夹具](guides/wastewater-fixture.md) | Implemented | `wastewater-mini` 契约 fixture 的确定性 AI 建模计划 |
| `ICE3-STUDIO-GUIDE-002` | [Large Industrial Fixture](guides/large-industrial-fixture.md) | Implemented | 程序化大型工业场景、共享几何、拓扑与性能 fixture |
| `ICE3-STUDIO-GUIDE-003` | [污水处理厂工程](../projects/README.md) | Implemented | 默认开发工程、总平面、工艺拓扑、统计与扩展方式 |
| `ICE3-STUDIO-ARC-001` | [Architecture](architecture.md) | Implemented | 组件边界、真源、安全策略与尚未实现的能力清单 |
| `ICE3-STUDIO-REF-001` | [能力清单](reference/capabilities.md) | Implemented | 37 个固化方法 + 2 个实验方法、29 种 Operation、7 个 ChangeSet 状态、诊断码、CLI 与环境变量的代码镜像 |
| `ICE3-STUDIO-REF-002` | [API Reference](reference/api.md) | Implemented | JSON-RPC、Operation、revision 和导出契约 |
| `ICE3-STUDIO-REF-003` | [ChangeSet Review](reference/change-sets.md) | Implemented | 候选变更、digest 审批、权限和恢复 |
| `ICE3-STUDIO-REF-004` | [Diagnostics](reference/diagnostics.md) | Implemented | 稳定错误码和恢复规则 |
| `ICE3-STUDIO-REF-005` | [Toolchain Sidecar](reference/toolchain-sidecar.md) | Implemented | 固定版本 Sidecar、能力协商、缓存与故障降级 |
| `ICE3-STUDIO-ADR-001` | [Document as Source of Truth](decisions/0001-document-as-source-of-truth.md) | Implemented | Document 与派生 Geometry 的决策 |
| `ICE3-STUDIO-ADR-002` | [Local API Boundary](decisions/0002-local-api-boundary.md) | Implemented | 本地 API 与 Viewer 边界决策 |
| `ICE3-STUDIO-SPEC-002` | [Studio Web Workbench 设计](specs/2026-10-30-studio-web-workbench-design.md) | Implemented | AI-native 审查、表单编辑、ChangeSet 和通用工作台设计 |
| `ICE3-STUDIO-SPEC-003` | [Native GPU Compute 设计](specs/2026-10-30-native-gpu-compute-design.md) | Implemented | Native `wgpu` 功能已交付；性能未达默认启用门槛，主机保持 CPU |
| `ICE3-STUDIO-SPEC-006` | [GPU-Resident Geometry 设计](specs/2026-10-31-gpu-resident-geometry-design.md) | Implemented | 功能已交付，但 resident 性能门禁未通过；CPU 与 WebGL2 仍为默认 |
| `ICE3-STUDIO-SPEC-007` | [GPU Resident Hot-Path Redesign](specs/2026-10-31-gpu-hot-path-redesign-design.md) | Implemented | 稳定 handle、一次性 digest、异步批处理和分阶段 GPU 证据 |
| `ICE3-STUDIO-SPEC-008` | [Revision-Scoped GPU Scene Session](specs/2026-10-31-gpu-scene-session-design.md) | Implemented | CPU 增量场景默认启用；GPU 增量无额外性能优势，保持实验开关 |
| `ICE3-STUDIO-SPEC-009` | [Agent-Friendly Modeling API](specs/2026-10-31-agent-friendly-modeling-api-design.md) | Implemented | 面向外部 Agent 的有界上下文、分页发现、派生几何查询与稀疏 patch 契约 |
| `ICE3-STUDIO-SPEC-010` | [Agent Iteration Impact Analysis](specs/2026-10-31-agent-iteration-impact-design.md) | Implemented | 局部修改影响范围、几何重建与缓存复用契约 |
| `ICE3-STUDIO-SPEC-011` | [Large Reference Asset Editing](specs/2026-10-31-large-reference-assets-design.md) | Implemented | 外部大型资产、轻量语义绑定、流式预览和稳定取景 |
| `ICE3-STUDIO-SPEC-012` | [工程评审级污水处理厂资产](specs/2026-11-01-realistic-wastewater-plant-asset-design.md) | Implemented | 原创通用厂区、真实构筑物、管线、材质、场地与视觉验收 |
| `ICE3-STUDIO-SPEC-013` | [污水处理厂工艺拓扑与工程管线校正](specs/2026-11-01-wastewater-process-piping-correction-design.md) | Approved | 工艺拓扑、连续弯头、防扭转扫掠和功能配色已交付；碰撞与支吊架校验仍待补 |
| `ICE3-STUDIO-SPEC-014` | [Agent 主导的 Studio 工作台](specs/2026-11-01-agent-led-workbench-design.md) | Implemented | Agent 默认入口、工作上下文、实体 Selection 和人工接手 |
| `ICE3-STUDIO-SPEC-015` | [Studio Runtime Tileset 导出](specs/2026-11-01-studio-runtime-tileset-export-design.md) | Deprecated | 由 Studio 原生 Asset Pipeline 迁移取代的 Sidecar 方案 |
| `ICE3-STUDIO-SPEC-016` | [Studio 原生 Asset Pipeline 迁移](specs/2026-11-01-native-asset-pipeline-migration-design.md) | Approved（Phase 1-3 Implemented） | 分阶段把 Source Import、Asset、Partition、LOD 和 Tileset 迁入 Studio |
| `ICE3-STUDIO-PLAN-015` | [工程评审级污水处理厂资产实施计划](plans/2026-11-01-realistic-wastewater-plant-asset.md) | Implemented | 工程化拆分、拓扑校验、细节组件、管线支撑、材质与 Chrome 验收 |
| `ICE3-STUDIO-PLAN-016` | [污水处理厂工艺拓扑校正实施计划](plans/2026-11-01-wastewater-process-topology-correction.md) | Implemented | P&ID 端口、RAS/初沉污泥/沼气链路、分流节点、泥饼输送和 API 重建验收 |
| `ICE3-STUDIO-PLAN-017` | [工程管线圆滑路径与功能配色实施计划](plans/2026-11-01-engineered-pipe-routing-color.md) | Implemented | 连续弯头、扫掠防扭转、亮银与功能配色和 Chrome 验收 |
| `ICE3-STUDIO-PLAN-018` | [Agent 主导工作台实施计划](plans/2026-11-01-agent-led-workbench.md) | Implemented | Agent 默认 tab、上下文、对象搜索、实体选择、人工接手和响应式验收 |
| `ICE3-STUDIO-PLAN-019` | [Studio 原生 Asset Pipeline Phase 1](plans/2026-11-01-native-asset-pipeline-phase-1.md) | Implemented | Rust GLB、partition、LOD、Tileset、binding manifest 和 CLI 导出 |
| `ICE3-STUDIO-PLAN-020` | [Studio 原生 Asset Pipeline Phase 2](plans/2026-11-01-native-asset-pipeline-phase-2.md) | Implemented | glTF/GLB 与 OBJ/MTL 原生 Source Import、CLI validate 和 OBJ Toolchain 退役 |
| `ICE3-STUDIO-PLAN-021` | [Studio 原生 Asset Pipeline Phase 3](plans/2026-11-02-native-asset-pipeline-phase-3.md) | Implemented | FBX/USD 原生 importer、隔离 worker、资源限制和 bridge 退役 |
| `ICE3-STUDIO-PLAN-014` | [Large Reference Asset Editing 实施计划](plans/2026-10-31-large-reference-assets.md) | Implemented | 引用资产存储、绑定编辑、流式 Viewer 与真实大模型验收 |
| `ICE3-STUDIO-PLAN-011` | [Revision-Scoped GPU Scene Session 实施计划](plans/2026-10-31-gpu-scene-session.md) | Implemented | CPU 增量场景、GPU residency、preview delta 和量化门禁 |
| `ICE3-STUDIO-PLAN-012` | [Agent-Friendly Modeling API 实施计划](plans/2026-10-31-agent-friendly-modeling-api.md) | Implemented | Agent manifest、上下文查询、派生场景发现和稀疏 patch 的 TDD 交付步骤 |
| `ICE3-STUDIO-PLAN-013` | [Agent Iteration Impact Analysis 实施计划](plans/2026-10-31-agent-iteration-impact.md) | Implemented | 局部修改影响、缓存证据、权限、Schema 和当前 Agent 验收 |
| `ICE3-STUDIO-PLAN-010` | [GPU Resident Hot-Path Redesign Implementation Plan](plans/2026-10-31-gpu-hot-path-redesign.md) | Implemented | 从 resident digest/handle 到批处理、证据和生产预览接线 |
| `ICE3-STUDIO-PLAN-009` | [GPU-Resident Geometry 实施计划](plans/2026-10-31-gpu-resident-geometry.md) | Implemented | Native residency、增量 packet、浏览器 buffer 复用和量化验收 |
| `ICE3-STUDIO-PLAN-008` | [Native GPU Compute Backend 实施计划](plans/2026-10-30-native-gpu-compute-backend.md) | Implemented | GPU 契约、设备生命周期、内核、调度、证据和 fail-closed 性能门禁 |
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

- `Draft` / `Approved`：设计已记录，尚未由代码完整交付。
- `Implemented`：实现、测试和用户入口均已交付。
- 实现已交付但量化发布门禁未通过的能力仍使用 `Implemented`，并在描述中明确标记
  “实验性”或“默认关闭”。

## 关联文档

- `ICE3-DOC-ARC-001` — ICE3 家族架构
- `ICE3-IND-RM-001` — Industrial Semantic Core
- `ICE3-TOOL-RM-001` — Toolchain
- `ICE3-ENG-RM-001` — Engine
- `ICE3-AGENT-RM-001` — Agent Console
