use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use studio_core::ids::Revision;
use studio_project::changeset::{ChangeSetId, ChangeSetStatus};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "event", rename_all_fields = "camelCase")]
#[ts(export)]
pub enum ServerEvent {
    #[serde(rename = "revision.committed")]
    RevisionCommitted { revision: Revision },
    #[serde(rename = "changeset.created")]
    ChangeSetCreated {
        change_set_id: ChangeSetId,
        version: u64,
        status: ChangeSetStatus,
    },
    #[serde(rename = "changeset.updated")]
    ChangeSetUpdated {
        change_set_id: ChangeSetId,
        version: u64,
        status: ChangeSetStatus,
    },
    #[serde(rename = "changeset.evaluated")]
    ChangeSetEvaluated {
        change_set_id: ChangeSetId,
        version: u64,
        status: ChangeSetStatus,
        candidate_digest: Option<String>,
    },
    #[serde(rename = "changeset.committed")]
    ChangeSetCommitted {
        change_set_id: ChangeSetId,
        version: u64,
        revision: Revision,
        candidate_digest: String,
    },
    #[serde(rename = "changeset.discarded")]
    ChangeSetDiscarded {
        change_set_id: ChangeSetId,
        version: u64,
    },
}
