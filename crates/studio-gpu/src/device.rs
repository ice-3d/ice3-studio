use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;
use std::time::Instant;

use crate::residency::{GpuGeometryResidency, ResidencyKey, ResidentBatchInput, ResidentBatchPlan};
use crate::resources::{DeviceResources, ResourceStats};
use crate::scene::session::GpuSceneSession;
use studio_core::document::NodeContent;
use studio_core::gpu_contract::{
    BackendCapabilities, BackendError, GeometryBackend, GeometryBackendKind, GeometryBackendMode,
    GeometryBatch, GeometryBatchResult, GeometryDigestSet, GeometryEvaluationMode,
    GeometryExecutionStats, GeometryPhaseEvidence, GeometryResidentBatchResult,
    GeometryResidentEntry, ResidentGeometryHandle, ResidentGeometryId,
};
use studio_core::ids::NodeId;
use studio_core::scene_contract::{
    SceneGeometryReadback, SceneGeometrySnapshotRequest, ScenePatch, SceneSessionId,
    SceneSessionState, SceneSessionStatus, SceneSubmission, SceneVisibilityRequest,
    SceneVisibilityResult,
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
    scene_sessions: BTreeMap<String, GpuSceneSession>,
    next_scene_session: u64,
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
                scene_sessions: BTreeMap::new(),
                next_scene_session: 0,
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

    pub fn create_scene_session(&self, project_id: &str) -> Result<SceneSessionId, BackendError> {
        let _ready = self.ready_device()?;
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.next_scene_session = state.next_scene_session.wrapping_add(1);
        let id = SceneSessionId(format!(
            "{project_id}-scene-{:08}",
            state.next_scene_session
        ));
        state.scene_sessions.insert(
            id.0.clone(),
            GpuSceneSession::new(self.device_generation.load(Ordering::Acquire), 0),
        );
        Ok(id)
    }

    pub fn apply_scene_patch(
        &self,
        session_id: &SceneSessionId,
        patch: ScenePatch,
    ) -> Result<SceneSubmission, BackendError> {
        let ready = self.ready_device()?;
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let session =
            state
                .scene_sessions
                .get_mut(&session_id.0)
                .ok_or_else(|| BackendError::Kernel {
                    message: "GPU scene session is unavailable".into(),
                })?;
        let previous_target = session.target_revision;
        session.target_revision = patch.target_revision;
        if session.state == SceneSessionState::Lost
            || session.device_generation != self.device_generation.load(Ordering::Acquire)
        {
            session.state = SceneSessionState::Failed;
            session.fallback_reason = Some("GPU scene device generation is stale".into());
            return Err(BackendError::DeviceLost {
                message: "GPU scene device generation is stale".into(),
            });
        }
        if session.state == SceneSessionState::Failed {
            return Err(BackendError::Unavailable {
                reason: session
                    .fallback_reason
                    .clone()
                    .unwrap_or_else(|| "GPU scene session has failed".into()),
            });
        }
        let valid_order = patch.base_revision == previous_target
            || (patch.full_rebuild
                && patch.base_revision == 0
                && (session.published_revision == 0
                    || patch.target_revision > patch.base_revision));
        if !valid_order {
            session.state = SceneSessionState::Failed;
            session.fallback_reason = Some(format!(
                "scene revision order is invalid: base {} target {} published {}",
                patch.base_revision, patch.target_revision, session.published_revision
            ));
            return Err(BackendError::Kernel {
                message: session.fallback_reason.clone().unwrap_or_default(),
            });
        }

        if patch.full_rebuild {
            session.geometries.clear();
            session.geometry_topologies.clear();
            session.instances = Default::default();
        }
        let result: Result<SceneSubmission, BackendError> = (|| {
            let planning_started = Instant::now();
            let mut phases = GeometryPhaseEvidence::default();
            let mut upload_bytes = 0u64;
            let mut geometry_dispatch_count = 0usize;
            let mut submission_count = 0usize;
            let mut dirty_range_bytes = 0u64;
            let mut pending = Vec::new();
            let mut cold_inputs = Vec::new();
            let mut cold_geometries = Vec::new();
            for change in patch.geometry {
                let geometry_id = change.geometry_id.clone();
                let topology_digest = change.digests.topology.clone();
                let previous = change
                    .previous_geometry_id
                    .as_ref()
                    .and_then(|id| session.geometries.get(id))
                    .or_else(|| session.geometries.get(&change.geometry_id));
                let update = if !patch.full_rebuild
                    && !change
                        .dirty
                        .contains(studio_core::gpu_contract::GeometryDirtyChannel::Topology)
                    && let Some(previous) = previous
                {
                    Some(ready.residency.update_geometry(
                        &ready.device,
                        &ready.queue,
                        previous,
                        &change.content,
                        change.position_ranges.clone(),
                    )?)
                } else {
                    None
                };
                if let Some(entry) = update {
                    merge_phases(&mut phases, &entry.evidence.phases);
                    upload_bytes = upload_bytes.saturating_add(entry.evidence.stats.upload_bytes);
                    geometry_dispatch_count += 1;
                    submission_count += 1;
                    pending.push(entry.id.clone());
                    session
                        .geometry_bounds
                        .insert(geometry_id.clone(), entry.bounds);
                    session
                        .geometry_topologies
                        .insert(geometry_id.clone(), topology_digest);
                    session.geometries.insert(change.geometry_id, entry.id);
                } else {
                    let mesh = match &change.content {
                        NodeContent::CustomMesh(mesh) => mesh.clone(),
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
                    upload_bytes = upload_bytes
                        .saturating_add(mesh.positions.len() as u64 * 16)
                        .saturating_add(mesh.indices.len() as u64 * 8);
                    cold_geometries.push((change.geometry_id.clone(), topology_digest));
                    cold_inputs.push(ResidentBatchInput {
                        node_id: NodeId::from(change.geometry_id.clone()),
                        content: change.content,
                        mesh,
                        digests: Some(change.digests),
                    });
                }
                dirty_range_bytes = dirty_range_bytes.saturating_add(
                    change
                        .position_ranges
                        .iter()
                        .map(|range| range.count as u64 * 16)
                        .sum::<u64>(),
                );
            }
            if !cold_inputs.is_empty() {
                let outcome =
                    ready
                        .residency
                        .evaluate_batch(&ready.device, &ready.queue, cold_inputs)?;
                merge_phases(&mut phases, &outcome.phases);
                submission_count = submission_count.saturating_add(outcome.submission_count);
                geometry_dispatch_count =
                    geometry_dispatch_count.saturating_add(outcome.entries.len());
                for (entry, (geometry_id, topology_digest)) in
                    outcome.entries.into_iter().zip(cold_geometries)
                {
                    pending.push(entry.id.clone());
                    session
                        .geometry_bounds
                        .insert(geometry_id.clone(), entry.bounds);
                    session
                        .geometry_topologies
                        .insert(geometry_id.clone(), topology_digest);
                    session.geometries.insert(geometry_id, entry.id);
                }
            }
            let instance_update = session.instances.apply_changes(patch.instances);
            let removed = session.instances.remove_nodes(&patch.removed_node_ids);
            let instance_count = session.instances.records().len();
            if instance_count == 0 {
                let previous_instance_buffer = session.instance_buffer.take();
                if let Some(buffer) = previous_instance_buffer {
                    ready.resources.buffers().release(buffer);
                }
                session.instance_capacity = 0;
            } else if session.instance_buffer.is_none()
                || instance_count > session.instance_capacity
            {
                let matrix_data = session.instances.matrix_data();
                let bytes = bytemuck::cast_slice(&matrix_data);
                let previous_instance_buffer = session.instance_buffer.take();
                let buffer = ready.resources.buffers().acquire(
                    &ready.device,
                    crate::resources::BufferRole::DynamicAttribute,
                    BufferUsages::VERTEX | BufferUsages::STORAGE | BufferUsages::COPY_DST,
                    bytes.len() as u64,
                    "ICE3 scene instance matrices",
                )?;
                ready.queue.write_buffer(&buffer.buffer, 0, bytes);
                upload_bytes = upload_bytes.saturating_add(bytes.len() as u64);
                if let Some(previous) = previous_instance_buffer {
                    ready.resources.buffers().release(previous);
                }
                session.instance_buffer = Some(buffer);
                session.instance_capacity = instance_count;
            } else {
                let buffer = session
                    .instance_buffer
                    .as_ref()
                    .expect("instance buffer checked above");
                for (range, matrix_data) in session
                    .instances
                    .matrix_range_data(&instance_update.changed_ranges)
                {
                    let bytes = bytemuck::cast_slice(&matrix_data);
                    ready
                        .queue
                        .write_buffer(&buffer.buffer, range.start as u64 * 64, bytes);
                    upload_bytes = upload_bytes.saturating_add(bytes.len() as u64);
                }
            }
            let bounds_data = session.instances.bounds_data(&session.geometry_bounds);
            let previous_bounds_buffer = session.instance_bounds_buffer.take();
            if bounds_data.is_empty() {
                if let Some(buffer) = previous_bounds_buffer {
                    ready.resources.buffers().release(buffer);
                }
            } else {
                let bytes = bytemuck::cast_slice(&bounds_data);
                let buffer = ready.resources.buffers().acquire(
                    &ready.device,
                    crate::resources::BufferRole::DynamicAttribute,
                    BufferUsages::STORAGE | BufferUsages::COPY_DST,
                    bytes.len() as u64,
                    "ICE3 scene instance bounds",
                )?;
                ready.queue.write_buffer(&buffer.buffer, 0, bytes);
                if let Some(previous) = previous_bounds_buffer {
                    ready.resources.buffers().release(previous);
                }
                session.instance_bounds_buffer = Some(buffer);
            }
            for id in &patch.removed_geometry_ids {
                session.geometries.remove(id);
                session.geometry_bounds.remove(id);
                session.geometry_topologies.remove(id);
            }
            session.pending_geometries.extend(pending.iter().cloned());
            session.pending_submission_count = session
                .pending_submission_count
                .saturating_add(submission_count);
            session.submission_count = session.submission_count.saturating_add(submission_count);
            session.geometry_dispatch_count = session
                .geometry_dispatch_count
                .saturating_add(geometry_dispatch_count);
            session.dirty_range_bytes = session.dirty_range_bytes.saturating_add(dirty_range_bytes);
            phases.planning_ms = elapsed_ms(planning_started);
            session.phases = phases.clone();
            session.state = if session.pending_geometries.is_empty() {
                SceneSessionState::Idle
            } else {
                SceneSessionState::Pending
            };
            if session.pending_geometries.is_empty() {
                session.published_revision = patch.target_revision;
            }
            Ok(SceneSubmission {
                session_id: session_id.clone(),
                target_revision: patch.target_revision,
                submission_count,
                geometry_dispatch_count,
                instance_range_count: instance_update.changed_ranges.len()
                    + removed.removed_ranges.len(),
                upload_bytes,
                readback_bytes: 0,
                phases,
            })
        })();
        if let Err(error) = &result {
            session.state = SceneSessionState::Failed;
            session.fallback_reason = Some(error.to_string());
        }
        result
    }

    pub fn wait_scene_revision(
        &self,
        session_id: &SceneSessionId,
        revision: u64,
    ) -> Result<SceneSessionStatus, BackendError> {
        let wait_started = Instant::now();
        let ready = self.ready_device()?;
        let pending =
            {
                let state = self
                    .state
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let session = state.scene_sessions.get(&session_id.0).ok_or_else(|| {
                    BackendError::Kernel {
                        message: "GPU scene session is unavailable".into(),
                    }
                })?;
                if revision > session.target_revision {
                    return Err(BackendError::Kernel {
                        message: "scene revision has not been queued".into(),
                    });
                }
                session.pending_geometries.clone()
            };
        for geometry in pending {
            if let Err(error) = ready.residency.evaluate_handle(&ready.device, &geometry) {
                let mut state = self
                    .state
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if let Some(session) = state.scene_sessions.get_mut(&session_id.0) {
                    session.state = SceneSessionState::Failed;
                    session.fallback_reason = Some(error.to_string());
                }
                return Err(error);
            }
        }
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let session =
            state
                .scene_sessions
                .get_mut(&session_id.0)
                .ok_or_else(|| BackendError::Kernel {
                    message: "GPU scene session is unavailable".into(),
                })?;
        session.pending_geometries.clear();
        session.pending_submission_count = 0;
        session.published_revision = revision;
        session.state = SceneSessionState::Idle;
        session.phases.wait_ms = session.phases.wait_ms.max(elapsed_ms(wait_started));
        let stats = ready.residency.stats();
        Ok(session.status(session_id, stats.resident_bytes, stats.peak_resident_bytes))
    }

    pub fn scene_status(&self, session_id: &SceneSessionId) -> Option<SceneSessionStatus> {
        let ready = self.ready_device().ok()?;
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let session = state.scene_sessions.get(&session_id.0)?;
        let stats = ready.residency.stats();
        Some(session.status(session_id, stats.resident_bytes, stats.peak_resident_bytes))
    }

    pub fn close_scene_session(&self, session_id: &SceneSessionId) {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .scene_sessions
            .remove(&session_id.0);
    }

    pub fn snapshot_scene_geometries(
        &self,
        session_id: &SceneSessionId,
        request: SceneGeometrySnapshotRequest,
    ) -> Result<Vec<SceneGeometryReadback>, BackendError> {
        let ready = self.ready_device()?;
        let ids =
            {
                let state = self
                    .state
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let session = state.scene_sessions.get(&session_id.0).ok_or_else(|| {
                    BackendError::Kernel {
                        message: "GPU scene session is unavailable".into(),
                    }
                })?;
                if request.target_revision > session.target_revision {
                    return Err(BackendError::Kernel {
                        message: "scene geometry readback references a future revision".into(),
                    });
                }
                request
                    .geometry_ids
                    .iter()
                    .map(|id| {
                        session
                            .geometries
                            .get(id)
                            .cloned()
                            .map(|resident| (id.clone(), resident))
                            .ok_or_else(|| BackendError::Kernel {
                                message: format!("scene geometry is unavailable: {id}"),
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()?
            };
        ids.into_iter()
            .map(|(geometry_id, resident)| {
                let mesh = ready
                    .residency
                    .snapshot(&ready.device, &ready.queue, &resident)?;
                Ok(SceneGeometryReadback {
                    geometry_id,
                    positions: mesh.vertices.iter().map(|vertex| vertex.position).collect(),
                    indices: mesh.indices,
                    topology_changed: false,
                })
            })
            .collect()
    }

    pub fn visible_scene_instances(
        &self,
        session_id: &SceneSessionId,
        planes: [[f32; 4]; 6],
    ) -> Result<Vec<u32>, BackendError> {
        let ready = self.ready_device()?;
        let (matrices, bounds, count) =
            {
                let state = self
                    .state
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let session = state.scene_sessions.get(&session_id.0).ok_or_else(|| {
                    BackendError::Kernel {
                        message: "GPU scene session is unavailable".into(),
                    }
                })?;
                (
                    session
                        .instance_buffer
                        .as_ref()
                        .map(|buffer| buffer.buffer.clone()),
                    session
                        .instance_bounds_buffer
                        .as_ref()
                        .map(|buffer| buffer.buffer.clone()),
                    session.instances.records().len() as u32,
                )
            };
        let (Some(matrices), Some(bounds)) = (matrices, bounds) else {
            return Ok(Vec::new());
        };
        let params = ready.resources.buffers().acquire(
            &ready.device,
            crate::resources::BufferRole::DynamicAttribute,
            BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            96,
            "ICE3 visibility planes",
        )?;
        ready
            .queue
            .write_buffer(&params.buffer, 0, bytemuck::cast_slice(&planes));
        let output = ready.resources.buffers().acquire(
            &ready.device,
            crate::resources::BufferRole::DeviceOutput,
            BufferUsages::STORAGE | BufferUsages::COPY_SRC,
            count as u64 * 4,
            "ICE3 visible instance indices",
        )?;
        let counter = ready.resources.buffers().acquire(
            &ready.device,
            crate::resources::BufferRole::DeviceOutput,
            BufferUsages::STORAGE | BufferUsages::COPY_SRC | BufferUsages::COPY_DST,
            4,
            "ICE3 visible instance count",
        )?;
        ready
            .queue
            .write_buffer(&counter.buffer, 0, &0u32.to_le_bytes());
        let bind_group = ready.device.create_bind_group(&BindGroupDescriptor {
            label: Some("ICE3 visibility bindings"),
            layout: ready.resources.pipelines().visibility_layout(),
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: matrices.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: bounds.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: params.buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: output.buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 4,
                    resource: counter.buffer.as_entire_binding(),
                },
            ],
        });
        let mut encoder = ready
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("ICE3 visibility encoder"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
                label: Some("ICE3 visibility compute"),
                timestamp_writes: None,
            });
            pass.set_pipeline(
                ready
                    .resources
                    .pipelines()
                    .get(crate::resources::PipelineKey::InstanceVisibility)?,
            );
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(count.div_ceil(64), 1, 1);
        }
        let output_staging = ready
            .resources
            .staging()
            .acquire(&ready.device, count as u64 * 4)?;
        let count_staging = ready.resources.staging().acquire(&ready.device, 4)?;
        encoder.copy_buffer_to_buffer(
            &output.buffer,
            0,
            &output_staging.buffer,
            0,
            count as u64 * 4,
        );
        encoder.copy_buffer_to_buffer(&counter.buffer, 0, &count_staging.buffer, 0, 4);
        ready.queue.submit([encoder.finish()]);
        let (output_sender, output_receiver) = mpsc::channel();
        output_staging
            .buffer
            .slice(..count as u64 * 4)
            .map_async(MapMode::Read, move |result| {
                let _ = output_sender.send(result);
            });
        let (count_sender, count_receiver) = mpsc::channel();
        count_staging
            .buffer
            .slice(..4)
            .map_async(MapMode::Read, move |result| {
                let _ = count_sender.send(result);
            });
        ready
            .device
            .poll(PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(30)),
            })
            .map_err(|error| BackendError::Kernel {
                message: format!("visibility wait failed: {error}"),
            })?;
        output_receiver
            .recv()
            .map_err(|_| BackendError::Kernel {
                message: "visibility output mapping callback was lost".into(),
            })?
            .map_err(|error| BackendError::Kernel {
                message: format!("visibility output mapping failed: {error}"),
            })?;
        count_receiver
            .recv()
            .map_err(|_| BackendError::Kernel {
                message: "visibility count mapping callback was lost".into(),
            })?
            .map_err(|error| BackendError::Kernel {
                message: format!("visibility count mapping failed: {error}"),
            })?;
        let count_mapped = count_staging
            .buffer
            .slice(..4)
            .get_mapped_range()
            .map_err(|error| BackendError::Kernel {
                message: format!("visibility count mapping failed: {error}"),
            })?;
        let visible_count = u32::from_le_bytes(count_mapped[..4].try_into().unwrap()) as usize;
        drop(count_mapped);
        count_staging.buffer.unmap();
        let output_mapped = output_staging
            .buffer
            .slice(..visible_count as u64 * 4)
            .get_mapped_range()
            .map_err(|error| BackendError::Kernel {
                message: format!("visibility output mapping failed: {error}"),
            })?;
        let mut visible = bytemuck::cast_slice::<u8, u32>(&output_mapped).to_vec();
        drop(output_mapped);
        output_staging.buffer.unmap();
        ready.resources.staging().complete(1);
        ready.resources.buffers().release(params);
        ready.resources.buffers().release(output);
        ready.resources.buffers().release(counter);
        visible.sort_unstable();
        Ok(visible)
    }

    pub fn visible_scene_instances_response(
        &self,
        session_id: &SceneSessionId,
        request: SceneVisibilityRequest,
    ) -> Result<SceneVisibilityResult, BackendError> {
        Ok(SceneVisibilityResult {
            revision: request.target_revision,
            visible_indices: NativeGpuBackend::visible_scene_instances(
                self,
                session_id,
                request.planes,
            )?,
        })
    }

    #[doc(hidden)]
    pub fn fail_scene_for_test(&self, session_id: &SceneSessionId, reason: &str) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(session) = state.scene_sessions.get_mut(&session_id.0) {
            session.state = SceneSessionState::Failed;
            session.fallback_reason = Some(reason.into());
        }
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

    async fn create_scene_session(&self, project_id: &str) -> Result<SceneSessionId, BackendError> {
        NativeGpuBackend::create_scene_session(self, project_id)
    }

    async fn apply_scene_patch(
        &self,
        session: &SceneSessionId,
        patch: ScenePatch,
    ) -> Result<SceneSubmission, BackendError> {
        NativeGpuBackend::apply_scene_patch(self, session, patch)
    }

    async fn wait_scene_revision(
        &self,
        session: &SceneSessionId,
        revision: u64,
    ) -> Result<SceneSessionStatus, BackendError> {
        NativeGpuBackend::wait_scene_revision(self, session, revision)
    }

    async fn close_scene_session(&self, session: &SceneSessionId) -> Result<(), BackendError> {
        NativeGpuBackend::close_scene_session(self, session);
        Ok(())
    }

    fn scene_status(&self, session: &SceneSessionId) -> Option<SceneSessionStatus> {
        NativeGpuBackend::scene_status(self, session)
    }

    async fn snapshot_scene_geometries(
        &self,
        session: &SceneSessionId,
        request: SceneGeometrySnapshotRequest,
    ) -> Result<Vec<SceneGeometryReadback>, BackendError> {
        NativeGpuBackend::snapshot_scene_geometries(self, session, request)
    }

    async fn visible_scene_instances(
        &self,
        session: &SceneSessionId,
        request: SceneVisibilityRequest,
    ) -> Result<SceneVisibilityResult, BackendError> {
        NativeGpuBackend::visible_scene_instances_response(self, session, request)
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
    for session in state.scene_sessions.values_mut() {
        session.state = SceneSessionState::Lost;
        session.fallback_reason = Some(reason.clone());
    }
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

fn merge_phases(target: &mut GeometryPhaseEvidence, source: &GeometryPhaseEvidence) {
    target.lookup_ms += source.lookup_ms;
    target.planning_ms += source.planning_ms;
    target.validation_ms += source.validation_ms;
    target.encode_ms += source.encode_ms;
    target.submit_ms += source.submit_ms;
    target.snapshot_ms += source.snapshot_ms;
    target.transport_ms += source.transport_ms;
    target.browser_upload_ms += source.browser_upload_ms;
    if let Some(kernel_ms) = source.gpu_kernel_ms {
        *target.gpu_kernel_ms.get_or_insert(0.0) += kernel_ms;
    }
}

fn elapsed_ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1_000.0
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
            scene_sessions: BTreeMap::new(),
            next_scene_session: 0,
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
