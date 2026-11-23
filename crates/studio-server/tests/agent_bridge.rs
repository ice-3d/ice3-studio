use std::time::Duration;

use axum::{
    Router,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use studio_server::{ServerConfig, start_test_server};
use tokio::net::TcpListener;
use tokio_tungstenite::{connect_async, tungstenite};

async fn start_upstream(expected_token: Option<&str>) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let expected_token = expected_token.map(str::to_owned);
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route(
                "/agent",
                get(
                    move |headers: axum::http::HeaderMap, upgrade: WebSocketUpgrade| {
                        let expected_token = expected_token.clone();
                        async move {
                            if let Some(expected) = expected_token {
                                let actual = headers
                                    .get(axum::http::header::AUTHORIZATION)
                                    .and_then(|value| value.to_str().ok());
                                if actual != Some(format!("Bearer {expected}").as_str()) {
                                    return axum::http::StatusCode::UNAUTHORIZED.into_response();
                                }
                            }
                            upgrade.on_upgrade(upstream)
                        }
                    },
                ),
            ),
        )
        .await
        .unwrap();
    });
    (format!("ws://127.0.0.1:{port}/agent"), task)
}

async fn upstream(mut socket: WebSocket) {
    while let Some(Ok(message)) = socket.recv().await {
        let Message::Text(text) = message else {
            continue;
        };
        let value: Value = serde_json::from_str(text.as_str()).unwrap();
        match value["type"].as_str() {
            Some("ping") => {
                socket
                    .send(Message::Text(
                        json!({
                            "type": "pong",
                            "protocolVersion": 1
                        })
                        .to_string()
                        .into(),
                    ))
                    .await
                    .unwrap();
            }
            Some("intent.submit") => {
                socket
                    .send(Message::Text(
                        json!({
                            "type": "run.started",
                            "protocolVersion": 1,
                            "requestId": value["requestId"],
                            "runId": "run-1"
                        })
                        .to_string()
                        .into(),
                    ))
                    .await
                    .unwrap();
            }
            _ => {}
        }
    }
}

#[tokio::test]
async fn relays_ping_and_intent_through_authenticated_bridge() {
    let (url, upstream_task) = start_upstream(None).await;
    let mut config = ServerConfig::for_tests("bridge");
    config.agent_gateway = Some(studio_server::AgentGatewayConfig::new(url, None).unwrap());
    let server = start_test_server(config).await;
    let ws_url = format!(
        "{}/agent/ws?token={}",
        server.base_url().replacen("http://", "ws://", 1),
        server.token()
    );
    let (mut socket, _) = connect_async(ws_url).await.unwrap();
    socket
        .send(tungstenite::Message::Text(
            json!({"type": "ping", "protocolVersion": 1})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let pong = tokio::time::timeout(Duration::from_secs(2), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(pong.to_text().unwrap().contains("\"type\":\"pong\""));

    socket
        .send(tungstenite::Message::Text(
            json!({
                "type": "intent.submit",
                "protocolVersion": 1,
                "requestId": "request-1",
                "intent": {
                    "text": "Move the pump",
                    "projectId": "bridge",
                    "revision": 1,
                    "selectionIds": ["node-1"],
                    "focusNodeId": "node-1",
                    "camera": {
                        "position": [1, 2, 3],
                        "target": [0, 0, 0],
                        "fov": 42
                    }
                }
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();
    let started = tokio::time::timeout(Duration::from_secs(2), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
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

#[tokio::test]
async fn forwards_host_configured_token_only_upstream() {
    let (url, upstream_task) = start_upstream(Some("upstream-secret")).await;
    let mut config = ServerConfig::for_tests("bridge-token");
    config.agent_gateway =
        Some(studio_server::AgentGatewayConfig::new(url, Some("upstream-secret".into())).unwrap());
    let server = start_test_server(config).await;
    let ws_url = format!(
        "{}/agent/ws?token={}",
        server.base_url().replacen("http://", "ws://", 1),
        server.token()
    );
    let (mut socket, _) = connect_async(ws_url).await.unwrap();
    socket
        .send(tungstenite::Message::Text(
            json!({"type": "ping", "protocolVersion": 1})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let pong = tokio::time::timeout(Duration::from_secs(2), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(pong.to_text().unwrap().contains("\"type\":\"pong\""));
    upstream_task.abort();
}

#[tokio::test]
async fn rejects_missing_session_token() {
    let server = start_test_server(ServerConfig::for_tests("auth")).await;
    let result = connect_async(format!(
        "{}/agent/ws",
        server.base_url().replacen("http://", "ws://", 1)
    ))
    .await;
    assert!(result.is_err());
}
