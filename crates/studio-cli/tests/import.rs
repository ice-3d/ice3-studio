use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::json;
use sha2::{Digest, Sha256};
use studio_assets::{read_glb, validate_prepared_profile};

const PNG_BYTES: &[u8] = &[
    0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R',
];

#[test]
fn imports_obj_to_a_readable_prepared_glb() {
    let fixture = source_fixture("obj/synthetic-plant.obj");
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
    let fixture = source_fixture("obj/synthetic-plant.obj");
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
    let fixture = source_fixture("obj/synthetic-plant.obj");
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

#[test]
fn import_publishes_glb_and_every_referenced_texture() {
    let root = textured_gltf_fixture();
    let output = root.path().join("plant.glb");
    let result = Command::new(env!("CARGO_BIN_EXE_studio-cli"))
        .args([
            "import",
            root.path().join("surface.gltf").to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--asset-id",
            "plant",
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );

    let parsed = read_glb(&output).unwrap();
    let uri = parsed.json["images"][0]["uri"].as_str().unwrap();
    assert!(uri.starts_with("plant.textures/"));
    let texture = output.parent().unwrap().join(uri);
    assert!(texture.is_file());
    assert_eq!(
        format!("{:x}", Sha256::digest(std::fs::read(texture).unwrap())),
        format!("{:x}", Sha256::digest(PNG_BYTES))
    );
}

fn source_fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/source-import")
        .join(name)
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
