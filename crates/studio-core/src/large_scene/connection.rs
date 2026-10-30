use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::{NodeId, RelationId};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum InstancePortRef {
    Instance {
        node_id: NodeId,
        port: String,
    },
    SetItem {
        set_node_id: NodeId,
        ordinal: u32,
        port: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum RoutePolicy {
    Direct,
    Orthogonal,
    Buried { depth: f32 },
    Rack { elevation: f32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ConnectionIntent {
    pub from: InstancePortRef,
    pub to: InstancePortRef,
    pub medium: String,
    pub diameter: f32,
    pub route_policy: RoutePolicy,
    pub relation_id: RelationId,
    pub metadata: BTreeMap<String, String>,
}
