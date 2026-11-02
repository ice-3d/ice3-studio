#![cfg(feature = "jt")]

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use studio_import::jt::{JtConverterConfig, run_jt_converter};

static TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn passes_only_fixed_absolute_paths() {
    let _serial = serial();
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("assembly.jt");
    let output = root.path().join("raw.glb");
    std::fs::write(&input, "ICE3_JT_CONVERTER_CONTRACT_V1\n").unwrap();
    let config = config(fixture("capture-args.sh"), Duration::from_secs(2));

    run_jt_converter(&input, &output, &config).unwrap();

    let args = std::fs::read_to_string(root.path().join("raw.glb.args")).unwrap();
    let expected_input = input.canonicalize().unwrap();
    let expected_output = root.path().canonicalize().unwrap().join("raw.glb");
    assert_eq!(
        args,
        format!(
            "{}\n{}\n",
            expected_input.display(),
            expected_output.display()
        )
    );
}

#[test]
fn reports_unavailable_and_non_zero_converter() {
    let _serial = serial();
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("assembly.jt");
    let output = root.path().join("raw.glb");
    std::fs::write(&input, "").unwrap();

    let missing = config(root.path().join("missing"), Duration::from_secs(1));
    assert_eq!(
        run_jt_converter(&input, &output, &missing)
            .unwrap_err()
            .code(),
        "ICE3_STUDIO_IMPORT_JT_CONVERTER_UNAVAILABLE"
    );

    let failing = config(fixture("failing-converter.sh"), Duration::from_secs(5));
    assert_eq!(
        run_jt_converter(&input, &output, &failing)
            .unwrap_err()
            .code(),
        "ICE3_STUDIO_IMPORT_JT_CONVERT_FAILED"
    );
}

#[test]
fn validates_regular_glb_output() {
    let _serial = serial();
    let root = tempfile::tempdir().unwrap();
    let invalid = root.path().join("invalid.glb");
    std::fs::write(&invalid, b"not-a-glb").unwrap();
    assert_eq!(
        studio_import::jt::validate_jt_converter_output(&invalid)
            .unwrap_err()
            .code(),
        "ICE3_STUDIO_IMPORT_JT_EXPORT_INVALID"
    );

    let valid = root.path().join("valid.glb");
    std::fs::copy(fixture("synthetic-contract.raw.glb"), &valid).unwrap();
    studio_import::jt::validate_jt_converter_output(&valid).unwrap();
}

#[test]
fn rejects_oversized_symlink_and_directory_outputs() {
    let _serial = serial();
    let root = tempfile::tempdir().unwrap();
    let oversized = root.path().join("oversized.glb");
    let file = std::fs::File::create(&oversized).unwrap();
    file.set_len(512 * 1024 * 1024 + 1).unwrap();
    assert_eq!(
        studio_import::jt::validate_jt_converter_output(&oversized)
            .unwrap_err()
            .code(),
        "ICE3_STUDIO_IMPORT_LIMIT_EXCEEDED"
    );

    assert_eq!(
        studio_import::jt::validate_jt_converter_output(root.path())
            .unwrap_err()
            .code(),
        "ICE3_STUDIO_IMPORT_JT_EXPORT_INVALID"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let link = root.path().join("linked.glb");
        symlink(fixture("synthetic-contract.raw.glb"), &link).unwrap();
        assert_eq!(
            studio_import::jt::validate_jt_converter_output(&link)
                .unwrap_err()
                .code(),
            "ICE3_STUDIO_IMPORT_JT_EXPORT_INVALID"
        );
    }
}

#[test]
fn drains_noisy_stdout_and_stderr_without_deadlock() {
    let _serial = serial();
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("assembly.jt");
    let output = root.path().join("raw.glb");
    std::fs::write(&input, "ICE3_JT_CONVERTER_CONTRACT_V1\n").unwrap();
    let config = config(fixture("noisy-converter.sh"), Duration::from_secs(2));
    let started = Instant::now();

    run_jt_converter(&input, &output, &config).unwrap();

    assert!(started.elapsed() < Duration::from_secs(2));
    studio_import::jt::validate_jt_converter_output(&output).unwrap();
}

