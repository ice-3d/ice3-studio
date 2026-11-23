# Agent Gateway 端到端接线设计

> Document ID: ICE3-STUDIO-SPEC-039
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

## 1. 摘要

Studio 的公开 API、Agent 工具清单、ChangeSet 审批和候选预览已经完整，但 Web Viewer 没有
真实 Agent Gateway 配置或传输实现。`AgentPanel` 在常规启动中始终使用 unavailable
transport，`AgentStore.submit` 还把 project、revision 和 Selection 固定为空值。

本设计补上最小端到端闭环：

```text
Viewer intent + context
  → same-origin /agent/ws
  → Studio Server authenticated bridge
  → independent loopback Agent Gateway
  → public Studio API
  → ChangeSet
  → linked candidate
  → human review / approval / commit
```

Studio 仍不包含模型凭据、Prompt 或 Agent policy；外部 Gateway 继续拥有语言理解和规划。

## 2. 目标

- Viewer 能区分未配置、连接中、已连接和错误。
- Viewer 提交真实 project、revision、Selection、焦点对象和相机上下文。
- 显示 run 生命周期、消息、计划、工具状态、取消和错误。
- Gateway 链接 ChangeSet 后自动载入记录和候选，并引导人工审阅。
- Gateway token 不进入浏览器状态、消息或日志。
- 未配置 Gateway 时，对象浏览、人工编辑、ChangeSet 审阅和提交完全可用。
- 使用确定性 loopback Gateway 完成 Chrome E2E，不依赖真实模型服务。

## 3. 非目标

- 在 Studio 中存储或配置模型供应商凭据。
- 模型选择、Prompt 模板、Agent policy 编辑或多 Agent 编排。
- Agent 直接 commit；commit 继续需要人工审批。
- 跨设备远程 Gateway、云租户、多用户会话或权限模型。
- 完整聊天历史、多会话列表或离线消息持久化。
- 让 Viewer 理解 Gateway 内部工具调用细节或模型语义。
- 修改 Studio Agent API 的 tool authority、ChangeSet 或 revision 契约。

## 4. 架构边界

### 4.1 外部 Gateway

Gateway 是独立进程，可包含模型凭据和策略。它与 Studio 的唯一生产写入路径仍是公开
Studio API；Server bridge 不调用 Core、Project Store 或 Engine 私有路径。

### 4.2 Studio Server Bridge

Studio Server 增加同源 WebSocket `/agent/ws`。本地请求使用现有 session token 授权；
Server 再以 host-only 配置连接上游 Gateway。

配置：

```text
STUDIO_AGENT_GATEWAY_URL=ws://127.0.0.1:9130/agent
STUDIO_AGENT_GATEWAY_TOKEN=<host-only optional token>
```

本轮只允许 loopback `ws://` URL：

- host 必须为 `127.0.0.1`、`::1` 或 `localhost`；
- 拒绝 userinfo、fragment 和 query 中的凭据；
- token 只能来自环境变量；
- URL 和 token 不接受客户端请求覆盖。

### 4.3 Viewer

Viewer 只连接同源 `/agent/ws`，不直接连接 Gateway，因此不需要 CORS、Gateway URL 或
Gateway token。Connection 和 run 状态进入 `AgentStore`。

## 5. WebSocket 协议

协议为 JSON object per WebSocket text message，版本 `1`。

### 5.1 Viewer → Bridge

```json
{
  "type": "intent.submit",
  "protocolVersion": 1,
  "requestId": "request-000001",
  "intent": {
    "text": "Move the selected pump to clear the access road.",
    "projectId": "wastewater-plant",
    "revision": 12,
    "selectionIds": ["node-000123"],
    "focusNodeId": "node-000123",
    "camera": {
      "position": [45, 32, 55],
      "target": [0, 1, 0],
      "fov": 42
    }
  }
}
```

```json
{
  "type": "run.cancel",
  "protocolVersion": 1,
  "runId": "run-000001"
}
```

```json
{
  "type": "ping",
  "protocolVersion": 1
}
```

约束：

- `requestId` 1..256 字符；
- `text` 非空，最多 32 KiB；
- `projectId` 最多 256 字符；
- `selectionIds` 最多 64 项，每项最多 256 字符；
- 相机数值必须 finite，FOV 在 `(0, 179]`；
- 一条 intent 对应一个 run。

### 5.2 Bridge / Gateway → Viewer

```json
{ "type": "gateway.unavailable", "protocolVersion": 1, "message": "Gateway is not configured" }
```

```json
{ "type": "run.started", "protocolVersion": 1, "requestId": "request-000001", "runId": "run-000001" }
```

```json
{ "type": "message.delta", "protocolVersion": 1, "runId": "run-000001", "text": "Checking route clearance." }
```

