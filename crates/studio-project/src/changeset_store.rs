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

    fn path_for(&self, id: &ChangeSetId) -> PathBuf {
        self.directory.join(format!("{}.json", id.as_str()))
    }
}
