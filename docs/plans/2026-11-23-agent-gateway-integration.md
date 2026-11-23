# Agent Gateway 端到端接线 Implementation Plan

> Document ID: ICE3-STUDIO-PLAN-036
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans. This repository prohibits subagents unless the user explicitly enables them; execute natively.

**Goal:** 让 Viewer 通过受控同源桥提交真实工程上下文、显示 Gateway 流式运行与工具状态、取消运行，并自动载入 Agent 链接的 ChangeSet，同时把 Agent 面板重构成真正的 AI-first 任务控制台。

**Architecture:** Studio Server 新增 authenticated `/agent/ws` 桥，按 host-only 配置连接独立 loopback Gateway；Viewer 只连接同源 socket。`AgentStore` 管理连接、单 run、消息/计划/工具预算和 ChangeSet 链接；Workbench 提供真实上下文并载入候选，审批仍由人工完成。

**Tech Stack:** Rust 2024、axum WebSocket、tokio、tokio-tungstenite、React 19.3.1、TypeScript 7.0.2、Playwright 1.64.1、Google Chrome。

**Spec:** [`docs/specs/2026-11-23-agent-gateway-integration-design.md`](../specs/2026-11-23-agent-gateway-integration-design.md)

## Global Constraints

- Gateway 是独立进程；Studio 不存储模型凭据、Prompt 或 Agent policy。
- 本轮只允许 host 配置的 loopback `ws://` Gateway。
- Gateway token 只存在于 Studio Server，不进入浏览器、日志、Diagnostic 或测试快照。
- `/agent/ws` 必须使用现有 local session token 授权。
- Agent 不能 commit；ChangeSet 审批和提交必须由人完成。
- 单条消息上限 1 MiB，intent 文本上限 32 KiB，Selection 上限 64。
- 消息、计划、工具状态、连接和 ChangeSet 链接都必须有界。
- 未配置或断线时，对象搜索、Inspect、Changes 和人工提交保持可用。
- Chrome 验证使用用户安装的 Google Chrome。
- E2E 使用确定性 loopback Gateway，不调用真实模型服务。
- 开发在功能分支；最终根级 `npm run verify` 必须通过。

## Review Focus

- Gateway token、完整 upstream URL 或消息正文不能通过 Viewer、错误响应或日志泄漏。
- `/agent/ws` 认证、上游失败、下游断开和超时必须清理 socket，不能留下后台任务。
- 旧 request/run 的迟到事件不能覆盖新 run；取消后不能重新进入 running。
- 流式消息和工具事件必须有界，不能因长消息或无限制 delta 阻塞界面。
- Agent-first UI 必须让目标输入和当前运行成为主视觉路径，同时离线时人工入口仍然清楚。

---

## File Structure

| 文件 | 责任 |
|---|---|
| `crates/studio-server/src/agent_bridge.rs` | URL/token 配置、协议校验、双向 relay、超时和限制 |
| `crates/studio-server/src/rpc.rs` | `/agent/ws` route、AppState 配置和认证 |
| `crates/studio-server/src/lib.rs` | 导出 bridge 配置类型 |
| `crates/studio-cli/src/main.rs` | `STUDIO_AGENT_GATEWAY_*` 环境变量接线 |
| `apps/viewer/src/agent/agentProtocol.ts` | wire parser、事件 normalize 和 context 类型 |
| `apps/viewer/src/agent/WebSocketAgentTransport.ts` | 同源 WebSocket 管理、提交、取消和事件分发 |
| `apps/viewer/src/agent/AgentStore.ts` | connection/run/status、消息预算、工具和链接 |
| `apps/viewer/src/agent/AgentProvider.tsx` | Store 注入 |
| `apps/viewer/src/agent/AgentPanel.tsx` | AI-first 任务控制台 |
| `apps/viewer/src/api/StudioClient.ts` | `changeset.getCandidate` |
| `apps/viewer/src/changeset/ChangeSetStore.ts` | `loadLinked` |
| `apps/viewer/src/viewport/SceneAdapter.ts` | camera context |
| `apps/viewer/src/app/Workbench.tsx` | 真实上下文、链接 ChangeSet、取消 |

---

### Task 1: Gateway 配置与 Server 接线

**Files:**
- Create: `crates/studio-server/src/agent_bridge.rs`
- Modify: `crates/studio-server/src/lib.rs`
- Modify: `crates/studio-server/src/rpc.rs`
- Modify: `crates/studio-cli/src/main.rs`
- Modify: `crates/studio-server/Cargo.toml`
- Test: `crates/studio-server/src/agent_bridge.rs` unit tests

**Interfaces:**
- Consumes: `STUDIO_AGENT_GATEWAY_URL`、`STUDIO_AGENT_GATEWAY_TOKEN`
- Produces:
  - `AgentGatewayConfig { url: String, token: Option<String> }`
  - `AgentGatewayConfig::from_env() -> Result<Option<Self>, AgentBridgeError>`
  - `AgentBridgeError`
  - `ServerConfig.agent_gateway: Option<AgentGatewayConfig>`

- [ ] **Step 1: Write failing config validation tests**

Add to `agent_bridge.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_loopback_ws_urls() {
        for url in [
            "ws://127.0.0.1:9130/agent",
            "ws://localhost:9130/agent",
            "ws://[::1]:9130/agent",
        ] {
            assert!(validate_gateway_url(url).is_ok(), "{url}");
        }
    }

    #[test]
    fn rejects_non_loopback_credentials_and_queries() {
        for url in [
            "wss://127.0.0.1:9130/agent",
            "ws://example.com/agent",
            "ws://user:pass@127.0.0.1:9130/agent",
            "ws://127.0.0.1:9130/agent?token=secret",
            "ws://127.0.0.1:9130/agent#fragment",
        ] {
            assert!(validate_gateway_url(url).is_err(), "{url}");
        }
    }

    #[test]
    fn error_display_never_contains_the_token() {
        let error = AgentBridgeError::Upstream("Authorization: Bearer secret-token".into());
        assert!(!error.public_message().contains("secret-token"));
    }
}
```

- [ ] **Step 2: Run the tests and verify they fail**

