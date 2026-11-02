use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use glam::{Mat4, Vec3};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use crate::prepare_canonical;
use crate::{
    AssetError, AssetMetrics, AssetResult, BudgetProfile, BudgetStatus, PreparationMetadata,
    PrepareArtifact, PrepareOptions, edge_color_from_material, evaluate_budget,
    generate_edge_indices, material_suppresses_edges,
};

#[derive(Debug, Clone, PartialEq)]
pub struct CanonicalMaterial {
    pub id: String,
    pub name: String,
    pub base_color: [f32; 3],
    pub opacity: f32,
    pub extras: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CanonicalPrimitive {
    pub positions: Vec<[f32; 3]>,
    pub normals: Option<Vec<[f32; 3]>>,
    pub colors: Option<Vec<[f32; 3]>>,
    pub indices: Vec<u32>,
    pub material: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CanonicalMesh {
    pub id: String,
    pub primitives: Vec<CanonicalPrimitive>,
    pub extras: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CanonicalNode {
    pub id: String,
    pub name: String,
    pub parent: Option<String>,
    pub matrix: [f32; 16],
    pub mesh: Option<usize>,
    pub material: Option<usize>,
    pub extras: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CanonicalScene {
    pub asset_id: String,
    pub source_format: String,
    pub nodes: Vec<CanonicalNode>,
    pub meshes: Vec<CanonicalMesh>,
    pub materials: Vec<CanonicalMaterial>,
    pub asset_extras: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedGlbOptions {
    pub asset_id: String,
    pub source_format: String,
    pub source_digest: String,
}

#[derive(Default)]
struct BinaryBuilder {
    bytes: Vec<u8>,
    views: Vec<Value>,
    accessors: Vec<Value>,
}

struct MeshWriteContext<'a> {
    asset_id: &'a str,
    binary: &'a mut BinaryBuilder,
    source_materials: &'a [CanonicalMaterial],
    material_indices: &'a MaterialIndices,
    generate_edges: bool,
    crease_angle_degrees: f32,
}

impl BinaryBuilder {
    fn append(&mut self, bytes: &[u8], target: Option<u32>) -> usize {
        while !self.bytes.len().is_multiple_of(4) {
            self.bytes.push(0);
        }
        let offset = self.bytes.len();
        self.bytes.extend_from_slice(bytes);
        let index = self.views.len();
        let mut view = json!({
            "buffer": 0,
            "byteOffset": offset,
            "byteLength": bytes.len()
        });
        if let Some(target) = target {
            view["target"] = Value::from(target);
        }
        self.views.push(view);
        index
    }

    fn accessor(&mut self, accessor: Value) -> usize {
        let index = self.accessors.len();
        self.accessors.push(accessor);
        index
    }
}

pub fn write_prepared_glb(
    scene: &CanonicalScene,
    options: &PreparedGlbOptions,
) -> AssetResult<Vec<u8>> {
    prepare_canonical(
        scene,
        PrepareOptions {
            asset_id: options.asset_id.clone(),
            source_format: options.source_format.clone(),
            source_digest: options.source_digest.clone(),
            crease_angle_degrees: 30.0,
            generate_edges: false,
            budget: BudgetProfile::interactive_desktop_v1(),
        },
    )
    .map(|artifact| artifact.bytes)
}

pub(crate) fn prepare_canonical_document(
    scene: &CanonicalScene,
    options: PrepareOptions,
) -> AssetResult<PrepareArtifact> {
    let (mut gltf, binary) = build_canonical_document(scene, &options)?;
    let mut metrics = metrics_from_document(&gltf, 0, 0, 0)?;
    let preparation = preparation_metadata(&options);

    for _ in 0..16 {
        let budget = evaluate_budget(metrics, &options.budget);
        if budget.status == BudgetStatus::Error {
            return Err(AssetError::BudgetExceeded);
        }
        set_profile_stats(&mut gltf, metrics)?;
        set_profile_budget(&mut gltf, &budget)?;
        set_profile_preparation(&mut gltf, &preparation)?;
        let (bytes, json_chunk_bytes, bin_chunk_bytes) = serialize_and_pack(&gltf, &binary)?;
        let next =
            metrics_from_document(&gltf, bytes.len() as u64, json_chunk_bytes, bin_chunk_bytes)?;
        if next == metrics {
            let artifact = PrepareArtifact {
                bytes,
                metrics,
                preparation,
                budget,
                content_digest: format!("{:x}", Sha256::digest(&binary)),
                warnings: Vec::new(),
            };
            let source = crate::read_glb_bytes(&artifact.bytes, Path::new("."))?;
            crate::validate_prepared_profile(&source)?;
            return Ok(artifact);
        }
        metrics = next;
    }

    Err(invalid(
        "Prepared metrics did not converge to a stable encoded size",
    ))
}

fn build_canonical_document(
    scene: &CanonicalScene,
    options: &PrepareOptions,
) -> AssetResult<(Value, Vec<u8>)> {
    let prepared_options = PreparedGlbOptions {
        asset_id: options.asset_id.clone(),
        source_format: options.source_format.clone(),
        source_digest: options.source_digest.clone(),
    };
    let asset_id = prepared_options.asset_id.trim();
    if asset_id.is_empty() || asset_id != scene.asset_id.trim() {
        return Err(invalid(
            "asset ID must be non-empty and match the source scene",
        ));
    }
    validate_materials(&scene.materials)?;
    let node_indices = node_indices(scene)?;
    let world_matrices = world_matrices(scene, &node_indices)?;
    let edge_materials = used_edge_materials(scene)?;
    let (materials, material_indices) =
        output_materials(&scene.materials, &edge_materials, asset_id)?;
    let mut binary = BinaryBuilder::default();
    let mut mesh_context = MeshWriteContext {
        asset_id,
        binary: &mut binary,
        source_materials: &scene.materials,
        material_indices: &material_indices,
        generate_edges: options.generate_edges,
        crease_angle_degrees: options.crease_angle_degrees,
    };
    let mut output_meshes = Vec::new();
    let mut mesh_variants = BTreeMap::new();
    let mut mesh_indices = Vec::new();
    let mut node_meshes = Vec::with_capacity(scene.nodes.len());
    for (node_index, node) in scene.nodes.iter().enumerate() {
        let Some(mesh_index) = node.mesh else {
            node_meshes.push(None);
            continue;
        };
        if mesh_index >= scene.meshes.len() {
            return Err(invalid("node mesh index is out of range"));
        }
        if let Some(material) = node.material
            && material >= scene.materials.len()
        {
            return Err(invalid("node material index is out of range"));
        }
        let key = (mesh_index, node.material);
        let output_index = if let Some(index) = mesh_variants.get(&key) {
            *index
        } else {
            let mesh = &scene.meshes[mesh_index];
            let output = write_mesh(mesh, node.material, &mut mesh_context)?;
            let index = output_meshes.len();
            output_meshes.push(output);
            mesh_indices.push(mesh_index);
            mesh_variants.insert(key, index);
            index
        };
        node_meshes.push(Some(output_index));
        let _ = node_index;
    }

    let mut children = vec![Vec::new(); scene.nodes.len()];
    let mut roots = Vec::new();
    for (index, node) in scene.nodes.iter().enumerate() {
        if let Some(parent) = &node.parent {
            let parent_index = *node_indices
                .get(parent)
                .ok_or_else(|| invalid("node parent is missing"))?;
            children[parent_index].push(index);
        } else {
            roots.push(index);
        }
    }
    let mut selection_ids = BTreeSet::new();
    let output_nodes = scene
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            output_node(
                node,
                index,
                node_meshes[index],
                &children[index],
                asset_id,
                &mut selection_ids,
            )
        })
        .collect::<AssetResult<Vec<_>>>()?;
    validate_output_meshes(&scene.meshes)?;
    let byte_length = binary.bytes.len();
    while !binary.bytes.len().is_multiple_of(4) {
        binary.bytes.push(0);
    }
    let content_digest = format!("{:x}", Sha256::digest(&binary.bytes));
    let bounds = output_bounds(scene, &node_meshes, &world_matrices)?;
    let stats = json!({
        "nodeCount": output_nodes.len(),
        "maxNodeDepth": max_depth(&roots, &children),
        "meshCount": output_meshes.len(),
        "primitiveCount": output_meshes
            .iter()
            .map(|mesh| mesh["primitives"].as_array().map_or(0, Vec::len))
            .sum::<usize>(),
        "triangleCount": scene.meshes.iter().flat_map(|mesh| &mesh.primitives)
            .map(|primitive| primitive.indices.len() as u64 / 3)
            .sum::<u64>(),
        "edgeSegmentCount": 0,
        "materialCount": materials.len(),
        "selectionGroupCount": selection_ids.len()
    });
    let mut profile = match &scene.asset_extras {
        Value::Object(object) => object.clone(),
        Value::Null => Map::new(),
        _ => return Err(invalid("asset extras must be an object or null")),
    };
    profile.insert("profile".into(), Value::from("ice3-prepared"));
    profile.insert("profileVersion".into(), Value::from("1.0"));
    profile.insert("contractVersion".into(), Value::from("1.0"));
    profile.insert("assetId".into(), Value::from(asset_id));
    profile.insert(
        "sourceFormat".into(),
        Value::from(prepared_options.source_format.as_str()),
    );
    profile.insert(
        "sourceDigest".into(),
        Value::from(prepared_options.source_digest.as_str()),
    );
    profile.insert(
        "contentDigest".into(),
        json!({
            "algorithm": "sha256",
            "value": content_digest,
            "covers": "binary-chunk"
        }),
    );
    profile.insert("bounds".into(), bounds_json(bounds));
    profile.insert("stats".into(), stats);
    profile.insert(
        "preparation".into(),
        json!({
            "runtimeConversionRequired": false,
            "sourceFormat": prepared_options.source_format,
            "unitsNormalized": true,
            "matrixTransformsBaked": false,
            "requiredCompressionDecoded": true,
            "edgesPrepared": false,
            "selectionGroupsPrepared": true
        }),
    );
    profile.insert(
        "budget".into(),
        json!({
            "profile": "interactive-desktop-v1",
            "status": "pass",
            "warningCount": 0,
            "errorCount": 0
        }),
    );
    let mut gltf = json!({
        "asset": {
            "version": "2.0",
            "generator": "ICE3 Studio Native Import",
            "extras": {"ice3": profile}
        },
        "scene": 0,
        "scenes": [{"nodes": roots}],
        "nodes": output_nodes,
        "meshes": output_meshes,
        "materials": materials,
        "accessors": binary.accessors,
        "bufferViews": binary.views,
        "buffers": [{"byteLength": byte_length}]
    });
    if binary.bytes.is_empty() {
        gltf.as_object_mut().expect("object").remove("buffers");
    }
    Ok((gltf, binary.bytes))
}

fn write_mesh(
    mesh: &CanonicalMesh,
    material_override: Option<usize>,
    context: &mut MeshWriteContext<'_>,
) -> AssetResult<Value> {
    let mut primitives = Vec::with_capacity(mesh.primitives.len());
    for primitive in &mesh.primitives {
        validate_primitive(primitive)?;
        let positions = primitive
            .positions
            .iter()
            .flat_map(|value| *value)
            .collect::<Vec<_>>();
        let position_view = context
            .binary
            .append(bytemuck::cast_slice(&positions), Some(34962));
        let position_accessor = context.binary.accessor(json!({
            "bufferView": position_view,
            "componentType": 5126,
            "count": primitive.positions.len(),
            "type": "VEC3",
            "min": component_bounds(&primitive.positions, 0),
            "max": component_bounds(&primitive.positions, 1)
        }));
        let mut attributes = Map::from_iter([("POSITION".into(), Value::from(position_accessor))]);
        let generated_normals = primitive
            .normals
            .is_none()
            .then(|| compute_normals(&primitive.positions, &primitive.indices));
        let normals = primitive.normals.as_ref().or(generated_normals.as_ref());
        if let Some(normals) = normals {
            let values = normals.iter().flat_map(|value| *value).collect::<Vec<_>>();
            let view = context
                .binary
                .append(bytemuck::cast_slice(&values), Some(34962));
            let accessor = context.binary.accessor(json!({
                "bufferView": view,
                "componentType": 5126,
                "count": normals.len(),
                "type": "VEC3"
            }));
            attributes.insert("NORMAL".into(), Value::from(accessor));
        }
        if let Some(colors) = &primitive.colors {
            let values = colors.iter().flat_map(|value| *value).collect::<Vec<_>>();
            let view = context
                .binary
                .append(bytemuck::cast_slice(&values), Some(34962));
            let accessor = context.binary.accessor(json!({
                "bufferView": view,
                "componentType": 5126,
                "count": colors.len(),
                "type": "VEC3"
            }));
            attributes.insert("COLOR_0".into(), Value::from(accessor));
        }
        let index_accessor = write_indices(
            context.binary,
            &primitive.indices,
            primitive.positions.len(),
        );
        let source_material = material_override.or(primitive.material);
        if let Some(material) = source_material
            && material >= context.material_indices.source_count
        {
            return Err(invalid("primitive material index is out of range"));
        }
        let triangle_material = if primitive.colors.is_some() {
            context.material_indices.vertex_colors
        } else {
            source_material.unwrap_or(context.material_indices.default)
        };
        let edge_material = source_material
            .and_then(|index| context.material_indices.edges[index])
            .unwrap_or(context.material_indices.default_edges);
        let output = json!({
            "attributes": attributes,
            "indices": index_accessor,
            "mode": 4,
            "material": triangle_material
        });
        primitives.push(output);

        let suppressed = source_material.is_some_and(|index| {
            material_suppresses_edges(&context.source_materials[index].extras)
        });
        if context.generate_edges && !suppressed {
            let edge_indices = generate_edge_indices(
                &primitive.positions,
                &primitive.indices,
                context.crease_angle_degrees,
            )?;
            if !edge_indices.is_empty() {
                let edge_accessor =
                    write_indices(context.binary, &edge_indices, primitive.positions.len());
                primitives.push(json!({
                    "attributes": {"POSITION": position_accessor},
                    "indices": edge_accessor,
                    "mode": 1,
                    "material": edge_material,
                    "extras": {
                        "ice3": {
                            "role": "edges",
                            "widthPx": 3
                        }
                    }
                }));
            }
        }
    }
    let mut extras = mesh.extras.clone();
    ensure_ice3_id(
        &mut extras,
        &format!("{}/mesh/{}", context.asset_id, mesh.id),
        None,
    )?;
    Ok(json!({
        "name": mesh.id,
        "primitives": primitives,
        "extras": extras
    }))
}

fn write_indices(binary: &mut BinaryBuilder, indices: &[u32], vertex_count: usize) -> usize {
    let (index_bytes, component_type) = if vertex_count <= u16::MAX as usize {
        (
            indices
                .iter()
                .flat_map(|index| (*index as u16).to_le_bytes())
                .collect::<Vec<_>>(),
            5123,
        )
    } else {
        (
            indices
                .iter()
                .flat_map(|index| index.to_le_bytes())
                .collect::<Vec<_>>(),
            5125,
        )
    };
    let index_view = binary.append(&index_bytes, Some(34963));
    binary.accessor(json!({
        "bufferView": index_view,
        "componentType": component_type,
        "count": indices.len(),
        "type": "SCALAR"
    }))
}

struct MaterialIndices {
    source_count: usize,
    edges: Vec<Option<usize>>,
    default: usize,
    default_edges: usize,
    vertex_colors: usize,
}

fn output_materials(
    materials: &[CanonicalMaterial],
    edge_materials: &[bool],
    asset_id: &str,
) -> AssetResult<(Vec<Value>, MaterialIndices)> {
    if edge_materials.len() != materials.len() {
        return Err(invalid(
            "edge material usage does not match source materials",
        ));
    }
    let source_count = materials.len();
    let mut output = materials
        .iter()
        .map(|material| {
            let mut extras = material.extras.clone();
            ensure_ice3_id(
                &mut extras,
                &format!("{asset_id}/material/{}", material.id),
                None,
            )?;
            Ok(json!({
                "name": material.name,
                "pbrMetallicRoughness": {
                    "baseColorFactor": [
                        material.base_color[0],
                        material.base_color[1],
                        material.base_color[2],
                        material.opacity
                    ],
                    "metallicFactor": 0.0,
                    "roughnessFactor": 1.0
                },
                "alphaMode": if material.opacity < 1.0 { "BLEND" } else { "OPAQUE" },
                "extras": extras
            }))
        })
        .collect::<AssetResult<Vec<_>>>()?;

    let mut edges = vec![None; source_count];
    for (index, material) in materials.iter().enumerate() {
        if !edge_materials[index] {
            continue;
        }
        let edge_color = edge_color_from_material(&material.extras)?;
        let mut extras = json!({
            "ice3": {
                "id": format!("{asset_id}/material/{index}/edges")
            }
        });
        ensure_ice3_id(
            &mut extras,
            &format!("{asset_id}/material/{index}/edges"),
            None,
        )?;
        edges[index] = Some(output.len());
        output.push(json!({
            "name": format!("{} edges", material.name),
            "pbrMetallicRoughness": {
                "baseColorFactor": edge_color,
                "metallicFactor": 0.0,
                "roughnessFactor": 1.0
            },
            "alphaMode": if edge_color[3] < 1.0 { "BLEND" } else { "OPAQUE" },
            "extras": extras
        }));
    }

    let default = output.len();
    output.push(json!({
        "name": "default",
        "pbrMetallicRoughness": {
            "baseColorFactor": [0.5, 0.5, 0.5, 1.0],
            "metallicFactor": 0.0,
            "roughnessFactor": 1.0
        },
        "alphaMode": "OPAQUE",
        "extras": {"ice3": {"id": format!("{asset_id}/material/default")}}
    }));
    let default_edges = output.len();
    output.push(json!({
        "name": "default edges",
        "pbrMetallicRoughness": {
            "baseColorFactor": [230.0 / 255.0, 242.0 / 255.0, 1.0, 1.0],
            "metallicFactor": 0.0,
            "roughnessFactor": 1.0
        },
        "alphaMode": "OPAQUE",
        "extras": {"ice3": {"id": format!("{asset_id}/material/default/edges")}}
    }));
    let vertex_colors = output.len();
    output.push(json!({
        "name": "vertex colors",
        "pbrMetallicRoughness": {
            "baseColorFactor": [1.0, 1.0, 1.0, 1.0],
            "metallicFactor": 0.0,
            "roughnessFactor": 1.0
        },
        "alphaMode": "OPAQUE",
        "extras": {"ice3": {"id": format!("{asset_id}/material/vertex-colors")}}
    }));

    Ok((
        output,
        MaterialIndices {
            source_count,
            edges,
            default,
            default_edges,
            vertex_colors,
        },
    ))
}

fn used_edge_materials(scene: &CanonicalScene) -> AssetResult<Vec<bool>> {
    let mut used = vec![false; scene.materials.len()];
    for node in &scene.nodes {
        let Some(mesh_index) = node.mesh else {
            continue;
        };
        let mesh = scene
            .meshes
            .get(mesh_index)
            .ok_or_else(|| invalid("node mesh index is out of range"))?;
        if let Some(material) = node.material
            && material >= scene.materials.len()
        {
            return Err(invalid("node material index is out of range"));
        }
        for primitive in &mesh.primitives {
            let source_material = node.material.or(primitive.material);
            if let Some(material) = source_material
                && material >= scene.materials.len()
            {
                return Err(invalid("primitive material index is out of range"));
            }
            if let Some(material) = source_material
                && !material_suppresses_edges(&scene.materials[material].extras)
            {
                used[material] = true;
            }
        }
    }
    Ok(used)
}

fn output_node(
    node: &CanonicalNode,
    index: usize,
    mesh: Option<usize>,
    children: &[usize],
    asset_id: &str,
    selection_ids: &mut BTreeSet<String>,
) -> AssetResult<Value> {
    if node.matrix.iter().any(|value| !value.is_finite()) {
        return Err(invalid("node matrix contains a non-finite value"));
    }
    let mut extras = node.extras.clone();
    let selection_id = ensure_ice3_id(&mut extras, &node.id, Some(asset_id))?;
    selection_ids.insert(selection_id);
    let mut output = json!({
        "name": node.name,
        "matrix": node.matrix,
        "extras": extras
    });
    if let Some(mesh) = mesh {
        output["mesh"] = Value::from(mesh);
    }
    if !children.is_empty() {
        output["children"] = json!(children);
    }
    let _ = index;
    Ok(output)
}

fn ensure_ice3_id(extras: &mut Value, id: &str, asset_id: Option<&str>) -> AssetResult<String> {
    if extras.is_null() {
        *extras = json!({});
    }
    let object = extras
        .as_object_mut()
        .ok_or_else(|| invalid("extras must be an object"))?;
    let ice3 = object
        .entry("ice3")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| invalid("ice3 extras must be an object"))?;
    ice3.entry("id")
        .or_insert_with(|| Value::from(id.to_owned()));
    let selection = ice3
        .get("selectionId")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| {
            asset_id
                .map(|asset| format!("{asset}/node/{id}"))
                .unwrap_or_else(|| id.to_owned())
        });
    ice3.entry("selectionId")
        .or_insert_with(|| Value::from(selection.clone()));
    Ok(selection)
}

