use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::{
    ImportError, ImportRequest, ImportWarning, NativeImportLimits, SourceImporter,
    SourceImporterRegistry, WorkerLimits, validate_scene_limits,
};

pub const WORKER_PROTOCOL_VERSION: u32 = 1;
const MAX_WORKER_REQUEST_BYTES: usize = 1_048_576;
const MAX_WORKER_RESPONSE_BYTES: usize = 536_870_912;
const MAX_WORKER_STDERR_BYTES: usize = 1_048_576;
const WORKER_POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerRequest {
    pub protocol: u32,
    pub adapter: String,
    pub path: PathBuf,
    pub asset_id: String,
    pub limits: WorkerLimits,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WorkerResponse {
    V1 {
        source_scene: crate::SourceScene,
        warnings: Vec<ImportWarning>,
    },
    V1Failure {
        code: String,
        message: String,
    },
    V2 {
        source_scene: crate::SourceScene,
        warnings: Vec<ImportWarning>,
    },
}

#[derive(Debug, Clone, Copy)]
struct ProcessLimits {
    response_bytes: usize,
    stderr_bytes: usize,
}

impl Default for ProcessLimits {
    fn default() -> Self {
        Self {
            response_bytes: MAX_WORKER_RESPONSE_BYTES,
            stderr_bytes: MAX_WORKER_STDERR_BYTES,
        }
    }
}

pub fn run_isolated_import<A: SourceImporter + ?Sized>(
    adapter: &A,
    request: &ImportRequest,
    limits: NativeImportLimits,
) -> Result<crate::SourceScene, ImportError> {
    run_isolated_import_with_launcher(
        adapter,
        request,
        limits,
        ProcessLimits::default(),
        |request_path| {
            let mut command = Command::new(std::env::current_exe()?);
            command.arg("--internal-import-worker").arg(request_path);
            Ok(command)
        },
    )
}

fn run_isolated_import_with_launcher<A, F>(
    adapter: &A,
    request: &ImportRequest,
    limits: NativeImportLimits,
    process_limits: ProcessLimits,
    launch: F,
) -> Result<crate::SourceScene, ImportError>
where
    A: SourceImporter + ?Sized,
    F: FnOnce(&Path) -> std::io::Result<Command>,
{
    let worker_request = WorkerRequest {
        protocol: WORKER_PROTOCOL_VERSION,
        adapter: adapter.id().into(),
        path: request.path.clone(),
        asset_id: request.asset_id.clone(),
        limits: WorkerLimits::from_limits(limits)?,
    };
    let request_bytes = bincode::serde::encode_to_vec(&worker_request, bincode::config::standard())
        .map_err(|error| protocol_invalid(format!("could not encode request: {error}")))?;
    let temp = tempfile::Builder::new()
        .prefix("ice3-studio-import-")
        .tempdir()?;
    let request_path = temp.path().join("request.bin");
    {
        let mut file = std::fs::File::create(&request_path)?;
        file.write_all(&request_bytes)?;
        file.sync_all()?;
    }
    let response_path = response_path_for_request(&request_path)?;

    let mut command = launch(&request_path)?;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| ImportError::WorkerFailed("worker stderr was unavailable".into()))?;
    let stderr_exceeded = Arc::new(AtomicBool::new(false));
    let stderr_exceeded_by_reader = Arc::clone(&stderr_exceeded);
    let stderr_reader = std::thread::spawn(move || {
        collect_bounded_stderr(
            stderr,
            process_limits.stderr_bytes,
            stderr_exceeded_by_reader,
        )
    });

    let started = Instant::now();
    let outcome = wait_for_worker(
        &mut child,
        started,
        limits.timeout,
        Arc::clone(&stderr_exceeded),
    )?;
    let stderr_bytes = stderr_reader
        .join()
        .map_err(|_| ImportError::WorkerFailed("worker stderr collector panicked".into()))??;

    if outcome.timed_out {
        return Err(ImportError::WorkerTimeout);
    }
    if outcome.stderr_exceeded {
        return Err(ImportError::WorkerOutputTooLarge(format!(
            "stderr exceeded {} bytes (observed at least {stderr_bytes})",
            process_limits.stderr_bytes
        )));
    }
    let response = read_worker_response(&response_path, process_limits.response_bytes)?;
    if let Some(response) = response {
        match decode_worker_response(&response) {
            Ok(mut scene) => {
                if !outcome.status.success() {
                    return Err(ImportError::WorkerFailed(format!(
                        "worker exited with {}",
                        outcome.status
                    )));
                }
                validate_scene_limits(&scene, &limits)?;
                scene.warnings.sort();
                scene.warnings.dedup();
                Ok(scene)
            }
            Err(error @ ImportError::WorkerReported { .. }) => Err(error),
            Err(error) if outcome.status.success() => Err(error),
            Err(_) => Err(ImportError::WorkerFailed(format!(
                "worker exited with {} without a valid failure response",
                outcome.status
            ))),
        }
    } else if outcome.status.success() {
        Err(protocol_invalid("worker did not publish a response".into()))
    } else {
        Err(ImportError::WorkerFailed(format!(
            "worker exited with {}",
            outcome.status
        )))
    }
}

