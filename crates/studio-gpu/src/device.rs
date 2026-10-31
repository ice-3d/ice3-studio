use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;
use std::time::Instant;

use crate::residency::{GpuGeometryResidency, ResidencyKey, ResidentBatchInput, ResidentBatchPlan};
use crate::resources::{DeviceResources, ResourceStats};
use studio_core::document::NodeContent;
use studio_core::gpu_contract::{
    BackendCapabilities, BackendError, GeometryBackend, GeometryBackendKind, GeometryBackendMode,
    GeometryBatch, GeometryBatchResult, GeometryDigestSet, GeometryEvaluationMode,
    GeometryExecutionStats, GeometryResidentBatchResult, GeometryResidentEntry,
    ResidentGeometryHandle, ResidentGeometryId,
};
use wgpu::{
    Adapter, BindGroupDescriptor, BindGroupEntry, BufferDescriptor, BufferUsages,
    CommandEncoderDescriptor, ComputePassDescriptor, ComputePipelineDescriptor, Device,
    DeviceDescriptor, ExperimentalFeatures, Features, Instance, InstanceDescriptor, Limits,
    MapMode, MemoryHints, PollType, PowerPreference, Queue, RequestAdapterOptions,
    ShaderModuleDescriptor, ShaderSource, Trace,
};

#[derive(Debug, Clone)]
pub struct GpuHostLimits {
    pub max_buffer_bytes: u64,
    pub max_job_memory_bytes: u64,
    pub max_concurrent_jobs: usize,
}

impl Default for GpuHostLimits {
    fn default() -> Self {
        Self {
            max_buffer_bytes: 512 * 1024 * 1024,
            max_job_memory_bytes: 512 * 1024 * 1024,
            max_concurrent_jobs: 2,
        }
    }
}

struct DeviceState {
    capabilities: Option<BackendCapabilities>,
    _adapter: Option<Adapter>,
    _device: Option<Arc<Device>>,
    _queue: Option<Arc<Queue>>,
    _resources: Option<Arc<DeviceResources>>,
    _residency: Option<Arc<GpuGeometryResidency>>,
    cancelled_jobs: HashSet<String>,
    completed_resident_jobs: BTreeMap<String, GeometryResidentBatchResult>,
    stats: GeometryExecutionStats,
}

struct ReadyDevice {
    device: Arc<Device>,
    queue: Arc<Queue>,
    resources: Arc<DeviceResources>,
    residency: Arc<GpuGeometryResidency>,
}

pub struct NativeGpuBackend {
    mode: GeometryBackendMode,
    state: Arc<Mutex<DeviceState>>,
    limits: GpuHostLimits,
    device_generation: Arc<AtomicU64>,
}

