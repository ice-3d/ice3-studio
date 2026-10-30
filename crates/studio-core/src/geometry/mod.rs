mod evaluate;
mod primitives;
mod sweep;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::math::Vec3;

pub use evaluate::evaluate_node_content;

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
}
