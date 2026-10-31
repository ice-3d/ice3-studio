use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use studio_core::document::NodeContent;
use studio_core::geometry::{Bounds, EvaluatedMesh, TriangleMesh};
use studio_core::gpu_contract::{
    BackendError, GeometryEvaluationEvidence, GeometryEvaluationMode, GeometryExecutionStats,
    GeometryResidentEntry, ResidentGeometryId,
};
use studio_core::ids::NodeId;

use crate::kernels::mesh::{
    ResidentMeshGpu, create_resident_custom_mesh, snapshot_resident_custom_mesh,
};
use crate::residency::ResidencyKey;
use crate::resources::DeviceResources;

const RESIDENT_KERNEL_VERSION: &str = "geometry-kernel-v1";

pub struct ResidentGeometryRecord {
    pub id: ResidentGeometryId,
    pub node_id: NodeId,
    pub key: ResidencyKey,
    pub bounds: Bounds,
    pub vertex_count: usize,
    pub triangle_count: usize,
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
    last_evidence: GeometryEvaluationEvidence,
}

pub struct GpuGeometryResidency {
    resources: Arc<DeviceResources>,
    max_bytes: u64,
    state: Mutex<StoreState>,
}

impl GpuGeometryResidency {
    pub(crate) fn new(resources: Arc<DeviceResources>, max_bytes: u64) -> Self {
        Self {
            resources,
            max_bytes,
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
        let key = ResidencyKey::for_content(&content, "", RESIDENT_KERNEL_VERSION)?;
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
            );
            state.last_evidence = resident.evidence.clone();
            return Ok(resident);
        }

        let gpu = create_resident_custom_mesh(device, queue, &self.resources, mesh)?;
        let bytes = gpu.byte_size();
        self.evict_for(&mut state, bytes)?;
        let bounds = mesh_bounds(mesh)?;
        let record = ResidentGeometryRecord {
            id: ResidentGeometryId(key.result.clone()),
            node_id: NodeId::from("resident-mesh"),
            key: key.clone(),
            bounds,
            vertex_count: mesh.positions.len(),
            triangle_count: gpu.triangle_count as usize,
            gpu,
            source: mesh.clone(),
        };
        state.resident_bytes = state.resident_bytes.saturating_add(bytes);
        state.peak_resident_bytes = state.peak_resident_bytes.max(state.resident_bytes);
        state.generation = generation;
        let entry = resident_entry(&record, GeometryEvaluationMode::Cold, &state, 1);
        state.entries.insert(
            key.result,
            StoredEntry {
                record,
                pinned: false,
                last_used: generation,
            },
        );
        state.last_evidence = entry.evidence.clone();
        Ok(entry)
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
        node_id: record.node_id.clone(),
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
