use std::collections::{BTreeMap, BTreeSet};

use glam::Mat4;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use studio_assets::{AccessorData, SourceGlb, read_accessor};
use studio_core::geometry::Bounds;

use super::{PartitionError, PartitionLeaf, analyze_units};
use crate::PipelineError;
use crate::profile::AssetProfile;
use crate::tileset::{
    BuildOptions, PartitionArtifact, PartitionFile, PartitionStats, TilesetDocument,
};

#[derive(Debug, Clone, Copy)]
pub(crate) struct EdgeOptions {
    generate_edges: bool,
    crease_angle_degrees: f32,
}

impl EdgeOptions {
    const fn disabled() -> Self {
        Self {
            generate_edges: false,
            crease_angle_degrees: 30.0,
        }
    }

    pub(crate) fn from_profile(profile: &AssetProfile) -> Self {
        Self {
            generate_edges: profile.generate_edges,
            crease_angle_degrees: profile.crease_angle_degrees,
        }
    }
}

impl Default for EdgeOptions {
    fn default() -> Self {
        Self::disabled()
    }
}

pub(crate) fn build_artifact(
    source: &SourceGlb,
    options: &BuildOptions,
) -> Result<PartitionArtifact, PipelineError> {
    build_artifact_with_options(source, options, EdgeOptions::disabled())
}

pub(crate) fn build_artifact_with_profile(
    source: &SourceGlb,
    asset_id: &str,
    profile: &AssetProfile,
) -> Result<PartitionArtifact, PipelineError> {
    build_artifact_with_options(
        source,
        &BuildOptions {
            asset_id: asset_id.into(),
            max_triangles: profile.max_triangles,
            max_bytes: profile.max_bytes,
        },
        EdgeOptions::from_profile(profile),
    )
}

fn build_artifact_with_options(
    source: &SourceGlb,
    options: &BuildOptions,
    edge_options: EdgeOptions,
) -> Result<PartitionArtifact, PipelineError> {
    let asset_id = options.asset_id.trim();
    if asset_id.is_empty() {
        return Err(PipelineError::Tileset("assetId must not be empty".into()));
    }
    let units = analyze_units(source)?;
    let plan = super::try_create_partition_plan(&units, options.max_triangles)?;
    let mut files = Vec::with_capacity(plan.leaves.len());
    let mut selection_ids = BTreeSet::new();
    for (index, leaf) in plan.leaves.iter().enumerate() {
        let built = build_leaf(source, asset_id, leaf, edge_options)?;
        if built.bytes.len() > options.max_bytes {
            return Err(
                PartitionError::Invalid(format!("partition {} exceeds maxBytes", leaf.id)).into(),
            );
        }
        selection_ids.extend(built.selection_ids);
        files.push(PartitionFile {
            relative_path: format!("parts/part-{index:03}.glb"),
            content_hash: format!("{:x}", Sha256::digest(&built.bytes)),
            byte_length: built.bytes.len(),
            bytes: built.bytes,
        });
    }
    let tileset = TilesetDocument::from_plan(asset_id, &plan, &files)?;
    let triangle_count = files
        .iter()
        .map(part_triangle_count)
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .sum();
    let max_part_bytes = files
        .iter()
        .map(|file| file.byte_length as u64)
        .max()
        .unwrap_or(0);
    let (edge_segment_count, edge_bytes) = files
        .iter()
        .map(part_edge_metrics)
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .fold((0_u64, 0_u64), |(segments, bytes), next| {
            (segments + next.0, bytes + next.1)
        });
    Ok(PartitionArtifact {
        files,
        tileset,
        stats: PartitionStats {
            triangle_count,
            edge_segment_count,
            edge_bytes,
            source_triangle_count: plan.triangle_count,
            node_count: plan
                .leaves
                .iter()
                .flat_map(|leaf| leaf.units.iter())
                .map(|unit| unit.node_ids.len() as u64)
                .sum(),
            partition_count: plan.leaves.len(),
            max_part_bytes,
            render_batch_count: 0,
            coarse_count: 0,
        },
        selection_ids,
        plan,
    })
}

pub(crate) struct BuiltLeaf {
    pub(crate) bytes: Vec<u8>,
    pub(crate) selection_ids: BTreeSet<String>,
}

struct PendingEdge {
    mesh_index: usize,
    positions_accessor: usize,
    positions: Vec<[f32; 3]>,
    indices: Vec<u32>,
    source_material_key: Option<usize>,
    source_material_index: Option<usize>,
    edge_material_index: Option<usize>,
}

#[allow(clippy::too_many_arguments, dead_code)]
pub(crate) fn append_edges_to_primitive(
    json: &mut Value,
    binary: &mut Vec<u8>,
    target_mesh_index: usize,
    positions_accessor: usize,
    positions: &[[f32; 3]],
    indices: &[u32],
    material_index: usize,
    edge_material_index: usize,
    crease_angle_degrees: f32,
) -> Result<u32, PipelineError> {
    let materials = json
        .get("materials")
        .and_then(Value::as_array)
        .ok_or_else(|| PipelineError::Tileset("part materials are missing".into()))?;
    let source_extras = materials
        .get(material_index)
        .and_then(|material| material.get("extras"))
        .ok_or_else(|| PipelineError::Tileset("edge source material is missing".into()))?
        .clone();
    if studio_assets::material_suppresses_edges(&source_extras) {
        return Ok(0);
    }
    let edge_material = materials
        .get(edge_material_index)
        .ok_or_else(|| PipelineError::Tileset("edge material is missing".into()))?;
    if !edge_material
        .pointer("/extras/ice3/id")
        .and_then(Value::as_str)
        .is_some_and(|id| id.ends_with("/edges"))
    {
        return Err(PipelineError::Tileset(
            "edge primitive must use an edge material".into(),
        ));
    }
    let edge_color = studio_assets::edge_color_from_material(&source_extras)?;
    let edge_material = json
        .get_mut("materials")
        .and_then(Value::as_array_mut)
        .and_then(|materials| materials.get_mut(edge_material_index))
        .and_then(Value::as_object_mut)
        .ok_or_else(|| PipelineError::Tileset("edge material is missing".into()))?;
    edge_material.insert(
        "pbrMetallicRoughness".into(),
        json!({
            "baseColorFactor": edge_color,
            "metallicFactor": 0.0,
            "roughnessFactor": 1.0
        }),
    );
    edge_material.insert(
        "alphaMode".into(),
        Value::from(if edge_color[3] < 1.0 {
            "BLEND"
        } else {
            "OPAQUE"
        }),
    );
    let edge_indices =
        studio_assets::generate_edge_indices(positions, indices, crease_angle_degrees)?;
    if edge_indices.is_empty() {
        return Ok(0);
    }

    while !binary.len().is_multiple_of(4) {
        binary.push(0);
    }
    let (edge_bytes, component_type) = if positions.len() <= u16::MAX as usize {
        (
            edge_indices
                .iter()
                .flat_map(|index| (*index as u16).to_le_bytes())
                .collect::<Vec<_>>(),
            5123,
        )
    } else {
        (
            edge_indices
                .iter()
                .flat_map(|index| index.to_le_bytes())
                .collect::<Vec<_>>(),
            5125,
        )
    };
    let buffer_views = json
        .get_mut("bufferViews")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| PipelineError::Tileset("part bufferViews are missing".into()))?;
    let buffer_view_index = buffer_views.len();
    buffer_views.push(json!({
        "buffer": 0,
        "byteOffset": binary.len(),
        "byteLength": edge_bytes.len(),
        "target": 34963
    }));
    binary.extend_from_slice(&edge_bytes);

    let accessors = json
        .get_mut("accessors")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| PipelineError::Tileset("part accessors are missing".into()))?;
    let edge_accessor = accessors.len();
    accessors.push(json!({
        "bufferView": buffer_view_index,
        "componentType": component_type,
        "count": edge_indices.len(),
        "type": "SCALAR"
    }));

    let meshes = json
        .get_mut("meshes")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| PipelineError::Tileset("part meshes are missing".into()))?;
    let target_mesh = meshes
        .get_mut(target_mesh_index)
        .ok_or_else(|| PipelineError::Tileset("edge target mesh is missing".into()))?;
    let primitives = target_mesh
        .get_mut("primitives")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| PipelineError::Tileset("edge target primitives are missing".into()))?;
    primitives.push(json!({
        "attributes": {
            "POSITION": positions_accessor
        },
        "indices": edge_accessor,
        "mode": 1,
        "material": edge_material_index,
        "extras": {
            "ice3": {
                "role": "edges",
                "widthPx": 3
            }
        }
    }));
    u32::try_from(edge_indices.len() / 2)
        .map_err(|_| PipelineError::Tileset("edge segment count overflowed".into()))
}

