use std::collections::{HashMap, HashSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::document::{Document, NodeContent, SceneNode};
use crate::geometry::{Bounds, evaluate_node_content};
use crate::large_scene::evaluate::{evaluate_large_scene, evaluated_scene_bounds};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[ts(export)]
pub enum DiagnosticCode {
    #[serde(rename = "ICE3_STUDIO_API_INVALID")]
    #[ts(rename = "ICE3_STUDIO_API_INVALID")]
    ApiInvalid,
    #[serde(rename = "ICE3_STUDIO_API_VERSION_UNSUPPORTED")]
    #[ts(rename = "ICE3_STUDIO_API_VERSION_UNSUPPORTED")]
    ApiVersionUnsupported,
    #[serde(rename = "ICE3_STUDIO_AUTH_REQUIRED")]
    #[ts(rename = "ICE3_STUDIO_AUTH_REQUIRED")]
    AuthRequired,
    #[serde(rename = "ICE3_STUDIO_PROJECT_NOT_FOUND")]
    #[ts(rename = "ICE3_STUDIO_PROJECT_NOT_FOUND")]
    ProjectNotFound,
    #[serde(rename = "ICE3_STUDIO_PROJECT_LOCKED")]
    #[ts(rename = "ICE3_STUDIO_PROJECT_LOCKED")]
    ProjectLocked,
    #[serde(rename = "ICE3_STUDIO_REVISION_CONFLICT")]
    #[ts(rename = "ICE3_STUDIO_REVISION_CONFLICT")]
    RevisionConflict,
    #[serde(rename = "ICE3_STUDIO_OPERATION_INVALID")]
    #[ts(rename = "ICE3_STUDIO_OPERATION_INVALID")]
    OperationInvalid,
    #[serde(rename = "ICE3_STUDIO_REFERENCE_INVALID")]
    #[ts(rename = "ICE3_STUDIO_REFERENCE_INVALID")]
    ReferenceInvalid,
    #[serde(rename = "ICE3_STUDIO_REFERENCE_EXPORT_UNSUPPORTED")]
    #[ts(rename = "ICE3_STUDIO_REFERENCE_EXPORT_UNSUPPORTED")]
    ReferenceExportUnsupported,
    #[serde(rename = "ICE3_STUDIO_GEOMETRY_INVALID")]
    #[ts(rename = "ICE3_STUDIO_GEOMETRY_INVALID")]
    GeometryInvalid,
    #[serde(rename = "ICE3_STUDIO_CONSTRAINT_FAILED")]
    #[ts(rename = "ICE3_STUDIO_CONSTRAINT_FAILED")]
    ConstraintFailed,
    #[serde(rename = "ICE3_STUDIO_BUDGET_EXCEEDED")]
    #[ts(rename = "ICE3_STUDIO_BUDGET_EXCEEDED")]
    BudgetExceeded,
    #[serde(rename = "ICE3_STUDIO_EXPORT_FAILED")]
    #[ts(rename = "ICE3_STUDIO_EXPORT_FAILED")]
    ExportFailed,
    #[serde(rename = "ICE3_STUDIO_RUNTIME_PREVIEW_FAILED")]
    #[ts(rename = "ICE3_STUDIO_RUNTIME_PREVIEW_FAILED")]
    RuntimePreviewFailed,
    #[serde(rename = "ICE3_STUDIO_PERSISTENCE_FAILED")]
    #[ts(rename = "ICE3_STUDIO_PERSISTENCE_FAILED")]
    PersistenceFailed,
    #[serde(rename = "ICE3_STUDIO_COMPONENT_INVALID")]
    #[ts(rename = "ICE3_STUDIO_COMPONENT_INVALID")]
    ComponentInvalid,
    #[serde(rename = "ICE3_STUDIO_PARAMETER_INVALID")]
    #[ts(rename = "ICE3_STUDIO_PARAMETER_INVALID")]
    ParameterInvalid,
    #[serde(rename = "ICE3_STUDIO_INSTANCE_INVALID")]
    #[ts(rename = "ICE3_STUDIO_INSTANCE_INVALID")]
    InstanceInvalid,
    #[serde(rename = "ICE3_STUDIO_INSTANCE_SET_INVALID")]
    #[ts(rename = "ICE3_STUDIO_INSTANCE_SET_INVALID")]
    InstanceSetInvalid,
    #[serde(rename = "ICE3_STUDIO_CONNECTION_INVALID")]
    #[ts(rename = "ICE3_STUDIO_CONNECTION_INVALID")]
    ConnectionInvalid,
    #[serde(rename = "ICE3_STUDIO_GEOMETRY_CACHE_FAILED")]
    #[ts(rename = "ICE3_STUDIO_GEOMETRY_CACHE_FAILED")]
    GeometryCacheFailed,
    #[serde(rename = "ICE3_STUDIO_GPU_UNAVAILABLE")]
    #[ts(rename = "ICE3_STUDIO_GPU_UNAVAILABLE")]
    GpuUnavailable,
    #[serde(rename = "ICE3_STUDIO_GPU_INCOMPATIBLE")]
    #[ts(rename = "ICE3_STUDIO_GPU_INCOMPATIBLE")]
    GpuIncompatible,
    #[serde(rename = "ICE3_STUDIO_GPU_BUDGET_EXCEEDED")]
    #[ts(rename = "ICE3_STUDIO_GPU_BUDGET_EXCEEDED")]
    GpuBudgetExceeded,
    #[serde(rename = "ICE3_STUDIO_GPU_KERNEL_FAILED")]
    #[ts(rename = "ICE3_STUDIO_GPU_KERNEL_FAILED")]
    GpuKernelFailed,
    #[serde(rename = "ICE3_STUDIO_GPU_DEVICE_LOST")]
    #[ts(rename = "ICE3_STUDIO_GPU_DEVICE_LOST")]
    GpuDeviceLost,
    #[serde(rename = "ICE3_STUDIO_GPU_PARITY_FAILED")]
    #[ts(rename = "ICE3_STUDIO_GPU_PARITY_FAILED")]
    GpuParityFailed,
    #[serde(rename = "ICE3_STUDIO_GEOMETRY_OVERLAP")]
    #[ts(rename = "ICE3_STUDIO_GEOMETRY_OVERLAP")]
    GeometryOverlap,
}

pub const DIAGNOSTIC_CODES: &[&str] = &[
    "ICE3_STUDIO_API_INVALID",
    "ICE3_STUDIO_API_VERSION_UNSUPPORTED",
    "ICE3_STUDIO_AUTH_REQUIRED",
    "ICE3_STUDIO_PROJECT_NOT_FOUND",
    "ICE3_STUDIO_PROJECT_LOCKED",
    "ICE3_STUDIO_REVISION_CONFLICT",
    "ICE3_STUDIO_OPERATION_INVALID",
    "ICE3_STUDIO_REFERENCE_INVALID",
    "ICE3_STUDIO_REFERENCE_EXPORT_UNSUPPORTED",
    "ICE3_STUDIO_GEOMETRY_INVALID",
    "ICE3_STUDIO_CONSTRAINT_FAILED",
    "ICE3_STUDIO_BUDGET_EXCEEDED",
    "ICE3_STUDIO_EXPORT_FAILED",
    "ICE3_STUDIO_RUNTIME_PREVIEW_FAILED",
    "ICE3_STUDIO_PERSISTENCE_FAILED",
    "ICE3_STUDIO_COMPONENT_INVALID",
    "ICE3_STUDIO_PARAMETER_INVALID",
    "ICE3_STUDIO_INSTANCE_INVALID",
    "ICE3_STUDIO_INSTANCE_SET_INVALID",
    "ICE3_STUDIO_CONNECTION_INVALID",
    "ICE3_STUDIO_GEOMETRY_CACHE_FAILED",
    "ICE3_STUDIO_GPU_UNAVAILABLE",
    "ICE3_STUDIO_GPU_INCOMPATIBLE",
    "ICE3_STUDIO_GPU_BUDGET_EXCEEDED",
    "ICE3_STUDIO_GPU_KERNEL_FAILED",
    "ICE3_STUDIO_GPU_DEVICE_LOST",
    "ICE3_STUDIO_GPU_PARITY_FAILED",
    "ICE3_STUDIO_GEOMETRY_OVERLAP",
];

impl DiagnosticCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ApiInvalid => "ICE3_STUDIO_API_INVALID",
            Self::ApiVersionUnsupported => "ICE3_STUDIO_API_VERSION_UNSUPPORTED",
            Self::AuthRequired => "ICE3_STUDIO_AUTH_REQUIRED",
            Self::ProjectNotFound => "ICE3_STUDIO_PROJECT_NOT_FOUND",
            Self::ProjectLocked => "ICE3_STUDIO_PROJECT_LOCKED",
            Self::RevisionConflict => "ICE3_STUDIO_REVISION_CONFLICT",
            Self::OperationInvalid => "ICE3_STUDIO_OPERATION_INVALID",
            Self::ReferenceInvalid => "ICE3_STUDIO_REFERENCE_INVALID",
            Self::ReferenceExportUnsupported => "ICE3_STUDIO_REFERENCE_EXPORT_UNSUPPORTED",
            Self::GeometryInvalid => "ICE3_STUDIO_GEOMETRY_INVALID",
            Self::ConstraintFailed => "ICE3_STUDIO_CONSTRAINT_FAILED",
            Self::BudgetExceeded => "ICE3_STUDIO_BUDGET_EXCEEDED",
            Self::ExportFailed => "ICE3_STUDIO_EXPORT_FAILED",
            Self::RuntimePreviewFailed => "ICE3_STUDIO_RUNTIME_PREVIEW_FAILED",
            Self::PersistenceFailed => "ICE3_STUDIO_PERSISTENCE_FAILED",
            Self::ComponentInvalid => "ICE3_STUDIO_COMPONENT_INVALID",
            Self::ParameterInvalid => "ICE3_STUDIO_PARAMETER_INVALID",
            Self::InstanceInvalid => "ICE3_STUDIO_INSTANCE_INVALID",
            Self::InstanceSetInvalid => "ICE3_STUDIO_INSTANCE_SET_INVALID",
            Self::ConnectionInvalid => "ICE3_STUDIO_CONNECTION_INVALID",
            Self::GeometryCacheFailed => "ICE3_STUDIO_GEOMETRY_CACHE_FAILED",
            Self::GpuUnavailable => "ICE3_STUDIO_GPU_UNAVAILABLE",
            Self::GpuIncompatible => "ICE3_STUDIO_GPU_INCOMPATIBLE",
            Self::GpuBudgetExceeded => "ICE3_STUDIO_GPU_BUDGET_EXCEEDED",
            Self::GpuKernelFailed => "ICE3_STUDIO_GPU_KERNEL_FAILED",
            Self::GpuDeviceLost => "ICE3_STUDIO_GPU_DEVICE_LOST",
            Self::GpuParityFailed => "ICE3_STUDIO_GPU_PARITY_FAILED",
            Self::GeometryOverlap => "ICE3_STUDIO_GEOMETRY_OVERLAP",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Diagnostic {
    /// 稳定的 `ICE3_STUDIO_*` 诊断码，调用方应依赖它而不是 message 文本。
    pub code: DiagnosticCode,
    /// 原子事务中出错操作的序号，从 0 开始。
    pub operation_index: Option<usize>,
    /// 相关的稳定对象 ID。
    pub object_id: Option<String>,
    /// 指向无效文档字段的 RFC 6901 JSON Pointer。
    pub pointer: String,
    /// 人类可读的说明；可能随版本变化，不要用于分支判断。
    pub message: String,
}

pub fn validate_document(document: &Document) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut node_ids = HashSet::new();
    for (index, node) in document.scene.nodes().iter().enumerate() {
        if !node.id.is_valid() {
            diagnostics.push(reference(
                Some(index),
                node.id.as_str(),
                format!("/scene/nodes/{index}/id"),
                "node ID is invalid",
            ));
        }
        if !node_ids.insert(node.id.clone()) {
            diagnostics.push(reference(
                Some(index),
                node.id.as_str(),
                format!("/scene/nodes/{index}/id"),
                "node ID is duplicated",
            ));
        }
        if !finite_transform(node) {
            diagnostics.push(geometry(
                Some(index),
                node.id.as_str(),
                format!("/scene/nodes/{index}/transform"),
                "node transform must be finite with non-zero scale".into(),
            ));
        }
        if let Err(error) = evaluate_node_content(&node.content) {
            diagnostics.push(geometry(
                Some(index),
                node.id.as_str(),
                format!("/scene/nodes/{index}/content"),
                error.to_string(),
            ));
        }
        if let NodeContent::ReferenceInstance(reference) = &node.content
            && (reference.asset_id.trim().is_empty() || reference.selection_id.trim().is_empty())
        {
            diagnostics.push(reference_diagnostic(
                Some(index),
                node.id.as_str(),
                format!("/scene/nodes/{index}/content"),
                "reference asset and selection IDs must not be empty",
            ));
        }
    }

    let node_by_id: HashMap<_, _> = document
        .scene
        .nodes()
        .iter()
        .map(|node| (node.id.clone(), node))
        .collect();
    let has_overlap_constraint = document
        .constraints
        .iter()
        .any(|constraint| constraint.constraint_type == "no_overlap");
    let constraint_bounds = if has_overlap_constraint {
        derived_source_bounds(document)
    } else {
        HashMap::new()
    };
    for (index, node) in document.scene.nodes().iter().enumerate() {
        if let Some(parent) = &node.parent_id
            && !node_by_id.contains_key(parent)
        {
            diagnostics.push(reference(
                Some(index),
                node.id.as_str(),
                format!("/scene/nodes/{index}/parentId"),
                "parent node does not exist",
            ));
        }
        if let Some(material) = &node.material_id
            && !document
                .materials
                .iter()
                .any(|candidate| candidate.id == *material)
        {
            diagnostics.push(reference(
                Some(index),
                node.id.as_str(),
                format!("/scene/nodes/{index}/materialId"),
                "material does not exist",
            ));
        }
        if has_parent_cycle(node, &node_by_id) {
            diagnostics.push(reference(
                Some(index),
                node.id.as_str(),
                format!("/scene/nodes/{index}/parentId"),
                "node hierarchy contains a cycle",
            ));
        }
    }

    let mut material_ids = HashSet::new();
    for (index, material) in document.materials.iter().enumerate() {
        if !material.id.is_valid() {
            diagnostics.push(reference(
                None,
                material.id.as_str(),
                format!("/materials/{index}/id"),
                "material ID is invalid",
            ));
        }
        if !material_ids.insert(material.id.clone()) {
            diagnostics.push(reference(
                None,
                material.id.as_str(),
                format!("/materials/{index}/id"),
                "material ID is duplicated",
            ));
        }
        if !material.base_color.iter().all(|value| value.is_finite())
            || !(0.0..=1.0).contains(&material.opacity)
        {
            diagnostics.push(geometry(
                None,
                material.id.as_str(),
                format!("/materials/{index}"),
                "material values must be finite with opacity between zero and one".into(),
            ));
        }
        if let Some(surface) = &material.surface {
            let factors_valid =
                (0.0..=1.0).contains(&surface.metallic) && (0.0..=1.0).contains(&surface.roughness);
            let emissive_valid = surface.emissive.is_none_or(|emissive| {
                emissive
                    .iter()
                    .all(|value| value.is_finite() && *value >= 0.0)
            });
            let scale_valid = surface
                .uv_scale
                .iter()
                .all(|value| value.is_finite() && *value > 0.0);
            let texture_valid = surface
                .base_color_texture
                .as_ref()
                .is_none_or(|path| texture_path_is_relative(path))
                && surface
                    .normal_texture
                    .as_ref()
                    .is_none_or(|path| texture_path_is_relative(path))
                && surface
                    .orm_texture
                    .as_ref()
                    .is_none_or(|path| texture_path_is_relative(path));
            let normal_scale_valid = surface
                .normal_scale
                .is_none_or(|scale| scale.is_finite() && (0.0..=2.0).contains(&scale));
            if !(factors_valid
                && emissive_valid
                && scale_valid
                && texture_valid
                && normal_scale_valid)
            {
                diagnostics.push(geometry(
                    None,
                    material.id.as_str(),
                    format!("/materials/{index}/surface"),
                    "material surface must keep metallic/roughness in [0,1], emissive non-negative, uvScale positive, and a relative texture path".into(),
                ));
            }
        }
    }

    let mut relation_ids = HashSet::new();
    for (index, relation) in document.relations.iter().enumerate() {
        if !relation.id.is_valid() {
            diagnostics.push(reference(
                None,
                relation.id.as_str(),
                format!("/relations/{index}/id"),
                "relation ID is invalid",
            ));
        }
        if !relation_ids.insert(relation.id.clone()) {
            diagnostics.push(reference(
                None,
                relation.id.as_str(),
                format!("/relations/{index}/id"),
                "relation ID is duplicated",
            ));
        }
        if !node_by_id.contains_key(&relation.from_node_id)
            || !node_by_id.contains_key(&relation.to_node_id)
        {
            diagnostics.push(reference(
                None,
                relation.id.as_str(),
                format!("/relations/{index}"),
                "relation endpoint does not exist",
            ));
        }
    }

    let mut constraint_ids = HashSet::new();
    for (index, constraint) in document.constraints.iter().enumerate() {
        if !constraint.id.is_valid() {
            diagnostics.push(reference(
                None,
                constraint.id.as_str(),
                format!("/constraints/{index}/id"),
                "constraint ID is invalid",
            ));
        }
        if !constraint_ids.insert(constraint.id.clone()) {
            diagnostics.push(reference(
                None,
                constraint.id.as_str(),
                format!("/constraints/{index}/id"),
                "constraint ID is duplicated",
            ));
        }
        if constraint
            .node_ids
            .iter()
            .any(|id| !node_by_id.contains_key(id))
        {
            diagnostics.push(reference(
                None,
                constraint.id.as_str(),
                format!("/constraints/{index}/nodeIds"),
                "constraint node does not exist",
            ));
        }
        if constraint.constraint_type == "no_overlap"
            && overlapping_nodes(&constraint.node_ids, &node_by_id, &constraint_bounds)
        {
            diagnostics.push(Diagnostic {
                code: DiagnosticCode::ConstraintFailed,
                operation_index: None,
                object_id: Some(constraint.id.as_str().to_owned()),
                pointer: format!("/constraints/{index}"),
                message: "no_overlap constraint is violated".into(),
            });
        }
    }
    diagnostics.extend(crate::large_scene::validate::validate_large_scene(document));
    diagnostics
}

