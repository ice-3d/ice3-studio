use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::Digest;
use studio_assets::canonical::{PreparedGlbOptions, write_prepared_glb};
use studio_assets::{AssetError, read_glb_bytes, validate_prepared_profile};
#[cfg(feature = "fbx")]
use studio_import::FbxImporter;
#[cfg(feature = "usd")]
use studio_import::UsdImporter;
#[cfg(any(feature = "fbx", feature = "usd"))]
use studio_import::run_isolated_import;
use studio_import::{
    GltfImporter, ImportError, ImportRequest, ImportWarning, NativeImportLimits, ObjImporter,
    SourceImporter, SourceImporterRegistry, execute_worker_request, to_canonical,
};
use tempfile::NamedTempFile;
use thiserror::Error;

#[cfg(debug_assertions)]
const DEBUG_WORKER_TIMEOUT_ENV: &str = "ICE3_STUDIO_IMPORT_WORKER_TIMEOUT_MS";
#[cfg(debug_assertions)]
const DEBUG_MAX_TRIANGLES_ENV: &str = "ICE3_STUDIO_IMPORT_MAX_TRIANGLES";

#[derive(Debug)]
pub struct ImportOptions {
    pub source: PathBuf,
    pub output: PathBuf,
    pub asset_id: String,
    pub adapter: String,
    pub warnings_as_errors: bool,
    pub force: bool,
    pub limits: NativeImportLimits,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub command: String,
    pub output: PathBuf,
    pub adapter: String,
    pub warnings: Vec<ImportWarning>,
    pub source_digest: String,
    pub bytes: usize,
}

