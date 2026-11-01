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
                                    ├→ PC-101 → AE-101/102/103 → SC-101/102/103 ┐
                                    └→ PC-201 → AE-201/202/203 → SC-201/202/203 ┴→ FL-101 滤池 → UV-101 消毒
```

### 水线（18 台）

| 编号 | 构筑物 | 位置 | 主要构造 |
|---|---|---|---|
| GR-101 | 进水格栅渠 | -135, 0 | 两道格栅各 13 根栅条，检修走道与栏杆 |
| PS-101 | 提升泵房 | -105, 0 | 门洞、通风百叶、三台立式泵（底座/泵体/电机/出水管） |
| GC-101 | 曝气沉砂池 | -70, 0 | 阶梯砂斗、刮砂桥与驱动、8 道曝气支管 |
| PC-101 / PC-201 | 初沉池 | 0, ±20 | 圆形，中心进水筒、周边出水堰、刮泥桥、环形栏杆、检修爬梯 |
| AE-x01 | 曝气池（厌氧段） | 45, ±20 | 矩形廊道，三段导流墙、10 道曝气支管 |
| AE-x02 / x03 | 曝气池（缺氧/好氧段） | 92/139, ±20 | 同上 |
| SC-x01~x03 | 二沉池 | 190/235/280, ±20 | 圆形，直径大于初沉池，带剩余污泥出口 |
| FL-101 | 滤池 | 350, 0 | 4 格分仓，各带滤料层、水面与反冲洗配管 |
| UV-101 | 紫外消毒渠 | 395, 0 | 3 组灯架共 12 支灯管 |

### 污泥线（5 台）

| 编号 | 构筑物 | 位置 | 主要构造 |
|---|---|---|---|
| TH-101 / TH-102 | 污泥浓缩池 | 250/288, 62 | 圆形，中心搅拌器与刮臂、周边出水堰 |
| DG-101 / DG-102 | 消化池 | 340, 62/102 | 圆柱池体，阶梯锥顶、环形检修平台、16 级外壁爬梯 |
| DW-101 | 脱水机房 | 405, 78 | 厂房 + 三台卧式离心机（机座/转鼓/电机） |

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

污泥：waste-sludge → thickened-sludge → digested-sludge
```

曝气池因此拆成 `aeration-feed`（接初沉出水）和 `aeration-mixed`（接混合液）。

## 规模

| | |
|---|---|
| 组件定义 | 37 |
| 实例 | 200 |
| 连接 | 31 |
| 材质 | 41 |
| 操作数 | 525 |
| 展开节点 | 13,667 |
| 唯一网格 | 969 |
| 有效三角形 | 347,636 |
| 唯一三角形 | 40,620 |

模型包含带检修桥、出水堰、刮泥机和爬梯的圆形池；三廊道曝气池及其曝气支管；带泵组、
鼓风机、加药、换热和脱水设备的功能区；以及带路缘、排水沟、装卸场地和分区检修道路的
完整厂区。

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

**注意**：脚本是创建模式，同名工程已存在会报 `project already exists`。要重建必须先删除
工程目录（会丢修订历史）。日常小改动用 RPC 增量修改，不要重跑脚本。

## 已知问题

**真实方向光阴影尚未启用。** 资产已经通过材质和几何避免黑色假阴影贴片，但
`ice3-engine` 的 WebGL2 阴影通道仍在修复中，因此当前画面没有建筑和池体的真实投影。

**池体尺寸硬编码。** 初沉池半径 13、二沉池半径 15 都写死在生成函数里，组件参数
（`ComponentParameter`）尚未使用。原因是 `ScalarBinding` 只支持字面量或参数名，
不支持 `radius - wall/2` 这类派生表达式，参数化需要先补齐表达式求值。