fn reference_diagnostic(
    operation_index: Option<usize>,
    object_id: &str,
    pointer: String,
    message: impl Into<String>,
) -> Diagnostic {
    Diagnostic {
        code: DiagnosticCode::ReferenceInvalid,
        operation_index,
        object_id: Some(object_id.to_owned()),
        pointer,
        message: message.into(),
    }
}

fn finite_transform(node: &SceneNode) -> bool {
    let rotation_norm_squared = node
        .transform
        .rotation
        .iter()
        .map(|value| value * value)
        .sum::<f32>();
    node.transform.translation.x.is_finite()
        && node.transform.translation.y.is_finite()
        && node.transform.translation.z.is_finite()
        && node
            .transform
            .rotation
            .iter()
            .all(|value| value.is_finite())
        && rotation_norm_squared > f32::EPSILON
        && node.transform.scale.x.is_finite()
        && node.transform.scale.y.is_finite()
        && node.transform.scale.z.is_finite()
        && node.transform.scale.x > 0.0
        && node.transform.scale.y > 0.0
        && node.transform.scale.z > 0.0
}

fn has_parent_cycle(node: &SceneNode, nodes: &HashMap<crate::ids::NodeId, &SceneNode>) -> bool {
    let mut seen = HashSet::new();
    let mut current = node.parent_id.as_ref();
    while let Some(parent) = current {
        if !seen.insert(parent.clone()) {
            return true;
        }
        current = nodes
            .get(parent)
            .and_then(|candidate| candidate.parent_id.as_ref());
    }
    false
}

