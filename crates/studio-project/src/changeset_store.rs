use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::ProjectError;
use crate::changeset::{ChangeSet, ChangeSetId};
use crate::store::write_atomic;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeSetStoreDiagnostic {
    pub path: String,
    pub message: String,
}

/// 变更集写方法的幂等记录：同一个键配同一份请求只执行一次。
///
/// 记录落在 `changesets/idempotency/keys.json`，而不是 `changesets/*.json`，
/// 因为 `list()` 会把目录下所有 `.json` 当作变更集读取。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeSetIdempotencyRecord {
    pub key: String,
    pub request_digest: String,
    pub change_set_id: ChangeSetId,
}

pub struct ChangeSetStore {
    directory: PathBuf,
    diagnostics: Mutex<Vec<ChangeSetStoreDiagnostic>>,
}

impl ChangeSetStore {
    pub fn new(project_dir: &Path) -> Result<Self, ProjectError> {
        let directory = project_dir.join("changesets");
        std::fs::create_dir_all(&directory).map_err(ProjectError::Io)?;
        Ok(Self {
            directory,
            diagnostics: Mutex::new(Vec::new()),
        })
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn diagnostics(&self) -> Vec<ChangeSetStoreDiagnostic> {
        self.diagnostics.lock().expect("diagnostics lock").clone()
    }

    pub fn save(&self, change_set: &ChangeSet) -> Result<(), ProjectError> {
        let bytes = serde_json::to_vec(change_set).map_err(ProjectError::Json)?;
        write_atomic(&self.path_for(&change_set.id), &bytes)
    }

    pub fn load(&self, id: &ChangeSetId) -> Result<ChangeSet, ProjectError> {
        let bytes = std::fs::read(self.path_for(id)).map_err(ProjectError::Io)?;
        serde_json::from_slice(&bytes).map_err(ProjectError::Json)
    }

    pub fn list(&self) -> Result<Vec<ChangeSet>, ProjectError> {
        let mut diagnostics = self.diagnostics.lock().expect("diagnostics lock");
        diagnostics.clear();
        let mut change_sets: Vec<ChangeSet> = Vec::new();
        for entry in std::fs::read_dir(&self.directory).map_err(ProjectError::Io)? {
            let entry = entry.map_err(ProjectError::Io)?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            match std::fs::read(&path)
                .map_err(ProjectError::Io)
                .and_then(|bytes| serde_json::from_slice(&bytes).map_err(ProjectError::Json))
            {
                Ok(change_set) => change_sets.push(change_set),
                Err(error) => diagnostics.push(ChangeSetStoreDiagnostic {
                    path: path.display().to_string(),
                    message: error.to_string(),
                }),
            }
        }
        change_sets.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(change_sets)
    }

    /// 查找某个幂等键的既有结果。文件不存在视为从未使用过该键。
    pub fn idempotency_lookup(
        &self,
        key: &str,
    ) -> Result<Option<ChangeSetIdempotencyRecord>, ProjectError> {
        let path = self.idempotency_path();
        if !path.exists() {
            return Ok(None);
        }
        let records = self.read_idempotency()?;
        Ok(records.into_iter().find(|record| record.key == key))
    }

    /// 记录一次成功的写操作。同一个键只保留首条记录。
    pub fn idempotency_remember(
        &self,
        record: ChangeSetIdempotencyRecord,
    ) -> Result<(), ProjectError> {
        let path = self.idempotency_path();
        let mut records = self.read_idempotency()?;
        if records.iter().any(|existing| existing.key == record.key) {
            return Ok(());
        }
        records.push(record);
        records.sort_by(|left, right| left.key.cmp(&right.key));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(ProjectError::Io)?;
        }
        let bytes = serde_json::to_vec(&records).map_err(ProjectError::Json)?;
        write_atomic(&path, &bytes)
    }

    fn read_idempotency(&self) -> Result<Vec<ChangeSetIdempotencyRecord>, ProjectError> {
        let path = self.idempotency_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let bytes = std::fs::read(&path).map_err(ProjectError::Io)?;
        serde_json::from_slice(&bytes).map_err(ProjectError::Json)
    }

    fn path_for(&self, id: &ChangeSetId) -> PathBuf {
        self.directory.join(format!("{}.json", id.as_str()))
    }

    fn idempotency_path(&self) -> PathBuf {
        self.directory.join("idempotency").join("keys.json")
    }
}