impl NativeGpuBackend {
    pub fn new(mode: GeometryBackendMode) -> Self {
        Self {
            mode,
            state: Arc::new(Mutex::new(DeviceState {
                capabilities: None,
                _adapter: None,
                _device: None,
                _queue: None,
                _resources: None,
                _residency: None,
                cancelled_jobs: HashSet::new(),
                completed_resident_jobs: BTreeMap::new(),
                stats: GeometryExecutionStats::default(),
            })),
            limits: GpuHostLimits::default(),
            device_generation: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn for_test_gpu() -> Option<Self> {
        let backend = Self::new(GeometryBackendMode::Gpu);
        pollster::block_on(backend.initialize()).ok()?;
        Some(backend)
    }

    pub async fn discover(&self) -> Result<CapabilityProbe, BackendError> {
        let instance = Instance::new(InstanceDescriptor::new_without_display_handle());
        let adapter = instance
            .request_adapter(&RequestAdapterOptions {
                power_preference: PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: None,
                apply_limit_buckets: true,
            })
            .await
            .map_err(|error| BackendError::Unavailable {
                reason: error.to_string(),
            })?;
        let info = adapter.get_info();
        if adapter.limits().max_buffer_size < self.limits.max_job_memory_bytes {
            return Err(BackendError::Incompatible {
                reason: "adapter buffer limit is below the host per-job budget".into(),
            });
        }
        let adapter_limits = adapter.limits();
        let mut limits = Limits::default().using_resolution(adapter_limits.clone());
        limits.max_buffer_size = self
            .limits
            .max_buffer_bytes
            .min(adapter_limits.max_buffer_size);
        let required_features = adapter.features() & Features::TIMESTAMP_QUERY;
        let (device, queue) = adapter
            .request_device(&DeviceDescriptor {
                label: Some("ICE3 Studio Native Compute"),
                required_features,
                required_limits: limits,
                experimental_features: ExperimentalFeatures::disabled(),
                memory_hints: MemoryHints::Performance,
                trace: Trace::Off,
            })
            .await
            .map_err(|error| BackendError::Unavailable {
                reason: error.to_string(),
            })?;
        warmup_one_triangle(&device, &queue)?;
        let resources = DeviceResources::create(&device, &queue, &self.limits)?;
        let device_generation = self.device_generation.fetch_add(1, Ordering::AcqRel) + 1;
        let residency = Arc::new(GpuGeometryResidency::new(
            resources.clone(),
            self.limits.max_job_memory_bytes,
            device_generation,
        ));
        let state = self.state.clone();
        device.set_device_lost_callback(move |reason, message| {
            record_device_loss(&state, format!("{reason:?}: {message}"));
        });
        Ok(CapabilityProbe {
            capabilities: BackendCapabilities {
                kind: GeometryBackendKind::Gpu,
                health: "ready".into(),
                accelerated: true,
                adapter_tier: format!("{:?}", info.device_type).to_ascii_lowercase(),
                backend_family: format!("{:?}", info.backend).to_ascii_lowercase(),
                memory_budget_bytes: self.limits.max_buffer_bytes,
                fallback_reason: None,
            },
            adapter,
            device,
            queue,
            resources,
            residency,
        })
    }

    pub async fn capabilities(&self) -> Result<BackendCapabilities, BackendError> {
        if self.mode == GeometryBackendMode::Cpu {
            return Ok(BackendCapabilities::cpu());
        }
        if let Some(capabilities) = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .capabilities
            .clone()
        {
            return Ok(capabilities);
        }
        self.initialize().await?;
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .capabilities
            .clone()
            .ok_or_else(|| BackendError::Unavailable {
                reason: "device initialization did not publish capabilities".into(),
            })
    }

    pub fn evaluate_primitive(
        &self,
        job: crate::PrimitiveJob,
    ) -> Result<crate::MeshReadback, BackendError> {
        let ready = self.ready_device()?;
        let (device, queue, resources) = (&ready.device, &ready.queue, &ready.resources);
        match job.primitive {
            studio_core::geometry::Primitive::Box {
                width,
                height,
                depth,
            } => {
                validate_positive(&[width, height, depth])?;
                crate::evaluate_box(
                    device,
                    queue,
                    resources,
                    crate::PrimitiveGpuParams::for_box(width, height, depth),
                )
            }
            studio_core::geometry::Primitive::Plane { width, depth } => {
                validate_positive(&[width, depth, 1.0])?;
                crate::evaluate_plane(
                    device,
                    queue,
                    resources,
                    crate::PrimitiveGpuParams::for_plane(width, depth),
                )
            }
            studio_core::geometry::Primitive::Cylinder {
                radius,
                height,
                segments,
            } => {
                validate_positive(&[radius, height])?;
                if !(3..=256).contains(&segments) {
                    return Err(BackendError::Kernel {
                        message: "cylinder segments are outside the supported range".into(),
                    });
                }
                crate::evaluate_cylinder(
                    device,
                    queue,
                    resources,
                    crate::PrimitiveGpuParams::for_cylinder(radius, height, segments),
                )
            }
            studio_core::geometry::Primitive::Sphere {
                radius,
                segments,
                rings,
            } => {
                validate_positive(&[radius])?;
                if !(3..=256).contains(&segments) || !(2..=128).contains(&rings) {
                    return Err(BackendError::Kernel {
                        message: "sphere segments or rings are outside the supported range".into(),
                    });
                }
                crate::evaluate_sphere(
                    device,
                    queue,
                    resources,
                    crate::PrimitiveGpuParams::for_sphere(radius, segments, rings),
                )
            }
        }
    }

    pub fn evaluate_sweep(
        &self,
        job: crate::SweepJob,
    ) -> Result<crate::MeshReadback, BackendError> {
        let ready = self.ready_device()?;
        let point_count =
            u32::try_from(job.points.len()).map_err(|_| BackendError::BudgetExceeded)?;
        let params =
            crate::PrimitiveGpuParams::for_sweep(point_count, job.radius, job.radial_segments)
                .ok_or(BackendError::BudgetExceeded)?;
        crate::evaluate_sweep(
            &ready.device,
            &ready.queue,
            &ready.resources,
            params,
            &job.points,
        )
    }

    pub fn evaluate_custom_mesh(
        &self,
        mesh: &studio_core::geometry::TriangleMesh,
    ) -> Result<studio_core::geometry::EvaluatedMesh, BackendError> {
        let ready = self.ready_device()?;
        crate::evaluate_custom_mesh(
            &ready.device,
            &ready.queue,
            ready.resources.pipelines(),
            mesh,
        )
    }

    pub fn evaluate_custom_mesh_resident(
        &self,
        mesh: &studio_core::geometry::TriangleMesh,
    ) -> Result<studio_core::gpu_contract::GeometryResidentEntry, BackendError> {
        let ready = self.ready_device()?;
        ready
            .residency
            .evaluate_custom_mesh(&ready.device, &ready.queue, mesh)
    }

    pub fn update_resident_geometry(
        &self,
        current: &ResidentGeometryId,
        content: &NodeContent,
        ranges: Vec<studio_core::scene_contract::ElementRange>,
    ) -> Result<studio_core::gpu_contract::GeometryResidentEntry, BackendError> {
        let ready = self.ready_device()?;
        ready
            .residency
            .update_geometry(&ready.device, &ready.queue, current, content, ranges)
    }

    pub fn prepare_geometry_digests(
        &self,
        content: &NodeContent,
    ) -> Result<GeometryDigestSet, BackendError> {
        Ok(
            ResidencyKey::for_content(content, "", "geometry-kernel-v1")?
                .digest_set("", "geometry-kernel-v1"),
        )
    }

    pub fn evaluate_with_digests(
        &self,
        mesh: &studio_core::geometry::TriangleMesh,
        digests: GeometryDigestSet,
        geometry_id: &str,
    ) -> Result<GeometryResidentEntry, BackendError> {
        let ready = self.ready_device()?;
        let content = NodeContent::CustomMesh(mesh.clone());
        ready.residency.evaluate_with_digests(
            &ready.device,
            &ready.queue,
            geometry_id,
            &content,
            mesh,
            &digests,
        )
    }

    pub fn evaluate_resident_handle(
        &self,
        id: &ResidentGeometryId,
    ) -> Result<GeometryResidentEntry, BackendError> {
        let ready = self.ready_device()?;
        ready.residency.evaluate_handle(&ready.device, id)
    }

    pub fn evaluate_resident_handle_checked(
        &self,
        handle: &ResidentGeometryHandle,
    ) -> Result<GeometryResidentEntry, BackendError> {
        let ready = self.ready_device()?;
        ready
            .residency
            .evaluate_handle_checked(&ready.device, handle)
    }

    pub fn resident_handle(
        &self,
        id: &ResidentGeometryId,
    ) -> Result<ResidentGeometryHandle, BackendError> {
        self.ready_device()?.residency.handle(id)
    }

    pub fn resident_phase_counters(&self) -> Result<crate::ResidentPhaseCounters, BackendError> {
        Ok(self.ready_device()?.residency.phase_counters())
    }

    pub fn snapshot_geometry(
        &self,
        id: &studio_core::gpu_contract::ResidentGeometryId,
    ) -> Result<studio_core::geometry::EvaluatedMesh, BackendError> {
        let ready = self.ready_device()?;
        ready.residency.snapshot(&ready.device, &ready.queue, id)
    }

    pub fn resident_stats(
        &self,
    ) -> Result<studio_core::gpu_contract::GeometryEvaluationEvidence, BackendError> {
        Ok(self.ready_device()?.residency.stats())
    }

    pub fn pin_resident(
        &self,
        id: &studio_core::gpu_contract::ResidentGeometryId,
    ) -> Result<(), BackendError> {
        self.ready_device()?.residency.pin(id)
    }

    pub fn unpin_resident(&self, id: &studio_core::gpu_contract::ResidentGeometryId) {
        if let Ok(ready) = self.ready_device() {
            ready.residency.unpin(id);
        }
    }

    pub fn evaluate_resident(
        &self,
        batch: GeometryBatch,
    ) -> Result<GeometryResidentBatchResult, BackendError> {
        let plan = ResidentBatchPlan::from_batch(batch);
        if self.is_cancelled(&plan.job_id) {
            return Err(BackendError::Kernel {
                message: "resident batch was cancelled".into(),
            });
        }
        let ready = self.ready_device()?;
        let mut inputs = Vec::with_capacity(plan.nodes.len());
        for node in plan.nodes {
            let mesh =
                match &node.content {
                    studio_core::document::NodeContent::CustomMesh(mesh) => mesh.clone(),
                    content => {
                        let evaluated = studio_core::geometry::evaluate_node_content(content)
                            .map_err(|error| BackendError::Kernel {
                                message: error.to_string(),
                            })?;
                        studio_core::geometry::TriangleMesh {
                            positions: evaluated
                                .vertices
                                .into_iter()
                                .map(|vertex| vertex.position)
                                .collect(),
                            indices: evaluated.indices,
                        }
                    }
                };
            inputs.push(ResidentBatchInput {
                node_id: node.node_id,
                content: node.content,
                mesh,
                digests: node.digests,
            });
        }
        let outcome = ready
            .residency
            .evaluate_batch(&ready.device, &ready.queue, inputs)?;
        if self.is_cancelled(&plan.job_id) {
            return Err(BackendError::Kernel {
                message: "resident batch was cancelled".into(),
            });
        }
        let mut evidence = outcome
            .entries
            .last()
            .map(|entry| entry.evidence.clone())
            .unwrap_or_default();
        evidence.mode = if outcome.submission_count == 0 {
            GeometryEvaluationMode::Resident
        } else {
            GeometryEvaluationMode::Cold
        };
        evidence.submission_count = outcome.submission_count;
        evidence.phases = outcome.phases;
        evidence.stats.readback_bytes = 0;
        let result = GeometryResidentBatchResult {
            job_id: plan.job_id.clone(),
            backend: GeometryBackendKind::Gpu,
            entries: outcome.entries,
            evidence,
        };
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .completed_resident_jobs
            .insert(plan.job_id, result.clone());
        Ok(result)
    }

    pub fn poll_resident(&self, job_id: &str) -> Option<GeometryResidentBatchResult> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .completed_resident_jobs
            .remove(job_id)
    }

    pub fn wait_resident(&self, job_id: &str) -> Result<GeometryResidentBatchResult, BackendError> {
        self.poll_resident(job_id)
            .ok_or_else(|| BackendError::Kernel {
                message: if self.is_cancelled(job_id) {
                    "resident batch was cancelled".into()
                } else {
                    "resident batch result is not available".into()
                },
            })
    }

    fn mark_cancelled(&self, job_id: &str) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.cancelled_jobs.insert(job_id.to_owned());
        state.completed_resident_jobs.remove(job_id);
    }