```json
{ "type": "plan.updated", "protocolVersion": 1, "runId": "run-000001", "summary": "Inspect, propose, evaluate." }
```

```json
{
  "type": "tool.updated",
  "protocolVersion": 1,
  "runId": "run-000001",
  "toolName": "scene.search",
  "phase": "finished",
  "summary": "Found the selected pump"
}
```

```json
{
  "type": "changeset.linked",
  "protocolVersion": 1,
  "runId": "run-000001",
  "changeSetId": "changeset-000001",
  "entryIds": ["entry-000001"]
}
```

```json
{ "type": "run.finished", "protocolVersion": 1, "runId": "run-000001" }
```

```json
{
  "type": "run.failed",
  "protocolVersion": 1,
  "runId": "run-000001",
  "diagnosticCode": "ICE3_AGENT_RUN_FAILED",
  "message": "The proposal could not be evaluated"
}
```

Bridge 添加 `pong` 响应；未知消息类型失败关闭。

## 6. Server Bridge 限制与错误

| 项目 | 限制 |
|---|---:|
| 单条 WebSocket 消息 | 1 MiB |
| intent 文本 | 32 KiB |
| Selection IDs | 64 |
| 每秒消息 | 100 |
| 上游连接超时 | 5 秒 |
| 单次 connect handshake | 10 秒 |
| 连接最大时长 | 30 分钟 |
| 客户端队列 | 256 条 |
| stderr/日志中的 token | 永不写入 |

Bridge 行为：

- 未配置 URL：发送 `gateway.unavailable` 后关闭。
- URL 非法、非 loopback、包含 userinfo/query：启动配置错误，不尝试连接。
- 上游连接失败：发送稳定 unavailable/error 消息。
- 上游断开：关闭 Viewer socket，不自动重放已启动的 run。
- Viewer 断开：关闭对应上游 socket，避免孤儿会话。
- 上游或客户端超过限制：关闭连接并记录不含消息正文的 Diagnostic。
- Server 不持久化 Agent 消息、tool 输出或 run 历史。

## 7. AgentStore

`AgentSnapshot` 扩展为：

```text
connection: connecting | connected | unavailable | error
runStatus: idle | starting | running | finished | failed | cancelled
requestId
runId
messages
plan
tools
linkedChangeSetId
linkedEntryIds
lastContext
error
```

规则：

- 同一时间只有一个 active run。
- `intent.submit` 后状态进入 starting。
- `run.started` 关联 requestId 并进入 running。
- 事件 runId 与当前 run 不一致时忽略。
- 新 run 清空上一次消息、计划、工具和链接，但保留 connection。
- `message.delta` 追加流式文本；消息总预算为 64 KiB，超过后保留尾部并显示截断状态。
- `tool.updated` 按 runId、toolName 和顺序记录；最多 100 条。
- `changeset.linked` 保存 ChangeSet ID 和 entry IDs，不自动审批。
- `run.finished`、`run.failed`、`run.cancel` 更新终态。
- 连接错误不会清空已链接 ChangeSet；人工审阅继续。

## 8. 上下文与相机

Workbench 提交时创建不可变 context snapshot：

```text
projectId
revision
selectionIds
focusNodeId
camera position/target/fov
```

`focusName` 只保留在 Workbench 本地显示快照中，不写入 wire intent；Gateway 使用 stable
`focusNodeId`。相机数据通过 `SceneAdapter.getCameraContext()` 获取。

revision 前进后：

- 已完成 run 保留历史。
- 运行中的 run 不自动改绑新 revision。
- 后续 `changeset.linked` 仍可载入，但 ChangeSet 自身 stale 状态以 Server 为准。
- 用户下一次提交使用最新 revision。

## 9. ChangeSet 链接

新增 `ChangeSetStore.loadLinked(changeSetId)`：

1. 调用 `changeset.get` 读取记录；
2. 调用 `changeset.getCandidate` 读取已有候选，不重新 evaluate；
3. 更新 record、active、impact 可用时可读取；
4. candidateDigest 存在时获取 candidate preview；
5. 不自动 markReviewed、不自动 approve、不自动 commit。

Workbench 收到链接后切到 Changes tab，并显示 Agent 来源及候选审阅入口。ChangeSet 已
stale、blocked 或 discarded 时按现有状态展示，不尝试修复。

## 10. UI

Agent 面板状态：

| 状态 | 内容 |
|---|---|
| Connecting | 连接提示；人工工具可用 |
| Ready | 目标输入、上下文、提交按钮 |
| Running | 目标、Cancel、计划、消息、工具状态 |
| Finished | 最终状态和链接的 ChangeSet |
| Failed/Error | 错误、Retry、人工接手 |
| Unavailable | 未配置说明、人工接手、配置提示 |

