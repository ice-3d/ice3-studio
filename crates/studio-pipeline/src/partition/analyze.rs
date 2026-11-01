use std::collections::{BTreeMap, BTreeSet};

use glam::{Mat4, Quat, Vec3};
use serde_json::Value;
use studio_assets::{AccessorData, SourceGlb, read_accessor};
use studio_core::geometry::Bounds;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PartitionError {
    #[error("partition source is empty: {0}")]
    Empty(String),
    #[error("partition source is invalid: {0}")]
    Invalid(String),
    #[error("partition options are invalid: {0}")]
    Options(String),
    #[error(transparent)]
    Asset(#[from] studio_assets::AssetError),
}

impl PartitionError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Empty(_) => "ICE3_STUDIO_PARTITION_EMPTY",
            Self::Invalid(_) | Self::Options(_) => "ICE3_STUDIO_PARTITION_INVALID",
            Self::Asset(error) => error.code(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PartitionUnit {
    pub id: String,
    pub mesh_id: String,
    pub mesh_index: usize,
    pub mesh_keys: Vec<String>,
    pub node_ids: Vec<String>,
    pub triangle_count: u64,
    pub bounds: Bounds,
    pub world_matrices: Vec<Mat4>,
}

struct SceneNode<'a> {
    id: String,
    value: &'a serde_json::Map<String, Value>,
    world: Mat4,
}

pub fn analyze_units(source: &SourceGlb) -> Result<Vec<PartitionUnit>, PartitionError> {
    let nodes = array(source.json.get("nodes"), "nodes")?;
    let meshes = array(source.json.get("meshes"), "meshes")?;
    let scene_index = source
        .json
        .get("scene")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let scene = array(source.json.get("scenes"), "scenes")?
        .get(scene_index)
        .and_then(Value::as_object)
        .ok_or_else(|| PartitionError::Empty("active scene is missing".into()))?;
    let roots = integer_array(scene.get("nodes"), "scene.nodes")?;
    let mut scene_nodes = Vec::new();
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    for root in roots {
        visit_node(
            source,
            nodes,
            root,
            Mat4::IDENTITY,
            &mut visiting,
            &mut visited,
            &mut scene_nodes,
        )?;
    }

    let mut ids = BTreeSet::new();
    for node in &scene_nodes {
        if !ids.insert(node.id.clone()) {
            return Err(PartitionError::Invalid(format!(
                "duplicate node id {}",
                node.id
            )));
        }
    }
    let mesh_ids = mesh_ids(meshes)?;
    let mut by_mesh = BTreeMap::<usize, Vec<&SceneNode<'_>>>::new();
    for node in &scene_nodes {
        let Some(mesh_index) = node.value.get("mesh").and_then(Value::as_u64) else {
            continue;
        };
        let mesh_index = usize::try_from(mesh_index)
            .map_err(|_| PartitionError::Invalid("mesh index is too large".into()))?;
        if mesh_index >= meshes.len() {
            return Err(PartitionError::Empty(format!(
                "mesh {mesh_index} is missing"
            )));
        }
        by_mesh.entry(mesh_index).or_default().push(node);
    }
    if by_mesh.is_empty() {
        return Err(PartitionError::Empty(
            "active scene has no mesh nodes".into(),
        ));
    }

    let mut units = Vec::new();
    for (mesh_index, instances) in by_mesh {
        let analysis = analyze_mesh(source, &meshes[mesh_index], mesh_index)?;
        if analysis.triangle_count == 0 {
            continue;
        }
        let mut instances = instances;
        instances.sort_by(|left, right| left.id.cmp(&right.id));
        let mut bounds: Option<Bounds> = None;
        let mut world_matrices = Vec::with_capacity(instances.len());
        let mut node_ids = Vec::with_capacity(instances.len());
        let mut mesh_keys = Vec::with_capacity(instances.len());
        let mesh_id = &mesh_ids[mesh_index];
        for node in instances {
            let transformed = transform_bounds(analysis.bounds, node.world);
            bounds = Some(match bounds {
                Some(current) => union_bounds(current, transformed),
                None => transformed,
            });
            world_matrices.push(node.world);
            node_ids.push(node.id.clone());
            mesh_keys.push(format!("{mesh_id}#{}", node.id));
        }
        units.push(PartitionUnit {
            id: mesh_id.clone(),
            mesh_id: mesh_id.clone(),
            mesh_index,
            mesh_keys,
            node_ids,
            triangle_count: analysis.triangle_count,
            bounds: bounds.expect("mesh instances are non-empty"),
            world_matrices,
        });
    }
    if units.is_empty() {
        return Err(PartitionError::Empty(
            "active scene has no triangles".into(),
        ));
    }
    units.sort_by(|left, right| {
        left.id
            .cmp(&right.id)
            .then_with(|| left.mesh_index.cmp(&right.mesh_index))
    });
    Ok(units)
}

struct MeshAnalysis {
    triangle_count: u64,
    bounds: Bounds,
}

fn analyze_mesh(
    source: &SourceGlb,
    mesh: &Value,
    mesh_index: usize,
) -> Result<MeshAnalysis, PartitionError> {
    let mesh = mesh
        .as_object()
        .ok_or_else(|| PartitionError::Invalid(format!("mesh {mesh_index} is invalid")))?;
    let primitives = array(mesh.get("primitives"), "mesh.primitives")?;
    let mut triangle_count = 0_u64;
    let mut bounds = None;
    for (primitive_index, primitive) in primitives.iter().enumerate() {
        let primitive = primitive.as_object().ok_or_else(|| {
            PartitionError::Invalid(format!(
                "mesh {mesh_index} primitive {primitive_index} is invalid"
            ))
        })?;
        let mode = primitive.get("mode").and_then(Value::as_u64).unwrap_or(4);
        if !matches!(mode, 4..=6) {
            continue;
        }
        let attributes = primitive
            .get("attributes")
            .and_then(Value::as_object)
            .ok_or_else(|| PartitionError::Invalid("primitive attributes are invalid".into()))?;
        let position_index = attributes
            .get("POSITION")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| PartitionError::Empty("POSITION accessor is missing".into()))?;
        let position = accessor(source, position_index)?;
        let element_count = match primitive.get("indices") {
            Some(index) => {
                let index = index
                    .as_u64()
                    .and_then(|value| usize::try_from(value).ok())
                    .ok_or_else(|| PartitionError::Invalid("indices is invalid".into()))?;
                accessor(source, index)?
                    .get("count")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| PartitionError::Invalid("index count is missing".into()))?
            }
            None => position
                .get("count")
                .and_then(Value::as_u64)
                .ok_or_else(|| PartitionError::Invalid("position count is missing".into()))?,
        };
        let triangles = match mode {
            4 if element_count % 3 == 0 => element_count / 3,
            4 => {
                return Err(PartitionError::Invalid(
                    "triangle index count is not divisible by three".into(),
                ));
            }
            5 => element_count.saturating_sub(2),
            6 if element_count >= 3 => element_count - 2,
            6 => 0,
            _ => 0,
        };
        triangle_count = triangle_count
            .checked_add(triangles)
            .ok_or_else(|| PartitionError::Invalid("triangle count overflows".into()))?;
        let primitive_bounds = accessor_bounds(source, position_index)?;
        bounds = Some(match bounds {
            Some(current) => union_bounds(current, primitive_bounds),
            None => primitive_bounds,
        });
    }
    let bounds =
        bounds.ok_or_else(|| PartitionError::Empty("mesh has no supported geometry".into()))?;
    Ok(MeshAnalysis {
        triangle_count,
        bounds,
    })
}

