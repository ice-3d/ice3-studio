use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::document::NodeContent;
use crate::geometry::{Primitive, TriangleMesh};
use crate::gpu_contract::{
    GeometryDigestSet, GeometryDirtyChannel, GeometryDirtyMask, GeometryPhaseEvidence,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElementRange {
    pub start: u32,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneGeometryChange {
    pub geometry_id: String,
    pub previous_geometry_id: Option<String>,
    pub content: NodeContent,
    pub digests: GeometryDigestSet,
    pub dirty: GeometryDirtyMask,
    pub position_ranges: Vec<ElementRange>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneInstanceChange {
    pub node_id: String,
    pub geometry_id: Option<String>,
    pub matrix: [f32; 16],
    pub material_id: Option<String>,
    pub visible: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScenePatch {
    pub base_revision: u64,
    pub target_revision: u64,
    pub full_rebuild: bool,
    pub geometry: Vec<SceneGeometryChange>,
    pub instances: Vec<SceneInstanceChange>,
    pub removed_geometry_ids: Vec<String>,
    pub removed_node_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SceneSessionId(pub String);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SceneSessionState {
    #[default]
    Idle,
    Pending,
    Failed,
    Lost,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SceneSessionStatus {
    pub enabled: bool,
    pub session_id: Option<String>,
    pub state: SceneSessionState,
    pub published_revision: u64,
    pub target_revision: u64,
    pub pending_patches: usize,
    pub pending_submissions: usize,
    pub geometry_count: usize,
    pub topology_count: usize,
    pub attribute_count: usize,
    pub instance_count: usize,
    pub resident_bytes: u64,
    pub peak_resident_bytes: u64,
    pub dirty_range_bytes: u64,
    pub readback_bytes: u64,
    pub submission_count: usize,
    pub geometry_dispatch_count: usize,
    pub fallback_reason: Option<String>,
    pub phases: GeometryPhaseEvidence,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneSubmission {
    pub session_id: SceneSessionId,
    pub target_revision: u64,
    pub submission_count: usize,
    pub geometry_dispatch_count: usize,
    pub instance_range_count: usize,
    pub upload_bytes: u64,
    pub readback_bytes: u64,
    pub phases: GeometryPhaseEvidence,
}

pub fn changed_position_ranges(
    previous: &TriangleMesh,
    next: &TriangleMesh,
) -> Option<Vec<ElementRange>> {
    if previous.positions.len() != next.positions.len() || previous.indices != next.indices {
        return None;
    }

    let mut ranges = Vec::new();
    let mut active: Option<ElementRange> = None;
    for (index, (before, after)) in previous.positions.iter().zip(&next.positions).enumerate() {
        if before == after {
            if let Some(range) = active.take() {
                ranges.push(range);
            }
            continue;
        }
        let index = u32::try_from(index).ok()?;
        match &mut active {
            Some(range) if range.start.saturating_add(range.count) == index => range.count += 1,
            Some(_) => {
                ranges.push(active.take()?);
                active = Some(ElementRange {
                    start: index,
                    count: 1,
                });
            }
            None => {
                active = Some(ElementRange {
                    start: index,
                    count: 1,
                })
            }
        }
    }
    if let Some(range) = active {
        ranges.push(range);
    }
    Some(ranges)
}

pub fn scene_content_dirty(previous: &NodeContent, next: &NodeContent) -> GeometryDirtyMask {
    if previous == next {
        return GeometryDirtyMask::empty();
    }
    match (previous, next) {
        (NodeContent::CustomMesh(before), NodeContent::CustomMesh(after)) => {
            custom_mesh_dirty(before, after)
        }
        (NodeContent::Primitive(before), NodeContent::Primitive(after)) => {
            primitive_dirty(before, after)
        }
        (
            NodeContent::Sweep {
                points: before_points,
                radius: before_radius,
                radial_segments: before_segments,
            },
            NodeContent::Sweep {
                points: after_points,
                radius: after_radius,
                radial_segments: after_segments,
            },
        ) => {
            if before_segments != after_segments || before_points.len() != after_points.len() {
                all_geometry()
            } else if before_points != after_points || before_radius != after_radius {
                channels(&[
                    GeometryDirtyChannel::Positions,
                    GeometryDirtyChannel::Normals,
                    GeometryDirtyChannel::Bounds,
                ])
            } else {
                GeometryDirtyMask::empty()
            }
        }
        _ => all_geometry(),
    }
}

fn custom_mesh_dirty(before: &TriangleMesh, after: &TriangleMesh) -> GeometryDirtyMask {
    let mut dirty = GeometryDirtyMask::empty();
    if before.positions.len() != after.positions.len() || before.indices != after.indices {
        dirty.insert(GeometryDirtyChannel::Topology);
        dirty.insert(GeometryDirtyChannel::Indices);
    }
    if before.positions != after.positions {
        dirty.insert(GeometryDirtyChannel::Positions);
        dirty.insert(GeometryDirtyChannel::Normals);
        dirty.insert(GeometryDirtyChannel::Bounds);
    }
    if before.indices != after.indices {
        dirty.insert(GeometryDirtyChannel::Normals);
    }
    dirty
}

fn primitive_dirty(before: &Primitive, after: &Primitive) -> GeometryDirtyMask {
    match (before, after) {
        (Primitive::Box { .. }, Primitive::Box { .. })
        | (Primitive::Plane { .. }, Primitive::Plane { .. }) => channels(&[
            GeometryDirtyChannel::Positions,
            GeometryDirtyChannel::Bounds,
        ]),
        (
            Primitive::Cylinder {
                segments: before_segments,
                ..
            },
            Primitive::Cylinder {
                segments: after_segments,
                ..
            },
        ) if before_segments != after_segments => all_geometry(),
        (
            Primitive::Sphere {
                segments: before_segments,
                rings: before_rings,
                ..
            },
            Primitive::Sphere {
                segments: after_segments,
                rings: after_rings,
                ..
            },
        ) if before_segments != after_segments || before_rings != after_rings => all_geometry(),
        (Primitive::Cylinder { .. }, Primitive::Cylinder { .. })
        | (Primitive::Sphere { .. }, Primitive::Sphere { .. }) => channels(&[
            GeometryDirtyChannel::Positions,
            GeometryDirtyChannel::Normals,
            GeometryDirtyChannel::Bounds,
        ]),
        _ => all_geometry(),
    }
}

fn all_geometry() -> GeometryDirtyMask {
    channels(&[
        GeometryDirtyChannel::Topology,
        GeometryDirtyChannel::Positions,
        GeometryDirtyChannel::Indices,
        GeometryDirtyChannel::Normals,
        GeometryDirtyChannel::Bounds,
    ])
}

fn channels(values: &[GeometryDirtyChannel]) -> GeometryDirtyMask {
    let mut dirty = GeometryDirtyMask::empty();
    for channel in values {
        dirty.insert(*channel);
    }
    dirty
}
