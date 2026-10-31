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

## 高保真参考资产

程序化 fixture 用于 API、拓扑和共享几何回归；它不应作为最终视觉精度基准。厂区视觉
验收使用工作区中的真实 Prepared GLB 和分层 Tileset，通过 Reference Asset API 流式
加载完整几何，同时创建 204 个可被 Agent 搜索、选择和局部编辑的语义绑定。

```bash
npm run example:wwtp-reference
```

该命令会启动本地 Studio、注册原始资产、创建语义绑定，并由 Viewer 按实体角色应用
展示材质，最后输出可手动检查的 URL。当前参考资产包含 1,761 个源节点、204 个设备绑定、75 种材质和
`5,757,962` 个有效三角形。高保真展示使用带完整节点层级的 Prepared GLB 模式，让
设备颜色可以沿子树传递，而不是被 Render Batch 合并成少数共享材质。Viewer 会保留
原始材质色相，并将背景结构亮度向白色混合 42%；泵、风机、格栅、初沉/二沉池、生化池、
消化池、深度处理和加药设备分别使用可辨识的工业配色。跨设备共享的几何保持中性色，
避免局部改色扩散成整片色块。

### 配色依据

- 真实厂区航拍中，主体通常是浅灰或米灰混凝土、深灰沥青、蓝绿水体、银灰钢构件，
  艳色只出现在设备外壳、防护件和管道识别中。
- 工业 HMI 的通用原则是以中性背景为主体，保留高饱和色给报警、异常和操作状态；
  参见 [ISA-101](https://www.isa.org/standards-and-publications/isa-standards/isa-101-standard)。
- 管线按介质或危险类别辅助识别，空气、可燃气体、腐蚀性化学品和污泥不能共用一个
  颜色；参见 [ASME A13.1](https://www.asme.org/codes-standards/find-codes-standards/a13-1-scheme-identification-piping-systems)。
- 红、橙、黄、绿、蓝等安全色具有固定警示语义，不应仅用于装饰；参见
  [OSHA 1910.144](https://www.osha.gov/laws-regs/regulations/standardnumber/1910/1910.144)。

因此展示材质按实体角色区分：浅灰混凝土构筑物、深灰道路、蓝绿工艺水体、银灰栏杆
和钢平台、浅暖灰建筑、深色设备壳体，以及按空气、化学品、气体和污泥分别识别的管线。

高保真 Viewer 的管线颜色以功能为主：主工艺水和空气使用蓝色，混合液与回流水使用
绿色，沼气和可燃气体使用黄色，化学药剂使用橙色，污泥使用棕色，滤液使用紫色，
反冲洗水使用青色。钢管支架仍保持中性钢灰，不跟随内部介质变色。

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