fn visit_node<'a>(
    source: &SourceGlb,
    nodes: &'a [Value],
    index: usize,
    parent: Mat4,
    visiting: &mut BTreeSet<usize>,
    visited: &mut BTreeSet<usize>,
    output: &mut Vec<SceneNode<'a>>,
) -> Result<(), PartitionError> {
    let node = nodes
        .get(index)
        .and_then(Value::as_object)
        .ok_or_else(|| PartitionError::Empty(format!("node {index} is missing")))?;
    if !visiting.insert(index) {
        return Err(PartitionError::Invalid(format!(
            "node graph contains a cycle at node {index}"
        )));
    }
    if visited.contains(&index) {
        return Err(PartitionError::Invalid(format!(
            "node {index} has multiple parents"
        )));
    }
    let local = local_matrix(node)?;
    let world = parent * local;
    if !world.is_finite() {
        return Err(PartitionError::Invalid(format!(
            "node {index} transform is not finite"
        )));
    }
    output.push(SceneNode {
        id: stable_id(source, node, index, "node")?,
        value: node,
        world,
    });
    let children = optional_integer_array(node.get("children"), "node.children")?;
    for child in children {
        if child >= nodes.len() {
            return Err(PartitionError::Empty(format!("node {child} is missing")));
        }
        visit_node(source, nodes, child, world, visiting, visited, output)?;
    }
    visiting.remove(&index);
    visited.insert(index);
    Ok(())
}

