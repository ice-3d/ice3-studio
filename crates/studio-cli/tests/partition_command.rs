use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

#[test]
fn partition_writes_edges_tileset_and_part_glbs() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("dist");
    let result = run_cli(
        &[
            "partition",
            fixture("gltf/prepare-cube.gltf").to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--asset-id",
            "partition-cube",
            "--adapter",
            "gltf",
        ],
        &[],
    );
    assert_success(&result);

    assert!(output.join("tileset.json").is_file());
    let part = std::fs::read_dir(output.join("parts"))
        .unwrap()
        .next()
        .expect("partition part")
        .unwrap()
        .path();
    let parsed = studio_assets::read_glb(&part).unwrap();
    assert!(
        parsed.json["meshes"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|mesh| mesh["primitives"].as_array().unwrap())
            .any(|primitive| {
                primitive["mode"] == 1 && primitive["extras"]["ice3"]["role"] == "edges"
            })
    );
    assert!(
        parsed.json["asset"]["extras"]["ice3"]["stats"]["edgeSegmentCount"]
            .as_u64()
            .unwrap()
            > 0
    );
    let tileset: Value =
        serde_json::from_slice(&std::fs::read(output.join("tileset.json")).unwrap()).unwrap();
    assert!(
        tileset["asset"]["extras"]["ice3"]["stats"]["edgeSegmentCount"]
            .as_u64()
            .unwrap()
            > 0
    );
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["command"], "partition");
    assert_eq!(report["assetId"], "partition-cube");
    assert_eq!(report["adapter"], "gltf");
    assert!(report["stats"]["edgeSegmentCount"].as_u64().unwrap() > 0);
}

#[test]
fn auto_selects_every_native_source_adapter() {
    let root = tempfile::tempdir().unwrap();
    let mut cases = vec![
        ("obj/synthetic-plant.obj", "obj"),
        #[cfg(feature = "fbx")]
        ("fbx/synthetic-binary.fbx", "fbx"),
        #[cfg(feature = "usd")]
        ("usd/synthetic-plant.usdc", "usd"),
        #[cfg(feature = "step")]
        ("step/synthetic-skid.step", "step"),
        #[cfg(feature = "ifc")]
        ("ifc/synthetic-plant.ifc", "ifc"),
        #[cfg(feature = "jt")]
        ("jt/synthetic-contract.jt", "jt"),
    ];
    cases.push(("gltf/prepare-cube.gltf", "gltf"));
    for (fixture_name, adapter) in cases {
        let output = root.path().join(adapter);
        let environment = if adapter == "jt" {
            vec![
                (
                    "ICE3_JT_CONVERTER".to_owned(),
                    fixture("jt/converter.mjs").to_string_lossy().into_owned(),
                ),
                ("ICE3_JT_TIMEOUT_MS".to_owned(), "2000".into()),
            ]
        } else {
            Vec::new()
        };
        let result = run_cli(
            &[
                "partition",
                fixture(fixture_name).to_str().unwrap(),
                "--output",
                output.to_str().unwrap(),
                "--asset-id",
                adapter,
                "--max-triangles",
                "500000",
                "--skip-edges",
            ],
            &environment,
        );
        assert!(
            result.status.success(),
            "{fixture_name}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let report: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(report["adapter"], adapter, "{fixture_name}");
        assert!(output.join("tileset.json").is_file(), "{fixture_name}");
    }
}

#[test]
fn rejects_invalid_partition_option_combinations_without_output() {
    let root = tempfile::tempdir().unwrap();
    let cases = [
        vec!["--semantic-proxy", "auto"],
        vec![
            "--lod",
            "--lod-allocation",
            "proportional",
            "--semantic-proxy",
            "auto",
        ],
        vec!["--lod-ratio", "0"],
        vec!["--lod-ratio", "1"],
        vec!["--lod-curve-samples", "3"],
        vec!["--lod-curve-samples", "33"],
        vec![
            "--lod-min-unit-triangles",
            "251000",
            "--lod-max-triangles",
            "250000",
        ],
    ];
    for (index, extra) in cases.iter().enumerate() {
        let output = root.path().join(format!("invalid-{index}"));
        let mut arguments = vec![
            "partition".to_owned(),
            fixture("gltf/prepare-cube.gltf")
                .to_string_lossy()
                .into_owned(),
            "--output".to_owned(),
            output.to_string_lossy().into_owned(),
            "--asset-id".to_owned(),
            "invalid".to_owned(),
        ];
        arguments.extend(extra.iter().map(|value| (*value).to_owned()));
        let result = run_cli_owned(&arguments, &[]);
        assert!(!result.status.success(), "case {index}");
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("ICE3_STUDIO_ASSET_INVALID_OPTIONS"),
            "case {index}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(!output.exists(), "case {index}");
    }
}

#[test]
fn no_clobber_and_force_replace_whole_output_directories() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("dist");
    std::fs::create_dir(&output).unwrap();
    std::fs::write(output.join("keep.txt"), "keep").unwrap();

    let rejected = run_cli(
        &[
            "partition",
            fixture("gltf/prepare-cube.gltf").to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--asset-id",
            "no-clobber",
        ],
        &[],
    );
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("ICE3_STUDIO_ASSET_OUTPUT_EXISTS"));
    assert_eq!(
        std::fs::read_to_string(output.join("keep.txt")).unwrap(),
        "keep"
    );

    let replaced = run_cli(
        &[
            "partition",
            fixture("gltf/prepare-cube.gltf").to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--asset-id",
            "force",
            "--force",
        ],
        &[],
    );
    assert_success(&replaced);
    assert!(!output.join("keep.txt").exists());
    assert!(output.join("tileset.json").is_file());
}

#[test]
fn failed_import_or_mismatched_adapter_does_not_create_output() {
    let root = tempfile::tempdir().unwrap();
    let empty_output = root.path().join("empty");
    let empty = run_cli(
        &[
            "partition",
            fixture("obj/empty.obj").to_str().unwrap(),
            "--output",
            empty_output.to_str().unwrap(),
            "--asset-id",
            "empty",
        ],
        &[],
    );
    assert!(!empty.status.success());
    assert!(!empty_output.exists());

    let mismatch_output = root.path().join("mismatch");
    let mismatch = run_cli(
        &[
            "partition",
            fixture("gltf/prepare-cube.gltf").to_str().unwrap(),
            "--output",
            mismatch_output.to_str().unwrap(),
            "--asset-id",
            "mismatch",
            "--adapter",
            "obj",
        ],
        &[],
    );
    assert!(!mismatch.status.success());
    assert!(
        String::from_utf8_lossy(&mismatch.stderr)
            .contains("ICE3_STUDIO_IMPORT_ADAPTER_UNAVAILABLE")
    );
    assert!(!mismatch_output.exists());
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn run_cli(arguments: &[&str], environment: &[(String, String)]) -> Output {
    run_cli_owned(
        &arguments
            .iter()
            .map(|value| (*value).to_owned())
            .collect::<Vec<_>>(),
        environment,
    )
}

fn run_cli_owned(arguments: &[String], environment: &[(String, String)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_studio-cli"));
    command.args(arguments);
    for (name, value) in environment {
        command.env(name, value);
    }
    command.output().unwrap()
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/source-import")
        .join(name)
}
