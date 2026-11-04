# ICE3 Studio

> Document ID: ICE3-STUDIO-RM-001
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

**本地优先的 AI Native 工业 3D 建模工具。** 建模入口是意图而不是工具栏：你描述要建什么、
改什么，AI 把它翻译成受类型约束的建模操作，Rust 内核把它变成确定性的文档修订，你在自己
的界面里审阅、微调、批准。

Studio 自带几何内核，不把建模委托给任何外部建模软件；也不绑定模型供应商，不内置凭据、
Prompt 或 Agent 策略。

```
意图 → Agent 查询/提案 → ChangeSet → 人审阅 + 表单微调 → 人工令牌批准 → 新 revision
```

## 1. 快速开始

需要 Node.js 24.x、npm 11+、Rust 1.9x，macOS 或 Linux。

```bash
git clone https://github.com/ice-3d/ice3-studio.git
cd ice3-studio
npm --prefix apps/viewer ci
npm run dev          # 自动选 projects/ 里最新的工程，打印 Viewer URL
```

`Ctrl+C` 停止。只想跑契约测试用的小工程：`npm run dev:mini`。

提交前跑全量门禁（32 道，含 fmt / clippy / 测试 / Engine+prepare parity / schema /
文档契约 / fixture / viewer / E2E / 依赖审计）：

```bash
npm run verify
```

## 2. 人机分工

| 角色 | 负责 | 不负责 |
|---|---|---|
| 你 | 表达意图、审阅候选变更、微调参数、批准提交 | 手工摆每个构件、对齐几何 |
| 外部 AI | 理解语言、查询场景、提交操作提案 | 直接改几何、跳过校验、自行提交 |
| Studio 内核 | 校验、求值、diff、持久化、导出、缓存复用 | 理解语言、决定改什么 |

**AI 承担建模动作，人保留决定权。** 这条由 API 权限强制，不靠约定：默认
`review-required` 下 Agent 能查询、提案、求值，但不能应用或提交；ChangeSet 有 7 个状态，
只有 `ready` 且 digest 未被篡改才可提交。

## 3. 建模能力

内核以通用原语和参数化组合为底座，行业语义通过 metadata、relation、assembly 表达，
不写进 Core 的类型系统。

| 能力 | 内容 |
|---|---|
| 几何 | 参数化 primitive、sweep、稳定顶点序的三角网格求值、圆角盒（12 边倒角/圆角） |
| 组件 | component definition、typed parameter、part override、实例化 |
| 装配 | 有向/无向 typed relation、port 感知连接、工艺线成链校验 |
| 材质 | metallic / roughness / emissive / uvScale、**底色贴图、法线贴图、ORM 打包图**（R 遮蔽 / G 粗糙度 / B 金属度）、alpha blend、directional/flat/water shading |
| 布局 | instance set、确定性实例展开、批量变换 |
| 事务 | 原子事务、幂等键、revision 冲突检测、undo/redo（深度 200） |
| 输出 | 标准 GLB、Runtime preview、原生 partition/LOD Tileset bundle、BOM/STEP/STL/OBJ/IFC/DXF |
| 大场景 | 共享几何复用、内容寻址几何缓存、增量求值、外部引用资产 |

变更通过受类型的 operation 表达（`node.*`、`primitive.update`、`mesh.replace`、
`component.*`、`instance.*`、`instanceSet.*`、`connection.*`、`material.*`、
`relation.*`、`constraint.*`）。**完整清单以
[能力清单](docs/reference/capabilities.md) 与
[API Reference](docs/reference/api.md) 为准**——那两份由代码生成，本文不复述数字。

## 4. 界面

Web Viewer 是界面层，只消费公开 API，没有私有修改路径。工作台以 Agent 为先，同时保留
Objects / 3D Scene / Review dock：

| 区域 | 作用 |
|---|---|
| Objects | 对象搜索、层级过滤、source/derived identity、完整实体 Selection |
| 3D Scene | WebGL2 默认、WebGPU 实验性显式开启；渲染当前 revision 或 candidate |
| Agent | 默认 tab：网关状态、工程、revision、焦点对象、计划与活动 |
| Inspect | 人工参数检查与 `instance.update` 编译，可被 Agent 显式接手 |
| Changes | 候选 diff、诊断、审阅与人工批准 |
| Evidence | Operations、Timeline、Diagnostics |

网关离线时界面如实显示离线，并且**不阻塞**对象搜索、参数检查、ChangeSet 审阅或人工提交。

