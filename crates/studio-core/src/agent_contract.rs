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
    /// 可调用全部读方法（context、search、evaluation、measure、BOM、clash、validation）。
    pub can_read: bool,
    /// 可创建 ChangeSet 并向其中追加候选操作，但不能落盘为 revision。
    pub can_propose: bool,
    /// 可提交已批准的候选；`review-required` 下只有人工审批令牌为 true。
    pub can_commit: bool,
    /// 可绕过 ChangeSet 直接写文档；只有显式配置的 `autonomous` 模式为 true。
    pub can_apply_directly: bool,
    /// 可丢弃候选变更集。
    pub can_discard: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentLimits {
    /// `scene.search` 省略 `limit` 时的返回条数，默认 25。
    pub search_default_limit: usize,
    /// `scene.search` 允许的最大 `limit`，超出返回 `ICE3_STUDIO_API_INVALID`，默认 100。
    pub search_max_limit: usize,
    /// 单次 `evaluation.query` 允许的节点 ID 数量上限，默认 32。
    pub evaluation_max_nodes: usize,
    /// 单次 JSON-RPC 请求体的字节上限，默认 64 MiB。
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
    /// 只承载子节点的分组，自身没有几何。
    Group,
    /// 参数化图元：盒、平面、圆柱、球。
    Primitive,
    /// 显式三角网格。
    Mesh,
    /// 沿路径扫掠得到的几何，管道与线缆走这一类。
    Sweep,
    /// 组件定义的实例，几何由定义及其参数展开。
    ComponentInstance,
    /// 按规则批量展开的实例集合，展开前只有一个声明。
    InstanceSet,
    /// 连接意图展开出的路径，派生 ID 形如 `<connection>/route`。
    Connection,
    /// 外部引用资产的派生实例，几何来自 Prepared GLB 而非文档。
    ReferenceInstance,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
#[ts(export)]
pub enum SceneSearchScope {
    /// 只检索源对象，即 Document 中真实存在的节点。
    #[default]
    Source,
    /// 只检索求值后的派生对象，包括展开的部件与管线路径。
    Derived,
    /// 同时检索两者；同一 ID 只返回一次。
    Both,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub enum SceneSearchField {
    /// 稳定节点 ID。投影时不可省略，它是后续所有读方法的入口。
    Id,
    /// 该条结果属于源对象还是派生对象。
    Scope,
    /// 内容类别，决定该节点有没有自身几何。
    Kind,
    /// 显示名，未命名对象为 null。
    Name,
    /// 父节点 ID，根节点为 null。
    ParentId,
    /// 所属业务实体的 ID；派生部件指向其宿主实例或连接。
    OwnerId,
    /// 是否参与渲染。
    Visible,
    /// 组件定义 ID，仅组件实例有值。
    ComponentDefinitionId,
    /// 材质 ID，未绑定材质时为空。
    MaterialId,
    /// 派生几何的内容寻址 ID，可直接喂给 `evaluation.query`。
    GeometryId,
    /// 世界空间包围盒，单位米。
    Bounds,
    /// 业务属性键值，内核不解释其含义。
    Metadata,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
#[ts(export)]
pub enum EvaluationInclude {
    /// 只返回计数、bounds 与拓扑/属性 digest，不返回顶点或索引数据。
    Summary,
    /// 追加顶点坐标数组；必须同时包含 `Summary`。
    Positions,
    /// 追加三角形索引数组；必须同时包含 `Summary`。
    Indices,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase", optional_fields = nullable)]
#[ts(export)]
pub struct SceneSearchRequest {
    /// 检索范围：源对象、派生对象或两者，默认 `source`。
    #[ts(as = "Option<_>", optional)]
    pub scope: SceneSearchScope,
    /// 对稳定 ID 与显示名做大小写不敏感的子串匹配。
    pub query: Option<String>,
    /// 精确匹配某个节点 ID；与其它过滤条件取交集。
    pub node_id: Option<String>,
    /// 只返回归属于该业务实体的节点。
    pub owner_id: Option<String>,
    /// 只返回该节点的直接子节点。
    pub parent_id: Option<String>,
    /// 业务属性的精确键值匹配，多个键之间为 AND。
    #[ts(as = "Option<_>", optional)]
    pub metadata: BTreeMap<String, String>,
    /// 只返回指定内容类别的节点。
    pub content_kind: Option<SceneContentKind>,
    /// 只返回使用该组件定义的实例。
    pub component_definition_id: Option<ComponentId>,
    /// 只返回绑定该材质的节点。
    pub material_id: Option<MaterialId>,
    /// 按可见性过滤。
    pub visible: Option<bool>,
    /// 只返回参与该类型 relation 的源节点；`scope=derived` 时无效。
    pub relation_type: Option<String>,
    /// 只返回与指定节点存在 relation 的源节点；`scope=derived` 时无效。
    pub related_node_id: Option<String>,
    /// 世界空间包围盒相交过滤，单位米。
    pub bounds: Option<Bounds>,
    /// 字段投影；`id`、`scope`、`kind` 必须保留，未知字段名会被拒绝。
    pub fields: Option<Vec<SceneSearchField>>,
    /// 不透明分页游标，绑定 revision 与查询摘要；跨 revision 使用会被拒绝。
    pub cursor: Option<String>,
    /// 返回条数，取值 1..=100，默认 25。
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
    /// Document 中源节点总数，不含展开结果。
    pub source_nodes: usize,
    /// 求值后展开的节点总数，包含组件部件与管线路径。
    pub derived_nodes: usize,
    /// 已注册的外部引用资产数量。
    pub reference_assets: usize,
    /// 求值后实际参与渲染的三角形总数。
    pub effective_triangles: u64,
    /// 去重后的唯一网格数量；实例共享几何时小于节点数。
    pub unique_meshes: usize,
    /// 组件定义（蓝图）数量。
    pub component_definitions: usize,
    /// 组件实例节点数量。
    pub instances: usize,
    /// 实例集合数量，集合内元素不单独计数。
    pub instance_sets: usize,
    /// typed relation 数量。
    pub relations: usize,
    /// 材质数量。
    pub materials: usize,
    /// 文档级约束标签数量，不代表几何约束已求解。
    pub constraints: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct SceneContentKindCount {
    /// 内容类别。
    pub kind: SceneContentKind,
    /// 该类别在检索范围内的节点数。
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct RelationTypeCount {
    /// relation 的类型标签，取值由项目自定义，内核不解释。
    pub relation_type: String,
    /// 该类型在文档中的出现次数。
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct SceneFacets {
    /// 按内容类别聚合的节点计数。
    pub content_kinds: Vec<SceneContentKindCount>,
    /// 按类型聚合的 relation 计数，最多 50 条，按类型名稳定排序。
    pub relation_types: Vec<RelationTypeCount>,
    /// relation 类型超过 50 种时为 true，表示清单被截断而非穷举。
    pub relation_types_truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct SceneCoordinateSystem {
    /// 手性，当前固定为 `right`。
    pub handedness: String,
    /// 上方向轴，当前固定为 `Y`。
    pub up_axis: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct SceneContext {
    /// 工程 ID。
    pub project_id: String,
    /// 本响应所代表的已提交 revision；后续分页与提交都以它为基准。
    pub revision: u64,
    /// API 版本字符串，仅用于诊断，不构成兼容性承诺。
    pub api_version: String,
    /// 文档 schema 版本。
    pub schema_version: u32,
    /// 长度单位，当前固定为 `meters`。
    pub units: String,
    /// 坐标约定。
    pub coordinate_system: SceneCoordinateSystem,
    /// 整个场景的世界空间包围盒，单位米；空场景为 null。
    pub bounds: Option<Bounds>,
    /// 源对象与派生对象的计数摘要。
    pub counts: SceneCounts,
    /// 内容类别与 relation 类型的聚合视图。
    pub facets: SceneFacets,
    /// 当前令牌的有效权限。
    pub access: AgentAccess,
    /// 本会话适用的响应与请求限额。
    pub limits: AgentLimits,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", optional_fields)]
#[ts(export)]
pub struct SceneSearchItem {
    /// 稳定节点 ID；派生对象的 ID 形如 `<instance>/<part>` 或 `<connection>/route`。
    pub id: String,
    /// 该条结果属于源对象还是派生对象。
    pub scope: SceneSearchScope,
    /// 内容类别，决定该节点是否有自身几何。
    pub kind: SceneContentKind,
    /// 显示名，未命名对象为 null。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 父节点 ID，根节点为 null。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// 所属业务实体 ID；派生部件指向宿主实例或连接。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,
    /// 是否参与渲染。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    /// 组件定义 ID，仅组件实例与实例集合有值。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component_definition_id: Option<ComponentId>,
    /// 材质 ID，未绑定材质时为 null。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub material_id: Option<MaterialId>,
    /// 内容寻址的派生几何 ID；纯分组节点为 null。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry_id: Option<String>,
    /// 世界空间包围盒，单位米。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds: Option<Bounds>,
    /// 业务属性键值，内核不解释其含义。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct SceneSearchResult {
    /// 本页所代表的已提交 revision；翻页期间 revision 前进会导致游标失效。
    pub revision: u64,
    /// 按 `(scope, id)` 稳定排序的结果页。
    pub items: Vec<SceneSearchItem>,
    /// 下一页游标；为 null 表示已到末页。
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct EvaluationQueryRequest {
    /// 1 到 32 个唯一的派生节点 ID；任一 ID 未知会使整个请求失败。
    pub node_ids: Vec<String>,
    /// 需要返回的内容，默认只返回 `summary`；请求几何数据时必须包含它。
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
    /// 请求中给出的派生节点 ID。
    pub node_id: String,
    /// 内容寻址几何 ID；纯分组节点为 null。
    pub geometry_id: Option<String>,
    /// 顶点数量。
    pub vertex_count: usize,
    /// 三角形索引数量，通常等于三角形数乘以 3。
    pub index_count: usize,
    /// 该节点求值后的世界空间包围盒，单位米。
    pub bounds: Option<Bounds>,
    /// 拓扑内容的稳定摘要；用于判断两次求值的拓扑是否一致。
    pub topology_digest: Option<String>,
    /// 顶点属性（位置、法线等）的稳定摘要。
    pub attribute_digest: Option<String>,
    /// 顶点坐标数组，仅在请求中包含 `positions` 时返回。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub positions: Option<Vec<[f32; 3]>>,
    /// 三角形索引数组，仅在请求中包含 `indices` 时返回。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub indices: Option<Vec<u32>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct EvaluationQueryResult {
    /// 求值所基于的已提交 revision。
    pub revision: u64,
    /// 与请求 `node_ids` 顺序一一对应的结果。
    pub items: Vec<EvaluationQueryItem>,
}

pub const MAX_PART_SUMMARIES: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct ComponentGetParams {
    /// 组件定义 ID，通常来自 `scene.search` 返回的 `componentDefinitionId`。
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
    /// 只承载子部件的分组，没有自身几何。
    Group,
    /// 轴对齐长方体。
    Box,
    /// 单面平面，常用于地面或标牌。
    Plane,
    /// 圆柱，轴向可通过参数指定。
    Cylinder,
    /// 球体。
    Sphere,
    /// 显式三角网格。
    CustomMesh,
    /// 沿点序列路径扫掠的管状几何。
    Sweep,
    /// 二维闭合轮廓沿 +Z 拉伸成的棱柱。
    Extrude,
    /// 带 12 条边倒角/圆角的盒体。
    RoundedBox,
    /// 引用另一个组件定义，展开时递归求值。
    ComponentReference,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct ComponentPartSummary {
    /// 部件在所属定义内的稳定 ID。
    pub id: String,
    /// 父部件 ID，顶层部件为 null。
    pub parent_id: Option<String>,
    /// 部件几何类别。
    pub kind: ComponentPartKind,
    /// 被引用的组件定义 ID，仅 `component_reference` 部件有值。
    pub referenced_definition_id: Option<ComponentId>,
    /// `repeat` 展开前的声明次数；null 表示不重复。
    pub repeat: Option<u32>,
    /// 是否参与渲染。
    pub visible: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct ComponentDefinitionSummary {
    /// 组件定义 ID。
    pub id: ComponentId,
    /// 显示名。
    pub name: String,
    /// 参数 schema：名称、类型、取值范围或枚举、默认值与单位。
    pub parameters: Vec<crate::large_scene::component::ComponentParameter>,
    /// 端口清单：所属部件、位置、方向、介质与管径，供连接提案使用。
    pub ports: Vec<crate::large_scene::component::ComponentPort>,
    /// 部件层级摘要，最多 100 条。
    pub parts: Vec<ComponentPartSummary>,
    /// 部件条目总数，`repeat` 不展开。
    pub part_count: usize,
    /// 部件超过 100 条时为 true，表示摘要被截断。
    pub parts_truncated: bool,
    /// 当前 revision 中以该定义为蓝本的实例与实例集合数量之和。
    pub instance_count: usize,
    /// 定义级业务属性，内核不解释。
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct ComponentGetResult {
    /// 读取所基于的已提交 revision。
    pub revision: u64,
    /// 有界定义摘要。
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
    /// 是否附带全场景空间重叠审计，默认 false。重叠只产生 `GEOMETRY_OVERLAP`
    /// warning，不会让事务失败；精确三角级碰撞请用 `scene.clashes`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub include_overlaps: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
#[ts(export)]
pub enum AgentSideEffect {
    /// 只读，不改变文档或缓存。
    Read,
    /// 写入候选变更集，不推进 revision。
    Proposal,
    /// 需要审批后落盘为 revision。
    Commit,
    /// 绕过 ChangeSet 直接写文档，仅 `autonomous` 模式可用。
    DirectWrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
#[ts(export)]
pub enum AgentIdempotency {
    /// 没有幂等保护；超时重试可能产生重复副作用。
    None,
    /// 必须携带幂等键，同一键的相同请求返回首次结果，不同内容复用会被拒绝。
    Required,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentTool {
    /// 工具名，与 JSON-RPC 方法名一致。
    pub name: String,
    /// 一句话说明该工具做什么、返回什么。
    pub summary: String,
    /// 副作用类别，决定能否在只读会话里调用。
    pub side_effect: AgentSideEffect,
    /// 是否必须经过人工审批才能产生效果。
    pub requires_approval: bool,
    /// 幂等约束；`required` 时必须携带幂等键。
    pub idempotency: AgentIdempotency,
    /// 当前令牌能否调用；false 时不要尝试。
    pub available: bool,
    /// `available=false` 的原因。
    pub unavailable_reason: Option<String>,
    /// 指向 `schemaUrl` 中入参定义的 JSON Pointer。
    pub input_schema_ref: String,
    /// 指向出参定义的 JSON Pointer；无结构化返回时为 null。
    pub output_schema_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentWorkflow {
    /// 工作流名：inspect、propose、review、commit、import-reference、recover。
    pub name: String,
    /// 工具名序列；`a|b` 表示二者择一，顺序即推荐调用顺序。
    pub steps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentManifest {
    /// API 版本字符串，仅用于诊断。
    pub api_version: String,
    /// Agent 契约版本，当前为 `1.0`。
    pub contract_version: String,
    /// 可下载的 JSON Schema 地址。
    pub schema_url: String,
    /// 可下载的 TypeScript 声明地址。
    pub types_url: String,
    /// 当前令牌的有效权限。
    pub access: AgentAccess,
    /// 响应与请求限额。
    pub limits: AgentLimits,
    /// 可用工具清单，含副作用、审批、幂等与 schema 引用。
    pub tools: Vec<AgentTool>,
    /// 推荐的调用序列，只描述顺序，不含自然语言提示。
    pub workflows: Vec<AgentWorkflow>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct ChangeImpactCounts {
    /// 新增数量。
    pub added: usize,
    /// 修改数量。
    pub updated: usize,
    /// 删除数量。
    pub removed: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
#[ts(export)]
pub enum IterationImpactScope {
    /// 只影响直接改动的对象，增量求值即可。
    Local,
    /// 影响有限的一组依赖对象，仍可增量求值。
    Targeted,
    /// 影响面较大，需要重建多个分区或组件。
    Broad,
    /// 必须全量重建场景。
    FullRebuild,
    /// 候选阻塞或过期，无法给出预测。
    Unavailable,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
#[ts(export)]
pub enum IterationImpactReason {
    /// 节点内容（几何、名称、可见性）发生变化。
    NodeContent,
    /// 节点或实例的变换矩阵发生变化，可走 CPU 增量缓存。
    NodeTransform,
    /// 实例参数发生变化，需要重新展开该实例。
    InstanceParameters,
    /// 材质绑定发生变化。
    MaterialAssignment,
    /// 组件定义发生变化，所有使用该定义的实例都受影响。
    ComponentDefinition,
    /// relation 拓扑发生变化。
    RelationTopology,
    /// 约束标签发生变化。
    Constraint,
    /// 需要全量重建，无法增量复用。
    FullRebuild,
    /// 候选校验失败，影响无法评估。
    ValidationBlocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct IterationImpact {
    /// 影响范围档位；`unavailable` 表示候选阻塞或过期，无法给出预测。
    pub scope: IterationImpactScope,
    /// 是否必须重建整个场景；为 true 时本次迭代的代价与全量求值相当。
    pub full_scene_rebuild_required: bool,
    /// 源节点的增删改计数。
    pub source_nodes: ChangeImpactCounts,
    /// 派生节点（展开部件与管线路径）的增删改计数。
    pub derived_nodes: ChangeImpactCounts,
    /// relation 的增删改计数。
    pub relations: ChangeImpactCounts,
    /// 组件定义的增删改计数。
    pub components: ChangeImpactCounts,
    /// 材质的增删改计数。
    pub materials: ChangeImpactCounts,
    /// 受影响的源节点总数。
    pub affected_source_nodes: usize,
    /// 受影响的派生节点总数。
    pub affected_derived_nodes: usize,
    /// 复用已有缓存的几何数量，不重新求值。
    pub reused_geometry: usize,
    /// 需要重新生成的几何数量。
    pub generated_geometry: usize,
    /// 内容寻址几何缓存命中次数。
    pub cache_hits: usize,
    /// 内容寻址几何缓存未命中次数。
    pub cache_misses: usize,
    /// 判定影响范围的具体原因，按稳定顺序返回。
    pub reasons: Vec<IterationImpactReason>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase", optional_fields = nullable)]
#[ts(export)]
pub struct RuntimePreviewRequest {
    /// 已提交 revision；与 `candidateDigest` 二选一，省略时取当前 revision。
    pub revision: Option<u64>,
    /// 候选变更集的 digest；给出时预览该未提交候选，而不是已提交 revision。
    pub candidate_digest: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
#[ts(export)]
pub enum AgentErrorCategory {
    /// 参数或文档内容不合法；修正入参后可重试。
    Validation,
    /// revision 或版本冲突；必须先重新读取当前状态。
    Conflict,
    /// 引用的对象不存在；需要重新检索有效 ID。
    NotFound,
    /// 当前令牌缺少所需权限；换令牌或转人工。
    Permission,
    /// 超出条数、字节或节点数限额；缩小请求范围后重试。
    Limit,
    /// 能力当前不可用（如审批、预览或模式未开启）；稍后或换路径。
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentErrorData {
    /// 失败类别，决定恢复路径。
    pub category: AgentErrorCategory,
    /// 相同请求原样重试是否可能成功；不表示应当重试。
    pub retryable: bool,
    /// 原子事务中出错操作的序号，从 0 开始。
    pub operation_index: Option<usize>,
    /// 相关的稳定对象 ID。
    pub object_id: Option<String>,
    /// 指向无效文档字段的 RFC 6901 JSON Pointer。
    pub pointer: Option<String>,
    /// 期望值，例如要求的 revision 或字段类型。
    #[ts(type = "unknown | null")]
    pub expected: Option<serde_json::Value>,
    /// 实际收到的值。
    #[ts(type = "unknown | null")]
    pub actual: Option<serde_json::Value>,
    /// 简短的声明式恢复动作，不含自然语言提示或模型指令。
    pub suggestions: Vec<String>,
}

/// One endpoint of a measurement: either a scene node (its world bounds) or an
/// explicit world point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase", optional_fields = nullable)]
#[ts(export)]
pub struct MeasurePoint {
    /// 以节点世界包围盒中心作为端点；与 `point` 二选一。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub node_id: Option<String>,
    /// 直接给出世界坐标点，单位米；与 `nodeId` 二选一。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub point: Option<[f32; 3]>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct MeasureRequest {
    /// 起点，节点或世界坐标点。
    pub from: MeasurePoint,
    /// 终点，节点或世界坐标点。
    pub to: MeasurePoint,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct MeasureResult {
    /// 解析后的起点世界坐标，单位米。
    pub from: [f32; 3],
    /// 解析后的终点世界坐标，单位米。
    pub to: [f32; 3],
    /// 两端点的直线距离，单位米。
    pub distance: f32,
    /// 终点相对起点的逐轴差值，单位米。
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
    /// 分组依据：`definition`（默认）、`material`、`owner` 或 `metadata:<key>`。
    pub group_by: Option<String>,
    /// 返回行数上限，默认 200，最大 1000。
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
    /// 该组所有部件的世界空间合并包围盒，单位米。
    pub bounds: Option<Bounds>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct BomResult {
    /// 按分组聚合的行，按 `key` 稳定排序。
    pub rows: Vec<BomRow>,
    /// 分组总数，可能大于 `rows` 的长度。
    pub total: usize,
    /// 行数被 `limit` 截断时为 true。
    pub truncated: bool,
    /// 实际生效的分组依据。
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
    /// 净空阈值，单位米；间距小于该值的部件对会被报告，默认 0。
    pub clearance: Option<f32>,
    /// 比较粒度：`owners`（按设备聚合）或 `parts`（按部件），默认 `owners`。
    pub scope: Option<String>,
    /// 是否执行三角级窄相位；关闭时只做包围盒判断，更快但可能误报。
    pub precise: Option<bool>,
    /// 是否把恰好接触（gap 为 0）计入结果，默认 false。
    pub include_touching: Option<bool>,
    /// 只比较这些设备及其部件；省略表示全场景。
    pub owner_ids: Option<Vec<String>>,
    /// Triangle pairs examined per candidate in the narrow phase. Raise it when
    /// big meshes come back `confirmed: null`; it is the only cost knob.
    pub triangle_budget: Option<usize>,
    /// 返回的干涉对数量上限，默认 200，最大 1000。
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export)]
pub struct ClashPair {
    /// 第一个部件的派生节点 ID。
    pub a: String,
    /// 第二个部件的派生节点 ID。
    pub b: String,
    /// 第一个部件所属的源实例。
    pub a_owner: String,
    /// 第二个部件所属的源实例。
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
    /// 结果被 `limit` 截断时为 true。
    pub truncated: bool,
    /// 本次采用的净空阈值，单位米。
    pub clearance: f32,
    /// 本次采用的比较粒度。
    pub scope: String,
    /// 是否执行了三角级窄相位。
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