`changeset.linked` 卡片显示 ChangeSet ID、状态、候选可审阅状态和 `Review changes`。
消息、计划和工具列表有界滚动；长内容不挤压编辑器。

在线且存在焦点对象时，面板在目标输入下方提供一个次要的 `Inspect selected object`
入口。它把上下文交还给人工参数编辑，但不削弱 `Run with Agent` 的主操作层级。移动端
顺序固定为 3D Scene → Agent/Review dock → Objects，让 Agent 工作流先于对象管理出现。

键盘：

- `Ctrl/Cmd+Enter` 提交当前 intent；
- Cancel 和 Review changes 可聚焦；
- 运行期间不重复提交；
- `aria-live` 只播报连接和 run 状态变化，不逐 token 播报。

## 11. 测试

### 11.1 Rust Server

- 未配置 Gateway 返回 unavailable。
- 配置校验拒绝非 loopback、userinfo、query credential。
- 认证缺失拒绝 `/agent/ws`。
- 双向消息转发和 ping/pong。
- Cancel 转发。
- 上游断开关闭下游。
- Viewer 断开清理上游。
- 超大消息、未知消息、超速率失败关闭。
- token 不出现在错误、日志或序列化响应中。

### 11.2 Viewer

- 真实 context 提交，不发送空 project/revision/selection。
- connection 和 run 状态迁移。
- 陈旧 run 事件忽略。
- message delta、plan、tool、link、finish、failed。
- Cancel。
- linked ChangeSet 载入不 evaluate、不 commit。
- Gateway 断线后人工工具保持可用。

### 11.3 Chrome E2E

E2E server 启动确定性 loopback Gateway：

1. 提交 intent；
2. Gateway 验证 project、revision、selection 和 camera；
3. 返回 started、plan、tool、message、ChangeSet link、finished；
4. Viewer 显示结果并切到 Changes；
5. 人工 Review candidate 和 Approve & commit；
6. 另一个测试覆盖 unavailable；
7. 另一个测试覆盖上游中途断开。

## 12. 代码落点

| 文件 | 变更 |
|---|---|
| `crates/studio-server/src/agent_bridge.rs` | Gateway 配置、WebSocket 认证、上游连接、消息限制 |
| `crates/studio-server/src/rpc.rs` | `/agent/ws` route 与共享 session token |
| `crates/studio-server/src/lib.rs` | bridge 模块与 ServerConfig |
| `crates/studio-cli/src/main.rs` | host config 环境变量与 serve 接线 |
| `apps/viewer/src/agent/transport.ts` | Gateway protocol 事件和 context 类型 |
| `apps/viewer/src/agent/WebSocketAgentTransport.ts` | 同源 socket、请求/run 关联、取消 |
| `apps/viewer/src/agent/AgentStore.ts` | 状态、事件、上下文和预算 |
| `apps/viewer/src/agent/AgentPanel.tsx` | 连接、运行、工具、错误和链接 UI |
| `apps/viewer/src/agent/AgentProvider.tsx` | Store 注入 |
| `apps/viewer/src/api/StudioClient.ts` | `changeset.getCandidate` |
| `apps/viewer/src/changeset/ChangeSetStore.ts` | `loadLinked` |
| `apps/viewer/src/app/Workbench.tsx` | 真实 context、docked ChangeSet、cancel |
| `apps/viewer/src/viewport/SceneAdapter.ts` | camera context |
| `scripts/run-viewer-e2e-server.mjs` | deterministic Gateway fixture |

## 13. 验收

- Viewer 普通启动时如实显示 connecting/unavailable，不伪造 ready。
- 配置 Gateway 后可完成意图、流式消息、工具状态、ChangeSet 链接和人工提交。
- 未配置或断线时人工编辑与审批无回归。
- Gateway token 不出现在浏览器、普通日志、错误响应或测试快照。
- 所有消息和运行状态有界。
- Chrome E2E 使用用户安装的 Google Chrome。
- 最终根级 `npm run verify` 全部通过。

## 14. 关联文档

- `ICE3-STUDIO-POL-001` — [仓库规则](../../AGENTS.md)
- `ICE3-STUDIO-ARC-001` — [Studio 架构](../architecture.md)
- `ICE3-STUDIO-SPEC-002` — [Studio Web Workbench](2026-10-30-studio-web-workbench-design.md)
- `ICE3-STUDIO-SPEC-014` — [Agent 主导工作台](2026-11-01-agent-led-workbench-design.md)
- `ICE3-STUDIO-GUIDE-005` — [MCP 适配器](../guides/mcp.md)
- `ICE3-STUDIO-REF-003` — [ChangeSet 审阅](../reference/change-sets.md)
