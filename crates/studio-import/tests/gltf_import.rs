use std::path::{Path, PathBuf};

use serde_json::json;
use studio_import::{GltfImporter, SourceFormat};

const PNG_BYTES: &[u8] = &[
    0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R',
];

#[test]
fn imports_gltf_geometry_hierarchy_and_selection_ids() {
    let scene = GltfImporter::new()
        .import_file(&fixture("minimal.glb"), "gltf-fixture")
        .unwrap();
    assert_eq!(scene.source_format, SourceFormat::Gltf);
    assert_eq!(scene.nodes.len(), 1);
    assert_eq!(scene.meshes.len(), 1);
    assert_eq!(scene.meshes[0].primitives[0].indices, [0, 1, 2]);
    assert_eq!(
        scene.nodes[0].extras["ice3"]["selectionId"],
        "gltf-fixture/node/node-000000"
    );
}

#[test]
fn preserves_a_mesh_shared_by_two_nodes() {
    let scene = GltfImporter::new()
        .import_file(&fixture("shared-mesh.gltf"), "shared")
        .unwrap();
    assert_eq!(scene.meshes.len(), 1);
    assert_eq!(
        scene
            .nodes
            .iter()
            .filter(|node| node.mesh == Some(0))
            .count(),
        2
    );
    assert_eq!(scene.nodes[0].parent, None);
    assert_eq!(scene.nodes[1].parent, None);
}

#[test]
fn imports_external_data_and_triangle_uvs_as_resources() {
    let root = tempfile::tempdir().unwrap();
    let texture_dir = root.path().join("textures");
    std::fs::create_dir(&texture_dir).unwrap();
    std::fs::write(texture_dir.join("base.png"), PNG_BYTES).unwrap();
    std::fs::write(root.path().join("mesh.bin"), triangle_binary()).unwrap();
    let path = root.path().join("uv.gltf");
    std::fs::write(&path, uv_gltf_json("textures/base.png")).unwrap();

    let scene = GltfImporter::new().import_file(&path, "uv").unwrap();

    assert_eq!(
        scene.meshes[0].primitives[0].uvs,
        Some(vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]])
    );
    assert_eq!(scene.resources.len(), 1);
    assert_eq!(scene.resources[0].media_type, "image/png");
    assert_eq!(scene.resources[0].digest.len(), 64);
}

#[test]
fn imports_an_embedded_glb_image_without_an_unsupported_resource_warning() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("embedded.glb");
    std::fs::write(&path, embedded_image_glb()).unwrap();

    let scene = GltfImporter::new().import_file(&path, "embedded").unwrap();

    assert_eq!(scene.resources.len(), 1);
    assert_eq!(scene.resources[0].bytes, PNG_BYTES);
    assert!(!scene.warnings.iter().any(|warning| {
        warning.code == "ICE3_STUDIO_IMPORT_RESOURCE_UNSUPPORTED"
            && warning.message.contains("images")
    }));
}

fn fixture(name: &str) -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    match name {
        "shared-mesh.gltf" => manifest.join("tests/fixtures/gltf/shared-mesh.gltf"),
        "minimal.glb" => manifest.join("../../fixtures/source-import/gltf/minimal.glb"),
        _ => panic!("unknown fixture"),
    }
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

fn uv_gltf_json(image_uri: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "asset": {"version": "2.0"},
        "buffers": [{"uri": "mesh.bin", "byteLength": 66}],
        "bufferViews": [
            {"buffer": 0, "byteOffset": 0, "byteLength": 36},
            {"buffer": 0, "byteOffset": 36, "byteLength": 24},
            {"buffer": 0, "byteOffset": 60, "byteLength": 6}
        ],
        "accessors": [
            {
                "bufferView": 0,
                "componentType": 5126,
                "count": 3,
                "type": "VEC3",
                "min": [0.0, 0.0, 0.0],
                "max": [1.0, 1.0, 0.0]
            },
            {"bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC2"},
            {"bufferView": 2, "componentType": 5123, "count": 3, "type": "SCALAR"}
        ],
        "images": [{"uri": image_uri}],
        "samplers": [{"wrapS": 10497, "wrapT": 10497}],
        "textures": [{"sampler": 0, "source": 0}],
        "materials": [{
            "pbrMetallicRoughness": {
                "baseColorTexture": {"index": 0}
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
    }))
    .unwrap()
}

fn embedded_image_glb() -> Vec<u8> {
    let mut binary = triangle_binary();
    binary.extend_from_slice(&[0, 0]);
    binary.extend_from_slice(PNG_BYTES);
    let gltf = json!({
        "asset": {"version": "2.0"},
        "buffers": [{"byteLength": binary.len()}],
        "bufferViews": [
            {"buffer": 0, "byteOffset": 0, "byteLength": 36},
            {"buffer": 0, "byteOffset": 36, "byteLength": 24},
            {"buffer": 0, "byteOffset": 60, "byteLength": 6},
            {"buffer": 0, "byteOffset": 68, "byteLength": PNG_BYTES.len()}
        ],
        "accessors": [
            {
                "bufferView": 0,
                "componentType": 5126,
                "count": 3,
                "type": "VEC3",
                "min": [0.0, 0.0, 0.0],
                "max": [1.0, 1.0, 0.0]
            },
            {"bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC2"},
            {"bufferView": 2, "componentType": 5123, "count": 3, "type": "SCALAR"}
        ],
        "images": [{"bufferView": 3, "mimeType": "image/png"}],
        "samplers": [{"wrapS": 10497, "wrapT": 10497}],
        "textures": [{"sampler": 0, "source": 0}],
        "materials": [{
            "pbrMetallicRoughness": {
                "baseColorTexture": {"index": 0}
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
    pack_glb(&serde_json::to_vec(&gltf).unwrap(), &binary)
}

fn pack_glb(json: &[u8], binary: &[u8]) -> Vec<u8> {
    let mut json_chunk = json.to_vec();
    while !json_chunk.len().is_multiple_of(4) {
        json_chunk.push(b' ');
    }
    let mut binary_chunk = binary.to_vec();
    while !binary_chunk.len().is_multiple_of(4) {
        binary_chunk.push(0);
    }
    let total = 12 + 8 + json_chunk.len() + 8 + binary_chunk.len();
    let mut glb = Vec::with_capacity(total);
    glb.extend_from_slice(&0x4654_6C67_u32.to_le_bytes());
    glb.extend_from_slice(&2_u32.to_le_bytes());
    glb.extend_from_slice(&(total as u32).to_le_bytes());
    glb.extend_from_slice(&(json_chunk.len() as u32).to_le_bytes());
    glb.extend_from_slice(&0x4E4F_534A_u32.to_le_bytes());
    glb.extend_from_slice(&json_chunk);
    glb.extend_from_slice(&(binary_chunk.len() as u32).to_le_bytes());
    glb.extend_from_slice(&0x004E_4942_u32.to_le_bytes());
    glb.extend_from_slice(&binary_chunk);
    glb
}
