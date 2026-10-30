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
        let mut file = match create(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if !remove_stale(&path) {
                    return Err(LockError::AlreadyLocked);
                }
                create(&path).map_err(LockError::Io)?
            }
            Err(error) => return Err(LockError::Io(error)),
        };
        writeln!(file, "pid={}", std::process::id()).map_err(LockError::Io)?;
        Ok(Self { path, _file: file })
    }

    pub(crate) fn path_for(project: &Path) -> PathBuf {
        project.join(".lock")
    }
}

fn create(path: &Path) -> Result<File, std::io::Error> {
    OpenOptions::new().write(true).create_new(true).open(path)
}

fn remove_stale(path: &Path) -> bool {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return false;
    };
    let Some(pid) = contents
        .trim()
        .strip_prefix("pid=")
        .and_then(|value| value.parse::<u32>().ok())
    else {
        return false;
    };
    let alive = std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if alive {
        false
    } else {
        std::fs::remove_file(path).is_ok()
    }
}

impl Drop for ProjectLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
