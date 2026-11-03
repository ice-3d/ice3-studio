# 测量（Measurement）设计

> Document ID: ICE3-STUDIO-SPEC-026
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

## 1. 背景与目标

Studio 此前没有任何测量能力：Agent 无法回答"这两个设备相距多远/净空多少"，人在视口里
也只能靠肉眼估。本设计同时补齐两条通道：

- **API**：`scene.measure`，供 Agent 做尺寸查询；
- **Viewer**：测量工具，供人双选两个对象直接读距离。

非目标：任意网格点级测量（引擎 `raycast` 只返回节点，不返回命中点）、角度测量、
持久化尺寸标注。

## 2. 度量语义

每个端点（`MeasurePoint`）二选一：

```json
{ "nodeId": "node-000001" }
{ "point": [12.5, 0, -3.0] }
```

- `nodeId`：取该节点的**世界包围盒中心**作为端点，并保留包围盒用于间隙计算；
- `point`：直接使用世界坐标。

结果：

| 字段 | 含义 |
|---|---|
| `from` / `to` | 解析后的世界坐标 |
| `distance` | 两端点中心距 |
| `delta` | `to - from` 的轴向差 |
| `clearance` | 两个包围盒的**最近表面间隙**（轴向上逐轴取 `max(0, b.min-a.max, a.min-b.max)` 再求模；重叠为 0）；有任一端是自由点时等于 `distance` |

`clearance` 是净空判断真正需要的量，`distance` 只是中心距，两者都返回。

## 3. API

```
POST /rpc  scene.measure
{ "from": {"nodeId": "node-000001"}, "to": {"nodeId": "node-000010"} }
```

- 只读方法，写权限不要求；`review-required` 与 `autonomous` 下都可用。
- `agentManifest` 中登记为 `Read` 工具，参数/返回类型分别是 `MeasureRequest` /
  `MeasureResult`。
- 实现：`studio_core::agent_contract::measure_request`，复用 `evaluate_session_scene`
  的世界 bounds，不返回几何。

## 4. Viewer 工具

`apps/viewer/src/viewport/MeasureTool.ts`：

1. 视口左上角的 **Measure** / **Clear** 按钮，纯 DOM 叠加，不进入 React 树，避免与
   引擎追加的 canvas 冲突；
2. 开启后点击两个对象（复用引擎 `SELECTION_CHANGE`），第一次提示 "Pick the second
   object"；
3. 用 `SceneAdapter.measureBetween` 取两个实体的世界 bounds，`measureBounds` 计算
   `distance`/`clearance`，再用 `engine.project` 把两个中心投到屏幕，画虚线 + 端点 +
   标签（`12.35 m · gap 2.00 m`）；
4. 测量存在时以 rAF 持续重投影，相机移动后线段跟随；任一端点离开视锥时隐藏；
5. 客户端计算，不依赖服务端，离线也能用。

纯函数 `src/viewport/measure.ts::measureBounds` 与 API 的语义一致，由
`apps/viewer/tests/measure.test.ts` 覆盖。

## 5. 代码落点

| 文件 | 变更 |
|---|---|
| `crates/studio-core/src/agent_contract.rs` | `MeasurePoint` / `MeasureRequest` / `MeasureResult` + `measure_request` |
| `crates/studio-server/src/rpc.rs` | `scene.measure` 能力与处理 |
| `crates/studio-server/src/agent.rs` | Agent 工具登记 |
| `crates/studio-cli/src/schema.rs` | 方法与类型加入固定 schema |
| `apps/viewer/src/viewport/measure.ts` | 纯度量函数 |
| `apps/viewer/src/viewport/SceneAdapter.ts` | `measureBetween` / `projectPoint` / `addSelectionListener` |
| `apps/viewer/src/viewport/MeasureTool.ts` | 视口测量工具 |
| `apps/viewer/src/viewport/Viewport.tsx` | 挂载/释放工具 |
| `apps/viewer/src/app/studioTokens.css` | 工具栏、叠加线、标签样式 |

## 6. 验收

- API：相邻盒（中心距 3、间隙 2）、重叠盒（间隙 0）、点对点（3-4-5）、节点+点、
  未知节点/双端设置/双端为空均按预期；`{"from":{"nodeId":...},"to":{"point":[...]}}`
  JSON 解析通过。
- Viewer：`measureBounds` 单测（中心距、间隙、重叠）与 `formatMetres` 通过；
  typecheck、构建、e2e 通过。
- schema `--check` 通过；方法清单 39（运行时 41）。
