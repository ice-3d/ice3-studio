use std::collections::VecDeque;
use std::str::FromStr;
use std::time::{Duration, Instant};

use axum::extract::ws::{Message as DownstreamMessage, WebSocket};
use axum::http::{header::AUTHORIZATION, HeaderValue, Uri};
use futures_util::{SinkExt, StreamExt};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use tokio_tungstenite::tungstenite::{
    client::IntoClientRequest,
    Message as UpstreamMessage,
};

pub const MAX_AGENT_MESSAGE_BYTES: usize = 1024 * 1024;
const MAX_INTENT_TEXT_BYTES: usize = 32 * 1024;
const MAX_CONNECTION: Duration = Duration::from_secs(30 * 60);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_MESSAGES_PER_SECOND: usize = 100;

#[derive(Debug, Default)]
struct MessageRateLimiter {
    timestamps: VecDeque<Instant>,
}

impl MessageRateLimiter {
    fn try_acquire(&mut self) -> bool {
        let now = Instant::now();
        while self
            .timestamps
            .front()
            .is_some_and(|timestamp| now.duration_since(*timestamp) >= Duration::from_secs(1))
        {
            self.timestamps.pop_front();
        }
        if self.timestamps.len() >= MAX_MESSAGES_PER_SECOND {
            return false;
        }
        self.timestamps.push_back(now);
        true
    }
}

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
    if !matches!(authority.host(), "127.0.0.1" | "[::1]" | "::1" | "localhost") {
        return Err(AgentBridgeError::Config);
    }
    Ok(())
}

pub async fn run(mut downstream: WebSocket, config: Option<AgentGatewayConfig>) {
    let Some(config) = config else {
        let _ = downstream
            .send(DownstreamMessage::Text(
                serde_json::json!({
                    "type": "gateway.unavailable",
                    "protocolVersion": 1,
                    "message": "Agent Gateway is not configured"
                })
                .to_string()
                .into(),
            ))
            .await;
        let _ = downstream.close().await;
        return;
    };

    match tokio::time::timeout(MAX_CONNECTION, relay(&mut downstream, config)).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            let _ = downstream
                .send(DownstreamMessage::Text(
                    serde_json::json!({
                        "type": "gateway.unavailable",
                        "protocolVersion": 1,
                        "message": error.public_message()
                    })
                    .to_string()
                    .into(),
                ))
                .await;
        }
        Err(_) => {}
    }
    let _ = downstream.close().await;
}

async fn relay(
    downstream: &mut WebSocket,
    config: AgentGatewayConfig,
) -> Result<(), AgentBridgeError> {
    let request = upstream_request(&config)?;
    let (upstream, _) = tokio::time::timeout(
        CONNECT_TIMEOUT,
        tokio_tungstenite::connect_async(request),
    )
    .await
    .map_err(|_| AgentBridgeError::Unavailable)?
    .map_err(|_| AgentBridgeError::Unavailable)?;

    let (mut downstream_tx, mut downstream_rx) = downstream.split();
    let (mut upstream_tx, mut upstream_rx) = upstream.split();
    let mut rate_limiter = MessageRateLimiter::default();
    loop {
        tokio::select! {
            message = downstream_rx.next() => {
                match message {
                    Some(Ok(DownstreamMessage::Text(text))) => {
                        if !rate_limiter.try_acquire() {
                            return Err(AgentBridgeError::Protocol(
                                "client message rate exceeds limit".into(),
                            ));
                        }
                        validate_client_message(text.as_str())?;
                        let value: Value = serde_json::from_str(text.as_str())
                            .map_err(|_| AgentBridgeError::Protocol("invalid JSON".into()))?;
                        if value["type"] == "ping" {
                            downstream_tx.send(DownstreamMessage::Text(
                                serde_json::json!({
                                    "type": "pong",
                                    "protocolVersion": 1
                                }).to_string().into()
                            )).await.map_err(|_| AgentBridgeError::Upstream("downstream closed".into()))?;
                        } else {
                            upstream_tx.send(UpstreamMessage::Text(text.to_string().into()))
                                .await
                                .map_err(|_| AgentBridgeError::Upstream("upstream closed".into()))?;
                        }
                    }
                    Some(Ok(DownstreamMessage::Close(_))) | None | Some(Err(_)) => return Ok(()),
                    Some(Ok(DownstreamMessage::Binary(_))) => {
                        return Err(AgentBridgeError::Protocol("binary messages are unsupported".into()));
                    }
                    Some(Ok(_)) => {}
                }
            }
            message = upstream_rx.next() => {
                match message {
                    Some(Ok(UpstreamMessage::Text(text))) => {
                        validate_server_message(text.as_str())?;
                        downstream_tx.send(DownstreamMessage::Text(text.to_string().into()))
                            .await
                            .map_err(|_| AgentBridgeError::Upstream("downstream closed".into()))?;
                    }
                    Some(Ok(UpstreamMessage::Close(_))) | None | Some(Err(_)) => return Ok(()),
                    Some(Ok(UpstreamMessage::Binary(_))) => {
                        return Err(AgentBridgeError::Protocol("binary messages are unsupported".into()));
                    }
                    Some(Ok(_)) => {}
                }
            }
        }
    }
}