```bash
cargo test -p studio-server --lib agent_bridge::tests
```

Expected: FAIL because `agent_bridge` and validation functions do not exist.

- [ ] **Step 3: Add bridge config and URL validation**

Create `crates/studio-server/src/agent_bridge.rs`:

```rust
use std::str::FromStr;

use axum::http::Uri;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentGatewayConfig {
    pub url: String,
    #[serde(skip_serializing)]
    pub token: Option<String>,
}

impl AgentGatewayConfig {
    pub fn new(url: impl Into<String>, token: Option<String>) -> Result<Self, AgentBridgeError> {
        let url = url.into();
        validate_gateway_url(&url)?;
        Ok(Self { url, token })
    }

    pub fn from_env() -> Result<Option<Self>, AgentBridgeError> {
        let Some(url) = std::env::var("STUDIO_AGENT_GATEWAY_URL").ok() else {
            return Ok(None);
        };
        Self::new(url, std::env::var("STUDIO_AGENT_GATEWAY_TOKEN").ok()).map(Some)
    }
}

#[derive(Debug, Error)]
pub enum AgentBridgeError {
    #[error("agent gateway configuration is invalid")]
    Config,
    #[error("agent gateway is unavailable")]
    Unavailable,
    #[error("agent gateway failed")]
    Upstream(String),
    #[error("agent protocol message is invalid")]
    Protocol(String),
}

impl AgentBridgeError {
    pub fn public_message(&self) -> &'static str {
        match self {
            Self::Config => "Agent Gateway configuration is invalid",
            Self::Unavailable => "Agent Gateway is unavailable",
            Self::Upstream(_) => "Agent Gateway connection failed",
            Self::Protocol(_) => "Agent Gateway protocol error",
        }
    }
}

pub fn validate_gateway_url(value: &str) -> Result<(), AgentBridgeError> {
    let uri = Uri::from_str(value).map_err(|_| AgentBridgeError::Config)?;
    if uri.scheme_str() != Some("ws") || uri.query().is_some() || value.contains('#') {
        return Err(AgentBridgeError::Config);
    }
    let authority = uri.authority().ok_or(AgentBridgeError::Config)?;
    if authority.as_str().contains('@') {
        return Err(AgentBridgeError::Config);
    }
    let host = authority.host();
    if !matches!(host, "127.0.0.1" | "::1" | "localhost") {
        return Err(AgentBridgeError::Config);
    }
    Ok(())
}
```

- [ ] **Step 4: Add ServerConfig and CLI wiring**

Add to `ServerConfig`:

```rust
pub agent_gateway: Option<crate::AgentGatewayConfig>,
```

Set `None` in `for_tests` and `review_required`, and wire CLI:

```rust
agent_gateway: crate::agent_bridge::AgentGatewayConfig::from_env()?,
```

Export from `lib.rs`:

```rust
pub use agent_bridge::{AgentBridgeError, AgentGatewayConfig};
```

Add `futures-util.workspace = true` and move `tokio-tungstenite.workspace = true` to normal dependencies.

- [ ] **Step 5: Run config tests and compile**

```bash
cargo test -p studio-server --lib agent_bridge::tests
cargo check -p studio-server -p studio-cli
```

Expected: 3 tests pass; crates compile.

- [ ] **Step 6: Commit**

```bash
git add crates/studio-server/src/agent_bridge.rs crates/studio-server/src/lib.rs crates/studio-server/src/rpc.rs crates/studio-server/Cargo.toml crates/studio-cli/src/main.rs
git commit -m "feat(agent): configure loopback gateway bridge"
```

---

### Task 2: Authenticated Agent WebSocket Relay

**Files:**
- Modify: `crates/studio-server/src/agent_bridge.rs`
- Modify: `crates/studio-server/src/rpc.rs`
- Test: `crates/studio-server/tests/agent_bridge.rs`

**Interfaces:**
- Consumes: `AgentGatewayConfig`
- Produces:
  - `/agent/ws?token=<session-token>`
  - `run_agent_bridge(socket, config)`
  - Unavailable message:
    `{"type":"gateway.unavailable","protocolVersion":1,"message":"..."}`
  - Ping → pong same connection.

- [ ] **Step 1: Write failing integration tests**

Create `crates/studio-server/tests/agent_bridge.rs` with a deterministic upstream:

```rust
use std::time::Duration;

use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::Response,
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use studio_server::{ServerConfig, start_test_server};
use tokio::net::TcpListener;
use tokio_tungstenite::{connect_async, tungstenite};

async fn start_upstream() -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route("/agent", get(|upgrade: WebSocketUpgrade| async move {
                upgrade.on_upgrade(upstream)
            })),
        ).await.unwrap();
    });
    (format!("ws://127.0.0.1:{port}/agent"), task)
}

async fn upstream(mut socket: WebSocket) {
    while let Some(Ok(message)) = socket.recv().await {
        let Message::Text(text) = message else { continue };
        let value: Value = serde_json::from_str(text.as_str()).unwrap();
        match value["type"].as_str() {
            Some("ping") => {
                socket.send(Message::Text(json!({
                    "type": "pong",
                    "protocolVersion": 1
                }).to_string().into())).await.unwrap();
            }
            Some("intent.submit") => {
                socket.send(Message::Text(json!({
                    "type": "run.started",
                    "protocolVersion": 1,
                    "requestId": value["requestId"],
                    "runId": "run-1"
                }).to_string().into())).await.unwrap();
            }
            _ => {}
        }
    }
}

#[tokio::test]
async fn relays_ping_and_intent_through_authenticated_bridge() {
    let (url, upstream_task) = start_upstream().await;
    let mut config = ServerConfig::for_tests("bridge");
    config.agent_gateway = Some(
        studio_server::AgentGatewayConfig::new(url, None).unwrap()
    );
    let server = start_test_server(config).await;
    let ws_url = format!(
        "{}/agent/ws?token={}",
        server.base_url().replacen("http://", "ws://", 1),
        server.token()
    );
    let (mut socket, _) = connect_async(ws_url).await.unwrap();
    socket.send(tungstenite::Message::Text(
        json!({"type":"ping","protocolVersion":1}).to_string().into()
    )).await.unwrap();
    let pong = tokio::time::timeout(Duration::from_secs(2), socket.next())
        .await.unwrap().unwrap().unwrap();
    assert!(pong.to_text().unwrap().contains("\"type\":\"pong\""));

    socket.send(tungstenite::Message::Text(json!({
        "type":"intent.submit",
        "protocolVersion":1,
        "requestId":"request-1",
        "intent":{
            "text":"Move the pump",
            "projectId":"bridge",
            "revision":1,
            "selectionIds":["node-1"],
            "focusNodeId":"node-1",
            "camera":{"position":[1,2,3],"target":[0,0,0],"fov":42}
        }
    }).to_string().into())).await.unwrap();
    let started = tokio::time::timeout(Duration::from_secs(2), socket.next())
        .await.unwrap().unwrap().unwrap();
    assert!(started.to_text().unwrap().contains("\"runId\":\"run-1\""));
    upstream_task.abort();
}

#[tokio::test]
async fn unconfigured_gateway_reports_unavailable() {
    let config = ServerConfig::for_tests("unconfigured");
    let server = start_test_server(config).await;
    let ws_url = format!(
        "{}/agent/ws?token={}",
        server.base_url().replacen("http://", "ws://", 1),
        server.token()
    );
    let (mut socket, _) = connect_async(ws_url).await.unwrap();
    let event = socket.next().await.unwrap().unwrap();
    assert!(event.to_text().unwrap().contains("gateway.unavailable"));
}
```

