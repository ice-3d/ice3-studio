#![cfg(feature = "jt")]

use std::path::PathBuf;
use std::process::Command;

#[test]
fn imports_jt_through_the_studio_worker() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("jt.glb");
    let result = Command::new(env!("CARGO_BIN_EXE_studio-cli"))
        .args([
            "import",
            fixture("synthetic-contract.jt").to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--asset-id",
            "cli-jt",
            "--adapter",
            "jt",
        ])
        .env("ICE3_JT_CONVERTER", fixture("converter.mjs"))
        .env("ICE3_JT_TIMEOUT_MS", "2000")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );

    let bytes = std::fs::read(&output).unwrap();
    assert_eq!(&bytes[..4], b"glTF");
    assert!(bytes.len() > 20);
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["adapter"], "jt");
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/source-import/jt")
        .join(name)
}
