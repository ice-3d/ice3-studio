use std::path::PathBuf;

use clap::{Parser, Subcommand};
use serde::Serialize;
use studio_assets::{AssetError, read_glb, validate_prepared_profile};
use studio_cli::import::{
    ImportOptions, execute_internal_worker, import_limits_from_environment, import_source,
};
use studio_cli::partition::{PartitionOptions, partition_source};
use studio_cli::prepare_command::{InspectOptions, PrepareCommandOptions, run_build, run_inspect};
use studio_cli::schema;
use studio_core::gpu_contract::GeometryBackendMode;
use studio_pipeline::{LodAllocationMode, RenderBatchMode, SemanticProxyMode};
use studio_project::{ProjectStore, WorkspacePath};
use studio_server::capabilities::WriteMode;
use studio_server::runtime_tileset::RuntimeTilesetExporter;
use studio_server::{ServerConfig, serve};

#[cfg(debug_assertions)]
const TEST_WORKER_CRASH_ENV: &str = "ICE3_STUDIO_IMPORT_WORKER_CRASH";

#[derive(Debug, Parser)]
#[command(name = "ice3-studio")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Import a source asset and write a Prepared GLB.
    Import {
        source: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        asset_id: String,
        #[arg(
            long,
            default_value = "auto",
            value_parser = ["auto", "gltf", "obj", "fbx", "usd", "step", "ifc", "jt"]
        )]
        adapter: String,
        #[arg(long)]
        warnings_as_errors: bool,
        #[arg(long)]
        force: bool,
    },
    /// Build a source asset with prepared render edges.
    Build {
        source: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        asset_id: String,
        #[arg(
            long,
            default_value = "auto",
            value_parser = ["auto", "gltf", "obj", "fbx", "usd", "step", "ifc", "jt"]
        )]
        adapter: String,
        #[arg(long, default_value_t = 30.0)]
        crease_angle: f32,
        #[arg(long)]
        skip_edges: bool,
        #[arg(long)]
        budget: Option<String>,
        #[arg(long)]
        report: Option<PathBuf>,
        #[arg(long)]
        warnings_as_errors: bool,
        #[arg(long)]
        force: bool,
        #[arg(long)]
        json: bool,
    },
    /// Inspect a source or Prepared GLB without publishing files.
    Inspect {
        source: PathBuf,
        #[arg(
            long,
            default_value = "auto",
            value_parser = ["auto", "gltf", "obj", "fbx", "usd", "step", "ifc", "jt"]
        )]
        adapter: String,
        #[arg(long)]
        crease_angle: Option<f32>,
        #[arg(long)]
        skip_edges: bool,
        #[arg(long)]
        budget: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Partition any registered source into a runtime Tileset directory.
    Partition {
        source: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        asset_id: String,
        #[arg(
            long,
            default_value = "auto",
            value_parser = ["auto", "gltf", "obj", "fbx", "usd", "step", "ifc", "jt"]
        )]
        adapter: String,
        #[arg(long, default_value_t = 500_000)]
        max_triangles: u64,
        #[arg(long, default_value_t = 33_554_432)]
        max_bytes: usize,
        #[arg(long, default_value_t = 30.0)]
        crease_angle: f32,
        #[arg(long)]
        skip_edges: bool,
        #[arg(long)]
        budget: Option<String>,
        #[arg(long, default_value = "off", value_parser = ["off", "static"])]
        render_batch: String,
        #[arg(long)]
        lod: bool,
        #[arg(long, default_value_t = 0.15)]
        lod_ratio: f64,
        #[arg(
            long,
            default_value = "proportional",
            value_parser = ["proportional", "adaptive"]
        )]
        lod_allocation: String,
        #[arg(long, default_value_t = 250_000)]
        lod_max_triangles: u64,
        #[arg(long, default_value_t = 8)]
        lod_min_unit_triangles: u64,
        #[arg(long, default_value_t = 8)]
        lod_curve_samples: u32,
        #[arg(long, default_value_t = 67_108_864)]
        lod_max_memory_bytes: usize,
        #[arg(long, default_value = "off", value_parser = ["off", "auto"])]
        semantic_proxy: String,
        #[arg(long)]
        coarse_chunk_max_bytes: Option<usize>,
        #[arg(long)]
        warnings_as_errors: bool,
        #[arg(long)]
        force: bool,
        #[arg(long)]
        json: bool,
    },
    /// Validate a GLB, glTF, or Prepared GLB.
    Validate { source: PathBuf },
    /// Export a committed Studio revision as a runtime Tileset bundle.
    Export {
        #[command(subcommand)]
        command: ExportCommand,
    },
    Schema {
        #[arg(long, default_value = "schemas")]
        output: PathBuf,
        #[arg(long)]
        check: bool,
    },
    /// 把工程目录打包成 `.ice3` 单文件容器。
    Pack {
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        output: PathBuf,
        /// 按指定的 metadata 键分组生成 resources 概览（可选）。
        #[arg(long)]
        group_by: Option<String>,
    },
    /// 从 `.ice3` 容器还原工程目录。
    Unpack {
        archive: PathBuf,
        #[arg(long)]
        into: PathBuf,
    },
    Serve {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        project: String,
        #[arg(long)]
        fixture: Option<PathBuf>,
        #[arg(long, default_value_t = 8120)]
        port: u16,
        #[arg(long, default_value = "review-required")]
        write_mode: String,
        #[arg(long, default_value = "cpu", value_parser = parse_compute_mode)]
        native_gpu: GeometryBackendMode,
    },
}