pub fn execute_worker_request(
    request_path: &Path,
    registry: &SourceImporterRegistry,
) -> Result<(), ImportError> {
    match execute_worker_request_inner(request_path, registry) {
        Ok(()) => Ok(()),
        Err(error) => {
            write_worker_failure(request_path, &error)?;
            Err(error)
        }
    }
}

fn execute_worker_request_inner(
    request_path: &Path,
    registry: &SourceImporterRegistry,
) -> Result<(), ImportError> {
    let request_bytes = read_request(request_path)?;
    let (request, consumed): (WorkerRequest, usize) = bincode::serde::decode_from_slice(
        &request_bytes,
        bincode::config::standard().with_limit::<MAX_WORKER_REQUEST_BYTES>(),
    )
    .map_err(|error| protocol_invalid(format!("could not decode request: {error}")))?;
    if consumed != request_bytes.len() {
        return Err(protocol_invalid(
            "request contains trailing protocol bytes".into(),
        ));
    }
    if request.protocol != WORKER_PROTOCOL_VERSION {
        return Err(protocol_invalid(format!(
            "unsupported request protocol {}",
            request.protocol
        )));
    }
    let limits = request.limits.to_limits()?;
    let mut scene = registry.import(&ImportRequest {
        path: request.path,
        asset_id: request.asset_id,
        requested_adapter: Some(request.adapter),
    })?;
    scene.warnings.sort();
    scene.warnings.dedup();
    validate_scene_limits(&scene, &limits)?;

    let response = WorkerResponse::V1 {
        warnings: scene.warnings.clone(),
        source_scene: scene,
    };
    let response_bytes = bincode::serde::encode_to_vec(&response, bincode::config::standard())
        .map_err(|error| protocol_invalid(format!("could not encode response: {error}")))?;
    if response_bytes.len() > MAX_WORKER_RESPONSE_BYTES {
        return Err(ImportError::LimitExceeded(format!(
            "worker response exceeds {MAX_WORKER_RESPONSE_BYTES} bytes"
        )));
    }
    write_worker_response(request_path, &response_bytes)
}

pub fn decode_worker_response(bytes: &[u8]) -> Result<crate::SourceScene, ImportError> {
    let (response, consumed): (WorkerResponse, usize) = bincode::serde::decode_from_slice(
        bytes,
        bincode::config::standard().with_limit::<MAX_WORKER_RESPONSE_BYTES>(),
    )
    .map_err(|error| protocol_invalid(format!("could not decode response: {error}")))?;
    if consumed != bytes.len() {
        return Err(protocol_invalid(
            "response contains trailing protocol bytes".into(),
        ));
    }
    let (mut scene, warnings) = match response {
        WorkerResponse::V1 {
            source_scene,
            warnings,
        } => (source_scene, warnings),
        WorkerResponse::V1Failure { code, message } => {
            validate_worker_error_code(&code)?;
            if message.is_empty() || message.len() > 65_536 {
                return Err(protocol_invalid(
                    "worker failure message has an invalid length".into(),
                ));
            }
            return Err(ImportError::WorkerReported { code, message });
        }
        WorkerResponse::V2 { .. } => {
            return Err(protocol_invalid("unsupported response protocol 2".into()));
        }
    };
    let mut all_warnings: std::collections::BTreeSet<_> = scene.warnings.iter().cloned().collect();
    all_warnings.extend(warnings);
    scene.warnings = all_warnings.into_iter().collect();
    Ok(scene)
}

