use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::{ImportError, ImportRequest, SourceImporter, SourceScene};

pub struct SourceImporterRegistry {
    importers: Vec<Box<dyn SourceImporter>>,
    by_id: BTreeMap<String, usize>,
    by_extension: BTreeMap<String, usize>,
}

impl SourceImporterRegistry {
    pub fn try_new(importers: Vec<Box<dyn SourceImporter>>) -> Result<Self, ImportError> {
        let mut by_id = BTreeMap::new();
        let mut by_extension = BTreeMap::new();
        for (index, importer) in importers.iter().enumerate() {
            let id = importer.id().trim().to_ascii_lowercase();
            if id.is_empty() {
                return Err(ImportError::RegistryInvalid(
                    "importer id must not be empty".into(),
                ));
            }
            if by_id.insert(id.clone(), index).is_some() {
                return Err(ImportError::RegistryInvalid(format!(
                    "duplicate importer id: {id}"
                )));
            }
            let mut seen_for_importer = BTreeSet::new();
            for extension in importer.extensions() {
                let extension = normalize_extension(extension);
                if extension.is_empty() || !seen_for_importer.insert(extension.clone()) {
                    return Err(ImportError::RegistryInvalid(format!(
                        "invalid or duplicate extension for {id}: {extension}"
                    )));
                }
                if by_extension.insert(extension.clone(), index).is_some() {
                    return Err(ImportError::RegistryInvalid(format!(
                        "duplicate importer extension: .{extension}"
                    )));
                }
            }
        }
        Ok(Self {
            importers,
            by_id,
            by_extension,
        })
    }

    pub fn new(importers: Vec<Box<dyn SourceImporter>>) -> Self {
        Self::try_new(importers).expect("valid source importer registry")
    }

    pub fn resolve(
        &self,
        path: &Path,
        requested_adapter: Option<&str>,
    ) -> Result<&dyn SourceImporter, ImportError> {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .map(normalize_extension)
            .ok_or_else(|| ImportError::Unsupported(path.display().to_string()))?;
        if let Some(requested) = requested_adapter
            && requested != "auto"
        {
            let requested = requested.to_ascii_lowercase();
            let index = self
                .by_id
                .get(&requested)
                .copied()
                .ok_or_else(|| ImportError::AdapterUnavailable(requested.clone()))?;
            let importer = self.importers[index].as_ref();
            if !importer
                .extensions()
                .iter()
                .any(|candidate| normalize_extension(candidate) == extension)
            {
                return Err(ImportError::AdapterUnavailable(requested));
            }
            return Ok(importer);
        }
        self.by_extension
            .get(&extension)
            .copied()
            .map(|index| self.importers[index].as_ref())
            .ok_or_else(|| ImportError::Unsupported(format!(".{extension}")))
    }

    pub fn import(&self, request: &ImportRequest) -> Result<SourceScene, ImportError> {
        self.resolve(&request.path, request.requested_adapter.as_deref())?
            .import(request)
    }
}

fn normalize_extension(value: &str) -> String {
    value.trim().trim_start_matches('.').to_ascii_lowercase()
}
