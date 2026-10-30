use std::collections::{BTreeMap, BTreeSet};

use glam::{Mat4, Quat, Vec3};
use serde_json::{Value, json};
use thiserror::Error;

use crate::document::Document;
use crate::geometry::EvaluatedMesh;
use crate::large_scene::evaluate::{EvaluatedScene, EvaluatedSceneNode, evaluate_large_scene};
use crate::validation::validate_document;

const GLB_MAGIC: u32 = 0x4654_6C67;
const GLB_VERSION: u32 = 2;
const JSON_CHUNK_TYPE: u32 = 0x4E4F_534A;
const BIN_CHUNK_TYPE: u32 = 0x004E_4942;

#[derive(Debug, Error)]
pub enum ExportError {
    #[error("document validation failed")]
    Validation,
    #[error("geometry evaluation failed: {0}")]
    Geometry(#[from] crate::geometry::GeometryError),
    #[error("derived scene hierarchy is invalid")]
    InvalidHierarchy,
    #[error("derived scene evaluation failed: {0}")]
    LargeScene(#[from] crate::large_scene::evaluate::LargeSceneError),
    #[error("GLB JSON encoding failed")]
    Json(#[source] serde_json::Error),
}

pub fn export_glb(document: &Document) -> Result<Vec<u8>, ExportError> {
    if !validate_document(document).is_empty() {
        return Err(ExportError::Validation);
    }
    let evaluated = evaluate_large_scene(document)?;
    export_glb_from_scene(document, &evaluated)
}

pub fn export_glb_from_scene(
    document: &Document,
    evaluated: &EvaluatedScene,
) -> Result<Vec<u8>, ExportError> {
    let mut binary = Vec::new();
    let mut buffer_views = Vec::<Value>::new();
    let mut accessors = Vec::<Value>::new();
    let material_indices: BTreeMap<_, _> = document
        .materials
        .iter()
        .enumerate()
        .map(|(index, material)| (material.id.clone(), index))
        .collect();
    let materials = document
        .materials
        .iter()
        .map(|material| {
            let mut base_color = material.base_color.map(srgb_to_linear);
            base_color[3] = material.opacity;
            json!({
                "name": material.name,
                "pbrMetallicRoughness": {
                    "baseColorFactor": base_color,
                    "metallicFactor": 0.0,
                    "roughnessFactor": 0.9
                },
                "alphaMode": if material.opacity < 1.0 { "BLEND" } else { "OPAQUE" }
            })
        })
        .collect::<Vec<_>>();
    let mut primitive_by_key = BTreeMap::new();
    for (key, mesh) in &evaluated.meshes {
        primitive_by_key.insert(
            key.clone(),
            export_geometry(mesh, &mut binary, &mut buffer_views, &mut accessors),
        );
    }

    let node_index_by_id: BTreeMap<_, _> = evaluated
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.id.clone(), index))
        .collect();
    let mut variant_keys = BTreeSet::<(String, Option<usize>)>::new();
    for node in &evaluated.nodes {
        let Some(mesh_key) = &node.mesh_key else {
            continue;
        };
        let material = node
            .material_id
            .as_ref()
            .and_then(|id| material_indices.get(id))
            .copied();
        variant_keys.insert((mesh_key.clone(), material));
    }
    let variant_keys = variant_keys.into_iter().collect::<Vec<_>>();
    let mut meshes = Vec::with_capacity(variant_keys.len());
    let mut mesh_variants = BTreeMap::new();
    for (variant, (mesh_key, material)) in variant_keys.into_iter().enumerate() {
        let primitive = primitive_by_key
            .get(&mesh_key)
            .ok_or(ExportError::InvalidHierarchy)?;
        meshes.push(mesh_json(&mesh_key, primitive, material));
        mesh_variants.insert((mesh_key, material), variant);
    }

    let mut child_indices: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (index, node) in evaluated.nodes.iter().enumerate() {
        if let Some(parent) = &node.parent_id {
            if !node_index_by_id.contains_key(parent) {
                return Err(ExportError::InvalidHierarchy);
            }
            child_indices.entry(parent.clone()).or_default().push(index);
        }
    }
    let nodes = evaluated
        .nodes
        .iter()
        .map(|node| {
            let id = node.id.as_str();
            let mut value = json!({
                "name": id,
                "matrix": node_matrix(node),
                "extras": {
                    "ice3": {
                        "id": id,
                        "ownerId": node.owner_id,
                        "displayName": node.name,
                        "metadata": node.metadata
                    }
                }
            });
            if !node.visible {
                value["extras"]["ice3"]["visible"] = Value::Bool(false);
            }
            if let Some(mesh_key) = &node.mesh_key {
                let material = node
                    .material_id
                    .as_ref()
                    .and_then(|id| material_indices.get(id))
                    .copied();
                let mesh = mesh_variants
                    .get(&(mesh_key.clone(), material))
                    .ok_or(ExportError::InvalidHierarchy)?;
                value["mesh"] = json!(mesh);
            }
            if let Some(children) = child_indices.get(id) {
                value["children"] = json!(children);
            }
            Ok(value)
        })
        .collect::<Result<Vec<_>, ExportError>>()?;
    let roots = evaluated
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(index, node)| node.parent_id.is_none().then_some(index))
        .collect::<Vec<_>>();
    let mut gltf = json!({
        "asset": {
            "version": "2.0",
            "generator": "ICE3 Studio"
        },
        "scene": 0,
        "scenes": [{ "nodes": roots }],
        "nodes": nodes,
        "meshes": meshes,
        "materials": materials,
        "accessors": accessors,
        "bufferViews": buffer_views,
        "buffers": [{ "byteLength": binary.len() }],
        "extras": {
            "ice3": {
                "projectId": document.project_id,
                "revision": document.revision.as_u64(),
                "relations": document.relations,
                "constraints": document.constraints
            }
        }
    });
    if binary.is_empty() {
        gltf.as_object_mut().expect("object").remove("buffers");
    }
    let json = serde_json::to_vec(&gltf).map_err(ExportError::Json)?;
    Ok(pack_glb(json, binary))
}

