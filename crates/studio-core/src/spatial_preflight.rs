//! Bounding-box preflight for a candidate change set.
//!
//! This answers one question before a human approves a proposal: *where* does
//! this change land, and does it leave the scene? It is deliberately coarse —
//! it ignores rotation and works on per-node AABB, so a rotated cylinder gets a
//! conservative box. Precise interference is `scene.clashes`'s job.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::document::{Document, NodeContent, SceneNode};
use crate::geometry::{Bounds, Primitive};
use crate::ids::NodeId;
use crate::math::Vec3;

/// Spatial summary of a candidate revision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SpatialPreflight {
    /// 改动覆盖的世界包围盒；改动没有可计算几何时为 null。
    pub changed_bounds: Option<Bounds>,
    /// 已提交场景的包围盒，用于对照。
    pub scene_bounds: Option<Bounds>,
    /// 新增节点中完全落在场景包围盒之外的个数。
    pub outside_node_count: usize,
    /// 改动涉及的可见节点数；为 0 说明这次变更不会产生可见几何。
    pub changed_visible_nodes: usize,
}

/// `changed` 是要检查的节点 ID，通常取自 diff 的新增与修改集合。
pub fn preflight(base: &Document, candidate: &Document, changed: &[NodeId]) -> SpatialPreflight {
    let scene_bounds = base
        .scene
        .nodes()
        .iter()
        .filter_map(world_bounds)
        .reduce(merge);

    let bounds_by_id: BTreeMap<&NodeId, Bounds> = candidate
        .scene
        .nodes()
        .iter()
        .filter_map(|node| world_bounds(node).map(|bounds| (&node.id, bounds)))
        .collect();

    let changed_bounds = changed
        .iter()
        .filter_map(|id| bounds_by_id.get(id).copied())
        .reduce(merge);

    let mut outside_node_count = 0;
    let mut changed_visible_nodes = 0;
    for id in changed {
        let Some(node) = candidate.scene.nodes().iter().find(|node| &node.id == id) else {
            continue;
        };
        if !node.visible || !bounds_by_id.contains_key(id) {
            continue;
        }
        changed_visible_nodes += 1;
        let inside = match (bounds_by_id.get(id), &scene_bounds) {
            (Some(node_bounds), Some(scene_bounds)) => intersects(*node_bounds, *scene_bounds),
            _ => true,
        };
        if !inside {
            outside_node_count += 1;
        }
    }

    SpatialPreflight {
        changed_bounds,
        scene_bounds,
        outside_node_count,
        changed_visible_nodes,
    }
}

fn world_bounds(node: &SceneNode) -> Option<Bounds> {
    let half = half_extent(&node.content)?;
    let scale = node.transform.scale;
    let half = Vec3 {
        x: half.x * scale.x.abs(),
        y: half.y * scale.y.abs(),
        z: half.z * scale.z.abs(),
    };
    let center = node.transform.translation;
    Some(Bounds {
        min: Vec3 {
            x: center.x - half.x,
            y: center.y - half.y,
            z: center.z - half.z,
        },
        max: Vec3 {
            x: center.x + half.x,
            y: center.y + half.y,
            z: center.z + half.z,
        },
    })
}

/// Half-extent of a node's own geometry, in local space.
fn half_extent(content: &NodeContent) -> Option<Vec3> {
    match content {
        NodeContent::Primitive(primitive) => Some(match primitive {
            Primitive::Box {
                width,
                height,
                depth,
            } => Vec3 {
                x: width * 0.5,
                y: height * 0.5,
                z: depth * 0.5,
            },
            Primitive::Plane { width, depth } => Vec3 {
                x: width * 0.5,
                y: 0.0,
                z: depth * 0.5,
            },
            Primitive::Cylinder { radius, height, .. } => Vec3 {
                x: *radius,
                y: height * 0.5,
                z: *radius,
            },
            Primitive::Sphere { radius, .. } => Vec3 {
                x: *radius,
                y: *radius,
                z: *radius,
            },
        }),
        NodeContent::Sweep { points, radius, .. } => {
            let mut min = Vec3 {
                x: f32::INFINITY,
                y: f32::INFINITY,
                z: f32::INFINITY,
            };
            let mut max = Vec3 {
                x: f32::NEG_INFINITY,
                y: f32::NEG_INFINITY,
                z: f32::NEG_INFINITY,
            };
            for point in points {
                min.x = min.x.min(point[0]);
                min.y = min.y.min(point[1]);
                min.z = min.z.min(point[2]);
                max.x = max.x.max(point[0]);
                max.y = max.y.max(point[1]);
                max.z = max.z.max(point[2]);
            }
            if !min.x.is_finite() {
                return None;
            }
            Some(Vec3 {
                x: (max.x - min.x) * 0.5 + radius,
                y: (max.y - min.y) * 0.5 + radius,
                z: (max.z - min.z) * 0.5 + radius,
            })
        }
        NodeContent::CustomMesh(mesh) => {
            let mut min = Vec3 {
                x: f32::INFINITY,
                y: f32::INFINITY,
                z: f32::INFINITY,
            };
            let mut max = Vec3 {
                x: f32::NEG_INFINITY,
                y: f32::NEG_INFINITY,
                z: f32::NEG_INFINITY,
            };
            for position in &mesh.positions {
                min.x = min.x.min(position[0]);
                min.y = min.y.min(position[1]);
                min.z = min.z.min(position[2]);
                max.x = max.x.max(position[0]);
                max.y = max.y.max(position[1]);
                max.z = max.z.max(position[2]);
            }
            if !min.x.is_finite() {
                return None;
            }
            Some(Vec3 {
                x: (max.x - min.x) * 0.5,
                y: (max.y - min.y) * 0.5,
                z: (max.z - min.z) * 0.5,
            })
        }
        _ => None,
    }
}

