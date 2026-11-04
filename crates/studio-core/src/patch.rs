use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::document::MaterialSurface;
use crate::ids::{ComponentId, MaterialId, NodeId};
use crate::large_scene::component::ParameterValue;
use crate::large_scene::connection::{InstancePortRef, RoutePolicy};
use crate::math::Transform;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase", optional_fields = nullable)]
#[ts(export)]
pub struct InstancePatch {
    /// 要修改的组件实例或实例集合节点 ID。
    pub node_id: NodeId,
    /// 新的显示名；省略或 null 表示不修改。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 新的局部变换；省略或 null 表示不修改。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<Transform>,
    /// 是否参与渲染；省略表示不修改。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    /// 切换到另一个组件定义；省略表示不修改。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition_id: Option<ComponentId>,
    /// 只替换列出的参数值，未列出的参数保持不变。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(as = "Option<_>", optional)]
    pub set_parameters: BTreeMap<String, ParameterValue>,
    /// 要删除的参数名；与 `setParameters` 出现同名条目时校验失败。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<_>", optional)]
    pub remove_parameters: Vec<String>,
    /// 合并写入的业务属性，不覆盖未列出的键。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(as = "Option<_>", optional)]
    pub set_metadata: BTreeMap<String, String>,
    /// 要删除的属性键；键不存在时不报错。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<_>", optional)]
    pub remove_metadata_keys: Vec<String>,
    /// 绑定材质；省略或 null 表示不修改。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material_id: Option<MaterialId>,
    /// 为 true 时清除材质绑定；与 `materialId` 同时给出为非法组合。
    #[serde(default)]
    #[ts(as = "Option<_>", optional)]
    pub clear_material: bool,
}

impl InstancePatch {
    pub fn has_input(&self) -> bool {
        self.name.is_some()
            || self.transform.is_some()
            || self.visible.is_some()
            || self.definition_id.is_some()
            || !self.set_parameters.is_empty()
            || !self.remove_parameters.is_empty()
            || !self.set_metadata.is_empty()
            || !self.remove_metadata_keys.is_empty()
            || self.material_id.is_some()
            || self.clear_material
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase", optional_fields = nullable)]
#[ts(export)]
pub struct MaterialPatch {
    /// 要修改的材质 ID。
    pub material_id: MaterialId,
    /// 新的显示名；省略表示不修改。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 基础色 RGBA，各分量 0..=1。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_color: Option<[f32; 4]>,
    /// 不透明度，0..=1；小于 1 时导出为 alpha blend。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f32>,
    /// 描边颜色 RGBA；与 `clearEdgeColor` 同时给出为非法组合。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge_color: Option<[f32; 4]>,
    /// 为 true 时清除描边颜色；与 `edgeColor` 同时给出为非法组合。
    #[serde(default)]
    #[ts(as = "Option<_>", optional)]
    pub clear_edge_color: bool,
    /// Replaces the whole surface description; use `clearSurface` to go back to
    /// the built-in dielectric defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<MaterialSurface>,
    /// 为 true 时把表面恢复为内置电介质默认值。
    #[serde(default)]
    #[ts(as = "Option<_>", optional)]
    pub clear_surface: bool,
}

impl MaterialPatch {
    pub fn has_input(&self) -> bool {
        self.name.is_some()
            || self.base_color.is_some()
            || self.opacity.is_some()
            || self.edge_color.is_some()
            || self.surface.is_some()
            || self.clear_surface
            || self.clear_edge_color
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase", optional_fields = nullable)]
#[ts(export)]
pub struct ComponentPatch {
    /// 要修改的组件定义 ID。
    pub component_id: ComponentId,
    /// 新的显示名；省略表示不修改。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 只覆盖列出的参数默认值；已显式设置该参数的实例不受影响。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(as = "Option<_>", optional)]
    pub set_parameter_defaults: BTreeMap<String, ParameterValue>,
    /// 合并写入的定义级业务属性。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(as = "Option<_>", optional)]
    pub set_metadata: BTreeMap<String, String>,
    /// 要删除的属性键；键不存在时不报错。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<_>", optional)]
    pub remove_metadata_keys: Vec<String>,
}

impl ComponentPatch {
    pub fn has_input(&self) -> bool {
        self.name.is_some()
            || !self.set_parameter_defaults.is_empty()
            || !self.set_metadata.is_empty()
            || !self.remove_metadata_keys.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase", optional_fields = nullable)]
#[ts(export)]
pub struct ConnectionPatch {
    /// 要修改的连接节点 ID。
    pub node_id: NodeId,
    /// 起点端口引用；省略表示不修改。端点变更会重新校验 relation。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<InstancePortRef>,
    /// 终点端口引用；省略表示不修改。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<InstancePortRef>,
    /// 介质标签（水、污泥、沼气等），内核只按其分组，不解释语义。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub medium: Option<String>,
    /// 管径，单位米。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diameter: Option<f32>,
    /// 路由策略，决定扫掠路径的取点与弯头约束。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_policy: Option<RoutePolicy>,
    /// 合并写入的业务属性。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(as = "Option<_>", optional)]
    pub set_metadata: BTreeMap<String, String>,
    /// 要删除的属性键；键不存在时不报错。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<_>", optional)]
    pub remove_metadata_keys: Vec<String>,
}

impl ConnectionPatch {
    pub fn has_input(&self) -> bool {
        self.from.is_some()
            || self.to.is_some()
            || self.medium.is_some()
            || self.diameter.is_some()
            || self.route_policy.is_some()
            || !self.set_metadata.is_empty()
            || !self.remove_metadata_keys.is_empty()
    }
}