fn local_matrix(node: &serde_json::Map<String, Value>) -> Result<Mat4, PartitionError> {
    if let Some(matrix) = node.get("matrix") {
        let values = finite_array(Some(matrix), 16, "matrix")?;
        let values: [f32; 16] = values
            .try_into()
            .map_err(|_| PartitionError::Invalid("matrix must contain 16 values".into()))?;
        return Ok(Mat4::from_cols_array(&values));
    }
    let translation = vector3(node.get("translation"), [0.0, 0.0, 0.0], "translation")?;
    let rotation = vector4(node.get("rotation"), [0.0, 0.0, 0.0, 1.0], "rotation")?;
    let scale = vector3(node.get("scale"), [1.0, 1.0, 1.0], "scale")?;
    let quaternion = Quat::from_xyzw(rotation[0], rotation[1], rotation[2], rotation[3]);
    if quaternion.length_squared() <= f32::EPSILON {
        return Err(PartitionError::Invalid(
            "rotation quaternion is zero".into(),
        ));
    }
    Ok(Mat4::from_scale_rotation_translation(
        Vec3::from_array(scale),
        quaternion.normalize(),
        Vec3::from_array(translation),
    ))
}

fn mesh_ids(meshes: &[Value]) -> Result<Vec<String>, PartitionError> {
    let mut ids = BTreeSet::new();
    meshes
        .iter()
        .enumerate()
        .map(|(index, mesh)| {
            let mesh = mesh
                .as_object()
                .ok_or_else(|| PartitionError::Invalid(format!("mesh {index} is invalid")))?;
            let id = profile_id(mesh).unwrap_or_else(|| format!("mesh-{index}"));
            if !ids.insert(id.clone()) {
                return Err(PartitionError::Invalid(format!("duplicate mesh id {id}")));
            }
            Ok(id)
        })
        .collect()
}

fn stable_id(
    _source: &SourceGlb,
    node: &serde_json::Map<String, Value>,
    index: usize,
    kind: &str,
) -> Result<String, PartitionError> {
    Ok(profile_id(node)
        .or_else(|| node.get("name").and_then(Value::as_str).map(str::to_owned))
        .unwrap_or_else(|| format!("{kind}-{index}")))
}