fn overlapping_nodes(
    node_ids: &[crate::ids::NodeId],
    nodes: &HashMap<crate::ids::NodeId, &SceneNode>,
    bounds_by_node: &HashMap<crate::ids::NodeId, Bounds>,
) -> bool {
    let bounds: Vec<_> = node_ids
        .iter()
        .filter(|id| nodes.contains_key(id))
        .filter_map(|id| bounds_by_node.get(id).copied())
        .collect();
    for left in 0..bounds.len() {
        for right in left + 1..bounds.len() {
            let a = bounds[left];
            let b = bounds[right];
            if a.min.x < b.max.x
                && a.max.x > b.min.x
                && a.min.y < b.max.y
                && a.max.y > b.min.y
                && a.min.z < b.max.z
                && a.max.z > b.min.z
            {
                return true;
            }
        }
    }
    false
}

fn derived_source_bounds(document: &Document) -> HashMap<crate::ids::NodeId, Bounds> {
    let Ok(evaluated) = evaluate_large_scene(document) else {
        return HashMap::new();
    };
    let bounds = evaluated_scene_bounds(&evaluated);
    let mut output = HashMap::new();
    for node in &evaluated.nodes {
        let Some(node_bounds) = bounds.get(&node.id) else {
            continue;
        };
        let source_id = document
            .scene
            .nodes()
            .iter()
            .find(|source| {
                node.owner_id == source.id.as_str()
                    || node
                        .owner_id
                        .strip_prefix(source.id.as_str())
                        .is_some_and(|suffix| suffix.starts_with('/'))
            })
            .map(|source| source.id.clone());
        let Some(source_id) = source_id else {
            continue;
        };
        output
            .entry(source_id)
            .and_modify(|current: &mut Bounds| {
                current.min.x = current.min.x.min(node_bounds.min.x);
                current.min.y = current.min.y.min(node_bounds.min.y);
                current.min.z = current.min.z.min(node_bounds.min.z);
                current.max.x = current.max.x.max(node_bounds.max.x);
                current.max.y = current.max.y.max(node_bounds.max.y);
                current.max.z = current.max.z.max(node_bounds.max.z);
            })
            .or_insert(*node_bounds);
    }
    output
}