fn upstream_request(
    config: &AgentGatewayConfig,
) -> Result<tokio_tungstenite::tungstenite::handshake::client::Request, AgentBridgeError> {
    let mut request = config
        .url
        .as_str()
        .into_client_request()
        .map_err(|_| AgentBridgeError::Config)?;
    if let Some(token) = &config.token {
        let value = HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|_| AgentBridgeError::Config)?;
        request.headers_mut().insert(AUTHORIZATION, value);
    }
    Ok(request)
}

fn parse_protocol_message(value: &str) -> Result<Value, AgentBridgeError> {
    if value.len() > MAX_AGENT_MESSAGE_BYTES {
        return Err(AgentBridgeError::Protocol("message exceeds size limit".into()));
    }
    let value: Value =
        serde_json::from_str(value).map_err(|_| AgentBridgeError::Protocol("invalid JSON".into()))?;
    if !value.is_object() || value["protocolVersion"] != 1 {
        return Err(AgentBridgeError::Protocol("unsupported protocol version".into()));
    }
    if !value["type"].is_string() {
        return Err(AgentBridgeError::Protocol("message type is missing".into()));
    }
    Ok(value)
}

fn validate_client_message(value: &str) -> Result<(), AgentBridgeError> {
    let value = parse_protocol_message(value)?;
    match value["type"].as_str() {
        Some("ping") => Ok(()),
        Some("run.cancel") => {
            non_empty_string(&value, "runId")?;
            Ok(())
        }
        Some("intent.submit") => {
            non_empty_string(&value, "requestId")?;
            let intent = value["intent"]
                .as_object()
                .ok_or_else(|| AgentBridgeError::Protocol("intent is missing".into()))?;
            let text = intent
                .get("text")
                .and_then(Value::as_str)
                .filter(|text| !text.trim().is_empty())
                .ok_or_else(|| AgentBridgeError::Protocol("intent text is missing".into()))?;
            if text.len() > MAX_INTENT_TEXT_BYTES {
                return Err(AgentBridgeError::Protocol("intent text exceeds limit".into()));
            }
            if !intent.get("projectId").is_some_and(Value::is_string)
                || !intent.get("revision").is_some_and(Value::is_u64)
                || !intent.get("selectionIds").is_some_and(Value::is_array)
            {
                return Err(AgentBridgeError::Protocol("intent context is invalid".into()));
            }
            if intent["selectionIds"].as_array().is_some_and(|items| items.len() > 64) {
                return Err(AgentBridgeError::Protocol("selection exceeds limit".into()));
            }
            validate_camera(&intent["camera"])?;
            Ok(())
        }
        Some(other) => Err(AgentBridgeError::Protocol(format!(
            "unsupported client message: {other}"
        ))),
        None => Err(AgentBridgeError::Protocol("message type is missing".into())),
    }
}

fn validate_server_message(value: &str) -> Result<(), AgentBridgeError> {
    let value = parse_protocol_message(value)?;
    match value["type"].as_str() {
        Some(
            "gateway.unavailable"
            | "run.started"
            | "message.delta"
            | "plan.updated"
            | "tool.updated"
            | "changeset.linked"
            | "run.finished"
            | "run.failed"
            | "pong",
        ) => Ok(()),
        Some(other) => Err(AgentBridgeError::Protocol(format!(
            "unsupported server message: {other}"
        ))),
        None => Err(AgentBridgeError::Protocol("message type is missing".into())),
    }
}

fn non_empty_string(value: &Value, key: &str) -> Result<(), AgentBridgeError> {
    value[key]
        .as_str()
        .filter(|value| !value.is_empty())
        .map(|_| ())
        .ok_or_else(|| AgentBridgeError::Protocol(format!("{key} is missing")))
}

fn validate_camera(value: &Value) -> Result<(), AgentBridgeError> {
    let vector = |key: &str| -> Result<(), AgentBridgeError> {
        let values = value[key]
            .as_array()
            .filter(|values| values.len() == 3)
            .ok_or_else(|| AgentBridgeError::Protocol("camera vector is invalid".into()))?;
        if !values.iter().all(|value| value.as_f64().is_some_and(f64::is_finite)) {
            return Err(AgentBridgeError::Protocol("camera vector is invalid".into()));
        }
        Ok(())
    };
    vector("position")?;
    vector("target")?;
    let fov = value["fov"]
        .as_f64()
        .filter(|fov| fov.is_finite() && *fov > 0.0 && *fov <= 179.0)
        .ok_or_else(|| AgentBridgeError::Protocol("camera fov is invalid".into()))?;
    let _ = fov;
    Ok(())
}

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

    #[test]
    fn rejects_unknown_message_types() {
        assert!(validate_client_message(
            r#"{"type":"agent.execute_shell","protocolVersion":1}"#
        )
        .is_err());
        assert!(validate_server_message(
            r#"{"type":"tool.raw_payload","protocolVersion":1}"#
        )
        .is_err());
    }

    #[test]
    fn rejects_messages_above_the_limit() {
        let oversized = format!(
            r#"{{"type":"message.delta","protocolVersion":1,"runId":"run-1","text":"{}"}}"#,
            "x".repeat(MAX_AGENT_MESSAGE_BYTES)
        );
        assert!(parse_protocol_message(&oversized).is_err());
    }

    #[test]
    fn limits_downstream_message_rate() {
        let mut limiter = MessageRateLimiter::default();
        for _ in 0..100 {
            assert!(limiter.try_acquire());
        }
        assert!(!limiter.try_acquire());
    }
}