fn validate_materials(materials: &[CanonicalMaterial]) -> AssetResult<()> {
    let mut ids = BTreeSet::new();
    for material in materials {
        if material.id.trim().is_empty()
            || !ids.insert(&material.id)
            || !unit_value(material.opacity)
            || material.base_color.iter().any(|value| !unit_value(*value))
        {
            return Err(invalid("material is invalid"));
        }
    }
    Ok(())
}

fn validate_primitive(primitive: &CanonicalPrimitive) -> AssetResult<()> {
    if primitive.positions.is_empty()
        || primitive.indices.is_empty()
        || !primitive.indices.len().is_multiple_of(3)
        || primitive
            .positions
            .iter()
            .flatten()
            .any(|value| !value.is_finite())
        || primitive
            .indices
            .iter()
            .any(|index| *index as usize >= primitive.positions.len())
    {
        return Err(invalid("primitive geometry is invalid"));
    }
    if primitive
        .normals
        .as_ref()
        .is_some_and(|normals| normals.len() != primitive.positions.len())
        || primitive
            .colors
            .as_ref()
            .is_some_and(|colors| colors.len() != primitive.positions.len())
    {
        return Err(invalid("primitive attributes do not match positions"));
    }
    Ok(())
}

fn validate_output_meshes(meshes: &[CanonicalMesh]) -> AssetResult<()> {
    let mut ids = BTreeSet::new();
    for mesh in meshes {
        if mesh.id.trim().is_empty() || !ids.insert(&mesh.id) || mesh.primitives.is_empty() {
            return Err(invalid("mesh is invalid"));
        }
    }
    Ok(())
}

