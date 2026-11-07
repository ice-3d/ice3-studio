use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use studio_core::apply::ApplyResult;
use studio_core::diff::DocumentDiff;
use studio_core::document::Document;
use studio_core::ids::Revision;
use studio_core::operation::Operation;
use studio_core::spatial_preflight::SpatialPreflight;
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
    /// 提案来源类别，区分 Agent、人工表单与导入。
    pub kind: ChangeSetSourceKind,
    /// 来源的可读标签，例如 Agent 名称或表单入口，仅用于审计展示。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ChangeSetEntry {
    /// 条目 ID，删除时按它定位。
    pub id: EntryId,
    /// 该条操作的提出者。
    pub source: ChangeSetSource,
    /// 类型化建模操作，与 `document.apply` 的 operation 同构。
    pub operation: Operation,
    /// 创建时间，Unix 毫秒。
    pub created_at: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ChangeSet {
    /// 变更集持久化格式版本。
    pub schema_version: u32,
    /// 变更集 ID。
    pub id: ChangeSetId,
    /// 所属工程 ID。
    pub project_id: String,
    /// 创建时的基线 revision；当前 revision 前进后本变更集会变为 stale。
    pub base_revision: Revision,
    /// 乐观并发版本，每次增删条目递增；写入时必须回传 `expectedVersion`。
    pub version: u64,
    /// 生命周期状态：draft、evaluating、ready、blocked、stale、committed、discarded。
    pub status: ChangeSetStatus,
    /// 人类可读标题，用于审阅列表。
    pub title: String,
    /// 候选操作条目。
    pub entries: Vec<ChangeSetEntry>,
    /// 求值通过后生成的候选摘要；提交时必须原样回传作为审批凭据。
    pub candidate_digest: Option<String>,
    /// 最近一次求值产生的诊断。
    pub diagnostics: Vec<Diagnostic>,
    /// 已提交时对应的 revision。
    pub committed_revision: Option<Revision>,
    /// 已提交时的事务结果。
    #[serde(default)]
    pub commit_result: Option<ApplyResult>,
    /// 提交时使用的幂等键，供审计与重试比对。
    #[serde(default)]
    pub commit_idempotency_key: Option<String>,
    /// 创建时间，Unix 毫秒。
    pub created_at: u64,
    /// 最近更新时间，Unix 毫秒。
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
    /// 被求值的变更集 ID。
    pub change_set_id: ChangeSetId,
    /// 求值时变更集的版本。
    pub version: u64,
    /// 求值所基于的基线 revision。
    pub base_revision: Revision,
    /// 求值后的状态；`ready` 才允许提交。
    pub status: ChangeSetStatus,
    /// 候选摘要；与审批时回传的 `approvedDigest` 必须一致。
    pub candidate_digest: Option<String>,
    /// 完整候选文档，仅在 `includeCandidate=true` 时返回；体积可能较大。
    pub candidate: Option<Document>,
    /// 候选相对基线的结构化差异。
    pub diff: DocumentDiff,
    /// 校验诊断；存在 error 时状态为 `blocked`。
    pub diagnostics: Vec<Diagnostic>,
    /// 候选的空间预检摘要；用于审阅时判断改动落在哪里、是否跑出场景。
    /// 它是信息性的，不影响 `status`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spatial: Option<SpatialPreflight>,
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
