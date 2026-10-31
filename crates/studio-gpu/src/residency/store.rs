use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use studio_core::document::NodeContent;
use studio_core::geometry::{Bounds, EvaluatedMesh, TriangleMesh};
use studio_core::gpu_contract::{
    BackendError, GeometryDigestSet, GeometryEvaluationEvidence, GeometryEvaluationMode,
    GeometryExecutionStats, GeometryResidentEntry, ResidentGeometryHandle, ResidentGeometryId,
    ResidentGeometryState,
};
use studio_core::ids::NodeId;

use crate::kernels::mesh::{
    ResidentMeshGpu, create_resident_custom_mesh, encode_resident_custom_mesh,
    prepare_resident_custom_mesh, snapshot_resident_custom_mesh,
};
use crate::residency::ResidencyKey;
use crate::resources::DeviceResources;

const RESIDENT_KERNEL_VERSION: &str = "geometry-kernel-v1";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResidentPhaseCounters {
    pub full_hashes: u64,
    pub upload_bytes: u64,
    pub fence_waits: u64,
}

pub(crate) struct ResidentBatchInput {
    pub node_id: NodeId,
    pub content: NodeContent,
    pub mesh: TriangleMesh,
    pub digests: Option<GeometryDigestSet>,
}

pub(crate) struct ResidentBatchOutcome {
    pub entries: Vec<GeometryResidentEntry>,
    pub submission_count: usize,
    pub phases: studio_core::gpu_contract::GeometryPhaseEvidence,
}

pub struct ResidentGeometryRecord {
    pub id: ResidentGeometryId,
    pub node_id: NodeId,
    pub key: ResidencyKey,
    pub content: NodeContent,
    pub bounds: Bounds,
    pub vertex_count: usize,
    pub triangle_count: usize,
    pub handle: ResidentGeometryHandle,
    gpu: ResidentMeshGpu,
    source: TriangleMesh,
}

struct StoredEntry {
    record: ResidentGeometryRecord,
    pinned: bool,
    last_used: u64,
}

#[derive(Default)]
struct StoreState {
    entries: BTreeMap<String, StoredEntry>,
    generation: u64,
    resident_bytes: u64,
    peak_resident_bytes: u64,
    topology_hits: usize,
    resident_hits: usize,
    evictions: usize,
    phase_counters: ResidentPhaseCounters,
    last_evidence: GeometryEvaluationEvidence,
}

pub struct GpuGeometryResidency {
    resources: Arc<DeviceResources>,
    max_bytes: u64,
    device_generation: u64,
    state: Mutex<StoreState>,
}

impl GpuGeometryResidency {
    pub(crate) fn new(
        resources: Arc<DeviceResources>,
        max_bytes: u64,
        device_generation: u64,
    ) -> Self {
        Self {
            resources,
            max_bytes,
            device_generation,
            state: Mutex::new(StoreState {
                last_evidence: evidence(GeometryEvaluationMode::Cpu, 0, 0, 0, 0),
                ..StoreState::default()
            }),
        }
    }

    pub(crate) fn evaluate_custom_mesh(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mesh: &TriangleMesh,
    ) -> Result<GeometryResidentEntry, BackendError> {
        let content = NodeContent::CustomMesh(mesh.clone());
        self.evaluate_content(
            device,
            queue,
            &NodeId::from("resident-mesh"),
            &content,
            mesh,
        )
        .map(|(entry, _)| entry)
    }

    pub(crate) fn evaluate_content(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        node_id: &NodeId,
        content: &NodeContent,
        evaluated_mesh: &TriangleMesh,
    ) -> Result<(GeometryResidentEntry, bool), BackendError> {
        let key = ResidencyKey::for_content(content, "", RESIDENT_KERNEL_VERSION)?;
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .phase_counters
            .full_hashes += 1;
        self.evaluate_key(device, queue, node_id, content, evaluated_mesh, key)
    }

    pub(crate) fn evaluate_with_digests(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        geometry_id: &str,
        content: &NodeContent,
        evaluated_mesh: &TriangleMesh,
        digests: &GeometryDigestSet,
    ) -> Result<GeometryResidentEntry, BackendError> {
        let node_id = NodeId::from(geometry_id);
        let key = ResidencyKey::from_digests(digests);
        let (mut entry, _) =
            self.evaluate_key(device, queue, &node_id, content, evaluated_mesh, key)?;
        entry.node_id = node_id;
        Ok(entry)
    }

    pub(crate) fn evaluate_batch(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        inputs: Vec<ResidentBatchInput>,
    ) -> Result<ResidentBatchOutcome, BackendError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut seen_new = BTreeSet::new();
        let mut keys = Vec::with_capacity(inputs.len());
        let mut prepared = Vec::new();
        let mut phases = studio_core::gpu_contract::GeometryPhaseEvidence::default();

