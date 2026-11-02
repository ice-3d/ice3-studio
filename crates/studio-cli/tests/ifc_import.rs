#![cfg(feature = "ifc")]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use studio_assets::{read_glb, validate_prepared_profile};

const WORKER_TIMEOUT_ENV: &str = "ICE3_STUDIO_IMPORT_WORKER_TIMEOUT_MS";
#[cfg(debug_assertions)]
const MAX_TRIANGLES_ENV: &str = "ICE3_STUDIO_IMPORT_MAX_TRIANGLES";

#[test]
fn imports_ifc_through_the_internal_worker() {
    let result = run_import(
        &toolchain_fixture("ifc/synthetic-plant.ifc"),
        "ifc",
        "ifc-fixture",
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
        "ifc-fixture"
    );
    assert!(!String::from_utf8_lossy(&result.output.stdout).contains("Bridge"));
}

#[test]
fn auto_selects_ifc_from_the_extension() {
    let result = run_import(
        &toolchain_fixture("ifc/synthetic-plant.ifc"),
        "auto",
        "auto-ifc",
        false,
    );

    assert!(
        result.output.status.success(),
        "{}",
        String::from_utf8_lossy(&result.output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&result.output.stdout).unwrap();
    assert_eq!(report["adapter"], "ifc");
}

#[test]
fn unsupported_ifc_schema_does_not_replace_output() {
    assert_failure_preserves_output(
        "ifc/ifc4-legacy.ifc",
        "ICE3_STUDIO_IMPORT_IFC_SCHEMA_UNSUPPORTED",
    );
}

#[test]
fn unsupported_ifc_units_do_not_replace_output() {
    for fixture in ["ifc/unsupported-unit.ifc", "ifc/conflicting-units.ifc"] {
        assert_failure_preserves_output(fixture, "ICE3_STUDIO_IMPORT_IFC_UNIT_UNSUPPORTED");
    }
}

#[test]
fn empty_and_truncated_ifc_do_not_publish_output() {
    for (fixture, expected_code) in [
        ("ifc/empty-model.ifc", "ICE3_STUDIO_IMPORT_EMPTY_GEOMETRY"),
        ("ifc/truncated.ifc", "ICE3_STUDIO_IMPORT_INVALID_SOURCE"),
    ] {
        let result = run_import(&toolchain_fixture(fixture), "ifc", "ifc-failure", false);
        assert!(!result.output.status.success());
        assert!(
            String::from_utf8_lossy(&result.output.stderr).contains(expected_code),
            "{fixture}: {}",
            String::from_utf8_lossy(&result.output.stderr)
        );
        assert!(!result.path.exists());
    }
}

#[cfg(debug_assertions)]
#[test]
fn limit_failure_code_crosses_the_worker_boundary() {
    let root = tempfile::tempdir().unwrap();
    let output_path = root.path().join("ifc.glb");
    let result = run_import_at_path(
        &toolchain_fixture("ifc/synthetic-plant.ifc"),
        "ifc",
        "ifc-limit",
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
    let output_path = root.path().join("ifc.glb");
    std::fs::write(&output_path, b"keep").unwrap();
    let result = run_import_at_path(
        &toolchain_fixture("ifc/synthetic-plant.ifc"),
        "ifc",
        "ifc-timeout",
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

fn assert_failure_preserves_output(fixture: &str, expected_code: &str) {
    let root = tempfile::tempdir().unwrap();
    let output_path = root.path().join("ifc.glb");
    std::fs::write(&output_path, b"keep").unwrap();
    let result = run_import_at_path(
        &toolchain_fixture(fixture),
        "ifc",
        "ifc-failure",
        &output_path,
        true,
    )
    .output()
    .unwrap();

    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains(expected_code),
        "{fixture}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(std::fs::read(&output_path).unwrap(), b"keep");
}

fn run_import(fixture: &Path, adapter: &str, asset_id: &str, force: bool) -> ImportRun {
    let root = tempfile::tempdir().unwrap();
    let output_path = root.path().join("ifc.glb");
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
