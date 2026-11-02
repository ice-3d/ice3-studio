#![cfg(feature = "jt")]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};

const SENTINEL: &[u8] = b"keep-existing";

#[test]
fn missing_converter_is_reported_without_publishing() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("jt.glb");
    std::fs::write(&output, SENTINEL).unwrap();

    let result = run_import(
        &root,
        &output,
        "synthetic-contract.jt",
        &[("ICE3_JT_CONVERTER", None)],
    );

    assert_failure(
        &result,
        &output,
        "ICE3_STUDIO_IMPORT_JT_CONVERTER_UNAVAILABLE",
    );
}

#[test]
fn converter_failure_and_invalid_output_are_stable() {
    for (fixture_name, code) in [
        (
            "failing-converter.sh",
            "ICE3_STUDIO_IMPORT_JT_CONVERT_FAILED",
        ),
        (
            "invalid-converter.sh",
            "ICE3_STUDIO_IMPORT_JT_EXPORT_INVALID",
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("jt.glb");
        std::fs::write(&output, SENTINEL).unwrap();

        let result = run_import(
            &root,
            &output,
            "synthetic-contract.jt",
            &[(
                "ICE3_JT_CONVERTER",
                Some(fixture(fixture_name).to_str().unwrap()),
            )],
        );

        assert_failure(&result, &output, code);
    }
}

#[test]
fn empty_geometry_uses_the_source_import_code() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("jt.glb");
    std::fs::write(&output, SENTINEL).unwrap();

    let result = run_import(
        &root,
        &output,
        "empty.jt",
        &[(
            "ICE3_JT_CONVERTER",
            Some(fixture("converter.mjs").to_str().unwrap()),
        )],
    );

    assert_failure(&result, &output, "ICE3_STUDIO_IMPORT_EMPTY_GEOMETRY");
}

#[cfg(debug_assertions)]
#[cfg(unix)]
#[test]
fn worker_timeout_terminates_converter_descendants() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("jt.glb");
    let marker = root.path().join("orphan");
    std::fs::write(&output, SENTINEL).unwrap();

    let started = Instant::now();
    let result = run_import(
        &root,
        &output,
        "synthetic-contract.jt",
        &[
            (
                "ICE3_JT_CONVERTER",
                Some(fixture("tree-converter.sh").to_str().unwrap()),
            ),
            ("ICE3_STUDIO_IMPORT_WORKER_TIMEOUT_MS", Some("100")),
            ("ICE3_JT_ORPHAN_MARKER", Some(marker.to_str().unwrap())),
        ],
    );

    assert!(
        started.elapsed() < Duration::from_secs(6),
        "timeout cleanup exceeded its wall-time budget: {:?}",
        started.elapsed()
    );
    assert_failure(&result, &output, "ICE3_STUDIO_IMPORT_JT_TIMEOUT");
    std::thread::sleep(Duration::from_millis(2_500));
    assert!(!marker.exists(), "converter descendant survived timeout");
}

#[cfg(unix)]
#[test]
fn oversized_converter_output_is_rejected() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("jt.glb");
    std::fs::write(&output, SENTINEL).unwrap();

    let result = run_import(
        &root,
        &output,
        "synthetic-contract.jt",
        &[(
            "ICE3_JT_CONVERTER",
            Some(fixture("oversize-converter.sh").to_str().unwrap()),
        )],
    );

    assert_failure(&result, &output, "ICE3_STUDIO_IMPORT_LIMIT_EXCEEDED");
}

fn run_import(
    root: &tempfile::TempDir,
    output: &Path,
    source: &str,
    environment: &[(&str, Option<&str>)],
) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_studio-cli"));
    command
        .current_dir(root.path())
        .args([
            "import",
            fixture(source).to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--asset-id",
            "failed-jt",
            "--adapter",
            "jt",
        ])
        .env_remove("ICE3_JT_CONVERTER")
        .env_remove("ICE3_JT_TIMEOUT_MS")
        .env_remove("ICE3_STUDIO_IMPORT_WORKER_TIMEOUT_MS")
        .env_remove("ICE3_JT_ORPHAN_MARKER");
    for (key, value) in environment {
        if let Some(value) = value {
            command.env(key, value);
        } else {
            command.env_remove(key);
        }
    }
    command.output().unwrap()
}

fn assert_failure(result: &Output, output: &Path, code: &str) {
    assert!(
        !result.status.success(),
        "import unexpectedly succeeded: {}",
        String::from_utf8_lossy(&result.stdout)
    );
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains(code), "missing {code} in stderr:\n{stderr}");
    assert_eq!(std::fs::read(output).unwrap(), SENTINEL);
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/source-import/jt")
        .join(name)
}
