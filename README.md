# ICE3 Studio

> Document ID: ICE3-STUDIO-RM-001
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

ICE3 Studio 是本地优先、API-first 的 AI-native 建模系统。Studio Core 使用 Rust 从零
构建场景、几何、材质、关系、校验、版本和导出能力；外部 AI 通过受控 API 创建和修改
模型，Web Viewer 负责展示、选择和人工审查。

当前 `wastewater-mini` MVP 已实现并通过自动化验收：外部客户端可以通过同一套公开
API 建模、修复失败事务、撤销/重做、重开项目、导出标准 GLB，并在 Viewer 中查看 revision
更新后的新预览。Studio 不依赖其它建模应用，Rust 始终拥有建模内核。

## 快速开始

默认开发场景是完整的高保真污水处理厂，保留 204 个可搜索、可选择和可局部编辑的语义
绑定，使用分层流式资产承载完整几何，不把大型模型内联进 Studio Document：

```bash
npm --prefix apps/viewer ci
npm run dev
```

首次运行会注册 251 MB 引用资产并提交 204 条绑定（约两分钟），之后每次启动直接复用
已存项目（约两秒）。命令打印 Viewer URL 并持续服务，`Ctrl+C` 停止。

完整验证与基准：

```bash
npm run verify
npm run benchmark
npm run benchmark:gpu
```

`wastewater-mini`（16 KB JSON，11 个节点）保留为契约测试 fixture，被 `npm run verify`、
E2E 和 MVP benchmark 使用，可用 `npm run dev:mini` 单独启动。

完整的本地启动、API 调用和 Viewer 使用方式见
[Getting Started](docs/guides/getting-started.md)。

## 能力边界

- Rust Studio Core：Document、通用几何、材质、关系、约束、事务、校验和 GLB 导出。
- Rust Studio Server：仅 loopback 的 JSON-RPC、写入令牌、revision WebSocket 和受控
  Toolchain 预览编排。
- Managed Toolchain Sidecar：固定版本的本地资产准备进程，提供 Preview、GLB 准备、
  Validation、LOD/partition 和能力状态；Rust Core 保持独立。
- ChangeSet 审查层：AI 与人工表单变更共享候选 digest；默认由短期人工授权提交。
- Native GPU Compute and resident preview（experimental）：Rust `wgpu` 几何内核、CPU parity、
  常驻资源、增量 preview packet 与 WebGL2/WebGPU buffer 复用；当前设备未达到 release
  性能门槛，因此自动路径仍使用 CPU。Preview packet 还需显式设置
  `STUDIO_PREVIEW_PACKETS=1`。
- TypeScript Viewer：只通过公开 API 展示场景、revision、诊断和选择状态。
- `wastewater-mini`：确定性 AI API 验收 fixture，不是写死在 Core 中的行业类型。

## Agent API

外部 Agent 可以通过 `system.agentManifest` 发现工具、当前权限和调用限制，再使用
`scene.context` 获取固定大小项目摘要。`scene.search` 支持 source/derived 节点、字段投影
和 revision-bound cursor；`evaluation.query` 只对显式列出的最多 32 个派生节点返回摘要或
按需 positions/indices。

模型修改仍通过原子 `document.apply` 或持久化 ChangeSet。默认 `review-required` 模式下，
Agent 可以查询、提案和求值，但不能直接应用或提交；人工审批令牌负责最终提交。稀疏
`instance.patch`、`component.patch`、`connection.patch` 和 `material.patch` 只修改指定字段，
并继续经过完整校验、diff、幂等和 revision 冲突检查。

大型场景基准中，`scene.context` 为 860 bytes，25 条 source+derived 搜索结果页约
7.9 KiB；机器门禁分别限制为 16 KiB 和 128 KiB。Studio 不内置模型、Prompt、MCP Gateway
或 Agent 策略。

## 性能基线

`npm run benchmark` 使用 release build，在目标 Apple Silicon Mac 上记录并输出
`output/studio-mvp-benchmark.json`。当前基线为：

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

