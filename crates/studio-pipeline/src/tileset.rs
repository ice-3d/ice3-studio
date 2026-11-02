use std::collections::BTreeMap;
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::Digest;
use studio_core::geometry::Bounds;

use crate::PipelineError;
use crate::profile::{AssetProfile, RenderBatchMode};
use studio_assets::{AccessorData, SourceGlb, read_accessor};

use crate::partition::build::{
    EdgeOptions, GeometryOverride, build_artifact_with_profile, build_batched_leaf,
    build_leaf_with_overrides,
};
use crate::partition::{PartitionLeaf, PartitionPlan, PartitionPlanNode, PartitionUnit};

#[derive(Debug, Clone)]
pub struct BuildOptions {
    pub asset_id: String,
    pub max_triangles: u64,
    pub max_bytes: usize,
}

#[derive(Debug, Clone)]
pub struct PartitionFile {
    pub relative_path: String,
    pub content_hash: String,
    pub byte_length: usize,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PartitionStats {
    pub triangle_count: u64,
    pub edge_segment_count: u64,
    pub edge_bytes: u64,
    pub source_triangle_count: u64,
    pub node_count: u64,
    pub partition_count: usize,
    pub max_part_bytes: u64,
    pub render_batch_count: usize,
    pub coarse_count: usize,
}

#[derive(Debug, Clone)]
pub struct PartitionArtifact {
    pub files: Vec<PartitionFile>,
    pub tileset: TilesetDocument,
    pub stats: PartitionStats,
    pub selection_ids: BTreeSet<String>,
    pub plan: PartitionPlan,
}

impl PartitionArtifact {
    pub fn tileset_json(&self) -> String {
        serde_json::to_string(&self.tileset).expect("Tileset serialization is infallible")
    }

    pub fn all_selection_ids(&self) -> &BTreeSet<String> {
        &self.selection_ids
    }

