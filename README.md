# ICE3 Studio

> Document ID: ICE3-STUDIO-RM-001
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

ICE3 Studio 是一款本地优先的 AI Native 工业 3D 建模工具。建模的主要入口是 AI 意图，
不是手工工具栏：用户描述要建什么、要改什么，AI 把它转成受类型约束的建模操作，Rust
建模内核把它变成确定性的文档修订，人在自己的界面里审阅、微调、批准。

Studio 自带几何内核，不把建模委托给任何其它建模应用；不绑定模型供应商，不内置凭据、
Prompt 或 Agent 策略。

## 人机分工

| 角色 | 负责 | 不负责 |
|---|---|---|
| 用户 | 表达意图、审阅候选变更、微调参数、批准提交 | 手工摆放每个构件、对齐几何 |
| 外部 AI | 理解自然语言、查询场景、提交操作提案 | 直接修改几何、跳过校验、自行提交 |
| Studio 内核 | 校验、求值、diff、持久化、导出、缓存复用 | 理解语言、决定改什么 |

一句话：**AI 承担建模动作，人保留决定权。** 这条分工由 API 权限强制，不靠约定。

## 建模能力

内核以通用原语和参数化组合为底座，行业语义通过 metadata、relation 和 assembly 表达，
不写进 Core 的类型系统。

| 能力 | 内容 |
|---|---|
| 几何 | 参数化 primitive、sweep、稳定顶点序的三角网格求值 |
| 组件 | component definition、typed parameter、part override、实例化 |
| 装配 | 有向/无向 typed relation、port 感知连接、工艺线成链校验 |
| 材质 | material 定义、指派、稀疏 patch |
| 布局 | instance set、确定性实例展开、批量变换 |
| 事务 | 原子事务、幂等键、revision 冲突检测、undo/redo |
| 输出 | 标准 GLB 导出、Runtime preview |
| 大场景 | 共享几何复用、内容寻址几何缓存、增量求值、外部引用资产 |

变更通过 27 种 operation 表达（`node.*`、`component.*`、`instance.*`、`instanceSet.*`、
`connection.*`、`material.*`、`relation.*`、`constraint.*`）。全量清单见
[能力清单](docs/reference/capabilities.md)。

## 工作流

```
用户意图 → Agent 查询/提案 → ChangeSet（候选 digest）
        → 人在 Viewer 审阅 + 表单微调 → 人工令牌批准 → 新 revision
```

默认 `review-required` 模式：Agent 可以查询、提案、求值，**不能直接应用或提交**。人工
审批令牌负责最后一步。同一份候选由 AI 提案和人工表单共用，提交前校验 digest 未被篡改。
ChangeSet 有 7 个状态（draft / evaluating / ready / blocked / stale / committed /
discarded），只有 `ready` 且 digest 一致才可提交。

## 界面

Web Viewer 是工具的界面层，只消费公开 API，没有私有修改路径：

- 对象树（source 与 derived 节点）
- 3D 视口（WebGL2 默认，WebGPU 显式实验开启）
- 属性表单（typed parameter 编译成 `instance.update`）
- Agent 面板（流式对话、工具调用、失败与取消）
- 变更审阅（候选 diff、诊断、批准）
- 诊断与 revision 事件

## 当前场景与测试资产

| 场景 | 规模 | 用于 |
|---|---|---|
| 污水处理厂工程 | 681 个源节点、166 个实例、63 条连接、355,900 有效三角形 | 默认开发、近景审查与 Studio 建模能力验证 |
| 程序化大型工业 fixture | 208 个主对象、560 条操作、6 条工艺线、938,964 有效三角形、99 条连接 | 通用大场景契约测试与性能门禁 |
| `wastewater-mini` | 11 个源节点 | 启动、API 和端到端契约测试 |

Studio 启动时扫描 `projects/*/project.json`，默认打开修改时间最新的工程；时间相同时
选择 `currentRevision` 更高的工程。

## 快速开始

```bash
npm --prefix apps/viewer ci
npm run dev
```

`npm run dev` 自动选择 `projects/` 中的最新工程，当前为 `wastewater-plant`。命令打印
Viewer URL 并持续服务，`Ctrl+C` 停止。

完整验证与基准：

```bash
npm run verify      # 18 道门禁：fmt / clippy / test / schema / fixture / viewer / E2E / audit
npm run benchmark
npm run benchmark:gpu
```

`wastewater-mini`（16 KB JSON，11 个节点）是契约测试 fixture，被 `npm run verify`、
E2E 和 MVP benchmark 依赖，可用 `npm run dev:mini` 单独启动。

端到端流程见 [Getting Started](docs/guides/getting-started.md)。

## 工程文件

工程是一个目录，语义与几何分离：Document、变更集、操作历史通常只有几百 KB，引用资产
在 `references/` 下可达数百 MB。撤销深度上限 200，超出后最早的快照被回收。

可打包成单文件 `.ice3`（未压缩 tar，`tar -tf` 可直接查看）：

```bash
cargo run -p studio-cli -- pack --project ./projects/wastewater-plant --output wwtp.ice3
cargo run -p studio-cli -- unpack wwtp.ice3 --into ./restored
```

