use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const PNG_BYTES: &[u8] = &[
    0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R',
];

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
fn partition_publishes_textures_beside_part_glbs() {
    let root = textured_gltf_fixture();
    let output = root.path().join("dist");
    let result = run_cli(
        &[
            "partition",
            root.path().join("surface.gltf").to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--asset-id",
            "partition-textured",
            "--adapter",
            "gltf",
        ],
        &[],
    );
    assert_success(&result);

    let part = first_part_glb(&output);
    let parsed = studio_assets::read_glb(&part).unwrap();
    let uri = parsed.json["images"][0]["uri"].as_str().unwrap();
    let digest = format!("{:x}", Sha256::digest(PNG_BYTES));
    assert_eq!(uri, format!("textures/{digest}.png"));
    assert!(output.join("parts").join(uri).is_file());
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

fn first_part_glb(output: &Path) -> PathBuf {
    std::fs::read_dir(output.join("parts"))
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|extension| extension == "glb"))
        .expect("partition GLB")
}

fn textured_gltf_fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let positions = [[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    let uvs = [[0.0_f32, 0.0], [1.0, 0.0], [0.0, 1.0]];
    let indices = [0_u16, 1, 2];
    let mut binary = Vec::new();
    for value in positions.into_iter().flatten() {
        binary.extend_from_slice(&value.to_le_bytes());
    }
    for value in uvs.into_iter().flatten() {
        binary.extend_from_slice(&value.to_le_bytes());
    }
    for value in indices {
        binary.extend_from_slice(&value.to_le_bytes());
    }
    std::fs::write(root.path().join("mesh.bin"), binary).unwrap();
    let texture_dir = root.path().join("textures");
    std::fs::create_dir(&texture_dir).unwrap();
    std::fs::write(texture_dir.join("base.png"), PNG_BYTES).unwrap();
    let gltf = json!({
        "asset": {"version": "2.0"},
        "buffers": [{"uri": "mesh.bin", "byteLength": 66}],
        "bufferViews": [
            {"buffer": 0, "byteOffset": 0, "byteLength": 36},
            {"buffer": 0, "byteOffset": 36, "byteLength": 24},
            {"buffer": 0, "byteOffset": 60, "byteLength": 6}
        ],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3"},
            {"bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC2"},
            {"bufferView": 2, "componentType": 5123, "count": 3, "type": "SCALAR"}
        ],
        "images": [{"uri": "textures/base.png"}],
        "samplers": [{"wrapS": 10497, "wrapT": 10497}],
        "textures": [{"source": 0, "sampler": 0}],
        "materials": [{
            "pbrMetallicRoughness": {
                "baseColorTexture": {"index": 0},
                "metallicFactor": 0.8,
                "roughnessFactor": 0.25
            }
        }],
        "meshes": [{
            "primitives": [{
                "attributes": {"POSITION": 0, "TEXCOORD_0": 1},
                "indices": 2,
                "material": 0
            }]
        }],
        "nodes": [{"mesh": 0}],
        "scenes": [{"nodes": [0]}],
        "scene": 0
    });
    std::fs::write(
        root.path().join("surface.gltf"),
        serde_json::to_vec(&gltf).unwrap(),
    )
    .unwrap();
    root
}