fn compute_normals(positions: &[[f32; 3]], indices: &[u32]) -> Vec<[f32; 3]> {
    let mut normals = vec![Vec3::ZERO; positions.len()];
    for triangle in indices.as_chunks::<3>().0 {
        let [a, b, c] = triangle.map(|index| Vec3::from_array(positions[index as usize]));
        let normal = (b - a).cross(c - a);
        if normal.length_squared() <= f32::EPSILON {
            continue;
        }
        normals[triangle[0] as usize] += normal;
        normals[triangle[1] as usize] += normal;
        normals[triangle[2] as usize] += normal;
    }
    normals
        .into_iter()
        .map(|normal| {
            if normal.length_squared() <= f32::EPSILON {
                Vec3::Z.to_array()
            } else {
                normal.normalize().to_array()
            }
        })
        .collect()
}

fn node_indices(scene: &CanonicalScene) -> AssetResult<BTreeMap<String, usize>> {
    let mut output = BTreeMap::new();
    for (index, node) in scene.nodes.iter().enumerate() {
        if node.id.trim().is_empty() || output.insert(node.id.clone(), index).is_some() {
            return Err(invalid("node ID is empty or duplicated"));
        }
    }
    Ok(output)
}

fn world_matrices(
    scene: &CanonicalScene,
    indices: &BTreeMap<String, usize>,
) -> AssetResult<Vec<Mat4>> {
    fn visit(
        index: usize,
        scene: &CanonicalScene,
        indices: &BTreeMap<String, usize>,
        visiting: &mut BTreeSet<usize>,
        visited: &mut BTreeMap<usize, Mat4>,
    ) -> AssetResult<Mat4> {
        if let Some(matrix) = visited.get(&index) {
            return Ok(*matrix);
        }
        if !visiting.insert(index) {
            return Err(invalid("node hierarchy contains a cycle"));
        }
        let node = &scene.nodes[index];
        let parent = if let Some(parent) = &node.parent {
            let parent_index = *indices
                .get(parent)
                .ok_or_else(|| invalid("node parent is missing"))?;
            visit(parent_index, scene, indices, visiting, visited)?
        } else {
            Mat4::IDENTITY
        };
        let matrix = parent * Mat4::from_cols_array(&node.matrix);
        visiting.remove(&index);
        visited.insert(index, matrix);
        Ok(matrix)
    }
    let mut visited = BTreeMap::new();
    for index in 0..scene.nodes.len() {
        visit(index, scene, indices, &mut BTreeSet::new(), &mut visited)?;
    }
    (0..scene.nodes.len())
        .map(|index| {
            visited
                .get(&index)
                .copied()
                .ok_or_else(|| invalid("node world matrix is missing"))
        })
        .collect()
}

