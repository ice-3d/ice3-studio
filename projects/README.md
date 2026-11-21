# 污水处理厂工程（wastewater-plant）

> Document ID: ICE3-STUDIO-GUIDE-003
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

用 Studio 原生组件建的 10 万吨级污水处理厂。几何、端口、管道全部由内核按参数生成，
不引用任何外部 GLB。

## 1. 打开

```bash
cd ice3-studio
STUDIO_SESSION_TOKEN=dev STUDIO_VIEWER_DIR=$PWD/apps/viewer/dist \
  ./target/release/studio-cli serve \
  --workspace ./projects --project wastewater-plant --port 8120
```

浏览器打开 `http://127.0.0.1:8120/?token=dev`。

预览、Asset Prepare、partition、LOD 和 Tileset 均由 Studio 原生 crate 完成，不依赖
Toolchain 或外部 DCC。需要发布 Agent Runtime bundle 时运行 `export tileset`；需要
直接处理其它 source 时运行 `partition`。完整链路见
[Asset Pipeline](../docs/reference/asset-pipeline.md)。

## 2. 修改

所有改动走 `document.apply`，每次提交产生一个 revision。

```bash
curl -s -X POST http://127.0.0.1:8120/rpc \
  -H 'authorization: Bearer dev' -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"document.apply","params":{
        "expectedRevision": 1, "dryRun": false, "idempotencyKey": "edit-1",
        "operations": [{"type":"node.update","node_id":"node-000010","name":"PC-101 初沉池"}]}}'
```

`expectedRevision` 不匹配会被拒绝，避免并发覆盖。

## 3. 打包与还原

```bash
./target/release/studio-cli pack --project projects/wastewater-plant --output wwtp.ice3
./target/release/studio-cli unpack wwtp.ice3 --into /tmp/restored
```

## 4. 目录内容

| 路径 | 作用 |
|---|---|
| `project.json` | 工程 id、当前修订、撤销栈 |
| `revisions/<n>.json` | 场景快照：节点、材质、关系，组件以摘要引用 |
| `components/<id>-<digest>.json` | 组件定义，按内容寻址，同一版本在所有修订间共享 |
| `history/operations.jsonl` | 操作历史，逐行可重放 |
| `cache/` | 几何与预览缓存，不入库，可重建 |
| `references/` | 外链资产，不入库 |

## 5. 总平面

两条并联系列，共用进水段与深度处理。占地约 560 × 210 米。

```
GR-101 格栅渠 → PS-101 泵房 → GC-101 沉砂池
                                    └→ DD-001 配水井
                                         ├→ PC-101 → AE-101/102/103 → DD-101 → SC-101/102/103 ┐
                                         └→ PC-201 → AE-201/202/203 → DD-201 → SC-201/202/203 ┴→ FB-101 → FL-101 → UV-101
```

二沉池的混合液回流与剩余污泥分别进入 RWS-101；RAS 经 `RED-101` 异径管返回两条缺氧线，
WAS 进入浓缩池。初沉池污泥经 SCW-101 集泥井进入浓缩池。两座消化池沼气经 `GAS-TEE-101`
三通汇入 GCL-101 集气总管，再穿过 `SLV-101` 套管进入 GH-101，并接入 GF-101 安全火炬；
`SLV-102` 套管用于回流污泥穿池壁。脱水泥饼由输送机搬运，不再绘制为液体圆管。

### 5.1 水线（18 台）

