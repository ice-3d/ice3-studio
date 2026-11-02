#![cfg(feature = "step")]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use studio_assets::{read_glb, validate_prepared_profile};

const WORKER_TIMEOUT_ENV: &str = "ICE3_STUDIO_IMPORT_WORKER_TIMEOUT_MS";
#[cfg(debug_assertions)]
const MAX_TRIANGLES_ENV: &str = "ICE3_STUDIO_IMPORT_MAX_TRIANGLES";

#[test]
fn imports_step_through_the_internal_worker() {
    let result = run_import(
        &source_fixture("step/synthetic-skid.step"),
        "step",
        "step-fixture",
        false,
    );

    assert!(
        result.output.status.success(),
        "{}",
        String::from_utf8_lossy(&result.output.stderr)
    );
    let source = read_glb(&result.path).unwrap();
    assert_eq!(
        validate_prepared_profile(&source).unwrap().asset_id,
        "step-fixture"
    );
    assert!(!String::from_utf8_lossy(&result.output.stdout).contains("Bridge"));
}

#[test]
fn auto_selects_step_from_the_source_extension() {
    let result = run_import(
        &source_fixture("step/synthetic-skid.step"),
        "auto",
        "auto-step",
        false,
    );

    assert!(
        result.output.status.success(),
        "{}",
        String::from_utf8_lossy(&result.output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&result.output.stdout).unwrap();
    assert_eq!(report["adapter"], "step");
}

#[test]
fn auto_selects_step_from_the_stp_extension() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("synthetic-skid.stp");
    std::fs::copy(source_fixture("step/synthetic-skid.step"), &source).unwrap();
    let output_path = root.path().join("step.glb");

    let output = run_import_at_path(&source, "auto", "auto-stp", &output_path, false)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["adapter"], "step");
}

#[test]
fn truncated_step_does_not_replace_existing_output() {
    let root = tempfile::tempdir().unwrap();
    let output_path = root.path().join("step.glb");
    std::fs::write(&output_path, b"keep").unwrap();

    let result = run_import_at_path(
        &source_fixture("step/truncated.step"),
        "step",
        "step-truncated",
        &output_path,
        true,
    )
    .output()
    .unwrap();

    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("ICE3_STUDIO_IMPORT_INVALID_SOURCE"));
    assert_eq!(std::fs::read(&output_path).unwrap(), b"keep");
}

#[test]
fn non_ap242_step_is_rejected_with_a_stable_code() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("ap214.step");
    let original = std::fs::read_to_string(source_fixture("step/synthetic-skid.step")).unwrap();
    std::fs::write(
        &source,
        original.replace(
            "AP242_MANAGED_MODEL_BASED_3D_ENGINEERING_MIM_LF",
            "AUTOMOTIVE_DESIGN",
        ),
    )
    .unwrap();
    let output_path = root.path().join("step.glb");

    let result = run_import_at_path(&source, "step", "step-ap214", &output_path, false)
        .output()
        .unwrap();

    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr)
            .contains("ICE3_STUDIO_IMPORT_STEP_SCHEMA_UNSUPPORTED")
    );
    assert!(!output_path.exists());
}

#[cfg(debug_assertions)]
#[test]
fn limit_failure_code_crosses_the_worker_boundary() {
    let root = tempfile::tempdir().unwrap();
    let output_path = root.path().join("step.glb");
    let result = run_import_at_path(
        &source_fixture("step/synthetic-skid.step"),
        "step",
        "step-limit",
        &output_path,
        false,
    )
    .env(MAX_TRIANGLES_ENV, "1")
    .output()
    .unwrap();

    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("ICE3_STUDIO_IMPORT_LIMIT_EXCEEDED"));
    assert!(!output_path.exists());
}

#[cfg(debug_assertions)]
#[test]
fn timeout_does_not_replace_existing_output() {
    let root = tempfile::tempdir().unwrap();
    let output_path = root.path().join("step.glb");
    std::fs::write(&output_path, b"keep").unwrap();
    let result = run_import_at_path(
        &source_fixture("step/synthetic-skid.step"),
        "step",
        "step-timeout",
        &output_path,
        true,
    )
    .env(WORKER_TIMEOUT_ENV, "1")
    .output()
    .unwrap();

    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("ICE3_STUDIO_IMPORT_TIMEOUT"));
    assert_eq!(std::fs::read(&output_path).unwrap(), b"keep");
}

fn run_import(fixture: &Path, adapter: &str, asset_id: &str, force: bool) -> ImportRun {
    let root = tempfile::tempdir().unwrap();
    let output_path = root.path().join("step.glb");
    let output = run_import_at_path(fixture, adapter, asset_id, &output_path, force)
        .output()
        .unwrap();
    ImportRun {
        output,
        path: output_path,
        _root: root,
    }
}

fn run_import_at_path(
    fixture: &Path,
    adapter: &str,
    asset_id: &str,
    output_path: &Path,
    force: bool,
) -> Command {
    let mut command = Command::new(studio_binary());
    command.args([
        "import",
        fixture.to_str().unwrap(),
        "--output",
        output_path.to_str().unwrap(),
        "--asset-id",
        asset_id,
        "--adapter",
        adapter,
    ]);
    if force {
        command.arg("--force");
    }
    command
}

fn studio_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_studio-cli"))
}

fn source_fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/source-import")
        .join(name)
}

struct ImportRun {
    output: Output,
    path: PathBuf,
    _root: tempfile::TempDir,
}