fn write_worker_failure(request_path: &Path, error: &ImportError) -> Result<(), ImportError> {
    let response = WorkerResponse::V1Failure {
        code: error.code().to_owned(),
        message: error.to_string(),
    };
    let bytes = bincode::serde::encode_to_vec(&response, bincode::config::standard())
        .map_err(|error| protocol_invalid(format!("could not encode failure response: {error}")))?;
    write_worker_response(request_path, &bytes)
}

fn validate_worker_error_code(code: &str) -> Result<(), ImportError> {
    if !code.starts_with("ICE3_STUDIO_IMPORT_")
        || code.len() > 128
        || !code
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(protocol_invalid(
            "worker failure code is not a stable diagnostic".into(),
        ));
    }
    Ok(())
}

struct WorkerOutcome {
    status: std::process::ExitStatus,
    timed_out: bool,
    stderr_exceeded: bool,
}

fn wait_for_worker(
    child: &mut Child,
    started: Instant,
    timeout: Duration,
    stderr_exceeded: Arc<AtomicBool>,
) -> Result<WorkerOutcome, ImportError> {
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(WorkerOutcome {
                status,
                timed_out: false,
                stderr_exceeded: stderr_exceeded.load(Ordering::Acquire),
            });
        }
        if stderr_exceeded.load(Ordering::Acquire) {
            let _ = child.kill();
            let status = child.wait()?;
            return Ok(WorkerOutcome {
                status,
                timed_out: false,
                stderr_exceeded: true,
            });
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let status = child.wait()?;
            return Ok(WorkerOutcome {
                status,
                timed_out: true,
                stderr_exceeded: stderr_exceeded.load(Ordering::Acquire),
            });
        }
        std::thread::sleep(WORKER_POLL_INTERVAL);
    }
}

fn collect_bounded_stderr(
    mut stderr: impl Read,
    limit: usize,
    exceeded: Arc<AtomicBool>,
) -> Result<usize, ImportError> {
    let mut total = 0_usize;
    let mut buffer = [0_u8; 8192];
    loop {
        let read = stderr.read(&mut buffer)?;
        if read == 0 {
            return Ok(total);
        }
        total = total.saturating_add(read);
        if total > limit {
            exceeded.store(true, Ordering::Release);
            return Ok(total);
        }
    }
}

fn read_worker_response(path: &Path, limit: usize) -> Result<Option<Vec<u8>>, ImportError> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(ImportError::Io(error)),
    };
    if !metadata.file_type().is_file() {
        return Err(protocol_invalid(
            "worker response is not a regular file".into(),
        ));
    }
    if metadata.len() > limit as u64 {
        return Err(ImportError::WorkerOutputTooLarge(format!(
            "response exceeded {limit} bytes"
        )));
    }
    let mut file = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    std::io::Read::by_ref(&mut file)
        .take((limit as u64).saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(ImportError::WorkerOutputTooLarge(format!(
            "response exceeded {limit} bytes"
        )));
    }
    Ok(Some(bytes))
}

fn read_request(path: &Path) -> Result<Vec<u8>, ImportError> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() {
        return Err(protocol_invalid(
            "worker request is not a regular file".into(),
        ));
    }
    if metadata.len() > MAX_WORKER_REQUEST_BYTES as u64 {
        return Err(protocol_invalid(format!(
            "worker request exceeds {MAX_WORKER_REQUEST_BYTES} bytes"
        )));
    }
    let mut file = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    std::io::Read::by_ref(&mut file)
        .take((MAX_WORKER_REQUEST_BYTES as u64).saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_WORKER_REQUEST_BYTES {
        return Err(protocol_invalid(format!(
            "worker request exceeds {MAX_WORKER_REQUEST_BYTES} bytes"
        )));
    }
    Ok(bytes)
}