容器默认排除 `references/` 与 `cache/`，通常只有几百 KB，并附带
`resources/summary.json` 与按区域着色的 `resources/layout.svg` 作为工程概览。
详见 [能力清单](docs/reference/capabilities.md)。

## 架构

```
外部 AI 或脚本
      │  JSON-RPC 2.0（loopback + 写入令牌）
      ▼
Studio Server ── 认证、单事务串行、持久化、revision 事件
      │
      ├── studio-core      Document、几何、事务、校验、GLB 导出（无 IO）
      ├── studio-project   路径、写锁、原子快照、变更集、几何缓存
      ├── studio-gpu       wgpu 几何内核、residency（实验）
      └── studio-toolchain-client → 固定版本 Toolchain Sidecar
      │
      └── revision 事件 ──→ Web Viewer（薄客户端）
```

Document 加操作历史是唯一真源，几何、bounds、诊断、GLB、预览全部是可从文档重算的派生值。
详见 [Architecture](docs/architecture.md) 与两份架构决策（
[0001](docs/decisions/0001-document-as-source-of-truth.md)、
[0002](docs/decisions/0002-local-api-boundary.md)）。

## API

39 个 JSON-RPC 方法，分 9 个域（`system`、`project`、`document`、`scene`、`validation`、
`evaluation`、`export`、`changeset`、`referenceAsset`）。完整方法表、参数与返回见
[API Reference](docs/reference/api.md)，错误码见 [Diagnostics](docs/reference/diagnostics.md)。

面向 Agent 的读接口：

- `system.agentManifest` — 可用工具、当前令牌权限、调用上限
- `scene.context` — 固定大小项目摘要（大场景实测 860 字节）
- `scene.search` — source/derived 节点分页，revision-bound cursor，支持字段投影
- `evaluation.query` — 只对显式列出的最多 32 个派生节点返回摘要或按需几何负载

稀疏 patch（`instance.patch`、`component.patch`、`connection.patch`、`material.patch`）
只修改指定字段，但仍走完整校验、diff、幂等和 revision 冲突检查。

Studio 不内置模型、Prompt、网关或 Agent 策略；自然语言理解留在外部。

## 性能基线

`npm run benchmark` 在目标 Apple Silicon Mac 上运行 release build，输出
`output/studio-mvp-benchmark.json`。任一 p95 超限即失败。

| 指标 | p95 |
|---|---:|
| Server 冷启动到 health | 461.68 ms |
| 重开 100 Node 项目 | 121.53 ms |
| 100 Operations 原子事务 | 23.96 ms |
| 100 Operations ChangeSet 求值 | 15.31 ms |
| `wastewater-mini` 求值 | 37.51 ms |
| source GLB 导出 | 1.04 ms |
| Toolchain Runtime preview | 10.21 ms |
| ChangeSet candidate preview | 17.11 ms |

**大场景增量是主要收益来源**：CPU 增量场景缓存默认开启，transform-only revision 从
约 1.285 s 降到约 116 ms（11.1×）。同一场景上 GPU 增量路径约 126 ms，约为 CPU 增量的
0.92×，没有优势，因此 native GPU 保持实验性和默认关闭。

GPU 数据：500k 三角形 CPU p50 3.84 ms / cold GPU 10.74 ms；2M 三角形 11.36 ms /
39.24 ms。resident handle 命中 0.00 ms 是不含哈希、上传和 fence 的缓存命中，不代表
kernel 更快。只有窄视锥可见性查询 GPU 明显领先：同一组 243 个 node ID，CPU p50/p95
2.613/3.616 ms，GPU 0.601/2.276 ms（p50 4.35×），该接口同样保持实验状态。

性能门禁失败关闭，`auto` 继续选 CPU。6.1M 有效三角形的常驻资源约占 46 MiB GPU 内存。

## 边界与缺口

已实现但实验性的能力：native GPU compute、resident geometry、preview packet、GPU scene
session。功能已交付，但性能门禁未过，默认关闭。

**产品范围内尚未实现**（待补齐的缺口，不是范围外决定）：布尔几何（开孔、管道穿插的并
交差）、自由曲线与曲线驱动 sweep、雕刻与有机网格编辑、协作编辑、云租户与远程部署、大
静态资产导入、跨项目 GPU 缓冲持久化。清单与理由见
[Architecture](docs/architecture.md) 的 Not Yet Implemented。

不属于本仓库职责：Runtime LOD 与空间分区由 Toolchain 负责。

本地 API 只监听 loopback，写入需要进程级令牌，项目路径限制在配置的 workspace 内。
这套边界是单用户本地工具的边界，**不是**多租户授权模型；远程部署需要独立的安全与租户设计。

## 文档

- [文档索引](docs/README.md)
- [Architecture](docs/architecture.md)
- [能力清单](docs/reference/capabilities.md)
- [API Reference](docs/reference/api.md)
- [Diagnostics](docs/reference/diagnostics.md)
- [ChangeSet 审阅](docs/reference/change-sets.md)
- [Toolchain Sidecar](docs/reference/toolchain-sidecar.md)

## AI 辅助开发

本项目在设计和实现过程中大量使用了 AI 辅助开发。

## 许可证

本项目采用 [MIT License](LICENSE) 许可。
