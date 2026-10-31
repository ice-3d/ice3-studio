use studio_core::document::NodeContent;
use studio_core::geometry::{Primitive, TriangleMesh};
use studio_core::gpu_contract::{GeometryDirtyChannel, GeometryDirtyMask};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirtyPlan {
    pub dirty: GeometryDirtyMask,
}

impl DirtyPlan {
    pub fn for_transition(previous: Option<&NodeContent>, next: &NodeContent) -> Self {
        let Some(previous) = previous else {
            return Self {
                dirty: all_geometry(),
            };
        };
        if previous == next {
            return Self {
                dirty: GeometryDirtyMask::empty(),
            };
        }
        Self {
            dirty: match (previous, next) {
                (NodeContent::Primitive(before), NodeContent::Primitive(after)) => {
                    primitive_dirty(before, after)
                }
                (NodeContent::CustomMesh(before), NodeContent::CustomMesh(after)) => {
                    custom_mesh_dirty(before, after)
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
                ) => sweep_dirty(
                    before_points,
                    *before_radius,
                    *before_segments,
                    after_points,
                    *after_radius,
                    *after_segments,
                ),
                _ => all_geometry(),
            },
        }
    }

    pub fn for_transform_change() -> Self {
        let mut dirty = GeometryDirtyMask::empty();
        dirty.insert(GeometryDirtyChannel::Transform);
        dirty.insert(GeometryDirtyChannel::Bounds);
        Self { dirty }
    }

    pub fn for_material_change() -> Self {
        let mut dirty = GeometryDirtyMask::empty();
        dirty.insert(GeometryDirtyChannel::Material);
        Self { dirty }
    }

    pub fn requires_topology_rebuild(self) -> bool {
        self.dirty.contains(GeometryDirtyChannel::Topology)
    }
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
                radius: old_radius,
                height: old_height,
                segments: old_segments,
            },
            Primitive::Cylinder {
                radius: new_radius,
                height: new_height,
                segments: new_segments,
            },
        ) => {
            if old_segments != new_segments {
                rebuild_geometry()
            } else {
                let mut dirty = GeometryDirtyMask::empty();
                if old_radius != new_radius || old_height != new_height {
                    dirty.insert(GeometryDirtyChannel::Positions);
                    dirty.insert(GeometryDirtyChannel::Bounds);
                }
                dirty
            }
        }
        (
            Primitive::Sphere {
                radius: old_radius,
                segments: old_segments,
                rings: old_rings,
            },
            Primitive::Sphere {
                radius: new_radius,
                segments: new_segments,
                rings: new_rings,
            },
        ) => {
            if old_segments != new_segments || old_rings != new_rings {
                rebuild_geometry()
            } else {
                let mut dirty = GeometryDirtyMask::empty();
                if old_radius != new_radius {
                    dirty.insert(GeometryDirtyChannel::Positions);
                    dirty.insert(GeometryDirtyChannel::Bounds);
                }
                dirty
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

fn sweep_dirty(
    before_points: &[[f32; 3]],
    before_radius: f32,
    before_segments: u32,
    after_points: &[[f32; 3]],
    after_radius: f32,
    after_segments: u32,
) -> GeometryDirtyMask {
    if before_segments != after_segments || before_points.len() != after_points.len() {
        return rebuild_geometry();
    }
    let mut dirty = GeometryDirtyMask::empty();
    if before_points != after_points || before_radius != after_radius {
        dirty.insert(GeometryDirtyChannel::Positions);
        dirty.insert(GeometryDirtyChannel::Normals);
        dirty.insert(GeometryDirtyChannel::Bounds);
    }
    dirty
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

fn rebuild_geometry() -> GeometryDirtyMask {
    all_geometry()
}

fn channels(channels: &[GeometryDirtyChannel]) -> GeometryDirtyMask {
    let mut dirty = GeometryDirtyMask::empty();
    for channel in channels {
        dirty.insert(*channel);
    }
    dirty
}
