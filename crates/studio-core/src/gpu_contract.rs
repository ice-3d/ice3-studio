use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::document::NodeContent;
use crate::geometry::EvaluatedMesh;
use crate::ids::NodeId;
use crate::validation::Diagnostic;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum GeometryBackendKind {
    Cpu,
    Gpu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum GeometryBackendMode {
    Auto,
    Cpu,
    Gpu,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum GeometryJobPriority {
    Background,
    Throughput,
    Interactive,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeometryBatch {
    pub job_id: String,
    pub priority: GeometryJobPriority,
    pub budget_bytes: u64,
    pub nodes: Vec<GeometryJobNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeometryJobNode {
    pub node_id: NodeId,
    pub content: NodeContent,
    pub content_digest: String,
    pub dependency_digest: String,
    pub cache_key: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GeometryExecutionStats {
    pub cpu_ms: f64,
    pub upload_bytes: u64,
    pub kernel_ms: f64,
    pub readback_bytes: u64,
    pub queue_wait_ms: f64,
    pub gpu_memory_estimate_bytes: u64,
    pub cache_hits: usize,
    pub fallback_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GeometryBatchResult {
    pub job_id: String,
    pub backend: GeometryBackendKind,
    pub meshes: BTreeMap<NodeId, EvaluatedMesh>,
    pub stats: GeometryExecutionStats,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BackendCapabilities {
    pub kind: GeometryBackendKind,
    pub health: String,
    pub accelerated: bool,
    pub adapter_tier: String,
    pub backend_family: String,
    pub memory_budget_bytes: u64,
    pub fallback_reason: Option<String>,
}

impl BackendCapabilities {
    pub fn cpu() -> Self {
        Self {
            kind: GeometryBackendKind::Cpu,
            health: "ready".into(),
            accelerated: false,
            adapter_tier: "reference".into(),
            backend_family: "cpu".into(),
            memory_budget_bytes: 0,
            fallback_reason: None,
        }
    }
}

#[derive(Debug, Error)]
pub enum BackendError {
    #[error("GPU backend is unavailable: {reason}")]
    Unavailable { reason: String },
    #[error("GPU backend is incompatible: {reason}")]
    Incompatible { reason: String },
    #[error("GPU job exceeds the memory budget")]
    BudgetExceeded,
    #[error("GPU kernel failed: {message}")]
    Kernel { message: String },
    #[error("GPU device was lost: {message}")]
    DeviceLost { message: String },
    #[error("GPU parity check failed")]
    Parity,
}

#[async_trait::async_trait]
pub trait GeometryBackend: Send + Sync {
    async fn initialize(&self) -> Result<BackendCapabilities, BackendError>;
    async fn evaluate(&self, batch: GeometryBatch) -> Result<GeometryBatchResult, BackendError>;
    async fn cancel(&self, job_id: &str) -> Result<(), BackendError>;
    fn stats(&self) -> GeometryExecutionStats;
}