pub(crate) fn build_leaf(
    source: &SourceGlb,
    asset_id: &str,
    leaf: &PartitionLeaf,
    edge_options: EdgeOptions,
) -> Result<BuiltLeaf, PipelineError> {
    build_leaf_with_overrides(source, asset_id, leaf, &BTreeMap::new(), edge_options)
}

pub(crate) fn build_batched_leaf(
    source: &SourceGlb,
    asset_id: &str,
    leaf: &PartitionLeaf,
    edge_options: EdgeOptions,
) -> Result<(BuiltLeaf, usize), PipelineError> {
    let source_nodes = source_nodes(source)?;
    let source_meshes = source
        .json
        .get("meshes")
        .and_then(Value::as_array)
        .ok_or_else(|| PipelineError::Tileset("source meshes are missing".into()))?;
    let source_materials = source
        .json
        .get("materials")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut writer = PartWriter {
        edge_options,
        ..PartWriter::default()
    };
    let mut material_map = BTreeMap::new();
    let mut batches = BTreeMap::<u64, BatchDraft>::new();
    let mut semantic_ids = Vec::new();
    let mut semantic_node_ids = Vec::new();
    let mut semantic_selection_ids = Vec::new();
    let mut selection_ids = BTreeSet::new();
    let mut nodes = Vec::new();

    for unit in &leaf.units {
        for (node_id, matrix) in unit.node_ids.iter().zip(&unit.world_matrices) {
            let (_, node) = source_nodes
                .get(node_id)
                .ok_or_else(|| PipelineError::Tileset(format!("node {node_id} is missing")))?;
            let mut extras = node
                .get("extras")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let mut ice3 = extras
                .get("ice3")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let selection_id = business_selection_id(&ice3, node_id, asset_id);
            let output_node_id = node_instance_id(asset_id, &leaf.id, node_id);
            ice3.insert("id".into(), Value::from(output_node_id.clone()));
            ice3.insert("selectionId".into(), Value::from(selection_id.clone()));
            extras.insert("ice3".into(), Value::Object(ice3));
            let semantic_index = semantic_ids.len() as u32;
            semantic_ids.push(output_node_id.clone());
            semantic_node_ids.push(output_node_id);
            semantic_selection_ids.push(selection_id.clone());
            selection_ids.insert(selection_id);
            nodes.push(json!({
                "name": node.get("name").and_then(Value::as_str).unwrap_or(node_id),
                "matrix": Mat4::IDENTITY.to_cols_array(),
                "extras": extras
            }));
            append_batch_geometry(
                source,
                source_meshes,
                unit.mesh_index,
                *matrix,
                semantic_index,
                &mut batches,
            )?;
        }
    }

    let mut batch_metadata = Vec::new();
    for (batch_index, batch) in batches.values().enumerate() {
        let (mesh, output_material, triangle_count, bounds) = writer.copy_batch(
            batch,
            &source_materials,
            &mut material_map,
            asset_id,
            batch_index,
        )?;
        let mesh_index = writer.meshes.len();
        writer.meshes.push(mesh);
        nodes.push(json!({
            "name": format!("render-batch-{batch_index:03}"),
            "matrix": Mat4::IDENTITY.to_cols_array(),
            "mesh": mesh_index,
            "extras": {
                "ice3": {
                    "id": format!(
                        "{asset_id}/render-batch-node/{}/{:03}",
                        leaf.id,
                        batch_index,
                    )
                }
            }
        }));
        batch_metadata.push(json!({
            "id": format!("batch-{batch_index:03}"),
            "mesh": mesh_index,
            "primitive": 0,
            "material": output_material,
            "mode": 4,
            "triangleCount": triangle_count,
            "bounds": bounds
        }));
    }
    let root_nodes = (0..nodes.len()).collect::<Vec<_>>();
    writer.root_nodes = root_nodes;
    let batch_count = batch_metadata.len();
    let batching = json!({
        "version": "1.0",
        "semanticIds": semantic_ids,
        "semanticNodeIds": semantic_node_ids,
        "semanticSelectionIds": semantic_selection_ids,
        "batches": batch_metadata
    });
    let triangle_count = batches
        .values()
        .map(|batch| batch.indices.len() as u64 / 3)
        .sum();
    let bytes = writer.finish(
        source,
        asset_id,
        leaf.bounds,
        nodes,
        triangle_count,
        batch_count as u64,
        selection_ids.len(),
        Some(batching),
    )?;
    Ok((
        BuiltLeaf {
            bytes,
            selection_ids,
        },
        batch_count,
    ))
}

struct BatchDraft {
    positions: Vec<f32>,
    normals: Vec<f32>,
    colors: Vec<f32>,
    has_colors: bool,
    indices: Vec<u32>,
    semantic_indices: Vec<u32>,
    material: Option<usize>,
    bounds_min: [f32; 3],
    bounds_max: [f32; 3],
}

fn append_batch_geometry(
    source: &SourceGlb,
    meshes: &[Value],
    mesh_index: usize,
    matrix: Mat4,
    semantic_index: u32,
    batches: &mut BTreeMap<u64, BatchDraft>,
) -> Result<(), PipelineError> {
    let mesh = meshes
        .get(mesh_index)
        .and_then(Value::as_object)
        .ok_or_else(|| PipelineError::Tileset("batch source mesh is missing".into()))?;
    let primitives = mesh
        .get("primitives")
        .and_then(Value::as_array)
        .ok_or_else(|| PipelineError::Tileset("batch primitives are missing".into()))?;
    for primitive in primitives {
        let primitive = primitive
            .as_object()
            .ok_or_else(|| PipelineError::Tileset("batch primitive is invalid".into()))?;
        if primitive.get("mode").and_then(Value::as_u64).unwrap_or(4) != 4 {
            continue;
        }
        let position_index = primitive
            .get("attributes")
            .and_then(Value::as_object)
            .and_then(|attributes| attributes.get("POSITION"))
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| PipelineError::Tileset("batch POSITION is missing".into()))?;
        let AccessorData::F32 {
            values: positions,
            components: 3,
        } = read_accessor(source, position_index)?
        else {
            return Err(PipelineError::Tileset(
                "batch POSITION must be VEC3 float".into(),
            ));
        };
        let normals = match primitive
            .get("attributes")
            .and_then(Value::as_object)
            .and_then(|attributes| attributes.get("NORMAL"))
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
        {
            Some(index) => match read_accessor(source, index)? {
                AccessorData::F32 {
                    values,
                    components: 3,
                } if values.len() == positions.len() => values,
                _ => vec![0.0; positions.len()],
            },
            None => vec![0.0; positions.len()],
        };
        let colors = match primitive
            .get("attributes")
            .and_then(Value::as_object)
            .and_then(|attributes| attributes.get("COLOR_0"))
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
        {
            Some(index) => match read_accessor(source, index)? {
                AccessorData::F32 {
                    values,
                    components: 3,
                } if values.len() == positions.len() => Some(values),
                _ => {
                    return Err(PipelineError::Tileset(
                        "batch COLOR_0 must be VEC3 float".into(),
                    ));
                }
            },
            None => None,
        };
        let indices = match primitive
            .get("indices")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
        {
            Some(index) => match read_accessor(source, index)? {
                AccessorData::U16 { values, .. } => {
                    values.into_iter().map(u32::from).collect::<Vec<_>>()
                }
                AccessorData::U32 { values, .. } => values,
                AccessorData::F32 { .. } => {
                    return Err(PipelineError::Tileset(
                        "batch indices must be integers".into(),
                    ));
                }
            },
            None => (0..positions.len() as u32 / 3).collect(),
        };
        let material = primitive
            .get("material")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok());
        let key = material.map_or(u64::MAX, |value| value as u64);
        let batch = batches.entry(key).or_insert_with(|| BatchDraft {
            positions: Vec::new(),
            normals: Vec::new(),
            colors: Vec::new(),
            has_colors: false,
            indices: Vec::new(),
            semantic_indices: Vec::new(),
            material,
            bounds_min: [f32::INFINITY; 3],
            bounds_max: [f32::NEG_INFINITY; 3],
        });
        let vertex_offset = batch.positions.len() as u32 / 3;
        let normal_matrix = glam::Mat3::from_mat4(matrix).inverse().transpose();
        for (point, normal) in positions
            .as_chunks::<3>()
            .0
            .iter()
            .zip(normals.as_chunks::<3>().0)
        {
            let transformed = matrix.transform_point3(glam::Vec3::from_array(*point));
            batch.positions.extend_from_slice(&transformed.to_array());
            let transformed_normal = if normal == &[0.0, 0.0, 0.0] {
                glam::Vec3::Z
            } else {
                (normal_matrix * glam::Vec3::from_array(*normal)).normalize_or_zero()
            };
            batch
                .normals
                .extend_from_slice(&transformed_normal.to_array());
            batch.semantic_indices.push(semantic_index);
            batch.bounds_min[0] = batch.bounds_min[0].min(transformed.x);
            batch.bounds_min[1] = batch.bounds_min[1].min(transformed.y);
            batch.bounds_min[2] = batch.bounds_min[2].min(transformed.z);
            batch.bounds_max[0] = batch.bounds_max[0].max(transformed.x);
            batch.bounds_max[1] = batch.bounds_max[1].max(transformed.y);
            batch.bounds_max[2] = batch.bounds_max[2].max(transformed.z);
        }
        if let Some(colors) = colors {
            if !batch.has_colors {
                batch.colors.resize(vertex_offset as usize * 3, 1.0);
                batch.has_colors = true;
            }
            batch.colors.extend(colors);
        } else if batch.has_colors {
            batch
                .colors
                .resize(batch.colors.len() + positions.len(), 1.0);
        }
        batch
            .indices
            .extend(indices.into_iter().map(|index| index + vertex_offset));
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub(crate) struct GeometryOverride {
    pub positions: Vec<f32>,
    pub indices: Vec<u32>,
    pub material_index: Option<usize>,
}