#[derive(Debug, Subcommand)]
enum ExportCommand {
    Tileset {
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        asset_id: Option<String>,
        #[arg(long, default_value = "agent-console-v1")]
        profile: String,
        #[arg(long)]
        force: bool,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "jt")]
    if studio_import::jt::run_jt_converter_supervisor(std::env::args_os().skip(1))? {
        return Ok(());
    }

    if let Some(request_path) = internal_worker_request()? {
        #[cfg(debug_assertions)]
        if std::env::var_os(TEST_WORKER_CRASH_ENV)
            .is_some_and(|value| value == std::ffi::OsStr::new("1"))
        {
            std::process::abort();
        }
        execute_internal_worker(&request_path).map_err(|error| CliFailure(error.to_string()))?;
        return Ok(());
    }

    let cli = Cli::parse();
    match cli.command {
        Command::Import {
            source,
            output,
            asset_id,
            adapter,
            warnings_as_errors,
            force,
        } => {
            let limits =
                import_limits_from_environment().map_err(|error| CliFailure(error.to_string()))?;
            let report = import_source(ImportOptions {
                source,
                output,
                asset_id,
                adapter,
                warnings_as_errors,
                force,
                limits,
            })
            .map_err(|error| CliFailure(error.to_string()))?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Command::Build {
            source,
            output,
            asset_id,
            adapter,
            crease_angle,
            skip_edges,
            budget,
            report,
            warnings_as_errors,
            force,
            json,
        } => {
            let limits =
                import_limits_from_environment().map_err(|error| CliFailure(error.to_string()))?;
            let output = run_build(PrepareCommandOptions {
                source,
                output,
                asset_id,
                adapter,
                crease_angle_degrees: crease_angle,
                skip_edges,
                budget,
                report,
                warnings_as_errors,
                force,
                limits,
            })
            .map_err(|error| CliFailure(error.to_string()))?;
            print_json(&output.report, json)?;
        }
        Command::Inspect {
            source,
            adapter,
            crease_angle,
            skip_edges,
            budget,
            json,
        } => {
            let limits =
                import_limits_from_environment().map_err(|error| CliFailure(error.to_string()))?;
            let report = run_inspect(InspectOptions {
                source,
                adapter,
                crease_angle_degrees: crease_angle,
                skip_edges,
                budget,
                limits,
            })
            .map_err(|error| CliFailure(error.to_string()))?;
            print_json(&report, json)?;
        }
        Command::Partition {
            source,
            output,
            asset_id,
            adapter,
            max_triangles,
            max_bytes,
            crease_angle,
            skip_edges,
            budget,
            render_batch,
            lod,
            lod_ratio,
            lod_allocation,
            lod_max_triangles,
            lod_min_unit_triangles,
            lod_curve_samples,
            lod_max_memory_bytes,
            semantic_proxy,
            coarse_chunk_max_bytes,
            warnings_as_errors,
            force,
            json,
        } => {
            let limits =
                import_limits_from_environment().map_err(|error| CliFailure(error.to_string()))?;
            let report = partition_source(PartitionOptions {
                source,
                output,
                asset_id,
                adapter,
                max_triangles,
                max_bytes,
                crease_angle_degrees: crease_angle,
                skip_edges,
                budget,
                render_batch: parse_render_batch(&render_batch)?,
                lod_enabled: lod,
                lod_ratio,
                lod_allocation: parse_lod_allocation(&lod_allocation)?,
                lod_max_triangles,
                lod_min_unit_triangles,
                lod_curve_samples,
                lod_max_memory_bytes,
                semantic_proxy: parse_semantic_proxy(&semantic_proxy)?,
                coarse_chunk_max_bytes,
                warnings_as_errors,
                force,
                limits,
            })
            .map_err(|error| CliFailure(error.to_string()))?;
            print_json(&report, json)?;
        }
        Command::Validate { source } => {
            let parsed = read_glb(&source)?;
            let report = match validate_prepared_profile(&parsed) {
                Ok(profile) => serde_json::json!({
                    "valid": true,
                    "kind": "prepared",
                    "assetId": profile.asset_id,
                    "profileVersion": profile.profile_version
                }),
                Err(AssetError::ProfileUnsupported) => serde_json::json!({
                    "valid": true,
                    "kind": "source",
                    "sourceDigest": parsed.digest
                }),
                Err(error) => return Err(error.into()),
            };
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Command::Export { command } => match command {
            ExportCommand::Tileset {
                project,
                output,
                asset_id,
                profile,
                force,
            } => {
                if profile != "agent-console-v1" {
                    return Err(format!(
                        "ICE3_STUDIO_ASSET_TILESET_INVALID: unsupported runtime profile: {profile}"
                    )
                    .into());
                }
                let project_id = project
                    .file_name()
                    .and_then(|value| value.to_str())
                    .ok_or("project path must end with a project ID")?;
                let workspace_root = project
                    .parent()
                    .ok_or("project path must have a parent directory")?;
                let workspace = WorkspacePath::new(workspace_root, project_id)?;
                let store = ProjectStore::open_read_only(workspace)?;
                let document = store.load_current_read_only()?;
                let project_dir = store.project_dir().to_path_buf();
                drop(store);
                let mut exporter = RuntimeTilesetExporter::new(output, force);
                if let Some(asset_id) = asset_id {
                    exporter = exporter.with_asset_id(asset_id);
                }
                let result = exporter.export(&document, &project_dir)?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
        },
        Command::Schema { output, check } => schema::generate(&output, check)?,
        Command::Pack {
            project,
            output,
            group_by,
        } => {
            let manifest = studio_project::archive::pack_with_grouping(
                &project,
                &output,
                group_by.as_deref(),
            )?;
            println!("{}", serde_json::to_string_pretty(&manifest)?);
        }
        Command::Unpack { archive, into } => {
            let manifest = studio_project::archive::unpack(&archive, &into)?;
            println!("{}", serde_json::to_string_pretty(&manifest)?);
        }
        Command::Serve {
            workspace,
            project,
            fixture,
            port,
            write_mode,
            native_gpu,
        } => {
            let write_mode = WriteMode::parse(&write_mode)?;
            serve(ServerConfig {
                workspace_root: workspace,
                project_id: project,
                token: None,
                fixture,
                port,
                approval_token: None,
                write_mode,
                viewer_dir: std::env::var_os("STUDIO_VIEWER_DIR").map(PathBuf::from),
                compute_mode: native_gpu,
                preview_packet_enabled: std::env::var("STUDIO_PREVIEW_PACKETS")
                    .is_ok_and(|value| value == "1"),
                scene_sessions_enabled: std::env::var("STUDIO_GPU_SCENE_SESSIONS")
                    .is_ok_and(|value| value == "1"),
                incremental_scene_cache: std::env::var("STUDIO_INCREMENTAL_SCENE_CACHE")
                    .map_or(true, |value| value != "0"),
            })
            .await?;
        }
    }
    Ok(())
}

fn print_json(value: &impl Serialize, compact: bool) -> Result<(), serde_json::Error> {
    if compact {
        println!("{}", serde_json::to_string(value)?);
    } else {
        println!("{}", serde_json::to_string_pretty(value)?);
    }
    Ok(())
}

fn internal_worker_request() -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    if arguments.first().map(std::ffi::OsString::as_os_str)
        != Some(std::ffi::OsStr::new("--internal-import-worker"))
    {
        return Ok(None);
    }
    let [_, request_path] = arguments.as_slice() else {
        return Err("expected --internal-import-worker REQUEST_PATH".into());
    };
    Ok(Some(PathBuf::from(request_path)))
}

struct CliFailure(String);

impl std::fmt::Debug for CliFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::fmt::Display for CliFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for CliFailure {}

fn parse_compute_mode(value: &str) -> Result<GeometryBackendMode, String> {
    match value {
        "cpu" => Ok(GeometryBackendMode::Cpu),
        "auto" => Ok(GeometryBackendMode::Auto),
        "gpu" => Ok(GeometryBackendMode::Gpu),
        _ => Err("native GPU mode must be cpu, auto, or gpu".into()),
    }
}

fn parse_render_batch(value: &str) -> Result<RenderBatchMode, Box<dyn std::error::Error>> {
    match value {
        "off" => Ok(RenderBatchMode::Off),
        "static" => Ok(RenderBatchMode::Static),
        _ => Err("render batch mode must be off or static".into()),
    }
}

fn parse_lod_allocation(value: &str) -> Result<LodAllocationMode, Box<dyn std::error::Error>> {
    match value {
        "proportional" => Ok(LodAllocationMode::Proportional),
        "adaptive" => Ok(LodAllocationMode::Adaptive),
        _ => Err("LOD allocation must be proportional or adaptive".into()),
    }
}

fn parse_semantic_proxy(value: &str) -> Result<SemanticProxyMode, Box<dyn std::error::Error>> {
    match value {
        "off" => Ok(SemanticProxyMode::Off),
        "auto" => Ok(SemanticProxyMode::Auto),
        _ => Err("semantic proxy mode must be off or auto".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_runtime_tileset_export_command() {
        let cli = Cli::try_parse_from([
            "ice3-studio",
            "export",
            "tileset",
            "--project",
            "project",
            "--output",
            "output",
            "--profile",
            "agent-console-v1",
            "--force",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Command::Export {
                command: ExportCommand::Tileset { force: true, .. }
            }
        ));
    }

    #[test]
    fn parses_import_and_validate_commands() {
        let import = Cli::try_parse_from([
            "ice3-studio",
            "import",
            "plant.obj",
            "--output",
            "plant.glb",
            "--asset-id",
            "plant",
            "--adapter",
            "obj",
            "--warnings-as-errors",
            "--force",
        ])
        .unwrap();
        assert!(matches!(
            import.command,
            Command::Import {
                adapter,
                warnings_as_errors: true,
                force: true,
                ..
            } if adapter == "obj"
        ));

        let validate = Cli::try_parse_from(["ice3-studio", "validate", "plant.glb"]).unwrap();
        assert!(matches!(validate.command, Command::Validate { .. }));
    }

    #[test]
    fn restricts_import_adapters_to_the_native_registry() {
        for adapter in ["auto", "gltf", "obj", "fbx", "usd", "step", "ifc", "jt"] {
            let cli = Cli::try_parse_from([
                "ice3-studio",
                "import",
                "plant.test",
                "--output",
                "plant.glb",
                "--asset-id",
                "plant",
                "--adapter",
                adapter,
            ])
            .unwrap();
            assert!(matches!(
                cli.command,
                Command::Import { adapter: parsed, .. } if parsed == adapter
            ));
        }
        assert!(
            Cli::try_parse_from([
                "ice3-studio",
                "import",
                "plant.test",
                "--output",
                "plant.glb",
                "--asset-id",
                "plant",
                "--adapter",
                "external",
            ])
            .is_err()
        );
    }

    #[test]
    fn parses_partition_defaults_and_explicit_options() {
        let defaults = Cli::try_parse_from([
            "ice3-studio",
            "partition",
            "plant.gltf",
            "--output",
            "dist",
            "--asset-id",
            "plant",
        ])
        .unwrap();
        assert!(matches!(
            defaults.command,
            Command::Partition {
                adapter,
                max_triangles: 500_000,
                max_bytes: 33_554_432,
                crease_angle,
                skip_edges: false,
                render_batch,
                lod: false,
                lod_ratio,
                lod_allocation,
                lod_max_triangles: 250_000,
                lod_min_unit_triangles: 8,
                lod_curve_samples: 8,
                lod_max_memory_bytes: 67_108_864,
                semantic_proxy,
                coarse_chunk_max_bytes: None,
                force: false,
                ..
            } if adapter == "auto"
                && (crease_angle - 30.0).abs() < f32::EPSILON
                && render_batch == "off"
                && (lod_ratio - 0.15).abs() < f64::EPSILON
                && lod_allocation == "proportional"
                && semantic_proxy == "off"
        ));

        let explicit = Cli::try_parse_from([
            "ice3-studio",
            "partition",
            "plant.gltf",
            "--output",
            "dist",
            "--asset-id",
            "plant",
            "--adapter",
            "gltf",
            "--render-batch",
            "static",
            "--lod",
            "--lod-allocation",
            "adaptive",
            "--semantic-proxy",
            "auto",
            "--coarse-chunk-max-bytes",
            "1024",
            "--force",
        ])
        .unwrap();
        assert!(matches!(
            explicit.command,
            Command::Partition {
                adapter,
                render_batch,
                lod: true,
                lod_allocation,
                semantic_proxy,
                coarse_chunk_max_bytes: Some(1024),
                force: true,
                ..
            } if adapter == "gltf"
                && render_batch == "static"
                && lod_allocation == "adaptive"
                && semantic_proxy == "auto"
        ));
    }
}