Add a server accessor:

```rust
pub fn token(&self) -> &str {
    &self.token
}
```

- [ ] **Step 2: Run the tests and verify they fail**

```bash
cargo test -p studio-server --test agent_bridge
```

Expected: FAIL because `/agent/ws` does not exist.

- [ ] **Step 3: Add authenticated route**

In `build_router`, add:

```rust
.route("/agent/ws", get(agent_websocket))
```

In `rpc.rs`:

```rust
async fn agent_websocket(
    State(state): State<AppState>,
    Query(query): Query<HttpToken>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    let (authorized, config) = {
        let session = state.session.lock().expect("session lock");
        (
            query.token.as_deref() == Some(session.token.as_str())
                || bearer_token(&headers) == Some(session.token.as_str()),
            state.agent_gateway.clone(),
        )
    };
    if !authorized {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    upgrade
        .max_message_size(crate::agent_bridge::MAX_AGENT_MESSAGE_BYTES)
        .on_upgrade(move |socket| crate::agent_bridge::run(socket, config))
}
```

`AppState` gains:

```rust
agent_gateway: Option<AgentGatewayConfig>
```

`build_state`/`serve` pass `config.agent_gateway`.

- [ ] **Step 4: Implement unavailable and upstream relay**

In `agent_bridge.rs`:

```rust
pub const MAX_AGENT_MESSAGE_BYTES: usize = 1024 * 1024;
const MAX_CONNECTION: Duration = Duration::from_secs(30 * 60);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

pub async fn run(mut downstream: axum::extract::ws::WebSocket, config: Option<AgentGatewayConfig>) {
    let Some(config) = config else {
        let _ = downstream.send(axum::extract::ws::Message::Text(
            serde_json::json!({
                "type":"gateway.unavailable",
                "protocolVersion":1,
                "message":"Agent Gateway is not configured"
            }).to_string().into()
        )).await;
        let _ = downstream.close().await;
        return;
    };
    let result = tokio::time::timeout(MAX_CONNECTION, relay(&mut downstream, config)).await;
    if result.is_err() {
        let _ = downstream.close().await;
    }
}

async fn relay(
    downstream: &mut axum::extract::ws::WebSocket,
    config: AgentGatewayConfig,
) -> Result<(), AgentBridgeError> {
    let request = upstream_request(&config)?;
    let (upstream, _) = tokio::time::timeout(CONNECT_TIMEOUT, tokio_tungstenite::connect_async(request))
        .await
        .map_err(|_| AgentBridgeError::Unavailable)?
        .map_err(|_| AgentBridgeError::Unavailable)?;
    let (mut downstream_tx, mut downstream_rx) = downstream.split();
    let (mut upstream_tx, mut upstream_rx) = upstream.split();
    loop {
        tokio::select! {
            message = downstream_rx.next() => relay_downstream_to_upstream(message, &mut upstream_tx).await?,
            message = upstream_rx.next() => relay_upstream_to_downstream(message, &mut downstream_tx).await?,
        }
    }
}
```

Implement `upstream_request` with `IntoClientRequest`, adding host-only
`Authorization: Bearer <token>`. Validate every outbound/inbound JSON:

- known `type`;
- `protocolVersion == 1`;
- 1 MiB text limit;
- ping → pong locally;
- unknown type → `AgentBridgeError::Protocol`.

- [ ] **Step 5: Add authentication and oversized-message tests**

In `agent_bridge.rs` tests:

```rust
#[tokio::test]
async fn rejects_missing_session_token() {
    let server = start_test_server(ServerConfig::for_tests("auth")).await;
    let result = connect_async(format!(
        "{}/agent/ws",
        server.base_url().replacen("http://", "ws://", 1)
    )).await;
    assert!(result.is_err());
}
```

Add a unit validation test for an over-1-MiB text frame and unknown protocol type.

- [ ] **Step 6: Run Rust tests**

```bash
cargo test -p studio-server --test agent_bridge
cargo test -p studio-server --lib agent_bridge::tests
```

Expected: all pass.

- [ ] **Step 7: Commit**

```bash
git add crates/studio-server/src/agent_bridge.rs crates/studio-server/src/rpc.rs crates/studio-server/tests/agent_bridge.rs
git commit -m "feat(agent): relay authenticated gateway sessions"
```

---

### Task 3: Viewer Agent Protocol and WebSocket Transport

**Files:**
- Create: `apps/viewer/src/agent/agentProtocol.ts`
- Create: `apps/viewer/src/agent/WebSocketAgentTransport.ts`
- Modify: `apps/viewer/src/agent/transport.ts`
- Test: `apps/viewer/tests/agent-protocol.test.ts`
- Test: `apps/viewer/tests/websocket-agent-transport.test.ts`