#[derive(Debug, Error)]
pub enum ImportCommandError {
    #[error("{code}: {0}", code = .0.code())]
    Import(#[from] ImportError),
    #[error("{0}")]
    Asset(#[from] AssetError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("output already exists")]
    OutputExists,
    #[error("output must have a valid file name")]
    InvalidOutputName,
    #[error("{code}: {message}")]
    WarningAsError { code: String, message: String },
    #[error("ICE3_STUDIO_IMPORT_INVALID_LIMIT: invalid {name}: {value}")]
    InvalidLimit { name: &'static str, value: String },
}

pub fn source_importer_registry() -> Result<SourceImporterRegistry, ImportError> {
    #[allow(unused_mut)]
    let mut importers: Vec<Box<dyn SourceImporter>> =
        vec![Box::new(GltfImporter::new()), Box::new(ObjImporter::new())];
    #[cfg(feature = "fbx")]
    importers.push(Box::new(FbxImporter::new()));
    #[cfg(feature = "usd")]
    importers.push(Box::new(UsdImporter::new()));
    SourceImporterRegistry::try_new(importers)
}

pub fn execute_internal_worker(request_path: &Path) -> Result<(), ImportCommandError> {
    let registry = source_importer_registry()?;
    execute_worker_request(request_path, &registry)?;
    Ok(())
}

pub fn import_limits_from_environment() -> Result<NativeImportLimits, ImportCommandError> {
    #[cfg(debug_assertions)]
    {
        let mut limits = NativeImportLimits::default();
        if let Some(value) = std::env::var_os(DEBUG_WORKER_TIMEOUT_ENV) {
            let value = value
                .to_str()
                .ok_or_else(|| ImportCommandError::InvalidLimit {
                    name: DEBUG_WORKER_TIMEOUT_ENV,
                    value: value.to_string_lossy().into_owned(),
                })?;
            let timeout_millis =
                value
                    .parse::<u64>()
                    .map_err(|_| ImportCommandError::InvalidLimit {
                        name: DEBUG_WORKER_TIMEOUT_ENV,
                        value: value.into(),
                    })?;
            if timeout_millis == 0 {
                return Err(ImportCommandError::InvalidLimit {
                    name: DEBUG_WORKER_TIMEOUT_ENV,
                    value: value.into(),
                });
            }
            limits.timeout = std::time::Duration::from_millis(timeout_millis);
        }
        if let Some(value) = std::env::var_os(DEBUG_MAX_TRIANGLES_ENV) {
            let value = value
                .to_str()
                .ok_or_else(|| ImportCommandError::InvalidLimit {
                    name: DEBUG_MAX_TRIANGLES_ENV,
                    value: value.to_string_lossy().into_owned(),
                })?;
            let max_triangles =
                value
                    .parse::<u64>()
                    .map_err(|_| ImportCommandError::InvalidLimit {
                        name: DEBUG_MAX_TRIANGLES_ENV,
                        value: value.into(),
                    })?;
            if max_triangles == 0 {
                return Err(ImportCommandError::InvalidLimit {
                    name: DEBUG_MAX_TRIANGLES_ENV,
                    value: value.into(),
                });
            }
            limits.max_triangles = max_triangles;
        }
        Ok(limits)
    }
    #[cfg(not(debug_assertions))]
    {
        Ok(NativeImportLimits::default())
    }
}

pub fn import_source(options: ImportOptions) -> Result<ImportReport, ImportCommandError> {
    let registry = source_importer_registry()?;
    let request = ImportRequest {
        path: options.source.clone(),
        asset_id: options.asset_id.clone(),
        requested_adapter: Some(options.adapter),
    };
    let selected_adapter = registry
        .resolve(&request.path, request.requested_adapter.as_deref())?
        .id();
    let imported = match selected_adapter {
        #[cfg(feature = "fbx")]
        "fbx" => run_isolated_import(&FbxImporter::new(), &request, options.limits)?,
        #[cfg(feature = "usd")]
        "usd" => run_isolated_import(&UsdImporter::new(), &request, options.limits)?,
        _ => registry.import(&request)?,
    };

    if options.warnings_as_errors
        && let Some(warning) = imported.warnings.first()
    {
        return Err(ImportCommandError::WarningAsError {
            code: warning.code.clone(),
            message: warning.message.clone(),
        });
    }

    let source_bytes = std::fs::read(&options.source)?;
    let source_digest = format!("{:x}", sha2::Sha256::digest(&source_bytes));
    let canonical = to_canonical(&imported);
    let bytes = write_prepared_glb(
        &canonical,
        &PreparedGlbOptions {
            asset_id: options.asset_id,
            source_format: canonical.source_format.clone(),
            source_digest: source_digest.clone(),
        },
    )?;
    let parsed = read_glb_bytes(&bytes, &std::env::temp_dir())?;
    validate_prepared_profile(&parsed)?;
    write_atomic_output(&options.output, &bytes, options.force)?;

    Ok(ImportReport {
        command: "import".into(),
        output: options.output,
        adapter: imported.source_format.as_str().into(),
        warnings: imported.warnings,
        source_digest,
        bytes: bytes.len(),
    })
}

fn write_atomic_output(output: &Path, bytes: &[u8], force: bool) -> Result<(), ImportCommandError> {
    if output.exists() && !force {
        return Err(ImportCommandError::OutputExists);
    }
    let parent = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    std::io::Write::write_all(&mut temporary, bytes)?;
    temporary.as_file().sync_all()?;
    persist_temporary_output(temporary, output, force)
}

fn persist_temporary_output(
    temporary: NamedTempFile,
    output: &Path,
    force: bool,
) -> Result<(), ImportCommandError> {
    if force {
        temporary
            .persist(output)
            .map(|_| ())
            .map_err(|error| ImportCommandError::Io(error.error))
    } else {
        match temporary.persist_noclobber(output) {
            Ok(_) => Ok(()),
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(ImportCommandError::OutputExists)
            }
            Err(error) => Err(ImportCommandError::Io(error.error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[test]
    fn non_force_publication_never_replaces_a_concurrently_created_file() {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("plant.glb");
        let mut temporary = tempfile::NamedTempFile::new_in(root.path()).unwrap();
        temporary.write_all(b"new").unwrap();
        std::fs::write(&output, b"keep").unwrap();

        let error = persist_temporary_output(temporary, &output, false).unwrap_err();

        assert!(matches!(error, ImportCommandError::OutputExists));
        assert_eq!(std::fs::read(&output).unwrap(), b"keep");
    }

    #[test]
    fn force_publication_replaces_an_existing_file() {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("plant.glb");
        let mut temporary = tempfile::NamedTempFile::new_in(root.path()).unwrap();
        temporary.write_all(b"new").unwrap();
        std::fs::write(&output, b"keep").unwrap();

        persist_temporary_output(temporary, &output, true).unwrap();

        assert_eq!(std::fs::read(&output).unwrap(), b"new");
    }
}
