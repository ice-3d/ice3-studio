# 污水处理厂工程（wastewater-plant）

用 Studio 原生组件建的污水处理厂工程。几何、端口、管道全部由内核按参数生成，不引用任何外部 GLB。

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
| `changesets/` | 待审变更集 |
| `cache/` | 几何与预览缓存，不入库，可重建 |
| `references/` | 外链资产，不入库 |

## 工艺链

沿 x 轴自西向东，11 台构筑物、9 条连接：

| 编号 | 构筑物 | 位置 | 主要构造 |
|---|---|---|---|
| GR-101 | 进水格栅渠 | -135 | 两道格栅各 13 根栅条，检修走道与栏杆 |
| PS-101 | 提升泵房 | -105 | 门洞、通风百叶、三台立式泵（底座/泵体/电机/出水管） |
| GC-101 | 曝气沉砂池 | -70 | 阶梯砂斗、刮砂桥与驱动、8 道曝气支管 |
| PC-101 | 初沉池 | 0 | 圆形，中心进水筒、周边出水堰、刮泥桥、环形栏杆、检修爬梯 |
| AE-101 | 曝气池（厌氧段） | 45 | 矩形廊道，三段导流墙、10 道曝气支管、检修走道 |
| AE-102 | 曝气池（缺氧段） | 92 | 同上 |
| AE-103 | 曝气池（好氧段） | 139 | 同上 |
| SC-101 | 二沉池 | 200 | 圆形，直径大于初沉池 |
| FL-101 | 滤池 | 258 | 4 格分仓，各带滤料层、水面与反冲洗配管 |
| UV-101 | 紫外消毒渠 | 302 | 3 组灯架共 12 支灯管 |
| BLW-101 | 鼓风机房 | 60 / z=-34 | 单层厂房 |
| CHEM-101 | 加药间 | 210 / z=-32 | 两层厂房 |
| CTRL-101 | 中控楼 | -135 / z=-30 | 三层厂房，带入口雨棚 |

### 端口介质

管道由连接意图路由，介质必须与端口一致，否则校验拒绝：

```
raw-water → screened-water → lifted-water → degritted-water
→ settled-water → mixed-liquor → secondary-effluent
→ filtered-water → final-effluent
```

曝气池因此拆成 `aeration-feed`（接初沉出水）和 `aeration-mixed`（接混合液）两种定义。

## 修订存储

组件定义不写在快照里，按内容摘要单独存成 `components/<id>-<digest>.json`，快照只记
`{ id, digest }`。组件没改动时不产生新文件。

| | 每次修订 |
|---|---|
| 拆分前 | 95 KB |
| 拆分后 | 40 KB |

摘要取组件内容 sha256 前 12 位，「改了又改回去」会命中已有文件。

## 扩展方式

增删单元时改 `scripts/build-wastewater-model.mjs`，然后重新生成：

```bash
node scripts/build-wastewater-model.mjs --workspace ./projects
```

注意：脚本是**创建**模式，同名工程已存在时会报 `project already exists`。要重建需先删除
工程目录（会丢失修订历史）。日常小改动用 RPC 增量修改，不要重跑脚本。

## 待办

- 多系列并联：初沉、生化、二沉都应按系列复制（当前每类只有一座）
- 污泥线：浓缩池、消化池、脱水机房
- 池体尺寸参数化：目前尺寸硬编码在生成函数里，改为组件参数后可批量变径
