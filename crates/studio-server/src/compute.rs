use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use studio_core::document::NodeContent;
use studio_core::geometry::{EvaluatedMesh, Primitive};
use studio_core::gpu_contract::{
    BackendCapabilities, BackendError, GeometryBackend, GeometryBackendKind, GeometryBackendMode,
    GeometryBatch, GeometryExecutionStats, GeometryJobNode, GeometryJobPriority,
};
use studio_core::ids::NodeId;
use studio_core::large_scene::cache::{
    GeometryCacheMetadata, GeometryEvaluation, GeometryEvaluator,
};
use studio_core::validation::DiagnosticCode;
use tokio::sync::{Notify, OnceCell};
use ts_rs::TS;

pub const INITIAL_GPU_THRESHOLD_TRIANGLES: usize = usize::MAX;
pub const MAX_CONCURRENT_GPU_JOBS: usize = 2;
pub const DEFAULT_GPU_BUDGET_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const GPU_KERNEL_VERSION: &str = "geometry-kernel-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComputePolicy {
    pub mode: GeometryBackendMode,
    pub gpu_threshold_triangles: usize,
    pub max_concurrent_gpu_jobs: usize,
    pub gpu_budget_bytes: u64,
}

impl Default for ComputePolicy {
    fn default() -> Self {
        Self {
            mode: GeometryBackendMode::Auto,
            gpu_threshold_triangles: INITIAL_GPU_THRESHOLD_TRIANGLES,
            max_concurrent_gpu_jobs: MAX_CONCURRENT_GPU_JOBS,
            gpu_budget_bytes: DEFAULT_GPU_BUDGET_BYTES,
        }
    }
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
}

impl ComputeError {
    pub fn diagnostic_code(&self) -> &'static str {
        match self {
            Self::GpuUnavailable { .. } => DiagnosticCode::GpuUnavailable.as_str(),
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
    last_backend: AtomicU8,
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
            last_backend: AtomicU8::new(0),
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

    pub async fn evaluate(&self, content: NodeContent) -> Result<ComputeResult, ComputeError> {
        self.evaluate_with_priority(content, GeometryJobPriority::Interactive)
            .await
    }

    pub async fn evaluate_with_priority(
        &self,
        content: NodeContent,
        priority: GeometryJobPriority,
    ) -> Result<ComputeResult, ComputeError> {
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

    pub fn status(&self) -> ComputeStatus {
        let probe = self.probe.get();
        let capabilities = probe.and_then(|probe| probe.capabilities.as_ref());
        let failure = probe.and_then(|probe| probe.failure.clone());
        let cpu_forced = self.policy.mode == GeometryBackendMode::Cpu;
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
            .evaluate_blocking(content.clone(), GeometryJobPriority::Throughput)
            .map(|result| GeometryEvaluation {
                mesh: result.mesh,
                metadata: GeometryCacheMetadata {
                    backend: result.backend,
                    kernel_version: result.kernel_version,
                    parity_status: result.parity_status,
                },
            })
            .map_err(compute_error_as_backend)
    }
}

fn compute_error_as_backend(error: ComputeError) -> BackendError {
    match error {
        ComputeError::Backend(error) => error,
        ComputeError::GpuUnavailable { reason } => BackendError::Unavailable { reason },
        ComputeError::Geometry(error) => BackendError::Kernel {
            message: error.to_string(),
        },
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
