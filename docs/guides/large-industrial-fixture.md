# 大型工业场景验收

> Document ID: ICE3-STUDIO-GUIDE-002
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

`large-industrial-site` 是一个完全虚构、可确定性重建的大型工业场景。它用于验证 Studio
能否通过公共 API 构建大型模型，而不是绑定污水处理或其他单一业务。

## 规模基线

- 厂区约 `1000 x 550 m`。
- 216 个主要可选组件实例，其中 12 个是功能建筑。
- 4 个 primary structure、18 个 biological reactor 和 12 个 secondary structure。
- 6 条 process train、8 个功能区、208 条有类型连接。
- 本次开发参考机器的评估结果为 `6,125,190` effective triangles。
- 23 个设备/建筑模板和 208 条独立连接路由，共 231 个 unique meshes。
- unique triangles 为 `716,203`，有效三角形共享比约 `8.55x`。

设备名称和区域只存在于 fixture metadata。Core 和公开 Operation 保持通用。

## 通过公共 API 生成

```bash
source ~/.nvm/nvm.sh
nvm use 24.22.1
npm run fixture:large-industrial
```

脚本会：

1. 构建 release Studio CLI。
2. 在临时 workspace 启动本地 autonomous Server。
3. 使用 `component.define`、`instance.create`、`relation.create` 和
   `connection.create` 提交高层计划。
4. 验证 dry-run、216 个主对象、三角形预算、拓扑和共享几何。
5. 导出 source GLB、关闭并重开项目。
6. 将机器可读结果写入 `output/large-industrial-fixture.json`。

生成物只用于本地验收，不进入 Git。

## 性能基准

```bash
npm run benchmark:large-industrial
```

报告写入 `output/large-industrial-benchmark.json`。2026-10-30 的开发参考机器结果：

| 指标 | p50 | p95 |
|---|---:|---:|
| 高层计划构建 | 20.586 ms | 56.563 ms |
| 完整初始 apply | 1,341.949 ms | 1,341.949 ms |
| 216 节点批量更新 + stats | 1,287.207 ms | 1,344.515 ms |
| warm scene stats | 1,187.955 ms | 1,221.104 ms |
| 单实例更新 + stats | 1,281.188 ms | 1,289.573 ms |
| source GLB export | 1,224.457 ms | 1,311.039 ms |
| close + reopen + stats | 3,860.797 ms | 4,030.901 ms |

本次 source GLB 为 `16,323,856` bytes，Document 序列化为 `37,838,489` bytes。局部更新只
生成 1 个 mesh，复用 230 个缓存 mesh。

这些数字是本机基线，不是跨设备绝对承诺。共享 CI 验证规模、正确性和门禁，不绑定固定硬件
耗时。

## 验收边界

- Studio 负责确定性文档、几何求值、连接路由和 source GLB。
- Toolchain 负责后续 Prepared GLB、Runtime LOD、空间分区和 render batching。
- 浏览器是否使用 WebGPU 与本验收无关；Studio Native GPU 在 M3 实施。
- 大型场景必须通过 `evaluation.stats` 获取统计，不应下载完整几何后才能判断规模。