        for input in inputs {
            let key = match &input.digests {
                Some(digests) => ResidencyKey::from_digests(digests),
                None => {
                    state.phase_counters.full_hashes += 1;
                    ResidencyKey::for_content(&input.content, "", RESIDENT_KERNEL_VERSION)?
                }
            };
            let result_key = key.result.clone();
            keys.push((input.node_id.clone(), result_key.clone()));
            if state.entries.contains_key(&result_key) || !seen_new.insert(result_key.clone()) {
                continue;
            }
            let pending =
                prepare_resident_custom_mesh(device, queue, &self.resources, &input.mesh)?;
            let bytes = pending.byte_size();
            self.evict_for(&mut state, bytes)?;
            phases.validation_ms += pending.phases().validation_ms;
            phases.planning_ms += pending.phases().planning_ms;
            prepared.push((key, input, pending));
        }

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("ICE3 resident batch encoder"),
        });
        for (_, _, pending) in &mut prepared {
            encode_resident_custom_mesh(&mut encoder, &self.resources, pending);
            phases.encode_ms += pending.phases().encode_ms;
        }
        let submission_count = usize::from(!prepared.is_empty());
        let submit_started = std::time::Instant::now();
        if submission_count > 0 {
            queue.submit([encoder.finish()]);
        }
        phases.submit_ms = elapsed_ms(submit_started);
        phases.wait_ms = 0.0;

        let mut inserted = BTreeSet::new();
        for (key, input, pending) in prepared {
            let gpu = pending.finish();
            let bytes = gpu.byte_size();
            let bounds = mesh_bounds(&input.mesh)?;
            let geometry_id = input.node_id.to_string();
            let record = ResidentGeometryRecord {
                id: ResidentGeometryId(key.result.clone()),
                node_id: input.node_id,
                key: key.clone(),
                content: input.content,
                bounds,
                vertex_count: input.mesh.positions.len(),
                triangle_count: gpu.triangle_count as usize,
                handle: ResidentGeometryHandle {
                    id: ResidentGeometryId(key.result.clone()),
                    geometry_id,
                    digests: key.digest_set("", RESIDENT_KERNEL_VERSION),
                    vertex_count: input.mesh.positions.len(),
                    triangle_count: gpu.triangle_count as usize,
                    device_generation: self.device_generation,
                    state: ResidentGeometryState::Ready,
                },
                gpu,
                source: input.mesh,
            };
            state.phase_counters.upload_bytes =
                state.phase_counters.upload_bytes.saturating_add(bytes);
            state.resident_bytes = state.resident_bytes.saturating_add(bytes);
            state.peak_resident_bytes = state.peak_resident_bytes.max(state.resident_bytes);
            inserted.insert(key.result.clone());
            let next_generation = state.generation.wrapping_add(1);
            state.entries.insert(
                key.result,
                StoredEntry {
                    record,
                    pinned: false,
                    last_used: next_generation,
                },
            );
            state.generation = next_generation;
        }

        let mut entries = Vec::with_capacity(keys.len());
        for (node_id, key) in keys {
            let stored = state
                .entries
                .get(&key)
                .ok_or_else(|| BackendError::Kernel {
                    message: "resident batch entry was not published".into(),
                })?;
            let mut entry = resident_entry(
                &stored.record,
                if inserted.contains(&key) {
                    GeometryEvaluationMode::Cold
                } else {
                    GeometryEvaluationMode::Resident
                },
                &state,
                usize::from(inserted.contains(&key)),
                &node_id,
            );
            entry.evidence.phases = phases.clone();
            entries.push(entry);
        }
        state.topology_hits += entries
            .iter()
            .filter(|entry| entry.mode == GeometryEvaluationMode::Resident)
            .count();
        state.resident_hits += entries
            .iter()
            .filter(|entry| entry.mode == GeometryEvaluationMode::Resident)
            .count();
        state.last_evidence = entries
            .last()
            .map(|entry| entry.evidence.clone())
            .unwrap_or_default();
        Ok(ResidentBatchOutcome {
            entries,
            submission_count,
            phases,
        })
    }

    pub(crate) fn evaluate_handle(
        &self,
        id: &ResidentGeometryId,
    ) -> Result<GeometryResidentEntry, BackendError> {
        let lookup_started = std::time::Instant::now();
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.generation = state.generation.wrapping_add(1);
        let generation = state.generation;
        {
            let stored = state
                .entries
                .get_mut(&id.0)
                .ok_or_else(|| BackendError::Kernel {
                    message: "resident geometry handle is unavailable".into(),
                })?;
            if stored.record.handle.device_generation != self.device_generation
                || stored.record.handle.state != ResidentGeometryState::Ready
            {
                return Err(BackendError::DeviceLost {
                    message: "resident geometry handle generation is stale".into(),
                });
            }
            stored.last_used = generation;
        }
        state.resident_hits += 1;
        let stored = state
            .entries
            .get(&id.0)
            .expect("resident entry checked above");
        let mut resident = resident_entry(
            &stored.record,
            GeometryEvaluationMode::Resident,
            &state,
            0,
            &stored.record.node_id,
        );
        resident.evidence.phases.lookup_ms = elapsed_ms(lookup_started);
        state.last_evidence = resident.evidence.clone();
        Ok(resident)
    }

    pub(crate) fn handle(
        &self,
        id: &ResidentGeometryId,
    ) -> Result<ResidentGeometryHandle, BackendError> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .entries
            .get(&id.0)
            .map(|entry| entry.record.handle.clone())
            .ok_or_else(|| BackendError::Kernel {
                message: "resident geometry handle is unavailable".into(),
            })
    }

    pub(crate) fn evaluate_handle_checked(
        &self,
        handle: &ResidentGeometryHandle,
    ) -> Result<GeometryResidentEntry, BackendError> {
        if handle.device_generation != self.device_generation
            || handle.state != ResidentGeometryState::Ready
        {
            return Err(BackendError::DeviceLost {
                message: "resident geometry handle generation is stale".into(),
            });
        }
        self.evaluate_handle(&handle.id)
    }

    pub(crate) fn phase_counters(&self) -> ResidentPhaseCounters {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .phase_counters
    }

    fn evaluate_key(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        node_id: &NodeId,
        content: &NodeContent,
        evaluated_mesh: &TriangleMesh,
        key: ResidencyKey,
    ) -> Result<(GeometryResidentEntry, bool), BackendError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.generation = state.generation.wrapping_add(1);
        let generation = state.generation;
        if state.entries.contains_key(&key.result) {
            state
                .entries
                .get_mut(&key.result)
                .expect("resident entry checked above")
                .last_used = generation;
            state.resident_hits += 1;
            state.topology_hits += 1;
            let resident = resident_entry(
                &state
                    .entries
                    .get(&key.result)
                    .expect("resident entry checked above")
                    .record,
                GeometryEvaluationMode::Resident,
                &state,
                0,
                node_id,
            );
            state.last_evidence = resident.evidence.clone();
            return Ok((resident, false));
        }

        let gpu = create_resident_custom_mesh(device, queue, &self.resources, evaluated_mesh)?;
        let phases = gpu.phases.clone();
        let bytes = gpu.byte_size();
        state.phase_counters.upload_bytes = state.phase_counters.upload_bytes.saturating_add(bytes);
        state.phase_counters.fence_waits += 1;
        self.evict_for(&mut state, bytes)?;
        let bounds = mesh_bounds(evaluated_mesh)?;
        let digests = key.digest_set("", RESIDENT_KERNEL_VERSION);
        let handle = ResidentGeometryHandle {
            id: ResidentGeometryId(key.result.clone()),
            geometry_id: node_id.to_string(),
            digests,
            vertex_count: evaluated_mesh.positions.len(),
            triangle_count: gpu.triangle_count as usize,
            device_generation: self.device_generation,
            state: ResidentGeometryState::Ready,
        };
        let record = ResidentGeometryRecord {
            id: ResidentGeometryId(key.result.clone()),
            node_id: node_id.clone(),
            key: key.clone(),
            content: content.clone(),
            bounds,
            vertex_count: evaluated_mesh.positions.len(),
            triangle_count: gpu.triangle_count as usize,
            handle,
            gpu,
            source: evaluated_mesh.clone(),
        };
        state.resident_bytes = state.resident_bytes.saturating_add(bytes);
        state.peak_resident_bytes = state.peak_resident_bytes.max(state.resident_bytes);
        state.generation = generation;
        let mut entry = resident_entry(&record, GeometryEvaluationMode::Cold, &state, 1, node_id);
        entry.evidence.phases = phases;
        state.entries.insert(
            key.result,
            StoredEntry {
                record,
                pinned: false,
                last_used: generation,
            },
        );
        state.last_evidence = entry.evidence.clone();
        Ok((entry, true))
    }

    pub(crate) fn snapshot(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: &ResidentGeometryId,
    ) -> Result<EvaluatedMesh, BackendError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let snapshot_started = std::time::Instant::now();
        let (mesh, readback_bytes) = {
            let entry = state
                .entries
                .get(&id.0)
                .ok_or_else(|| BackendError::Kernel {
                    message: "resident geometry is not available".into(),
                })?;
            let readback_bytes = entry.record.gpu.snapshot_readback_bytes();
            let mesh = snapshot_resident_custom_mesh(
                device,
                queue,
                &self.resources,
                &entry.record.gpu,
                &entry.record.source,
            )?;
            (mesh, readback_bytes)
        };
        state.last_evidence.mode = GeometryEvaluationMode::Snapshot;
        state.last_evidence.stats.readback_bytes = state
            .last_evidence
            .stats
            .readback_bytes
            .saturating_add(readback_bytes);
        state.last_evidence.phases.snapshot_ms = elapsed_ms(snapshot_started);
        Ok(mesh)
    }

    pub(crate) fn stats(&self) -> GeometryEvaluationEvidence {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .last_evidence
            .clone()
    }

    pub(crate) fn pin(&self, id: &ResidentGeometryId) -> Result<(), BackendError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let entry = state
            .entries
            .get_mut(&id.0)
            .ok_or_else(|| BackendError::Kernel {
                message: "resident geometry is not available".into(),
            })?;
        entry.pinned = true;
        Ok(())
    }

    pub(crate) fn unpin(&self, id: &ResidentGeometryId) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(entry) = state.entries.get_mut(&id.0) {
            entry.pinned = false;
        }
    }

    pub(crate) fn clear(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let entries = std::mem::take(&mut state.entries);
        state.resident_bytes = 0;
        drop(state);
        for (_, entry) in entries {
            entry.record.gpu.release(&self.resources);
        }
    }

    fn evict_for(&self, state: &mut StoreState, additional: u64) -> Result<(), BackendError> {
        if additional > self.max_bytes {
            return Err(BackendError::BudgetExceeded);
        }
        while state.resident_bytes.saturating_add(additional) > self.max_bytes {
            let key = state
                .entries
                .iter()
                .filter(|(_, entry)| !entry.pinned)
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, _)| key.clone())
                .ok_or(BackendError::BudgetExceeded)?;
            if let Some(entry) = state.entries.remove(&key) {
                state.resident_bytes = state
                    .resident_bytes
                    .saturating_sub(entry.record.gpu.byte_size());
                state.evictions += 1;
                entry.record.gpu.release(&self.resources);
            }
        }
        Ok(())
    }
}