fn merge(left: Bounds, right: Bounds) -> Bounds {
    Bounds {
        min: Vec3 {
            x: left.min.x.min(right.min.x),
            y: left.min.y.min(right.min.y),
            z: left.min.z.min(right.min.z),
        },
        max: Vec3 {
            x: left.max.x.max(right.max.x),
            y: left.max.y.max(right.max.y),
            z: left.max.z.max(right.max.z),
        },
    }
}

fn intersects(left: Bounds, right: Bounds) -> bool {
    left.min.x <= right.max.x
        && left.max.x >= right.min.x
        && left.min.y <= right.max.y
        && left.max.y >= right.min.y
        && left.min.z <= right.max.z
        && left.max.z >= right.min.z
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::SceneNode;
    use crate::math::Transform;

    fn document_with(nodes: &[(Vec3, f32)]) -> Document {
        let mut document = Document::new("test");
        for (index, (translation, size)) in nodes.iter().enumerate() {
            document.scene.nodes_mut().push(SceneNode {
                id: NodeId::new("node", index as u64 + 1).expect("id"),
                name: format!("node {index}"),
                parent_id: None,
                transform: Transform {
                    translation: *translation,
                    ..Transform::IDENTITY
                },
                material_id: None,
                content: NodeContent::Primitive(Primitive::Box {
                    width: *size,
                    height: *size,
                    depth: *size,
                }),
                metadata: BTreeMap::new(),
                visible: true,
            });
        }
        document
    }

    fn origin() -> Vec3 {
        Vec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        }
    }

    #[test]
    fn reports_the_bounds_a_change_lands_in() {
        let base = document_with(&[(origin(), 100.0)]);
        let candidate = document_with(&[
            (origin(), 100.0),
            (
                Vec3 {
                    x: 10.0,
                    y: 0.0,
                    z: 0.0,
                },
                2.0,
            ),
        ]);

        let result = preflight(&base, &candidate, &[NodeId::new("node", 2).expect("id")]);

        assert_eq!(result.changed_visible_nodes, 1);
        assert_eq!(result.outside_node_count, 0);
        let bounds = result.changed_bounds.expect("a box has bounds");
        assert_eq!(bounds.min.x, 9.0);
        assert_eq!(bounds.max.x, 11.0);
    }

    #[test]
    fn counts_changes_that_leave_the_scene() {
        let base = document_with(&[(origin(), 10.0)]);
        let candidate = document_with(&[
            (origin(), 10.0),
            (
                Vec3 {
                    x: 500.0,
                    y: 0.0,
                    z: 0.0,
                },
                2.0,
            ),
        ]);

        let result = preflight(&base, &candidate, &[NodeId::new("node", 2).expect("id")]);

        assert_eq!(result.outside_node_count, 1);
    }

    #[test]
    fn ignores_groups_because_they_have_no_geometry() {
        let base = document_with(&[(origin(), 10.0)]);
        let mut candidate = document_with(&[(origin(), 10.0)]);
        candidate.scene.nodes_mut().push(SceneNode::group(
            NodeId::new("node", 9).expect("id"),
            "assembly",
        ));

        let result = preflight(&base, &candidate, &[NodeId::new("node", 9).expect("id")]);

        assert_eq!(result.changed_visible_nodes, 0);
        assert!(result.changed_bounds.is_none());
    }
}
