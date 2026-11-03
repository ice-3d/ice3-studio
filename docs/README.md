# ICE3 Studio 文档索引

> Document ID: ICE3-STUDIO-IDX-001
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

## 1. 从这里开始

| ID | 标题 | Status | 用途 |
|---|---|---|---|
| `ICE3-STUDIO-RM-001` | [Studio README](../README.md) | Implemented | 项目定位、当前状态和文档入口 |
| `ICE3-STUDIO-ARC-001` | [Studio 架构](architecture.md) | Implemented | 组件边界、真源、安全策略与尚未实现的能力 |
| `ICE3-STUDIO-IDX-002` | [架构决策索引](decisions/README.md) | Implemented | ADR 索引 |
| `ICE3-STUDIO-POL-001` | [仓库规则](../AGENTS.md) | Implemented | 架构、交付、界面和命名边界 |

## 2. 指南

| ID | 标题 | Status | 用途 |
|---|---|---|---|
| `ICE3-STUDIO-GUIDE-001` | [污水模块 AI 验收夹具](guides/wastewater-fixture.md) | Implemented | `wastewater-mini` 契约 fixture 的确定性 AI 建模计划 |
| `ICE3-STUDIO-GUIDE-002` | [大型工业夹具](guides/large-industrial-fixture.md) | Implemented | 程序化大型工业场景、共享几何、拓扑与性能夹具 |
| `ICE3-STUDIO-GUIDE-003` | [污水处理厂工程](../projects/README.md) | Implemented | 默认开发工程、总平面、工艺拓扑、统计与扩展方式 |
| `ICE3-STUDIO-GUIDE-004` | [本地入门](guides/getting-started.md) | Implemented | 本地启动、API、Source Import、Prepare、partition、导出和验证 |

## 3. 参考与决策

| ID | 标题 | Status | 用途 |
|---|---|---|---|
| `ICE3-STUDIO-REF-001` | [能力清单](reference/capabilities.md) | Implemented | 方法、CLI、诊断、环境变量和能力的代码镜像 |
| `ICE3-STUDIO-REF-002` | [API Reference](reference/api.md) | Implemented | JSON-RPC、Operation、revision 和导出契约 |
| `ICE3-STUDIO-REF-003` | [ChangeSet 审阅](reference/change-sets.md) | Implemented | 候选变更、digest 审批、权限和恢复 |
| `ICE3-STUDIO-REF-004` | [Diagnostics](reference/diagnostics.md) | Implemented | 稳定错误码和恢复规则 |
| `ICE3-STUDIO-REF-005` | [资产流水线](reference/asset-pipeline.md) | Implemented | Source Import 到 Prepared GLB、Tileset 和 Agent 交付的当前生产链 |
| `ICE3-STUDIO-ADR-001` | [Document 作为真源](decisions/0001-document-as-source-of-truth.md) | Implemented | Document 与派生 Geometry 的决策 |
| `ICE3-STUDIO-ADR-002` | [本地 API 边界](decisions/0002-local-api-boundary.md) | Implemented | 本地 API 与 Viewer 边界决策 |
| `ICE3-STUDIO-NOTICE-001` | [第三方声明](../THIRD_PARTY_NOTICES.md) | Active | Source Import、converter containment 与许可证披露 |

## 4. 设计规格

