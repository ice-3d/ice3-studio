use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum LockError {
    #[error("project is locked by another writer")]
    AlreadyLocked,
    #[error("project lock could not be created")]
    Io(#[source] std::io::Error),
}

pub(crate) struct ProjectLock {
    path: PathBuf,
    _file: File,
}

impl ProjectLock {
    pub(crate) fn acquire(path: PathBuf) -> Result<Self, LockError> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    LockError::AlreadyLocked
                } else {
                    LockError::Io(error)
                }
            })?;
        writeln!(file, "pid={}", std::process::id()).map_err(LockError::Io)?;
        Ok(Self { path, _file: file })
    }

    pub(crate) fn path_for(project: &Path) -> PathBuf {
        project.join(".lock")
    }
}

impl Drop for ProjectLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
