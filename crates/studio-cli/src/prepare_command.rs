use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;
use studio_assets::{
    AssetBuildReport, AssetError, BudgetProfile, BudgetResult, BudgetStatus, PreparationMetadata,
    PrepareOptions, PrepareWarning, prepare_canonical, read_glb, validate_prepared_profile,
};
use studio_import::{ImportWarning, NativeImportLimits, to_canonical};
use tempfile::NamedTempFile;

use crate::import::{ImportCommandError, import_source_scene};

#[derive(Debug, Clone)]
pub struct PrepareCommandOptions {
    pub source: PathBuf,
    pub output: PathBuf,
    pub asset_id: String,
    pub adapter: String,
    pub crease_angle_degrees: f32,
    pub skip_edges: bool,
    pub budget: Option<String>,
    pub report: Option<PathBuf>,
    pub warnings_as_errors: bool,
    pub force: bool,
    pub limits: NativeImportLimits,
}

#[derive(Debug, Clone)]
pub struct InspectOptions {
    pub source: PathBuf,
    pub adapter: String,
    pub crease_angle_degrees: f32,
    pub skip_edges: bool,
    pub budget: Option<String>,
    pub limits: NativeImportLimits,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareCommandOutput {
    pub output: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report_path: Option<PathBuf>,
    pub report: AssetBuildReport,
}

#[derive(Debug)]
pub enum PrepareCommandError {
    Import(ImportCommandError),
    Asset(AssetError),
    InvalidOptions(String),
    OutputExists,
    WarningAsError { code: String, message: String },
    ReportWriteFailed(String),
    Io(std::io::Error),
}

impl PrepareCommandError {
    pub fn code(&self) -> &str {
        match self {
            Self::Import(error) => error.code(),
            Self::Asset(error) => error.code(),
            Self::InvalidOptions(_) => "ICE3_STUDIO_ASSET_INVALID_OPTIONS",
            Self::OutputExists => "ICE3_STUDIO_ASSET_OUTPUT_EXISTS",
            Self::WarningAsError { code, .. } => code,
            Self::ReportWriteFailed(_) => "ICE3_STUDIO_ASSET_REPORT_WRITE_FAILED",
            Self::Io(_) => "ICE3_STUDIO_ASSET_IO",
        }
    }
}

impl fmt::Display for PrepareCommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: ", self.code())?;
        match self {
            Self::Import(error) => write!(formatter, "{error}"),
            Self::Asset(error) => write!(formatter, "{error}"),
            Self::InvalidOptions(message) | Self::ReportWriteFailed(message) => {
                formatter.write_str(message)
            }
            Self::OutputExists => formatter.write_str("output or report already exists"),
            Self::WarningAsError { message, .. } => formatter.write_str(message),
            Self::Io(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for PrepareCommandError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Import(error) => Some(error),
            Self::Asset(error) => Some(error),
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ImportCommandError> for PrepareCommandError {
    fn from(error: ImportCommandError) -> Self {
        Self::Import(error)
    }
}

impl From<AssetError> for PrepareCommandError {
    fn from(error: AssetError) -> Self {
        Self::Asset(error)
    }
}

impl From<std::io::Error> for PrepareCommandError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

pub fn run_build(
    options: PrepareCommandOptions,
) -> Result<PrepareCommandOutput, PrepareCommandError> {
    let imported = import_source_scene(
        &options.source,
        &options.asset_id,
        &options.adapter,
        options.limits,
    )?;
    let source_digest = digest_label(&imported.source_digest);
    let budget = resolve_budget(options.budget.as_deref())?;
    let artifact = prepare_canonical(
        &to_canonical(&imported.scene),
        PrepareOptions {
            asset_id: options.asset_id.clone(),
            source_format: imported.scene.source_format.as_str().into(),
            source_digest: source_digest.clone(),
            crease_angle_degrees: options.crease_angle_degrees,
            generate_edges: !options.skip_edges,
            budget,
        },
    )?;
    let warnings = prepare_warnings(&imported.warnings, &artifact.warnings);
    if options.warnings_as_errors {
        if artifact.budget.status == BudgetStatus::Warning {
            return Err(PrepareCommandError::WarningAsError {
                code: "ICE3_STUDIO_ASSET_BUDGET_WARNING".into(),
                message: "asset exceeds one or more soft budget limits".into(),
            });
        }
        if let Some(warning) = warnings.first() {
            return Err(PrepareCommandError::WarningAsError {
                code: warning.code.clone(),
                message: warning.message.clone(),
            });
        }
    }
    let report = AssetBuildReport::new(
        "build",
        source_digest,
        digest_label(&artifact.content_digest),
        options.asset_id,
        artifact.metrics,
        artifact.preparation,
        artifact.budget,
        warnings,
    );
    let report_bytes = report_json(&report)?;
    publish_outputs(
        &options.output,
        &artifact.bytes,
        options.report.as_deref(),
        &report_bytes,
        options.force,
    )?;
    Ok(PrepareCommandOutput {
        output: options.output,
        report_path: options.report,
        report,
    })
}

pub fn run_inspect(options: InspectOptions) -> Result<AssetBuildReport, PrepareCommandError> {
    match read_glb(&options.source) {
        Ok(parsed) => match validate_prepared_profile(&parsed) {
            Ok(profile) => {
                let profile_extras = &parsed.json["asset"]["extras"]["ice3"];
                let source_digest = profile_extras["sourceDigest"]
                    .as_str()
                    .map(digest_label)
                    .unwrap_or_else(|| digest_label(&parsed.digest));
                let content_digest = profile_extras
                    .pointer("/contentDigest/value")
                    .and_then(Value::as_str)
                    .map(digest_label)
                    .unwrap_or_else(|| digest_label(&parsed.digest));
                Ok(AssetBuildReport::new(
                    "inspect",
                    source_digest,
                    content_digest,
                    profile.asset_id,
                    profile.metrics,
                    profile
                        .preparation
                        .unwrap_or_else(PreparationMetadata::without_edges),
                    profile.budget.unwrap_or_else(pass_budget),
                    Vec::new(),
                ))
            }
            Err(AssetError::ProfileUnsupported) => inspect_source(options),
            Err(error) => Err(error.into()),
        },
        Err(_) => inspect_source(options),
    }
}

fn inspect_source(options: InspectOptions) -> Result<AssetBuildReport, PrepareCommandError> {
    let asset_id = default_asset_id(&options.source)?;
    let imported =
        import_source_scene(&options.source, &asset_id, &options.adapter, options.limits)?;
    let source_digest = digest_label(&imported.source_digest);
    let artifact = prepare_canonical(
        &to_canonical(&imported.scene),
        PrepareOptions {
            asset_id,
            source_format: imported.scene.source_format.as_str().into(),
            source_digest: source_digest.clone(),
            crease_angle_degrees: options.crease_angle_degrees,
            generate_edges: !options.skip_edges,
            budget: resolve_budget(options.budget.as_deref())?,
        },
    )?;
    let warnings = prepare_warnings(&imported.warnings, &artifact.warnings);
    Ok(AssetBuildReport::new(
        "inspect",
        source_digest,
        digest_label(&artifact.content_digest),
        artifact_asset_id(&imported.scene),
        artifact.metrics,
        artifact.preparation,
        artifact.budget,
        warnings,
    ))
}

fn artifact_asset_id(scene: &studio_import::SourceScene) -> String {
    scene.asset_id.clone()
}

fn resolve_budget(value: Option<&str>) -> Result<BudgetProfile, PrepareCommandError> {
    let Some(value) = value else {
        return Ok(BudgetProfile::interactive_desktop_v1());
    };
    match value {
        "interactive-desktop-v1" => Ok(BudgetProfile::interactive_desktop_v1()),
        "unrestricted" => Ok(BudgetProfile::unrestricted()),
        path => {
            let source = std::fs::read_to_string(path).map_err(|error| {
                AssetError::InvalidBudget(format!("cannot read budget file: {error}"))
            })?;
            BudgetProfile::from_json(&source).map_err(Into::into)
        }
    }
}

fn prepare_warnings(source: &[ImportWarning], prepared: &[PrepareWarning]) -> Vec<PrepareWarning> {
    let mut warnings = source
        .iter()
        .map(|warning| PrepareWarning {
            code: warning.code.clone(),
            message: warning.message.clone(),
            path: warning.path.clone(),
        })
        .collect::<Vec<_>>();
    warnings.extend_from_slice(prepared);
    warnings.sort();
    warnings.dedup();
    warnings
}

fn digest_label(value: &str) -> String {
    if value.starts_with("sha256:") {
        value.into()
    } else {
        format!("sha256:{value}")
    }
}

fn pass_budget() -> BudgetResult {
    BudgetResult {
        profile: "legacy".into(),
        status: BudgetStatus::Pass,
        warning_count: 0,
        error_count: 0,
        checks: Vec::new(),
    }
}

fn default_asset_id(source: &Path) -> Result<String, PrepareCommandError> {
    source
        .file_stem()
        .and_then(|value| value.to_str())
        .map(|value| {
            let sanitized = value
                .chars()
                .map(|character| {
                    if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
                        character
                    } else {
                        '-'
                    }
                })
                .collect::<String>()
                .trim_matches(['.', '-'])
                .to_owned();
            if sanitized.is_empty() {
                "asset".into()
            } else {
                sanitized
            }
        })
        .ok_or_else(|| PrepareCommandError::InvalidOptions("source has no usable asset ID".into()))
}

fn report_json(report: &AssetBuildReport) -> Result<Vec<u8>, PrepareCommandError> {
    let mut bytes = serde_json::to_vec_pretty(report)
        .map_err(|error| PrepareCommandError::ReportWriteFailed(error.to_string()))?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn publish_outputs(
    output_path: &Path,
    output_bytes: &[u8],
    report_path: Option<&Path>,
    report_bytes: &[u8],
    force: bool,
) -> Result<(), PrepareCommandError> {
    if report_path.is_some_and(|path| path == output_path) {
        return Err(PrepareCommandError::InvalidOptions(
            "output and report paths must differ".into(),
        ));
    }
    if !force && (output_path.exists() || report_path.is_some_and(Path::exists)) {
        return Err(PrepareCommandError::OutputExists);
    }
    let output_parent = output_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(output_parent)?;
    let mut output_temp = NamedTempFile::new_in(output_parent)?;
    output_temp.write_all(output_bytes)?;
    output_temp.as_file().sync_all()?;

    let mut report_temp = match report_path {
        Some(path) => {
            let parent = path
                .parent()
                .filter(|path| !path.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            std::fs::create_dir_all(parent)
                .map_err(|error| PrepareCommandError::ReportWriteFailed(error.to_string()))?;
            let mut temporary = NamedTempFile::new_in(parent)
                .map_err(|error| PrepareCommandError::ReportWriteFailed(error.to_string()))?;
            temporary
                .write_all(report_bytes)
                .map_err(|error| PrepareCommandError::ReportWriteFailed(error.to_string()))?;
            temporary
                .as_file()
                .sync_all()
                .map_err(|error| PrepareCommandError::ReportWriteFailed(error.to_string()))?;
            Some((path.to_path_buf(), temporary))
        }
        None => None,
    };

    if force {
        return publish_force(output_path, output_temp, report_temp);
    }

    let report_created = if let Some((path, temporary)) = report_temp.take() {
        temporary.persist_noclobber(&path).map_err(|error| {
            if error.error.kind() == std::io::ErrorKind::AlreadyExists {
                PrepareCommandError::OutputExists
            } else {
                PrepareCommandError::ReportWriteFailed(error.error.to_string())
            }
        })?;
        Some(path)
    } else {
        None
    };
    match output_temp.persist_noclobber(output_path) {
        Ok(_) => Ok(()),
        Err(error) => {
            if let Some(path) = report_created {
                let _ = std::fs::remove_file(path);
            }
            if error.error.kind() == std::io::ErrorKind::AlreadyExists {
                Err(PrepareCommandError::OutputExists)
            } else {
                Err(PrepareCommandError::Io(error.error))
            }
        }
    }
}

fn publish_force(
    output_path: &Path,
    output_temp: NamedTempFile,
    report_temp: Option<(PathBuf, NamedTempFile)>,
) -> Result<(), PrepareCommandError> {
    let output_backup = move_to_backup(output_path)?;
    let report_path = report_temp.as_ref().map(|(path, _)| path.clone());
    let report_backup = if let Some(path) = report_path.as_deref() {
        match move_to_backup(path) {
            Ok(backup) => backup,
            Err(error) => {
                restore_backup(output_path, output_backup);
                return Err(error);
            }
        }
    } else {
        None
    };

    if let Some((path, temporary)) = report_temp
        && let Err(error) = temporary.persist(&path)
    {
        restore_backup(output_path, output_backup);
        restore_backup(&path, report_backup);
        return Err(PrepareCommandError::ReportWriteFailed(
            error.error.to_string(),
        ));
    }

    if let Err(error) = output_temp.persist(output_path) {
        if let Some(path) = report_path.as_deref() {
            let _ = std::fs::remove_file(path);
            restore_backup(output_path, output_backup);
            restore_backup(path, report_backup);
        } else {
            restore_backup(output_path, output_backup);
        }
        return Err(PrepareCommandError::Io(error.error));
    }

    remove_backup(report_backup);
    remove_backup(output_backup);
    Ok(())
}

fn move_to_backup(path: &Path) -> Result<Option<PathBuf>, PrepareCommandError> {
    if !path.exists() {
        return Ok(None);
    }
    let backup = unique_backup_path(path);
    std::fs::rename(path, &backup)?;
    Ok(Some(backup))
}

fn unique_backup_path(path: &Path) -> PathBuf {
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("asset");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    parent.join(format!(".{name}.{}.{nonce}.backup", std::process::id()))
}

fn restore_backup(path: &Path, backup: Option<PathBuf>) {
    let _ = std::fs::remove_file(path);
    if let Some(backup) = backup {
        let _ = std::fs::rename(backup, path);
    }
}

fn remove_backup(backup: Option<PathBuf>) {
    if let Some(backup) = backup {
        let _ = std::fs::remove_file(backup);
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    use tempfile::NamedTempFile;

    use super::*;

    #[test]
    fn force_restores_output_when_report_cannot_be_backed_up() {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("cube.glb");
        let report_directory = root.path().join("report-directory");
        let report = report_directory.join("cube.report.json");
        std::fs::create_dir(&report_directory).unwrap();
        std::fs::write(&output, b"old-output").unwrap();
        std::fs::write(&report, b"old-report").unwrap();

        let mut output_temp = NamedTempFile::new_in(root.path()).unwrap();
        output_temp.write_all(b"new-output").unwrap();
        let mut report_temp = NamedTempFile::new_in(root.path()).unwrap();
        report_temp.write_all(b"new-report").unwrap();
        let original_permissions = std::fs::metadata(&report_directory).unwrap().permissions();
        std::fs::set_permissions(&report_directory, std::fs::Permissions::from_mode(0o555))
            .unwrap();

        let result = publish_force(&output, output_temp, Some((report.clone(), report_temp)));

        std::fs::set_permissions(&report_directory, original_permissions).unwrap();
        assert!(result.is_err());
        assert_eq!(std::fs::read(output).unwrap(), b"old-output");
        assert_eq!(std::fs::read(report).unwrap(), b"old-report");
    }
}
