use std::path::{Path, PathBuf};
#[cfg(feature = "jt")]
use std::time::Duration;

use serde::Serialize;
use sha2::Digest;
use studio_assets::{
    AssetError, AssetMetrics, BudgetProfile, BudgetResult, BudgetStatus, PreparationMetadata,
    PrepareOptions, prepare_canonical,
};
#[cfg(feature = "fbx")]
use studio_import::FbxImporter;
#[cfg(feature = "ifc")]
use studio_import::IfcImporter;
#[cfg(feature = "jt")]
use studio_import::JtImporter;
#[cfg(feature = "step")]
use studio_import::StepImporter;
#[cfg(feature = "usd")]
use studio_import::UsdImporter;
#[cfg(feature = "jt")]
use studio_import::jt::jt_timeout_from_environment;
#[cfg(any(
    feature = "fbx",
    feature = "ifc",
    feature = "jt",
    feature = "step",
    feature = "usd"
))]
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
    pub metrics: AssetMetrics,
    pub content_digest: String,
    pub preparation: PreparationMetadata,
    pub budget: BudgetResult,
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

impl ImportCommandError {
    pub fn code(&self) -> &str {
        match self {
            Self::Import(error) => error.code(),
            Self::Asset(error) => error.code(),
            Self::Io(_) => "ICE3_STUDIO_ASSET_IO",
            Self::OutputExists => "ICE3_STUDIO_ASSET_OUTPUT_EXISTS",
            Self::InvalidOutputName => "ICE3_STUDIO_ASSET_INVALID_OPTIONS",
            Self::WarningAsError { code, .. } => code,
            Self::InvalidLimit { .. } => "ICE3_STUDIO_ASSET_INVALID_OPTIONS",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImportedSource {
    pub scene: studio_import::SourceScene,
    pub adapter: String,
    pub source_digest: String,
    pub warnings: Vec<ImportWarning>,
}

pub fn source_importer_registry() -> Result<SourceImporterRegistry, ImportError> {
    #[allow(unused_mut)]
    let mut importers: Vec<Box<dyn SourceImporter>> =
        vec![Box::new(GltfImporter::new()), Box::new(ObjImporter::new())];
    #[cfg(feature = "fbx")]
    importers.push(Box::new(FbxImporter::new()));
    #[cfg(feature = "usd")]
    importers.push(Box::new(UsdImporter::new()));
    #[cfg(feature = "step")]
    importers.push(Box::new(StepImporter::new()));
    #[cfg(feature = "ifc")]
    importers.push(Box::new(IfcImporter::new()));
    #[cfg(feature = "jt")]
    importers.push(Box::new(JtImporter::new()));
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

pub fn import_source_scene(
    source: &Path,
    asset_id: &str,
    adapter: &str,
    limits: NativeImportLimits,
) -> Result<ImportedSource, ImportCommandError> {
    let registry = source_importer_registry()?;
    let request = ImportRequest {
        path: source.to_path_buf(),
        asset_id: asset_id.into(),
        requested_adapter: Some(adapter.into()),
    };
    let selected_adapter = registry
        .resolve(&request.path, request.requested_adapter.as_deref())?
        .id();
    let imported = match selected_adapter {
        #[cfg(feature = "fbx")]
        "fbx" => run_isolated_import(&FbxImporter::new(), &request, limits)?,
        #[cfg(feature = "usd")]
        "usd" => run_isolated_import(&UsdImporter::new(), &request, limits)?,
        #[cfg(feature = "step")]
        "step" => run_isolated_import(&StepImporter::new(), &request, limits)?,
        #[cfg(feature = "ifc")]
        "ifc" => run_isolated_import(&IfcImporter::new(), &request, limits)?,
        #[cfg(feature = "jt")]
        "jt" => {
            let limits = jt_worker_limits(limits)?;
            run_isolated_import(&JtImporter::new(), &request, limits)
                .map_err(map_jt_worker_error)?
        }
        _ => registry.import(&request)?,
    };
    let warnings = imported.warnings.clone();
    let source_bytes = std::fs::read(source)?;
    Ok(ImportedSource {
        scene: imported,
        adapter: selected_adapter.into(),
        source_digest: format!("{:x}", sha2::Sha256::digest(&source_bytes)),
        warnings,
    })
}

pub fn import_source(options: ImportOptions) -> Result<ImportReport, ImportCommandError> {
    let imported = import_source_scene(
        &options.source,
        &options.asset_id,
        &options.adapter,
        options.limits,
    )?;
    if options.warnings_as_errors
        && let Some(warning) = imported.warnings.first()
    {
        return Err(ImportCommandError::WarningAsError {
            code: warning.code.clone(),
            message: warning.message.clone(),
        });
    }

    let canonical = to_canonical(&imported.scene);
    let artifact = prepare_canonical(
        &canonical,
        PrepareOptions {
            asset_id: options.asset_id,
            source_format: canonical.source_format.clone(),
            source_digest: format!("sha256:{}", imported.source_digest),
            crease_angle_degrees: 30.0,
            generate_edges: false,
            budget: BudgetProfile::interactive_desktop_v1(),
        },
    )?;
    if options.warnings_as_errors && artifact.budget.status == BudgetStatus::Warning {
        return Err(ImportCommandError::WarningAsError {
            code: "ICE3_STUDIO_ASSET_BUDGET_WARNING".into(),
            message: "asset exceeds one or more soft budget limits".into(),
        });
    }
    if options.warnings_as_errors
        && let Some(warning) = artifact.warnings.first()
    {
        return Err(ImportCommandError::WarningAsError {
            code: warning.code.clone(),
            message: warning.message.clone(),
        });
    }
    let mut warnings = imported.warnings;
    warnings.extend(artifact.warnings.iter().map(|warning| ImportWarning {
        code: warning.code.clone(),
        message: warning.message.clone(),
        path: warning.path.clone(),
    }));
    warnings.sort();
    warnings.dedup();
    write_atomic_output(&options.output, &artifact.bytes, options.force)?;

    Ok(ImportReport {
        command: "import".into(),
        output: options.output,
        adapter: imported.adapter,
        warnings,
        source_digest: imported.source_digest,
        bytes: artifact.bytes.len(),
        metrics: artifact.metrics,
        content_digest: artifact.content_digest,
        preparation: artifact.preparation,
        budget: artifact.budget,
    })
}

#[cfg(feature = "jt")]
fn jt_worker_limits(
    mut limits: NativeImportLimits,
) -> Result<NativeImportLimits, ImportCommandError> {
    #[cfg(debug_assertions)]
    if std::env::var_os(DEBUG_WORKER_TIMEOUT_ENV).is_some() {
        return Ok(limits);
    }
    limits.timeout = jt_timeout_from_environment()
        .map_err(ImportCommandError::Import)?
        .saturating_add(Duration::from_secs(5));
    Ok(limits)
}

#[cfg(feature = "jt")]
fn map_jt_worker_error(error: ImportError) -> ImportCommandError {
    if matches!(error, ImportError::WorkerTimeout) {
        ImportError::SourceDiagnostic {
            code: "ICE3_STUDIO_IMPORT_JT_TIMEOUT".into(),
            message: "JT import worker exceeded its timeout".into(),
        }
        .into()
    } else {
        error.into()
    }
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
