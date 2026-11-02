use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;
use studio_assets::{
    AssetError, AssetMetrics, BudgetProfile, BudgetResult, BudgetStatus, PreparationMetadata,
    PrepareOptions, PrepareWarning, prepare_canonical, read_glb, validate_crease_angle,
    validate_prepared_profile,
};
use studio_import::{ImportError, ImportWarning, NativeImportLimits, to_canonical};
use studio_pipeline::{
    AssetProfile, LodAllocationMode, PartitionStats, PipelineError, RenderBatchMode,
    SemanticProxyMode, build_with_profile,
};

use crate::import::{ImportCommandError, import_source_scene};

#[derive(Debug, Clone)]
pub struct PartitionOptions {
    pub source: PathBuf,
    pub output: PathBuf,
    pub asset_id: String,
    pub adapter: String,
    pub max_triangles: u64,
    pub max_bytes: usize,
    pub crease_angle_degrees: f32,
    pub skip_edges: bool,
    pub budget: Option<String>,
    pub render_batch: RenderBatchMode,
    pub lod_enabled: bool,
    pub lod_ratio: f64,
    pub lod_allocation: LodAllocationMode,
    pub lod_max_triangles: u64,
    pub lod_min_unit_triangles: u64,
    pub lod_curve_samples: u32,
    pub lod_max_memory_bytes: usize,
    pub semantic_proxy: SemanticProxyMode,
    pub coarse_chunk_max_bytes: Option<usize>,
    pub warnings_as_errors: bool,
    pub force: bool,
    pub limits: NativeImportLimits,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartitionOutput {
    pub command: &'static str,
    pub output: PathBuf,
    pub adapter: String,
    pub asset_id: String,
    pub source_digest: String,
    pub profile: String,
    pub files: Vec<PartitionFileReport>,
    pub stats: PartitionStatsReport,
    pub metrics: AssetMetrics,
    pub preparation: PreparationMetadata,
    pub budget: BudgetResult,
    pub warnings: Vec<PrepareWarning>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartitionFileReport {
    pub path: String,
    pub bytes: usize,
    pub sha256: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartitionStatsReport {
    pub triangle_count: u64,
    pub edge_segment_count: u64,
    pub edge_bytes: u64,
    pub source_triangle_count: u64,
    pub node_count: u64,
    pub partition_count: usize,
    pub max_part_bytes: u64,
    pub render_batch_count: usize,
    pub coarse_count: usize,
}

impl From<&PartitionStats> for PartitionStatsReport {
    fn from(stats: &PartitionStats) -> Self {
        Self {
            triangle_count: stats.triangle_count,
            edge_segment_count: stats.edge_segment_count,
            edge_bytes: stats.edge_bytes,
            source_triangle_count: stats.source_triangle_count,
            node_count: stats.node_count,
            partition_count: stats.partition_count,
            max_part_bytes: stats.max_part_bytes,
            render_batch_count: stats.render_batch_count,
            coarse_count: stats.coarse_count,
        }
    }
}

#[derive(Debug)]
pub enum PartitionError {
    Import(ImportCommandError),
    Asset(AssetError),
    Pipeline(PipelineError),
    InvalidOptions(String),
    OutputExists,
    WarningAsError {
        code: String,
        message: String,
    },
    Validation(String),
    RollbackFailed {
        primary: Box<PartitionError>,
        details: Vec<String>,
    },
    Io(std::io::Error),
}

impl PartitionError {
    pub fn code(&self) -> &str {
        match self {
            Self::Import(error) => error.code(),
            Self::Asset(error) => error.code(),
            Self::Pipeline(error) => error.code(),
            Self::InvalidOptions(_) => "ICE3_STUDIO_ASSET_INVALID_OPTIONS",
            Self::OutputExists => "ICE3_STUDIO_ASSET_OUTPUT_EXISTS",
            Self::WarningAsError { code, .. } => code,
            Self::Validation(_) => "ICE3_STUDIO_ASSET_TILESET_INVALID",
            Self::RollbackFailed { primary, .. } => primary.code(),
            Self::Io(_) => "ICE3_STUDIO_ASSET_IO",
        }
    }
}

impl fmt::Display for PartitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: ", self.code())?;
        match self {
            Self::Import(error) => write!(formatter, "{error}"),
            Self::Asset(error) => write!(formatter, "{error}"),
            Self::Pipeline(error) => write!(formatter, "{error}"),
            Self::InvalidOptions(message)
            | Self::Validation(message)
            | Self::WarningAsError { message, .. } => formatter.write_str(message),
            Self::OutputExists => formatter.write_str("output directory already exists"),
            Self::RollbackFailed { primary, details } => write!(
                formatter,
                "{primary}; publication rollback also failed: {}",
                details.join("; ")
            ),
            Self::Io(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for PartitionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Import(error) => Some(error),
            Self::Asset(error) => Some(error),
            Self::Pipeline(error) => Some(error),
            Self::RollbackFailed { primary, .. } => Some(primary),
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ImportCommandError> for PartitionError {
    fn from(error: ImportCommandError) -> Self {
        Self::Import(error)
    }
}

impl From<AssetError> for PartitionError {
    fn from(error: AssetError) -> Self {
        Self::Asset(error)
    }
}

impl From<PipelineError> for PartitionError {
    fn from(error: PipelineError) -> Self {
        Self::Pipeline(error)
    }
}

impl From<std::io::Error> for PartitionError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

pub fn partition_source(options: PartitionOptions) -> Result<PartitionOutput, PartitionError> {
    validate_options(&options)?;
    if options.output.exists() && !options.force {
        return Err(PartitionError::OutputExists);
    }
    let budget = resolve_budget(options.budget.as_deref())?;
    let imported = import_source_scene(
        &options.source,
        &options.asset_id,
        &options.adapter,
        options.limits,
    )?;
    let source_digest = digest_label(&imported.source_digest);
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
    let warnings = merge_warnings(&imported.warnings, &artifact.warnings);
    reject_warnings_as_errors(&options, &artifact.budget, &warnings)?;

    let profile = profile_from_options(&options);
    let parent = output_parent(&options.output);
    std::fs::create_dir_all(&parent)?;
    let mut prepared_file = tempfile::Builder::new()
        .prefix(".ice3-studio-partition-source-")
        .suffix(".glb")
        .tempfile_in(&parent)?;
    prepared_file.write_all(&artifact.bytes)?;
    prepared_file.as_file().sync_all()?;
    let prepared = read_glb(prepared_file.path())?;
    validate_prepared_profile(&prepared)?;

    let partition = build_with_profile(&prepared, &options.asset_id, &profile)?;
    for file in &partition.files {
        let parsed = studio_assets::read_glb_bytes(&file.bytes, &std::env::temp_dir())?;
        validate_prepared_profile(&parsed).map_err(|error| {
            PartitionError::Validation(format!("{}: {error}", file.relative_path))
        })?;
    }

    let staging = write_staging(&parent, &partition)?;
    publish_directory(staging, &options.output, options.force)?;

    Ok(PartitionOutput {
        command: "partition",
        output: options.output,
        adapter: imported.adapter,
        asset_id: options.asset_id,
        source_digest,
        profile: profile.id,
        files: partition
            .files
            .iter()
            .map(|file| PartitionFileReport {
                path: file.relative_path.clone(),
                bytes: file.byte_length,
                sha256: file.content_hash.clone(),
            })
            .collect(),
        stats: PartitionStatsReport::from(&partition.stats),
        metrics: artifact.metrics,
        preparation: artifact.preparation,
        budget: artifact.budget,
        warnings,
    })
}

fn validate_options(options: &PartitionOptions) -> Result<(), PartitionError> {
    if options.asset_id.trim().is_empty() {
        return Err(PartitionError::InvalidOptions(
            "asset ID must not be empty".into(),
        ));
    }
    validate_crease_angle(options.crease_angle_degrees)
        .map_err(|error| PartitionError::InvalidOptions(error.to_string()))?;
    let profile = profile_from_options(options);
    profile
        .validate()
        .map_err(|error| PartitionError::InvalidOptions(error.to_string()))
}

fn profile_from_options(options: &PartitionOptions) -> AssetProfile {
    let mut profile = AssetProfile::direct_partition(options.max_triangles, options.max_bytes);
    profile.crease_angle_degrees = options.crease_angle_degrees;
    profile.generate_edges = !options.skip_edges;
    profile.render_batch = options.render_batch;
    profile.lod_enabled = options.lod_enabled;
    profile.lod_ratio = options.lod_ratio;
    profile.lod_allocation = options.lod_allocation;
    profile.lod_max_triangles = options.lod_max_triangles;
    profile.lod_min_unit_triangles = options.lod_min_unit_triangles;
    profile.lod_curve_samples = options.lod_curve_samples;
    profile.lod_max_memory_bytes = options.lod_max_memory_bytes;
    profile.semantic_proxy = options.semantic_proxy;
    profile.coarse_chunk_max_bytes = options.coarse_chunk_max_bytes.unwrap_or(options.max_bytes);
    profile
}

fn resolve_budget(value: Option<&str>) -> Result<BudgetProfile, PartitionError> {
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

fn reject_warnings_as_errors(
    options: &PartitionOptions,
    budget: &BudgetResult,
    warnings: &[PrepareWarning],
) -> Result<(), PartitionError> {
    if !options.warnings_as_errors {
        return Ok(());
    }
    if budget.status == BudgetStatus::Warning {
        return Err(PartitionError::WarningAsError {
            code: "ICE3_STUDIO_ASSET_BUDGET_WARNING".into(),
            message: "asset exceeds one or more soft budget limits".into(),
        });
    }
    if let Some(warning) = warnings.first() {
        return Err(PartitionError::WarningAsError {
            code: warning.code.clone(),
            message: warning.message.clone(),
        });
    }
    Ok(())
}

fn merge_warnings(source: &[ImportWarning], prepared: &[PrepareWarning]) -> Vec<PrepareWarning> {
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

fn output_parent(output: &Path) -> PathBuf {
    output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

fn write_staging(
    parent: &Path,
    artifact: &studio_pipeline::PartitionArtifact,
) -> Result<tempfile::TempDir, PartitionError> {
    let staging = tempfile::Builder::new()
        .prefix(".ice3-studio-partition-staging-")
        .tempdir_in(parent)?;
    for file in &artifact.files {
        let path = staging.path().join(&file.relative_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, &file.bytes)?;
    }
    let mut tileset = serde_json::to_vec_pretty(&artifact.tileset)
        .map_err(|error| PartitionError::Validation(error.to_string()))?;
    tileset.push(b'\n');
    std::fs::write(staging.path().join("tileset.json"), tileset)?;
    Ok(staging)
}

fn publish_directory(
    staging: tempfile::TempDir,
    output: &Path,
    force: bool,
) -> Result<(), PartitionError> {
    publish_directory_with(staging, output, force, |from, to| std::fs::rename(from, to))
}

fn publish_directory_with(
    staging: tempfile::TempDir,
    output: &Path,
    force: bool,
    publish: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
) -> Result<(), PartitionError> {
    if output.exists() && !force {
        return Err(PartitionError::OutputExists);
    }
    if !force {
        if output.exists() {
            return Err(PartitionError::OutputExists);
        }
        publish(staging.path(), output)?;
        return Ok(());
    }

    let parent = output_parent(output);
    let backup = tempfile::Builder::new()
        .prefix(".ice3-studio-partition-backup-")
        .tempdir_in(parent)?;
    std::fs::remove_dir(backup.path())?;
    let had_output = output.exists();
    if had_output {
        std::fs::rename(output, backup.path())?;
    }
    if let Err(error) = publish(staging.path(), output) {
        let primary = PartitionError::from(error);
        if had_output && let Err(restore_error) = std::fs::rename(backup.path(), output) {
            let backup_path = backup.keep();
            return Err(PartitionError::RollbackFailed {
                primary: Box::new(primary),
                details: vec![
                    restore_error.to_string(),
                    format!("backup preserved at {}", backup_path.display()),
                ],
            });
        }
        return Err(primary);
    }
    if had_output {
        backup.close()?;
    }
    Ok(())
}

impl From<ImportError> for PartitionError {
    fn from(error: ImportError) -> Self {
        Self::Import(error.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forced_publish_restores_the_previous_directory_when_replacement_fails() {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("dist");
        std::fs::create_dir(&output).unwrap();
        std::fs::write(output.join("keep.txt"), "keep").unwrap();
        let staging = tempfile::Builder::new()
            .prefix("replacement-")
            .tempdir_in(root.path())
            .unwrap();
        std::fs::write(staging.path().join("new.txt"), "new").unwrap();

        let result = publish_directory_with(staging, &output, true, |_, _| {
            Err(std::io::Error::other("injected replacement failure"))
        });

        assert!(result.is_err());
        assert_eq!(
            std::fs::read_to_string(output.join("keep.txt")).unwrap(),
            "keep"
        );
        assert!(!output.join("new.txt").exists());
    }
}
