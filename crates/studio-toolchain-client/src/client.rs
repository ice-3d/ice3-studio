use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::protocol::{
    RpcEnvelope, ToolchainAssetResult, ToolchainCapabilities, ToolchainPartitionResult,
    ToolchainValidationResult,
};
use crate::validate_workspace_path;

const MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024;
const MAX_STDERR_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone)]
pub struct ToolchainLaunchConfig {
    pub program: PathBuf,
    pub args: Vec<PathBuf>,
    pub environment: Vec<(String, String)>,
    pub workspace_root: PathBuf,
    pub request_timeout: Duration,
}

impl ToolchainLaunchConfig {
    pub fn from_manifest(
        application_root: impl AsRef<std::path::Path>,
    ) -> Result<Self, ToolchainClientError> {
        let application_root = application_root.as_ref().canonicalize().map_err(|error| {
            ToolchainClientError::Unavailable {
                reason: error.to_string(),
            }
        })?;
        let manifest_path = application_root.join("toolchain/manifest.json");
        let manifest: ToolchainManifest =
            serde_json::from_slice(&std::fs::read(&manifest_path).map_err(|error| {
                ToolchainClientError::Unavailable {
                    reason: error.to_string(),
                }
            })?)
            .map_err(|error| ToolchainClientError::protocol(error.to_string()))?;
        if !manifest.contract_version.starts_with("1.") {
            return Err(ToolchainClientError::Incompatible {
                required: "1.x".into(),
                received: manifest.contract_version,
            });
        }
        if manifest.platform != current_platform()
            || manifest.architecture != current_architecture()
        {
            return Err(ToolchainClientError::Incompatible {
                required: format!("{}/{}", current_platform(), current_architecture()),
                received: format!("{}/{}", manifest.platform, manifest.architecture),
            });
        }
        let entrypoint = validate_workspace_path(&application_root, &manifest.entrypoint)?;
        let package_entry = validate_workspace_path(&application_root, &manifest.package_entry)?;
        for (path, expected) in &manifest.files {
            let absolute = validate_workspace_path(&application_root, path)?;
            let digest = Sha256::digest(std::fs::read(&absolute).map_err(|error| {
                ToolchainClientError::Unavailable {
                    reason: error.to_string(),
                }
            })?);
            if format!("{digest:x}") != *expected {
                return Err(ToolchainClientError::protocol(format!(
                    "toolchain bundle hash mismatch: {path}"
                )));
            }
        }
        Ok(Self {
            program: entrypoint,
            args: vec![package_entry, "sidecar".into()],
            environment: std::env::var_os("PATH")
                .map(|path| vec![("PATH".into(), path.to_string_lossy().into_owned())])
                .unwrap_or_default(),
            workspace_root: application_root,
            request_timeout: Duration::from_secs(30),
        })
    }