## 5. CLI

```bash
cargo run -p studio-cli -- <command>
```

| 命令 | 用途 |
|---|---|
| `import` | 源资产 → Prepared GLB（`--adapter gltf\|obj\|fbx\|usd\|step\|ifc\|jt`） |
| `build` | 源资产 → 带 render edges 的 Prepared GLB（`--report` 出 build report） |
| `inspect` | 只读 metrics / preparation / budget，不写文件 |
| `validate` | 校验 GLB / glTF / Prepared GLB |
| `partition` | 任意已注册源 → Runtime Tileset 目录 |
| `export` | 已提交 revision → Tileset / STEP / STL / OBJ / IFC / DXF / BOM CSV |
| `schema` | 重新生成 `schemas/`（`--check` 用于门禁） |
| `pack` / `unpack` | 工程目录 ↔ 单文件 `.ice3` 容器 |
| `serve` | 启动本地服务（Viewer + JSON-RPC） |

**导出演示资产**（Agent Console 依赖这一步）：

```bash
cargo run -p studio-cli -- export tileset \
  --project ./projects/wastewater-plant \
  --output /tmp/ice3-runtime-tileset \
  --profile agent-console-v1 \
  --asset-id ice3-virtual-wwtp --force
```

`--asset-id` 决定 `bindings.json` 里 `selectionId` 的命名空间，**必须**与消费方的 demo
asset id 一致（演示资产为 `ice3-virtual-wwtp`），否则设备剧本的 `focus_equipment` 命中
不了 Selection。

原生支持的源格式：glTF/GLB、OBJ/MTL、FBX（ASCII/Binary）、USD（ASCII/Binary）、
STEP AP242、IFC4X3、JT。前六种由 Studio 自带 worker 解析，无需外部 DCC；JT 是唯一例外，
由部署方通过 `ICE3_JT_CONVERTER` 提供组织批准的 converter，Studio 固定调用参数并校验
输出。worker 有硬超时、进程树清理、崩溃隔离、512 MiB response / 1 MiB stderr 上限，
默认预算 1,000,000 nodes、20,000,000 triangles、512 MiB decoded memory、120 秒。

`import` 生成 `ice3-prepared` profile、真实 metrics 和 `interactive-desktop-v1` budget，
默认不生成 render edges；`build` 默认以 30° crease angle 生成 `mode=1`、
`role="edges"`、`widthPx=3` 的 line primitive。两者发布前都做校验，失败不覆盖已有文件。

## 6. 工程与场景

工程是一个目录：Document、变更集、操作历史通常几百 KB，`references/` 下的引用资产可达
数百 MB。可以打包成单文件 `.ice3`（未压缩 tar，`tar -tf` 可直读），默认排除
`references/` 与 `cache/`，附带 `resources/summary.json` 和按区域着色的
`resources/layout.svg`。

| 场景 | 规模 | 用途 |
|---|---|---|
| `wastewater-plant` | 681 源节点、166 实例、63 连接、约 35.6 万三角形 | 默认开发与近景审查 |
| 程序化大型工业 fixture | 208 主对象、560 操作、6 条工艺线、约 93.7 万三角形 | 大场景契约与性能门禁 |
| `wastewater-mini` | 11 节点（16 KB JSON） | 启动、API、E2E 契约测试 |

`npm run dev` 扫描 `projects/*/project.json`，默认打开修改时间最新的工程；时间相同时取
`currentRevision` 更高的那个。

## 7. 架构

```
外部 AI 或脚本
      │  JSON-RPC 2.0（loopback + 写入令牌）
      ▼
Studio Server ── 认证、单事务串行、持久化、revision 事件
      │
      ├── studio-core      Document、几何、事务、校验、GLB 导出（无 IO）
      ├── studio-assets    glTF/GLB 读取、Prepared Profile、accessor、来源摘要
      ├── studio-import    glTF/GLB、OBJ/MTL、FBX、USD、STEP、IFC、JT → SourceScene
      ├── studio-pipeline  partition、LOD、coarse chunk、Tileset、render batch
      ├── studio-project   路径、写锁、原子快照、变更集、几何缓存
      └── studio-gpu       wgpu 几何内核、residency（实验）
      │
      └── revision 事件 ──→ Web Viewer（薄客户端）
```

**Document + 操作历史是唯一真源**，几何、bounds、诊断、GLB、预览全是从文档重算的派生值。

