use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use studio_core::geometry::Bounds;

use crate::PipelineError;
use crate::partition::{PartitionPlan, PartitionPlanNode};

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
    pub source_triangle_count: u64,
    pub node_count: u64,
    pub partition_count: usize,
    pub max_part_bytes: u64,
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