| 编号 | 构筑物 | 位置 | 主要构造 |
|---|---|---|---|
| GR-101 | 进水格栅渠 | -135, 0 | 两道格栅各 13 根栅条，检修走道与栏杆 |
| PS-101 | 提升泵房 | -105, 0 | 门洞、通风百叶、三台立式泵（底座/泵体/电机/出水管） |
| GC-101 | 曝气沉砂池 | -70, 0 | 阶梯砂斗、刮砂桥与驱动、8 道曝气支管 |
| PC-101 / PC-201 | 初沉池 | 0, ±20 | 圆形，中心进水筒、周边出水堰、刮泥桥、环形栏杆、检修爬梯 |
| AE-x01 | 厌氧池 | 45, ±20 | 矩形廊道、导流墙、潜水搅拌器与导流筒 |
| AE-x02 | 缺氧池 | 92, ±20 | 矩形廊道、潜水搅拌器、RAS 回流入口 |
| AE-x03 | 好氧池 | 139, ±20 | 矩形廊道、曝气总管、支管和扩散器 |
| SC-x01~x03 | 二沉池 | 190/235/280, ±20 | 圆形，分别设 RAS 回流和剩余污泥出口 |
| FL-101 | 滤池 | 350, 0 | 4 格分仓，各带滤料层、水面与反冲洗配管 |
| UV-101 | 紫外消毒渠 | 395, 0 | 3 组灯架共 12 支灯管 |

### 5.2 污泥线（5 台）

| 编号 | 构筑物 | 位置 | 主要构造 |
|---|---|---|---|
| TH-101 / TH-102 | 污泥浓缩池 | 250/288, 62 | 接收初沉与剩余污泥，中心搅拌器与周边出水堰 |
| DG-101 / DG-102 | 消化池 | 340, 62/102 | 圆柱池体，阶梯锥顶、环形检修平台、16 级外壁爬梯 |
| DW-101 | 脱水机房 | 405, 78 | 厂房 + 三台卧式离心机（机座/转鼓/电机） |

新增工艺节点包括 DD-001/DD-x01 配水井、FB-101 二沉集水井、SCW-101 初沉集泥井、
RWS-101 回流污泥泵站、GCL-101 沼气集气总管、GH-101 沼气柜、GF-101 火炬，以及
CV-101/CV-102 泥饼输送机。

### 5.3 辅助（3 台）

| 编号 | 构筑物 | 位置 |
|---|---|---|
| CTRL-101 | 中控楼 | -135, -62（三层，带入口雨棚） |
| BLW-101 | 鼓风机房 | 92, -62 |
| CHEM-101 | 加药间 | 350, -55 |

### 5.4 端口介质

管道由连接意图路由，介质必须与端口一致，否则校验拒绝：

```
水线：raw-water → screened-water → lifted-water → degritted-water
      → settled-water → mixed-liquor → secondary-effluent
      → filtered-water → final-effluent

回流污泥：return-sludge → anoxic return-inlet
剩余与初沉污泥：waste-sludge / primary-sludge
      → thickened-sludge → digested-sludge → dewatered-cake
沼气：biogas → gas holder → safety flare
```

生化池按 `anaerobic-basin`、`anoxic-basin` 和 `aerobic-basin` 分别定义，避免厌氧、
缺氧段错误生成曝气扩散器。

管线按功能着色，并在通用路径层生成连续弯头：

| 颜色 | 功能 |
|---|---|
| 亮银 | 普通原水与工艺水 |
| 绿色 | 二沉出水、滤后水与最终出水 |
| 蓝色 | 曝气空气与沼气 |
| 红色 | 初沉、回流、剩余、浓缩和消化污泥 |
| 黄色 | 化学药剂 |

## 6. 规模

| | |
|---|---|
| 组件定义 | 86 |
| 实例 | 172 |
| 连接 | 65 |
| 材质 | 48 |
| 操作数 | 3,404 |
| 展开节点 | 7,446 |
| 唯一网格 | 1,444 |
| 有效三角形 | 383,712 |
| 唯一三角形 | 137,076 |

模型包含带检修桥、出水堰、刮泥机和爬梯的圆形池；按工艺模式区分的三段生化池；完整的
RAS、初沉污泥、剩余污泥和沼气链路；带泵组、鼓风机、加药、换热和脱水设备的功能区；
以及带路缘、排水沟、装卸场地和分区检修道路的完整厂区。