fn reference(
    operation_index: Option<usize>,
    object_id: &str,
    pointer: String,
    message: &str,
) -> Diagnostic {
    Diagnostic {
        code: DiagnosticCode::ReferenceInvalid,
        operation_index,
        object_id: Some(object_id.to_owned()),
        pointer,
        message: message.into(),
    }
}

pub const MIN_OVERLAP_METERS: f32 = 0.1;
pub const SLAB_MIN_EXTENT_METERS: f32 = 0.5;
pub const MAX_OVERLAP_DIAGNOSTICS: usize = 50;
/// 重叠体积占较大实体体积的比例低于该值时视为贴边或室内设备，不上报。
pub const OVERLAP_VOLUME_RATIO_LIMIT: f32 = 0.15;

/// 全局空间重叠审计：warning 级，不参与事务校验。
///
/// 实体是源节点（派生部件 world bounds 的并集）。按顺序排除连接节点、
/// 薄板实体、祖先-后代对与单向包含（嵌套）；三轴交叠深度均超过阈值才报告，
/// 双向包含（实体重合）视为冲突上报。
pub fn validate_overlaps(document: &Document) -> Vec<Diagnostic> {
    let bounds_by_node = derived_source_bounds(document);
    if bounds_by_node.len() < 2 {
        return Vec::new();
    }
    let nodes_by_id: HashMap<crate::ids::NodeId, &SceneNode> = document
        .scene
        .nodes()
        .iter()
        .map(|node| (node.id.clone(), node))
        .collect();

    let mut entities: Vec<(&crate::ids::NodeId, Bounds)> = bounds_by_node
        .iter()
        .filter(|(id, _)| {
            nodes_by_id
                .get(*id)
                .is_some_and(|node| !matches!(node.content, NodeContent::Connection(_)))
        })
        .filter(|(_, bounds)| {
            let extent = |min: f32, max: f32| max - min;
            extent(bounds.min.x, bounds.max.x) >= SLAB_MIN_EXTENT_METERS
                && extent(bounds.min.y, bounds.max.y) >= SLAB_MIN_EXTENT_METERS
                && extent(bounds.min.z, bounds.max.z) >= SLAB_MIN_EXTENT_METERS
        })
        .map(|(id, bounds)| (id, *bounds))
        .collect();
    entities.sort_by_key(|(id, _)| (*id).clone());

    let mut overlaps: Vec<(crate::ids::NodeId, crate::ids::NodeId, [f32; 3])> = Vec::new();
    for left in 0..entities.len() {
        for right in left + 1..entities.len() {
            let (id_a, bounds_a) = &entities[left];
            let (id_b, bounds_b) = &entities[right];
            if is_ancestor_pair(id_a, id_b, &nodes_by_id) {
                continue;
            }
            // 单向包含是嵌套（池内水面、建筑内设备）；双向包含是实体重合，
            // 属于最严重的空间冲突，必须继续走深度检查上报。
            let a_contains_b = bounds_contains(bounds_a, bounds_b);
            let b_contains_a = bounds_contains(bounds_b, bounds_a);
            if a_contains_b != b_contains_a {
                continue;
            }
            let depth = overlap_depth(bounds_a, bounds_b);
            if depth[0] > MIN_OVERLAP_METERS
                && depth[1] > MIN_OVERLAP_METERS
                && depth[2] > MIN_OVERLAP_METERS
                && overlap_volume_ratio(bounds_a, bounds_b, depth) >= OVERLAP_VOLUME_RATIO_LIMIT
            {
                overlaps.push(((*id_a).clone(), (*id_b).clone(), depth));
            }
        }
    }

    let total = overlaps.len();
    let mut diagnostics: Vec<Diagnostic> = overlaps
        .into_iter()
        .take(MAX_OVERLAP_DIAGNOSTICS)
        .map(|(id_a, id_b, depth)| {
            let name_a = nodes_by_id
                .get(&id_a)
                .map(|node| node.name.as_str())
                .unwrap_or_else(|| id_a.as_str())
                .to_owned();
            let name_b = nodes_by_id
                .get(&id_b)
                .map(|node| node.name.as_str())
                .unwrap_or_else(|| id_b.as_str())
                .to_owned();
            overlap_diagnostic(
                &id_a,
                &id_b,
                &format!(
                    "spatial overlap between {name_a} and {name_b}: x={:.2}m y={:.2}m z={:.2}m",
                    depth[0], depth[1], depth[2]
                ),
            )
        })
        .collect();
    if total > MAX_OVERLAP_DIAGNOSTICS {
        diagnostics.push(Diagnostic {
            code: DiagnosticCode::GeometryOverlap,
            operation_index: None,
            object_id: None,
            pointer: "/scene".into(),
            message: format!(
                "scene overlap audit found {total} pairs; showing the first \
                 {MAX_OVERLAP_DIAGNOSTICS}"
            ),
        });
    }
    diagnostics
}

