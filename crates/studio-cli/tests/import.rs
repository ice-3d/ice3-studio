use std::path::{Path, PathBuf};
use std::process::Command;

use studio_assets::{read_glb, validate_prepared_profile};

#[test]
fn imports_obj_to_a_readable_prepared_glb() {
    let fixture = toolchain_fixture("obj/synthetic-plant.obj");
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("plant.glb");
    let binary = env!("CARGO_BIN_EXE_studio-cli");
    let result = Command::new(binary)
        .args([
            "import",
            fixture.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--asset-id",
            "plant",
            "--adapter",
            "obj",
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let source = read_glb(&output).unwrap();
    let profile = validate_prepared_profile(&source).unwrap();
    assert_eq!(profile.asset_id, "plant");
    assert!(profile.node_count > 0);
    assert!(profile.mesh_count > 0);

    let validation = Command::new(binary)
        .args(["validate", output.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        validation.status.success(),
        "{}",
        String::from_utf8_lossy(&validation.stderr)
    );
}

#[test]
fn existing_output_is_rejected_unless_force_is_set() {
    let fixture = toolchain_fixture("obj/synthetic-plant.obj");
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("plant.glb");
    std::fs::write(&output, "keep").unwrap();
    let binary = env!("CARGO_BIN_EXE_studio-cli");
    let rejected = Command::new(binary)
        .args([
            "import",
            fixture.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--asset-id",
            "plant",
        ])
        .output()
        .unwrap();
    assert!(!rejected.status.success());
    assert_eq!(std::fs::read_to_string(&output).unwrap(), "keep");

    let replaced = Command::new(binary)
        .args([
            "import",
            fixture.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--asset-id",
            "plant",
            "--force",
        ])
        .output()
        .unwrap();
    assert!(
        replaced.status.success(),
        "{}",
        String::from_utf8_lossy(&replaced.stderr)
    );
    assert!(read_glb(&output).is_ok());
}

#[test]
fn warnings_as_errors_prevents_publication() {
    let fixture = toolchain_fixture("obj/synthetic-plant.obj");
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("plant.glb");
    let binary = env!("CARGO_BIN_EXE_studio-cli");
    let result = Command::new(binary)
        .args([
            "import",
            fixture.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--asset-id",
            "plant",
            "--warnings-as-errors",
        ])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(!output.exists());
}

fn toolchain_fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../ice3-toolchain/tests/fixtures")
        .join(name)
}
