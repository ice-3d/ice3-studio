# 连接与管线附件检视设计

> Document ID: ICE3-STUDIO-SPEC-036
>
> Status: Proposed
>
> Owner: ICE3 Studio maintainers

## 1. 摘要

Studio 已把三通、异径管、穿墙套管交付为普通 Component Instance，并由 Core 校验端口、
路由、宿主和预算。当前缺口是公开读接口没有一个稳定、有界、revision-bound 的视图，
调用方无法一次取得以下同源事实：

- connection 的正式路由折线与解析后的端口世界坐标；
- 共享 `lineId` 的分段主管；
- 该主管上的三通、异径管和套管；
- 套管宿主；
- 与各段 `supportLineId` 对应的支撑站点。

本设计增加通用只读方法 `route.inspect`，并在 Viewer Inspector 中提供线路审查视图。
它不让 Viewer 解析组件表达式、重建路径或读取私有缓存；所有事实来自 Core 的正式端口
解析和路由实现。

## 2. 目标

- 按 `lineId` 返回完整的分段线路视图，而不是只返回调用方碰巧选中的一个 connection。
- 返回起止端口的 resolved position、direction、diameter 和 medium。
- 返回线路上的附件、套管宿主和支撑站点，使用稳定 node ID 关联。
- 对有界响应明确标记 truncation，不静默丢数据。
- 支持 expected revision 防陈旧读取。
- Viewer 从选中的 connection、附件或支撑对象进入 Inspector，显示线路关系和端口事实。
- Viewer 可点击相关 node ID 跳转到既有 Selection。
- Agent manifest、schema、TypeScript bindings、Reference 文档和测试同步更新。

## 3. 非目标

- 不新增写操作、自动布线或自动附件放置。
- 不返回或提交操作候选，不绕过 ChangeSet。
- 不在 Viewer 中重新实现端口表达式、路由折弯或支撑计算。
- 不返回完整 Document、组件定义或任意几何负载。
- 不复制 route safety 或 attachment diagnostics；诊断继续来自 `validation.run`。
- 不增加 industry-specific 的 Core 类型或字段。
- 不处理 Boolean、曲线 sweep、雕刻、协作或远程部署。

## 4. 设计原则

1. **同源解析。** `route.inspect` 必须调用 `resolve_port` 和 `route_connection`。
2. **线路优先。** 以 `lineId` 聚合，因为三通、异径管会把一条物理主管拆为多个 connection。
3. **只读且无副作用。** 方法不创建 revision、cache、preview、ChangeSet 或临时文件。
4. **稳定排序。** segments、attachments、supports 均按 node ID 排序，并限制响应大小。
5. **显式截断。** 达到预算时返回已有项并设置对应 truncation 标志。
6. **产品边界。** Viewer 只消费公开 API；Server 委托 Core 构造 inspection。

## 5. 公开 API

### 5.1 Method

`route.inspect` 是只读 JSON-RPC 方法。

请求：

```json
{
  "lineId": "L-RAS-HEADER-101",
  "revision": 1
}
```

| 字段 | 必填 | 约束 |
|---|---:|---|
| `lineId` | 是 | 非空字符串，最多 256 字符 |
| `revision` | 否 | 正整数；提供时必须等于当前 committed revision |

`revision` 不匹配返回 `ICE3_STUDIO_REVISION_CONFLICT`。`lineId` 没有对应 connection
返回 `ICE3_STUDIO_REFERENCE_INVALID`。正式路由无法解析返回
`ICE3_STUDIO_CONNECTION_INVALID`。

### 5.2 Response

