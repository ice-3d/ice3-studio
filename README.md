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
```

完整的本地启动、API 调用和 Viewer 使用方式见
[Getting Started](docs/guides/getting-started.md)。

## 能力边界

- Rust Studio Core：Document、通用几何、材质、关系、约束、事务、校验和 GLB 导出。
- Rust Studio Server：仅 loopback 的 JSON-RPC、写入令牌、revision WebSocket 和受控
  Toolchain 预览编排。
- TypeScript Viewer：只通过公开 API 展示场景、revision、诊断和选择状态。
- `wastewater-mini`：确定性 AI API 验收 fixture，不是写死在 Core 中的行业类型。

## 性能基线

`npm run benchmark` 使用 release build，在目标 Apple Silicon Mac 上记录并输出
`output/studio-mvp-benchmark.json`。当前基线为：

| 指标 | p95 |
|---|---:|
| Server 冷启动到 health | 432.58 ms |
| 重开 100 Node 项目 | 2.07 ms |
| 100 Operations 原子事务 | 17.51 ms |
| `wastewater-mini` 求值 | 1.68 ms |
| source GLB 导出 | 0.90 ms |
| Toolchain Runtime preview | 127.37 ms |

这些值是当前设备上的工程基线，不是跨机器容量承诺。脚本会在任一 p95 超过设计上限时
失败。

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
