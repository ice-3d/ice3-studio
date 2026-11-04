mod clash;
mod csg;
mod evaluate;
mod extrude;
mod primitives;
mod rounded;
mod sweep;

use std::collections::HashSet;

use glam::{Mat4, Quat, Vec3 as GlamVec3};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::document::{Document, SceneNode};
use crate::math::Vec3;

pub use clash::{CLASH_EPSILON, meshes_intersect, signed_gap, sweep_pairs, within_clearance};
pub use csg::csg_subtract;
pub use evaluate::evaluate_node_content;
pub use extrude::extrude_profile;
pub use rounded::{MAX_ROUNDED_SEGMENTS, rounded_box};

pub fn world_bounds(
    document: &Document,
    node: &SceneNode,
) -> Result<Option<Bounds>, GeometryError> {
    let mesh = evaluate_node_content(&node.content)?;
    if mesh.vertices.is_empty() {
        return Ok(None);
    }
    let mut chain = Vec::new();
    let mut current = node;
    let mut seen = HashSet::new();
    loop {
        if !seen.insert(current.id.clone()) {
            return Err(GeometryError::InvalidTransform);
        }
        chain.push(node_matrix(current));
        let Some(parent_id) = &current.parent_id else {
            break;
        };
        let Some(parent) = document
            .scene
            .nodes()
            .iter()
            .find(|candidate| candidate.id == *parent_id)
        else {
            return Err(GeometryError::InvalidTransform);
        };
        current = parent;
    }
    let matrix = chain
        .iter()
        .rev()
        .fold(Mat4::IDENTITY, |world, local| world * *local);
    let mut minimum = [f32::INFINITY; 3];
    let mut maximum = [f32::NEG_INFINITY; 3];
    for x in [mesh.bounds.min.x, mesh.bounds.max.x] {
        for y in [mesh.bounds.min.y, mesh.bounds.max.y] {
            for z in [mesh.bounds.min.z, mesh.bounds.max.z] {
                let point = matrix.transform_point3(GlamVec3::new(x, y, z));
                minimum = [
                    minimum[0].min(point.x),
                    minimum[1].min(point.y),
                    minimum[2].min(point.z),
                ];
                maximum = [
                    maximum[0].max(point.x),
                    maximum[1].max(point.y),
                    maximum[2].max(point.z),
                ];
            }
        }
    }
    Ok(Some(Bounds {
        min: Vec3 {
            x: minimum[0],
            y: minimum[1],
            z: minimum[2],
        },
        max: Vec3 {
            x: maximum[0],
            y: maximum[1],
            z: maximum[2],
        },
    }))
}

fn node_matrix(node: &SceneNode) -> Mat4 {
    Mat4::from_scale_rotation_translation(
        GlamVec3::new(
            node.transform.scale.x,
            node.transform.scale.y,
            node.transform.scale.z,
        ),
        Quat::from_array(node.transform.rotation),
        GlamVec3::new(
            node.transform.translation.x,
            node.transform.translation.y,
            node.transform.translation.z,
        ),
    )
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum Primitive {
    Box {
        width: f32,
        height: f32,
        depth: f32,
    },
    Plane {
        width: f32,
        depth: f32,
    },
    Cylinder {
        radius: f32,
        height: f32,
        segments: u32,
    },
    Sphere {
        radius: f32,
        segments: u32,
        rings: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TriangleMesh {
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[ts(export)]
pub struct Bounds {
    pub min: Vec3,
    pub max: Vec3,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EvaluatedMesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub bounds: Bounds,
}

impl EvaluatedMesh {
    pub fn empty() -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
            bounds: Bounds {
                min: Vec3::default(),
                max: Vec3::default(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum GeometryError {
    #[error("transform or hierarchy is invalid")]
    InvalidTransform,
    #[error("geometry dimensions must be finite and positive")]
    InvalidDimensions,
    #[error("geometry contains a non-finite position")]
    NonFinitePosition,
    #[error("mesh indices are empty, invalid, or out of range")]
    InvalidIndices,
    #[error("sweep requires at least two distinct points")]
    InvalidSweep,
    #[error("primitive segment counts are outside the supported range")]
    InvalidSegments,
    #[error("extrusion profile is invalid")]
    InvalidProfile,
    #[error("CSG operation failed")]
    CsgFailed,
    #[error("CSG input exceeds the triangle budget")]
    CsgTooLarge,
}