    pub fn with_workspace_root(mut self, workspace_root: impl Into<PathBuf>) -> Self {
        self.workspace_root = workspace_root.into();
        self.args.push("--workspace".into());
        self.args.push(self.workspace_root.clone());
        self
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToolchainManifest {
    contract_version: String,
    #[allow(dead_code)]
    package_version: String,
    #[allow(dead_code)]
    node_version: String,
    platform: String,
    architecture: String,
    entrypoint: String,
    package_entry: String,
    #[allow(dead_code)]
    sha256: String,
    files: HashMap<String, String>,
}

#[derive(Debug, Error)]
pub enum ToolchainClientError {
    #[error("Toolchain sidecar is unavailable: {reason}")]
    Unavailable { reason: String },
    #[error("Toolchain contract is incompatible: required {required}, received {received}")]
    Incompatible { required: String, received: String },
    #[error("Toolchain request {method} timed out")]
    Timeout { method: String },
    #[error("Toolchain protocol error: {message}")]
    Protocol { message: String },
    #[error("Toolchain rejected {method}: {code}: {message}")]
    Rejected {
        method: String,
        code: String,
        message: String,
        retryable: bool,
    },
    #[error("Toolchain sidecar exited: {message}")]
    Crashed { message: String },
}

impl ToolchainClientError {
    pub(crate) fn protocol(message: impl Into<String>) -> Self {
        Self::Protocol {
            message: message.into(),
        }
    }
}

enum PendingReply {
    Response(Value),
    Protocol(String),
    Closed(String),
}

pub struct ToolchainClient {
    child: Mutex<Child>,
    stdin: Mutex<Option<ChildStdin>>,
    pending: Arc<Mutex<HashMap<u64, Sender<PendingReply>>>>,
    next_id: AtomicU64,
    workspace_root: PathBuf,
    request_timeout: Duration,
    stderr: Arc<Mutex<VecDeque<String>>>,
    reader: Option<JoinHandle<()>>,
    stderr_reader: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for ToolchainClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ToolchainClient")
            .field("workspace_root", &self.workspace_root)
            .field("request_timeout", &self.request_timeout)
            .finish_non_exhaustive()
    }
}

impl ToolchainClient {
    pub fn spawn(config: ToolchainLaunchConfig) -> Result<Self, ToolchainClientError> {
        let workspace_root = config.workspace_root.canonicalize().map_err(|error| {
            ToolchainClientError::Unavailable {
                reason: error.to_string(),
            }
        })?;
        let mut command = Command::new(&config.program);
        command
            .args(&config.args)
            .current_dir(&workspace_root)
            .env_clear()
            .envs(config.environment)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(|error| ToolchainClientError::Unavailable {
                reason: error.to_string(),
            })?;
        let stdin = child.stdin.take();
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ToolchainClientError::Unavailable {
                reason: "sidecar stdout pipe is unavailable".into(),
            })?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| ToolchainClientError::Unavailable {
                reason: "sidecar stderr pipe is unavailable".into(),
            })?;
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let stderr_buffer = Arc::new(Mutex::new(VecDeque::new()));
        let reader_pending = Arc::clone(&pending);
        let reader = thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut bytes = Vec::new();
            loop {
                bytes.clear();
                match reader.read_until(b'\n', &mut bytes) {
                    Ok(0) => {
                        fail_pending(
                            &reader_pending,
                            PendingReply::Closed,
                            "sidecar stdout closed",
                        );
                        break;
                    }
                    Ok(_) => {
                        if bytes.len() > MAX_MESSAGE_BYTES {
                            fail_pending(
                                &reader_pending,
                                PendingReply::Protocol,
                                "sidecar response exceeds 8 MiB",
                            );
                            continue;
                        }
                        let line = String::from_utf8_lossy(&bytes);
                        let Ok(envelope) = serde_json::from_str::<RpcEnvelope>(&line) else {
                            fail_pending(
                                &reader_pending,
                                PendingReply::Protocol,
                                "sidecar stdout contains a non-protocol line",
                            );
                            continue;
                        };
                        if envelope.jsonrpc != "2.0" {
                            fail_pending(
                                &reader_pending,
                                PendingReply::Protocol,
                                "sidecar response uses an unsupported JSON-RPC version",
                            );
                            continue;
                        }
                        let Some(id) = envelope.id else {
                            continue;
                        };
                        let Some(sender) = reader_pending
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .remove(&id)
                        else {
                            continue;
                        };
                        if let Some(error) = envelope.error {
                            let data = error.data.unwrap_or(crate::protocol::RpcErrorData {
                                code: "ICE3_TOOL_SIDECAR_JOB_FAILED".into(),
                                message: error.message,
                                retryable: false,
                            });
                            let _ = sender.send(PendingReply::Response(json!({
                                "__error": {
                                    "jsonRpcCode": error.code,
                                    "code": data.code,
                                    "message": data.message,
                                    "retryable": data.retryable,
                                }
                            })));
                        } else if let Some(result) = envelope.result {
                            let _ = sender.send(PendingReply::Response(result));
                        } else {
                            let _ = sender.send(PendingReply::Protocol(
                                "sidecar response has neither result nor error".into(),
                            ));
                        }
                    }
                    Err(error) => {
                        fail_pending(
                            &reader_pending,
                            PendingReply::Closed,
                            &format!("sidecar stdout failed: {error}"),
                        );
                        break;
                    }
                }
            }
        });
        let reader_stderr = Arc::clone(&stderr_buffer);
        let stderr_reader = thread::spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap_or(0) > 0 {
                let mut buffer = reader_stderr
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                buffer.push_back(line.trim_end().to_owned());
                while buffer.iter().map(String::len).sum::<usize>() > MAX_STDERR_BYTES {
                    buffer.pop_front();
                }
                line.clear();
            }
        });
        Ok(Self {
            child: Mutex::new(child),
            stdin: Mutex::new(stdin),
            pending,
            next_id: AtomicU64::new(1),
            workspace_root,
            request_timeout: config.request_timeout,
            stderr: stderr_buffer,
            reader: Some(reader),
            stderr_reader: Some(stderr_reader),
        })
    }

    pub fn capabilities(&self) -> Result<ToolchainCapabilities, ToolchainClientError> {
        let capabilities: ToolchainCapabilities =
            self.request("toolchain.capabilities", json!({}), self.request_timeout)?;
        let major = capabilities
            .contract_version
            .split('.')
            .next()
            .unwrap_or_default();
        if major != "1" {
            return Err(ToolchainClientError::Incompatible {
                required: "1.x".into(),
                received: capabilities.contract_version,
            });
        }
        Ok(capabilities)
    }

    pub fn asset_validate(
        &self,
        params: Value,
    ) -> Result<ToolchainValidationResult, ToolchainClientError> {
        self.request("asset.validate", params, self.request_timeout)
    }

    pub fn asset_prepare(
        &self,
        params: Value,
    ) -> Result<ToolchainAssetResult, ToolchainClientError> {
        self.asset_result("asset.prepare", params)
    }

    pub fn asset_preview(
        &self,
        params: Value,
    ) -> Result<ToolchainAssetResult, ToolchainClientError> {
        self.asset_result("asset.preview", params)
    }

    pub fn asset_partition(
        &self,
        params: Value,
    ) -> Result<ToolchainPartitionResult, ToolchainClientError> {
        let result: ToolchainPartitionResult =
            self.request("asset.partition", params, self.request_timeout)?;
        validate_workspace_path(&self.workspace_root, &result.tileset_path)?;
        for file in &result.files {
            validate_workspace_path(&self.workspace_root, &file.path)?;
        }
        Ok(result)
    }

    pub fn cancel(&self, request_id: u64) -> Result<(), ToolchainClientError> {
        self.write_message(json!({
            "jsonrpc": "2.0",
            "method": "$/cancel",
            "params": { "requestId": request_id }
        }))
    }

    pub fn request<T: DeserializeOwned>(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<T, ToolchainClientError> {
        if method.trim().is_empty() {
            return Err(ToolchainClientError::protocol("method must not be empty"));
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = mpsc::channel();
        self.pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(id, sender);
        let message = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });
        if let Err(error) = self.write_message(message) {
            self.pending
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(&id);
            return Err(error);
        }
        let reply = match receiver.recv_timeout(timeout) {
            Ok(reply) => reply,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                self.pending
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .remove(&id);
                let _ = self.cancel(id);
                return Err(ToolchainClientError::Timeout {
                    method: method.into(),
                });
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(ToolchainClientError::Crashed {
                    message: "sidecar response channel closed".into(),
                });
            }
        };
        match reply {
            PendingReply::Response(value) => {
                if let Some(error) = value.get("__error") {
                    return Err(ToolchainClientError::Rejected {
                        method: method.into(),
                        code: error["code"]
                            .as_str()
                            .unwrap_or("ICE3_TOOL_SIDECAR_JOB_FAILED")
                            .into(),
                        message: error["message"]
                            .as_str()
                            .unwrap_or("sidecar request failed")
                            .into(),
                        retryable: error["retryable"].as_bool().unwrap_or(false),
                    });
                }
                serde_json::from_value(value).map_err(|error| {
                    ToolchainClientError::protocol(format!("invalid {method} response: {error}"))
                })
            }
            PendingReply::Protocol(message) => Err(ToolchainClientError::Protocol { message }),
            PendingReply::Closed(message) => Err(self.crashed(message)),
        }
    }

    pub fn stderr_tail(&self) -> Vec<String> {
        self.stderr
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .cloned()
            .collect()
    }

    fn asset_result(
        &self,
        method: &str,
        params: Value,
    ) -> Result<ToolchainAssetResult, ToolchainClientError> {
        let result: ToolchainAssetResult = self.request(method, params, self.request_timeout)?;
        validate_workspace_path(&self.workspace_root, &result.manifest_path)?;
        validate_workspace_path(&self.workspace_root, &result.primary_path)?;
        Ok(result)
    }

    fn write_message(&self, message: Value) -> Result<(), ToolchainClientError> {
        let encoded = serde_json::to_vec(&message)
            .map_err(|error| ToolchainClientError::protocol(error.to_string()))?;
        if encoded.len() > MAX_MESSAGE_BYTES {
            return Err(ToolchainClientError::protocol(
                "sidecar request exceeds 8 MiB",
            ));
        }
        let mut stdin = self
            .stdin
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let writer = stdin
            .as_mut()
            .ok_or_else(|| ToolchainClientError::Crashed {
                message: "sidecar stdin is closed".into(),
            })?;
        writer
            .write_all(&encoded)
            .and_then(|()| writer.write_all(b"\n"))
            .and_then(|()| writer.flush())
            .map_err(|error| ToolchainClientError::Crashed {
                message: format!("sidecar write failed: {error}"),
            })
    }

    fn crashed(&self, message: impl Into<String>) -> ToolchainClientError {
        let mut message = message.into();
        let stderr = self.stderr_tail();
        if !stderr.is_empty() {
            message.push_str(&format!("; stderr: {}", stderr.join(" | ")));
        }
        ToolchainClientError::Crashed { message }
    }
}

impl Drop for ToolchainClient {
    fn drop(&mut self) {
        self.stdin
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        if let Some(reader) = self.stderr_reader.take() {
            let _ = reader.join();
        }
    }
}

fn fail_pending(
    pending: &Arc<Mutex<HashMap<u64, Sender<PendingReply>>>>,
    make_reply: impl Fn(String) -> PendingReply,
    message: &str,
) {
    let mut pending = pending
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for (_, sender) in pending.drain() {
        let _ = sender.send(make_reply(message.into()));
    }
}

fn current_platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        std::env::consts::OS
    }
}

fn current_architecture() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "arm64"
    } else if cfg!(target_arch = "x86_64") {
        "x64"
    } else {
        std::env::consts::ARCH
    }
}