本地 API 只监听 loopback，写入需要进程级令牌，路径限制在配置的 workspace 内——这是单用户
本地工具的边界，**不是**多租户授权模型。详见 [Architecture](docs/architecture.md)、
[ADR-0001 文档即真源](docs/decisions/0001-document-as-source-of-truth.md)、
[ADR-0002 本地 API 边界](docs/decisions/0002-local-api-boundary.md)。

## 8. 性能与实验性能力

`npm run benchmark` 在目标 Apple Silicon Mac 上跑 release build，任一 p95 超限即失败，
输出 `output/studio-mvp-benchmark.json`。同一场景上 transform-only revision 走 CPU 增量
缓存约 116 ms，比全量重算的 1.285 s 快约 11 倍。

GPU 增量路径约 126 ms，是 CPU 增量的 0.92×，**没有优势**；只有窄视锥可见性查询明显领先
（同一组 243 个 node ID，p50 4.35×）。因此 native GPU compute、resident geometry、
preview packet、GPU scene session 一交付即实验、默认关闭，`auto` 继续选 CPU。这些数字
随机器与版本变化，以本机实跑为准。

## 9. 边界与缺口

**产品范围内尚未实现**（待补齐，不是范围外决定）：布尔并集与交集、任意网格之间的布尔运算、
自由曲线与曲线驱动 sweep、雕刻与有机网格编辑、协作编辑、云租户与远程部署、跨项目 GPU
缓冲持久化。凸体布尔减除（盒/圆柱/球开孔）已交付，见
[能力清单](docs/reference/capabilities.md) §3。
清单与理由见 [Architecture](docs/architecture.md) 的 Not Yet Implemented。

生产路径不依赖 Toolchain、Node Asset Runtime 或 Blender；Sidecar 与旧 repository 的
release surface 已退役。JT 保留为部署方提供的 `ICE3_JT_CONVERTER` 受控例外。

## 10. 给 AI Agent 的约定

写给会改这个仓库的 Agent，是硬约束，不是建议。

**必须遵守**

1. `schemas/` 与 `bindings/` 是生成物。改 Rust 类型后必须重新生成
   （`cargo run -p studio-cli -- schema --output schemas`）；并且**先把各 crate 的
   `bindings/*.ts` 副本同步**，再生成汇总的 `schemas/studio-types.ts`——它是拼接出来的，
   漏一步就落后一版。
2. `npm run verify` 是唯一的"完成"标准（fmt、clippy `-D warnings`、`cargo test`、schema
   `--check`、viewer typecheck/build/E2E 全在内）。只跑 `cargo test` 不算完。
3. 资产改动走 CLI，不要手改 `projects/*/revisions/*.json`。真要改：`snapshotDigest =
   sha256(revision 文件字节)`，并在 `history/operations.jsonl` 追加一条对应记录，否则
   工程直接打不开。
4. 导出的 `--asset-id` 是与消费方的契约；改了属于破坏性变更，必须同步通知下游。
5. 文档改动要同步 `docs/README.md` 索引与 front-matter（Document ID / Status / Owner），
   否则文档门禁失败。

**找东西的最短路径**

| 想知道 | 去哪 |
|---|---|
| 有哪些 operation、字段 schema | `schemas/studio-api.schema.json`（唯一真相）+ `docs/reference/capabilities.md` |
| JSON-RPC 方法表 | `docs/reference/api.md` |
| 某个设计为什么这样 | `docs/specs/`、`docs/decisions/` |
| 几何怎么求值 | `crates/studio-core/src/` |
| 资产怎么切成 Tileset | `crates/studio-pipeline/src/` |
| 性能门禁阈值 | `benchmarks/`、`scripts/benchmark-prepare.mjs` |

## 11. 文档

- [文档索引](docs/README.md) · [Architecture](docs/architecture.md)
- [能力清单](docs/reference/capabilities.md) · [API Reference](docs/reference/api.md) · [Diagnostics](docs/reference/diagnostics.md)
- [ChangeSet 审阅](docs/reference/change-sets.md) · [Asset Pipeline](docs/reference/asset-pipeline.md)
- 指南：[Getting Started](docs/guides/getting-started.md) · [大场景资产](docs/guides/large-industrial-fixture.md) · [污水厂 fixture](docs/guides/wastewater-fixture.md)
- [第三方声明](THIRD_PARTY_NOTICES.md)

## 12. 许可证

[MIT](LICENSE)
