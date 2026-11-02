use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::Digest;
use studio_assets::canonical::{PreparedGlbOptions, write_prepared_glb};
use studio_assets::{AssetError, read_glb_bytes, validate_prepared_profile};
use studio_import::{
    FbxImporter, GltfImporter, ImportError, ImportRequest, ImportWarning, NativeImportLimits,
    ObjImporter, SourceImporterRegistry, UsdImporter, execute_worker_request, run_isolated_import,
    to_canonical,
};
use thiserror::Error;

#[cfg(debug_assertions)]
const DEBUG_WORKER_TIMEOUT_ENV: &str = "ICE3_STUDIO_IMPORT_WORKER_TIMEOUT_MS";

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
    SourceImporterRegistry::try_new(vec![
        Box::new(GltfImporter::new()),
        Box::new(ObjImporter::new()),
        Box::new(FbxImporter::new()),
        Box::new(UsdImporter::new()),
    ])
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
        "fbx" => run_isolated_import(&FbxImporter::new(), &request, options.limits)?,
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
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let name = output
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(ImportCommandError::InvalidOutputName)?;
    let temporary = parent.join(format!(".{name}.{nonce}.tmp"));
    let result = (|| {
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&temporary, output)?;
        Ok::<(), std::io::Error>(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result.map_err(Into::into)
}
