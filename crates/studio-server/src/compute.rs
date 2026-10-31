use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use studio_core::document::NodeContent;
use studio_core::geometry::{EvaluatedMesh, Primitive};
use studio_core::gpu_contract::{
    BackendCapabilities, BackendError, GeometryBackend, GeometryBackendKind, GeometryBackendMode,
    GeometryBatch, GeometryEvaluationEvidence, GeometryEvaluationMode, GeometryExecutionStats,
    GeometryJobNode, GeometryJobPriority, GeometryPhaseEvidence, GeometryResidentBatchResult,
    GeometrySnapshotRequest, ResidentGeometryHandle, ResidentGeometryId, ResidentGeometryState,
};
use studio_core::ids::NodeId;
use studio_core::large_scene::cache::{
    GeometryCacheMetadata, GeometryEvaluation, GeometryEvaluator,
};
use studio_core::scene_contract::{
    ScenePatch, SceneSessionId, SceneSessionStatus, SceneSubmission,
};
use studio_core::validation::DiagnosticCode;
use studio_gpu::ResidencyKey;
use tokio::sync::{Notify, OnceCell};
use ts_rs::TS;

pub const INITIAL_GPU_THRESHOLD_TRIANGLES: usize = usize::MAX;
pub const MAX_CONCURRENT_GPU_JOBS: usize = 2;
pub const DEFAULT_GPU_BUDGET_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const GPU_KERNEL_VERSION: &str = "geometry-kernel-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum GeometryEvaluationPurpose {
    Preview,
    Candidate,
    SourceExport,
    Diagnostics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotPolicy {
    Resident,
    CpuReadable,
}

