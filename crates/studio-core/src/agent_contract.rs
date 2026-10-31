use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::geometry::Bounds;
use crate::ids::{ComponentId, MaterialId};

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
