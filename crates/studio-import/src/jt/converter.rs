use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use command_group::CommandGroup;

use crate::ImportError;
use crate::jt::JT_CONVERTER_PGID_ENV as PGID_ENV;

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
        let timeout = jt_timeout_from_environment()?;
        Ok(Self { binary, timeout })
    }

    pub const fn timeout(&self) -> Duration {
        self.timeout
    }
}

pub fn jt_timeout_from_environment() -> Result<Duration, ImportError> {
    match std::env::var("ICE3_JT_TIMEOUT_MS") {
        Ok(value) => {
            let value = value.trim();
            let millis = value
                .bytes()
                .all(|byte| byte.is_ascii_digit())
                .then(|| value.parse::<u64>().ok())
                .flatten()
                .filter(|value| *value > 0)
                .ok_or_else(converter_failed)?;
            Ok(Duration::from_millis(millis))
        }
        Err(std::env::VarError::NotPresent) => Ok(DEFAULT_TIMEOUT),
        Err(std::env::VarError::NotUnicode(_)) => Err(converter_failed()),
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
    let mut group = command.group();
    #[cfg(windows)]
    group.kill_on_drop(true);
    let mut child = group.spawn().map_err(|_| converter_failed())?;
    let mut rendezvous = match record_converter_group(child.id()) {
        Ok(rendezvous) => rendezvous,
        Err(error) => {
            let _ = terminate_converter(&mut child, &mut None);
            return Err(error);
        }
    };
    let stdout = match child.inner().stdout.take() {
        Some(stdout) => stdout,
        None => {
            terminate_converter(&mut child, &mut rendezvous)?;
            return Err(converter_failed());
        }
    };
    let stderr = match child.inner().stderr.take() {
        Some(stderr) => stderr,
        None => {
            terminate_converter(&mut child, &mut rendezvous)?;
            return Err(converter_failed());
        }
    };
    let stdout = collect_stream(stdout);
    let stderr = collect_stream(stderr);

    let outcome = match wait_for_converter(&mut child, config.timeout, &mut rendezvous) {
        Ok(outcome) => outcome,
        Err(error) => {
            let cleanup = terminate_converter(&mut child, &mut rendezvous);
            let stream = check_stream_errors(&stdout, &stderr);
            let _ = fs::remove_file(&output);
            cleanup?;
            stream?;
            return Err(error);
        }
    };
    check_stream_errors(&stdout, &stderr).inspect_err(|_| {
        let _ = fs::remove_file(&output);
    })?;

    match outcome {
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

#[cfg(unix)]
fn wait_for_converter(
    child: &mut command_group::GroupChild,
    timeout: Duration,
    rendezvous: &mut Option<ConverterGroupRendezvous>,
) -> Result<ConverterOutcome, ImportError> {
    let started = Instant::now();
    loop {
        if leader_has_exited(child)? {
            // Keep the exited leader unreaped so its PID/PGID cannot be
            // reused, then kill any descendants that remain in its group.
            kill_group_if_present(child)?;
            close_rendezvous(rendezvous)?;
            return child
                .wait()
                .map(ConverterOutcome::Exited)
                .map_err(|_| converter_failed());
        }
        if started.elapsed() >= timeout {
            terminate_converter(child, rendezvous)?;
            return Ok(ConverterOutcome::TimedOut);
        }
        thread::sleep(WAIT_POLL_INTERVAL);
    }
}

#[cfg(windows)]
fn wait_for_converter(
    child: &mut command_group::GroupChild,
    timeout: Duration,
    rendezvous: &mut Option<ConverterGroupRendezvous>,
) -> Result<ConverterOutcome, ImportError> {
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|_| converter_failed())? {
            return Ok(ConverterOutcome::Exited(status));
        }
        if started.elapsed() >= timeout {
            terminate_converter(child, rendezvous)?;
            return Ok(ConverterOutcome::TimedOut);
        }
        thread::sleep(WAIT_POLL_INTERVAL);
    }
}

#[cfg(unix)]
fn leader_has_exited(child: &mut command_group::GroupChild) -> Result<bool, ImportError> {
    let mut info = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
    let result = unsafe {
        libc::waitid(
            libc::P_PID,
            child.id() as libc::id_t,
            info.as_mut_ptr(),
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    };
    if result != 0 {
        return Err(converter_failed());
    }
    let info = unsafe { info.assume_init() };
    Ok(unsafe { info.si_pid() } != 0)
}

#[cfg(unix)]
fn kill_group_if_present(child: &mut command_group::GroupChild) -> Result<(), ImportError> {
    match child.kill() {
        Ok(()) => Ok(()),
        Err(error) if matches!(error.raw_os_error(), Some(libc::EPERM) | Some(libc::ESRCH)) => {
            // macOS reports EPERM for a group containing only the unreaped
            // leader. Since the leader remains unreaped, the PGID cannot have
            // been reused; there are no live descendants left to terminate.
            Ok(())
        }
        Err(_) => Err(converter_failed()),
    }
}

fn terminate_converter(
    child: &mut command_group::GroupChild,
    rendezvous: &mut Option<ConverterGroupRendezvous>,
) -> Result<(), ImportError> {
    #[cfg(unix)]
    {
        use command_group::{Signal, UnixChildExt};
        if child.signal(Signal::SIGTERM).is_err() {
            if leader_has_exited(child)? {
                kill_group_if_present(child)?;
                close_rendezvous(rendezvous)?;
                child.wait().map_err(|_| converter_failed())?;
                return Ok(());
            }
            return Err(converter_failed());
        }
    }
    #[cfg(windows)]
    {
        child.kill().map_err(|_| converter_failed())?;
        close_rendezvous(rendezvous)?;
        return Ok(());
    }

    #[cfg(unix)]
    {
        let grace_deadline = Instant::now() + TERMINATE_GRACE;
        while Instant::now() < grace_deadline {
            if leader_has_exited(child)? {
                kill_group_if_present(child)?;
                close_rendezvous(rendezvous)?;
                child.wait().map_err(|_| converter_failed())?;
                return Ok(());
            }
            thread::sleep(WAIT_POLL_INTERVAL);
        }
        kill_group_if_present(child)?;
        close_rendezvous(rendezvous)?;
        child.wait().map_err(|_| converter_failed())?;
    }
    Ok(())
}

fn collect_stream(
    mut reader: impl Read + Send + 'static,
) -> Receiver<Result<Vec<u8>, ImportError>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(collect_bounded(&mut reader));
    });
    receiver
}

