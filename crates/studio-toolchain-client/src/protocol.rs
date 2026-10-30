use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolchainCapabilities {
    pub contract_version: String,
    pub package_version: String,
    pub operations: Vec<String>,
    pub input_formats: Vec<String>,
    pub features: ToolchainFeatures,
    pub limits: ToolchainLimits,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolchainFeatures {
    pub lod: bool,
    pub partition: bool,
    pub tileset: bool,
    pub render_batching: bool,
    pub cancellation: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolchainLimits {
    pub max_message_bytes: u64,
    pub max_concurrent_jobs: u64,
    pub max_source_bytes: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolchainDiagnostic {
    pub code: String,
    pub message: String,
    pub pointer: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolchainAssetResult {
    pub asset_id: String,
    pub content_hash: String,
    pub profile_version: String,
    pub manifest_path: String,
    pub primary_path: String,
    pub byte_length: u64,
    pub diagnostics: Vec<ToolchainDiagnostic>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolchainValidationResult {
    pub status: String,
    pub diagnostics: Vec<ToolchainDiagnostic>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolchainPartitionResult {
    pub asset_id: String,
    pub content_hash: String,
    pub tileset_path: String,
    pub files: Vec<ToolchainOutputFile>,
    pub diagnostics: Vec<ToolchainDiagnostic>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolchainOutputFile {
    pub path: String,
    pub content_hash: String,
    pub byte_length: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ToolchainState {
    Starting,
    Ready,
    Busy,
    Unavailable,
    Incompatible,
    Failed,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RpcEnvelope {
    pub jsonrpc: String,
    pub id: Option<u64>,
    pub result: Option<serde_json::Value>,
    pub error: Option<RpcError>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RpcError {
    pub code: i64,
    pub message: String,
    pub data: Option<RpcErrorData>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RpcErrorData {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}
