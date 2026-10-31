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

```bash
npm --prefix apps/viewer ci
npm run verify
npm run benchmark
npm run benchmark:gpu
```

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
transform-only revision 的 p50 已从默认 CPU 路径约 1.28 s 降到约 117 ms，达到 5×
目标；初始 GPU 场景构建仍约 24.8 s，因此场景会话继续保持实验性和默认关闭。
失败不会回滚已提交 Document。

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