pub(crate) fn build_leaf_with_overrides(
    source: &SourceGlb,
    asset_id: &str,
    leaf: &PartitionLeaf,
    overrides: &BTreeMap<usize, GeometryOverride>,
    edge_options: EdgeOptions,
) -> Result<BuiltLeaf, PipelineError> {
    let source_nodes = source_nodes(source)?;
    let source_meshes = source
        .json
        .get("meshes")
        .and_then(Value::as_array)
        .ok_or_else(|| PipelineError::Tileset("source meshes are missing".into()))?;
    let source_materials = source
        .json
        .get("materials")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut output = PartWriter {
        edge_options,
        ..PartWriter::default()
    };
    let mut mesh_map = BTreeMap::new();
    let mut material_map = BTreeMap::new();
    let mut output_nodes = Vec::new();
    let mut selection_ids = BTreeSet::new();
    let mut primitive_count = 0_u64;
    let mut source_triangles = 0_u64;

    for unit in &leaf.units {
        let mesh = source_meshes
            .get(unit.mesh_index)
            .ok_or_else(|| PipelineError::Tileset("partition mesh is missing".into()))?;
        let output_mesh = if let Some(index) = mesh_map.get(&unit.mesh_index) {
            *index
        } else if let Some(geometry) = overrides.get(&unit.mesh_index) {
            let (mesh, triangles) = output.copy_override(
                geometry,
                &source_materials,
                &mut material_map,
                asset_id,
                unit.mesh_index,
            )?;
            let index = output.meshes.len();
            output.meshes.push(mesh);
            mesh_map.insert(unit.mesh_index, index);
            primitive_count += 1;
            source_triangles += triangles;
            index
        } else {
            let (mesh, primitives, triangles) = output.copy_mesh(
                source,
                mesh,
                unit.mesh_index,
                &source_materials,
                &mut material_map,
                asset_id,
            )?;
            let index = output.meshes.len();
            output.meshes.push(mesh);
            mesh_map.insert(unit.mesh_index, index);
            primitive_count += primitives;
            source_triangles += triangles;
            index
        };
        for (node_id, matrix) in unit.node_ids.iter().zip(&unit.world_matrices) {
            let (_source_index, node) = source_nodes
                .get(node_id)
                .ok_or_else(|| PipelineError::Tileset(format!("node {node_id} is missing")))?;
            let mut extras = node
                .get("extras")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let mut ice3 = extras
                .get("ice3")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let selection_id = business_selection_id(&ice3, node_id, asset_id);
            let output_node_id = node_instance_id(asset_id, &leaf.id, node_id);
            ice3.insert("id".into(), Value::from(output_node_id));
            ice3.insert("selectionId".into(), Value::from(selection_id.clone()));
            extras.insert("ice3".into(), Value::Object(ice3));
            selection_ids.insert(selection_id);
            output_nodes.push(json!({
                "name": node.get("name").and_then(Value::as_str).unwrap_or(node_id),
                "matrix": matrix.to_cols_array(),
                "mesh": output_mesh,
                "extras": extras
            }));
        }
    }
    let root_nodes = (0..output_nodes.len()).collect::<Vec<_>>();
    output.root_nodes = root_nodes;
    let bytes = output.finish(
        source,
        asset_id,
        leaf.bounds,
        output_nodes,
        source_triangles,
        primitive_count,
        selection_ids.len(),
        None,
    )?;
    Ok(BuiltLeaf {
        bytes,
        selection_ids,
    })
}

#[derive(Default)]
struct PartWriter {
    edge_options: EdgeOptions,
    binary: Vec<u8>,
    buffer_views: Vec<Value>,
    accessors: Vec<Value>,
    accessor_map: BTreeMap<usize, usize>,
    view_map: BTreeMap<usize, usize>,
    meshes: Vec<Value>,
    materials: Vec<Value>,
    pending_edges: Vec<PendingEdge>,
    edge_sources: Vec<usize>,
    edge_source_set: BTreeSet<usize>,
    edge_material_map: BTreeMap<usize, usize>,
    default_material: Option<usize>,
    default_edge_material: Option<usize>,
    vertex_colors_material: Option<usize>,
    root_nodes: Vec<usize>,
}

