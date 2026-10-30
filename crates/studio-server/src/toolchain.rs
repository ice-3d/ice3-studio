use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use studio_toolchain_client::{
    ToolchainAssetResult, ToolchainClient, ToolchainClientError, ToolchainLaunchConfig,
    ToolchainState,
};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ToolchainStatus {
    pub state: ToolchainState,
    pub contract_version: Option<String>,
    pub package_version: Option<String>,
    pub capabilities: Vec<String>,
    pub reason: Option<String>,
}

impl ToolchainStatus {
    fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            state: ToolchainState::Unavailable,
            contract_version: None,
            package_version: None,
            capabilities: Vec::new(),
            reason: Some(reason.into()),
        }
    }

    fn incompatible(reason: impl Into<String>) -> Self {
        Self {
            state: ToolchainState::Incompatible,
            contract_version: None,
            package_version: None,
            capabilities: Vec::new(),
            reason: Some(reason.into()),
        }
    }
}

pub struct ToolchainService {
    workspace_root: PathBuf,
    launch: Mutex<Option<ToolchainLaunchConfig>>,
    client: Mutex<Option<Arc<ToolchainClient>>>,
    status: RwLock<ToolchainStatus>,
}

impl ToolchainService {
    pub fn new(workspace_root: &Path, toolchain_cli: Option<PathBuf>) -> Self {
        let root = workspace_root
            .canonicalize()
            .unwrap_or_else(|_| workspace_root.to_path_buf());
        let service = Self {
            workspace_root: root,
            launch: Mutex::new(None),
            client: Mutex::new(None),
            status: RwLock::new(ToolchainStatus::unavailable(
                "Toolchain sidecar is not configured",
            )),
        };
        if let Some(script) = toolchain_cli {
            service.reconfigure(script);
        } else if let Some(application_root) = std::env::var_os("STUDIO_APP_ROOT") {
            service.configure_managed(PathBuf::from(application_root));
        }
        service
    }

    pub fn reconfigure(&self, script: PathBuf) {
        let config = match self.build_launch(script) {
            Ok(config) => config,
            Err(error) => {
                *self
                    .launch
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
                *self
                    .status
                    .write()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                    ToolchainStatus::unavailable(error);
                return;
            }
        };
        self.activate(config);
    }

    pub fn configure_managed(&self, application_root: PathBuf) {
        match ToolchainLaunchConfig::from_manifest(application_root)
            .map(|config| config.with_workspace_root(self.workspace_root.clone()))
        {
            Ok(config) => self.activate(config),
            Err(error) => {
                *self
                    .launch
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
                *self
                    .status
                    .write()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                    if matches!(error, ToolchainClientError::Incompatible { .. }) {
                        ToolchainStatus::incompatible(error.to_string())
                    } else {
                        ToolchainStatus::unavailable(error.to_string())
                    };
            }
        }
    }