fn output_bounds(
    scene: &CanonicalScene,
    node_meshes: &[Option<usize>],
    world_matrices: &[Mat4],
) -> AssetResult<([f32; 3], [f32; 3])> {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for (node_index, node) in scene.nodes.iter().enumerate() {
        let Some(mesh_index) = node.mesh else {
            continue;
        };
        let _ = node_meshes
            .get(node_index)
            .ok_or_else(|| invalid("node mesh output is missing"))?;
        let matrix = world_matrices[node_index];
        for primitive in &scene.meshes[mesh_index].primitives {
            for position in &primitive.positions {
                let point = matrix.transform_point3(Vec3::from_array(*position));
                for axis in 0..3 {
                    min[axis] = min[axis].min(point[axis]);
                    max[axis] = max[axis].max(point[axis]);
                }
            }
        }
    }
    if min.iter().any(|value| !value.is_finite()) || max.iter().any(|value| !value.is_finite()) {
        return Ok(([0.0; 3], [0.0; 3]));
    }
    Ok((min, max))
}

fn bounds_json((min, max): ([f32; 3], [f32; 3])) -> Value {
    json!({"min": min, "max": max})
}

fn component_bounds(positions: &[[f32; 3]], which: usize) -> [f32; 3] {
    let mut output = if which == 0 {
        [f32::INFINITY; 3]
    } else {
        [f32::NEG_INFINITY; 3]
    };
    for position in positions {
        for axis in 0..3 {
            output[axis] = if which == 0 {
                output[axis].min(position[axis])
            } else {
                output[axis].max(position[axis])
            };
        }
    }
    output
}

