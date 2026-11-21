use std::process::{Command, Output};

use serde_json::json;
use studio_assets::{AccessorData, read_accessor};

const PNG_BYTES: &[u8] = &[
    0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R',
];

#[test]
fn imported_material_survives_import_and_build_with_all_resources() {
    let root = complete_surface_fixture();
    let import_output = root.path().join("imported.glb");
    let build_output = root.path().join("built.glb");

    run_success(&[
        "import",
        root.path().join("surface.gltf").to_str().unwrap(),
        "--output",
        import_output.to_str().unwrap(),
        "--asset-id",
        "surface",
    ]);
    run_success(&[
        "build",
        root.path().join("surface.gltf").to_str().unwrap(),
        "--output",
        build_output.to_str().unwrap(),
        "--asset-id",
        "surface",
    ]);

    for output in [&import_output, &build_output] {
        let parsed = studio_assets::read_glb(output).unwrap();
        let images = parsed.json["images"].as_array().unwrap();
        assert_eq!(images.len(), 4);
        for image in images {
            assert!(
                output
                    .parent()
                    .unwrap()
                    .join(image["uri"].as_str().unwrap())
                    .is_file()
            );
        }
        let material = &parsed.json["materials"][0];
        let pbr = &material["pbrMetallicRoughness"];
        assert!((pbr["metallicFactor"].as_f64().unwrap() - 0.8).abs() < 1e-6);
        assert!((pbr["roughnessFactor"].as_f64().unwrap() - 0.25).abs() < 1e-6);
        assert!(pbr["baseColorTexture"]["index"].is_number());
        assert!(pbr["metallicRoughnessTexture"]["index"].is_number());
        assert!(material["normalTexture"]["index"].is_number());
        assert!(material["occlusionTexture"]["index"].is_number());
        assert_eq!(
            pbr["baseColorTexture"]["extensions"]["KHR_texture_transform"]["offset"],
            json!([0.25, 0.5])
        );
        assert_eq!(
            pbr["baseColorTexture"]["extensions"]["KHR_texture_transform"]["scale"],
            json!([2.0, 3.0])
        );
        assert_eq!(parsed.json["samplers"][0]["wrapS"], 33071);
        assert_eq!(parsed.json["samplers"][0]["wrapT"], 33648);
        assert_eq!(material["alphaMode"], "MASK");
        assert!((material["alphaCutoff"].as_f64().unwrap() - 0.3).abs() < 1e-6);
        assert_eq!(material["doubleSided"], true);

        let primitive = &parsed.json["meshes"][0]["primitives"][0];
        let uv_accessor = primitive["attributes"]["TEXCOORD_0"].as_u64().unwrap() as usize;
        let AccessorData::F32 { values, components } = read_accessor(&parsed, uv_accessor).unwrap()
        else {
            panic!("TEXCOORD_0 must decode to F32");
        };
        assert_eq!(components, 2);
        assert_eq!(values, [0.0, 0.0, 1.0, 0.0, 0.0, 1.0]);
    }
}

fn run_success(arguments: &[&str]) {
    let output = Command::new(env!("CARGO_BIN_EXE_studio-cli"))
        .args(arguments)
        .output()
        .unwrap();
    assert_success(&output);
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn complete_surface_fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("mesh.bin"), triangle_binary()).unwrap();
    let texture_dir = root.path().join("textures");
    std::fs::create_dir(&texture_dir).unwrap();
    for (index, name) in ["base.png", "normal.png", "occlusion.png", "orm.png"]
        .into_iter()
        .enumerate()
    {
        let mut bytes = PNG_BYTES.to_vec();
        bytes.push(index as u8);
        std::fs::write(texture_dir.join(name), bytes).unwrap();
    }
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
        "images": [
            {"uri": "textures/base.png"},
            {"uri": "textures/normal.png"},
            {"uri": "textures/occlusion.png"},
            {"uri": "textures/orm.png"}
        ],
        "samplers": [{
            "magFilter": 9729,
            "minFilter": 9987,
            "wrapS": 33071,
            "wrapT": 33648
        }],
        "textures": [
            {"sampler": 0, "source": 0},
            {"sampler": 0, "source": 1},
            {"sampler": 0, "source": 2},
            {"sampler": 0, "source": 3}
        ],
        "materials": [{
            "pbrMetallicRoughness": {
                "baseColorFactor": [0.4, 0.5, 0.6, 0.8],
                "metallicFactor": 0.8,
                "roughnessFactor": 0.25,
                "baseColorTexture": {
                    "index": 0,
                    "extensions": {
                        "KHR_texture_transform": {
                            "offset": [0.25, 0.5],
                            "rotation": 0.125,
                            "scale": [2.0, 3.0]
                        }
                    }
                },
                "metallicRoughnessTexture": {"index": 3}
            },
            "normalTexture": {"index": 1, "scale": 1.7},
            "occlusionTexture": {"index": 2},
            "emissiveFactor": [0.0, 0.1, 0.2],
            "alphaMode": "MASK",
            "alphaCutoff": 0.3,
            "doubleSided": true
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

fn triangle_binary() -> Vec<u8> {
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
    binary
}