这些值是当前设备上的工程基线，不是跨机器容量承诺。脚本会在任一 p95 超过设计上限时
失败。

`npm run benchmark:gpu` 运行 50k、500k 和 2M 三角形的 CPU、cold、warm-topology、
resident、snapshot 与 preview 证据，并输出 `output/native-gpu-benchmark.json`。当前
integrated Metal 设备上，500k triangles 的 CPU p50 为 3.84 ms、cold GPU 为 10.74 ms、
resident handle 为 0.00 ms；2M triangles 分别为 11.36 ms、39.24 ms 和 0.00 ms。Resident
handle 测量的是无哈希、无上传、无 fence 的缓存命中，不代表新的 GPU kernel 比 CPU 快；
热路径哈希为 0，lookup p50 低于 0.1 ms。报告将 resident 缓存命中与 cold GPU crossover
拆成独立门禁，不可用或未接线的 GPU kernel/queue 时间输出 `null`，不会用 wall-clock
冒充 kernel 时间。Cold 和 warm-topology 仍明显慢于 CPU，因此整体性能门禁失败关闭，
`auto` 继续选择 CPU，WebGPU 浏览器渲染仍要求显式实验开启。

`STUDIO_GPU_SCENE_SESSIONS=1` 配合 `--native-gpu gpu` 可启用实验性 revision-scoped GPU
场景会话，`npm run benchmark:gpu-scene` 会输出大型工业场景对照。精确尺寸 buffer 分配后，
完整 6.1M 有效三角形的 233 个共享几何和 1,938 个实例约占用 46 MiB GPU 内存。
CPU incremental scene session 默认开启，transform-only revision 的 p50 已从全量路径
约 1.285 s 降到约 116 ms，获得约 11.1× 提升。GPU incremental 路径约 126 ms，相对 CPU
增量路径约为 0.92×，没有额外性能优势，因此 GPU scene session 仍保持实验性和默认关闭。
生产 preview 路径会在上一 revision packet 可用时写入增量 packet；transform-only 更新不包含
geometry payload，geometry 更新只回读受影响的 scene geometry。失败不会回滚已提交 Document。

三方同场 benchmark 显示：CPU 增量相对 CPU 全量的 transform 提升约 11.0×；GPU 相对
CPU 增量的整体提升约为 10.2×，这说明主要收益来自增量场景架构，而非 GPU kernel。
实例 transform 在 GPU 实验路径中只上传变化的 64-byte matrix range。
`evaluation.visibleInstances` 在 CPU 默认路径中使用同一组世界空间包围球执行回退；
场景会话启用时使用 resident instance bounds 和 GPU compute。窄视锥基准中 CPU 与 GPU
返回相同的 243 个稳定 node ID；31 次采样中 CPU p50/p95 为 2.613/3.616 ms，
GPU 为 0.601/2.276 ms，p50 约快 4.35×。该接口和场景会话一起保持显式实验状态。

同场分解显示 GPU transform revision 的约 126 ms 中，场景 apply 约 125 ms、scene wait
约 0.49 ms、stats 约 0.30 ms；GPU scene planning 约 1.8 ms。当前主要成本已转移到
Document 事务、持久化和 RPC 边界。

默认基准不配置 Toolchain Sidecar，因此 `runtimePreviewMs.samples` 为 0；Toolchain
预览协议和浏览器 E2E 由 `npm run verify` 单独覆盖。

## AI 辅助开发

本项目在设计和实现过程中大量使用了 AI 辅助开发。

## 文档

- [文档索引](docs/README.md)
- [Architecture](docs/architecture.md)
- [API Reference](docs/reference/api.md)
- [Diagnostics](docs/reference/diagnostics.md)
- [AI-Native Modeling MVP 设计](docs/specs/2026-10-30-ai-native-studio-mvp-design.md)

## 许可证

本项目采用 [MIT License](LICENSE) 许可。
