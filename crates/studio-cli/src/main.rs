use std::path::PathBuf;

use clap::{Parser, Subcommand};
use studio_cli::schema;
use studio_core::gpu_contract::GeometryBackendMode;
use studio_project::{ProjectStore, WorkspacePath};
use studio_server::capabilities::WriteMode;
use studio_server::runtime_tileset::RuntimeTilesetExporter;
use studio_server::{ServerConfig, serve};

#[derive(Debug, Parser)]
#[command(name = "ice3-studio")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
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
        #[arg(long, default_value = "agent-console-v1")]
        profile: String,
        #[arg(long)]
        force: bool,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.command {
        Command::Export { command } => match command {
            ExportCommand::Tileset {
                project,
                output,
                profile,
                force,
            } => {
                if profile != "agent-console-v1" {
                    return Err(format!("unsupported runtime profile: {profile}").into());
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
                let result =
                    RuntimeTilesetExporter::new(output, force).export(&document, &project_dir)?;
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
                toolchain_cli: None,
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

fn parse_compute_mode(value: &str) -> Result<GeometryBackendMode, String> {
    match value {
        "cpu" => Ok(GeometryBackendMode::Cpu),
        "auto" => Ok(GeometryBackendMode::Auto),
        "gpu" => Ok(GeometryBackendMode::Gpu),
        _ => Err("native GPU mode must be cpu, auto, or gpu".into()),
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
}
