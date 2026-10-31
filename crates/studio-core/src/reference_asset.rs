use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::geometry::Bounds;
use crate::ids::MaterialId;
use crate::math::Transform;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct ReferenceInstanceContent {
    pub asset_id: String,
    pub selection_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct ReferenceAssetDescriptor {
    pub asset_id: String,
    pub source_digest: String,
    pub prepared_digest: Option<String>,
    pub tileset_digest: Option<String>,
    pub bounds: Bounds,
    pub stats: ReferenceAssetStats,
    pub imported_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct ReferenceAssetStats {
    pub source_nodes: usize,
    pub effective_triangles: u64,
    pub unique_meshes: usize,
    pub materials: usize,
    pub equipment: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct ReferenceAssetEquipment {
    pub selection_id: String,
    pub name: String,
    pub kind: Option<String>,
    pub process_stage: Option<String>,
    pub area: Option<String>,
    pub bounds: Bounds,
    pub transform: Transform,
    pub material_ids: Vec<MaterialId>,
}