**Interfaces:**
- Consumes: `/agent/ws`
- Produces:
  - `AgentContext`
  - `parseAgentServerMessage(value: unknown): AgentServerMessage | null`
  - `serializeAgentClientMessage(message: AgentClientMessage): string`
  - `createWebSocketAgentTransport(options): AgentTransport`
  - Transport `submitIntent` returns `{ requestId }`; it never fabricates a run ID.
  - Local transport event `connection.updated` carries connecting/connected/unavailable/error.

- [ ] **Step 1: Write failing protocol tests**

Create `agent-protocol.test.ts`:

```ts
import assert from 'node:assert/strict';
import test from 'node:test';
import {
  parseAgentServerMessage,
  serializeAgentClientMessage,
} from '../src/agent/agentProtocol';

test('serializes intent with the current context and no display-name fields', () => {
  const text = serializeAgentClientMessage({
    type: 'intent.submit',
    protocolVersion: 1,
    requestId: 'request-1',
    intent: {
      text: 'Move the pump',
      projectId: 'plant',
      revision: 3,
      selectionIds: ['node-1'],
      focusNodeId: 'node-1',
      camera: { position: [1, 2, 3], target: [0, 0, 0], fov: 42 },
    },
  });
  assert.match(text, /"projectId":"plant"/);
  assert.doesNotMatch(text, /focusName/);
});

test('parses known events and rejects unknown or wrong-version messages', () => {
  assert.deepEqual(
    parseAgentServerMessage({
      type: 'run.started',
      protocolVersion: 1,
      requestId: 'request-1',
      runId: 'run-1',
    }),
    {
      type: 'run.started',
      protocolVersion: 1,
      requestId: 'request-1',
      runId: 'run-1',
    },
  );
  assert.equal(parseAgentServerMessage({
    type: 'unknown',
    protocolVersion: 1,
  }), null);
  assert.equal(parseAgentServerMessage({
    type: 'run.started',
    protocolVersion: 2,
    requestId: 'request-1',
    runId: 'run-1',
  }), null);
});
```

- [ ] **Step 2: Run protocol tests and verify they fail**

```bash
cd apps/viewer
node esbuild-tests.mjs
node --test test-dist/agent-protocol.test.js
```

Expected: FAIL because files do not exist.

- [ ] **Step 3: Define protocol types and parsers**

Modify `transport.ts`, keeping `AgentEvent` compatible, and add in
`agentProtocol.ts`:

```ts
export type AgentCameraContext = {
  position: [number, number, number];
  target: [number, number, number];
  fov: number;
};

export type AgentContext = {
  projectId: string;
  revision: number;
  selectionIds: string[];
  focusNodeId: string | null;
  camera: AgentCameraContext;
};

export type AgentServerMessage =
  | AgentEvent
  | { type: 'gateway.unavailable'; protocolVersion: 1; message: string }
  | { type: 'pong'; protocolVersion: 1 };

export function parseAgentServerMessage(value: unknown): AgentServerMessage | null {
  if (!isRecord(value) || value.protocolVersion !== 1 || typeof value.type !== 'string') {
    return null;
  }
  if (value.type === 'gateway.unavailable') {
    return typeof value.message === 'string'
      ? { type: value.type, protocolVersion: 1, message: value.message }
      : null;
  }
  return parseKnownAgentEvent(value);
}
```

`parseKnownAgentEvent` validates every field and accepts `tool.updated`.

- [ ] **Step 4: Implement WebSocket transport with injectable socket**

```ts
export function createWebSocketAgentTransport(options: {
  token: string;
  WebSocketImpl?: typeof WebSocket;
}): AgentTransport {
  const WebSocketImpl = options.WebSocketImpl ?? WebSocket;
  const listeners = new Set<(event: AgentEvent) => void>();
  let socket: WebSocket | null = null;
  let connection: 'connecting' | 'connected' | 'unavailable' | 'error' = 'connecting';
  const pending: string[] = [];
  const emit = (event: AgentEvent): void => {
    for (const listener of listeners) listener(event);
  };
  const updateConnection = (
    next: 'connecting' | 'connected' | 'unavailable' | 'error',
  ): void => {
    connection = next;
    emit({ type: 'connection.updated', connection: next });
  };
  return {
    get connection() { return connection; },
    async submitIntent(intent) {
      ensureSocket();
      const requestId = crypto.randomUUID();
      const message = serializeAgentClientMessage({
        type: 'intent.submit',
        protocolVersion: 1,
        requestId,
        intent,
      });
      if (socket!.readyState === WebSocket.OPEN) socket!.send(message);
      else pending.push(message);
      return { requestId };
    },
    async cancel(runId) {
      socket?.send(serializeAgentClientMessage({
        type: 'run.cancel',
        protocolVersion: 1,
        runId,
      }));
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}
```

The transport drains `pending` on `open`, sets connected, and turns `close` or malformed
protocol data into connection/error events. `AgentStore` waits for `run.started` and
correlates it with the returned `requestId`.

- [ ] **Step 5: Add fake-socket transport tests**

Create `websocket-agent-transport.test.ts` with a fake `WebSocketImpl` that
records sends and can emit JSON events. Verify:

- intent context is serialized;
- messages sent before `open` are queued and flushed once;
- run.started is dispatched;
- cancel sends run ID;
- malformed message is ignored;
- unavailable event sets connection unavailable.

- [ ] **Step 6: Run focused tests and typecheck**

```bash
cd apps/viewer
node esbuild-tests.mjs
node --test test-dist/agent-protocol.test.js test-dist/websocket-agent-transport.test.js
npm run typecheck
```

Expected: all pass.

- [ ] **Step 7: Commit**

```bash
git add apps/viewer/src/agent/agentProtocol.ts apps/viewer/src/agent/WebSocketAgentTransport.ts apps/viewer/src/agent/transport.ts apps/viewer/tests/agent-protocol.test.ts apps/viewer/tests/websocket-agent-transport.test.ts
git commit -m "feat(agent): add viewer websocket protocol transport"
```

---

### Task 4: AgentStore Context, Run State and Bounds