| ID | 标题 | Status | 用途 |
|---|---|---|---|
| `ICE3-STUDIO-SPEC-001` | [AI-Native Modeling MVP 设计](specs/2026-10-30-ai-native-studio-mvp-design.md) | Historical | MVP 已完成设计记录；当前能力见 REF-001 与 REF-005 |
| `ICE3-STUDIO-SPEC-002` | [Studio Web Workbench 设计](specs/2026-10-30-studio-web-workbench-design.md) | Historical | Agent-first 工作台的已完成设计记录 |
| `ICE3-STUDIO-SPEC-003` | [Native GPU Compute 设计](specs/2026-10-30-native-gpu-compute-design.md) | Historical | Native `wgpu` 实验能力已完成设计记录，默认仍为 CPU |
| `ICE3-STUDIO-SPEC-004` | [Toolchain Sidecar Integration 设计](specs/2026-10-30-studio-toolchain-sidecar-design.md) | Historical | 已被 Studio 原生 Asset Pipeline 取代的 Sidecar 设计 |
| `ICE3-STUDIO-SPEC-005` | [Large-Scene Authoring 设计](specs/2026-10-30-large-scene-authoring-design.md) | Historical | 大型场景 authoring 的已完成设计记录 |
| `ICE3-STUDIO-SPEC-006` | [GPU-Resident Geometry 设计](specs/2026-10-31-gpu-resident-geometry-design.md) | Historical | Resident geometry 的已完成设计记录，量化门禁未通过 |
| `ICE3-STUDIO-SPEC-007` | [GPU Hot-Path Redesign](specs/2026-10-31-gpu-hot-path-redesign-design.md) | Historical | Handle、digest、异步批处理和 GPU 证据的设计记录 |
| `ICE3-STUDIO-SPEC-008` | [Revision-Scoped GPU Scene Session](specs/2026-10-31-gpu-scene-session-design.md) | Historical | CPU 增量场景默认启用，GPU 路径保持实验性的设计记录 |
| `ICE3-STUDIO-SPEC-009` | [Agent-Friendly Modeling API](specs/2026-10-31-agent-friendly-modeling-api-design.md) | Historical | Agent 上下文、分页发现和稀疏 patch 的已完成设计记录 |
| `ICE3-STUDIO-SPEC-010` | [Agent Iteration Impact Analysis](specs/2026-10-31-agent-iteration-impact-design.md) | Historical | 局部影响、几何重建与缓存复用的已完成设计记录 |
| `ICE3-STUDIO-SPEC-011` | [Large Reference Asset Editing](specs/2026-10-31-large-reference-assets-design.md) | Historical | 外部大型资产、轻量绑定和流式预览的已完成设计记录 |
| `ICE3-STUDIO-SPEC-012` | [工程评审级污水处理厂资产](specs/2026-11-01-realistic-wastewater-plant-asset-design.md) | Historical | 默认污水厂资产的已完成设计记录 |
| `ICE3-STUDIO-SPEC-013` | [污水处理厂工艺拓扑与工程管线校正](specs/2026-11-01-wastewater-process-piping-correction-design.md) | Approved | 工艺拓扑、连续弯头、防扭转扫掠和功能配色；碰撞与支吊架校验仍待补 |
| `ICE3-STUDIO-SPEC-014` | [Agent 主导的 Studio 工作台](specs/2026-11-01-agent-led-workbench-design.md) | Implemented | Agent 默认入口、工作上下文、实体 Selection 和人工接手 |
| `ICE3-STUDIO-SPEC-015` | [Studio Runtime Tileset 导出](specs/2026-11-01-studio-runtime-tileset-export-design.md) | Deprecated | 由 Studio 原生 Asset Pipeline 取代的 Sidecar 方案 |
| `ICE3-STUDIO-SPEC-016` | [Studio 原生 Asset Pipeline 迁移](specs/2026-11-01-native-asset-pipeline-migration-design.md) | Implemented | Source Import、Asset、Partition、LOD 和 Tileset 已全部迁入 Studio |
| `ICE3-STUDIO-SPEC-017` | [Studio 原生 STEP 与 IFC Source Import](specs/2026-11-02-native-step-ifc-import-design.md) | Implemented | STEP、IFC、parity 与 Toolchain adapter 退役 |
| `ICE3-STUDIO-SPEC-018` | [Studio 原生 JT Import 与 Toolchain 最终退役](specs/2026-11-02-native-jt-toolchain-retirement-design.md) | Implemented | JT converter bridge、原生 preview 和 Toolchain repository 退役 |
| `ICE3-STUDIO-SPEC-019` | [Studio Asset Prepare 与 Toolchain 功能等价](specs/2026-11-02-studio-asset-prepare-parity-design.md) | Historical | Phase 6A/6B/6C 已完成设计记录；当前链路见 REF-005 |
| `ICE3-STUDIO-SPEC-020` | [Agent 组件定义内省](specs/2026-11-03-agent-component-introspection-design.md) | Implemented | `component.get` 有界读：参数 schema、端口清单与部件摘要 |
| `ICE3-STUDIO-SPEC-021` | [场景空间重叠审计](specs/2026-11-03-scene-overlap-audit-design.md) | Implemented | `validation.run` 可选全局重叠 warning：连接/薄板/嵌套排除，重合上报 |
| `ICE3-STUDIO-SPEC-022` | [Agent 视觉自检截图通道](specs/2026-11-03-agent-screenshot-design.md) | Implemented | 工具层 Chrome 截图：任意机位、自起 server、非 RPC 形态决策 |