fn max_depth(roots: &[usize], children: &[Vec<usize>]) -> usize {
    fn visit(index: usize, children: &[Vec<usize>]) -> usize {
        children[index]
            .iter()
            .map(|child| 1 + visit(*child, children))
            .max()
            .unwrap_or(0)
    }
    roots
        .iter()
        .map(|root| visit(*root, children))
        .max()
        .unwrap_or(0)
}

fn serialize_and_pack(gltf: &Value, binary: &[u8]) -> AssetResult<(Vec<u8>, u64, u64)> {
    let json = serde_json::to_vec(gltf).map_err(|_| invalid("canonical JSON failed"))?;
    let mut json_chunk = json;
    while !json_chunk.len().is_multiple_of(4) {
        json_chunk.push(b' ');
    }
    let mut binary_chunk = binary.to_vec();
    while !binary_chunk.len().is_multiple_of(4) {
        binary_chunk.push(0);
    }
    let total = 12_usize
        .checked_add(8 + json_chunk.len())
        .and_then(|value| value.checked_add(8 + binary_chunk.len()))
        .ok_or_else(|| invalid("canonical GLB size overflowed"))?;
    let mut glb = Vec::with_capacity(total);
    glb.extend_from_slice(&0x4654_6C67_u32.to_le_bytes());
    glb.extend_from_slice(&2_u32.to_le_bytes());
    glb.extend_from_slice(&(total as u32).to_le_bytes());
    glb.extend_from_slice(&(json_chunk.len() as u32).to_le_bytes());
    glb.extend_from_slice(&0x4E4F_534A_u32.to_le_bytes());
    glb.extend_from_slice(&json_chunk);
    glb.extend_from_slice(&(binary_chunk.len() as u32).to_le_bytes());
    glb.extend_from_slice(&0x004E_4942_u32.to_le_bytes());
    glb.extend_from_slice(&binary_chunk);
    Ok((glb, json_chunk.len() as u64, binary_chunk.len() as u64))
}