**Files:**
- Modify: `apps/viewer/src/agent/AgentStore.ts`
- Modify: `apps/viewer/tests/agent-store.test.ts`

**Interfaces:**
- Consumes: `AgentContext`、`AgentTransport`
- Produces:
  - `AgentSnapshot` with connection/run/messages/plan/tools/link/context
  - `AgentTransport.submitIntent(...): Promise<{ requestId: string }>`
  - `submit(input: { text: string; context: AgentContext }): Promise<void>`
  - `cancel(): Promise<void>`
  - `retry(): Promise<void>`
  - `dismissError(): void`

- [ ] **Step 1: Replace outdated tests with failing state tests**

Add tests:

```ts
test('submits the latest context instead of empty defaults', async () => {
  let received: unknown;
  const transport = createConnectedTransport({
    submitIntent: async (intent) => {
      received = intent;
      return { requestId: 'request-1' };
    },
  });
  const store = new AgentStore(transport);
  await store.submit({
    text: 'Move the pump',
    context: {
      projectId: 'plant',
      revision: 7,
      selectionIds: ['node-7'],
      focusNodeId: 'node-7',
      camera: { position: [1, 2, 3], target: [0, 0, 0], fov: 42 },
    },
  });
  assert.deepEqual(received, {
    text: 'Move the pump',
    projectId: 'plant',
    revision: 7,
    selectionIds: ['node-7'],
    focusNodeId: 'node-7',
    camera: { position: [1, 2, 3], target: [0, 0, 0], fov: 42 },
  });
});

test('ignores events from a previous run', () => {
  const transport = createManualTransport();
  const store = new AgentStore(transport);
  transport.emit({ type: 'run.started', runId: 'run-1' });
  transport.emit({ type: 'plan.updated', runId: 'run-old', summary: 'Old plan' });
  assert.equal(store.snapshot.plan, null);
});
```

Add tests for message budget, tool budget, cancel, failed run and linked ChangeSet.

- [ ] **Step 2: Run tests and verify they fail**

```bash
cd apps/viewer
node esbuild-tests.mjs
node --test test-dist/agent-store.test.js
```

Expected: FAIL on missing types/methods.

- [ ] **Step 3: Implement store state machine**

```ts
export type AgentSnapshot = {
  connection: 'connecting' | 'connected' | 'unavailable' | 'error';
  runStatus: 'idle' | 'starting' | 'running' | 'finished' | 'failed' | 'cancelled';
  requestId: string | null;
  runId: string | null;
  messages: string[];
  plan: string | null;
  tools: AgentToolStatus[];
  linkedChangeSetId: string | null;
  linkedEntryIds: string[];
  lastContext: AgentContext | null;
  error: string | null;
  reviewAvailable: true;
  truncated: boolean;
};
```

On submit:

- require connected;
- create request ID;
- set starting;
- clear previous run state;
- call transport and retain the returned requestId for `run.started` correlation.

On events:

- run.started matches current request;
- ignore all mismatched run IDs;
- append bounded message deltas;
- update plan/tools/link;
- terminal events set final run status.

`cancel()` calls transport and optimistically sets cancelled; a later event from
the cancelled run is ignored.

- [ ] **Step 4: Run focused tests and full Viewer unit suite**

```bash
cd apps/viewer
node esbuild-tests.mjs
node --test test-dist/agent-store.test.js
npm test
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add apps/viewer/src/agent/AgentStore.ts apps/viewer/tests/agent-store.test.ts
git commit -m "feat(agent): track real run context and state"
```

---

### Task 5: Linked ChangeSet Loading

**Files:**
- Modify: `apps/viewer/src/api/StudioClient.ts`
- Modify: `apps/viewer/src/changeset/types.ts`
- Modify: `apps/viewer/src/changeset/ChangeSetStore.ts`
- Modify: `apps/viewer/tests/changeset-store.test.ts`

**Interfaces:**
- Consumes: `changeset.get`、`changeset.getCandidate`
- Produces:
  - `StudioClient.changeSetCandidate(changeSetId): Promise<ChangeSetEvaluation>`
  - `ChangeSetStore.loadLinked(changeSetId): Promise<void>`

- [ ] **Step 1: Write failing linked-load test**

```ts
test('loads an Agent-linked candidate without evaluating or approving', async () => {
  let evaluated = false;
  const client: ChangeSetClient = {
    createChangeSet: async () => ({ id: 'changeset-linked', version: 1 }),
    addChangeSetOperations: async () => ({ version: 2 }),
    removeChangeSetOperations: async () => ({ version: 2 }),
    evaluateChangeSet: async () => {
      evaluated = true;
      throw new Error('must not evaluate');
    },
    changeSet: async (id) => ({ ...changeSet, id }),
    changeSetCandidate: async (id) => ({
      changeSetId: id,
      version: 2,
      baseRevision: 2,
      status: 'ready',
      candidateDigest: 'sha256:linked',
      diff,
      diagnostics: [],
    }),
    commitChangeSet: async () => ({ revision: 3 }),
    candidatePreview: async () => ({ url: '/previews/candidates/linked' }),
  };
  const store = new ChangeSetStore(client);
  await store.loadLinked('changeset-linked');
  assert.equal(evaluated, false);
  assert.equal(store.snapshot.active?.candidateDigest, 'sha256:linked');
  assert.equal(store.snapshot.canApprove, false);
  assert.equal(store.snapshot.candidateUrl, '/previews/candidates/linked');
});
```

- [ ] **Step 2: Run and verify failure**

```bash
cd apps/viewer
node esbuild-tests.mjs
node --test test-dist/changeset-store.test.js
```

Expected: FAIL because linked APIs do not exist.

- [ ] **Step 3: Implement APIs and store load**

`StudioClient`:

```ts
changeSetCandidate(changeSetId: string): Promise<ChangeSetEvaluation> {
  return this.rpc('changeset.getCandidate', { changeSetId });
}
```

`ChangeSetClient` adds optional `changeSetCandidate`.

`ChangeSetStore.loadLinked`:

```ts
async loadLinked(changeSetId: string): Promise<void> {
  const [record, candidate] = await Promise.all([
    this.client.changeSet?.(changeSetId),
    this.client.changeSetCandidate?.(changeSetId),
  ]);
  if (!record || !candidate) throw new Error('Linked ChangeSet is unavailable');
  this.update({
    ...this.snapshot,
    record,
    active: candidate,
    reviewedDigest: null,
    canApprove: false,
    status: normalizeStatus(candidate.status),
    candidateUrl: null,
    impact: null,
    error: null,
  });
  if (candidate.candidateDigest && this.client.candidatePreview) {
    const preview = await this.client.candidatePreview(candidate.candidateDigest);
    this.update({ ...this.snapshot, candidateUrl: preview.url });
  }
}
```

- [ ] **Step 4: Run focused and full Viewer tests**

```bash
cd apps/viewer
node esbuild-tests.mjs
node --test test-dist/changeset-store.test.js
npm test
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add apps/viewer/src/api/StudioClient.ts apps/viewer/src/changeset/types.ts apps/viewer/src/changeset/ChangeSetStore.ts apps/viewer/tests/changeset-store.test.ts
git commit -m "feat(agent): load linked changesets without re-evaluation"
```

---

### Task 6: Provider, Context and Workbench Integration

**Files:**
- Create: `apps/viewer/src/agent/AgentProvider.tsx`
- Modify: `apps/viewer/src/main.tsx`
- Modify: `apps/viewer/src/app/Workbench.tsx`
- Modify: `apps/viewer/src/viewport/SceneAdapter.ts`
- Modify: `apps/viewer/src/viewport/Viewport.tsx`
- Test: `apps/viewer/tests/agent-store.test.ts`

**Interfaces:**
- Consumes:
  - `createWebSocketAgentTransport`
  - `AgentStore.submit({ text, context })`
  - `ChangeSetStore.loadLinked`
- Produces:
  - `AgentProvider({ store, children })`
  - `useAgentStore()`、`useAgent()`
  - `SceneAdapter.getCameraContext()`
  - `Viewport.onCameraContext`

- [ ] **Step 1: Add failing camera context test**

In `scene-transform-override.test.ts` or a new `camera-context.test.ts`, verify:

```ts
assert.deepEqual(camera, {
  position: [45, 32, 55],
  target: [0, 1, 0],
  fov: 42,
});
```

The test should call a pure helper or a SceneAdapter test hook, not private fields.

- [ ] **Step 2: Run and verify failure**

```bash
cd apps/viewer
node esbuild-tests.mjs
node --test test-dist/camera-context.test.js
```

Expected: FAIL because `getCameraContext` is missing.

- [ ] **Step 3: Implement camera context**

```ts
export type StudioCameraContext = {
  position: [number, number, number];
  target: [number, number, number];
  fov: number;
};

getCameraContext(): StudioCameraContext | null {
  if (!this.camera) return null;
  const position = this.camera.getPosition();
  const target = this.camera.getTarget();
  return {
    position: [position.x, position.y, position.z],
    target: [target.x, target.y, target.z],
    fov: this.camera.fov,
  };
}
```

Viewport receives `onAdapter?: (adapter: SceneAdapter | null) => void` and
reports mount/unmount. Workbench stores the adapter in a ref and reads camera
context at submit time.

- [ ] **Step 4: Inject real AgentStore**

`AgentProvider.tsx` mirrors other providers:

```tsx
const store = new AgentStore(createWebSocketAgentTransport({
  token: client.sessionToken,
}));
```

Expose a `StudioClient.sessionToken` getter without changing token storage.

`main.tsx` wraps:

```tsx
<SessionProvider store={session}>
  <AgentProvider store={agent}>
    <EditorDraftProvider store={editorDraft}>
      ...
    </EditorDraftProvider>
  </AgentProvider>
</SessionProvider>
```

- [ ] **Step 5: Wire Workbench submit and linked ChangeSet**

```ts
const submitAgent = async (text: string) => {
  const camera = viewportAdapter.current?.getCameraContext();
  if (!camera || !session.document) throw new Error('Scene context is unavailable');
  await agentStore.submit({
    text,
    context: {
      projectId: session.projectId,
      revision: session.revision,
      selectionIds: session.selectionIds,
      focusNodeId: selected?.id ?? null,
      camera,
    },
  });
};
```

Add an effect:

```ts
useEffect(() => {
  const id = agent.linkedChangeSetId;
  if (!id) return;
  void changeSetStore.loadLinked(id).then(() => setDock('changes'));
}, [agent.linkedChangeSetId, changeSetStore]);
```

Pass `store`, `onSubmit`, `onCancel`, `onReviewChanges` to `AgentPanel`.

- [ ] **Step 6: Run typecheck/build and focused tests**

```bash
npm --prefix apps/viewer run typecheck
npm --prefix apps/viewer run build
cd apps/viewer
node esbuild-tests.mjs
node --test test-dist/agent-store.test.js test-dist/camera-context.test.js
```

Expected: pass.

- [ ] **Step 7: Commit**

```bash
git add apps/viewer/src/agent/AgentProvider.tsx apps/viewer/src/main.tsx apps/viewer/src/app/Workbench.tsx apps/viewer/src/viewport/SceneAdapter.ts apps/viewer/src/viewport/Viewport.tsx apps/viewer/tests/camera-context.test.ts
git commit -m "feat(agent): wire real context and linked review"
```

---

### Task 7: AI-first Agent Mission Control UI

**Files:**
- Modify: `apps/viewer/src/agent/AgentPanel.tsx`
- Modify: `apps/viewer/src/app/studioTokens.css`
- Modify: `apps/viewer/e2e/viewer.spec.ts`

**Interfaces:**
- Consumes: `AgentSnapshot`、`onSubmit`、`onCancel`、`onReviewChanges`
- Produces:
  - Objective-first composer
  - Run status/tool timeline
  - Linked ChangeSet card
  - Context chips and offline/error recovery

- [ ] **Step 1: Add failing UI expectations**

Update `viewer.spec.ts`:

```ts
await expect(page.getByLabel('Run objective')).toBeVisible();
await expect(page.getByRole('button', { name: 'Run with Agent' })).toBeVisible();
await expect(page.getByText('Project')).toBeVisible();
await expect(page.getByText('Revision')).toBeVisible();
```

