use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::document::NodeContent;
use crate::geometry::{Bounds, EvaluatedMesh};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum GeometryEvaluationMode {
    Cpu,
    Cold,
    Warm,
    Resident,
    Snapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GeometryDigestSet {
    pub topology: String,
    pub attribute: String,
    pub dependency: String,
    pub kernel_version: String,
    pub parity_version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ResidentGeometryState {
    Pending,
    Ready,
    Lost,
    Evicted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
#[repr(u16)]
pub enum GeometryDirtyChannel {
    Topology = 1 << 0,
    Positions = 1 << 1,
    Indices = 1 << 2,
    Normals = 1 << 3,
    Bounds = 1 << 4,
    Visibility = 1 << 5,
    Material = 1 << 6,
    Transform = 1 << 7,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[ts(export)]
pub struct GeometryDirtyMask(pub u16);

impl GeometryDirtyMask {
    pub const fn empty() -> Self {
        Self(0)
    }

    pub fn insert(&mut self, channel: GeometryDirtyChannel) {
        self.0 |= channel as u16;
    }

    pub const fn contains(self, channel: GeometryDirtyChannel) -> bool {
        self.0 & channel as u16 != 0
    }

    pub const fn bits(self) -> u16 {
        self.0
    }
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
    pub digests: Option<GeometryDigestSet>,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GeometryEvaluationEvidence {
    pub mode: GeometryEvaluationMode,
    pub stats: GeometryExecutionStats,
    pub submission_count: usize,
    pub resident_geometries: usize,
    pub resident_bytes: u64,
    pub peak_resident_bytes: u64,
    pub topology_hits: usize,
    pub resident_hits: usize,
    pub evictions: usize,
    pub preview_packet_bytes: u64,
    pub browser_upload_bytes: u64,
    pub phases: GeometryPhaseEvidence,
}

impl Default for GeometryEvaluationEvidence {
    fn default() -> Self {
        Self {
            mode: GeometryEvaluationMode::Cpu,
            stats: GeometryExecutionStats::default(),
            submission_count: 0,
            resident_geometries: 0,
            resident_bytes: 0,
            peak_resident_bytes: 0,
            topology_hits: 0,
            resident_hits: 0,
            evictions: 0,
            preview_packet_bytes: 0,
            browser_upload_bytes: 0,
            phases: GeometryPhaseEvidence::default(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GeometryPhaseEvidence {
    pub lookup_ms: f64,
    pub planning_ms: f64,
    pub validation_ms: f64,
    pub encode_ms: f64,
    pub submit_ms: f64,
    pub gpu_kernel_ms: Option<f64>,
    pub wait_ms: f64,
    pub snapshot_ms: f64,
    pub transport_ms: f64,
    pub browser_upload_ms: f64,
}

#[derive(Debug, Clone)]
pub struct GeometryBatchResult {
    pub job_id: String,
    pub backend: GeometryBackendKind,
    pub meshes: BTreeMap<NodeId, EvaluatedMesh>,
    pub stats: GeometryExecutionStats,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(transparent)]
#[ts(export)]
#[ts(type = "string")]
pub struct ResidentGeometryId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ResidentGeometryHandle {
    pub id: ResidentGeometryId,
    pub geometry_id: String,
    pub digests: GeometryDigestSet,
    pub vertex_count: usize,
    pub triangle_count: usize,
    pub device_generation: u64,
    pub state: ResidentGeometryState,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeometryResidentEntry {
    pub id: ResidentGeometryId,
    pub node_id: NodeId,
    pub mode: GeometryEvaluationMode,
    pub dirty: GeometryDirtyMask,
    pub bounds: Bounds,
    pub vertex_count: usize,
    pub triangle_count: usize,
    pub evidence: GeometryEvaluationEvidence,
}

#[derive(Debug, Clone)]
pub struct GeometryResidentBatchResult {
    pub job_id: String,
    pub backend: GeometryBackendKind,
    pub entries: Vec<GeometryResidentEntry>,
    pub evidence: GeometryEvaluationEvidence,
}

#[derive(Debug, Clone)]
pub struct GeometrySnapshotRequest {
    pub geometry_ids: Vec<ResidentGeometryId>,
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
    async fn evaluate_resident(
        &self,
        _batch: GeometryBatch,
    ) -> Result<GeometryResidentBatchResult, BackendError> {
        Err(BackendError::Incompatible {
            reason: "resident geometry is unsupported".into(),
        })
    }
    async fn snapshot(
        &self,
        _request: GeometrySnapshotRequest,
    ) -> Result<GeometryBatchResult, BackendError> {
        Err(BackendError::Incompatible {
            reason: "geometry snapshots are unsupported".into(),
        })
    }
    async fn cancel(&self, job_id: &str) -> Result<(), BackendError>;
    fn stats(&self) -> GeometryExecutionStats;
}
