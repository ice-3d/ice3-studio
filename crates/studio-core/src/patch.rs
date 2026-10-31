use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::{ComponentId, MaterialId, NodeId};
use crate::large_scene::component::ParameterValue;
use crate::large_scene::connection::{InstancePortRef, RoutePolicy};
use crate::math::Transform;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase", optional_fields = nullable)]
#[ts(export)]
pub struct InstancePatch {
    pub node_id: NodeId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<Transform>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition_id: Option<ComponentId>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(as = "Option<_>", optional)]
    pub set_parameters: BTreeMap<String, ParameterValue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<_>", optional)]
    pub remove_parameters: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(as = "Option<_>", optional)]
    pub set_metadata: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<_>", optional)]
    pub remove_metadata_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material_id: Option<MaterialId>,
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
    pub material_id: MaterialId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_color: Option<[f32; 4]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge_color: Option<[f32; 4]>,
    #[serde(default)]
    #[ts(as = "Option<_>", optional)]
    pub clear_edge_color: bool,
}

impl MaterialPatch {
    pub fn has_input(&self) -> bool {
        self.name.is_some()
            || self.base_color.is_some()
            || self.opacity.is_some()
            || self.edge_color.is_some()
            || self.clear_edge_color
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase", optional_fields = nullable)]
#[ts(export)]
pub struct ComponentPatch {
    pub component_id: ComponentId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(as = "Option<_>", optional)]
    pub set_parameter_defaults: BTreeMap<String, ParameterValue>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(as = "Option<_>", optional)]
    pub set_metadata: BTreeMap<String, String>,
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
    pub node_id: NodeId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<InstancePortRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<InstancePortRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub medium: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diameter: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_policy: Option<RoutePolicy>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(as = "Option<_>", optional)]
    pub set_metadata: BTreeMap<String, String>,
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