fn metrics_from_document(
    gltf: &Value,
    encoded_bytes: u64,
    json_chunk_bytes: u64,
    bin_chunk_bytes: u64,
) -> AssetResult<AssetMetrics> {
    let nodes = gltf
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("nodes must be an array"))?;
    let meshes = gltf
        .get("meshes")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("meshes must be an array"))?;
    let materials = gltf
        .get("materials")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("materials must be an array"))?;
    let accessors = gltf
        .get("accessors")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("accessors must be an array"))?;
    let mut primitive_count = 0_u64;
    let mut triangle_count = 0_u64;
    let mut edge_segment_count = 0_u64;
    for mesh in meshes {
        let primitives = mesh
            .get("primitives")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("mesh primitives must be an array"))?;
        primitive_count += primitives.len() as u64;
        for primitive in primitives {
            let mode = primitive.get("mode").and_then(Value::as_u64).unwrap_or(4);
            let accessor = primitive
                .get("indices")
                .and_then(Value::as_u64)
                .and_then(|index| accessors.get(index as usize))
                .ok_or_else(|| invalid("primitive index accessor is missing"))?;
            let count = accessor
                .get("count")
                .and_then(Value::as_u64)
                .ok_or_else(|| invalid("primitive index count is missing"))?;
            if mode == 4 {
                triangle_count = triangle_count
                    .checked_add(count / 3)
                    .ok_or_else(|| invalid("triangle count overflowed"))?;
            } else if mode == 1 {
                edge_segment_count = edge_segment_count
                    .checked_add(count / 2)
                    .ok_or_else(|| invalid("edge count overflowed"))?;
            }
        }
    }
    let selection_group_count = nodes
        .iter()
        .filter_map(|node| node.pointer("/extras/ice3/selectionId"))
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>()
        .len() as u64;
    Ok(AssetMetrics {
        encoded_bytes,
        json_chunk_bytes,
        bin_chunk_bytes,
        node_count: nodes.len() as u64,
        max_node_depth: json_max_depth(gltf)? as u64,
        mesh_count: meshes.len() as u64,
        primitive_count,
        triangle_count,
        edge_segment_count,
        material_count: materials.len() as u64,
        selection_group_count,
    })
}