```json
{
  "revision": 1,
  "lineId": "L-RAS-HEADER-101",
  "supportLineIds": [
    "L-RAS-HEADER-101-reducer-ras-a-large",
    "L-RAS-HEADER-101-reducer-ras-a-small"
  ],
  "segments": [
    {
      "nodeId": "node-000886",
      "name": "RWS-101 -> AE-102",
      "medium": "return-sludge",
      "diameter": 0.2875,
      "routePolicy": { "kind": "rack", "elevation": 6.5 },
      "from": {
        "nodeId": "node-000885",
        "port": "small",
        "medium": "return-sludge",
        "diameter": 0.25,
        "position": { "x": 425.4, "y": 6.5, "z": -3.0 },
        "direction": { "x": -1.0, "y": 0.0, "z": 0.0 }
      },
      "to": {
        "nodeId": "node-000164",
        "port": "return-inlet",
        "medium": "return-sludge",
        "diameter": 0.35,
        "position": { "x": 275.0, "y": 4.0, "z": 30.0 },
        "direction": { "x": 0.0, "y": 0.0, "z": 1.0 }
      },
      "points": [
        [425.4, 6.5, -3.0],
        [275.0, 6.5, -3.0],
        [275.0, 4.0, 30.0]
      ]
    }
  ],
  "attachments": [
    {
      "nodeId": "node-000885",
      "name": "RED-101",
      "role": "pipe-fitting",
      "fittingKind": "reducer",
      "lineId": "L-RAS-HEADER-101",
      "supportLineId": null,
      "hostNodeId": null,
      "hostName": null
    }
  ],
  "supports": [
    {
      "nodeId": "node-000948",
      "name": "support anchor",
      "supportLineId": "L-RAS-HEADER-101-reducer-ras-a-small",
      "supportGroup": "L-RAS-HEADER-101-reducer-ras-a-small-3",
      "position": { "x": 350.0, "y": 6.5, "z": -3.0 }
    }
  ],
  "truncated": {
    "segments": false,
    "points": false,
    "attachments": false,
    "supports": false
  }
}
```

响应上限：

| 项目 | 上限 |
|---|---:|
| segments | 128 |
| 所有 segment 的 points 总数 | 4,096 |
| attachments | 128 |
| supports | 512 |

排序规则：

- segments 按 connection node ID 升序；
- attachments 按 attachment node ID 升序；
- supports 按 support node ID 升序；
- `supportLineIds` 按字符串升序去重。

超过上限时按稳定顺序保留前 N 项，并将对应 `truncated` 标志设为 `true`。points 截断
发生在已纳入的 segment 内，Viewer 必须显著显示“partial route”。

### 5.3 Attachment 与 Support 识别

- Attachment：metadata `lineId` 与目标线路完全相等，且 metadata `role` 为
  `pipe-fitting` 或 `pipe-penetration` 的节点。
- Host：attachment metadata `hostNodeId` 指向的节点；返回宿主稳定 ID 与名称。
- Support：metadata `supportLineId` 属于响应 `supportLineIds` 集合的节点。没有
  `supportLineId` 的旧资产使用 metadata `lineId` 回退。
- Attachment 与 support 仍使用普通字符串 metadata；Core 不新增专用类型。

## 6. Core

新增 `studio-core::large_scene::route_inspection`：

```rust
pub fn inspect_route(
    document: &Document,
    definitions: &BTreeMap<ComponentId, &ComponentDefinition>,
    line_id: &str,
    expected_revision: Option<u64>,
    limits: RouteInspectionLimits,
) -> Result<RouteInspection, RouteInspectionError>;
```

职责：

1. 检查 expected revision；
2. 收集所有 metadata `lineId` 匹配的 Connection；
3. 按稳定 node ID 排序；
4. 对每个 segment 调用 `resolve_port` 与 `route_connection`；
5. 收集 attachments、hosts、supportLineIds 和 supports；
6. 应用响应预算并设置 truncation；
7. 返回纯序列化结构，不保存缓存或派生 Document。

Core 不重新运行 route safety 或 attachment validation。调用方需要诊断时继续调用
`validation.run`；这样 `route.inspect` 的职责保持为事实读取，不重复失败策略。

## 7. Server

