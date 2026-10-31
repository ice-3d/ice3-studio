use std::collections::BTreeMap;

use studio_core::gpu_contract::{GeometryPhaseEvidence, ResidentGeometryId};
use studio_core::scene_contract::{SceneSessionId, SceneSessionState, SceneSessionStatus};

use crate::scene::instances::InstanceTable;

pub(crate) struct GpuSceneSession {
    pub device_generation: u64,
    pub published_revision: u64,
    pub target_revision: u64,
    pub state: SceneSessionState,
    pub instances: InstanceTable,
    pub geometries: BTreeMap<String, ResidentGeometryId>,
    pub pending_geometries: Vec<ResidentGeometryId>,
    pub submission_count: usize,
    pub geometry_dispatch_count: usize,
    pub dirty_range_bytes: u64,
    pub phases: GeometryPhaseEvidence,
    pub fallback_reason: Option<String>,
}

impl GpuSceneSession {
    pub(crate) fn new(device_generation: u64, published_revision: u64) -> Self {
        Self {
            device_generation,
            published_revision,
            target_revision: published_revision,
            state: SceneSessionState::Idle,
            instances: InstanceTable::default(),
            geometries: BTreeMap::new(),
            pending_geometries: Vec::new(),
            submission_count: 0,
            geometry_dispatch_count: 0,
            dirty_range_bytes: 0,
            phases: GeometryPhaseEvidence::default(),
            fallback_reason: None,
        }
    }

    pub(crate) fn status(
        &self,
        session_id: &SceneSessionId,
        resident_bytes: u64,
        peak_resident_bytes: u64,
    ) -> SceneSessionStatus {
        SceneSessionStatus {
            enabled: true,
            session_id: Some(session_id.0.clone()),
            state: self.state,
            published_revision: self.published_revision,
            target_revision: self.target_revision,
            pending_patches: usize::from(!self.pending_geometries.is_empty()),
            pending_submissions: self.pending_geometries.len(),
            geometry_count: self.geometries.len(),
            topology_count: self
                .geometries
                .values()
                .map(|id| id.0.as_str())
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            attribute_count: self.geometries.len(),
            instance_count: self.instances.len(),
            resident_bytes,
            peak_resident_bytes,
            dirty_range_bytes: self.dirty_range_bytes,
            readback_bytes: 0,
            submission_count: self.submission_count,
            geometry_dispatch_count: self.geometry_dispatch_count,
            fallback_reason: self.fallback_reason.clone(),
            phases: self.phases.clone(),
        }
    }
}
