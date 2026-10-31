use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use studio_core::PROJECT_SCHEMA_VERSION;
use studio_core::document::Document;
use studio_core::ids::Revision;
use thiserror::Error;

use crate::WorkspacePath;
use crate::changeset_store::ChangeSetStore;
use crate::lock::{LockError, ProjectLock};

/// 保留的撤销步数上限。每个 revision 都会写一份完整快照，历史上限决定
/// `revisions/` 的磁盘上界；超出后最早的快照会被回收，对应撤销记录同时失效。
pub const MAX_UNDO_DEPTH: usize = 200;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub revision: Revision,
    pub digest: String,
    #[serde(default)]
    pub event: serde_json::Value,
}

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("project already exists")]
    AlreadyExists,
    #[error("project does not exist or has incomplete storage")]
    Missing,
    #[error("project is locked by another writer")]
    Locked,
    #[error("project is closed")]
    Closed,
    #[error("project schema is unsupported")]
    UnsupportedSchema,
    #[error("project revision is invalid")]
    InvalidRevision,
    #[error("project snapshot digest does not match")]
    DigestMismatch,
    #[error("project history is empty")]
    EmptyHistory,
    #[error("project history is inconsistent")]
    InvalidHistory,
    #[error("project storage operation failed")]
    Io(#[source] std::io::Error),
    #[error("project JSON is invalid")]
    Json(#[source] serde_json::Error),
}

impl From<LockError> for ProjectError {
    fn from(value: LockError) -> Self {
        match value {
            LockError::AlreadyLocked => Self::Locked,
            LockError::Io(error) => Self::Io(error),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectMetadata {
    schema_version: u32,
    project_id: String,
    current_revision: Revision,
    snapshot_digest: String,
    undo_stack: Vec<Revision>,
    redo_stack: Vec<Revision>,
}

pub struct ProjectStore {
    path: WorkspacePath,
    lock: Option<ProjectLock>,
}

impl ProjectStore {
    pub fn create(path: WorkspacePath) -> Result<Self, ProjectError> {
        if path.project.exists() {
            return Err(ProjectError::AlreadyExists);
        }
        fs::create_dir_all(path.project.join("revisions")).map_err(ProjectError::Io)?;
        fs::create_dir_all(path.project.join("history")).map_err(ProjectError::Io)?;
        fs::create_dir_all(path.project.join("resources")).map_err(ProjectError::Io)?;
        fs::create_dir_all(path.project.join("changesets")).map_err(ProjectError::Io)?;
        fs::create_dir_all(path.project.join("references")).map_err(ProjectError::Io)?;
        fs::create_dir_all(path.project.join("cache/preview")).map_err(ProjectError::Io)?;
        fs::create_dir_all(path.project.join("cache/geometry")).map_err(ProjectError::Io)?;
        let lock = ProjectLock::acquire(ProjectLock::path_for(&path.project))?;
        Ok(Self {
            path,
            lock: Some(lock),
        })
    }

    pub fn open(path: WorkspacePath) -> Result<Self, ProjectError> {
        if !path.project.join("project.json").is_file() {
            return Err(ProjectError::Missing);
        }
        let lock = ProjectLock::acquire(ProjectLock::path_for(&path.project))?;
        let store = Self {
            path,
            lock: Some(lock),
        };
        std::fs::create_dir_all(store.path.project.join("changesets")).map_err(ProjectError::Io)?;
        std::fs::create_dir_all(store.path.project.join("references")).map_err(ProjectError::Io)?;
        std::fs::create_dir_all(store.path.project.join("cache/geometry"))
            .map_err(ProjectError::Io)?;
        store.validate_history()?;
        Ok(store)
    }

    pub fn load_current(&self) -> Result<Document, ProjectError> {
        self.ensure_open()?;
        let metadata = self.read_metadata()?;
        self.read_revision_with_digest(metadata.current_revision, &metadata.snapshot_digest)
    }

    pub fn project_dir(&self) -> &Path {
        self.path.project_dir()
    }

    pub fn change_set_store(&self) -> ChangeSetStore {
        ChangeSetStore::new(self.project_dir()).expect("project changeset directory")
    }

    pub fn load_revision(&self, revision: Revision) -> Result<Document, ProjectError> {
        self.ensure_open()?;
        let history = self.read_history()?;
        let entry = history
            .into_iter()
            .find(|entry| entry.revision == revision)
            .ok_or(ProjectError::InvalidRevision)?;
        self.read_revision_with_digest(revision, &entry.digest)
    }

    pub fn close(&mut self) {
        self.lock = None;
    }

    pub fn is_open(&self) -> bool {
        self.lock.is_some()
    }

    pub fn write_snapshot(&self, document: &Document) -> Result<(), ProjectError> {
        self.write_snapshot_with_history(document, &serde_json::json!({ "kind": "snapshot" }))
    }

    pub fn write_snapshot_with_history(
        &self,
        document: &Document,
        event: &serde_json::Value,
    ) -> Result<(), ProjectError> {
        self.ensure_open()?;
        let previous = self.read_metadata().ok();
        if previous.is_none() && document.revision != Revision::new(0) {
            return Err(ProjectError::InvalidRevision);
        }
        if previous
            .as_ref()
            .is_some_and(|metadata| document.revision <= metadata.current_revision)
        {
            return Err(ProjectError::InvalidRevision);
        }
        let mut undo_stack = previous
            .as_ref()
            .map(|metadata| metadata.undo_stack.clone())
            .unwrap_or_default();
        if let Some(metadata) = &previous {
            undo_stack.push(metadata.current_revision);
        }
        if undo_stack.len() > MAX_UNDO_DEPTH {
            undo_stack.drain(..undo_stack.len() - MAX_UNDO_DEPTH);
        }
        let metadata = self.write_revision(document, undo_stack, Vec::new(), event)?;
        self.prune_snapshots(&metadata)?;
        self.write_metadata(&metadata)
    }

    pub fn undo(&self) -> Result<Document, ProjectError> {
        self.ensure_open()?;
        let metadata = self.read_metadata()?;
        let target = *metadata
            .undo_stack
            .last()
            .ok_or(ProjectError::EmptyHistory)?;
        let mut restored = self.read_revision_with_digest(target, "")?;
        let current = metadata.current_revision;
        restored.revision = current.next();
        let next_metadata = self.write_revision(
            &restored,
            metadata.undo_stack[..metadata.undo_stack.len() - 1].to_vec(),
            vec![current],
            &serde_json::json!({
                "kind": "undo",
                "restoredFrom": target.as_u64()
            }),
        )?;
        self.prune_snapshots(&next_metadata)?;
        self.write_metadata(&next_metadata)?;
        Ok(restored)
    }

    pub fn redo(&self) -> Result<Document, ProjectError> {
        self.ensure_open()?;
        let metadata = self.read_metadata()?;
        let target = *metadata
            .redo_stack
            .last()
            .ok_or(ProjectError::EmptyHistory)?;
        let mut restored = self.read_revision_with_digest(target, "")?;
        let current = metadata.current_revision;
        restored.revision = current.next();
        let mut undo_stack = metadata.undo_stack.clone();
        undo_stack.push(current);
        let next_metadata = self.write_revision(
            &restored,
            undo_stack,
            metadata.redo_stack[..metadata.redo_stack.len() - 1].to_vec(),
            &serde_json::json!({
                "kind": "redo",
                "restoredFrom": target.as_u64()
            }),
        )?;
        self.prune_snapshots(&next_metadata)?;
        self.write_metadata(&next_metadata)?;
        Ok(restored)
    }

    pub fn history(&self) -> Result<Vec<HistoryEntry>, ProjectError> {
        self.ensure_open()?;
        self.read_history()
    }

    fn read_history(&self) -> Result<Vec<HistoryEntry>, ProjectError> {
        let path = self.path.project.join("history/operations.jsonl");
        if !path.exists() {
            return Ok(Vec::new());
        }
        let body = fs::read_to_string(path).map_err(ProjectError::Io)?;
        body.lines()
            .map(|line| serde_json::from_str(line).map_err(ProjectError::Json))
            .collect()
    }

    fn write_revision(
        &self,
        document: &Document,
        undo_stack: Vec<Revision>,
        redo_stack: Vec<Revision>,
        event: &serde_json::Value,
    ) -> Result<ProjectMetadata, ProjectError> {
        if document.schema_version != PROJECT_SCHEMA_VERSION {
            return Err(ProjectError::UnsupportedSchema);
        }
        let bytes = serde_json::to_vec(document).map_err(ProjectError::Json)?;
        let digest = format!("{:x}", Sha256::digest(&bytes));
        let revision_path = self.revision_path(document.revision);
        write_atomic(&revision_path, &bytes)?;
        let history_entry = HistoryEntry {
            revision: document.revision,
            digest: digest.clone(),
            event: event.clone(),
        };
        let history_entry = serde_json::to_string(&history_entry).map_err(ProjectError::Json)?;
        append_history(
            &self.path.project.join("history/operations.jsonl"),
            &history_entry,
        )?;
        Ok(ProjectMetadata {
            schema_version: PROJECT_SCHEMA_VERSION,
            project_id: document.project_id.clone(),
            current_revision: document.revision,
            snapshot_digest: digest,
            undo_stack,
            redo_stack,
        })
    }

    fn read_metadata(&self) -> Result<ProjectMetadata, ProjectError> {
        let path = self.path.project.join("project.json");
        let bytes = fs::read(path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                ProjectError::Missing
            } else {
                ProjectError::Io(error)
            }
        })?;
        let metadata: ProjectMetadata =
            serde_json::from_slice(&bytes).map_err(ProjectError::Json)?;
        if metadata.schema_version != 1 && metadata.schema_version != PROJECT_SCHEMA_VERSION {
            return Err(ProjectError::UnsupportedSchema);
        }
        Ok(metadata)
    }

    fn validate_history(&self) -> Result<(), ProjectError> {
        let metadata = self.read_metadata()?;
        let history = self.read_history()?;
        if history.is_empty() {
            return Err(ProjectError::EmptyHistory);
        }
        for (index, entry) in history.iter().enumerate() {
            if entry.revision != Revision::new(index as u64) {
                return Err(ProjectError::InvalidHistory);
            }
        }
        // 旧快照会被 prune_snapshots 回收，因此只校验仍被引用的 revision。
        let mut required = std::collections::HashSet::new();
        required.insert(metadata.current_revision);
        required.extend(metadata.undo_stack.iter().copied());
        required.extend(metadata.redo_stack.iter().copied());
        for entry in history
            .iter()
            .filter(|entry| required.contains(&entry.revision))
        {
            self.read_revision_with_digest(entry.revision, &entry.digest)?;
        }
        let Some(last) = history.last() else {
            return Err(ProjectError::EmptyHistory);
        };
        if last.revision != metadata.current_revision || last.digest != metadata.snapshot_digest {
            return Err(ProjectError::InvalidHistory);
        }
        Ok(())
    }

    fn ensure_open(&self) -> Result<(), ProjectError> {
        if self.lock.is_some() {
            Ok(())
        } else {
            Err(ProjectError::Closed)
        }
    }

    fn read_revision_with_digest(
        &self,
        revision: Revision,
        expected_digest: &str,
    ) -> Result<Document, ProjectError> {
        let bytes = fs::read(self.revision_path(revision)).map_err(ProjectError::Io)?;
        if !expected_digest.is_empty() {
            let digest = format!("{:x}", Sha256::digest(&bytes));
            if digest != expected_digest {
                return Err(ProjectError::DigestMismatch);
            }
        }
        let document: Document = serde_json::from_slice(&bytes).map_err(ProjectError::Json)?;
        document
            .migrate()
            .map_err(|_| ProjectError::UnsupportedSchema)
    }

    fn write_metadata(&self, metadata: &ProjectMetadata) -> Result<(), ProjectError> {
        let bytes = serde_json::to_vec(metadata).map_err(ProjectError::Json)?;
        write_atomic(&self.path.project.join("project.json"), &bytes)
    }

    /// 回收不再被当前 revision、撤销栈或重做栈引用的快照文件。
    /// 每个 revision 都写全量快照会让 `revisions/` 随编辑次数线性增长，
    /// 这里保证它只与 MAX_UNDO_DEPTH 相关。
    fn prune_snapshots(&self, metadata: &ProjectMetadata) -> Result<(), ProjectError> {
        let mut keep = std::collections::HashSet::new();
        keep.insert(metadata.current_revision.as_u64());
        keep.extend(metadata.undo_stack.iter().map(|revision| revision.as_u64()));
        keep.extend(metadata.redo_stack.iter().map(|revision| revision.as_u64()));

        let directory = self.path.project.join("revisions");
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(_) => return Ok(()),
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
                continue;
            };
            let Ok(revision) = stem.parse::<u64>() else {
                continue;
            };
            if !keep.contains(&revision) {
                fs::remove_file(&path).map_err(ProjectError::Io)?;
            }
        }
        Ok(())
    }

    fn revision_path(&self, revision: Revision) -> PathBuf {
        self.path
            .project
            .join("revisions")
            .join(format!("{:08}.json", revision.as_u64()))
    }
}

pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), ProjectError> {
    let temporary = path.with_extension(format!(
        "{}.tmp",
        path.extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
    ));
    let result = (|| -> Result<(), std::io::Error> {
        let mut file = File::create(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(ProjectError::Io)
}

fn append_history(path: &Path, entry: &str) -> Result<(), ProjectError> {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(ProjectError::Io)?;
    writeln!(file, "{entry}").map_err(ProjectError::Io)?;
    file.sync_all().map_err(ProjectError::Io)
}