For offline:

```ts
await expect(page.getByText('Configure STUDIO_AGENT_GATEWAY_URL')).toBeVisible();
await expect(page.getByRole('button', { name: 'Open manual controls' })).toBeVisible();
```

- [ ] **Step 2: Run focused E2E and verify failure**

```bash
cd apps/viewer
PLAYWRIGHT_CHANNEL=chrome npm run test:e2e -- --grep "Agent mission control|Agent Gateway"
```

Expected: FAIL on new AI-first labels.

- [ ] **Step 3: Implement information hierarchy**

`AgentPanel` order:

1. Header: mission title, connection/run status.
2. Objective composer, visually primary.
3. Compact context chips: project, revision, focus.
4. Active/latest run card with Cancel.
5. Plan card.
6. Tool timeline.
7. Activity log in bounded `<details>`.
8. Linked ChangeSet card with `Review changes`.

Key markup:

```tsx
<form className="agent-mission" onSubmit={submit}>
  <label className="agent-objective">
    <span>What should change?</span>
    <textarea aria-label="Run objective" ... />
    <small>⌘ Enter to run · uses project, revision and selection</small>
  </label>
  <button type="submit" className="button button--primary">Run with Agent</button>
</form>
```

Tool timeline:

```tsx
<ol className="agent-tools" aria-label="Agent tool activity">
  {snapshot.tools.map((tool) => (
    <li className={`agent-tool agent-tool--${tool.phase}`} key={tool.key}>
      <code>{tool.toolName}</code>
      <span>{tool.summary}</span>
    </li>
  ))}
</ol>
```

Linked ChangeSet:

```tsx
<section className="agent-handoff" aria-label="Linked ChangeSet">
  <span>Ready for review</span>
  <strong>{snapshot.linkedChangeSetId}</strong>
  <button onClick={onReviewChanges}>Review changes</button>
</section>
```

- [ ] **Step 4: Add responsive and contrast-safe CSS**

Use existing tokens only:

```css
.agent-mission {
  display: grid;
  gap: var(--space-3);
  padding: var(--space-4);
  border: 1px solid var(--line-strong);
  border-radius: var(--radius-lg);
  background:
    linear-gradient(145deg, var(--accent-soft), transparent 55%),
    var(--panel-alt);
}

.agent-context-strip {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
}

.agent-context-chip {
  padding: 5px 8px;
  border: 1px solid var(--line);
  border-radius: var(--radius-pill);
  color: var(--text-soft);
  font-family: "SFMono-Regular", "Menlo", monospace;
  font-size: 10px;
}

.agent-tools {
  display: grid;
  gap: var(--space-2);
  max-height: 220px;
  margin: 0;
  padding: 0;
  overflow: auto;
  list-style: none;
}

.agent-handoff {
  display: grid;
  gap: var(--space-2);
  padding: var(--space-3);
  border: 1px solid rgb(104 201 149 / 30%);
  border-radius: var(--radius-md);
  background: rgb(104 201 149 / 7%);
}
```

At `max-width: 900px`, textarea/buttons are 44px minimum and the panel is
scrollable without horizontal overflow.

- [ ] **Step 5: Keyboard behavior**

`Ctrl/Cmd+Enter` submits; plain Enter adds a newline. Disable submit while
running. Focus Cancel and Review changes with the normal focus ring. Do not
move focus on each streamed message.

- [ ] **Step 6: Run typecheck, build and focused Chrome E2E**

```bash
npm --prefix apps/viewer run typecheck
npm --prefix apps/viewer run build
cd apps/viewer
PLAYWRIGHT_CHANNEL=chrome npm run test:e2e -- --grep "Agent mission control|Agent Gateway"
```

Expected: pass.

- [ ] **Step 7: Commit**

```bash
git add apps/viewer/src/agent/AgentPanel.tsx apps/viewer/src/app/studioTokens.css apps/viewer/e2e/viewer.spec.ts
git commit -m "feat(viewer): make agent mission control the primary workflow"
```

---

### Task 8: Deterministic Gateway Chrome E2E

**Files:**
- Create: `scripts/agent-gateway-fixture.mjs`
- Create: `crates/studio-cli/src/bin/agent-gateway-fixture.rs`
- Modify: `scripts/run-viewer-e2e-server.mjs`
- Modify: `crates/studio-cli/Cargo.toml`
- Create: `apps/viewer/e2e/agent-gateway.spec.ts`

**Interfaces:**
- Consumes: `/agent/ws` protocol
- Produces:
  - Loopback deterministic Gateway
  - E2E success, unavailable and disconnect flows

- [ ] **Step 1: Write failing Chrome E2E**

```ts
test('runs an Agent objective and hands off a ChangeSet for review', async ({ page }) => {
  await page.goto('/');
  await page.getByLabel('Run objective').fill('Rename the clarifier for review');
  await page.getByRole('button', { name: 'Run with Agent' }).click();
  await expect(page.getByText('Planning the edit')).toBeVisible();
  await expect(page.getByText('scene.search')).toBeVisible();
  await expect(page.getByRole('region', { name: 'Linked ChangeSet' })).toContainText(
    'changeset-e2e',
  );
  await page.getByRole('button', { name: 'Review changes' }).click();
  const changes = page.getByRole('region', { name: 'ChangeSet review' });
  await expect(changes).toContainText('changeset-e2e');
  await expect(page.getByRole('button', { name: 'Approve & commit' })).toBeDisabled();
  await page.getByRole('button', { name: 'Review candidate' }).click();
  await page.getByRole('button', { name: 'Approve & commit' }).click();
  await expect(page.getByRole('status')).toContainText(/Revision \d+/);
});
```

Add unavailable and disconnect tests.

- [ ] **Step 2: Run and verify failure**

```bash
cd apps/viewer
PLAYWRIGHT_CHANNEL=chrome npm run test:e2e -- --grep "Agent objective|Gateway unavailable|Gateway disconnects"
```

Expected: FAIL because fixture Gateway and real transport are absent.

- [ ] **Step 3: Implement deterministic Gateway**