### 6.1 管线附件

- `pipe-tee-*`：三通，具有独立的 `run-in`、`run-out` 和 `branch` 端口；
- `pipe-reducer-*`：异径管，大端和小端连接不同直径的真实端口，壳体呈递减过渡；
- `wall-sleeve-*`：穿墙套管，具有 `wall-in`、`wall-out` 端口并绑定真实宿主节点；
- 三通或异径管拆分主管时，connection 使用 `supportLineId` 绑定该段的支撑站点，
  同时共享附件 `lineId` 以保留线路拓扑。

固定近景入口：

```bash
npm run asset:visual-review
```

其中 `pipe-fittings` 检查回流污泥异径管，`wall-penetration` 检查沼气穿墙套管。两处
近景都应在已安装的 Google Chrome 中打开；Viewer Diagnostics 应为空。

### 6.2 线路审查

在 Viewer 左侧搜索并选中 `RED-101 回流污泥异径管` 或
`SLV-101 沼气主管穿墙套管`，打开右侧 `Inspect`。`Route inspection` 区域应显示：

- 主 `lineId`、revision、共享该身份的全部分段和解析后的起止端口；
- 每段介质、管径、route policy、点数、世界坐标和方向；
- 与线路匹配的三通、异径管、穿墙套管、宿主以及支撑站点。

点击分段、端口、附件或支撑只更新当前 Selection，不创建 ChangeSet。支撑对象的
`supportLineId` 通过 Document 中 connection metadata 映射回主线路，避免把分段支撑身份
误当成公开线路 ID。

也可以直接调用只读 API：

```bash
curl -s -X POST http://127.0.0.1:8120/rpc \
  -H 'authorization: Bearer dev' -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"route.inspect",
       "params":{"lineId":"L-RAS-HEADER-101","revision":1}}'
```

当前工程统计以第 6 节表格为准；`route.inspect` 是读取接口，不改变 revision 或模型统计。

## 7. 修订存储

组件定义不写在快照里，按内容摘要单独存成 `components/<id>-<digest>.json`，快照只记
`{ id, digest }`。组件没改动时不产生新文件。

| | 每次修订 |
|---|---|
| 拆分前 | 95 KB |
| 拆分后 | 40 KB |

## 8. 扩展方式

改 `scripts/build-wastewater-model.mjs` 后重新生成：

```bash
node scripts/build-wastewater-model.mjs --workspace ./projects
```

**注意**：显式 `--workspace ./projects` 会替换同名工程并重新创建 Revision 1，不能保留
旧修订历史。日常小改动用 RPC 增量修改或 ChangeSet，不要重跑生成脚本。

重建是完整模型发布，不是只换几何。`site-plan.mjs` 会重新写入 36 个 Material Surface，
包括底色、法线、ORM、roughness/metallic 和 UV 缩放；测试会阻止贴图材质在 rebuild 中
静默丢失。重建完成后仍需从 Studio 重新执行 `export tileset`，下游才能得到新的几何和
材质组合。

## 9. 已知问题

**Studio 默认关闭方向光阴影。** 厂区尺度下，画面外构件和地面薄板会投出容易被误认为
空地上实体的长阴影；Viewer 因此默认使用固定光照，只有显式访问 `?shadows=1` 时才启用
方向光阴影。资产本身不再使用黑色接触阴影贴片。

**阶段二B安全门禁已完成。** 连续弯头、防扭转扫掠、路径碰撞、道路净空和支吊架间距已
纳入提交前校验。三通、异径管、穿墙套管及其端口、宿主穿越和预算门禁也已交付。

池体半径、深度和壁厚已经通过 `ComponentParameter` 写入实例；部件尺寸使用字面量、参数
引用或 `{ "expr": "radius - wall / 2" }` 派生表达式。表达式只支持受控四则运算、括号和
白名单数学函数，不允许执行任意脚本。
