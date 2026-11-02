use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use command_group::CommandGroup;

use crate::ImportError;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);
const TERMINATE_GRACE: Duration = Duration::from_secs(1);
const MAX_OUTPUT_BYTES: u64 = 512 * 1024 * 1024;
const MAX_CAPTURED_STREAM_BYTES: usize = 64 * 1024;
const WAIT_POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JtConverterConfig {
    binary: PathBuf,
    timeout: Duration,
}

impl JtConverterConfig {
    pub fn new(binary: impl Into<PathBuf>, timeout: Duration) -> Self {
        Self {
            binary: binary.into(),
            timeout,
        }
    }

    pub fn from_env() -> Result<Self, ImportError> {
        let binary = std::env::var("ICE3_JT_CONVERTER")
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                diagnostic(
                    "ICE3_STUDIO_IMPORT_JT_CONVERTER_UNAVAILABLE",
                    "JT converter is not configured",
                )
            })?;
        let binary = validate_converter_binary(PathBuf::from(binary))?;
        let timeout = match std::env::var("ICE3_JT_TIMEOUT_MS") {
            Ok(value) => {
                let value = value.trim();
                if value.is_empty()
                    || !value.bytes().all(|byte| byte.is_ascii_digit())
                    || value
                        .parse::<u64>()
                        .ok()
                        .filter(|value| *value > 0)
                        .is_none()
                {
                    return Err(converter_failed());
                }
                Duration::from_millis(value.parse::<u64>().map_err(|_| converter_failed())?)
            }
            Err(std::env::VarError::NotPresent) => DEFAULT_TIMEOUT,
            Err(std::env::VarError::NotUnicode(_)) => return Err(converter_failed()),
        };
        Ok(Self { binary, timeout })
    }

    pub const fn timeout(&self) -> Duration {
        self.timeout
    }
}

pub fn run_jt_converter(
    input: &Path,
    output: &Path,
    config: &JtConverterConfig,
) -> Result<(), ImportError> {
    let converter = validate_converter_binary(config.binary.clone())?;
    let input = input
        .canonicalize()
        .map_err(|_| ImportError::InvalidSource("JT converter input is unavailable".into()))?;
    let output = absolute_output_path(output)?;
    remove_stale_output(&output)?;

    let mut command = Command::new(converter);
    command
        .arg("--input")
        .arg(&input)
        .arg("--output")
        .arg(&output)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.group_spawn().map_err(|_| converter_failed())?;
    let stdout = child.inner().stdout.take().ok_or_else(converter_failed)?;
    let stderr = child.inner().stderr.take().ok_or_else(converter_failed)?;
    let stdout_reader = thread::spawn(move || collect_bounded(stdout));
    let stderr_reader = thread::spawn(move || collect_bounded(stderr));

    let outcome = wait_for_converter(&mut child, config.timeout);
    let _stdout = stdout_reader.join().map_err(|_| converter_failed())??;
    let _stderr = stderr_reader.join().map_err(|_| converter_failed())??;

    match outcome? {
        ConverterOutcome::Exited(status) if status.success() => {
            validate_jt_converter_output(&output).inspect_err(|_| {
                let _ = fs::remove_file(&output);
            })
        }
        ConverterOutcome::Exited(_) => {
            let _ = fs::remove_file(&output);
            Err(converter_failed())
        }
        ConverterOutcome::TimedOut => {
            let _ = fs::remove_file(&output);
            Err(diagnostic(
                "ICE3_STUDIO_IMPORT_JT_TIMEOUT",
                "JT converter exceeded its timeout",
            ))
        }
    }
}

pub fn validate_jt_converter_output(output: &Path) -> Result<(), ImportError> {
    let metadata = fs::symlink_metadata(output).map_err(|_| export_invalid())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(export_invalid());
    }
    if metadata.len() > MAX_OUTPUT_BYTES {
        return Err(ImportError::LimitExceeded(format!(
            "JT converter output exceeds {MAX_OUTPUT_BYTES} bytes"
        )));
    }
    if metadata.len() < 12 {
        return Err(export_invalid());
    }

    let mut header = [0_u8; 12];
    fs::File::open(output)
        .and_then(|mut file| file.read_exact(&mut header))
        .map_err(|_| export_invalid())?;
    if &header[..4] != b"glTF"
        || u32::from_le_bytes(header[4..8].try_into().expect("four bytes")) != 2
        || u64::from(u32::from_le_bytes(
            header[8..12].try_into().expect("four bytes"),
        )) != metadata.len()
    {
        return Err(export_invalid());
    }
    Ok(())
}

