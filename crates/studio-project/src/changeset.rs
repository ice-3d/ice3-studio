use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use studio_core::apply::ApplyResult;
use studio_core::diff::DocumentDiff;
use studio_core::document::Document;
use studio_core::ids::Revision;
use studio_core::operation::Operation;
use studio_core::validation::Diagnostic;
use thiserror::Error;
use ts_rs::TS;

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(transparent)]
#[ts(export)]
pub struct ChangeSetId(String);

impl ChangeSetId {
    pub fn parse(value: &str) -> Result<Self, ChangeSetError> {
        let Some(hex) = value.strip_prefix("changeset-") else {
            return Err(ChangeSetError::InvalidId);
        };
        if hex.len() == 16
            && hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            Ok(Self(value.into()))
        } else {
            Err(ChangeSetError::InvalidId)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(transparent)]
#[ts(export)]
pub struct EntryId(String);

impl EntryId {
    pub fn parse(value: &str) -> Result<Self, ChangeSetError> {
        let Some(number) = value.strip_prefix("entry-") else {
            return Err(ChangeSetError::InvalidEntryId);
        };
        if number.len() == 6 && number.bytes().all(|byte| byte.is_ascii_digit()) {
            Ok(Self(value.into()))
        } else {
            Err(ChangeSetError::InvalidEntryId)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ChangeSetStatus {
    Draft,
    Evaluating,
    Ready,
    Blocked,
    Stale,
    Committed,
    Discarded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ChangeSetSourceKind {
    Agent,
    Human,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ChangeSetSource {
    pub kind: ChangeSetSourceKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ChangeSetEntry {
    pub id: EntryId,
    pub source: ChangeSetSource,
    pub operation: Operation,
    pub created_at: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ChangeSet {
    pub schema_version: u32,
    pub id: ChangeSetId,
    pub project_id: String,
    pub base_revision: Revision,
    pub version: u64,
    pub status: ChangeSetStatus,
    pub title: String,
    pub entries: Vec<ChangeSetEntry>,
    pub candidate_digest: Option<String>,
    pub diagnostics: Vec<Diagnostic>,
    pub committed_revision: Option<Revision>,
    #[serde(default)]
    pub commit_result: Option<ApplyResult>,
    #[serde(default)]
    pub commit_idempotency_key: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

impl ChangeSet {
    pub fn new(
        id: ChangeSetId,
        project_id: impl Into<String>,
        base_revision: Revision,
        title: impl Into<String>,
        created_at: u64,
    ) -> Self {
        Self {
            schema_version: 1,
            id,
            project_id: project_id.into(),
            base_revision,
            version: 0,
            status: ChangeSetStatus::Draft,
            title: title.into(),
            entries: Vec::new(),
            candidate_digest: None,
            diagnostics: Vec::new(),
            committed_revision: None,
            commit_result: None,
            commit_idempotency_key: None,
            created_at,
            updated_at: created_at,
        }
    }

    pub fn add_entries(
        &mut self,
        expected_version: u64,
        mut entries: Vec<ChangeSetEntry>,
        updated_at: u64,
    ) -> Result<(), ChangeSetError> {
        self.require_mutable(expected_version)?;
        if entries.is_empty() {
            return Err(ChangeSetError::EmptyMutation);
        }
        let mut ids: std::collections::HashSet<_> =
            self.entries.iter().map(|entry| entry.id.clone()).collect();
        if entries.iter().any(|entry| !ids.insert(entry.id.clone())) {
            return Err(ChangeSetError::DuplicateEntryId);
        }
        self.entries.append(&mut entries);
        self.invalidate(updated_at);
        Ok(())
    }

    pub fn remove_entries(
        &mut self,
        expected_version: u64,
        entry_ids: &[EntryId],
        updated_at: u64,
    ) -> Result<(), ChangeSetError> {
        self.require_mutable(expected_version)?;
        if entry_ids.is_empty() {
            return Err(ChangeSetError::EmptyMutation);
        }
        let requested: std::collections::HashSet<_> = entry_ids.iter().collect();
        if !entry_ids
            .iter()
            .all(|id| self.entries.iter().any(|entry| entry.id == *id))
        {
            return Err(ChangeSetError::EntryNotFound);
        }
        self.entries.retain(|entry| !requested.contains(&entry.id));
        self.invalidate(updated_at);
        Ok(())
    }

    pub fn operations(&self) -> Vec<Operation> {
        self.entries
            .iter()
            .map(|entry| entry.operation.clone())
            .collect()
    }

    pub fn register_evaluation(
        &mut self,
        expected_version: u64,
        digest: Option<String>,
        diagnostics: Vec<Diagnostic>,
        updated_at: u64,
    ) -> Result<(), ChangeSetError> {
        if self.version != expected_version {
            return Err(ChangeSetError::VersionConflict {
                expected: expected_version,
                actual: self.version,
            });
        }
        self.status = if diagnostics.is_empty() {
            ChangeSetStatus::Ready
        } else {
            ChangeSetStatus::Blocked
        };
        self.candidate_digest = digest;
        self.diagnostics = diagnostics;
        self.updated_at = updated_at;
        Ok(())
    }

    pub fn mark_stale(&mut self, updated_at: u64) {
        self.status = ChangeSetStatus::Stale;
        self.updated_at = updated_at;
    }

    pub fn mark_committed(
        &mut self,
        result: ApplyResult,
        idempotency_key: String,
        updated_at: u64,
    ) {
        self.status = ChangeSetStatus::Committed;
        self.committed_revision = Some(result.revision);
        self.commit_result = Some(result);
        self.commit_idempotency_key = Some(idempotency_key);
        self.updated_at = updated_at;
    }

    pub fn discard(&mut self, updated_at: u64) {
        self.status = ChangeSetStatus::Discarded;
        self.updated_at = updated_at;
    }

    fn require_mutable(&self, expected_version: u64) -> Result<(), ChangeSetError> {
        if self.version != expected_version {
            return Err(ChangeSetError::VersionConflict {
                expected: expected_version,
                actual: self.version,
            });
        }
        if !matches!(
            self.status,
            ChangeSetStatus::Draft | ChangeSetStatus::Ready | ChangeSetStatus::Blocked
        ) {
            return Err(ChangeSetError::NotMutable);
        }
        Ok(())
    }

    fn invalidate(&mut self, updated_at: u64) {
        self.version += 1;
        self.status = ChangeSetStatus::Draft;
        self.candidate_digest = None;
        self.diagnostics.clear();
        self.updated_at = updated_at;
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ChangeSetEvaluation {
    pub change_set_id: ChangeSetId,
    pub version: u64,
    pub base_revision: Revision,
    pub status: ChangeSetStatus,
    pub candidate_digest: Option<String>,
    pub candidate: Option<Document>,
    pub diff: DocumentDiff,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Error)]
pub enum ChangeSetError {
    #[error("change set ID is invalid")]
    InvalidId,
    #[error("change set entry ID is invalid")]
    InvalidEntryId,
    #[error("change set version conflict: expected {expected}, actual {actual}")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("change set is not mutable in its current status")]
    NotMutable,
    #[error("change set mutation contains no entries")]
    EmptyMutation,
    #[error("change set entry ID is duplicated")]
    DuplicateEntryId,
    #[error("change set entry does not exist")]
    EntryNotFound,
}
