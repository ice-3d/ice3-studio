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
- 208 个主要可选组件实例，其中 10 个是功能建筑。
- 6 条完整 process train，覆盖取水、预处理、初沉、生化、二沉、深度处理、
  污泥处理和公共设施。
- 16 类可复用装配模板、913 个真实语义部件和 99 条有类型连接。
- 本次开发参考机器的评估结果为 `859,236` effective triangles。
- 模板、功能建筑、厂区节点和连接共生成 `1,016` 个 unique meshes。
- unique triangles 为 `96,340`，有效三角形共享比约 `8.92x`。

模板不是同一种圆柱的缩放副本，也没有使用随机小面片填充三角形预算。生化池包含
池体、水面、曝气立管、扩散器、巡检通道、栏杆和楼梯；初沉池、二沉池和浓缩池包含
圆池、中心井、驱动机构、刮泥臂、出水堰、环形栏杆和检修楼梯；消化池包含高筒、
穹顶、环形加强带、平台和爬梯。泵、格栅、风机、加药装置和分析仪使用各自的设备
体量、管路、阀门和防护件。功能建筑包含主体、屋面、窗带、入口、雨棚和屋顶设备。
场景还包含厂区地面、横纵道路、功能分区铺装、设备基础和工艺连接。

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
4. 验证 dry-run、208 个主对象、三角形预算、拓扑、逐部件材质和共享几何。
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
| 高层计划构建 | 2.337 ms | 7.321 ms |
| 完整初始 apply | 5,542.122 ms | 5,542.122 ms |
| 208 节点批量更新 + stats | 85.843 ms | 155.089 ms |
| warm scene stats | 0.522 ms | 0.995 ms |
| 单实例更新 + stats | 44.043 ms | 46.378 ms |
| 单实例参数更新 + stats | 48.849 ms | 51.056 ms |
| source GLB export | 198.490 ms | 226.367 ms |
| close + reopen + stats | 249.992 ms | 252.029 ms |

本次 source GLB 约 `6.1 MiB`，Document 约 `1.36 MiB`。局部更新复用已有缓存
geometry，不重新生成完整场景。

这些数字是本机基线，不是跨设备绝对承诺。共享 CI 验证规模、正确性和门禁，不绑定固定硬件
耗时。

## 验收边界

- Studio 负责确定性文档、几何求值、连接路由和 source GLB。
- Toolchain 负责后续 Prepared GLB、Runtime LOD、空间分区和 render batching。
- 浏览器是否使用 WebGPU 与本验收无关；Studio Native GPU 在 M3 实施。
- 大型场景必须通过 `evaluation.stats` 获取统计，不应下载完整几何后才能判断规模。