enum ConverterOutcome {
    Exited(std::process::ExitStatus),
    TimedOut,
}

fn wait_for_converter(
    child: &mut command_group::GroupChild,
    timeout: Duration,
) -> Result<ConverterOutcome, ImportError> {
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|_| converter_failed())? {
            // A converter that exits successfully must not be able to leave
            // background descendants holding pipes or mutating temporary output.
            let _ = child.kill();
            return Ok(ConverterOutcome::Exited(status));
        }
        if started.elapsed() >= timeout {
            terminate_converter(child)?;
            return Ok(ConverterOutcome::TimedOut);
        }
        thread::sleep(WAIT_POLL_INTERVAL);
    }
}

fn terminate_converter(child: &mut command_group::GroupChild) -> Result<(), ImportError> {
    #[cfg(unix)]
    {
        use command_group::{Signal, UnixChildExt};
        let _ = child.signal(Signal::SIGTERM);
    }
    #[cfg(windows)]
    {
        child.kill().map_err(|_| converter_failed())?;
        return Ok(());
    }

    #[cfg(unix)]
    {
        let grace_deadline = Instant::now() + TERMINATE_GRACE;
        while Instant::now() < grace_deadline {
            if child.try_wait().map_err(|_| converter_failed())?.is_some() {
                return Ok(());
            }
            thread::sleep(WAIT_POLL_INTERVAL);
        }
        let _ = child.kill();
        let _ = child.wait();
    }
    Ok(())
}

fn collect_bounded(mut reader: impl Read) -> Result<Vec<u8>, ImportError> {
    let mut retained = Vec::new();
    let mut chunk = [0_u8; 8 * 1024];
    loop {
        let read = reader.read(&mut chunk).map_err(|_| converter_failed())?;
        if read == 0 {
            return Ok(retained);
        }
        retained.extend_from_slice(&chunk[..read]);
        if retained.len() > MAX_CAPTURED_STREAM_BYTES {
            retained.drain(..retained.len() - MAX_CAPTURED_STREAM_BYTES);
        }
    }
}

fn validate_converter_binary(binary: PathBuf) -> Result<PathBuf, ImportError> {
    let metadata = fs::metadata(&binary).map_err(|_| converter_unavailable())?;
    if !metadata.is_file() {
        return Err(converter_unavailable());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err(converter_unavailable());
        }
    }
    binary.canonicalize().map_err(|_| converter_unavailable())
}

fn absolute_output_path(output: &Path) -> Result<PathBuf, ImportError> {
    let file_name = output.file_name().ok_or_else(export_invalid)?;
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let parent = parent.canonicalize().map_err(|_| export_invalid())?;
    Ok(parent.join(file_name))
}

fn remove_stale_output(output: &Path) -> Result<(), ImportError> {
    match fs::remove_file(output) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(export_invalid()),
    }
}

fn converter_unavailable() -> ImportError {
    diagnostic(
        "ICE3_STUDIO_IMPORT_JT_CONVERTER_UNAVAILABLE",
        "JT converter is unavailable",
    )
}

fn converter_failed() -> ImportError {
    diagnostic(
        "ICE3_STUDIO_IMPORT_JT_CONVERT_FAILED",
        "JT converter failed",
    )
}

fn export_invalid() -> ImportError {
    diagnostic(
        "ICE3_STUDIO_IMPORT_JT_EXPORT_INVALID",
        "JT converter did not produce a valid GLB",
    )
}

fn diagnostic(code: &str, message: &str) -> ImportError {
    ImportError::SourceDiagnostic {
        code: code.into(),
        message: message.into(),
    }
}