`crates/studio-cli/src/bin/agent-gateway-fixture.rs` is a deterministic
tokio-tungstenite WebSocket server. `scripts/agent-gateway-fixture.mjs` only
spawns this binary with a fixed port, expected project/revision and ChangeSet ID.
The fixture:

1. validates `intent.submit` project/revision/selection/camera;
2. sends plan/tool/message events;
3. creates a ChangeSet through Studio RPC using a proposal token;
4. sends `changeset.linked` and `run.finished`;
5. supports cancel and an intentional disconnect test command.

No model credential, external network call or production CLI command is added;
the binary exists only for local contract/E2E evidence.

- [ ] **Step 4: Start fixture from E2E server**

`run-viewer-e2e-server.mjs` starts Gateway before Studio and passes:

```text
STUDIO_AGENT_GATEWAY_URL=ws://127.0.0.1:<gateway-port>/agent
```

Ensure cleanup stops both processes.

The Chrome test pre-creates `changeset-e2e` through Studio RPC so the fixture can
link it without owning a proposal credential. The ChangeSet remains unreviewed
until the human E2E clicks Review candidate and Approve & commit.

- [ ] **Step 5: Run Chrome E2E**

```bash
cd apps/viewer
PLAYWRIGHT_CHANNEL=chrome npm run test:e2e -- --grep "Agent objective|Gateway unavailable|Gateway disconnects"
```

Expected: all pass, including manual commit.

- [ ] **Step 6: Commit**

```bash
git add scripts/agent-gateway-fixture.mjs scripts/run-viewer-e2e-server.mjs crates/studio-cli/src/bin/agent-gateway-fixture.rs crates/studio-cli/Cargo.toml apps/viewer/e2e/agent-gateway.spec.ts
git commit -m "test(agent): verify gateway workflow in chrome"
```

---

### Task 9: Documentation, Visual Audit and Final Verification

**Files:**
- Modify: `README.md`
- Modify: `docs/README.md`
- Modify: `docs/architecture.md`
- Modify: `docs/reference/capabilities.md`
- Modify: `docs/guides/getting-started.md`
- Modify: `docs/guides/mcp.md`
- Modify: `docs/specs/2026-11-23-agent-gateway-integration-design.md`
- Modify: `docs/plans/2026-11-23-agent-gateway-integration.md`

**Interfaces:**
- Consumes: all prior tasks
- Produces: implemented statuses and final evidence

- [ ] **Step 1: Document configuration and protocol**

Add to getting started:

```bash
STUDIO_AGENT_GATEWAY_URL=ws://127.0.0.1:9130/agent \
STUDIO_AGENT_GATEWAY_TOKEN='<optional>' \
npm run dev
```

Document that no Gateway is required for manual editing and that Agent cannot commit.

Add the message types and host-only token rule to capabilities and MCP guide.

- [ ] **Step 2: Update AI-first UI documentation**

README interface table:

```markdown
| Agent | 默认 AI 任务控制台：目标输入、运行计划、工具进度、链接 ChangeSet 和人工接手 |
```

Architecture explains context snapshot, relay boundary and ChangeSet handoff.

- [ ] **Step 3: Perform Chrome visual audit**

Use installed Google Chrome at:

- 1440px;
- 1024px;
- 768px;
- 390px.

Check:

- objective composer is the first visual action;
- context does not dominate;
- running status, plan and tools form one scan path;
- linked ChangeSet is obvious but secondary to current run;
- offline/error states preserve manual handoff;
- controls meet 44px targets at responsive breakpoints;
- no horizontal overflow.

Save screenshots under `output/playwright/agent-gateway/`.

- [ ] **Step 4: Run focused and final gates**

```bash
cargo test -p studio-server --test agent_bridge
npm --prefix apps/viewer test
npm --prefix apps/viewer run typecheck
npm --prefix apps/viewer run build
cd apps/viewer
PLAYWRIGHT_CHANNEL=chrome npm run test:e2e
cd ../..
npm run docs:check
npm run verify
```

Expected: all pass.

- [ ] **Step 5: Mark statuses Implemented**

Change SPEC-039 and PLAN-036 from Approved/Proposed to Implemented and update
`docs/README.md`.

- [ ] **Step 6: Commit**

```bash
git add README.md docs/README.md docs/architecture.md docs/reference/capabilities.md docs/guides/getting-started.md docs/guides/mcp.md docs/specs/2026-11-23-agent-gateway-integration-design.md docs/plans/2026-11-23-agent-gateway-integration.md
git commit -m "docs(agent): mark gateway integration implemented"
```

---

## Plan Self-Review

### Spec coverage

| Spec requirement | Task |
|---|---|
| host-only loopback Gateway URL/token | Task 1 |
| `/agent/ws` auth and upstream relay | Task 2 |
| protocol messages and limits | Task 2, Task 3 |
| real project/revision/selection/camera context | Task 3, Task 6 |
| connection/run/message/plan/tool state | Task 4 |
| cancel and stale event ignoring | Task 4 |
| linked ChangeSet load without evaluate | Task 5 |
| camera context | Task 6 |
| AI-first objective and run hierarchy | Task 7 |
| linked ChangeSet handoff UI | Task 7 |
| unavailable/disconnect behavior | Task 2, Task 4, Task 7, Task 8 |
| deterministic Chrome E2E | Task 8 |
| docs and full verification | Task 9 |

### Placeholder scan

No `TBD`, `TODO`, deferred implementation placeholder, or unspecified test.

### Type consistency

- `AgentContext` is defined in `agentProtocol.ts` and consumed by `AgentStore`
  and Workbench.
- `AgentGatewayConfig` is defined once and stored in `ServerConfig` and `AppState`.
- `AgentSnapshot` fields are used consistently by panel and Workbench.
- `loadLinked` consumes `changeSetCandidate`, never `evaluateChangeSet`.
- `getCameraContext` returns the exact `camera` shape sent in `intent.submit`.

### Review focus tests

- Token/output leakage: Task 1 and Task 2.
- Socket/task cleanup: Task 2.
- stale run event: Task 4.
- bounded stream/tool state: Task 4.
- AI-first visual hierarchy and offline handoff: Task 7 and Task 9 Chrome audit.
