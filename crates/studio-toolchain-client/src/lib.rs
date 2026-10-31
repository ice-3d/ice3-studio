//! Local process client for the ICE3 Toolchain sidecar contract.

mod client;
pub mod protocol;
mod workspace_path;

pub use client::{ToolchainClient, ToolchainClientError, ToolchainLaunchConfig};
pub use protocol::*;
pub use workspace_path::validate_workspace_path;
