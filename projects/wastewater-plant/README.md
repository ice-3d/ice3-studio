# 污水处理厂工程（wastewater-plant）

用 Studio 原生组件建的污水处理厂工程，几何、端口、管道全部由内核按参数生成，不引用任何外部 GLB。

## 打开

```bash
cd ice3-studio
STUDIO_SESSION_TOKEN=dev ./target/release/studio-cli serve \
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
| `revisions/<n>.json` | 每个修订一份完整场景快照 |
| `history/operations.jsonl` | 操作历史，逐行可重放 |
| `changesets/` | 待审变更集 |
| `cache/` | 几何与预览缓存，不入库，可重建 |
| `references/` | 外链资产，不入库 |

## 当前内容

5 台构筑物 + 4 条连接，是生化段的垂直切片：

| 节点 | 说明 |
|---|---|
| PC-101 | 初沉池，圆形，带中心进水筒、周边出水堰、刮泥桥、池顶栏杆、检修爬梯 |
| AE-101 | 曝气池（厌氧段），矩形廊道，带导流墙、曝气支管、检修走道 |
| AE-102 / AE-103 | 曝气池（缺氧段 / 好氧段），结构与 AE-101 同，进水介质为混合液 |
| SC-101 | 二沉池，圆形，与初沉池同构但直径更大 |
| 4 条连接 | 初沉 → 厌氧 → 缺氧 → 好氧 → 二沉，管道由连接意图路由 |

组件定义共 4 个：`primary-clarifier`、`aeration-feed`、`aeration-mixed`、`secondary-clarifier`。
曝气池拆成两种是因为首段进水是初沉出水、后续段是混合液，端口介质不同，校验会拒绝不匹配的连接。

## 扩展方式

新增单元时按同样模式：

1. 在 `scripts/build-wastewater-model.mjs` 里加一个构筑物生成函数
2. 用 `b.define()` 声明组件（带端口：名称、位置、朝向、介质、管径）
3. 用 `b.instance()` 摆放，用 `b.connect()` 连管
4. 重新生成或直接对现有工程下发 `component.define` + `instance.create`

## 已知问题

**修订快照是完整副本。** 每次修改新增一份约 94 KB 的快照，组件定义占其中约 78%，而组件在版本间几乎不变。改 100 次后 revisions/ 会到 9 MB 左右。

后续应改为：组件定义按文件拆分并被快照引用，或只保留最近 N 份快照、更早的靠 `history/operations.jsonl` 重放。
