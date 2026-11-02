use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use studio_assets::{read_glb, validate_prepared_profile};

const WORKER_TIMEOUT_ENV: &str = "ICE3_STUDIO_IMPORT_WORKER_TIMEOUT_MS";

#[test]
fn imports_fbx_and_usd_through_the_internal_worker() {
    for (fixture, adapter, asset_id) in [
        ("fbx/synthetic-binary.fbx", "fbx", "fbx-fixture"),
        ("usd/synthetic-plant.usdc", "usd", "usd-fixture"),
    ] {
        let result = run_import(fixture, adapter, asset_id, false);
        assert!(
            result.output.status.success(),
            "{}",
            String::from_utf8_lossy(&result.output.stderr)
        );
        let source = read_glb(&result.path).unwrap();
        assert_eq!(
            validate_prepared_profile(&source).unwrap().asset_id,
            asset_id
        );
        assert!(!String::from_utf8_lossy(&result.output.stdout).contains("Bridge"));

        let validation = Command::new(studio_binary())
            .args(["validate", result.path.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(
            validation.status.success(),
            "{}",
            String::from_utf8_lossy(&validation.stderr)
        );
    }
}

#[test]
fn auto_selects_fbx_and_usd_from_the_source_extension() {
    for (fixture, expected_adapter) in [
        ("fbx/synthetic-ascii.fbx", "fbx"),
        ("usd/synthetic-plant.usda", "usd"),
    ] {
        let result = run_import(fixture, "auto", "auto-fixture", false);
        assert!(
            result.output.status.success(),
            "{}",
            String::from_utf8_lossy(&result.output.stderr)
        );
        let report: serde_json::Value = serde_json::from_slice(&result.output.stdout).unwrap();
        assert_eq!(report["adapter"], expected_adapter);
    }
}

#[test]
fn timeout_does_not_replace_existing_output() {
    let root = tempfile::tempdir().unwrap();
    let output_path = root.path().join("plant.glb");
    std::fs::write(&output_path, "keep").unwrap();

    let result = run_import_at_path(
        &toolchain_fixture("usd/synthetic-plant.usda"),
        "usd",
        "timeout-fixture",
        &output_path,
        true,
    )
    .env(WORKER_TIMEOUT_ENV, "1")
    .output()
    .unwrap();

    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("ICE3_STUDIO_IMPORT_TIMEOUT"));
    assert_eq!(std::fs::read_to_string(output_path).unwrap(), "keep");
}

#[test]
fn worker_failure_does_not_replace_existing_output() {
    let root = tempfile::tempdir().unwrap();
    let output_path = root.path().join("plant.glb");
    std::fs::write(&output_path, "keep").unwrap();

    let result = run_import_at_path(
        &toolchain_fixture("usd/truncated.usda"),
        "usd",
        "failure-fixture",
        &output_path,
        true,
    )
    .output()
    .unwrap();

    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("ICE3_STUDIO_IMPORT_WORKER_FAILED"));
    assert_eq!(std::fs::read_to_string(output_path).unwrap(), "keep");
}

fn run_import(fixture: &str, adapter: &str, asset_id: &str, force: bool) -> ImportRun {
    let root = tempfile::tempdir().unwrap();
    let output_path = root.path().join("plant.glb");
    let output = run_import_at_path(
        &toolchain_fixture(fixture),
        adapter,
        asset_id,
        &output_path,
        force,
    )
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

fn toolchain_fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../ice3-toolchain/tests/fixtures")
        .join(name)
}

struct ImportRun {
    output: Output,
    path: PathBuf,
    _root: tempfile::TempDir,
}