fn json_max_depth(gltf: &Value) -> AssetResult<usize> {
    fn visit(nodes: &[Value], index: usize, active: &mut BTreeSet<usize>) -> AssetResult<usize> {
        if !active.insert(index) {
            return Err(invalid("node hierarchy contains a cycle"));
        }
        let children = nodes
            .get(index)
            .and_then(|node| node.get("children"))
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let mut depth = 0;
        for child in children {
            let child = child
                .as_u64()
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(|| invalid("node child index is invalid"))?;
            depth = depth.max(1 + visit(nodes, child, active)?);
        }
        active.remove(&index);
        Ok(depth)
    }
    let nodes = gltf
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("nodes must be an array"))?;
    let roots = gltf
        .get("scenes")
        .and_then(Value::as_array)
        .and_then(|scenes| scenes.first())
        .and_then(|scene| scene.get("nodes"))
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("default scene roots are missing"))?;
    let mut depth = 0;
    for root in roots {
        let root = root
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| invalid("scene root index is invalid"))?;
        depth = depth.max(visit(nodes, root, &mut BTreeSet::new())?);
    }
    Ok(depth)
}

fn set_profile_stats(gltf: &mut Value, metrics: AssetMetrics) -> AssetResult<()> {
    let profile = profile_mut(gltf)?;
    profile.insert(
        "stats".into(),
        json!({
            "nodeCount": metrics.node_count,
            "maxNodeDepth": metrics.max_node_depth,
            "meshCount": metrics.mesh_count,
            "primitiveCount": metrics.primitive_count,
            "triangleCount": metrics.triangle_count,
            "edgeSegmentCount": metrics.edge_segment_count,
            "materialCount": metrics.material_count,
            "selectionGroupCount": metrics.selection_group_count
        }),
    );
    Ok(())
}