    fn is_cancelled(&self, job_id: &str) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .cancelled_jobs
            .contains(job_id)
    }

    pub fn resource_stats(&self) -> ResourceStats {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            ._resources
            .as_ref()
            .map(|resources| resources.stats())
            .unwrap_or_default()
    }

    pub fn ready_staging_slots(&self) -> Result<usize, BackendError> {
        let ready = self.ready_device()?;
        Ok(ready.resources.ready_staging_slots())
    }

    pub fn warm_pipelines_for_test(&self) -> Result<(), BackendError> {
        let _ = self.ready_device()?;
        Ok(())
    }

    fn ready_device(&self) -> Result<ReadyDevice, BackendError> {
        if self.mode == GeometryBackendMode::Cpu {
            return Err(BackendError::Unavailable {
                reason: "GPU mode is disabled".into(),
            });
        }
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(capabilities) = &state.capabilities
            && capabilities.health != "ready"
        {
            return Err(BackendError::DeviceLost {
                message: capabilities
                    .fallback_reason
                    .clone()
                    .unwrap_or_else(|| "GPU device is no longer ready".into()),
            });
        }
        match (
            &state._device,
            &state._queue,
            &state._resources,
            &state._residency,
        ) {
            (Some(device), Some(queue), Some(resources), Some(residency)) => Ok(ReadyDevice {
                device: device.clone(),
                queue: queue.clone(),
                resources: resources.clone(),
                residency: residency.clone(),
            }),
            _ => Err(BackendError::Unavailable {
                reason: "GPU device has not been initialized".into(),
            }),
        }
    }
}

