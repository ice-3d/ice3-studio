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
    if !matches!(authority.host(), "127.0.0.1" | "[::1]" | "::1" | "localhost") {
        return Err(AgentBridgeError::Config);
    }
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
}
