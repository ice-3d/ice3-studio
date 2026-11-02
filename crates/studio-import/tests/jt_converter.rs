#![cfg(feature = "jt")]

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use studio_import::jt::{JtConverterConfig, run_jt_converter};

static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn passes_only_fixed_absolute_paths() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("assembly.jt");
    let output = root.path().join("raw.glb");
    std::fs::write(&input, "ICE3_JT_CONVERTER_CONTRACT_V1\n").unwrap();
    let config = JtConverterConfig::new(fixture("capture-args.sh"), Duration::from_secs(2));

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
fn reads_fixed_environment_configuration() {
    let _guard = ENV_LOCK.lock().unwrap();
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("assembly.jt");
    let output = root.path().join("raw.glb");
    std::fs::write(&input, "ICE3_JT_CONVERTER_CONTRACT_V1\n").unwrap();
    unsafe {
        std::env::set_var("ICE3_JT_CONVERTER", fixture("capture-args.sh"));
        std::env::set_var("ICE3_JT_TIMEOUT_MS", "2000");
    }

    let result =
        JtConverterConfig::from_env().and_then(|config| run_jt_converter(&input, &output, &config));

    unsafe {
        std::env::remove_var("ICE3_JT_CONVERTER");
        std::env::remove_var("ICE3_JT_TIMEOUT_MS");
    }
    result.unwrap();
}

#[test]
fn rejects_invalid_environment_configuration() {
    let _guard = ENV_LOCK.lock().unwrap();
    unsafe {
        std::env::remove_var("ICE3_JT_CONVERTER");
        std::env::set_var("ICE3_JT_TIMEOUT_MS", "0");
    }
    assert_eq!(
        JtConverterConfig::from_env().unwrap_err().code(),
        "ICE3_STUDIO_IMPORT_JT_CONVERTER_UNAVAILABLE"
    );

    unsafe {
        std::env::set_var("ICE3_JT_CONVERTER", fixture("capture-args.sh"));
        std::env::set_var("ICE3_JT_TIMEOUT_MS", "not-a-number");
    }
    assert_eq!(
        JtConverterConfig::from_env().unwrap_err().code(),
        "ICE3_STUDIO_IMPORT_JT_CONVERT_FAILED"
    );

    unsafe {
        std::env::remove_var("ICE3_JT_CONVERTER");
        std::env::remove_var("ICE3_JT_TIMEOUT_MS");
    }
}

#[test]
fn reports_unavailable_and_non_zero_converter() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("assembly.jt");
    let output = root.path().join("raw.glb");
    std::fs::write(&input, "").unwrap();

    let missing = JtConverterConfig::new(root.path().join("missing"), Duration::from_secs(1));
    assert_eq!(
        run_jt_converter(&input, &output, &missing)
            .unwrap_err()
            .code(),
        "ICE3_STUDIO_IMPORT_JT_CONVERTER_UNAVAILABLE"
    );

    let failing = JtConverterConfig::new(fixture("failing-converter.sh"), Duration::from_secs(1));
    assert_eq!(
        run_jt_converter(&input, &output, &failing)
            .unwrap_err()
            .code(),
        "ICE3_STUDIO_IMPORT_JT_CONVERT_FAILED"
    );
}

#[test]
fn validates_regular_glb_output() {
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
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("assembly.jt");
    let output = root.path().join("raw.glb");
    std::fs::write(&input, "ICE3_JT_CONVERTER_CONTRACT_V1\n").unwrap();
    let config = JtConverterConfig::new(fixture("noisy-converter.sh"), Duration::from_secs(2));
    let started = Instant::now();

    run_jt_converter(&input, &output, &config).unwrap();

    assert!(started.elapsed() < Duration::from_secs(2));
    studio_import::jt::validate_jt_converter_output(&output).unwrap();
}

#[cfg(unix)]
#[test]
fn timeout_terminates_descendants() {
    let _guard = ENV_LOCK.lock().unwrap();
    let root = tempfile::tempdir().unwrap();
    let marker = root.path().join("orphan");
    let input = root.path().join("assembly.jt");
    let output = root.path().join("raw.glb");
    std::fs::write(&input, "ICE3_JT_CONVERTER_CONTRACT_V1\n").unwrap();
    unsafe {
        std::env::set_var("ICE3_JT_ORPHAN_MARKER", &marker);
    }
    let config = JtConverterConfig::new(fixture("tree-converter.sh"), Duration::from_millis(50));

    let result = run_jt_converter(&input, &output, &config);

    unsafe {
        std::env::remove_var("ICE3_JT_ORPHAN_MARKER");
    }
    assert_eq!(result.unwrap_err().code(), "ICE3_STUDIO_IMPORT_JT_TIMEOUT");
    std::thread::sleep(Duration::from_millis(2_500));
    assert!(!marker.exists());
}

#[cfg(unix)]
#[test]
fn successful_parent_cannot_leave_background_descendants() {
    let _guard = ENV_LOCK.lock().unwrap();
    let root = tempfile::tempdir().unwrap();
    let marker = root.path().join("orphan");
    let input = root.path().join("assembly.jt");
    let output = root.path().join("raw.glb");
    std::fs::write(&input, "ICE3_JT_CONVERTER_CONTRACT_V1\n").unwrap();
    unsafe {
        std::env::set_var("ICE3_JT_ORPHAN_MARKER", &marker);
    }
    let config = JtConverterConfig::new(
        fixture("exit-with-child-converter.sh"),
        Duration::from_secs(2),
    );

    let result = run_jt_converter(&input, &output, &config);

    unsafe {
        std::env::remove_var("ICE3_JT_ORPHAN_MARKER");
    }
    result.unwrap();
    std::thread::sleep(Duration::from_millis(2_500));
    assert!(!marker.exists());
}

#[cfg(unix)]
#[test]
fn escaped_descendant_holding_pipes_cannot_extend_the_converter_timeout() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("assembly.jt");
    let output = root.path().join("raw.glb");
    std::fs::write(&input, "ICE3_JT_CONVERTER_CONTRACT_V1\n").unwrap();
    let config = JtConverterConfig::new(
        fixture("escape-with-child-converter.py"),
        Duration::from_secs(1),
    );
    let started = Instant::now();

    run_jt_converter(&input, &output, &config).unwrap();

    assert!(
        started.elapsed() < Duration::from_millis(1_500),
        "escaped pipe holder extended runtime to {:?}",
        started.elapsed()
    );
    studio_import::jt::validate_jt_converter_output(&output).unwrap();
}

#[cfg(unix)]
#[test]
fn converter_group_rendezvous_is_removed_before_return() {
    let _guard = ENV_LOCK.lock().unwrap();
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("assembly.jt");
    let output = root.path().join("raw.glb");
    let rendezvous = root.path().join("converter.pgid");
    std::fs::write(&input, "ICE3_JT_CONVERTER_CONTRACT_V1\n").unwrap();
    unsafe {
        std::env::set_var("ICE3_STUDIO_JT_CONVERTER_PGID_FILE", &rendezvous);
    }
    let config = JtConverterConfig::new(fixture("converter.mjs"), Duration::from_secs(2));

    let result = run_jt_converter(&input, &output, &config);

    unsafe {
        std::env::remove_var("ICE3_STUDIO_JT_CONVERTER_PGID_FILE");
    }
    result.unwrap();
    assert!(
        !rendezvous.exists(),
        "converter PGID rendezvous survived through group reaping"
    );
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/source-import/jt")
        .join(name)
}