pub struct CapabilityProbe {
    pub capabilities: BackendCapabilities,
    adapter: Adapter,
    device: Device,
    queue: Queue,
    resources: Arc<DeviceResources>,
    residency: Arc<GpuGeometryResidency>,
}

#[async_trait::async_trait]
impl GeometryBackend for NativeGpuBackend {
    async fn initialize(&self) -> Result<BackendCapabilities, BackendError> {
        if self.mode == GeometryBackendMode::Cpu {
            return Ok(BackendCapabilities::cpu());
        }
        if let Some(capabilities) = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .capabilities
            .clone()
        {
            if capabilities.health != "ready" {
                return Err(BackendError::DeviceLost {
                    message: capabilities
                        .fallback_reason
                        .unwrap_or_else(|| "GPU device is no longer ready".into()),
                });
            }
            return Ok(capabilities);
        }
        let probe = self.discover().await?;
        let capabilities = probe.capabilities.clone();
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.capabilities = Some(capabilities.clone());
        state._adapter = Some(probe.adapter);
        state._device = Some(Arc::new(probe.device));
        state._queue = Some(Arc::new(probe.queue));
        state._resources = Some(probe.resources);
        state._residency = Some(probe.residency);
        Ok(capabilities)
    }

    async fn evaluate(&self, batch: GeometryBatch) -> Result<GeometryBatchResult, BackendError> {
        if self.mode == GeometryBackendMode::Cpu {
            return evaluate_cpu_batch(batch);
        }
        let ready = self.ready_device()?;
        let started = Instant::now();
        let mut meshes = std::collections::BTreeMap::new();
        let mut upload_bytes = 0u64;
        let mut readback_bytes = 0u64;
        for node in batch.nodes {
            let (upload, readback) = estimate_transfer(&node.content);
            upload_bytes = upload_bytes.saturating_add(upload);
            readback_bytes = readback_bytes.saturating_add(readback);
            let mesh = match &node.content {
                studio_core::document::NodeContent::Group => {
                    studio_core::geometry::EvaluatedMesh::empty()
                }
                studio_core::document::NodeContent::Primitive(_) => {
                    let readback = self.evaluate_primitive(crate::PrimitiveJob {
                        primitive: match &node.content {
                            studio_core::document::NodeContent::Primitive(primitive) => {
                                primitive.clone()
                            }
                            _ => unreachable!(),
                        },
                    })?;
                    readback_to_mesh(readback)
                }
                studio_core::document::NodeContent::CustomMesh(mesh) => {
                    crate::evaluate_custom_mesh(
                        &ready.device,
                        &ready.queue,
                        ready.resources.pipelines(),
                        mesh,
                    )?
                }
                studio_core::document::NodeContent::Sweep {
                    points,
                    radius,
                    radial_segments,
                } => {
                    let readback = self.evaluate_sweep(crate::SweepJob {
                        points: points.clone(),
                        radius: *radius,
                        radial_segments: *radial_segments,
                    })?;
                    readback_to_mesh(readback)
                }
                studio_core::document::NodeContent::ComponentInstance(_)
                | studio_core::document::NodeContent::InstanceSet(_)
                | studio_core::document::NodeContent::Connection(_) => {
                    return Err(BackendError::Kernel {
                        message: "GPU batch contains unresolved scene content".into(),
                    });
                }
            };
            meshes.insert(node.node_id, mesh);
        }
        Ok(GeometryBatchResult {
            job_id: batch.job_id,
            backend: GeometryBackendKind::Gpu,
            meshes,
            stats: GeometryExecutionStats {
                kernel_ms: started.elapsed().as_secs_f64() * 1_000.0,
                upload_bytes,
                readback_bytes,
                gpu_memory_estimate_bytes: upload_bytes.saturating_add(readback_bytes),
                ..GeometryExecutionStats::default()
            },
            diagnostics: Vec::new(),
        })
    }