struct ExportedPrimitive {
    position_accessor: usize,
    normal_accessor: usize,
    index_accessor: usize,
}

fn export_geometry(
    mesh: &EvaluatedMesh,
    binary: &mut Vec<u8>,
    buffer_views: &mut Vec<Value>,
    accessors: &mut Vec<Value>,
) -> ExportedPrimitive {
    let positions = mesh
        .vertices
        .iter()
        .flat_map(|vertex| vertex.position)
        .collect::<Vec<_>>();
    let normals = mesh
        .vertices
        .iter()
        .flat_map(|vertex| vertex.normal)
        .collect::<Vec<_>>();
    let position_view = append_view(binary, buffer_views, bytemuck_f32(&positions), Some(34962));
    let normal_view = append_view(binary, buffer_views, bytemuck_f32(&normals), Some(34962));
    let index_bytes = if mesh.vertices.len() <= u16::MAX as usize {
        mesh.indices
            .iter()
            .flat_map(|index| (*index as u16).to_le_bytes())
            .collect::<Vec<_>>()
    } else {
        mesh.indices
            .iter()
            .flat_map(|index| index.to_le_bytes())
            .collect::<Vec<_>>()
    };
    let index_view = append_view(binary, buffer_views, index_bytes, Some(34963));
    let position_accessor = accessors.len();
    let min = [mesh.bounds.min.x, mesh.bounds.min.y, mesh.bounds.min.z];
    let max = [mesh.bounds.max.x, mesh.bounds.max.y, mesh.bounds.max.z];
    accessors.push(json!({
        "bufferView": position_view,
        "componentType": 5126,
        "count": positions.len() / 3,
        "type": "VEC3",
        "min": min,
        "max": max
    }));
    let normal_accessor = accessors.len();
    accessors.push(json!({
        "bufferView": normal_view,
        "componentType": 5126,
        "count": normals.len() / 3,
        "type": "VEC3"
    }));
    let index_accessor = accessors.len();
    accessors.push(json!({
        "bufferView": index_view,
        "componentType": if mesh.vertices.len() <= u16::MAX as usize { 5123 } else { 5125 },
        "count": mesh.indices.len(),
        "type": "SCALAR"
    }));
    ExportedPrimitive {
        position_accessor,
        normal_accessor,
        index_accessor,
    }
}

fn mesh_json(key: &str, primitive: &ExportedPrimitive, material: Option<usize>) -> Value {
    let mut value = json!({
        "attributes": {
            "POSITION": primitive.position_accessor,
            "NORMAL": primitive.normal_accessor
        },
        "indices": primitive.index_accessor,
        "mode": 4
    });
    if let Some(material) = material {
        value["material"] = json!(material);
    }
    json!({
        "name": key,
        "primitives": [value]
    })
}

fn append_view(
    binary: &mut Vec<u8>,
    views: &mut Vec<Value>,
    bytes: Vec<u8>,
    target: Option<u32>,
) -> usize {
    while !binary.len().is_multiple_of(4) {
        binary.push(0);
    }
    let offset = binary.len();
    binary.extend_from_slice(&bytes);
    let index = views.len();
    let mut view = json!({
        "buffer": 0,
        "byteOffset": offset,
        "byteLength": bytes.len()
    });
    if let Some(target) = target {
        view["target"] = json!(target);
    }
    views.push(view);
    index
}

fn pack_glb(mut json: Vec<u8>, mut binary: Vec<u8>) -> Vec<u8> {
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    while !binary.len().is_multiple_of(4) {
        binary.push(0);
    }
    let total = 12
        + 8
        + json.len()
        + if binary.is_empty() {
            0
        } else {
            8 + binary.len()
        };
    let mut output = Vec::with_capacity(total);
    output.extend_from_slice(&GLB_MAGIC.to_le_bytes());
    output.extend_from_slice(&GLB_VERSION.to_le_bytes());
    output.extend_from_slice(&(total as u32).to_le_bytes());
    output.extend_from_slice(&(json.len() as u32).to_le_bytes());
    output.extend_from_slice(&JSON_CHUNK_TYPE.to_le_bytes());
    output.extend_from_slice(&json);
    if !binary.is_empty() {
        output.extend_from_slice(&(binary.len() as u32).to_le_bytes());
        output.extend_from_slice(&BIN_CHUNK_TYPE.to_le_bytes());
        output.extend_from_slice(&binary);
    }
    output
}

fn node_matrix(node: &EvaluatedSceneNode) -> [f32; 16] {
    Mat4::from_scale_rotation_translation(
        Vec3::new(
            node.transform.scale.x,
            node.transform.scale.y,
            node.transform.scale.z,
        ),
        Quat::from_array(node.transform.rotation),
        Vec3::new(
            node.transform.translation.x,
            node.transform.translation.y,
            node.transform.translation.z,
        ),
    )
    .to_cols_array()
}

fn bytemuck_f32(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}