fn overlap_diagnostic(
    id_a: &crate::ids::NodeId,
    id_b: &crate::ids::NodeId,
    message: &str,
) -> Diagnostic {
    Diagnostic {
        code: DiagnosticCode::GeometryOverlap,
        operation_index: None,
        object_id: Some(format!("{}|{}", id_a.as_str(), id_b.as_str())),
        pointer: "/scene".into(),
        message: message.into(),
    }
}

fn is_ancestor_pair(
    id_a: &crate::ids::NodeId,
    id_b: &crate::ids::NodeId,
    nodes_by_id: &HashMap<crate::ids::NodeId, &SceneNode>,
) -> bool {
    let mut current = nodes_by_id
        .get(id_b)
        .and_then(|node| node.parent_id.clone());
    while let Some(parent) = current {
        if &parent == id_a {
            return true;
        }
        current = nodes_by_id
            .get(&parent)
            .and_then(|candidate| candidate.parent_id.clone());
    }
    let mut current = nodes_by_id
        .get(id_a)
        .and_then(|node| node.parent_id.clone());
    while let Some(parent) = current {
        if &parent == id_b {
            return true;
        }
        current = nodes_by_id
            .get(&parent)
            .and_then(|candidate| candidate.parent_id.clone());
    }
    false
}

fn bounds_contains(outer: &Bounds, inner: &Bounds) -> bool {
    outer.min.x <= inner.min.x
        && outer.min.y <= inner.min.y
        && outer.min.z <= inner.min.z
        && inner.max.x <= outer.max.x
        && inner.max.y <= outer.max.y
        && inner.max.z <= outer.max.z
}

