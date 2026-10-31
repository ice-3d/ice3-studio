use std::collections::BTreeMap;

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
    /// 资产声明的可覆盖材质槽位。Viewer 按槽位取色，不解析节点名。
    #[serde(default)]
    pub material_slots: Vec<ReferenceMaterialSlot>,
    /// 展示曝光系数。资产可声明自身材质整体偏暗需要提亮，内核只做数值传递。
    #[serde(default = "default_display_exposure")]
    pub display_exposure: f32,
    pub imported_at: String,
}

fn default_display_exposure() -> f32 {
    1.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct ReferenceAssetStats {
    pub source_nodes: usize,
    pub effective_triangles: u64,
    pub unique_meshes: usize,
    pub materials: usize,
    /// 语义绑定条目数。旧工程把该字段写作 `equipment`。
    #[serde(alias = "equipment")]
    pub bindings: usize,
}

/// 资产发布方声明的材质槽位。`color` 是资产的默认表现，可被工程覆盖。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct ReferenceMaterialSlot {
    pub id: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
}

/// 一条语义绑定：把资产内的稳定选择标识映射到可寻址的实体。
/// 行业属性一律放在 `metadata`，内核不认识任何具体字段。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct ReferenceAssetBinding {
    pub selection_id: String,
    pub name: String,
    pub kind: Option<String>,
    pub bounds: Bounds,
    pub transform: Transform,
    pub material_ids: Vec<MaterialId>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
    /// 该绑定使用的材质槽位。为空时使用资产的默认材质。
    #[serde(default)]
    pub slot: Option<String>,
    /// 资产声明该绑定不应显示（例如已废弃构件）。
    #[serde(default)]
    pub hidden: bool,
}