    async fn evaluate_resident(
        &self,
        batch: GeometryBatch,
    ) -> Result<GeometryResidentBatchResult, BackendError> {
        NativeGpuBackend::evaluate_resident(self, batch)
    }

    async fn cancel(&self, job_id: &str) -> Result<(), BackendError> {
        self.mark_cancelled(job_id);
        Ok(())
    }

    fn stats(&self) -> GeometryExecutionStats {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .stats
            .clone()
    }
}

fn record_device_loss(state: &Mutex<DeviceState>, reason: String) {
    let mut state = state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let residency = state._residency.take();
    if let Some(capabilities) = state.capabilities.as_mut() {
        capabilities.health = "unavailable".into();
        capabilities.accelerated = false;
        capabilities.fallback_reason = Some(reason);
    }
    drop(state);
    if let Some(residency) = residency {
        residency.clear();
    }
}

fn validate_positive(values: &[f32]) -> Result<(), BackendError> {
    if values.iter().all(|value| value.is_finite() && *value > 0.0) {
        Ok(())
    } else {
        Err(BackendError::Kernel {
            message: "primitive dimensions must be finite and positive".into(),
        })
    }
}

fn evaluate_cpu_batch(batch: GeometryBatch) -> Result<GeometryBatchResult, BackendError> {
    let mut meshes = std::collections::BTreeMap::new();
    let mut diagnostics = Vec::new();
    for node in batch.nodes {
        match studio_core::geometry::evaluate_node_content(&node.content) {
            Ok(mesh) => {
                meshes.insert(node.node_id, mesh);
            }
            Err(error) => diagnostics.push(studio_core::validation::Diagnostic {
                code: studio_core::validation::DiagnosticCode::GeometryInvalid,
                operation_index: None,
                object_id: Some(node.node_id.as_str().into()),
                pointer: format!("/nodes/{}/content", node.node_id.as_str()),
                message: error.to_string(),
            }),
        }
    }
    Ok(GeometryBatchResult {
        job_id: batch.job_id,
        backend: GeometryBackendKind::Cpu,
        meshes,
        stats: GeometryExecutionStats::default(),
        diagnostics,
    })
}