`route.inspect` 加入现有只读 dispatch。方法只读取当前 session 的 committed Document 和
component definitions，不持有 preview worker，不触发 geometry evaluation。

`system.capabilities` 公布该方法；`system.agentManifest` 将它归入只读 inspect 工具，
并加入 `inspect` workflow。Agent authority、项目路径和 operation capability 边界不变。

API 版本从 `1.4` 升为 `1.5`。方法总数从 schema 39 / runtime 41 变为
schema 40 / runtime 42。

## 8. Viewer

### 8.1 Line identity

Viewer 从当前选中节点解析 `lineId`：

1. `node.metadata.lineId`；
2. `node.content.metadata.lineId`；
3. connection content 内的 metadata `lineId`。

前三者都不存在时不请求 `route.inspect`。支持对象使用 `supportLineId` 对应的线路名称
仍由 Document 中的 connection metadata 映射，Viewer 不猜测路由。

### 8.2 Route Inspector

Inspect panel 在名称和参数表单前后增加只读 `Route` 区域：

- revision、lineId、segment 数；
- 每段 medium、diameter、route policy 和起止端口；
- 端口直径、方向与世界坐标；
- attachment 类型、宿主名称和稳定 ID；
- support group 与位置；
- partial route 提示；
- 错误状态和 retry。

相关对象行可点击，通过现有 Selection API 选中对应 node。点击宿主、附件或支撑时，
Viewer 不改变路由视图，直到新选中对象自身包含可解析的 `lineId`。

### 8.3 State

line inspection 使用选中的 lineId 和 revision 作为 key。Selection 或 revision 改变时
取消旧请求的 UI 所有权；旧响应不得覆盖新选择。失败不阻塞普通 Inspect 表单或现有
ChangeSet 流程。

## 9. 安全与兼容

- 只读方法不接受路径、URL、表达式或 executable。
- 响应只包含 committed Document 中稳定 ID、名称、坐标和 metadata 选择结果。
- 不返回 workspace 绝对路径、token、cache 路径或未发布的 candidate。
- 现有 `document.get`、`scene.search`、`component.get` 和 Connection wire format 不变。
- 不含 `lineId` 的旧项目继续工作；只有显式调用 `route.inspect` 才返回错误。
- Viewer 原有的选择、ChangeSet、诊断跳转和参考资产流程保持兼容。

## 10. 测试

Core：

- 解析参数化端口并返回正式 route points；
- 同一 lineId 的三通/异径管分段全部返回；
- attachments、hosts、supports 按 node ID 稳定排序；
- expected revision 不匹配失败；
- 不存在的 lineId 失败；
- 超过 segment、point、attachment、support 上限时正确截断；
- 不存在 lineId 的 metadata 不影响 Document validation。

Server：

- `route.inspect` read authority 可用且不需要写 token；
- stale revision 返回稳定 Diagnostic；
- capabilities、Agent manifest、schema 和 TypeScript bindings 包含新方法；
- 响应不包含路径或 candidate 数据。

Viewer：

- 选中 attachment 和 connection 均可解析 lineId 并请求 inspection；
- stale response 不覆盖新 selection；
- partial route 和失败状态可见；
- 点击 attachment/host/support 调用 Selection，不直接修改 Document；
- Chrome E2E 选中实际污水厂附件后显示线路和端口。

## 11. 验收标准

- `route.inspect` 返回同源、bounded、revision-bound 的线路事实。
- 三通和异径管拆分的多个 segment 能在同一 lineId 下完整呈现。
- 套管显示真实宿主，支撑按 supportLineId 显示。
- Viewer 不解析组件表达式或重建路由。
- Agent 可通过公开 manifest 发现并使用新工具。
- Chrome 近景或对象选择流程可完成线路审查，Diagnostics 仍可定位对象。
- `cargo fmt`、clippy、Rust tests、schema check、Viewer typecheck/unit/build/E2E、
  `npm run benchmark:route-safety` 和根级 `npm run verify` 全部通过。