    pub fn sorted_selection_ids(&self) -> Vec<String> {
        self.selection_ids.iter().cloned().collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TilesetDocument {
    pub asset: TilesetAsset,
    pub geometric_error: f64,
    pub root: TilesetTile,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TilesetAsset {
    pub version: String,
    pub extras: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TilesetTile {
    pub bounding_volume: TilesetBoundingVolume,
    pub geometric_error: f64,
    pub refine: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<TilesetContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contents: Option<Vec<TilesetContent>>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub children: Vec<TilesetTile>,
    pub extras: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TilesetBoundingVolume {
    #[serde(rename = "box")]
    pub bounds_box: [f64; 12],
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TilesetContent {
    pub uri: String,
}

impl TilesetDocument {
    pub(crate) fn from_plan(
        asset_id: &str,
        plan: &PartitionPlan,
        files: &[PartitionFile],
    ) -> Result<Self, PipelineError> {
        let mut next_leaf = 0;
        let root = tile_from_node(asset_id, &plan.root, files, &mut next_leaf, 0, "group-root")?;
        if next_leaf != files.len() {
            return Err(PipelineError::Tileset(
                "Tileset leaf count does not match files".into(),
            ));
        }
        if root.children.is_empty() && root.content.is_none() {
            return Err(PipelineError::Tileset(
                "root leaf has no content URI".into(),
            ));
        }
        Ok(Self {
            asset: TilesetAsset {
                version: "1.1".into(),
                extras: json!({
                    "ice3": {
                        "assetId": asset_id,
                        "contractVersion": "1.0"
                    }
                }),
            },
            geometric_error: root.geometric_error,
            root,
        })
    }
}

pub fn build_with_profile(
    source: &studio_assets::SourceGlb,
    asset_id: &str,
    profile: &AssetProfile,
) -> Result<PartitionArtifact, PipelineError> {
    profile.validate()?;
    let mut artifact = build_artifact_with_profile(source, asset_id, profile)?;
    let render_batch_count = if profile.render_batch == RenderBatchMode::Static {
        rebuild_with_static_batches(source, asset_id, &mut artifact, profile)?
    } else {
        0
    };
    let mut coarse_paths = BTreeMap::<String, Vec<String>>::new();
    if profile.lod_enabled {
        let partitions = collect_internal_partitions(&artifact.plan.root, "group-root");
        for (partition_id, node) in partitions {
            let files = build_coarse_files(source, asset_id, profile, &partition_id, &node.units)?;
            coarse_paths.insert(
                partition_id,
                files
                    .iter()
                    .map(|file| file.relative_path.clone())
                    .collect(),
            );
            artifact.files.extend(files);
        }
    }
    attach_coarse_content(
        &mut artifact.tileset.root,
        &artifact.plan.root,
        &coarse_paths,
    );
    artifact.stats.render_batch_count = render_batch_count;
    artifact.stats.coarse_count = coarse_paths.values().map(Vec::len).sum();
    refresh_artifact_stats(&mut artifact)?;
    Ok(artifact)
}

fn refresh_artifact_stats(artifact: &mut PartitionArtifact) -> Result<(), PipelineError> {
    let metrics = artifact
        .files
        .iter()
        .map(crate::partition::build::part_edge_metrics)
        .collect::<Result<Vec<_>, _>>()?;
    artifact.stats.edge_segment_count = metrics.iter().map(|(segments, _)| segments).sum();
    artifact.stats.edge_bytes = metrics.iter().map(|(_, bytes)| bytes).sum();
    artifact.stats.max_part_bytes = artifact
        .files
        .iter()
        .map(|file| file.byte_length as u64)
        .max()
        .unwrap_or(0);
    artifact.tileset.asset.extras["ice3"]["stats"] = json!({
        "triangleCount": artifact.stats.triangle_count,
        "edgeSegmentCount": artifact.stats.edge_segment_count,
        "edgeBytes": artifact.stats.edge_bytes,
        "partitionCount": artifact.stats.partition_count,
        "renderBatchCount": artifact.stats.render_batch_count,
        "coarseCount": artifact.stats.coarse_count,
        "maxPartBytes": artifact.stats.max_part_bytes
    });
    Ok(())
}

fn rebuild_with_static_batches(
    source: &SourceGlb,
    asset_id: &str,
    artifact: &mut PartitionArtifact,
    profile: &AssetProfile,
) -> Result<usize, PipelineError> {
    let mut render_batch_count = 0;
    for (index, leaf) in artifact.plan.leaves.iter().enumerate() {
        let (built, batches) =
            build_batched_leaf(source, asset_id, leaf, EdgeOptions::from_profile(profile))?;
        if built.bytes.len() > profile.max_bytes {
            return Err(crate::PipelineError::Tileset(format!(
                "partition {} exceeds maxBytes",
                leaf.id
            )));
        }
        render_batch_count += batches;
        let file = artifact
            .files
            .iter_mut()
            .find(|file| file.relative_path == format!("parts/part-{index:03}.glb"))
            .ok_or_else(|| PipelineError::Tileset("fine partition file is missing".into()))?;
        file.content_hash = format!("{:x}", sha2::Sha256::digest(&built.bytes));
        file.byte_length = built.bytes.len();
        file.bytes = built.bytes;
    }
    Ok(render_batch_count)
}

fn collect_internal_partitions<'a>(
    node: &'a PartitionPlanNode,
    partition_id: &str,
) -> Vec<(String, &'a PartitionPlanNode)> {
    if node.children.is_empty() {
        return Vec::new();
    }
    let mut output = vec![(partition_id.to_owned(), node)];
    for (index, child) in node.children.iter().enumerate() {
        output.extend(collect_internal_partitions(
            child,
            &format!("{partition_id}-{index}"),
        ));
    }
    output
}

fn attach_coarse_content(
    tile: &mut TilesetTile,
    node: &PartitionPlanNode,
    coarse_paths: &BTreeMap<String, Vec<String>>,
) {
    if let Some(partition_id) = tile
        .extras
        .get("ice3")
        .and_then(|extras| extras.get("partitionId"))
        .and_then(serde_json::Value::as_str)
        && let Some(paths) = coarse_paths.get(partition_id)
    {
        if paths.len() == 1 {
            tile.content = Some(TilesetContent {
                uri: paths[0].clone(),
            });
        } else {
            tile.content = None;
            tile.contents = Some(
                paths
                    .iter()
                    .cloned()
                    .map(|uri| TilesetContent { uri })
                    .collect(),
            );
        }
    }
    for (child_tile, child_node) in tile.children.iter_mut().zip(&node.children) {
        attach_coarse_content(child_tile, child_node, coarse_paths);
    }
}

fn build_coarse_files(
    source: &SourceGlb,
    asset_id: &str,
    profile: &AssetProfile,
    partition_id: &str,
    units: &[PartitionUnit],
) -> Result<Vec<PartitionFile>, PipelineError> {
    if units.is_empty() {
        return Err(PipelineError::Tileset(
            "coarse partition has no units".into(),
        ));
    }
    let source_bytes = units
        .iter()
        .map(|unit| {
            unit.triangle_count
                .saturating_mul(unit.node_ids.len() as u64)
                .saturating_mul(64)
        })
        .sum::<u64>();
    if source_bytes > profile.lod_max_memory_bytes as u64 {
        return Err(PipelineError::Tileset(
            "coarse partition exceeds the LOD memory budget".into(),
        ));
    }
    let overrides = create_coarse_overrides(source, profile, units)?;
    let leaf = PartitionLeaf {
        id: partition_id.into(),
        bounds: union_unit_bounds(units),
        triangle_count: units.iter().map(|unit| unit.triangle_count).sum(),
        units: units.to_vec(),
    };
    let built = build_leaf_with_overrides(
        source,
        &format!("{asset_id}:lod:{partition_id}"),
        &leaf,
        &overrides,
        EdgeOptions::from_profile(profile),
    )?;
    if built.bytes.len() <= profile.coarse_chunk_max_bytes || units.len() == 1 {
        if built.bytes.len() > profile.coarse_chunk_max_bytes {
            return Err(PipelineError::Tileset(format!(
                "coarse unit {partition_id} exceeds the chunk byte budget"
            )));
        }
        return Ok(vec![PartitionFile {
            relative_path: format!("coarse/{partition_id}.glb"),
            content_hash: format!("{:x}", sha2::Sha256::digest(&built.bytes)),
            byte_length: built.bytes.len(),
            bytes: built.bytes,
        }]);
    }
    let middle = units.len() / 2;
    let mut files = build_coarse_files(
        source,
        asset_id,
        profile,
        &format!("{partition_id}-chunk-0"),
        &units[..middle],
    )?;
    files.extend(build_coarse_files(
        source,
        asset_id,
        profile,
        &format!("{partition_id}-chunk-1"),
        &units[middle..],
    )?);
    Ok(files)
}

fn create_coarse_overrides(
    source: &SourceGlb,
    profile: &AssetProfile,
    units: &[PartitionUnit],
) -> Result<BTreeMap<usize, GeometryOverride>, PipelineError> {
    let geometries = units
        .iter()
        .map(|unit| read_unit_geometry(source, unit))
        .collect::<Result<Vec<_>, _>>()?;
    let source_triangles = geometries
        .iter()
        .map(|geometry| geometry.indices.len() as u64 / 3)
        .collect::<Vec<_>>();
    let targets = match profile.lod_allocation {
        super::lod::LodAllocationMode::Proportional => super::lod::allocate_lod_targets(
            &source_triangles,
            profile.lod_ratio,
            profile.lod_max_triangles,
            profile.lod_min_unit_triangles,
        ),
        super::lod::LodAllocationMode::Adaptive => super::lod::allocate_adaptive_targets(
            &source_triangles,
            profile.lod_ratio,
            profile.lod_max_triangles,
            profile.lod_min_unit_triangles,
        ),
    };
    let mut output = BTreeMap::new();
    for ((unit, mut geometry), target) in units.iter().zip(geometries).zip(targets) {
        if profile.semantic_proxy == super::lod::SemanticProxyMode::Auto
            && geometry.indices.len() / 3 >= 64
        {
            let samples = super::lod::FitSamples {
                vertices: geometry.positions.clone(),
                indices: geometry.indices.clone(),
            };
            if let Some(proxy) =
                super::lod::create_semantic_proxy(&samples, profile.semantic_proxy)?
                && proxy.triangle_count < target.target_triangles as usize
            {
                geometry.positions = proxy.positions;
                geometry.indices = proxy.indices;
            }
        } else {
            let simplified = super::lod::simplify_geometry(
                &geometry.positions,
                &geometry.indices,
                &super::lod::SimplifyOptions {
                    target_triangles: target.target_triangles as usize,
                    minimum_triangles: target.minimum_triangles as usize,
                },
                &super::lod::Simplifier,
            );
            geometry.positions = simplified.positions;
            geometry.indices = simplified.indices;
        }
        output.insert(
            unit.mesh_index,
            GeometryOverride {
                positions: geometry.positions,
                indices: geometry.indices,
                material_index: geometry.material_index,
            },
        );
    }
    Ok(output)
}

struct UnitGeometry {
    positions: Vec<f32>,
    indices: Vec<u32>,
    material_index: Option<usize>,
}

fn read_unit_geometry(
    source: &SourceGlb,
    unit: &PartitionUnit,
) -> Result<UnitGeometry, PipelineError> {
    let mesh = source
        .json
        .get("meshes")
        .and_then(serde_json::Value::as_array)
        .and_then(|meshes| meshes.get(unit.mesh_index))
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| PipelineError::Tileset("coarse source mesh is missing".into()))?;
    let primitives = mesh
        .get("primitives")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| PipelineError::Tileset("coarse primitives are missing".into()))?;
    let mut positions = Vec::new();
    let mut indices = Vec::new();
    let mut material_weights = BTreeMap::<usize, usize>::new();
    for primitive in primitives {
        let mode = primitive
            .get("mode")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(4);
        if mode != 4 {
            continue;
        }
        let position_index = primitive
            .get("attributes")
            .and_then(serde_json::Value::as_object)
            .and_then(|attributes| attributes.get("POSITION"))
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| PipelineError::Tileset("coarse POSITION is missing".into()))?;
        let AccessorData::F32 {
            values: primitive_positions,
            components: 3,
        } = read_accessor(source, position_index)?
        else {
            return Err(PipelineError::Tileset(
                "coarse POSITION must be VEC3 float".into(),
            ));
        };
        let vertex_offset = positions.len() / 3;
        let primitive_indices = match primitive
            .get("indices")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
        {
            Some(index) => match read_accessor(source, index)? {
                AccessorData::U16 { values, .. } => {
                    values.into_iter().map(u32::from).collect::<Vec<_>>()
                }
                AccessorData::U32 { values, .. } => values,
                AccessorData::F32 { .. } => {
                    return Err(PipelineError::Tileset(
                        "coarse indices must be integers".into(),
                    ));
                }
            },
            None => (0..primitive_positions.len() as u32 / 3).collect(),
        };
        positions.extend(primitive_positions);
        let primitive_triangle_count = primitive_indices.len() / 3;
        indices.extend(
            primitive_indices
                .into_iter()
                .map(|index| index + vertex_offset as u32),
        );
        if let Some(material) = primitive
            .get("material")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
        {
            *material_weights.entry(material).or_default() += primitive_triangle_count;
        }
    }
    Ok(UnitGeometry {
        positions,
        indices,
        material_index: material_weights
            .into_iter()
            .max_by_key(|(material, weight)| (*weight, usize::MAX - *material))
            .map(|(material, _)| material),
    })
}

fn union_unit_bounds(units: &[PartitionUnit]) -> Bounds {
    let first = units[0].bounds;
    units.iter().skip(1).fold(first, |current, unit| Bounds {
        min: studio_core::math::Vec3 {
            x: current.min.x.min(unit.bounds.min.x),
            y: current.min.y.min(unit.bounds.min.y),
            z: current.min.z.min(unit.bounds.min.z),
        },
        max: studio_core::math::Vec3 {
            x: current.max.x.max(unit.bounds.max.x),
            y: current.max.y.max(unit.bounds.max.y),
            z: current.max.z.max(unit.bounds.max.z),
        },
    })
}

fn tile_from_node(
    asset_id: &str,
    node: &PartitionPlanNode,
    files: &[PartitionFile],
    next_leaf: &mut usize,
    depth: usize,
    partition_id: &str,
) -> Result<TilesetTile, PipelineError> {
    let is_leaf = node.children.is_empty();
    let content_uris = if is_leaf {
        let file = files
            .get(*next_leaf)
            .ok_or_else(|| PipelineError::Tileset("leaf file is missing".into()))?;
        *next_leaf += 1;
        vec![file.relative_path.clone()]
    } else {
        Vec::new()
    };
    if !is_leaf && !content_uris.is_empty() {
        return Err(PipelineError::Tileset(
            "internal tile must not have content".into(),
        ));
    }
    let geometric_error = if is_leaf {
        0.0
    } else {
        diagonal(node.bounds) / (1_u64 << depth.min(20)) as f64
    };
    let children = node
        .children
        .iter()
        .enumerate()
        .map(|(index, child)| {
            tile_from_node(
                asset_id,
                child,
                files,
                next_leaf,
                depth + 1,
                &format!("{partition_id}-{index}"),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    if children
        .iter()
        .any(|child| child.geometric_error > geometric_error)
    {
        return Err(PipelineError::Tileset(
            "child geometricError exceeds parent".into(),
        ));
    }
    let (content, contents) = match content_uris.len() {
        0 => (None, None),
        1 => (
            Some(TilesetContent {
                uri: content_uris[0].clone(),
            }),
            None,
        ),
        _ => (
            None,
            Some(
                content_uris
                    .into_iter()
                    .map(|uri| TilesetContent { uri })
                    .collect(),
            ),
        ),
    };
    Ok(TilesetTile {
        bounding_volume: TilesetBoundingVolume {
            bounds_box: to_tiles_box(node.bounds),
        },
        geometric_error,
        refine: "REPLACE".into(),
        content,
        contents,
        children,
        extras: json!({"ice3": {"assetId": asset_id, "partitionId": partition_id}}),
    })
}

fn to_tiles_box(bounds: Bounds) -> [f64; 12] {
    let center_x = (bounds.min.x + bounds.max.x) as f64 / 2.0;
    let center_y = (bounds.min.y + bounds.max.y) as f64 / 2.0;
    let center_z = (bounds.min.z + bounds.max.z) as f64 / 2.0;
    let half_x = (bounds.max.x - bounds.min.x) as f64 / 2.0;
    let half_y = (bounds.max.y - bounds.min.y) as f64 / 2.0;
    let half_z = (bounds.max.z - bounds.min.z) as f64 / 2.0;
    [
        center_x, -center_z, center_y, half_x, 0.0, 0.0, 0.0, 0.0, half_y, 0.0, half_z, 0.0,
    ]
}

fn diagonal(bounds: Bounds) -> f64 {
    let x = (bounds.max.x - bounds.min.x) as f64;
    let y = (bounds.max.y - bounds.min.y) as f64;
    let z = (bounds.max.z - bounds.min.z) as f64;
    (x * x + y * y + z * z).sqrt()
}