fn readback_to_mesh(readback: crate::MeshReadback) -> studio_core::geometry::EvaluatedMesh {
    let mut minimum = [f32::INFINITY; 3];
    let mut maximum = [f32::NEG_INFINITY; 3];
    for vertex in &readback.vertices {
        for axis in 0..3 {
            minimum[axis] = minimum[axis].min(vertex.position[axis]);
            maximum[axis] = maximum[axis].max(vertex.position[axis]);
        }
    }
    studio_core::geometry::EvaluatedMesh {
        vertices: readback.vertices,
        indices: readback.indices,
        bounds: studio_core::geometry::Bounds {
            min: studio_core::math::Vec3 {
                x: minimum[0],
                y: minimum[1],
                z: minimum[2],
            },
            max: studio_core::math::Vec3 {
                x: maximum[0],
                y: maximum[1],
                z: maximum[2],
            },
        },
    }
}

fn estimate_transfer(content: &studio_core::document::NodeContent) -> (u64, u64) {
    use studio_core::document::NodeContent;
    use studio_core::geometry::Primitive;

    let (upload_positions, upload_indices, readback_vertices, readback_indices) = match content {
        NodeContent::Group
        | NodeContent::ComponentInstance(_)
        | NodeContent::InstanceSet(_)
        | NodeContent::Connection(_) => (0usize, 0usize, 0usize, 0usize),
        NodeContent::Primitive(Primitive::Box { .. }) => (0, 0, 24, 36),
        NodeContent::Primitive(Primitive::Plane { .. }) => (0, 0, 4, 6),
        NodeContent::Primitive(Primitive::Cylinder { segments, .. }) => {
            (0, 0, *segments as usize * 4 + 6, *segments as usize * 12)
        }
        NodeContent::Primitive(Primitive::Sphere {
            segments, rings, ..
        }) => (
            0,
            0,
            (*segments as usize + 1) * (*rings as usize + 1),
            *segments as usize * *rings as usize * 6,
        ),
        NodeContent::CustomMesh(mesh) => (
            mesh.positions.len(),
            mesh.indices.len(),
            mesh.positions.len(),
            mesh.indices.len(),
        ),
        NodeContent::Sweep {
            points,
            radial_segments,
            ..
        } => {
            let vertices = points.len() * *radial_segments as usize;
            let triangles = points
                .len()
                .saturating_sub(1)
                .saturating_mul(*radial_segments as usize)
                .saturating_mul(6)
                .saturating_add(radial_segments.saturating_sub(2).saturating_mul(6) as usize);
            (points.len(), 0, vertices, triangles * 3)
        }
    };
    (
        upload_positions as u64 * 12 + upload_indices as u64 * 4,
        readback_vertices as u64 * 24 + readback_indices as u64 * 4,
    )
}