fn overlap_depth(a: &Bounds, b: &Bounds) -> [f32; 3] {
    [
        a.max.x.min(b.max.x) - a.min.x.max(b.min.x),
        a.max.y.min(b.max.y) - a.min.y.max(b.min.y),
        a.max.z.min(b.max.z) - a.min.z.max(b.min.z),
    ]
}

fn overlap_volume_ratio(a: &Bounds, b: &Bounds, depth: [f32; 3]) -> f32 {
    let volume = |bounds: &Bounds| {
        (bounds.max.x - bounds.min.x)
            * (bounds.max.y - bounds.min.y)
            * (bounds.max.z - bounds.min.z)
    };
    let overlap = depth[0].max(0.0) * depth[1].max(0.0) * depth[2].max(0.0);
    let larger = volume(a).max(volume(b));
    if larger <= f32::EPSILON {
        return f32::INFINITY;
    }
    overlap / larger
}

fn geometry(
    operation_index: Option<usize>,
    object_id: &str,
    pointer: String,
    message: String,
) -> Diagnostic {
    Diagnostic {
        code: DiagnosticCode::GeometryInvalid,
        operation_index,
        object_id: Some(object_id.to_owned()),
        pointer,
        message,
    }
}

/// Texture references travel with the exported glTF, so they must be relative to
/// it: an absolute path or a remote URL would silently break on another machine.
fn texture_path_is_relative(path: &str) -> bool {
    !path.trim().is_empty()
        && !path.starts_with('/')
        && !path.starts_with('\\')
        && !path.contains("..")
        && !path.contains("://")
}