## 5. 实施计划

| ID | 标题 | Status | 用途 |
|---|---|---|---|
| `ICE3-STUDIO-PLAN-001` | [AI-Native Modeling MVP 实施计划](plans/2026-10-30-ai-native-studio-mvp.md) | Historical | Rust Core、API、Viewer 和 AI 验收的历史交付记录 |
| `ICE3-STUDIO-PLAN-002` | [ChangeSet 与审批 API 实施计划](plans/2026-10-30-studio-changeset-api.md) | Historical | 候选变更集、持久化、双权限和恢复的历史记录 |
| `ICE3-STUDIO-PLAN-003` | [Studio Web Workbench 实施计划](plans/2026-10-30-studio-web-workbench-ui.md) | Historical | React 工作台、只读画布和 Agent 界面的历史记录 |
| `ICE3-STUDIO-PLAN-005` | [Toolchain Sidecar 实施计划](plans/2026-10-30-studio-toolchain-sidecar.md) | Historical | 已退役 Sidecar 的历史实施记录 |
| `ICE3-STUDIO-PLAN-006` | [Large-Scene Authoring Foundation 实施计划](plans/2026-10-30-large-scene-authoring-foundation.md) | Historical | Schema、组件、实例、连接和 CPU 求值的历史记录 |
| `ICE3-STUDIO-PLAN-007` | [Large Industrial Scene Acceptance 实施计划](plans/2026-10-30-large-industrial-scene-acceptance.md) | Historical | 大规模场景、拓扑和性能验收的历史记录 |
| `ICE3-STUDIO-PLAN-008` | [Native GPU Compute Backend 实施计划](plans/2026-10-30-native-gpu-compute-backend.md) | Historical | GPU 契约、生命周期、内核和证据的历史记录 |
| `ICE3-STUDIO-PLAN-009` | [GPU-Resident Geometry 实施计划](plans/2026-10-31-gpu-resident-geometry.md) | Historical | Residency、增量 packet 和量化验收的历史记录 |
| `ICE3-STUDIO-PLAN-010` | [GPU Resident Hot-Path Redesign 实施计划](plans/2026-10-31-gpu-hot-path-redesign.md) | Historical | Resident digest、handle、批处理和生产预览的历史记录 |
| `ICE3-STUDIO-PLAN-011` | [Revision-Scoped GPU Scene Session 实施计划](plans/2026-10-31-gpu-scene-session.md) | Historical | CPU 增量、GPU residency 和 preview delta 的历史记录 |
| `ICE3-STUDIO-PLAN-012` | [Agent-Friendly Modeling API 实施计划](plans/2026-10-31-agent-friendly-modeling-api.md) | Historical | Agent manifest、上下文查询和稀疏 patch 的历史记录 |
| `ICE3-STUDIO-PLAN-013` | [Agent Iteration Impact Analysis 实施计划](plans/2026-10-31-agent-iteration-impact.md) | Historical | 局部影响、缓存证据和权限的历史记录 |
| `ICE3-STUDIO-PLAN-014` | [Large Reference Asset Editing 实施计划](plans/2026-10-31-large-reference-assets.md) | Historical | 引用资产存储、绑定编辑和流式 Viewer 的历史记录 |
| `ICE3-STUDIO-PLAN-015` | [工程评审级污水处理厂资产实施计划](plans/2026-11-01-realistic-wastewater-plant-asset.md) | Historical | 工程拆分、细节组件、管线支撑和材质验收的历史记录 |
| `ICE3-STUDIO-PLAN-016` | [污水处理厂工艺拓扑校正实施计划](plans/2026-11-01-wastewater-process-topology-correction.md) | Historical | P&ID 端口、污泥和沼气链路的历史记录 |
| `ICE3-STUDIO-PLAN-017` | [工程管线圆滑路径与功能配色实施计划](plans/2026-11-01-engineered-pipe-routing-color.md) | Historical | 连续弯头、扫掠防扭转和功能配色的历史记录 |
| `ICE3-STUDIO-PLAN-018` | [Agent 主导工作台实施计划](plans/2026-11-01-agent-led-workbench.md) | Implemented | Agent 默认 tab、上下文、实体选择、人工接手和响应式验收 |
| `ICE3-STUDIO-PLAN-019` | [Studio 原生 Asset Pipeline Phase 1](plans/2026-11-01-native-asset-pipeline-phase-1.md) | Implemented | Rust GLB、partition、LOD、Tileset、binding manifest 和 CLI 导出 |
| `ICE3-STUDIO-PLAN-020` | [Studio 原生 Asset Pipeline Phase 2](plans/2026-11-01-native-asset-pipeline-phase-2.md) | Implemented | glTF/GLB、OBJ/MTL Source Import 和 OBJ Toolchain 退役 |
| `ICE3-STUDIO-PLAN-021` | [Studio 原生 Asset Pipeline Phase 3](plans/2026-11-02-native-asset-pipeline-phase-3.md) | Historical | FBX/USD importer、隔离 worker 和 bridge 退役的历史记录 |
| `ICE3-STUDIO-PLAN-022` | [Studio 原生 STEP Source Import](plans/2026-11-02-native-step-import.md) | Historical | AP242 hierarchy、instances、worker 和 parity 的历史记录 |
| `ICE3-STUDIO-PLAN-023` | [Studio 原生 IFC Source Import](plans/2026-11-02-native-ifc-import.md) | Historical | IFC4X3 metadata、geometry、worker 和 parity 的历史记录 |
| `ICE3-STUDIO-PLAN-024` | [Studio 原生 JT Source Import](plans/2026-11-02-native-jt-import.md) | Implemented | Converter bridge、GLB canonicalization、worker 隔离和 JT parity |
| `ICE3-STUDIO-PLAN-025` | [Studio Sidecar 最终退役](plans/2026-11-02-studio-sidecar-retirement.md) | Implemented | Phase 5B 原生 preview、client/status/UI/build 清理 |
| `ICE3-STUDIO-PLAN-026` | [Toolchain Repository 最终退役](plans/2026-11-02-toolchain-repository-retirement.md) | Implemented | JT、Sidecar、release 清理、文档收口和归档候选 |
| `ICE3-STUDIO-PLAN-027` | [Studio Asset Prepare Core](plans/2026-11-02-studio-prepare-core.md) | Implemented | Phase 6A 共享 Prepare、edges、budget/report、build 和 inspect |
| `ICE3-STUDIO-PLAN-028` | [Studio Partition 与 Runtime Tileset 功能等价](plans/2026-11-02-studio-partition-runtime-parity.md) | Implemented | Phase 6B 统一 AssetProfile、direct partition 和 parity |
| `ICE3-STUDIO-PLAN-029` | [Studio Prepare Engine 集成与最终验收](plans/2026-11-02-studio-prepare-engine-finalization.md) | Implemented | Phase 6C Engine 消费、性能、文档和最终门禁 |

## 6. 状态说明

- `Approved`：设计已批准，仍存在明确未交付项。
- `Implemented`：能力、测试和用户入口已交付；实验能力会在正文标记。
- `Historical`：保留已完成或已退役实施过程的审计记录，不代表当前生产依赖。
- `Deprecated`：已由当前方案替代，仅供迁移追溯。

## 7. 关联文档

- `ICE3-DOC-ARC-001` — [ICE3 家族架构](https://github.com/ice-3d/ice3-doc/blob/main/docs/architecture.md)
- `ICE3-DOC-CTR-001` — [Asset Preparation Contract v1](https://github.com/ice-3d/ice3-doc/blob/main/docs/contracts/asset-preparation-contract-v1.md)
- `ICE3-ENG-RM-001` — [Engine](https://github.com/ice-3d/ice3-engine/blob/main/README.md)
- `ICE3-AGENT-RM-001` — [Agent Console](https://github.com/ice-3d/ice3-agent-console/blob/main/README.md)
- `ICE3-IND-RM-001` — [Industrial Semantic Core](https://github.com/ice-3d/ice3-industrial/blob/main/README.md)
