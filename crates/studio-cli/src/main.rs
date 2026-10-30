use std::path::PathBuf;

use clap::{Parser, Subcommand};
use studio_cli::schema;
use studio_server::capabilities::WriteMode;
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
        #[arg(long)]
        fixture: Option<PathBuf>,
        #[arg(long, default_value_t = 8120)]
        port: u16,
        #[arg(long, default_value = "review-required")]
        write_mode: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.command {
        Command::Schema { output, check } => schema::generate(&output, check)?,
        Command::Serve {
            workspace,
            project,
            fixture,
            port,
            write_mode,
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
            })
            .await?;
        }
    }
    Ok(())
}
