use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::geometry::{Bounds, EvaluatedMesh};
use crate::ids::{ComponentId, MaterialId};
use crate::large_scene::component::ComponentDefinition;
use crate::large_scene::evaluate::{EvaluatedScene, evaluated_scene_bounds};

pub const DEFAULT_SEARCH_LIMIT: usize = 25;
pub const MAX_SEARCH_LIMIT: usize = 100;
pub const MAX_EVALUATION_NODES: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentAccess {
    pub can_read: bool,
    pub can_propose: bool,
    pub can_commit: bool,
    pub can_apply_directly: bool,
    pub can_discard: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentLimits {
    pub search_default_limit: usize,
    pub search_max_limit: usize,
    pub evaluation_max_nodes: usize,
    pub max_rpc_body_bytes: u64,
}

impl Default for AgentLimits {
    fn default() -> Self {
        Self {
            search_default_limit: DEFAULT_SEARCH_LIMIT,
            search_max_limit: MAX_SEARCH_LIMIT,
            evaluation_max_nodes: MAX_EVALUATION_NODES,
            max_rpc_body_bytes: 64 * 1024 * 1024,
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
#[ts(export)]
pub enum SceneContentKind {
    Group,
    Primitive,
    Mesh,
    Sweep,
    ComponentInstance,
    InstanceSet,
    Connection,
    ReferenceInstance,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
#[ts(export)]
pub enum SceneSearchScope {
    #[default]
    Source,
    Derived,
    Both,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub enum SceneSearchField {
    Id,
    Scope,
    Kind,
    Name,
    ParentId,
    OwnerId,
    Visible,
    ComponentDefinitionId,
    MaterialId,
    GeometryId,
    Bounds,
    Metadata,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
#[ts(export)]
pub enum EvaluationInclude {
    Summary,
    Positions,
    Indices,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase", optional_fields = nullable)]
#[ts(export)]
pub struct SceneSearchRequest {
    #[ts(as = "Option<_>", optional)]
    pub scope: SceneSearchScope,
    pub query: Option<String>,
    pub node_id: Option<String>,
    pub owner_id: Option<String>,
    pub parent_id: Option<String>,
    #[ts(as = "Option<_>", optional)]
    pub metadata: BTreeMap<String, String>,
    pub content_kind: Option<SceneContentKind>,
    pub component_definition_id: Option<ComponentId>,
    pub material_id: Option<MaterialId>,
    pub visible: Option<bool>,
    pub relation_type: Option<String>,
    pub related_node_id: Option<String>,
    pub bounds: Option<Bounds>,
    pub fields: Option<Vec<SceneSearchField>>,
    pub cursor: Option<String>,
    #[ts(as = "Option<_>", optional)]
    pub limit: usize,
}

impl Default for SceneSearchRequest {
    fn default() -> Self {
        Self {
            scope: SceneSearchScope::Source,
            query: None,
            node_id: None,
            owner_id: None,
            parent_id: None,
            metadata: BTreeMap::new(),
            content_kind: None,
            component_definition_id: None,
            material_id: None,
            visible: None,
            relation_type: None,
            related_node_id: None,
            bounds: None,
            fields: None,
            cursor: None,
            limit: DEFAULT_SEARCH_LIMIT,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct SceneCounts {
    pub source_nodes: usize,
    pub derived_nodes: usize,
    pub reference_assets: usize,
    pub effective_triangles: u64,
    pub unique_meshes: usize,
    pub component_definitions: usize,
    pub instances: usize,
    pub instance_sets: usize,
    pub relations: usize,
    pub materials: usize,
    pub constraints: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct SceneContentKindCount {
    pub kind: SceneContentKind,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct RelationTypeCount {
    pub relation_type: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct SceneFacets {
    pub content_kinds: Vec<SceneContentKindCount>,
    pub relation_types: Vec<RelationTypeCount>,
    pub relation_types_truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct SceneCoordinateSystem {
    pub handedness: String,
    pub up_axis: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct SceneContext {
    pub project_id: String,
    pub revision: u64,
    pub api_version: String,
    pub schema_version: u32,
    pub units: String,
    pub coordinate_system: SceneCoordinateSystem,
    pub bounds: Option<Bounds>,
    pub counts: SceneCounts,
    pub facets: SceneFacets,
    pub access: AgentAccess,
    pub limits: AgentLimits,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", optional_fields)]
#[ts(export)]
pub struct SceneSearchItem {
    pub id: String,
    pub scope: SceneSearchScope,
    pub kind: SceneContentKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component_definition_id: Option<ComponentId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub material_id: Option<MaterialId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds: Option<Bounds>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct SceneSearchResult {
    pub revision: u64,
    pub items: Vec<SceneSearchItem>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct EvaluationQueryRequest {
    pub node_ids: Vec<String>,
    #[serde(default = "default_evaluation_include")]
    #[ts(as = "Option<_>", optional)]
    pub include: Vec<EvaluationInclude>,
}

fn default_evaluation_include() -> Vec<EvaluationInclude> {
    vec![EvaluationInclude::Summary]
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct EvaluationQueryItem {
    pub node_id: String,
    pub geometry_id: Option<String>,
    pub vertex_count: usize,
    pub index_count: usize,
    pub bounds: Option<Bounds>,
    pub topology_digest: Option<String>,
    pub attribute_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub positions: Option<Vec<[f32; 3]>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub indices: Option<Vec<u32>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct EvaluationQueryResult {
    pub revision: u64,
    pub items: Vec<EvaluationQueryItem>,
}

pub const MAX_PART_SUMMARIES: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct ComponentGetParams {
    pub component_id: String,
    /// 显式请求完整定义（含全部 parts binding），供 `component.update` 修改回写。
    /// 缺省只返回有界摘要。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub include_parts: Option<bool>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
#[ts(export)]
pub enum ComponentPartKind {
    Group,
    Box,
    Plane,
    Cylinder,
    Sphere,
    CustomMesh,
    Sweep,
    Extrude,
    RoundedBox,
    ComponentReference,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct ComponentPartSummary {
    pub id: String,
    pub parent_id: Option<String>,
    pub kind: ComponentPartKind,
    pub referenced_definition_id: Option<ComponentId>,
    pub repeat: Option<u32>,
    pub visible: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct ComponentDefinitionSummary {
    pub id: ComponentId,
    pub name: String,
    pub parameters: Vec<crate::large_scene::component::ComponentParameter>,
    pub ports: Vec<crate::large_scene::component::ComponentPort>,
    pub parts: Vec<ComponentPartSummary>,
    pub part_count: usize,
    pub parts_truncated: bool,
    pub instance_count: usize,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct ComponentGetResult {
    pub revision: u64,
    pub component: ComponentDefinitionSummary,
    /// `includeParts: true` 时的完整定义，与 `component.update` 的 payload 同构。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub definition: Option<ComponentDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct ValidationRunRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub include_overlaps: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
#[ts(export)]
pub enum AgentSideEffect {
    Read,
    Proposal,
    Commit,
    DirectWrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
#[ts(export)]
pub enum AgentIdempotency {
    None,
    Required,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentTool {
    pub name: String,
    pub summary: String,
    pub side_effect: AgentSideEffect,
    pub requires_approval: bool,
    pub idempotency: AgentIdempotency,
    pub available: bool,
    pub unavailable_reason: Option<String>,
    pub input_schema_ref: String,
    pub output_schema_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentWorkflow {
    pub name: String,
    pub steps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentManifest {
    pub api_version: String,
    pub contract_version: String,
    pub schema_url: String,
    pub types_url: String,
    pub access: AgentAccess,
    pub limits: AgentLimits,
    pub tools: Vec<AgentTool>,
    pub workflows: Vec<AgentWorkflow>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct ChangeImpactCounts {
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
#[ts(export)]
pub enum IterationImpactScope {
    Local,
    Targeted,
    Broad,
    FullRebuild,
    Unavailable,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
#[ts(export)]
pub enum IterationImpactReason {
    NodeContent,
    NodeTransform,
    InstanceParameters,
    MaterialAssignment,
    ComponentDefinition,
    RelationTopology,
    Constraint,
    FullRebuild,
    ValidationBlocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct IterationImpact {
    pub scope: IterationImpactScope,
    pub full_scene_rebuild_required: bool,
    pub source_nodes: ChangeImpactCounts,
    pub derived_nodes: ChangeImpactCounts,
    pub relations: ChangeImpactCounts,
    pub components: ChangeImpactCounts,
    pub materials: ChangeImpactCounts,
    pub affected_source_nodes: usize,
    pub affected_derived_nodes: usize,
    pub reused_geometry: usize,
    pub generated_geometry: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub reasons: Vec<IterationImpactReason>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase", optional_fields = nullable)]
#[ts(export)]
pub struct RuntimePreviewRequest {
    pub revision: Option<u64>,
    pub candidate_digest: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
#[ts(export)]
pub enum AgentErrorCategory {
    Validation,
    Conflict,
    NotFound,
    Permission,
    Limit,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentErrorData {
    pub category: AgentErrorCategory,
    pub retryable: bool,
    pub operation_index: Option<usize>,
    pub object_id: Option<String>,
    pub pointer: Option<String>,
    #[ts(type = "unknown | null")]
    pub expected: Option<serde_json::Value>,
    #[ts(type = "unknown | null")]
    pub actual: Option<serde_json::Value>,
    pub suggestions: Vec<String>,
}

/// One endpoint of a measurement: either a scene node (its world bounds) or an
/// explicit world point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase", optional_fields = nullable)]
#[ts(export)]
pub struct MeasurePoint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub node_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub point: Option<[f32; 3]>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct MeasureRequest {
    pub from: MeasurePoint,
    pub to: MeasurePoint,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct MeasureResult {
    pub from: [f32; 3],
    pub to: [f32; 3],
    pub distance: f32,
    pub delta: [f32; 3],
    /// Surface-to-surface gap when both endpoints are nodes (0 when their
    /// world bounds overlap); equal to `distance` when either is a free point.
    pub clearance: f32,
}

/// Measure the distance between two endpoints. Node endpoints resolve to their
/// world-bounds centre; `clearance` additionally reports the AABB gap.
pub fn measure_request(
    request: &MeasureRequest,
    bounds: &BTreeMap<String, Bounds>,
) -> Result<MeasureResult, String> {
    let (from, from_bounds) = resolve_measure_point(&request.from, bounds)?;
    let (to, to_bounds) = resolve_measure_point(&request.to, bounds)?;
    let delta = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    let distance = (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt();
    let clearance = match (from_bounds, to_bounds) {
        (Some(a), Some(b)) => bounds_gap(&a, &b),
        _ => distance,
    };
    Ok(MeasureResult {
        from,
        to,
        distance,
        delta,
        clearance,
    })
}

fn resolve_measure_point(
    point: &MeasurePoint,
    bounds: &BTreeMap<String, Bounds>,
) -> Result<([f32; 3], Option<Bounds>), String> {
    match (&point.node_id, point.point) {
        (Some(id), None) => {
            let world = bounds
                .get(id)
                .ok_or_else(|| format!("node not found: {id}"))?;
            Ok((bounds_centre(world), Some(*world)))
        }
        (None, Some(position)) => Ok((position, None)),
        _ => Err("measure point must set exactly one of nodeId or point".to_string()),
    }
}

fn bounds_centre(bounds: &Bounds) -> [f32; 3] {
    [
        (bounds.min.x + bounds.max.x) * 0.5,
        (bounds.min.y + bounds.max.y) * 0.5,
        (bounds.min.z + bounds.max.z) * 0.5,
    ]
}

fn bounds_gap(a: &Bounds, b: &Bounds) -> f32 {
    let axis_gap = |a_min: f32, a_max: f32, b_min: f32, b_max: f32| {
        (b_min - a_max).max(a_min - b_max).max(0.0)
    };
    let x = axis_gap(a.min.x, a.max.x, b.min.x, b.max.x);
    let y = axis_gap(a.min.y, a.max.y, b.min.y, b.max.y);
    let z = axis_gap(a.min.z, a.max.z, b.min.z, b.max.z);
    (x * x + y * y + z * z).sqrt()
}

pub const DEFAULT_BOM_LIMIT: usize = 200;
pub const MAX_BOM_LIMIT: usize = 1000;

/// Bill-of-materials request. `groupBy` is `definition` (default), `material`,
/// `owner`, or `metadata:<key>`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct BomRequest {
    pub group_by: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct BomRow {
    /// Raw grouping id (definition id, material id, owner id or metadata value).
    pub key: String,
    /// Human-readable label (definition/material name, metadata value).
    pub label: String,
    /// Evaluated part nodes in this group.
    pub count: usize,
    /// Distinct source instances in this group.
    pub owners: usize,
    /// Summed mesh volume (m³), scaled by the node scale.
    pub volume: f32,
    pub bounds: Option<Bounds>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct BomResult {
    pub rows: Vec<BomRow>,
    pub total: usize,
    pub truncated: bool,
    pub group_by: String,
}

/// Aggregate the evaluated scene into a bill of materials.
pub fn build_bom(
    document: &crate::document::Document,
    scene: &EvaluatedScene,
    request: &BomRequest,
) -> BomResult {
    let group_by = request
        .group_by
        .clone()
        .unwrap_or_else(|| "definition".to_string());
    let limit = request
        .limit
        .unwrap_or(DEFAULT_BOM_LIMIT)
        .clamp(1, MAX_BOM_LIMIT);

    let definitions: BTreeMap<&str, &str> = document
        .components
        .iter()
        .map(|definition| (definition.id.as_str(), definition.name.as_str()))
        .collect();
    let materials: BTreeMap<&str, &str> = document
        .materials
        .iter()
        .map(|material| (material.id.as_str(), material.name.as_str()))
        .collect();
    let definition_by_owner: BTreeMap<&str, &str> = document
        .scene
        .nodes()
        .iter()
        .filter_map(|node| match &node.content {
            crate::document::NodeContent::ComponentInstance(instance) => {
                Some((node.id.as_str(), instance.definition_id.as_str()))
            }
            _ => None,
        })
        .collect();

    let node_bounds = evaluated_scene_bounds(scene);
    let mut mesh_volumes: BTreeMap<&str, f32> = BTreeMap::new();
    let mut groups: BTreeMap<String, BomAccumulator> = BTreeMap::new();

    for node in &scene.nodes {
        let (key, label) = bom_key(
            node,
            &group_by,
            &definitions,
            &materials,
            &definition_by_owner,
        );
        let accumulator = groups.entry(key).or_default();
        accumulator.label = label;
        accumulator.count += 1;
        accumulator.owners.insert(node.owner_id.clone());
        if let Some(mesh_key) = &node.mesh_key {
            let volume = *mesh_volumes
                .entry(mesh_key.as_str())
                .or_insert_with(|| scene.meshes.get(mesh_key).map(mesh_volume).unwrap_or(0.0));
            let scale = node.transform.scale;
            accumulator.volume += volume * (scale.x * scale.y * scale.z).abs();
        }
        if let Some(bounds) = node_bounds.get(&node.id) {
            accumulator.include(bounds);
        }
    }

    let mut rows: Vec<BomRow> = groups
        .into_iter()
        .map(|(key, accumulator)| BomRow {
            key,
            label: accumulator.label,
            count: accumulator.count,
            owners: accumulator.owners.len(),
            volume: accumulator.volume,
            bounds: accumulator.bounds,
        })
        .collect();
    rows.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.key.cmp(&right.key))
    });
    let total = rows.len();
    let truncated = total > limit;
    rows.truncate(limit);
    BomResult {
        rows,
        total,
        truncated,
        group_by,
    }
}

#[derive(Default)]
struct BomAccumulator {
    label: String,
    count: usize,
    owners: BTreeSet<String>,
    volume: f32,
    bounds: Option<Bounds>,
}

impl BomAccumulator {
    fn include(&mut self, bounds: &Bounds) {
        self.bounds = Some(match self.bounds {
            None => *bounds,
            Some(current) => Bounds {
                min: crate::math::Vec3 {
                    x: current.min.x.min(bounds.min.x),
                    y: current.min.y.min(bounds.min.y),
                    z: current.min.z.min(bounds.min.z),
                },
                max: crate::math::Vec3 {
                    x: current.max.x.max(bounds.max.x),
                    y: current.max.y.max(bounds.max.y),
                    z: current.max.z.max(bounds.max.z),
                },
            },
        });
    }
}

fn bom_key(
    node: &crate::large_scene::evaluate::EvaluatedSceneNode,
    group_by: &str,
    definitions: &BTreeMap<&str, &str>,
    materials: &BTreeMap<&str, &str>,
    definition_by_owner: &BTreeMap<&str, &str>,
) -> (String, String) {
    if let Some(metadata_key) = group_by.strip_prefix("metadata:") {
        let value = node
            .metadata
            .get(metadata_key)
            .cloned()
            .unwrap_or_else(|| "unassigned".to_string());
        return (value.clone(), value);
    }
    match group_by {
        "material" => {
            let id = node
                .material_id
                .as_ref()
                .map(|material| material.as_str().to_string())
                .unwrap_or_else(|| "unassigned".to_string());
            let label = materials
                .get(id.as_str())
                .map(|name| (*name).to_string())
                .unwrap_or_else(|| id.clone());
            (id, label)
        }
        "owner" => (node.owner_id.clone(), node.owner_id.clone()),
        _ => {
            let id = definition_by_owner
                .get(node.owner_id.as_str())
                .map(|definition| (*definition).to_string())
                .unwrap_or_else(|| "primitive".to_string());
            let label = definitions
                .get(id.as_str())
                .map(|name| (*name).to_string())
                .unwrap_or_else(|| id.clone());
            (id, label)
        }
    }
}

fn mesh_volume(mesh: &EvaluatedMesh) -> f32 {
    let mut total: f32 = 0.0;
    for triangle in mesh.indices.as_chunks::<3>().0 {
        let a = mesh.vertices[triangle[0] as usize].position;
        let b = mesh.vertices[triangle[1] as usize].position;
        let c = mesh.vertices[triangle[2] as usize].position;
        total += (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
            + a[2] * (b[0] * c[1] - b[1] * c[0]))
            / 6.0;
    }
    total.abs()
}

pub const DEFAULT_CLASH_LIMIT: usize = 100;
pub const MAX_CLASH_LIMIT: usize = 1000;
/// Default number of triangle pairs examined for one clash candidate.
pub const DEFAULT_CLASH_TRIANGLE_BUDGET: usize = 20_000;
pub const MAX_CLASH_TRIANGLE_BUDGET: usize = 4_000_000;
/// Upper bound on the candidates that enter the narrow phase at all.
const MAX_PRECISE_CANDIDATES: usize = 256;

/// Clash / clearance query over the evaluated scene.
///
/// `clearance` is a distance in metres: `0` (default) reports penetrations only,
/// anything larger also reports parts that come that close. `scope` is `owners`
/// (default, compare parts belonging to different source instances) or `parts`
/// (also compare parts of the same instance). `precise` asks for a triangle
/// narrow phase on penetrating candidates. `ownerIds` restricts the audit to
/// those source instances.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct ClashRequest {
    pub clearance: Option<f32>,
    pub scope: Option<String>,
    pub precise: Option<bool>,
    pub include_touching: Option<bool>,
    pub owner_ids: Option<Vec<String>>,
    /// Triangle pairs examined per candidate in the narrow phase. Raise it when
    /// big meshes come back `confirmed: null`; it is the only cost knob.
    pub triangle_budget: Option<usize>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct ClashPair {
    /// Evaluated part node id of the first part.
    pub a: String,
    pub b: String,
    /// Source instance that owns each part.
    pub a_owner: String,
    pub b_owner: String,
    /// Signed separation in metres: negative is the penetration depth.
    pub gap: f32,
    /// True when the boxes penetrate, so `gap` is a depth rather than a gap.
    pub penetrating: bool,
    /// Narrow-phase verdict: `null` when `precise` was not requested or the
    /// triangle budget ran out.
    pub confirmed: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct ClashResult {
    /// Worst offenders first: deepest penetration, then closest approach.
    pub pairs: Vec<ClashPair>,
    /// Total pair count before `limit` was applied.
    pub total: usize,
    pub truncated: bool,
    pub clearance: f32,
    pub scope: String,
    pub precise: bool,
    /// Evaluated parts that entered the broad phase.
    pub checked_parts: usize,
    /// Candidates that the sweep produced before the scope filter.
    pub candidates: usize,
}

/// Detect clashes and clearance violations between evaluated parts.
pub fn build_clash_report(scene: &EvaluatedScene, request: &ClashRequest) -> ClashResult {
    let clearance = request.clearance.unwrap_or(0.0).max(0.0);
    let scope = request
        .scope
        .clone()
        .unwrap_or_else(|| "owners".to_string());
    let precise = request.precise.unwrap_or(false);
    let include_touching = request.include_touching.unwrap_or(false);
    let limit = request
        .limit
        .unwrap_or(DEFAULT_CLASH_LIMIT)
        .clamp(1, MAX_CLASH_LIMIT);
    let triangle_budget = request
        .triangle_budget
        .unwrap_or(DEFAULT_CLASH_TRIANGLE_BUDGET)
        .clamp(1, MAX_CLASH_TRIANGLE_BUDGET);
    let owners: Option<BTreeSet<&str>> = request
        .owner_ids
        .as_ref()
        .map(|ids| ids.iter().map(|id| id.as_str()).collect::<BTreeSet<&str>>());

    let matrices = crate::large_scene::evaluate::evaluated_world_matrices(scene);
    let bounds = crate::large_scene::evaluate::evaluated_scene_bounds_with(scene, &matrices);
    let parts: Vec<&crate::large_scene::evaluate::EvaluatedSceneNode> = scene
        .nodes
        .iter()
        .filter(|node| node.visible && node.mesh_key.is_some())
        .filter(|node| {
            owners
                .as_ref()
                .is_none_or(|allowed| allowed.contains(node.owner_id.as_str()))
        })
        .filter(|node| bounds.contains_key(node.id.as_str()))
        .collect();

    let boxes: Vec<Bounds> = parts.iter().map(|node| bounds[node.id.as_str()]).collect();
    let candidates = crate::geometry::sweep_pairs(&boxes, clearance);
    let candidate_count = candidates.len();

    let mut reported: Vec<(f32, usize, usize)> = Vec::new();
    for (left, right) in candidates {
        if scope != "parts" && parts[left].owner_id == parts[right].owner_id {
            continue;
        }
        let gap = crate::geometry::signed_gap(&boxes[left], &boxes[right]);
        if !clash_reported(gap, clearance, include_touching) {
            continue;
        }
        reported.push((gap, left, right));
    }
    // Worst offenders first, so the narrow-phase budget is spent on the pairs
    // that actually matter rather than on sweep order.
    reported.sort_by(|left, right| {
        left.0
            .partial_cmp(&right.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| parts[left.1].id.cmp(&parts[right.1].id))
            .then_with(|| parts[left.2].id.cmp(&parts[right.2].id))
    });

    let mut precise_candidates = 0usize;
    let mut pairs: Vec<ClashPair> = Vec::new();
    for (gap, left, right) in reported {
        let a = parts[left];
        let b = parts[right];
        let mut confirmed = None;
        if precise && gap < 0.0 && precise_candidates < MAX_PRECISE_CANDIDATES {
            precise_candidates += 1;
            let verdict = crate::geometry::meshes_intersect(
                &world_triangles(scene, a, &matrices),
                &world_triangles(scene, b, &matrices),
                triangle_budget,
            );
            if verdict == Some(false) {
                // Bounding volumes overlapped, geometry did not: not a clash.
                continue;
            }
            confirmed = verdict;
        }
        pairs.push(ClashPair {
            a: a.id.clone(),
            b: b.id.clone(),
            a_owner: a.owner_id.clone(),
            b_owner: b.owner_id.clone(),
            gap,
            penetrating: gap < 0.0,
            confirmed,
        });
    }

    let total = pairs.len();
    let truncated = total > limit;
    pairs.truncate(limit);

    ClashResult {
        pairs,
        total,
        truncated,
        clearance,
        scope,
        precise,
        checked_parts: parts.len(),
        candidates: candidate_count,
    }
}

/// A pair is reported when it penetrates, when it touches and the caller asked
/// for touching pairs, or when it sits inside the requested clearance band.
fn clash_reported(gap: f32, clearance: f32, include_touching: bool) -> bool {
    crate::geometry::within_clearance(gap, clearance, include_touching)
}

/// World-space triangles of an evaluated part, used by the narrow phase.
fn world_triangles(
    scene: &EvaluatedScene,
    node: &crate::large_scene::evaluate::EvaluatedSceneNode,
    matrices: &BTreeMap<String, glam::Mat4>,
) -> Vec<[[f32; 3]; 3]> {
    let Some(mesh_key) = node.mesh_key.as_ref() else {
        return Vec::new();
    };
    let Some(mesh) = scene.meshes.get(mesh_key) else {
        return Vec::new();
    };
    let matrix = matrices
        .get(node.id.as_str())
        .copied()
        .unwrap_or(glam::Mat4::IDENTITY);
    let mut triangles = Vec::with_capacity(mesh.indices.len() / 3);
    for triangle in mesh.indices.as_chunks::<3>().0 {
        let mut points = [[0.0f32; 3]; 3];
        for (slot, index) in triangle.iter().enumerate() {
            let position = mesh.vertices[*index as usize].position;
            let point = matrix.transform_point3(glam::Vec3::from_array(position));
            points[slot] = point.to_array();
        }
        triangles.push(points);
    }
    triangles
}
