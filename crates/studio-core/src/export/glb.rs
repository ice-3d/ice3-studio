use std::collections::{BTreeMap, BTreeSet};

use glam::{Mat4, Quat, Vec3};
use serde_json::{Value, json};
use thiserror::Error;

use crate::document::{Document, MaterialShading};
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
    let evaluated = evaluate_large_scene(document)?;
    export_glb_from_scene(document, &evaluated)
}

pub fn export_glb_from_scene(
    document: &Document,
    evaluated: &EvaluatedScene,
) -> Result<Vec<u8>, ExportError> {
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
            let mut base_color = material.base_color.map(srgb_to_linear);
            base_color[3] = material.opacity;
            let edge_color = material.edge_color.map(srgb_color_hex);
            let shading = match material.shading.as_ref() {
                Some(MaterialShading::Flat) => "flat",
                Some(MaterialShading::Water) => "water",
                Some(MaterialShading::Directional) | None => "directional",
            };
            json!({
                "name": material.name,
                "pbrMetallicRoughness": {
                    "baseColorFactor": base_color,
                    "metallicFactor": 0.0,
                    "roughnessFactor": 0.9
                },
                "alphaMode": if material.opacity < 1.0 { "BLEND" } else { "OPAQUE" },
                "extras": {
                    "ice3": {
                        "id": material.id.as_str(),
                        "edges": edge_color.is_some(),
                        "edgeColor": edge_color,
                        "shading": shading
                    }
                }
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
        meshes.push(mesh_json(&mesh_key, variant, primitive, material));
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
    let (scene_min, scene_max) = scene_bounds(evaluated);
    let mut gltf = json!({
        "asset": {
            "version": "2.0",
            "generator": "ICE3 Studio",
            // 标明这是 Studio 直接产出的可运行时加载资产。Viewer 只接受带此标记的 GLB，
            // 因此内核自己导出的文件不需要再经过外部预处理步骤。
            "extras": {
                "ice3": {
                    "profile": "ice3-prepared",
                    "profileVersion": "1.0",
                    "assetId": document.project_id,
                    "bounds": { "min": scene_min, "max": scene_max }
                }
            }
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

/// 场景在世界空间的包围盒：把每个网格的局部包围盒用累乘的父链变换展开后合并。
fn scene_bounds(evaluated: &EvaluatedScene) -> ([f32; 3], [f32; 3]) {
    use std::collections::BTreeMap;

    let mut world: BTreeMap<String, Mat4> = BTreeMap::new();
    // 迭代解析，允许节点顺序任意；没有进展说明父链断裂，停止。
    for _ in 0..evaluated.nodes.len().max(1) {
        let mut progressed = false;
        for node in &evaluated.nodes {
            if world.contains_key(&node.id) {
                continue;
            }
            let local = Mat4::from_cols_array(&node_matrix(node));
            match node.parent_id.as_deref() {
                None => {
                    world.insert(node.id.clone(), local);
                    progressed = true;
                }
                Some(parent) if world.contains_key(parent) => {
                    let inherited = world[parent];
                    world.insert(node.id.clone(), inherited * local);
                    progressed = true;
                }
                Some(_) => {}
            }
        }
        if !progressed {
            break;
        }
    }

    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    let mut found = false;
    for node in evaluated.nodes.iter() {
        // 场地之类的环境元素可以声明不参与取景包围盒，避免把相机推远。
        if node
            .metadata
            .get("bounds")
            .is_some_and(|value| value == "ignore")
        {
            continue;
        }
        let (Some(mesh_key), Some(matrix)) = (&node.mesh_key, world.get(&node.id)) else {
            continue;
        };
        let Some(mesh) = evaluated.meshes.get(mesh_key) else {
            continue;
        };
        let bounds = mesh.bounds;
        for corner in 0..8 {
            let point = Vec3::new(
                if corner & 1 == 0 {
                    bounds.min.x
                } else {
                    bounds.max.x
                },
                if corner & 2 == 0 {
                    bounds.min.y
                } else {
                    bounds.max.y
                },
                if corner & 4 == 0 {
                    bounds.min.z
                } else {
                    bounds.max.z
                },
            );
            let world_point = matrix.transform_point3(point);
            min = [
                min[0].min(world_point.x),
                min[1].min(world_point.y),
                min[2].min(world_point.z),
            ];
            max = [
                max[0].max(world_point.x),
                max[1].max(world_point.y),
                max[2].max(world_point.z),
            ];
            found = true;
        }
    }
    if found {
        (min, max)
    } else {
        ([0.0; 3], [0.0; 3])
    }
}

fn mesh_json(
    key: &str,
    variant: usize,
    primitive: &ExportedPrimitive,
    material: Option<usize>,
) -> Value {
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
    // 同一几何会因材质不同产生多个变体，形如 ID 必须带变体号才能保持唯一。
    json!({
        "name": key,
        "extras": { "ice3": { "id": format!("{key}#{variant}") } },
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

fn srgb_color_hex(color: [f32; 4]) -> String {
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}",
        channel(color[0]),
        channel(color[1]),
        channel(color[2])
    )
}