impl PartWriter {
    fn copy_batch(
        &mut self,
        batch: &BatchDraft,
        source_materials: &[Value],
        material_map: &mut BTreeMap<usize, usize>,
        asset_id: &str,
        batch_index: usize,
    ) -> Result<(Value, Option<usize>, u64, Value), PipelineError> {
        self.align();
        let position_view = self.buffer_views.len();
        let position_bytes = bytemuck::cast_slice(&batch.positions);
        self.buffer_views.push(json!({
            "buffer": 0,
            "byteOffset": self.binary.len(),
            "byteLength": position_bytes.len(),
            "target": 34962
        }));
        self.binary.extend_from_slice(position_bytes);
        let position_accessor = self.accessors.len();
        self.accessors.push(json!({
            "bufferView": position_view,
            "componentType": 5126,
            "count": batch.positions.len() / 3,
            "type": "VEC3",
            "min": batch.bounds_min,
            "max": batch.bounds_max
        }));
        self.align();
        let normal_view = self.buffer_views.len();
        let normal_bytes = bytemuck::cast_slice(&batch.normals);
        self.buffer_views.push(json!({
            "buffer": 0,
            "byteOffset": self.binary.len(),
            "byteLength": normal_bytes.len(),
            "target": 34962
        }));
        self.binary.extend_from_slice(normal_bytes);
        let normal_accessor = self.accessors.len();
        self.accessors.push(json!({
            "bufferView": normal_view,
            "componentType": 5126,
            "count": batch.normals.len() / 3,
            "type": "VEC3"
        }));
        let color_accessor = if batch.has_colors {
            self.align();
            let color_view = self.buffer_views.len();
            let color_bytes = bytemuck::cast_slice(&batch.colors);
            self.buffer_views.push(json!({
                "buffer": 0,
                "byteOffset": self.binary.len(),
                "byteLength": color_bytes.len(),
                "target": 34962
            }));
            self.binary.extend_from_slice(color_bytes);
            let accessor = self.accessors.len();
            self.accessors.push(json!({
                "bufferView": color_view,
                "componentType": 5126,
                "count": batch.colors.len() / 3,
                "type": "VEC3"
            }));
            Some(accessor)
        } else {
            None
        };
        self.align();
        let semantic_view = self.buffer_views.len();
        let semantic_bytes = bytemuck::cast_slice(&batch.semantic_indices);
        self.buffer_views.push(json!({
            "buffer": 0,
            "byteOffset": self.binary.len(),
            "byteLength": semantic_bytes.len(),
            "target": 34962
        }));
        self.binary.extend_from_slice(semantic_bytes);
        let semantic_accessor = self.accessors.len();
        self.accessors.push(json!({
            "bufferView": semantic_view,
            "componentType": 5125,
            "count": batch.semantic_indices.len(),
            "type": "SCALAR"
        }));
        self.align();
        let index_view = self.buffer_views.len();
        let index_bytes = bytemuck::cast_slice(&batch.indices);
        self.buffer_views.push(json!({
            "buffer": 0,
            "byteOffset": self.binary.len(),
            "byteLength": index_bytes.len(),
            "target": 34963
        }));
        self.binary.extend_from_slice(index_bytes);
        let index_accessor = self.accessors.len();
        self.accessors.push(json!({
            "bufferView": index_view,
            "componentType": 5125,
            "count": batch.indices.len(),
            "type": "SCALAR"
        }));
        let output_material = batch
            .material
            .map(|material| self.copy_material(source_materials, material_map, material, asset_id))
            .transpose()?;
        let mut primitive = json!({
            "attributes": {
                "POSITION": position_accessor,
                "NORMAL": normal_accessor,
                "_ICE3_SEMANTIC_INDEX": semantic_accessor
            },
            "indices": index_accessor,
            "mode": 4
        });
        if let Some(material) = output_material {
            primitive["material"] = Value::from(material);
        }
        if let Some(color_accessor) = color_accessor {
            primitive["attributes"]["COLOR_0"] = Value::from(color_accessor);
        }
        let (edge_source_material, queue_edges) =
            self.record_edge_material(source_materials, material_map, batch.material, asset_id)?;
        if self.edge_options.generate_edges && queue_edges {
            self.pending_edges.push(PendingEdge {
                mesh_index: self.meshes.len(),
                positions_accessor: position_accessor,
                positions: vec3_arrays(&batch.positions),
                indices: batch.indices.clone(),
                source_material_key: batch.material,
                source_material_index: edge_source_material,
                edge_material_index: None,
            });
        }
        Ok((
            json!({
                "name": format!("render-batch-{batch_index:03}"),
                "primitives": [primitive],
                "extras": {
                    "ice3": {
                        "id": format!("{asset_id}/render-batch-mesh/{batch_index:03}")
                    }
                }
            }),
            output_material,
            batch.indices.len() as u64 / 3,
            json!({
                "min": batch.bounds_min,
                "max": batch.bounds_max
            }),
        ))
    }