    fn activate(&self, config: ToolchainLaunchConfig) {
        *self
            .launch
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(config.clone());
        let result = ToolchainClient::spawn(config).and_then(|client| {
            let capabilities = client.capabilities()?;
            Ok((client, capabilities))
        });
        match result {
            Ok((client, capabilities)) => {
                *self
                    .client
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Arc::new(client));
                *self
                    .status
                    .write()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = ToolchainStatus {
                    state: ToolchainState::Ready,
                    contract_version: Some(capabilities.contract_version),
                    package_version: Some(capabilities.package_version),
                    capabilities: capabilities.operations,
                    reason: None,
                };
            }
            Err(error) => {
                *self
                    .client
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
                *self
                    .status
                    .write()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                    if matches!(error, ToolchainClientError::Incompatible { .. }) {
                        ToolchainStatus::incompatible(error.to_string())
                    } else {
                        ToolchainStatus::unavailable(error.to_string())
                    };
            }
        }
    }

    pub fn status(&self) -> ToolchainStatus {
        self.status
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn shutdown(&self) {
        *self
            .client
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        let mut status = self
            .status
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        status.state = ToolchainState::Unavailable;
        status.reason = Some("Toolchain sidecar is stopped".into());
    }

    pub fn preview(
        &self,
        source_path: &Path,
        output_path: &Path,
        asset_id: &str,
    ) -> Result<ToolchainAssetResult, ToolchainClientError> {
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| ToolchainClientError::Protocol {
                message: format!("preview output directory failed: {error}"),
            })?;
        }
        let params = json!({
            "sourcePath": relative_path(&self.workspace_root, source_path)?,
            "outputPath": relative_path(&self.workspace_root, output_path)?,
            "assetId": asset_id,
            "budget": "unrestricted"
        });
        self.with_retry(|client| client.asset_preview(params.clone()))
    }

    fn with_retry<T>(
        &self,
        mut operation: impl FnMut(&ToolchainClient) -> Result<T, ToolchainClientError>,
    ) -> Result<T, ToolchainClientError> {
        self.ensure_client()?;
        let first = {
            let client = self
                .client
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .as_ref()
                .cloned()
                .ok_or_else(|| ToolchainClientError::Unavailable {
                    reason: "Toolchain sidecar is not ready".into(),
                })?;
            operation(&client)
        };
        match first {
            Err(ToolchainClientError::Crashed { .. }) => {
                self.restart()?;
                let client = self
                    .client
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .as_ref()
                    .cloned()
                    .ok_or_else(|| ToolchainClientError::Unavailable {
                        reason: "Toolchain sidecar restart failed".into(),
                    })?;
                operation(&client)
            }
            result => result,
        }
    }

    fn ensure_client(&self) -> Result<(), ToolchainClientError> {
        if self
            .client
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_some()
        {
            return Ok(());
        }
        self.restart()
    }

    fn restart(&self) -> Result<(), ToolchainClientError> {
        let config = self
            .launch
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
            .ok_or_else(|| ToolchainClientError::Unavailable {
                reason: self
                    .status()
                    .reason
                    .unwrap_or_else(|| "Toolchain sidecar is not configured".into()),
            })?;
        let client = ToolchainClient::spawn(config)?;
        let capabilities = client.capabilities()?;
        *self
            .status
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = ToolchainStatus {
            state: ToolchainState::Ready,
            contract_version: Some(capabilities.contract_version),
            package_version: Some(capabilities.package_version),
            capabilities: capabilities.operations,
            reason: None,
        };
        *self
            .client
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Arc::new(client));
        Ok(())
    }

    fn build_launch(&self, script: PathBuf) -> Result<ToolchainLaunchConfig, String> {
        let script = script.canonicalize().map_err(|error| error.to_string())?;
        if !script.is_file() || !script.starts_with(&self.workspace_root) {
            return Err("Toolchain sidecar must be a file inside the workspace".into());
        }
        Ok(ToolchainLaunchConfig {
            program: node_binary()?,
            args: vec![
                script,
                "sidecar".into(),
                "--workspace".into(),
                self.workspace_root.clone(),
            ],
            environment: toolchain_environment(),
            workspace_root: self.workspace_root.clone(),
            request_timeout: Duration::from_secs(30),
        })
    }
}

fn relative_path(root: &Path, path: &Path) -> Result<String, ToolchainClientError> {
    let canonical_path = path
        .canonicalize()
        .or_else(|_| {
            let parent = path.parent().ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "path has no parent")
            })?;
            let file_name = path.file_name().ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "path has no file name")
            })?;
            Ok::<_, std::io::Error>(parent.canonicalize()?.join(file_name))
        })
        .map_err(|error| ToolchainClientError::Protocol {
            message: format!("Toolchain asset path is invalid: {error}"),
        })?;
    let relative =
        canonical_path
            .strip_prefix(root)
            .map_err(|_| ToolchainClientError::Protocol {
                message: "Toolchain asset path is outside the workspace".into(),
            })?;
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

fn node_binary() -> Result<PathBuf, String> {
    if let Some(node) = std::env::var_os("STUDIO_NODE") {
        return Ok(PathBuf::from(node));
    }
    let executable = if cfg!(windows) { "node.exe" } else { "node" };
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .map(|directory| directory.join(executable))
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| "Node.js runtime is unavailable".into())
}

fn toolchain_environment() -> Vec<(String, String)> {
    const PASSTHROUGH: [&str; 8] = [
        "PATH",
        "ICE3_TOOLCHAIN_CLI",
        "BLENDER_BIN",
        "ICE3_JT_CONVERTER",
        "ICE3_FBX_TIMEOUT_MS",
        "ICE3_USD_TIMEOUT_MS",
        "ICE3_JT_TIMEOUT_MS",
        "STUDIO_NODE",
    ];
    PASSTHROUGH
        .into_iter()
        .filter_map(|name| std::env::var(name).ok().map(|value| (name.into(), value)))
        .collect()
}
