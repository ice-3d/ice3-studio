use std::path::PathBuf;

use clap::{Parser, Subcommand};
use studio_cli::schema;
use studio_server::{ServerConfig, serve};

#[derive(Debug, Parser)]
#[command(name = "ice3-studio")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Schema {
        #[arg(long, default_value = "schemas")]
        output: PathBuf,
        #[arg(long)]
        check: bool,
    },
    Serve {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        project: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.command {
        Command::Schema { output, check } => schema::generate(&output, check)?,
        Command::Serve { workspace, project } => {
            serve(ServerConfig {
                workspace_root: workspace,
                project_id: project,
                token: None,
            })
            .await?;
        }
    }
    Ok(())
}
