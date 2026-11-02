use std::mem::size_of;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{ImportError, SourceScene};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeImportLimits {
    pub max_nodes: usize,
    pub max_triangles: u64,
    pub max_memory_bytes: usize,
    pub timeout: std::time::Duration,
}

impl Default for NativeImportLimits {
    fn default() -> Self {
        Self {
            max_nodes: 1_000_000,
            max_triangles: 20_000_000,
            max_memory_bytes: 536_870_912,
            timeout: std::time::Duration::from_secs(120),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerLimits {
    pub max_nodes: u64,
    pub max_triangles: u64,
    pub max_memory_bytes: u64,
    pub timeout_millis: u64,
}

impl WorkerLimits {
    pub(crate) fn from_limits(limits: NativeImportLimits) -> Result<Self, ImportError> {
        Ok(Self {
            max_nodes: u64::try_from(limits.max_nodes)
                .map_err(|_| invalid_limit("node budget does not fit the worker protocol"))?,
            max_triangles: limits.max_triangles,
            max_memory_bytes: u64::try_from(limits.max_memory_bytes)
                .map_err(|_| invalid_limit("memory budget does not fit the worker protocol"))?,
            timeout_millis: u64::try_from(limits.timeout.as_millis())
                .map_err(|_| invalid_limit("timeout does not fit the worker protocol"))?,
        })
    }

    pub(crate) fn to_limits(self) -> Result<NativeImportLimits, ImportError> {
        Ok(NativeImportLimits {
            max_nodes: usize::try_from(self.max_nodes)
                .map_err(|_| invalid_limit("worker node budget is not representable"))?,
            max_triangles: self.max_triangles,
            max_memory_bytes: usize::try_from(self.max_memory_bytes)
                .map_err(|_| invalid_limit("worker memory budget is not representable"))?,
            timeout: std::time::Duration::from_millis(self.timeout_millis),
        })
    }
}

pub fn validate_scene_limits(
    scene: &SourceScene,
    limits: &NativeImportLimits,
) -> Result<(), ImportError> {
    if scene.nodes.len() > limits.max_nodes {
        return Err(limit_exceeded(format!(
            "scene has {} nodes; limit is {}",
            scene.nodes.len(),
            limits.max_nodes
        )));
    }

    let mut triangles = 0_u64;
    for mesh in &scene.meshes {
        for primitive in &mesh.primitives {
            let primitive_triangles = u64::try_from(primitive.indices.len() / 3)
                .map_err(|_| limit_exceeded("triangle count is not representable"))?;
            triangles = triangles
                .checked_add(primitive_triangles)
                .ok_or_else(|| limit_exceeded("triangle count overflowed"))?;
        }
    }
    if triangles > limits.max_triangles {
        return Err(limit_exceeded(format!(
            "scene has {triangles} triangles; limit is {}",
            limits.max_triangles
        )));
    }

    let memory = estimated_scene_bytes(scene)
        .ok_or_else(|| limit_exceeded("estimated scene memory overflowed"))?;
    if memory > limits.max_memory_bytes {
        return Err(limit_exceeded(format!(
            "scene uses approximately {memory} bytes; limit is {}",
            limits.max_memory_bytes
        )));
    }

    Ok(())
}

fn estimated_scene_bytes(scene: &SourceScene) -> Option<usize> {
    let mut total = size_of::<SourceScene>();
    total = checked_add(total, scene.asset_id.capacity())?;
    total = checked_add(
        total,
        scene
            .nodes
            .capacity()
            .checked_mul(size_of::<crate::SourceNode>())?,
    )?;
    total = checked_add(
        total,
        scene
            .meshes
            .capacity()
            .checked_mul(size_of::<crate::SourceMesh>())?,
    )?;
    total = checked_add(
        total,
        scene
            .materials
            .capacity()
            .checked_mul(size_of::<crate::SourceMaterial>())?,
    )?;
    total = checked_add(
        total,
        scene
            .warnings
            .capacity()
            .checked_mul(size_of::<crate::ImportWarning>())?,
    )?;
    total = checked_add(total, json_bytes(&scene.asset_extras)?)?;

    for node in &scene.nodes {
        total = checked_add(total, node.id.capacity())?;
        total = checked_add(total, node.name.capacity())?;
        if let Some(parent) = &node.parent {
            total = checked_add(total, parent.capacity())?;
        }
        total = checked_add(total, json_bytes(&node.extras)?)?;
    }
    for mesh in &scene.meshes {
        total = checked_add(total, mesh.id.capacity())?;
        total = checked_add(
            total,
            mesh.primitives
                .capacity()
                .checked_mul(size_of::<crate::SourcePrimitive>())?,
        )?;
        total = checked_add(total, json_bytes(&mesh.extras)?)?;
        for primitive in &mesh.primitives {
            total = checked_add(
                total,
                primitive
                    .positions
                    .capacity()
                    .checked_mul(size_of::<[f32; 3]>())?,
            )?;
            total = checked_add(
                total,
                primitive
                    .normals
                    .as_ref()
                    .map_or(0, Vec::capacity)
                    .checked_mul(size_of::<[f32; 3]>())?,
            )?;
            total = checked_add(
                total,
                primitive
                    .colors
                    .as_ref()
                    .map_or(0, Vec::capacity)
                    .checked_mul(size_of::<[f32; 3]>())?,
            )?;
            total = checked_add(
                total,
                primitive.indices.capacity().checked_mul(size_of::<u32>())?,
            )?;
        }
    }
    for material in &scene.materials {
        total = checked_add(total, material.id.capacity())?;
        total = checked_add(total, material.name.capacity())?;
        total = checked_add(total, json_bytes(&material.extras)?)?;
    }
    for warning in &scene.warnings {
        total = checked_add(total, warning.code.capacity())?;
        total = checked_add(total, warning.message.capacity())?;
        if let Some(path) = &warning.path {
            total = checked_add(total, path.as_os_str().to_string_lossy().len())?;
        }
    }
    Some(total)
}

fn json_bytes(value: &Value) -> Option<usize> {
    let mut total = size_of::<Value>();
    total = checked_add(
        total,
        match value {
            Value::String(value) => value.capacity(),
            Value::Array(values) => {
                let mut array = values.capacity().checked_mul(size_of::<Value>())?;
                for value in values {
                    array = checked_add(array, json_bytes(value)?)?;
                }
                array
            }
            Value::Object(values) => {
                let mut object = values.len().checked_mul(size_of::<(String, Value)>())?;
                for (key, value) in values {
                    object = checked_add(object, key.capacity())?;
                    object = checked_add(object, json_bytes(value)?)?;
                }
                object
            }
            Value::Null | Value::Bool(_) | Value::Number(_) => 0,
        },
    )?;
    Some(total)
}

fn checked_add(left: usize, right: usize) -> Option<usize> {
    left.checked_add(right)
}

fn limit_exceeded(message: impl Into<String>) -> ImportError {
    ImportError::LimitExceeded(message.into())
}

fn invalid_limit(message: &str) -> ImportError {
    ImportError::WorkerProtocolInvalid(message.into())
}