fn check_stream_errors(
    stdout: &Receiver<Result<Vec<u8>, ImportError>>,
    stderr: &Receiver<Result<Vec<u8>, ImportError>>,
) -> Result<(), ImportError> {
    for stream in [stdout, stderr] {
        match stream.try_recv() {
            Ok(Ok(_)) | Err(TryRecvError::Empty) => {}
            Ok(Err(error)) => return Err(error),
            Err(TryRecvError::Disconnected) => return Err(converter_failed()),
        }
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

fn close_rendezvous(rendezvous: &mut Option<ConverterGroupRendezvous>) -> Result<(), ImportError> {
    if let Some(value) = rendezvous {
        value.mark_closed()?;
    }
    Ok(())
}

fn record_converter_group(pgid: u32) -> Result<Option<ConverterGroupRendezvous>, ImportError> {
    let Some(path) = std::env::var_os(PGID_ENV).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let path = PathBuf::from(path);
    let parent = path.parent().ok_or_else(converter_failed)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|_| converter_failed())?;
    temporary
        .write_all(format!("active {pgid}\n").as_bytes())
        .map_err(|_| converter_failed())?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|_| converter_failed())?;
    temporary
        .persist(&path)
        .map_err(|error| ImportError::Io(error.error))?;
    Ok(Some(ConverterGroupRendezvous { path }))
}

struct ConverterGroupRendezvous {
    path: PathBuf,
}

impl ConverterGroupRendezvous {
    fn mark_closed(&self) -> Result<(), ImportError> {
        #[cfg(unix)]
        {
            let mut file = match OpenOptions::new().read(true).write(true).open(&self.path) {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(error) => return Err(ImportError::Io(error)),
            };
            lock_rendezvous(&file)?;
            let result = (|| {
                file.set_len(0)?;
                file.seek(SeekFrom::Start(0))?;
                file.write_all(b"closed\n")?;
                file.sync_all()
            })()
            .map_err(ImportError::Io);
            unlock_rendezvous(&file)?;
            result
        }
        #[cfg(not(unix))]
        {
            Ok(())
        }
    }
}

impl Drop for ConverterGroupRendezvous {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(unix)]
fn lock_rendezvous(file: &fs::File) -> Result<(), ImportError> {
    use std::os::fd::AsRawFd;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } == 0 {
        Ok(())
    } else {
        Err(ImportError::Io(std::io::Error::last_os_error()))
    }
}

#[cfg(unix)]
fn unlock_rendezvous(file: &fs::File) -> Result<(), ImportError> {
    use std::os::fd::AsRawFd;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) } == 0 {
        Ok(())
    } else {
        Err(ImportError::Io(std::io::Error::last_os_error()))
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
