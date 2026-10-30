use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use studio_core::ids::Revision;
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RevisionEvent {
    pub event: String,
    pub revision: Revision,
}
