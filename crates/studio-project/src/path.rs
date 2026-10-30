use std::path::{Component, Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PathError {
    #[error("project ID must be a single safe path component")]
    InvalidProjectId,
    #[error("workspace root does not exist or cannot be resolved")]
    InvalidRoot(#[source] std::io::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspacePath {
    pub(crate) root: PathBuf,
    pub(crate) project: PathBuf,
}

impl WorkspacePath {
    pub fn new(root: &Path, project_id: &str) -> Result<Self, PathError> {
        if project_id.is_empty()
            || project_id == "."
            || project_id == ".."
            || !project_id
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            return Err(PathError::InvalidProjectId);
        }
        let root = root.canonicalize().map_err(PathError::InvalidRoot)?;
        let project = root.join(project_id);
        if Path::new(project_id).components().count() != 1
            || Path::new(project_id)
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(PathError::InvalidProjectId);
        }
        Ok(Self { root, project })
    }

    pub fn project_dir(&self) -> &Path {
        &self.project
    }

    pub fn workspace_root(&self) -> &Path {
        &self.root
    }
}
