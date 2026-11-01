# 污水处理厂工程（wastewater-plant）

用 Studio 原生组件建的 10 万吨级污水处理厂。几何、端口、管道全部由内核按参数生成，
不引用任何外部 GLB。

## 打开

```bash
cd ice3-studio
STUDIO_SESSION_TOKEN=dev STUDIO_VIEWER_DIR=$PWD/apps/viewer/dist \
  ./target/release/studio-cli serve \
  --workspace ./projects --project wastewater-plant --port 8120
```

浏览器打开 `http://127.0.0.1:8120/?token=dev`。

不需要 toolchain：预览由内核直接产出 GLB。配了 toolchain 才会额外做压缩与分块。

## 修改

所有改动走 `document.apply`，每次提交产生一个 revision。

```bash
curl -s -X POST http://127.0.0.1:8120/rpc \
  -H 'authorization: Bearer dev' -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"document.apply","params":{
        "expectedRevision": 1, "dryRun": false, "idempotencyKey": "edit-1",
        "operations": [{"type":"node.update","node_id":"node-000010","name":"PC-101 初沉池"}]}}'
```

`expectedRevision` 不匹配会被拒绝，避免并发覆盖。

## 打包与还原

```bash
./target/release/studio-cli pack --project projects/wastewater-plant --output wwtp.ice3
./target/release/studio-cli unpack wwtp.ice3 --into /tmp/restored
```

## 目录内容

| 路径 | 作用 |
|---|---|
| `project.json` | 工程 id、当前修订、撤销栈 |
| `revisions/<n>.json` | 场景快照：节点、材质、关系，组件以摘要引用 |
| `components/<id>-<digest>.json` | 组件定义，按内容寻址，同一版本在所有修订间共享 |
| `history/operations.jsonl` | 操作历史，逐行可重放 |
| `cache/` | 几何与预览缓存，不入库，可重建 |
| `references/` | 外链资产，不入库 |

## 总平面

两条并联系列，共用进水段与深度处理。占地约 560 × 210 米。

```
GR-101 格栅渠 → PS-101 泵房 → GC-101 沉砂池
                                    └→ DD-001 配水井
                                         ├→ PC-101 → AE-101/102/103 → DD-101 → SC-101/102/103 ┐
                                         └→ PC-201 → AE-201/202/203 → DD-201 → SC-201/202/203 ┴→ FB-101 → FL-101 → UV-101
```

二沉池的混合液回流与剩余污泥分别进入 RWS-101；RAS 返回两条缺氧线，WAS 进入浓缩池。
初沉池污泥经 SCW-101 集泥井进入浓缩池。消化池沼气经 GCL-101 集气总管进入 GH-101，
再接入 GF-101 安全火炬；脱水泥饼由输送机搬运，不再绘制为液体圆管。

### 水线（18 台）

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

### 污泥线（5 台）

| 编号 | 构筑物 | 位置 | 主要构造 |
|---|---|---|---|
| TH-101 / TH-102 | 污泥浓缩池 | 250/288, 62 | 接收初沉与剩余污泥，中心搅拌器与周边出水堰 |
| DG-101 / DG-102 | 消化池 | 340, 62/102 | 圆柱池体，阶梯锥顶、环形检修平台、16 级外壁爬梯 |
| DW-101 | 脱水机房 | 405, 78 | 厂房 + 三台卧式离心机（机座/转鼓/电机） |

新增工艺节点包括 DD-001/DD-x01 配水井、FB-101 二沉集水井、SCW-101 初沉集泥井、
RWS-101 回流污泥泵站、GCL-101 沼气集气总管、GH-101 沼气柜、GF-101 火炬，以及
CV-101/CV-102 泥饼输送机。

### 辅助（3 台）

| 编号 | 构筑物 | 位置 |
|---|---|---|
| CTRL-101 | 中控楼 | -135, -62（三层，带入口雨棚） |
| BLW-101 | 鼓风机房 | 92, -62 |
| CHEM-101 | 加药间 | 350, -55 |

### 端口介质

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
| 黄色 | 原水与工艺水 |
| 绿色 | 二沉出水、滤后水与最终出水 |
| 蓝色 | 曝气空气与沼气 |
| 红色 | 初沉、回流、剩余、浓缩和消化污泥 |
| 紫色 | 化学药剂 |

## 规模

| | |
|---|---|
| 组件定义 | 46 |
| 实例 | 210 |
| 连接 | 63 |
| 材质 | 45 |
| 操作数 | 757 |
| 展开节点 | 4,241 |
| 唯一网格 | 903 |
| 有效三角形 | 351,364 |
| 唯一三角形 | 110,464 |

模型包含带检修桥、出水堰、刮泥机和爬梯的圆形池；按工艺模式区分的三段生化池；完整的
RAS、初沉污泥、剩余污泥和沼气链路；带泵组、鼓风机、加药、换热和脱水设备的功能区；
以及带路缘、排水沟、装卸场地和分区检修道路的完整厂区。

## 修订存储

组件定义不写在快照里，按内容摘要单独存成 `components/<id>-<digest>.json`，快照只记
`{ id, digest }`。组件没改动时不产生新文件。

| | 每次修订 |
|---|---|
| 拆分前 | 95 KB |
| 拆分后 | 40 KB |

## 扩展方式

改 `scripts/build-wastewater-model.mjs` 后重新生成：

```bash
node scripts/build-wastewater-model.mjs --workspace ./projects
```

**注意**：显式 `--workspace ./projects` 会替换同名工程并重新创建 Revision 1，不能保留
旧修订历史。日常小改动用 RPC 增量修改或 ChangeSet，不要重跑生成脚本。

## 已知问题

**真实方向光阴影尚未启用。** 资产已经通过材质和几何避免黑色假阴影贴片，但
`ice3-engine` 的 WebGL2 阴影通道仍在修复中，因此当前画面没有建筑和池体的真实投影。

**阶段二B尚未完成。** 连续弯头和防扭转扫掠已经交付，但路径碰撞、穿墙套管、三通变径和
支吊架间距仍待后续实现。

**池体尺寸硬编码。** 初沉池半径 13、二沉池半径 15 都写死在生成函数里，组件参数
（`ComponentParameter`）尚未使用。原因是 `ScalarBinding` 只支持字面量或参数名，
不支持 `radius - wall/2` 这类派生表达式，参数化需要先补齐表达式求值。