fn elapsed_ms(started: std::time::Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1_000.0
}

impl Drop for GpuGeometryResidency {
    fn drop(&mut self) {
        self.clear();
    }
}

fn resident_entry(
    record: &ResidentGeometryRecord,
    mode: GeometryEvaluationMode,
    state: &StoreState,
    submission_count: usize,
    node_id: &NodeId,
) -> GeometryResidentEntry {
    let mut evidence = evidence(
        mode,
        submission_count,
        state.entries.len() + usize::from(submission_count > 0),
        state.resident_bytes,
        state.peak_resident_bytes,
    );
    evidence.topology_hits = state.topology_hits;
    evidence.resident_hits = state.resident_hits;
    evidence.evictions = state.evictions;
    GeometryResidentEntry {
        id: record.id.clone(),
        node_id: node_id.clone(),
        mode,
        dirty: Default::default(),
        bounds: record.bounds,
        vertex_count: record.vertex_count,
        triangle_count: record.triangle_count,
        evidence,
    }
}

fn evidence(
    mode: GeometryEvaluationMode,
    submission_count: usize,
    resident_geometries: usize,
    resident_bytes: u64,
    peak_resident_bytes: u64,
) -> GeometryEvaluationEvidence {
    GeometryEvaluationEvidence {
        mode,
        stats: GeometryExecutionStats::default(),
        submission_count,
        resident_geometries,
        resident_bytes,
        peak_resident_bytes,
        topology_hits: 0,
        resident_hits: 0,
        evictions: 0,
        preview_packet_bytes: 0,
        browser_upload_bytes: 0,
        phases: Default::default(),
    }
}

fn mesh_bounds(mesh: &TriangleMesh) -> Result<Bounds, BackendError> {
    let mut minimum = [f32::INFINITY; 3];
    let mut maximum = [f32::NEG_INFINITY; 3];
    for position in &mesh.positions {
        for axis in 0..3 {
            minimum[axis] = minimum[axis].min(position[axis]);
            maximum[axis] = maximum[axis].max(position[axis]);
        }
    }
    if minimum
        .iter()
        .chain(maximum.iter())
        .any(|value| !value.is_finite())
    {
        return Err(BackendError::Kernel {
            message: "resident mesh bounds are not finite".into(),
        });
    }
    Ok(Bounds {
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
    })
}
