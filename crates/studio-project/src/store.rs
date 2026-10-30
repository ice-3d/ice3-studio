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
use crate::lock::{LockError, ProjectLock};

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("project already exists")]
    AlreadyExists,
    #[error("project does not exist or has incomplete storage")]
    Missing,
    #[error("project is locked by another writer")]
    Locked,
    #[error("project schema is unsupported")]
    UnsupportedSchema,
    #[error("project revision is invalid")]
    InvalidRevision,
    #[error("project snapshot digest does not match")]
    DigestMismatch,
    #[error("project history is empty")]
    EmptyHistory,
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
    _lock: ProjectLock,
}

impl ProjectStore {
    pub fn create(path: WorkspacePath) -> Result<Self, ProjectError> {
        if path.project.exists() {
            return Err(ProjectError::AlreadyExists);
        }
        fs::create_dir_all(path.project.join("revisions")).map_err(ProjectError::Io)?;
        fs::create_dir_all(path.project.join("history")).map_err(ProjectError::Io)?;
        fs::create_dir_all(path.project.join("resources")).map_err(ProjectError::Io)?;
        fs::create_dir_all(path.project.join("cache/preview")).map_err(ProjectError::Io)?;
        let lock = ProjectLock::acquire(ProjectLock::path_for(&path.project))?;
        Ok(Self { path, _lock: lock })
    }

    pub fn open(path: WorkspacePath) -> Result<Self, ProjectError> {
        if !path.project.join("project.json").is_file() {
            return Err(ProjectError::Missing);
        }
        let lock = ProjectLock::acquire(ProjectLock::path_for(&path.project))?;
        Ok(Self { path, _lock: lock })
    }

    pub fn load_current(&self) -> Result<Document, ProjectError> {
        let metadata = self.read_metadata()?;
        self.read_revision_with_digest(metadata.current_revision, &metadata.snapshot_digest)
    }

    pub fn project_dir(&self) -> &Path {
        self.path.project_dir()
    }

    pub fn write_snapshot(&self, document: &Document) -> Result<(), ProjectError> {
        let previous = self.read_metadata().ok();
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
        let metadata = self.write_revision(document, undo_stack, Vec::new())?;
        self.write_metadata(&metadata)
    }

    pub fn undo(&self) -> Result<Document, ProjectError> {
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
        )?;
        self.write_metadata(&next_metadata)?;
        Ok(restored)
    }

    pub fn redo(&self) -> Result<Document, ProjectError> {
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
        )?;
        self.write_metadata(&next_metadata)?;
        Ok(restored)
    }

    pub fn history(&self) -> Result<Vec<String>, ProjectError> {
        let path = self.path.project.join("history/operations.jsonl");
        if !path.exists() {
            return Ok(Vec::new());
        }
        let body = fs::read_to_string(path).map_err(ProjectError::Io)?;
        Ok(body.lines().map(str::to_owned).collect())
    }

    fn write_revision(
        &self,
        document: &Document,
        undo_stack: Vec<Revision>,
        redo_stack: Vec<Revision>,
    ) -> Result<ProjectMetadata, ProjectError> {
        if document.schema_version != PROJECT_SCHEMA_VERSION {
            return Err(ProjectError::UnsupportedSchema);
        }
        let bytes = serde_json::to_vec(document).map_err(ProjectError::Json)?;
        let digest = format!("{:x}", Sha256::digest(&bytes));
        let revision_path = self.revision_path(document.revision);
        write_atomic(&revision_path, &bytes)?;
        append_history(
            &self.path.project.join("history/operations.jsonl"),
            &format!(
                "{{\"revision\":{},\"digest\":\"{}\"}}",
                document.revision.as_u64(),
                digest
            ),
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
        if metadata.schema_version != PROJECT_SCHEMA_VERSION {
            return Err(ProjectError::UnsupportedSchema);
        }
        Ok(metadata)
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
        serde_json::from_slice(&bytes).map_err(ProjectError::Json)
    }

    fn write_metadata(&self, metadata: &ProjectMetadata) -> Result<(), ProjectError> {
        let bytes = serde_json::to_vec(metadata).map_err(ProjectError::Json)?;
        write_atomic(&self.path.project.join("project.json"), &bytes)
    }

    fn revision_path(&self, revision: Revision) -> PathBuf {
        self.path
            .project
            .join("revisions")
            .join(format!("{:08}.json", revision.as_u64()))
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), ProjectError> {
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
    writeln!(file, "{entry}").map_err(ProjectError::Io)
}