    fn copy_override(
        &mut self,
        geometry: &GeometryOverride,
        source_materials: &[Value],
        material_map: &mut BTreeMap<usize, usize>,
        asset_id: &str,
        source_mesh_index: usize,
    ) -> Result<(Value, u64), PipelineError> {
        if geometry.positions.is_empty()
            || !geometry.positions.len().is_multiple_of(3)
            || geometry.indices.is_empty()
            || !geometry.indices.len().is_multiple_of(3)
        {
            return Err(PipelineError::Tileset(
                "LOD geometry buffers are invalid".into(),
            ));
        }
        self.align();
        let position_view = self.buffer_views.len();
        let min =
            (0..geometry.positions.len())
                .step_by(3)
                .fold([f32::INFINITY; 3], |mut min, offset| {
                    min[0] = min[0].min(geometry.positions[offset]);
                    min[1] = min[1].min(geometry.positions[offset + 1]);
                    min[2] = min[2].min(geometry.positions[offset + 2]);
                    min
                });
        let max = (0..geometry.positions.len()).step_by(3).fold(
            [f32::NEG_INFINITY; 3],
            |mut max, offset| {
                max[0] = max[0].max(geometry.positions[offset]);
                max[1] = max[1].max(geometry.positions[offset + 1]);
                max[2] = max[2].max(geometry.positions[offset + 2]);
                max
            },
        );
        let position_bytes = bytemuck::cast_slice(&geometry.positions);
        self.buffer_views.push(json!({
            "buffer": 0,
            "byteOffset": self.binary.len(),
            "byteLength": position_bytes.len(),
            "target": 34962
        }));
        self.binary.extend_from_slice(position_bytes);
        let position_accessor = self.accessors.len();
        self.accessors.push(json!({
            "bufferView": position_view,
            "componentType": 5126,
            "count": geometry.positions.len() / 3,
            "type": "VEC3",
            "min": min,
            "max": max
        }));
        self.align();
        let normals = compute_normals(&geometry.positions, &geometry.indices);
        let normal_view = self.buffer_views.len();
        let normal_bytes = bytemuck::cast_slice(&normals);
        self.buffer_views.push(json!({
            "buffer": 0,
            "byteOffset": self.binary.len(),
            "byteLength": normal_bytes.len(),
            "target": 34962
        }));
        self.binary.extend_from_slice(normal_bytes);
        let normal_accessor = self.accessors.len();
        self.accessors.push(json!({
            "bufferView": normal_view,
            "componentType": 5126,
            "count": normals.len() / 3,
            "type": "VEC3"
        }));
        self.align();
        let index_view = self.buffer_views.len();
        let index_bytes = bytemuck::cast_slice(&geometry.indices);
        self.buffer_views.push(json!({
            "buffer": 0,
            "byteOffset": self.binary.len(),
            "byteLength": index_bytes.len(),
            "target": 34963
        }));
        self.binary.extend_from_slice(index_bytes);
        let index_accessor = self.accessors.len();
        self.accessors.push(json!({
            "bufferView": index_view,
            "componentType": 5125,
            "count": geometry.indices.len(),
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
        let output_material = geometry
            .material_index
            .map(|material| self.copy_material(source_materials, material_map, material, asset_id))
            .transpose()?;
        if let Some(output_material) = output_material {
            primitive["material"] = Value::from(output_material);
        }
        let (edge_source_material, queue_edges) = self.record_edge_material(
            source_materials,
            material_map,
            geometry.material_index,
            asset_id,
        )?;
        if self.edge_options.generate_edges && queue_edges {
            self.pending_edges.push(PendingEdge {
                mesh_index: self.meshes.len(),
                positions_accessor: position_accessor,
                positions: vec3_arrays(&geometry.positions),
                indices: geometry.indices.clone(),
                source_material_key: geometry.material_index,
                source_material_index: edge_source_material,
                edge_material_index: None,
            });
        }
        Ok((
            json!({
                "name": format!("LOD {source_mesh_index}"),
                "primitives": [primitive],
                "extras": {
                    "ice3": {
                        "id": format!("{asset_id}/lod-mesh/{source_mesh_index}"),
                        "sourceMeshId": source_mesh_index
                    }
                }
            }),
            geometry.indices.len() as u64 / 3,
        ))
    }

    fn record_edge_material(
        &mut self,
        source_materials: &[Value],
        material_map: &mut BTreeMap<usize, usize>,
        source_material_index: Option<usize>,
        asset_id: &str,
    ) -> Result<(Option<usize>, bool), PipelineError> {
        let Some(source_material_index) = source_material_index else {
            return Ok((None, true));
        };
        let source_material = source_materials
            .get(source_material_index)
            .ok_or_else(|| PipelineError::Tileset("source material is missing".into()))?;
        let base_material = self.copy_material(
            source_materials,
            material_map,
            source_material_index,
            asset_id,
        )?;
        if studio_assets::material_suppresses_edges(
            source_material.get("extras").unwrap_or(&Value::Null),
        ) {
            return Ok((Some(base_material), false));
        }
        if self.edge_source_set.insert(source_material_index) {
            self.edge_sources.push(source_material_index);
        }
        Ok((Some(base_material), true))
    }

    fn finalize_materials(
        &mut self,
        source_materials: &[Value],
        asset_id: &str,
    ) -> Result<(), PipelineError> {
        for source_material_index in self.edge_sources.clone() {
            let source_material = source_materials
                .get(source_material_index)
                .ok_or_else(|| PipelineError::Tileset("source material is missing".into()))?;
            let mut material = source_material
                .as_object()
                .cloned()
                .ok_or_else(|| PipelineError::Tileset("material is invalid".into()))?;
            material.insert(
                "name".into(),
                Value::from(format!(
                    "{} edges",
                    material
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("material")
                )),
            );
            material.insert(
                "extras".into(),
                json!({
                    "ice3": {
                        "id": format!("{asset_id}/material/{source_material_index}/edges")
                    }
                }),
            );
            let edge_color = studio_assets::edge_color_from_material(
                source_material.get("extras").unwrap_or(&Value::Null),
            )?;
            material.insert(
                "pbrMetallicRoughness".into(),
                json!({
                    "baseColorFactor": edge_color,
                    "metallicFactor": 0.0,
                    "roughnessFactor": 1.0
                }),
            );
            material.insert(
                "alphaMode".into(),
                Value::from(if edge_color[3] < 1.0 {
                    "BLEND"
                } else {
                    "OPAQUE"
                }),
            );
            let edge_material = self.materials.len();
            self.materials.push(Value::Object(material));
            self.edge_material_map
                .insert(source_material_index, edge_material);
        }
        let default_material = *self.default_material.get_or_insert_with(|| {
            let index = self.materials.len();
            self.materials.push(json!({
                "name": "default",
                "extras": {
                    "ice3": {
                            "id": format!("{asset_id}/material/__reserved__/default")
                    }
                }
            }));
            index
        });
        let default_edge_material = *self.default_edge_material.get_or_insert_with(|| {
            let index = self.materials.len();
            self.materials.push(json!({
                "name": "default edges",
                "extras": {
                    "ice3": {
                            "id": format!("{asset_id}/material/__reserved__/default/edges")
                    }
                }
            }));
            index
        });
        self.vertex_colors_material.get_or_insert_with(|| {
            let index = self.materials.len();
            self.materials.push(json!({
                "name": "vertex colors",
                "extras": {
                    "ice3": {
                            "id": format!("{asset_id}/material/__reserved__/vertex-colors")
                    }
                }
            }));
            index
        });
        for pending in &mut self.pending_edges {
            let base_material = pending.source_material_index.unwrap_or(default_material);
            pending.source_material_index = Some(base_material);
            pending.edge_material_index = Some(
                pending
                    .source_material_key
                    .and_then(|material| self.edge_material_map.get(&material).copied())
                    .unwrap_or(default_edge_material),
            );
        }
        Ok(())
    }

    fn copy_material(
        &mut self,
        source_materials: &[Value],
        material_map: &mut BTreeMap<usize, usize>,
        source_material_index: usize,
        asset_id: &str,
    ) -> Result<usize, PipelineError> {
        if let Some(index) = material_map.get(&source_material_index) {
            return Ok(*index);
        }
        let source_material = source_materials
            .get(source_material_index)
            .ok_or_else(|| PipelineError::Tileset("source material is missing".into()))?;
        let mut material = source_material
            .as_object()
            .cloned()
            .ok_or_else(|| PipelineError::Tileset("material is invalid".into()))?;
        let mut extras = material
            .get("extras")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let mut ice3 = extras
            .get("ice3")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        ice3.entry("id")
            .or_insert_with(|| Value::from(format!("{asset_id}/material/{source_material_index}")));
        extras.insert("ice3".into(), Value::Object(ice3));
        material.insert("extras".into(), Value::Object(extras));
        let index = self.materials.len();
        self.materials.push(Value::Object(material));
        material_map.insert(source_material_index, index);
        Ok(index)
    }

    fn copy_mesh(
        &mut self,
        source: &SourceGlb,
        source_mesh: &Value,
        source_mesh_index: usize,
        source_materials: &[Value],
        material_map: &mut BTreeMap<usize, usize>,
        asset_id: &str,
    ) -> Result<(Value, u64, u64), PipelineError> {
        let output_mesh_index = self.meshes.len();
        let mut mesh = source_mesh
            .as_object()
            .cloned()
            .ok_or_else(|| PipelineError::Tileset("source mesh is invalid".into()))?;
        let input_primitives = mesh
            .get("primitives")
            .and_then(Value::as_array)
            .cloned()
            .ok_or_else(|| PipelineError::Tileset("mesh primitives are missing".into()))?;
        let mut primitives = Vec::new();
        let mut primitive_count = 0_u64;
        let mut triangles = 0_u64;
        for primitive in input_primitives {
            let mut primitive = primitive
                .as_object()
                .cloned()
                .ok_or_else(|| PipelineError::Tileset("primitive is invalid".into()))?;
            let mode = primitive.get("mode").and_then(Value::as_u64).unwrap_or(4);
            if !matches!(mode, 4..=6) {
                continue;
            }
            let source_triangle_count = primitive_triangles(source, &primitive)?;
            let attributes = primitive
                .get("attributes")
                .and_then(Value::as_object)
                .ok_or_else(|| PipelineError::Tileset("primitive attributes are missing".into()))?;
            let source_position_index = attributes
                .get("POSITION")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(|| PipelineError::Tileset("POSITION accessor is missing".into()))?;
            let source_indices = if self.edge_options.generate_edges {
                Some(primitive_triangle_list(source, &primitive)?)
            } else {
                None
            };
            let mut output_attributes = Map::new();
            for (semantic, value) in attributes {
                let old = value
                    .as_u64()
                    .and_then(|value| usize::try_from(value).ok())
                    .ok_or_else(|| PipelineError::Tileset("accessor index is invalid".into()))?;
                output_attributes.insert(
                    semantic.clone(),
                    Value::from(self.copy_accessor(source, old)?.to_string()),
                );
            }
            let output_attributes = output_attributes
                .into_iter()
                .map(|(key, value)| {
                    let index = value
                        .as_str()
                        .and_then(|value| value.parse::<usize>().ok())
                        .ok_or_else(|| PipelineError::Tileset("accessor remap failed".into()))?;
                    Ok((key, Value::from(index)))
                })
                .collect::<Result<Map<_, _>, PipelineError>>()?;
            let output_position_index = output_attributes
                .get("POSITION")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(|| PipelineError::Tileset("POSITION accessor remap failed".into()))?;
            primitive.insert("attributes".into(), Value::Object(output_attributes));
            if let Some(index) = primitive.get("indices").cloned() {
                let old = index
                    .as_u64()
                    .and_then(|value| usize::try_from(value).ok())
                    .ok_or_else(|| PipelineError::Tileset("index accessor is invalid".into()))?;
                primitive.insert(
                    "indices".into(),
                    Value::from(self.copy_accessor(source, old)?),
                );
            }
            triangles += source_triangle_count;
            let source_material_index = primitive
                .get("material")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok());
            if let Some(source_material_index) = source_material_index {
                let output_material = self.copy_material(
                    source_materials,
                    material_map,
                    source_material_index,
                    asset_id,
                )?;
                primitive.insert("material".into(), Value::from(output_material));
            } else if !self.materials.is_empty() {
                primitive.remove("material");
            }
            let (edge_source_material, queue_edges) = self.record_edge_material(
                source_materials,
                material_map,
                source_material_index,
                asset_id,
            )?;
            if self.edge_options.generate_edges
                && queue_edges
                && let Some(indices) = source_indices
            {
                let positions = read_positions(source, source_position_index)?;
                self.pending_edges.push(PendingEdge {
                    mesh_index: output_mesh_index,
                    positions_accessor: output_position_index,
                    positions,
                    indices,
                    source_material_key: source_material_index,
                    source_material_index: edge_source_material,
                    edge_material_index: None,
                });
            }
            primitive_count += 1;
            primitives.push(Value::Object(primitive));
        }
        mesh.insert("primitives".into(), Value::Array(primitives));
        let mut extras = mesh
            .get("extras")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let mut ice3 = extras
            .get("ice3")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        ice3.entry("id")
            .or_insert_with(|| Value::from(format!("{asset_id}/mesh/{source_mesh_index}")));
        extras.insert("ice3".into(), Value::Object(ice3));
        mesh.insert("extras".into(), Value::Object(extras));
        Ok((Value::Object(mesh), primitive_count, triangles))
    }

    fn copy_accessor(&mut self, source: &SourceGlb, old: usize) -> Result<usize, PipelineError> {
        if let Some(index) = self.accessor_map.get(&old) {
            return Ok(*index);
        }
        let source_accessor = source
            .json
            .get("accessors")
            .and_then(Value::as_array)
            .and_then(|accessors| accessors.get(old))
            .and_then(Value::as_object)
            .cloned()
            .ok_or_else(|| PipelineError::Tileset("accessor is missing".into()))?;
        let view = source_accessor
            .get("bufferView")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| PipelineError::Tileset("accessor has no bufferView".into()))?;
        let new_view = self.copy_buffer_view(source, view)?;
        let mut accessor = source_accessor;
        accessor.insert("bufferView".into(), Value::from(new_view));
        let index = self.accessors.len();
        self.accessors.push(Value::Object(accessor));
        self.accessor_map.insert(old, index);
        Ok(index)
    }

    fn copy_buffer_view(&mut self, source: &SourceGlb, old: usize) -> Result<usize, PipelineError> {
        if let Some(index) = self.view_map.get(&old) {
            return Ok(*index);
        }
        let view = source
            .json
            .get("bufferViews")
            .and_then(Value::as_array)
            .and_then(|views| views.get(old))
            .and_then(Value::as_object)
            .cloned()
            .ok_or_else(|| PipelineError::Tileset("bufferView is missing".into()))?;
        let buffer_index = view
            .get("buffer")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| PipelineError::Tileset("bufferView buffer is missing".into()))?;
        let offset = view.get("byteOffset").and_then(Value::as_u64).unwrap_or(0) as usize;
        let length = view
            .get("byteLength")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| PipelineError::Tileset("bufferView length is invalid".into()))?;
        let data = source
            .buffers
            .get(buffer_index)
            .and_then(|buffer| buffer.get(offset..offset.checked_add(length)?))
            .ok_or_else(|| PipelineError::Tileset("bufferView exceeds its buffer".into()))?;
        self.align();
        let new_offset = self.binary.len();
        self.binary.extend_from_slice(data);
        let mut view = view;
        view.insert("buffer".into(), Value::from(0));
        view.insert("byteOffset".into(), Value::from(new_offset));
        view.insert("byteLength".into(), Value::from(length));
        let index = self.buffer_views.len();
        self.buffer_views.push(Value::Object(view));
        self.view_map.insert(old, index);
        Ok(index)
    }

    fn align(&mut self) {
        while !self.binary.len().is_multiple_of(4) {
            self.binary.push(0);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn finish(
        mut self,
        source: &SourceGlb,
        asset_id: &str,
        bounds: Bounds,
        nodes: Vec<Value>,
        triangle_count: u64,
        primitive_count: u64,
        selection_group_count: usize,
        batching: Option<Value>,
    ) -> Result<Vec<u8>, PipelineError> {
        self.align();
        let source_materials = source
            .json
            .get("materials")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        self.finalize_materials(&source_materials, asset_id)?;
        let project_extras = source.json.get("extras").cloned().unwrap_or(Value::Null);
        let mut json = json!({
            "asset": {
                "version": "2.0",
                "generator": "ICE3 Studio",
                "extras": {
                    "ice3": {
                        "profile": "ice3-prepared",
                        "profileVersion": "1.0",
                        "contractVersion": "1.0",
                        "assetId": asset_id,
                        "bounds": bounds_json(bounds),
                        "contentDigest": {
                            "algorithm": "sha256",
                            "value": "",
                            "covers": "binary-chunk"
                        },
                        "preparation": {
                            "runtimeConversionRequired": false,
                            "sourceFormat": "gltf-2.0",
                            "unitsNormalized": true,
                            "matrixTransformsBaked": true,
                            "requiredCompressionDecoded": true,
                            "edgesPrepared": self.edge_options.generate_edges,
                            "selectionGroupsPrepared": true
                        },
                        "stats": {
                            "nodeCount": nodes.len(),
                            "maxNodeDepth": 0,
                            "meshCount": self.meshes.len(),
                            "primitiveCount": primitive_count,
                            "triangleCount": triangle_count,
                            "edgeSegmentCount": 0,
                            "materialCount": self.materials.len(),
                            "selectionGroupCount": selection_group_count
                        },
                        "budget": {
                            "profile": "interactive-desktop-v1",
                            "status": "pass",
                            "warningCount": 0,
                            "errorCount": 0
                        }
                    }
                }
            },
            "scene": 0,
            "scenes": [{"nodes": self.root_nodes}],
            "nodes": nodes,
            "meshes": self.meshes,
            "materials": self.materials,
            "accessors": self.accessors,
            "bufferViews": self.buffer_views,
            "buffers": [{"byteLength": self.binary.len()}],
            "extras": project_extras
        });
        // Texture tables are global and index-stable, so every tile that uses a
        // textured material carries the same arrays verbatim. The image files
        // themselves are copied next to the tileset by the exporter.
        for field in ["images", "samplers", "textures", "extensionsUsed"] {
            if let Some(value) = source.json.get(field)
                && value.as_array().is_none_or(|values| !values.is_empty())
            {
                json[field] = value.clone();
            }
        }
        let mut edge_segment_count = 0_u64;
        let mut edge_bytes = 0_u64;
        if self.edge_options.generate_edges {
            for pending in &self.pending_edges {
                let segments = append_edges_to_primitive(
                    &mut json,
                    &mut self.binary,
                    pending.mesh_index,
                    pending.positions_accessor,
                    &pending.positions,
                    &pending.indices,
                    pending.source_material_index.unwrap_or(0),
                    pending
                        .edge_material_index
                        .ok_or_else(|| PipelineError::Tileset("edge material is missing".into()))?,
                    self.edge_options.crease_angle_degrees,
                )?;
                if segments > 0 {
                    edge_segment_count += u64::from(segments);
                    edge_bytes += json["bufferViews"]
                        .as_array()
                        .and_then(|views| views.last())
                        .and_then(|view| view.get("byteLength"))
                        .and_then(Value::as_u64)
                        .ok_or_else(|| {
                            PipelineError::Tileset("edge buffer view is missing".into())
                        })?;
                }
            }
        }
        self.align();
        let digest = format!("{:x}", Sha256::digest(&self.binary));
        let final_primitive_count = json["meshes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|mesh| mesh["primitives"].as_array())
            .map(|primitives| primitives.len() as u64)
            .sum::<u64>();
        json["asset"]["extras"]["ice3"]["contentDigest"]["value"] = Value::from(digest);
        json["asset"]["extras"]["ice3"]["stats"]["primitiveCount"] =
            Value::from(final_primitive_count);
        json["asset"]["extras"]["ice3"]["stats"]["edgeSegmentCount"] =
            Value::from(edge_segment_count);
        json["asset"]["extras"]["ice3"]["stats"]["edgeBytes"] = Value::from(edge_bytes);
        if let Some(buffer) = json["buffers"].get_mut(0) {
            buffer["byteLength"] = Value::from(self.binary.len());
        }
        if self.binary.is_empty() {
            json.as_object_mut().expect("object").remove("buffers");
        }
        if let Some(batching) = batching {
            json["asset"]["extras"]["ice3"]["batching"] = batching;
        }
        pack_glb(
            &serde_json::to_vec(&json)
                .map_err(|error| PipelineError::Tileset(format!("part JSON failed: {error}")))?,
            &self.binary,
        )
    }
}

fn compute_normals(positions: &[f32], indices: &[u32]) -> Vec<f32> {
    let mut normals = vec![glam::Vec3::ZERO; positions.len() / 3];
    for triangle in indices.as_chunks::<3>().0 {
        let [a, b, c] = triangle.map(|index| {
            let offset = index as usize * 3;
            glam::Vec3::new(
                positions[offset],
                positions[offset + 1],
                positions[offset + 2],
            )
        });
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
        .flat_map(|normal| {
            if normal.length_squared() <= f32::EPSILON {
                glam::Vec3::Z.to_array()
            } else {
                normal.normalize().to_array()
            }
        })
        .collect()
}

fn vec3_arrays(values: &[f32]) -> Vec<[f32; 3]> {
    values
        .as_chunks::<3>()
        .0
        .iter()
        .map(|point| [point[0], point[1], point[2]])
        .collect()
}

fn read_positions(source: &SourceGlb, accessor: usize) -> Result<Vec<[f32; 3]>, PipelineError> {
    let AccessorData::F32 {
        values,
        components: 3,
    } = read_accessor(source, accessor)?
    else {
        return Err(PipelineError::Tileset(
            "triangle POSITION must be VEC3 float".into(),
        ));
    };
    Ok(vec3_arrays(&values))
}

fn primitive_triangle_list(
    source: &SourceGlb,
    primitive: &Map<String, Value>,
) -> Result<Vec<u32>, PipelineError> {
    let source_indices = match primitive
        .get("indices")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
    {
        Some(index) => match read_accessor(source, index)? {
            AccessorData::U16 { values, .. } => {
                values.into_iter().map(u32::from).collect::<Vec<_>>()
            }
            AccessorData::U32 { values, .. } => values,
            AccessorData::F32 { .. } => {
                return Err(PipelineError::Tileset(
                    "triangle indices must be integers".into(),
                ));
            }
        },
        None => {
            let position = primitive
                .get("attributes")
                .and_then(Value::as_object)
                .and_then(|attributes| attributes.get("POSITION"))
                .and_then(Value::as_u64)
                .ok_or_else(|| PipelineError::Tileset("POSITION accessor is missing".into()))?;
            let count = source.json["accessors"][position as usize]["count"]
                .as_u64()
                .ok_or_else(|| PipelineError::Tileset("position count is missing".into()))?;
            (0..count as u32).collect()
        }
    };
    let mode = primitive.get("mode").and_then(Value::as_u64).unwrap_or(4);
    let output = match mode {
        4 if source_indices.len().is_multiple_of(3) => source_indices,
        5 if source_indices.len() >= 3 => source_indices
            .windows(3)
            .enumerate()
            .flat_map(|(index, triangle)| {
                if index.is_multiple_of(2) {
                    [triangle[0], triangle[1], triangle[2]]
                } else {
                    [triangle[1], triangle[0], triangle[2]]
                }
            })
            .collect(),
        6 if source_indices.len() >= 3 => source_indices[1..]
            .windows(2)
            .flat_map(|edge| [source_indices[0], edge[0], edge[1]])
            .collect(),
        _ => {
            return Err(PipelineError::Tileset(
                "triangle primitive indices are invalid".into(),
            ));
        }
    };
    Ok(output)
}

type SourceNodeMap = BTreeMap<String, (usize, Map<String, Value>)>;

fn source_nodes(source: &SourceGlb) -> Result<SourceNodeMap, PipelineError> {
    let nodes = source
        .json
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or_else(|| PipelineError::Tileset("source nodes are missing".into()))?;
    let mut output = BTreeMap::new();
    for (index, node) in nodes.iter().enumerate() {
        let node = node
            .as_object()
            .ok_or_else(|| PipelineError::Tileset("source node is invalid".into()))?;
        let id = node
            .get("extras")
            .and_then(Value::as_object)
            .and_then(|extras| extras.get("ice3"))
            .and_then(Value::as_object)
            .and_then(|ice3| ice3.get("id"))
            .and_then(Value::as_str)
            .or_else(|| node.get("name").and_then(Value::as_str))
            .ok_or_else(|| PipelineError::Tileset("source node id is missing".into()))?;
        if output
            .insert(id.to_owned(), (index, node.clone()))
            .is_some()
        {
            return Err(PipelineError::Tileset(format!(
                "source node id {id} is duplicated"
            )));
        }
    }
    Ok(output)
}

fn business_selection_id(ice3: &Map<String, Value>, node_id: &str, asset_id: &str) -> String {
    let explicit = ice3.get("selectionId").and_then(Value::as_str);
    if explicit.is_some_and(|value| value.contains("/equipment/")) {
        return explicit.unwrap().to_owned();
    }
    let tag = ice3
        .get("metadata")
        .and_then(Value::as_object)
        .and_then(|metadata| metadata.get("tag").or_else(|| metadata.get("equipmentTag")))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty());
    if let Some(tag) = tag {
        let tag = tag
            .rsplit_once("/equipment/")
            .map_or(tag, |(_, value)| value)
            .trim_matches('/');
        return format!("{asset_id}/equipment/{tag}");
    }
    explicit
        .or_else(|| ice3.get("ownerId").and_then(Value::as_str))
        .or_else(|| ice3.get("id").and_then(Value::as_str))
        .map(str::to_owned)
        .unwrap_or_else(|| node_id.to_owned())
}

fn node_instance_id(asset_id: &str, leaf_id: &str, node_id: &str) -> String {
    format!("{asset_id}/node/{leaf_id}/{node_id}")
}

fn primitive_triangles(
    source: &SourceGlb,
    primitive: &Map<String, Value>,
) -> Result<u64, PipelineError> {
    let mode = primitive.get("mode").and_then(Value::as_u64).unwrap_or(4);
    let count = if let Some(index) = primitive.get("indices").and_then(Value::as_u64) {
        source.json["accessors"][index as usize]["count"]
            .as_u64()
            .ok_or_else(|| PipelineError::Tileset("index count is missing".into()))?
    } else {
        let position = primitive
            .get("attributes")
            .and_then(Value::as_object)
            .and_then(|attributes| attributes.get("POSITION"))
            .and_then(Value::as_u64)
            .ok_or_else(|| PipelineError::Tileset("POSITION accessor is missing".into()))?;
        source.json["accessors"][position as usize]["count"]
            .as_u64()
            .ok_or_else(|| PipelineError::Tileset("position count is missing".into()))?
    };
    Ok(match mode {
        4 if count.is_multiple_of(3) => count / 3,
        5 => count.saturating_sub(2),
        6 if count >= 3 => count - 2,
        _ => 0,
    })
}

fn bounds_json(bounds: Bounds) -> Value {
    json!({
        "min": [bounds.min.x, bounds.min.y, bounds.min.z],
        "max": [bounds.max.x, bounds.max.y, bounds.max.z]
    })
}

fn pack_glb(json: &[u8], binary: &[u8]) -> Result<Vec<u8>, PipelineError> {
    let mut json_chunk = json.to_vec();
    while !json_chunk.len().is_multiple_of(4) {
        json_chunk.push(b' ');
    }
    let mut binary_chunk = binary.to_vec();
    while !binary_chunk.len().is_multiple_of(4) {
        binary_chunk.push(0);
    }
    let total = 12usize
        .checked_add(8)
        .and_then(|value| value.checked_add(json_chunk.len()))
        .and_then(|value| value.checked_add(8))
        .and_then(|value| value.checked_add(binary_chunk.len()))
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| PipelineError::Tileset("GLB exceeds 4 GiB".into()))?;
    let mut output = Vec::with_capacity(total as usize);
    output.extend_from_slice(&0x4654_6C67_u32.to_le_bytes());
    output.extend_from_slice(&2_u32.to_le_bytes());
    output.extend_from_slice(&total.to_le_bytes());
    output.extend_from_slice(&(json_chunk.len() as u32).to_le_bytes());
    output.extend_from_slice(&0x4E4F_534A_u32.to_le_bytes());
    output.extend_from_slice(&json_chunk);
    output.extend_from_slice(&(binary_chunk.len() as u32).to_le_bytes());
    output.extend_from_slice(&0x004E_4942_u32.to_le_bytes());
    output.extend_from_slice(&binary_chunk);
    Ok(output)
}

fn part_triangle_count(file: &PartitionFile) -> Result<u64, PipelineError> {
    let source = studio_assets::read_glb_bytes(&file.bytes, &std::env::temp_dir())?;
    let meshes = source
        .json
        .get("meshes")
        .and_then(Value::as_array)
        .ok_or_else(|| PipelineError::Tileset("part meshes are missing".into()))?;
    let mut total = 0_u64;
    for mesh in meshes {
        for primitive in mesh
            .get("primitives")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let primitive = primitive
                .as_object()
                .ok_or_else(|| PipelineError::Tileset("part primitive is invalid".into()))?;
            total += primitive_triangles(&source, primitive)?;
        }
    }
    Ok(total)
}