fn profile_id(value: &serde_json::Map<String, Value>) -> Option<String> {
    value
        .get("extras")
        .and_then(Value::as_object)
        .and_then(|extras| extras.get("ice3"))
        .and_then(Value::as_object)
        .and_then(|profile| profile.get("id"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn accessor(
    source: &SourceGlb,
    index: usize,
) -> Result<&serde_json::Map<String, Value>, PartitionError> {
    array(source.json.get("accessors"), "accessors")?
        .get(index)
        .and_then(Value::as_object)
        .ok_or_else(|| PartitionError::Empty(format!("accessor {index} is missing")))
}

fn accessor_bounds(source: &SourceGlb, index: usize) -> Result<Bounds, PartitionError> {
    let accessor = accessor(source, index)?;
    if let (Some(min), Some(max)) = (accessor.get("min"), accessor.get("max")) {
        let min = finite_array(Some(min), 3, "accessor.min")?;
        let max = finite_array(Some(max), 3, "accessor.max")?;
        return Ok(Bounds {
            min: studio_core::math::Vec3 {
                x: min[0],
                y: min[1],
                z: min[2],
            },
            max: studio_core::math::Vec3 {
                x: max[0],
                y: max[1],
                z: max[2],
            },
        });
    }
    let data = read_accessor(source, index)?;
    let AccessorData::F32 { values, components } = data else {
        return Err(PartitionError::Empty(
            "POSITION must be a float accessor".into(),
        ));
    };
    if components != 3 || values.is_empty() {
        return Err(PartitionError::Empty(
            "POSITION must be a non-empty VEC3 accessor".into(),
        ));
    }
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for point in values.chunks_exact(3) {
        min = min.min(Vec3::new(point[0], point[1], point[2]));
        max = max.max(Vec3::new(point[0], point[1], point[2]));
    }
    Ok(glam_bounds(min, max))
}

fn transform_bounds(bounds: Bounds, matrix: Mat4) -> Bounds {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for x in [bounds.min.x, bounds.max.x] {
        for y in [bounds.min.y, bounds.max.y] {
            for z in [bounds.min.z, bounds.max.z] {
                let point = matrix.transform_point3(Vec3::new(x, y, z));
                min = min.min(point);
                max = max.max(point);
            }
        }
    }
    glam_bounds(min, max)
}

fn union_bounds(left: Bounds, right: Bounds) -> Bounds {
    Bounds {
        min: studio_core::math::Vec3 {
            x: left.min.x.min(right.min.x),
            y: left.min.y.min(right.min.y),
            z: left.min.z.min(right.min.z),
        },
        max: studio_core::math::Vec3 {
            x: left.max.x.max(right.max.x),
            y: left.max.y.max(right.max.y),
            z: left.max.z.max(right.max.z),
        },
    }
}

fn array<'a>(value: Option<&'a Value>, field: &str) -> Result<&'a [Value], PartitionError> {
    value
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| PartitionError::Invalid(format!("{field} must be an array")))
}

fn integer_array(value: Option<&Value>, field: &str) -> Result<Vec<usize>, PartitionError> {
    let values = array(value, field)?;
    values
        .iter()
        .map(|value| {
            value
                .as_u64()
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(|| PartitionError::Invalid(format!("{field} must contain indices")))
        })
        .collect()
}

fn optional_integer_array(
    value: Option<&Value>,
    field: &str,
) -> Result<Vec<usize>, PartitionError> {
    if value.is_none() {
        return Ok(Vec::new());
    }
    integer_array(value, field)
}

fn finite_array(
    value: Option<&Value>,
    length: usize,
    field: &str,
) -> Result<Vec<f32>, PartitionError> {
    let values = array(value, field)?;
    if values.len() != length {
        return Err(PartitionError::Invalid(format!(
            "{field} must contain {length} finite numbers"
        )));
    }
    values
        .iter()
        .map(|value| {
            value
                .as_f64()
                .filter(|value| value.is_finite())
                .map(|value| value as f32)
                .ok_or_else(|| {
                    PartitionError::Invalid(format!("{field} must contain finite numbers"))
                })
        })
        .collect()
}

fn vector3(
    value: Option<&Value>,
    default: [f32; 3],
    field: &str,
) -> Result<[f32; 3], PartitionError> {
    if value.is_none() {
        return Ok(default);
    }
    let values = finite_array(value, 3, field)?;
    Ok([values[0], values[1], values[2]])
}

fn vector4(
    value: Option<&Value>,
    default: [f32; 4],
    field: &str,
) -> Result<[f32; 4], PartitionError> {
    if value.is_none() {
        return Ok(default);
    }
    let values = finite_array(value, 4, field)?;
    Ok([values[0], values[1], values[2], values[3]])
}

fn glam_bounds(min: Vec3, max: Vec3) -> Bounds {
    Bounds {
        min: studio_core::math::Vec3 {
            x: min.x,
            y: min.y,
            z: min.z,
        },
        max: studio_core::math::Vec3 {
            x: max.x,
            y: max.y,
            z: max.z,
        },
    }
}