fn write_worker_response(request_path: &Path, bytes: &[u8]) -> Result<(), ImportError> {
    let response_path = response_path_for_request(request_path)?;
    let parent = response_path
        .parent()
        .ok_or_else(|| protocol_invalid("worker response has no parent directory".into()))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(&response_path)
        .map_err(|error| ImportError::Io(error.error))?;
    Ok(())
}

fn response_path_for_request(request_path: &Path) -> Result<PathBuf, ImportError> {
    let file_name = request_path
        .file_name()
        .ok_or_else(|| protocol_invalid("worker request has no file name".into()))?;
    let mut response_name = file_name.to_os_string();
    response_name.push(".response");
    let mut response_path = request_path.to_path_buf();
    response_path.set_file_name(response_name);
    Ok(response_path)
}

fn protocol_invalid(message: String) -> ImportError {
    ImportError::WorkerProtocolInvalid(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        SourceCapabilities, SourceFormat, SourceMesh, SourceNode, SourcePrimitive, SourceScene,
    };

    struct TestImporter;

    impl SourceImporter for TestImporter {
        fn id(&self) -> &'static str {
            "test"
        }

        fn extensions(&self) -> &'static [&'static str] {
            &[".test"]
        }

        fn capabilities(&self) -> SourceCapabilities {
            SourceCapabilities {
                hierarchy: true,
                vertex_colors: false,
                material_colors: false,
                instances: false,
                semantic_metadata: false,
            }
        }

        fn import(&self, _request: &ImportRequest) -> Result<SourceScene, ImportError> {
            unreachable!("the isolation tests launch their own worker process")
        }
    }

    fn fixture_scene() -> SourceScene {
        SourceScene {
            asset_id: "fixture".into(),
            source_format: SourceFormat::Gltf,
            nodes: vec![SourceNode {
                id: "fixture/node/0".into(),
                name: "Fixture".into(),
                parent: None,
                matrix: [
                    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
                ],
                mesh: Some(0),
                material: None,
                extras: serde_json::json!({}),
            }],
            meshes: vec![SourceMesh {
                id: "fixture/mesh/0".into(),
                primitives: vec![SourcePrimitive {
                    positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                    normals: None,
                    colors: None,
                    indices: vec![0, 1, 2],
                    material: None,
                }],
                extras: serde_json::json!({}),
            }],
            materials: Vec::new(),
            warnings: Vec::new(),
            asset_extras: serde_json::json!({}),
        }
    }

    fn import_request() -> ImportRequest {
        ImportRequest {
            path: PathBuf::from("fixture.test"),
            asset_id: "fixture".into(),
            requested_adapter: None,
        }
    }

    fn script_command(script: &'static str) -> impl FnOnce(&Path) -> std::io::Result<Command> {
        move |request_path| {
            let mut command = Command::new("/bin/sh");
            command
                .arg("-c")
                .arg(script)
                .arg("studio-import-test-worker")
                .arg("--internal-import-worker")
                .arg(request_path);
            Ok(command)
        }
    }

    #[test]
    fn ignores_worker_stdout_and_accepts_the_response_file() {
        let response = WorkerResponse::V1 {
            source_scene: fixture_scene(),
            warnings: Vec::new(),
        };
        let encoded =
            bincode::serde::encode_to_vec(&response, bincode::config::standard()).unwrap();
        let expected = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(expected.path(), encoded).unwrap();
        let expected_path = expected.path().to_path_buf();

        let scene = run_isolated_import_with_launcher(
            &TestImporter,
            &import_request(),
            NativeImportLimits {
                timeout: Duration::from_secs(2),
                ..NativeImportLimits::default()
            },
            ProcessLimits::default(),
            move |request_path| {
                let mut command = Command::new("/bin/sh");
                command
                    .env("EXPECTED_RESPONSE", &expected_path)
                    .arg("-c")
                    .arg("cp \"$EXPECTED_RESPONSE\" \"${2}.response\"; printf 'polluted stdout'")
                    .arg("studio-import-test-worker")
                    .arg("--internal-import-worker")
                    .arg(request_path);
                Ok(command)
            },
        )
        .unwrap();

        assert_eq!(scene.asset_id, "fixture");
    }

    #[test]
    fn maps_a_crashed_worker_to_a_stable_error() {
        let error = run_isolated_import_with_launcher(
            &TestImporter,
            &import_request(),
            NativeImportLimits::default(),
            ProcessLimits::default(),
            script_command("exit 7"),
        )
        .unwrap_err();

        assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_WORKER_FAILED");
    }

    #[test]
    fn preserves_a_reported_source_error_from_a_worker() {
        let response = WorkerResponse::V1Failure {
            code: "ICE3_STUDIO_IMPORT_INVALID_SOURCE".into(),
            message: "truncated source".into(),
        };
        let encoded =
            bincode::serde::encode_to_vec(&response, bincode::config::standard()).unwrap();
        let expected = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(expected.path(), encoded).unwrap();
        let expected_path = expected.path().to_path_buf();

        let error = run_isolated_import_with_launcher(
            &TestImporter,
            &import_request(),
            NativeImportLimits::default(),
            ProcessLimits::default(),
            move |request_path| {
                let mut command = Command::new("/bin/sh");
                command
                    .env("EXPECTED_RESPONSE", &expected_path)
                    .arg("-c")
                    .arg("cp \"$EXPECTED_RESPONSE\" \"${2}.response\"; exit 1")
                    .arg("studio-import-test-worker")
                    .arg("--internal-import-worker")
                    .arg(request_path);
                Ok(command)
            },
        )
        .unwrap_err();

        assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_INVALID_SOURCE");
        assert!(error.to_string().contains("truncated source"));
    }

    #[test]
    fn hard_kills_a_worker_after_the_timeout() {
        let marker_dir = tempfile::tempdir().unwrap();
        let marker = marker_dir.path().join("survived");
        let marker_for_worker = marker.clone();
        let error = run_isolated_import_with_launcher(
            &TestImporter,
            &import_request(),
            NativeImportLimits {
                timeout: Duration::from_millis(25),
                ..NativeImportLimits::default()
            },
            ProcessLimits::default(),
            move |request_path| {
                let mut command = Command::new("/bin/sh");
                command
                    .env("TIMEOUT_MARKER", &marker_for_worker)
                    .arg("-c")
                    .arg("sleep 0.2; printf alive > \"$TIMEOUT_MARKER\"")
                    .arg("studio-import-test-worker")
                    .arg("--internal-import-worker")
                    .arg(request_path);
                Ok(command)
            },
        )
        .unwrap_err();

        assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_TIMEOUT");
        std::thread::sleep(Duration::from_millis(250));
        assert!(!marker.exists(), "timed-out worker continued running");
    }

    #[test]
    fn rejects_oversized_worker_stderr() {
        let error = run_isolated_import_with_launcher(
            &TestImporter,
            &import_request(),
            NativeImportLimits {
                timeout: Duration::from_secs(2),
                ..NativeImportLimits::default()
            },
            ProcessLimits {
                response_bytes: 1024,
                stderr_bytes: 4,
            },
            script_command("printf 123456789 >&2; sleep 2"),
        )
        .unwrap_err();

        assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_WORKER_OUTPUT_TOO_LARGE");
    }

    #[test]
    fn rejects_oversized_worker_response() {
        let response = WorkerResponse::V1 {
            source_scene: fixture_scene(),
            warnings: Vec::new(),
        };
        let encoded =
            bincode::serde::encode_to_vec(&response, bincode::config::standard()).unwrap();
        assert!(encoded.len() > 8);
        let expected = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(expected.path(), encoded).unwrap();
        let expected_path = expected.path().to_path_buf();

        let error = run_isolated_import_with_launcher(
            &TestImporter,
            &import_request(),
            NativeImportLimits::default(),
            ProcessLimits {
                response_bytes: 8,
                stderr_bytes: 1024,
            },
            move |request_path| {
                let mut command = Command::new("/bin/sh");
                command
                    .env("EXPECTED_RESPONSE", &expected_path)
                    .arg("-c")
                    .arg("cp \"$EXPECTED_RESPONSE\" \"${2}.response\"")
                    .arg("studio-import-test-worker")
                    .arg("--internal-import-worker")
                    .arg(request_path);
                Ok(command)
            },
        )
        .unwrap_err();

        assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_WORKER_OUTPUT_TOO_LARGE");
    }
}