pub(crate) fn part_edge_metrics(file: &PartitionFile) -> Result<(u64, u64), PipelineError> {
    let source = studio_assets::read_glb_bytes(&file.bytes, &std::env::temp_dir())?;
    let accessors = source
        .json
        .get("accessors")
        .and_then(Value::as_array)
        .ok_or_else(|| PipelineError::Tileset("part accessors are missing".into()))?;
    let buffer_views = source
        .json
        .get("bufferViews")
        .and_then(Value::as_array)
        .ok_or_else(|| PipelineError::Tileset("part bufferViews are missing".into()))?;
    let mut segment_count = 0_u64;
    let mut edge_bytes = 0_u64;
    for primitive in source
        .json
        .get("meshes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|mesh| {
            mesh.get("primitives")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
    {
        if primitive.get("mode").and_then(Value::as_u64) != Some(1)
            || primitive
                .pointer("/extras/ice3/role")
                .and_then(Value::as_str)
                != Some("edges")
        {
            continue;
        }
        let index = primitive
            .get("indices")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| PipelineError::Tileset("edge indices are missing".into()))?;
        let accessor = accessors
            .get(index)
            .ok_or_else(|| PipelineError::Tileset("edge accessor is missing".into()))?;
        let count = accessor
            .get("count")
            .and_then(Value::as_u64)
            .ok_or_else(|| PipelineError::Tileset("edge count is missing".into()))?;
        let view_index = accessor
            .get("bufferView")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| PipelineError::Tileset("edge bufferView is missing".into()))?;
        let byte_length = buffer_views
            .get(view_index)
            .and_then(|view| view.get("byteLength"))
            .and_then(Value::as_u64)
            .ok_or_else(|| PipelineError::Tileset("edge byte length is missing".into()))?;
        segment_count += count / 2;
        edge_bytes += byte_length;
    }
    Ok((segment_count, edge_bytes))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde_json::{Value, json};

    use super::append_edges_to_primitive;

    fn edge_fixture(meshes: Value, materials: Option<Value>) -> Value {
        json!({
            "materials": materials.unwrap_or_else(|| json!([
                {
                    "name": "concrete",
                    "extras": {"ice3": {}}
                },
                {
                    "name": "concrete edges",
                    "extras": {"ice3": {"id": "test/materials/concrete/edges"}}
                }
            ])),
            "meshes": meshes,
            "bufferViews": [],
            "accessors": [{
                "componentType": 5126,
                "count": 3,
                "type": "VEC3"
            }]
        })
    }

    fn triangle_mesh() -> Value {
        json!([{
            "primitives": [{
                "attributes": {"POSITION": 0}
            }]
        }])
    }

    fn triangle_positions() -> Vec<[f32; 3]> {
        vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]
    }

    fn edge_accessor(json: &Value) -> &Value {
        let accessors = json["accessors"].as_array().unwrap();
        &accessors[accessors.len() - 1]
    }

    fn edge_bytes<'a>(json: &Value, binary: &'a [u8]) -> &'a [u8] {
        let accessor = edge_accessor(json);
        let view_index = accessor["bufferView"].as_u64().unwrap() as usize;
        let view = &json["bufferViews"][view_index];
        let offset = view["byteOffset"].as_u64().unwrap() as usize;
        let length = view["byteLength"].as_u64().unwrap() as usize;
        &binary[offset..offset + length]
    }

    fn decoded_u16_edges(json: &Value, binary: &[u8]) -> BTreeSet<(u16, u16)> {
        edge_bytes(json, binary)
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pair| {
                let edge = [
                    u16::from_le_bytes([pair[0], pair[1]]),
                    u16::from_le_bytes([pair[2], pair[3]]),
                ];
                if edge[0] <= edge[1] {
                    (edge[0], edge[1])
                } else {
                    (edge[1], edge[0])
                }
            })
            .collect()
    }

    fn decoded_u32_edges(json: &Value, binary: &[u8]) -> BTreeSet<(u32, u32)> {
        edge_bytes(json, binary)
            .as_chunks::<8>()
            .0
            .iter()
            .map(|pair| {
                let edge = [
                    u32::from_le_bytes(pair[..4].try_into().unwrap()),
                    u32::from_le_bytes(pair[4..].try_into().unwrap()),
                ];
                if edge[0] <= edge[1] {
                    (edge[0], edge[1])
                } else {
                    (edge[1], edge[0])
                }
            })
            .collect()
    }

    #[test]
    fn writes_u16_edges_for_small_meshes() {
        let positions = triangle_positions();
        let mut json = edge_fixture(triangle_mesh(), None);
        let mut binary = Vec::new();

        let segments = append_edges_to_primitive(
            &mut json,
            &mut binary,
            0,
            0,
            &positions,
            &[0, 1, 2],
            0,
            1,
            30.0,
        )
        .unwrap();

        assert_eq!(segments, 3);
        assert_eq!(edge_accessor(&json)["componentType"], 5123);
        assert_eq!(edge_accessor(&json)["count"], 6);
        assert_eq!(
            decoded_u16_edges(&json, &binary),
            BTreeSet::from([(0, 1), (0, 2), (1, 2)])
        );
    }

    #[test]
    fn writes_u32_edges_above_u16_vertex_limit() {
        let mut positions = vec![[0.0, 0.0, 0.0]; u16::MAX as usize + 1];
        positions[1] = [1.0, 0.0, 0.0];
        positions[u16::MAX as usize] = [0.0, 1.0, 0.0];
        let mut json = edge_fixture(triangle_mesh(), None);
        let mut binary = Vec::new();

        let segments = append_edges_to_primitive(
            &mut json,
            &mut binary,
            0,
            0,
            &positions,
            &[0, 1, u16::MAX as u32],
            0,
            1,
            30.0,
        )
        .unwrap();

        assert_eq!(segments, 3);
        assert_eq!(edge_accessor(&json)["componentType"], 5125);
        assert_eq!(edge_accessor(&json)["count"], 6);
        assert_eq!(
            decoded_u32_edges(&json, &binary),
            BTreeSet::from([(0, 1), (0, 65_535), (1, 65_535)])
        );
    }

    #[test]
    fn writes_custom_edge_material_role_width_and_color() {
        let positions = triangle_positions();
        let materials = json!([
            {
                "name": "steel",
                "extras": {"ice3": {"edgeColor": "#11223380"}}
            },
            {
                "name": "steel edges",
                "extras": {"ice3": {"id": "test/materials/steel/edges"}}
            }
        ]);
        let mut json = edge_fixture(triangle_mesh(), Some(materials));
        let mut binary = Vec::new();

        append_edges_to_primitive(
            &mut json,
            &mut binary,
            0,
            0,
            &positions,
            &[0, 1, 2],
            0,
            1,
            30.0,
        )
        .unwrap();

        let edge_primitive = &json["meshes"][0]["primitives"][1];
        assert_eq!(edge_primitive["material"], 1);
        assert_eq!(edge_primitive["extras"]["ice3"]["role"], "edges");
        assert_eq!(edge_primitive["extras"]["ice3"]["widthPx"], 3);
        let color = json["materials"][1]["pbrMetallicRoughness"]["baseColorFactor"]
            .as_array()
            .unwrap();
        let actual = color
            .iter()
            .map(|value| value.as_f64().unwrap())
            .collect::<Vec<_>>();
        let expected = [17.0 / 255.0, 34.0 / 255.0, 51.0 / 255.0, 128.0 / 255.0];
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert!((actual - expected).abs() < 1e-6);
        }
        assert_eq!(json["materials"][1]["alphaMode"], "BLEND");
    }

    #[test]
    fn suppressed_material_does_not_write_edges() {
        let positions = triangle_positions();
        let materials = json!([
            {
                "name": "glass",
                "extras": {"ice3": {"edges": false}}
            },
            {
                "name": "glass edges",
                "extras": {"ice3": {"id": "test/materials/glass/edges"}}
            }
        ]);
        let mut json = edge_fixture(triangle_mesh(), Some(materials));
        let original = json.clone();
        let mut binary = Vec::new();

        let segments = append_edges_to_primitive(
            &mut json,
            &mut binary,
            0,
            0,
            &positions,
            &[0, 1, 2],
            0,
            1,
            30.0,
        )
        .unwrap();

        assert_eq!(segments, 0);
        assert!(binary.is_empty());
        assert_eq!(json, original);
    }

    #[test]
    fn appends_edges_to_the_explicit_mesh_when_accessor_is_shared() {
        let positions = triangle_positions();
        let meshes = json!([
            {
                "name": "first",
                "primitives": [{"attributes": {"POSITION": 0}}]
            },
            {
                "name": "second",
                "primitives": [{"attributes": {"POSITION": 0}}]
            }
        ]);
        let mut json = edge_fixture(meshes, None);
        let mut binary = Vec::new();

        append_edges_to_primitive(
            &mut json,
            &mut binary,
            0,
            0,
            &positions,
            &[0, 1, 2],
            0,
            1,
            30.0,
        )
        .unwrap();

        assert_eq!(json["meshes"][0]["primitives"].as_array().unwrap().len(), 2);
        assert_eq!(json["meshes"][1]["primitives"].as_array().unwrap().len(), 1);
    }
}