impl SnapshotPolicy {
    pub const fn for_purpose(purpose: GeometryEvaluationPurpose) -> Self {
        match purpose {
            GeometryEvaluationPurpose::Preview | GeometryEvaluationPurpose::Candidate => {
                Self::Resident
            }
            GeometryEvaluationPurpose::SourceExport | GeometryEvaluationPurpose::Diagnostics => {
                Self::CpuReadable
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComputePolicy {
    pub mode: GeometryBackendMode,
    pub gpu_threshold_triangles: usize,
    pub max_concurrent_gpu_jobs: usize,
    pub gpu_budget_bytes: u64,
    pub resident_enabled: bool,
    pub preview_packet_enabled: bool,
    pub scene_sessions_enabled: bool,
}

impl Default for ComputePolicy {
    fn default() -> Self {
        Self {
            mode: GeometryBackendMode::Auto,
            gpu_threshold_triangles: INITIAL_GPU_THRESHOLD_TRIANGLES,
            max_concurrent_gpu_jobs: MAX_CONCURRENT_GPU_JOBS,
            gpu_budget_bytes: DEFAULT_GPU_BUDGET_BYTES,
            resident_enabled: false,
            preview_packet_enabled: false,
            scene_sessions_enabled: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ResidentGeometryStatus {
    pub enabled: bool,
    pub geometry_count: usize,
    pub resident_bytes: u64,
    pub peak_resident_bytes: u64,
    pub topology_hits: usize,
    pub resident_hits: usize,
    pub evictions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ComputeStatus {
    pub mode: GeometryBackendMode,
    pub selected_backend: GeometryBackendKind,
    pub adapter: String,
    pub tier: String,
    pub memory_budget_bytes: u64,
    pub health: String,
    pub fallback_reason: Option<String>,
    pub queue_depth: usize,
    pub max_concurrent_jobs: usize,
    pub gpu_threshold_triangles: usize,
    pub last_kernel_ms: f64,
    pub last_upload_bytes: u64,
    pub last_readback_bytes: u64,
    pub last_cache_hits: usize,
    pub last_evaluation_mode: String,
    pub resident_geometry: ResidentGeometryStatus,
    pub resident_bytes: u64,
    pub peak_resident_bytes: u64,
    pub topology_hits: usize,
    pub resident_hits: usize,
    pub evictions: usize,
    pub preview_packet_bytes: u64,
    pub browser_upload_bytes: u64,
    pub snapshot_readback_bytes: u64,
    pub preview_packet_version: Option<String>,
    pub preview_geometry_packet: Option<String>,
    pub pipelines_warmed: bool,
    pub resident_hot_path_hashes: usize,
    pub handle_lookup_ms: f64,
    pub last_geometry_phases: GeometryPhaseEvidence,
    pub scene_session: SceneSessionStatus,
}

#[derive(Debug, Clone)]
pub struct ComputeResult {
    pub mesh: EvaluatedMesh,
    pub backend: GeometryBackendKind,
    pub stats: GeometryExecutionStats,
    pub fallback_reason: Option<String>,
    pub kernel_version: String,
    pub parity_status: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ComputeError {
    #[error(transparent)]
    Backend(#[from] BackendError),
    #[error("native GPU compute is unavailable: {reason}")]
    GpuUnavailable { reason: String },
    #[error(transparent)]
    Geometry(#[from] studio_core::geometry::GeometryError),
    #[error("resident geometry evaluation is unavailable: {reason}")]
    ResidencyUnavailable { reason: String },
}

impl ComputeError {
    pub fn diagnostic_code(&self) -> &'static str {
        match self {
            Self::GpuUnavailable { .. } => DiagnosticCode::GpuUnavailable.as_str(),
            Self::ResidencyUnavailable { .. } => DiagnosticCode::GpuUnavailable.as_str(),
            Self::Geometry(_) => DiagnosticCode::GeometryInvalid.as_str(),
            Self::Backend(error) => match error {
                BackendError::Unavailable { .. } => DiagnosticCode::GpuUnavailable.as_str(),
                BackendError::Incompatible { .. } => DiagnosticCode::GpuIncompatible.as_str(),
                BackendError::BudgetExceeded => DiagnosticCode::GpuBudgetExceeded.as_str(),
                BackendError::Kernel { .. } => DiagnosticCode::GpuKernelFailed.as_str(),
                BackendError::DeviceLost { .. } => DiagnosticCode::GpuDeviceLost.as_str(),
                BackendError::Parity => DiagnosticCode::GpuParityFailed.as_str(),
            },
        }
    }
}

#[derive(Debug, Clone)]
struct Probe {
    capabilities: Option<BackendCapabilities>,
    failure: Option<String>,
}

struct PriorityGate {
    max_jobs: usize,
    in_flight: AtomicUsize,
    pending: [AtomicUsize; 3],
    notify: Notify,
}

impl PriorityGate {
    fn new(max_jobs: usize) -> Self {
        Self {
            max_jobs,
            in_flight: AtomicUsize::new(0),
            pending: [
                AtomicUsize::new(0),
                AtomicUsize::new(0),
                AtomicUsize::new(0),
            ],
            notify: Notify::new(),
        }
    }

    async fn acquire(self: &Arc<Self>, priority: GeometryJobPriority) -> GpuPermit {
        let rank = priority_rank(priority);
        self.pending[rank].fetch_add(1, Ordering::AcqRel);
        let mut pending = PendingRequest {
            gate: self.clone(),
            rank,
            acquired: false,
        };
        loop {
            let notified = self.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if rank == self.highest_priority() {
                let current = self.in_flight.load(Ordering::Acquire);
                if current < self.max_jobs
                    && self
                        .in_flight
                        .compare_exchange(current, current + 1, Ordering::AcqRel, Ordering::Acquire)
                        .is_ok()
                {
                    self.pending[rank].fetch_sub(1, Ordering::AcqRel);
                    pending.acquired = true;
                    return GpuPermit { gate: self.clone() };
                }
            }
            notified.await;
        }
    }

    fn highest_priority(&self) -> usize {
        for rank in (0..3).rev() {
            if self.pending[rank].load(Ordering::Acquire) > 0 {
                return rank;
            }
        }
        0
    }

    fn queue_depth(&self) -> usize {
        self.pending
            .iter()
            .map(|pending| pending.load(Ordering::Relaxed))
            .sum()
    }
}

struct PendingRequest {
    gate: Arc<PriorityGate>,
    rank: usize,
    acquired: bool,
}

impl Drop for PendingRequest {
    fn drop(&mut self) {
        if !self.acquired {
            self.gate.pending[self.rank].fetch_sub(1, Ordering::AcqRel);
            self.gate.notify.notify_waiters();
        }
    }
}

struct GpuPermit {
    gate: Arc<PriorityGate>,
}

impl Drop for GpuPermit {
    fn drop(&mut self) {
        self.gate.in_flight.fetch_sub(1, Ordering::AcqRel);
        self.gate.notify.notify_waiters();
    }
}

fn priority_rank(priority: GeometryJobPriority) -> usize {
    match priority {
        GeometryJobPriority::Background => 0,
        GeometryJobPriority::Throughput => 1,
        GeometryJobPriority::Interactive => 2,
    }
}

#[derive(Debug, Clone)]
struct LastCompute {
    content: NodeContent,
    resident_ids: Vec<ResidentGeometryId>,
}

#[derive(Debug, Clone)]
struct RegisteredGeometry {
    handle: ResidentGeometryHandle,
    content: NodeContent,
}

#[derive(Debug, Clone)]
pub struct GeometryHandleEvaluation {
    pub handle: ResidentGeometryHandle,
    pub mode: String,
    pub evidence: GeometryEvaluationEvidence,
}

pub struct ComputeService {
    policy: ComputePolicy,
    gpu: Option<Arc<dyn GeometryBackend>>,
    probe: OnceCell<Probe>,
    gate: Arc<PriorityGate>,
    gpu_available: Arc<AtomicBool>,
    runtime_failure: OnceLock<String>,
    last_kernel_ms_bits: std::sync::atomic::AtomicU64,
    last_upload_bytes: std::sync::atomic::AtomicU64,
    last_readback_bytes: std::sync::atomic::AtomicU64,
    last_cache_hits: AtomicUsize,
    last_snapshot_readback_bytes: std::sync::atomic::AtomicU64,
    last_backend: AtomicU8,
    pipeline_warmed: AtomicBool,
    last_evaluation_mode: Mutex<String>,
    resident_evidence: Mutex<GeometryEvaluationEvidence>,
    resident_by_content: Mutex<BTreeMap<String, ResidentGeometryId>>,
    registered_geometry: Mutex<BTreeMap<ResidentGeometryId, RegisteredGeometry>>,
    geometry_ids: Mutex<BTreeMap<String, ResidentGeometryId>>,
    resident_hot_path_hashes: AtomicUsize,
    handle_lookup_ms_bits: std::sync::atomic::AtomicU64,
    device_generation: std::sync::atomic::AtomicU64,
    last_compute: Mutex<Option<LastCompute>>,
    transforms: Mutex<BTreeMap<String, studio_core::math::Transform>>,
    active_scene_session: Mutex<Option<SceneSessionId>>,
}

impl ComputeService {
    pub fn new(policy: ComputePolicy, gpu: Option<Arc<dyn GeometryBackend>>) -> Self {
        let gpu_configured = gpu.is_some();
        Self {
            policy,
            gpu,
            probe: OnceCell::new(),
            gate: Arc::new(PriorityGate::new(policy.max_concurrent_gpu_jobs.max(1))),
            gpu_available: Arc::new(AtomicBool::new(gpu_configured)),
            runtime_failure: OnceLock::new(),
            last_kernel_ms_bits: std::sync::atomic::AtomicU64::new(0.0f64.to_bits()),
            last_upload_bytes: std::sync::atomic::AtomicU64::new(0),
            last_readback_bytes: std::sync::atomic::AtomicU64::new(0),
            last_cache_hits: AtomicUsize::new(0),
            last_snapshot_readback_bytes: std::sync::atomic::AtomicU64::new(0),
            last_backend: AtomicU8::new(0),
            pipeline_warmed: AtomicBool::new(false),
            last_evaluation_mode: Mutex::new("cpu".into()),
            resident_evidence: Mutex::new(GeometryEvaluationEvidence::default()),
            resident_by_content: Mutex::new(BTreeMap::new()),
            registered_geometry: Mutex::new(BTreeMap::new()),
            geometry_ids: Mutex::new(BTreeMap::new()),
            resident_hot_path_hashes: AtomicUsize::new(0),
            handle_lookup_ms_bits: std::sync::atomic::AtomicU64::new(0.0f64.to_bits()),
            device_generation: std::sync::atomic::AtomicU64::new(1),
            last_compute: Mutex::new(None),
            transforms: Mutex::new(BTreeMap::new()),
            active_scene_session: Mutex::new(None),
        }
    }

    pub fn defaults(gpu: Option<Arc<dyn GeometryBackend>>) -> Self {
        Self::new(ComputePolicy::default(), gpu)
    }

    pub async fn initialize(&self) -> Result<BackendCapabilities, ComputeError> {
        let probe = self.probe().await;
        if let Some(capabilities) = &probe.capabilities {
            Ok(capabilities.clone())
        } else {
            Err(ComputeError::GpuUnavailable {
                reason: probe
                    .failure
                    .clone()
                    .unwrap_or_else(|| "no native GPU backend was configured".into()),
            })
        }
    }

    pub async fn open_scene_session(
        &self,
        project_id: &str,
    ) -> Result<SceneSessionId, ComputeError> {
        if !self.policy.scene_sessions_enabled || self.policy.mode == GeometryBackendMode::Cpu {
            return Err(ComputeError::ResidencyUnavailable {
                reason: "GPU scene sessions are disabled".into(),
            });
        }
        let capabilities = self.initialize().await?;
        if capabilities.health != "ready" {
            return Err(ComputeError::GpuUnavailable {
                reason: capabilities
                    .fallback_reason
                    .unwrap_or_else(|| "GPU backend is not ready".into()),
            });
        }
        let Some(gpu) = &self.gpu else {
            return Err(ComputeError::GpuUnavailable {
                reason: "no native GPU backend was configured".into(),
            });
        };
        let next = gpu.create_scene_session(project_id).await?;
        let previous = self
            .active_scene_session
            .lock()
            .expect("scene session poisoned")
            .replace(next.clone());
        if let Some(previous) = previous {
            let _ = gpu.close_scene_session(&previous).await;
        }
        Ok(next)
    }

    pub async fn apply_scene_patch(
        &self,
        patch: ScenePatch,
    ) -> Result<SceneSubmission, ComputeError> {
        if !self.policy.scene_sessions_enabled {
            return Err(ComputeError::ResidencyUnavailable {
                reason: "GPU scene sessions are disabled".into(),
            });
        }
        let session = self
            .active_scene_session
            .lock()
            .expect("scene session poisoned")
            .clone()
            .ok_or_else(|| ComputeError::ResidencyUnavailable {
                reason: "no GPU scene session is open".into(),
            })?;
        let gpu = self
            .gpu
            .as_ref()
            .ok_or_else(|| ComputeError::GpuUnavailable {
                reason: "no native GPU backend was configured".into(),
            })?;
        Ok(gpu.apply_scene_patch(&session, patch).await?)
    }

    pub async fn wait_scene(&self, revision: u64) -> Result<SceneSessionStatus, ComputeError> {
        let session = self
            .active_scene_session
            .lock()
            .expect("scene session poisoned")
            .clone()
            .ok_or_else(|| ComputeError::ResidencyUnavailable {
                reason: "no GPU scene session is open".into(),
            })?;
        let gpu = self
            .gpu
            .as_ref()
            .ok_or_else(|| ComputeError::GpuUnavailable {
                reason: "no native GPU backend was configured".into(),
            })?;
        Ok(gpu.wait_scene_revision(&session, revision).await?)
    }

    pub async fn close_scene_session(&self) -> Result<(), ComputeError> {
        let session = self
            .active_scene_session
            .lock()
            .expect("scene session poisoned")
            .take();
        if let (Some(session), Some(gpu)) = (session, &self.gpu) {
            gpu.close_scene_session(&session).await?;
        }
        Ok(())
    }

    pub fn scene_status(&self) -> Option<SceneSessionStatus> {
        let session = self
            .active_scene_session
            .lock()
            .expect("scene session poisoned")
            .clone()?;
        self.gpu.as_ref()?.scene_status(&session)
    }

    pub async fn register_geometry(
        &self,
        geometry_id: String,
        content: NodeContent,
    ) -> Result<ResidentGeometryHandle, ComputeError> {
        if !self.policy.resident_enabled {
            return Err(ComputeError::ResidencyUnavailable {
                reason: "resident geometry is disabled".into(),
            });
        }
        if let Some(existing) = self
            .geometry_ids
            .lock()
            .expect("geometry id index poisoned")
            .get(&geometry_id)
            .cloned()
            && let Some(record) = self
                .registered_geometry
                .lock()
                .expect("registered geometry poisoned")
                .get(&existing)
            && record.content == content
        {
            return Ok(record.handle.clone());
        }

        let key = ResidencyKey::for_content(&content, "", GPU_KERNEL_VERSION)?;
        let digests = key.digest_set("", GPU_KERNEL_VERSION);
        let id = ResidentGeometryId(ResidencyKey::from_digests(&digests).result);
        let handle = ResidentGeometryHandle {
            id: id.clone(),
            geometry_id: geometry_id.clone(),
            digests,
            vertex_count: estimate_vertex_count(&content),
            triangle_count: estimate_triangles(&content),
            device_generation: self.device_generation.load(Ordering::Acquire),
            state: ResidentGeometryState::Pending,
        };
        self.resident_hot_path_hashes.fetch_add(1, Ordering::AcqRel);
        self.registered_geometry
            .lock()
            .expect("registered geometry poisoned")
            .insert(
                id.clone(),
                RegisteredGeometry {
                    handle: handle.clone(),
                    content,
                },
            );
        self.geometry_ids
            .lock()
            .expect("geometry id index poisoned")
            .insert(geometry_id, id);
        Ok(handle)
    }

    pub async fn evaluate_handle(
        &self,
        handle: &ResidentGeometryHandle,
    ) -> Result<GeometryHandleEvaluation, ComputeError> {
        let lookup_started = std::time::Instant::now();
        if handle.device_generation != self.device_generation.load(Ordering::Acquire)
            || handle.state == ResidentGeometryState::Lost
            || handle.state == ResidentGeometryState::Evicted
        {
            return Err(ComputeError::ResidencyUnavailable {
                reason: "resident geometry handle is stale".into(),
            });
        }
        let registered = self
            .registered_geometry
            .lock()
            .expect("registered geometry poisoned")
            .get(&handle.id)
            .cloned()
            .ok_or_else(|| ComputeError::ResidencyUnavailable {
                reason: "resident geometry handle is unknown".into(),
            })?;
        let lookup_ms = lookup_started.elapsed().as_secs_f64() * 1_000.0;
        let triangles = estimate_triangles(&registered.content);
        let estimated_bytes = estimate_geometry_bytes(&registered.content);
        let probe = self.probe().await;
        if !probe
            .capabilities
            .as_ref()
            .is_some_and(|capabilities| capabilities.health == "ready")
            || !self.gpu_available.load(Ordering::Acquire)
        {
            return Err(ComputeError::GpuUnavailable {
                reason: probe
                    .failure
                    .unwrap_or_else(|| "GPU backend is not ready".into()),
            });
        }
        let effective_budget = probe
            .capabilities
            .as_ref()
            .map(|capabilities| {
                min_budget(
                    self.policy.gpu_budget_bytes,
                    capabilities.memory_budget_bytes,
                )
            })
            .unwrap_or(self.policy.gpu_budget_bytes);
        if estimated_bytes > effective_budget {
            return Err(BackendError::BudgetExceeded.into());
        }
        let Some(gpu) = &self.gpu else {
            return Err(ComputeError::GpuUnavailable {
                reason: "no native GPU backend was configured".into(),
            });
        };
        let batch = GeometryBatch {
            job_id: format!("handle-{}-{triangles}", monotonic_job_id()),
            priority: GeometryJobPriority::Interactive,
            budget_bytes: self.policy.gpu_budget_bytes,
            nodes: vec![GeometryJobNode {
                node_id: NodeId::from(handle.geometry_id.clone()),
                content: registered.content,
                content_digest: handle.digests.attribute.clone(),
                dependency_digest: handle.digests.dependency.clone(),
                cache_key: handle.id.0.clone(),
                digests: Some(handle.digests.clone()),
            }],
        };
        let permit = self.gate.acquire(GeometryJobPriority::Interactive).await;
        let result = gpu.evaluate_resident(batch).await;
        drop(permit);
        match result {
            Ok(mut result) => {
                let mode = result
                    .entries
                    .first()
                    .map(|entry| evaluation_mode_label(entry.mode))
                    .unwrap_or("resident");
                if let Some(entry) = result.entries.first() {
                    result.evidence = entry.evidence.clone();
                }
                result.evidence.phases.lookup_ms = lookup_ms;
                let mut resident_handle = handle.clone();
                resident_handle.state = ResidentGeometryState::Ready;
                if let Some(entry) = result.entries.first() {
                    resident_handle.vertex_count = entry.vertex_count;
                    resident_handle.triangle_count = entry.triangle_count;
                }
                if let Some(record) = self
                    .registered_geometry
                    .lock()
                    .expect("registered geometry poisoned")
                    .get_mut(&handle.id)
                {
                    record.handle = resident_handle.clone();
                }
                self.pipeline_warmed.store(true, Ordering::Release);
                self.handle_lookup_ms_bits
                    .store(lookup_ms.to_bits(), Ordering::Relaxed);
                self.set_resident_evidence(result.evidence.clone());
                self.set_evaluation_mode(mode);
                Ok(GeometryHandleEvaluation {
                    handle: resident_handle,
                    mode: mode.into(),
                    evidence: result.evidence,
                })
            }
            Err(error) => {
                self.handle_resident_failure(&error);
                Err(error.into())
            }
        }
    }

    pub async fn snapshot_handle(
        &self,
        handle: &ResidentGeometryHandle,
    ) -> Result<studio_core::gpu_contract::GeometryBatchResult, ComputeError> {
        if handle.device_generation != self.device_generation.load(Ordering::Acquire)
            || handle.state != ResidentGeometryState::Ready
        {
            return Err(ComputeError::ResidencyUnavailable {
                reason: "resident geometry handle is stale".into(),
            });
        }
        let Some(gpu) = &self.gpu else {
            return Err(ComputeError::GpuUnavailable {
                reason: "no native GPU backend was configured".into(),
            });
        };
        let permit = self.gate.acquire(GeometryJobPriority::Interactive).await;
        let result = gpu
            .snapshot(GeometrySnapshotRequest {
                geometry_ids: vec![handle.id.clone()],
            })
            .await;
        drop(permit);
        match result {
            Ok(result) => {
                self.last_snapshot_readback_bytes
                    .store(result.stats.readback_bytes, Ordering::Relaxed);
                let mut evidence = self
                    .resident_evidence
                    .lock()
                    .expect("resident evidence poisoned")
                    .clone();
                evidence.mode = GeometryEvaluationMode::Snapshot;
                evidence.stats = result.stats.clone();
                self.set_resident_evidence(evidence);
                self.set_evaluation_mode("snapshot");
                Ok(result)
            }
            Err(error) => {
                self.handle_resident_failure(&error);
                Err(error.into())
            }
        }
    }

    pub fn preview_source_view(
        &self,
        handle: &ResidentGeometryHandle,
    ) -> Result<crate::preview_packet::PreviewGeometrySourceView, ComputeError> {
        let content = self
            .registered_geometry
            .lock()
            .expect("registered geometry poisoned")
            .get(&handle.id)
            .map(|registered| registered.content.clone())
            .ok_or_else(|| ComputeError::ResidencyUnavailable {
                reason: "resident geometry handle is unknown".into(),
            })?;
        let (positions, indices) = match &content {
            NodeContent::CustomMesh(mesh) => (mesh.positions.clone(), mesh.indices.clone()),
            _ => {
                let mesh = studio_core::geometry::evaluate_node_content(&content)?;
                (
                    mesh.vertices
                        .into_iter()
                        .map(|vertex| vertex.position)
                        .collect(),
                    mesh.indices,
                )
            }
        };
        Ok(crate::preview_packet::PreviewGeometrySourceView {
            handle: handle.clone(),
            positions: Arc::new(positions),
            indices: Arc::new(indices),
            needs_gpu_normals: false,
        })
    }

    pub async fn evaluate(&self, content: NodeContent) -> Result<ComputeResult, ComputeError> {
        self.evaluate_with_priority(content, GeometryJobPriority::Interactive)
            .await
    }

    pub async fn evaluate_with_priority(
        &self,
        content: NodeContent,
        priority: GeometryJobPriority,
    ) -> Result<ComputeResult, ComputeError> {
        self.remember_content(content.clone());
        let triangles = estimate_triangles(&content);
        let estimated_bytes = estimate_geometry_bytes(&content);
        let probe = self.probe().await;
        let effective_budget = probe
            .capabilities
            .as_ref()
            .map(|capabilities| {
                min_budget(
                    self.policy.gpu_budget_bytes,
                    capabilities.memory_budget_bytes,
                )
            })
            .unwrap_or(self.policy.gpu_budget_bytes);
        let use_gpu = match self.policy.mode {
            GeometryBackendMode::Cpu => false,
            GeometryBackendMode::Gpu => true,
            GeometryBackendMode::Auto => {
                triangles >= self.policy.gpu_threshold_triangles
                    && self.gpu_available.load(Ordering::Acquire)
                    && probe.capabilities.as_ref().is_some_and(|capabilities| {
                        capabilities.health == "ready"
                            && estimated_bytes
                                <= min_budget(
                                    self.policy.gpu_budget_bytes,
                                    capabilities.memory_budget_bytes,
                                )
                    })
            }
        };
        if !use_gpu {
            let fallback_reason = match self.policy.mode {
                GeometryBackendMode::Gpu => {
                    return Err(ComputeError::GpuUnavailable {
                        reason: probe
                            .failure
                            .clone()
                            .unwrap_or_else(|| "GPU backend is not ready".into()),
                    });
                }
                GeometryBackendMode::Auto if triangles >= self.policy.gpu_threshold_triangles => {
                    probe
                        .failure
                        .clone()
                        .or_else(|| {
                            probe.capabilities.as_ref().and_then(|capabilities| {
                                (capabilities.health != "ready")
                                    .then(|| capabilities.fallback_reason.clone())
                                    .flatten()
                            })
                        })
                        .or_else(|| {
                            (estimated_bytes > effective_budget)
                                .then(|| "GPU job exceeds the host memory budget".into())
                        })
                }
                _ => None,
            };
            return self.evaluate_cpu(content, fallback_reason);
        }

        let Some(gpu) = &self.gpu else {
            return Err(ComputeError::GpuUnavailable {
                reason: "no native GPU backend was configured".into(),
            });
        };
        if estimated_bytes > effective_budget {
            let error = BackendError::BudgetExceeded;
            return if self.policy.mode == GeometryBackendMode::Auto {
                self.evaluate_cpu(content, Some(error.to_string()))
            } else {
                Err(error.into())
            };
        }

        let queue_wait_started = std::time::Instant::now();
        let permit = self.gate.acquire(priority).await;
        let queue_wait_ms = queue_wait_started.elapsed().as_secs_f64() * 1_000.0;
        let node_id = NodeId::from("compute-node-000001");
        let batch = GeometryBatch {
            job_id: format!("compute-{}-{triangles}", monotonic_job_id()),
            priority,
            budget_bytes: self.policy.gpu_budget_bytes,
            nodes: vec![GeometryJobNode {
                node_id: node_id.clone(),
                content: content.clone(),
                content_digest: String::new(),
                dependency_digest: String::new(),
                cache_key: String::new(),
                digests: None,
            }],
        };
        let result = gpu.evaluate(batch).await;
        drop(permit);
        match result {
            Ok(batch_result) => {
                let Some(mesh) = batch_result.meshes.get(&node_id).cloned() else {
                    return self.fallback_or_error(
                        content,
                        BackendError::Kernel {
                            message: "GPU backend returned no mesh".into(),
                        },
                    );
                };
                let mut stats = batch_result.stats;
                stats.queue_wait_ms = queue_wait_ms;
                let fallback_reason = stats.fallback_reason.clone();
                self.record_stats(&stats);
                self.record_evaluation(GeometryEvaluationMode::Cold, None);
                self.last_backend.store(1, Ordering::Release);
                Ok(ComputeResult {
                    mesh,
                    backend: batch_result.backend,
                    stats,
                    fallback_reason,
                    kernel_version: GPU_KERNEL_VERSION.into(),
                    parity_status: "passed".into(),
                })
            }
            Err(error) => self.fallback_or_error(content, error),
        }
    }

    pub fn evaluate_blocking(
        &self,
        content: NodeContent,
        priority: GeometryJobPriority,
    ) -> Result<ComputeResult, ComputeError> {
        pollster::block_on(self.evaluate_with_priority(content, priority))
    }

    fn evaluate_with_resident_id_blocking(
        &self,
        content: NodeContent,
        priority: GeometryJobPriority,
    ) -> Result<(ComputeResult, Option<ResidentGeometryId>), ComputeError> {
        if !self.policy.resident_enabled || self.policy.mode == GeometryBackendMode::Cpu {
            return self
                .evaluate_blocking(content, priority)
                .map(|result| (result, None));
        }

        pollster::block_on(async {
            let content_key = resident_content_key(&content);
            let handle = self
                .register_geometry("compute-node-000001".into(), content)
                .await?;
            self.remember_content(
                self.registered_geometry
                    .lock()
                    .expect("registered geometry poisoned")
                    .get(&handle.id)
                    .expect("registered handle must exist")
                    .content
                    .clone(),
            );
            let resident = self.evaluate_handle(&handle).await?;
            let resident_id = Some(resident.handle.id.clone());
            if let Some(resident_id) = &resident_id {
                self.resident_by_content
                    .lock()
                    .expect("resident content index poisoned")
                    .insert(content_key, resident_id.clone());
            }
            let snapshot = self.snapshot_handle(&resident.handle).await?;
            let node_id = NodeId::from("compute-node-000001");
            let mesh = snapshot.meshes.get(&node_id).cloned().ok_or_else(|| {
                ComputeError::ResidencyUnavailable {
                    reason: "snapshot returned no mesh".into(),
                }
            })?;
            Ok((
                ComputeResult {
                    mesh,
                    backend: snapshot.backend,
                    stats: snapshot.stats,
                    fallback_reason: None,
                    kernel_version: GPU_KERNEL_VERSION.into(),
                    parity_status: "passed".into(),
                },
                resident_id,
            ))
        })
    }

    pub async fn evaluate_transform_only(
        &self,
        node_id: &str,
        transform: studio_core::math::Transform,
    ) -> Result<(), ComputeError> {
        if !self.policy.resident_enabled {
            return Err(ComputeError::ResidencyUnavailable {
                reason: "resident geometry is disabled".into(),
            });
        }
        self.transforms
            .lock()
            .expect("transform state poisoned")
            .insert(node_id.into(), transform);
        self.set_evaluation_mode("resident");
        Ok(())
    }

    pub async fn evaluate_for(
        &self,
        purpose: GeometryEvaluationPurpose,
    ) -> Result<GeometryResidentBatchResult, ComputeError> {
        if SnapshotPolicy::for_purpose(purpose) != SnapshotPolicy::Resident {
            return Err(ComputeError::ResidencyUnavailable {
                reason: format!("{purpose:?} requires an explicit CPU-readable snapshot"),
            });
        }
        if !self.policy.resident_enabled {
            return Err(ComputeError::ResidencyUnavailable {
                reason: "resident geometry is disabled".into(),
            });
        }
        if self.policy.mode == GeometryBackendMode::Cpu {
            return Err(ComputeError::ResidencyUnavailable {
                reason: "resident geometry requires a GPU backend".into(),
            });
        }
        let content = self
            .last_compute
            .lock()
            .expect("compute state poisoned")
            .as_ref()
            .map(|last| last.content.clone())
            .ok_or_else(|| ComputeError::ResidencyUnavailable {
                reason: "no geometry has been evaluated".into(),
            })?;
        let Some(gpu) = &self.gpu else {
            return Err(ComputeError::GpuUnavailable {
                reason: "no native GPU backend was configured".into(),
            });
        };

        let probe = self.probe().await;
        let healthy = probe
            .capabilities
            .as_ref()
            .is_some_and(|capabilities| capabilities.health == "ready")
            && self.gpu_available.load(Ordering::Acquire);
        let allow_gpu = match self.policy.mode {
            GeometryBackendMode::Cpu => false,
            GeometryBackendMode::Gpu | GeometryBackendMode::Auto => healthy,
        };
        if !allow_gpu {
            return Err(ComputeError::GpuUnavailable {
                reason: probe
                    .failure
                    .unwrap_or_else(|| "GPU backend is not ready".into()),
            });
        }
        let estimated_bytes = estimate_geometry_bytes(&content);
        let effective_budget = probe
            .capabilities
            .as_ref()
            .map(|capabilities| {
                min_budget(
                    self.policy.gpu_budget_bytes,
                    capabilities.memory_budget_bytes,
                )
            })
            .unwrap_or(self.policy.gpu_budget_bytes);
        if estimated_bytes > effective_budget {
            return Err(BackendError::BudgetExceeded.into());
        }

        let triangles = estimate_triangles(&content);
        let batch = GeometryBatch {
            job_id: format!("resident-{}-{triangles}", monotonic_job_id()),
            priority: GeometryJobPriority::Interactive,
            budget_bytes: self.policy.gpu_budget_bytes,
            nodes: vec![GeometryJobNode {
                node_id: NodeId::from("compute-node-000001"),
                content,
                content_digest: String::new(),
                dependency_digest: String::new(),
                cache_key: String::new(),
                digests: None,
            }],
        };
        let permit = self.gate.acquire(GeometryJobPriority::Interactive).await;
        let result = gpu.evaluate_resident(batch).await;
        drop(permit);
        match result {
            Ok(result) => {
                self.pipeline_warmed.store(true, Ordering::Release);
                self.record_evaluation(
                    GeometryEvaluationMode::Resident,
                    Some(result.evidence.clone()),
                );
                let ids = result
                    .entries
                    .iter()
                    .map(|entry| entry.id.clone())
                    .collect();
                if let Some(last) = self
                    .last_compute
                    .lock()
                    .expect("compute state poisoned")
                    .as_mut()
                {
                    last.resident_ids = ids;
                }
                Ok(result)
            }
            Err(error) => {
                self.handle_resident_failure(&error);
                Err(error.into())
            }
        }
    }

    pub async fn snapshot_for(
        &self,
        purpose: GeometryEvaluationPurpose,
    ) -> Result<studio_core::gpu_contract::GeometryBatchResult, ComputeError> {
        if SnapshotPolicy::for_purpose(purpose) != SnapshotPolicy::CpuReadable {
            return Err(ComputeError::ResidencyUnavailable {
                reason: format!("{purpose:?} does not request a CPU-readable snapshot"),
            });
        }
        if !self.policy.resident_enabled {
            return Err(ComputeError::ResidencyUnavailable {
                reason: "resident geometry is disabled".into(),
            });
        }
        let geometry_ids = self
            .last_compute
            .lock()
            .expect("compute state poisoned")
            .as_ref()
            .map(|last| last.resident_ids.clone())
            .filter(|ids| !ids.is_empty())
            .ok_or_else(|| ComputeError::ResidencyUnavailable {
                reason: "no resident geometry is available to snapshot".into(),
            })?;
        let Some(gpu) = &self.gpu else {
            return Err(ComputeError::GpuUnavailable {
                reason: "no native GPU backend was configured".into(),
            });
        };
        let permit = self.gate.acquire(GeometryJobPriority::Interactive).await;
        let result = gpu.snapshot(GeometrySnapshotRequest { geometry_ids }).await;
        drop(permit);
        match result {
            Ok(result) => {
                let mut evidence = self
                    .resident_evidence
                    .lock()
                    .expect("resident evidence poisoned")
                    .clone();
                evidence.mode = GeometryEvaluationMode::Snapshot;
                evidence.stats = result.stats.clone();
                self.record_stats(&result.stats);
                self.last_snapshot_readback_bytes
                    .store(result.stats.readback_bytes, Ordering::Relaxed);
                self.set_resident_evidence(evidence);
                self.set_evaluation_mode("snapshot");
                Ok(result)
            }
            Err(error) => {
                self.handle_resident_failure(&error);
                Err(error.into())
            }
        }
    }

    pub fn status(&self) -> ComputeStatus {
        let probe = self.probe.get();
        let capabilities = probe.and_then(|probe| probe.capabilities.as_ref());
        let failure = probe.and_then(|probe| probe.failure.clone());
        let cpu_forced = self.policy.mode == GeometryBackendMode::Cpu;
        let evidence = self
            .resident_evidence
            .lock()
            .expect("resident evidence poisoned")
            .clone();
        ComputeStatus {
            mode: self.policy.mode,
            selected_backend: if !cpu_forced && self.last_backend.load(Ordering::Acquire) == 1 {
                GeometryBackendKind::Gpu
            } else {
                GeometryBackendKind::Cpu
            },
            adapter: capabilities
                .map(|capabilities| capabilities.adapter_tier.clone())
                .unwrap_or_else(|| "reference".into()),
            tier: capabilities
                .map(|capabilities| capabilities.adapter_tier.clone())
                .unwrap_or_else(|| "cpu".into()),
            memory_budget_bytes: capabilities
                .map(|capabilities| {
                    min_budget(
                        self.policy.gpu_budget_bytes,
                        capabilities.memory_budget_bytes,
                    )
                })
                .unwrap_or(self.policy.gpu_budget_bytes),
            health: if cpu_forced {
                "ready".into()
            } else if !self.gpu_available.load(Ordering::Acquire) {
                "unavailable".into()
            } else {
                capabilities
                    .map(|capabilities| capabilities.health.clone())
                    .unwrap_or_else(|| {
                        if self.gpu.is_some() {
                            "probing".into()
                        } else {
                            "unavailable".into()
                        }
                    })
            },
            fallback_reason: if cpu_forced {
                None
            } else {
                failure
                    .or_else(|| capabilities.and_then(|value| value.fallback_reason.clone()))
                    .or_else(|| self.runtime_failure.get().cloned())
                    .or_else(|| {
                        self.gpu
                            .is_none()
                            .then(|| "no native GPU backend configured".into())
                    })
            },
            queue_depth: self.gate.queue_depth(),
            max_concurrent_jobs: self.policy.max_concurrent_gpu_jobs,
            gpu_threshold_triangles: self.policy.gpu_threshold_triangles,
            last_kernel_ms: f64::from_bits(self.last_kernel_ms_bits.load(Ordering::Relaxed)),
            last_upload_bytes: self.last_upload_bytes.load(Ordering::Relaxed),
            last_readback_bytes: self.last_readback_bytes.load(Ordering::Relaxed),
            last_cache_hits: self.last_cache_hits.load(Ordering::Relaxed),
            last_evaluation_mode: self
                .last_evaluation_mode
                .lock()
                .expect("evaluation mode poisoned")
                .clone(),
            resident_geometry: ResidentGeometryStatus {
                enabled: self.policy.resident_enabled,
                geometry_count: evidence.resident_geometries,
                resident_bytes: evidence.resident_bytes,
                peak_resident_bytes: evidence.peak_resident_bytes,
                topology_hits: evidence.topology_hits,
                resident_hits: evidence.resident_hits,
                evictions: evidence.evictions,
            },
            resident_bytes: evidence.resident_bytes,
            peak_resident_bytes: evidence.peak_resident_bytes,
            topology_hits: evidence.topology_hits,
            resident_hits: evidence.resident_hits,
            evictions: evidence.evictions,
            preview_packet_bytes: evidence.preview_packet_bytes,
            browser_upload_bytes: evidence.browser_upload_bytes,
            snapshot_readback_bytes: self.last_snapshot_readback_bytes.load(Ordering::Relaxed),
            preview_packet_version: self
                .policy
                .preview_packet_enabled
                .then(|| "ICE3PGP1".into()),
            preview_geometry_packet: self.policy.preview_packet_enabled.then(|| "1".into()),
            pipelines_warmed: self.pipeline_warmed.load(Ordering::Acquire),
            resident_hot_path_hashes: self.resident_hot_path_hashes.load(Ordering::Relaxed),
            handle_lookup_ms: f64::from_bits(self.handle_lookup_ms_bits.load(Ordering::Relaxed)),
            last_geometry_phases: evidence.phases.clone(),
            scene_session: self.scene_status().unwrap_or(SceneSessionStatus {
                enabled: self.policy.scene_sessions_enabled,
                fallback_reason: self.policy.scene_sessions_enabled.then(|| {
                    "scene session has not been opened or the backend is unavailable".into()
                }),
                ..SceneSessionStatus::default()
            }),
        }
    }

    pub fn policy(&self) -> ComputePolicy {
        self.policy
    }

    pub fn record_cache_hits(&self, cache_hits: usize) {
        self.last_cache_hits.store(cache_hits, Ordering::Relaxed);
    }

    pub fn cache_metadata(&self, content: &NodeContent) -> GeometryCacheMetadata {
        let triangles = estimate_triangles(content);
        let estimated_bytes = estimate_geometry_bytes(content);
        let effective_budget = self
            .probe
            .get()
            .and_then(|probe| probe.capabilities.as_ref())
            .map(|capabilities| {
                min_budget(
                    self.policy.gpu_budget_bytes,
                    capabilities.memory_budget_bytes,
                )
            })
            .unwrap_or(self.policy.gpu_budget_bytes);
        let gpu_ready = self.gpu_available.load(Ordering::Acquire)
            && self
                .probe
                .get()
                .and_then(|probe| probe.capabilities.as_ref())
                .is_some_and(|capabilities| capabilities.health == "ready");
        let uses_gpu = match self.policy.mode {
            GeometryBackendMode::Cpu => false,
            GeometryBackendMode::Gpu => gpu_ready,
            GeometryBackendMode::Auto => {
                gpu_ready
                    && triangles >= self.policy.gpu_threshold_triangles
                    && estimated_bytes <= effective_budget
            }
        };
        if uses_gpu {
            GeometryCacheMetadata {
                backend: GeometryBackendKind::Gpu,
                kernel_version: GPU_KERNEL_VERSION.into(),
                parity_status: "passed".into(),
            }
        } else {
            GeometryCacheMetadata::cpu()
        }
    }

    async fn probe(&self) -> Probe {
        self.probe
            .get_or_init(|| async {
                let Some(gpu) = &self.gpu else {
                    return Probe {
                        capabilities: None,
                        failure: Some("no native GPU backend was configured".into()),
                    };
                };
                match gpu.initialize().await {
                    Ok(capabilities) => {
                        self.gpu_available
                            .store(capabilities.health == "ready", Ordering::Release);
                        Probe {
                            failure: capabilities.fallback_reason.clone(),
                            capabilities: Some(capabilities),
                        }
                    }
                    Err(error) => {
                        self.gpu_available.store(false, Ordering::Release);
                        let _ = self.runtime_failure.set(error.to_string());
                        Probe {
                            capabilities: None,
                            failure: Some(error.to_string()),
                        }
                    }
                }
            })
            .await
            .clone()
    }

    fn fallback_or_error(
        &self,
        content: NodeContent,
        error: BackendError,
    ) -> Result<ComputeResult, ComputeError> {
        if self.policy.mode == GeometryBackendMode::Auto {
            self.gpu_available.store(false, Ordering::Release);
            let _ = self.runtime_failure.set(error.to_string());
            self.evaluate_cpu(content, Some(error.to_string()))
        } else {
            Err(error.into())
        }
    }

    fn evaluate_cpu(
        &self,
        content: NodeContent,
        fallback_reason: Option<String>,
    ) -> Result<ComputeResult, ComputeError> {
        let mesh = studio_core::geometry::evaluate_node_content(&content)?;
        self.last_backend.store(0, Ordering::Release);
        self.last_kernel_ms_bits
            .store(0.0f64.to_bits(), Ordering::Relaxed);
        self.last_upload_bytes.store(0, Ordering::Relaxed);
        self.last_readback_bytes.store(0, Ordering::Relaxed);
        self.last_cache_hits.store(0, Ordering::Relaxed);
        self.set_evaluation_mode("cpu");
        Ok(ComputeResult {
            mesh,
            backend: GeometryBackendKind::Cpu,
            stats: GeometryExecutionStats {
                fallback_reason: fallback_reason.clone(),
                ..GeometryExecutionStats::default()
            },
            fallback_reason,
            kernel_version: "geometry-kernel-v1".into(),
            parity_status: "reference".into(),
        })
    }

    fn record_stats(&self, stats: &GeometryExecutionStats) {
        self.last_kernel_ms_bits
            .store(stats.kernel_ms.to_bits(), Ordering::Relaxed);
        self.last_upload_bytes
            .store(stats.upload_bytes, Ordering::Relaxed);
        self.last_readback_bytes
            .store(stats.readback_bytes, Ordering::Relaxed);
        self.last_cache_hits
            .store(stats.cache_hits, Ordering::Relaxed);
    }

    fn remember_content(&self, content: NodeContent) {
        let mut last = self.last_compute.lock().expect("compute state poisoned");
        if last
            .as_ref()
            .is_none_or(|previous| previous.content != content)
        {
            *last = Some(LastCompute {
                content,
                resident_ids: Vec::new(),
            });
        }
    }

    fn resident_geometry_id_for(&self, content: &NodeContent) -> Option<ResidentGeometryId> {
        self.resident_by_content
            .lock()
            .expect("resident content index poisoned")
            .get(&resident_content_key(content))
            .cloned()
    }

    fn record_evaluation(
        &self,
        mode: GeometryEvaluationMode,
        evidence: Option<GeometryEvaluationEvidence>,
    ) {
        if let Some(evidence) = evidence {
            self.set_resident_evidence(evidence);
        }
        let label = match mode {
            GeometryEvaluationMode::Cpu => "cpu",
            GeometryEvaluationMode::Cold => "cold",
            GeometryEvaluationMode::Warm => "warm",
            GeometryEvaluationMode::Resident => "resident",
            GeometryEvaluationMode::Snapshot => "snapshot",
        };
        self.set_evaluation_mode(label);
    }

    fn set_resident_evidence(&self, evidence: GeometryEvaluationEvidence) {
        *self
            .resident_evidence
            .lock()
            .expect("resident evidence poisoned") = evidence;
    }

    fn set_evaluation_mode(&self, mode: &str) {
        *self
            .last_evaluation_mode
            .lock()
            .expect("evaluation mode poisoned") = mode.into();
    }

    fn handle_resident_failure(&self, error: &BackendError) {
        if matches!(error, BackendError::DeviceLost { .. }) {
            if let Some(last) = self
                .last_compute
                .lock()
                .expect("compute state poisoned")
                .as_mut()
            {
                last.resident_ids.clear();
            }
            self.gpu_available.store(false, Ordering::Release);
            self.last_backend.store(0, Ordering::Release);
            self.pipeline_warmed.store(false, Ordering::Release);
            self.device_generation.fetch_add(1, Ordering::AcqRel);
            self.resident_by_content
                .lock()
                .expect("resident content index poisoned")
                .clear();
            for record in self
                .registered_geometry
                .lock()
                .expect("registered geometry poisoned")
                .values_mut()
            {
                record.handle.state = ResidentGeometryState::Lost;
            }
            let mut evidence = self
                .resident_evidence
                .lock()
                .expect("resident evidence poisoned");
            evidence.resident_geometries = 0;
            evidence.resident_bytes = 0;
            evidence.resident_hits = 0;
            drop(evidence);
            let _ = self.runtime_failure.set(error.to_string());
            self.set_evaluation_mode("cpu");
        }
    }
}

pub struct ComputeEvaluator<'a> {
    service: &'a ComputeService,
}

impl<'a> ComputeEvaluator<'a> {
    pub fn new(service: &'a ComputeService) -> Self {
        Self { service }
    }
}

impl GeometryEvaluator for ComputeEvaluator<'_> {
    fn select_cache_metadata(&self, content: &NodeContent) -> GeometryCacheMetadata {
        self.service.cache_metadata(content)
    }

    fn evaluate(&self, content: &NodeContent) -> Result<GeometryEvaluation, BackendError> {
        self.service
            .evaluate_with_resident_id_blocking(content.clone(), GeometryJobPriority::Throughput)
            .map(|(result, resident_geometry_id)| GeometryEvaluation {
                mesh: result.mesh,
                metadata: GeometryCacheMetadata {
                    backend: result.backend,
                    kernel_version: result.kernel_version,
                    parity_status: result.parity_status,
                },
                resident_geometry_id,
            })
            .map_err(compute_error_as_backend)
    }

    fn resident_geometry_id(&self, content: &NodeContent) -> Option<ResidentGeometryId> {
        self.service.resident_geometry_id_for(content)
    }
}

fn compute_error_as_backend(error: ComputeError) -> BackendError {
    match error {
        ComputeError::Backend(error) => error,
        ComputeError::GpuUnavailable { reason } => BackendError::Unavailable { reason },
        ComputeError::Geometry(error) => BackendError::Kernel {
            message: error.to_string(),
        },
        ComputeError::ResidencyUnavailable { reason } => BackendError::Incompatible { reason },
    }
}

fn min_budget(host_budget: u64, adapter_budget: u64) -> u64 {
    if adapter_budget == 0 {
        host_budget
    } else {
        host_budget.min(adapter_budget)
    }
}

fn estimate_triangles(content: &NodeContent) -> usize {
    match content {
        NodeContent::Group
        | NodeContent::ComponentInstance(_)
        | NodeContent::InstanceSet(_)
        | NodeContent::Connection(_) => 0,
        NodeContent::Primitive(primitive) => match primitive {
            Primitive::Box { .. } => 12,
            Primitive::Plane { .. } => 2,
            Primitive::Cylinder { segments, .. } => *segments as usize * 12,
            Primitive::Sphere {
                segments, rings, ..
            } => *segments as usize * *rings as usize * 6,
        },
        NodeContent::CustomMesh(mesh) => mesh.indices.len() / 3,
        NodeContent::Sweep {
            points,
            radial_segments,
            ..
        } => points
            .len()
            .saturating_sub(1)
            .saturating_mul(*radial_segments as usize)
            .saturating_mul(6)
            .saturating_add(radial_segments.saturating_sub(2).saturating_mul(6) as usize),
    }
}

fn estimate_vertex_count(content: &NodeContent) -> usize {
    match content {
        NodeContent::Primitive(Primitive::Box { .. }) => 24,
        NodeContent::Primitive(Primitive::Plane { .. }) => 4,
        NodeContent::Primitive(Primitive::Cylinder { segments, .. }) => *segments as usize * 4 + 6,
        NodeContent::Primitive(Primitive::Sphere {
            segments, rings, ..
        }) => (*segments as usize + 1) * (*rings as usize + 1),
        NodeContent::CustomMesh(mesh) => mesh.positions.len(),
        NodeContent::Sweep {
            points,
            radial_segments,
            ..
        } => points.len() * *radial_segments as usize,
        NodeContent::Group
        | NodeContent::ComponentInstance(_)
        | NodeContent::InstanceSet(_)
        | NodeContent::Connection(_) => 0,
    }
}

fn evaluation_mode_label(mode: GeometryEvaluationMode) -> &'static str {
    match mode {
        GeometryEvaluationMode::Cpu => "cpu",
        GeometryEvaluationMode::Cold => "cold",
        GeometryEvaluationMode::Warm => "warm",
        GeometryEvaluationMode::Resident => "resident",
        GeometryEvaluationMode::Snapshot => "snapshot",
    }
}

fn estimate_geometry_bytes(content: &NodeContent) -> u64 {
    let (vertices, indices) = match content {
        NodeContent::Group
        | NodeContent::ComponentInstance(_)
        | NodeContent::InstanceSet(_)
        | NodeContent::Connection(_) => (0usize, 0usize),
        NodeContent::Primitive(Primitive::Box { .. }) => (24, 36),
        NodeContent::Primitive(Primitive::Plane { .. }) => (4, 6),
        NodeContent::Primitive(Primitive::Cylinder { segments, .. }) => {
            (*segments as usize * 4 + 6, *segments as usize * 12)
        }
        NodeContent::Primitive(Primitive::Sphere {
            segments, rings, ..
        }) => (
            (*segments as usize + 1) * (*rings as usize + 1),
            *segments as usize * *rings as usize * 6,
        ),
        NodeContent::CustomMesh(mesh) => (mesh.positions.len(), mesh.indices.len()),
        NodeContent::Sweep {
            points,
            radial_segments,
            ..
        } => (
            points.len() * *radial_segments as usize,
            estimate_triangles(content) * 3,
        ),
    };
    vertices as u64 * 64 + indices as u64 * 32
}

fn monotonic_job_id() -> u64 {
    use std::sync::atomic::AtomicU64;
    static NEXT_JOB: AtomicU64 = AtomicU64::new(1);
    NEXT_JOB.fetch_add(1, Ordering::Relaxed)
}

fn resident_content_key(content: &NodeContent) -> String {
    let bytes = serde_json::to_vec(content).unwrap_or_default();
    format!("{:x}", Sha256::digest(bytes))
}
