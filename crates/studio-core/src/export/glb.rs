use std::collections::BTreeMap;

use glam::{Mat4, Quat, Vec3};
use serde_json::{Value, json};
use thiserror::Error;

use crate::document::{Document, NodeContent, SceneNode};
use crate::geometry::{EvaluatedMesh, evaluate_node_content};
use crate::ids::NodeId;
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
    #[error("GLB JSON encoding failed")]
    Json(#[source] serde_json::Error),
}

pub fn export_glb(document: &Document) -> Result<Vec<u8>, ExportError> {
    if !validate_document(document).is_empty() {
        return Err(ExportError::Validation);
    }
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
            json!({
                "name": material.name,
                "pbrMetallicRoughness": {
                    "baseColorFactor": material
                        .base_color
                        .map(srgb_to_linear),
                    "metallicFactor": 0.0,
                    "roughnessFactor": 0.9,
                    "alphaMode": if material.opacity < 1.0 { "BLEND" } else { "OPAQUE" }
                }
            })
        })
        .collect::<Vec<_>>();
    let mut meshes = Vec::new();
    let mut mesh_index_by_node = BTreeMap::<NodeId, usize>::new();
    for node in document.scene.nodes() {
        if matches!(node.content, NodeContent::Group) {
            continue;
        }
        let evaluated = evaluate_node_content(&node.content)?;
        let mesh_index = meshes.len();
        mesh_index_by_node.insert(node.id.clone(), mesh_index);
        meshes.push(export_mesh(
            node,
            &evaluated,
            material_indices
                .get(
                    &node
                        .material_id
                        .clone()
                        .unwrap_or_else(|| crate::ids::MaterialId::from("missing-000000")),
                )
                .copied(),
            &mut binary,
            &mut buffer_views,
            &mut accessors,
        ));
    }

    let mut child_indices: BTreeMap<NodeId, Vec<usize>> = BTreeMap::new();
    for (index, node) in document.scene.nodes().iter().enumerate() {
        if let Some(parent) = &node.parent_id {
            child_indices.entry(parent.clone()).or_default().push(index);
        }
    }
    let nodes = document
        .scene
        .nodes()
        .iter()
        .enumerate()
        .map(|(index, node)| {
            let mut value = json!({
                "name": node.id.as_str(),
                "matrix": node_matrix(node),
                "extras": {
                    "ice3": {
                        "id": node.id.as_str(),
                        "displayName": node.name,
                        "metadata": node.metadata
                    }
                }
            });
            if !node.visible {
                value["extras"]["ice3"]["visible"] = Value::Bool(false);
            }
            if let Some(mesh) = mesh_index_by_node.get(&node.id) {
                value["mesh"] = json!(mesh);
            }
            if let Some(children) = child_indices.get(&node.id) {
                value["children"] = json!(children);
            }
            let _ = index;
            value
        })
        .collect::<Vec<_>>();
    let roots = document
        .scene
        .nodes()
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

fn export_mesh(
    node: &SceneNode,
    mesh: &EvaluatedMesh,
    material: Option<usize>,
    binary: &mut Vec<u8>,
    buffer_views: &mut Vec<Value>,
    accessors: &mut Vec<Value>,
) -> Value {
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
    let mut primitive = json!({
        "attributes": {
            "POSITION": position_accessor,
            "NORMAL": normal_accessor
        },
        "indices": index_accessor,
        "mode": 4
    });
    if let Some(material) = material {
        primitive["material"] = json!(material);
    }
    json!({
        "name": node.id.as_str(),
        "primitives": [primitive]
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

fn node_matrix(node: &SceneNode) -> [f32; 16] {
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