fn set_profile_budget(gltf: &mut Value, budget: &crate::BudgetResult) -> AssetResult<()> {
    let budget = serde_json::to_value(budget).map_err(|_| invalid("budget JSON failed"))?;
    profile_mut(gltf)?.insert("budget".into(), budget);
    Ok(())
}

fn set_profile_preparation(gltf: &mut Value, preparation: &PreparationMetadata) -> AssetResult<()> {
    profile_mut(gltf)?.insert(
        "preparation".into(),
        json!({
            "runtimeConversionRequired": preparation.runtime_conversion_required,
            "unitsNormalized": preparation.units_normalized,
            "matrixTransformsBaked": preparation.matrix_transforms_baked,
            "requiredCompressionDecoded": preparation.required_compression_decoded,
            "edgesPrepared": preparation.edges_prepared,
            "selectionGroupsPrepared": preparation.selection_groups_prepared,
            "creaseAngleDegrees": preparation.crease_angle_degrees,
            "options": {
                "creaseAngleDegrees": preparation.crease_angle_degrees,
                "edges": preparation.edges_prepared
            }
        }),
    );
    Ok(())
}

fn profile_mut(gltf: &mut Value) -> AssetResult<&mut Map<String, Value>> {
    gltf.pointer_mut("/asset/extras/ice3")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| invalid("Prepared profile is missing"))
}

fn preparation_metadata(options: &PrepareOptions) -> PreparationMetadata {
    PreparationMetadata {
        runtime_conversion_required: false,
        units_normalized: true,
        matrix_transforms_baked: false,
        required_compression_decoded: true,
        edges_prepared: options.generate_edges,
        selection_groups_prepared: true,
        crease_angle_degrees: options.crease_angle_degrees.clamp(0.0, 180.0),
    }
}

fn invalid(message: &str) -> AssetError {
    AssetError::ProfileInvalid(message.into())
}

fn unit_value(value: f32) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}