const WARMUP_SHADER: &str = r#"
@group(0) @binding(0) var<storage, read> input_values: array<f32>;
@group(0) @binding(1) var<storage, read_write> output_values: array<f32>;

@compute @workgroup_size(1)
fn main(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if (invocation.x >= 3u) {
        return;
    }
    output_values[invocation.x] = input_values[invocation.x] * 2.0 + 1.0;
}
"#;

fn warmup_one_triangle(device: &Device, queue: &Queue) -> Result<(), BackendError> {
    let input_values = [0.0f32, 1.0, 2.0];
    let input = device.create_buffer(&BufferDescriptor {
        label: Some("ICE3 GPU warm-up input"),
        size: std::mem::size_of_val(&input_values) as u64,
        usage: BufferUsages::STORAGE,
        mapped_at_creation: true,
    });
    let mut mapped_input =
        input
            .slice(..)
            .get_mapped_range_mut()
            .map_err(|error| BackendError::Kernel {
                message: format!("GPU warm-up input mapping failed: {error}"),
            })?;
    mapped_input.copy_from_slice(bytemuck::cast_slice(&input_values));
    drop(mapped_input);
    input.unmap();

    let output = device.create_buffer(&BufferDescriptor {
        label: Some("ICE3 GPU warm-up output"),
        size: std::mem::size_of_val(&input_values) as u64,
        usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("ICE3 GPU warm-up readback"),
        size: std::mem::size_of_val(&input_values) as u64,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("ICE3 GPU warm-up shader"),
        source: ShaderSource::Wgsl(WARMUP_SHADER.into()),
    });
    let pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
        label: Some("ICE3 GPU warm-up pipeline"),
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("ICE3 GPU warm-up bindings"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: input.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: output.as_entire_binding(),
            },
        ],
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("ICE3 GPU warm-up encoder"),
    });
    {
        let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
            label: Some("ICE3 GPU warm-up pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(3, 1, 1);
    }
    encoder.copy_buffer_to_buffer(
        &output,
        0,
        &readback,
        0,
        std::mem::size_of_val(&input_values) as u64,
    );
    queue.submit([encoder.finish()]);

    let slice = readback.slice(..);
    let (sender, receiver) = mpsc::channel();
    slice.map_async(MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    device
        .poll(PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(5)),
        })
        .map_err(|error| BackendError::Kernel {
            message: format!("GPU warm-up poll failed: {error}"),
        })?;
    receiver
        .recv_timeout(Duration::from_secs(5))
        .map_err(|error| BackendError::Kernel {
            message: format!("GPU warm-up readback did not complete: {error}"),
        })?
        .map_err(|error| BackendError::Kernel {
            message: format!("GPU warm-up readback failed: {error}"),
        })?;
    let mapped = slice
        .get_mapped_range()
        .map_err(|error| BackendError::Kernel {
            message: format!("GPU warm-up readback mapping failed: {error}"),
        })?;
    let actual: &[f32] = bytemuck::cast_slice(&mapped);
    let expected = [1.0f32, 3.0, 5.0];
    let matches = actual.len() == expected.len()
        && actual
            .iter()
            .zip(expected)
            .all(|(actual, expected)| (*actual - expected).abs() <= f32::EPSILON);
    drop(mapped);
    readback.unmap();
    if !matches {
        return Err(BackendError::Parity);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_loss_marks_cached_capabilities_unavailable() {
        let state = Mutex::new(DeviceState {
            capabilities: Some(BackendCapabilities {
                kind: GeometryBackendKind::Gpu,
                health: "ready".into(),
                accelerated: true,
                adapter_tier: "integrated".into(),
                backend_family: "native".into(),
                memory_budget_bytes: 512 * 1024 * 1024,
                fallback_reason: None,
            }),
            _adapter: None,
            _device: None,
            _queue: None,
            _resources: None,
            _residency: None,
            cancelled_jobs: HashSet::new(),
            completed_resident_jobs: BTreeMap::new(),
            stats: GeometryExecutionStats::default(),
        });

        record_device_loss(&state, "adapter removed".into());

        let capabilities = state
            .lock()
            .unwrap()
            .capabilities
            .clone()
            .expect("capabilities remain observable after loss");
        assert_eq!(capabilities.health, "unavailable");
        assert!(!capabilities.accelerated);
        assert_eq!(
            capabilities.fallback_reason.as_deref(),
            Some("adapter removed")
        );
    }
}