#[cfg(unix)]
#[test]
fn timeout_terminates_descendants() {
    let _serial = serial();
    let root = tempfile::tempdir().unwrap();
    let marker = root.path().join("orphan");
    let input = root.path().join("assembly.jt");
    let output = root.path().join("raw.glb");
    std::fs::write(&input, "ICE3_JT_CONVERTER_CONTRACT_V1\n").unwrap();
    let config = config(fixture("tree-converter.sh"), Duration::from_millis(50))
        .with_environment([("ICE3_JT_ORPHAN_MARKER", marker.as_os_str())]);

    let result = run_jt_converter(&input, &output, &config);

    assert_eq!(result.unwrap_err().code(), "ICE3_STUDIO_IMPORT_JT_TIMEOUT");
    std::thread::sleep(Duration::from_millis(2_500));
    assert!(!marker.exists());
}

#[cfg(unix)]
#[test]
fn successful_parent_cannot_leave_background_descendants() {
    let _serial = serial();
    let root = tempfile::tempdir().unwrap();
    let marker = root.path().join("orphan");
    let input = root.path().join("assembly.jt");
    let output = root.path().join("raw.glb");
    std::fs::write(&input, "ICE3_JT_CONVERTER_CONTRACT_V1\n").unwrap();
    let config = config(
        fixture("exit-with-child-converter.sh"),
        Duration::from_secs(2),
    )
    .with_environment([("ICE3_JT_ORPHAN_MARKER", marker.as_os_str())]);

    let result = run_jt_converter(&input, &output, &config);

    result.unwrap();
    std::thread::sleep(Duration::from_millis(2_500));
    assert!(!marker.exists());
}

#[cfg(unix)]
#[test]
fn converter_group_rendezvous_is_removed_before_return() {
    let _serial = serial();
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("assembly.jt");
    let output = root.path().join("raw.glb");
    let rendezvous = root.path().join("converter.pgid");
    std::fs::write(&input, "ICE3_JT_CONVERTER_CONTRACT_V1\n").unwrap();
    let config =
        config(fixture("converter.mjs"), Duration::from_secs(2)).with_rendezvous_path(&rendezvous);

    let result = run_jt_converter(&input, &output, &config);

    result.unwrap();
    assert!(
        !rendezvous.exists(),
        "converter PGID rendezvous survived through group reaping"
    );
}

#[test]
fn supervisor_does_not_execute_converter_before_start() {
    let _serial = serial();
    let root = tempfile::tempdir().unwrap();
    let marker = root.path().join("converter-started");
    let output = root.path().join("raw.glb");
    let input = root.path().join("assembly.jt");
    std::fs::write(&input, "ICE3_JT_CONVERTER_CONTRACT_V1\n").unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_studio-import-worker"))
        .arg("--internal-jt-converter-supervisor")
        .arg("--converter")
        .arg(fixture("start-marker-converter.sh"))
        .arg("--input")
        .arg(&input)
        .arg("--output")
        .arg(&output)
        .env("ICE3_JT_START_MARKER", &marker)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    std::thread::sleep(Duration::from_millis(150));
    assert!(
        !marker.exists(),
        "converter executed before supervisor start handshake"
    );
    drop(child.stdin.take());
    assert!(!child.wait().unwrap().success());
    assert!(
        !marker.exists(),
        "converter executed after a failed handshake"
    );
}

fn config(binary: impl Into<PathBuf>, timeout: Duration) -> JtConverterConfig {
    JtConverterConfig::new(binary, timeout)
        .with_supervisor(PathBuf::from(env!("CARGO_BIN_EXE_studio-import-worker")))
}

fn serial() -> MutexGuard<'static, ()> {
    TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/source-import/jt")
        .join(name)
}
