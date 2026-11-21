use std::path::{Path, PathBuf};

use serde_json::json;
use studio_assets::surface::AlphaMode;
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

#[test]
fn imports_all_surface_slots_and_texture_parameters() {
    let root = textured_surface_fixture(0, false);
    let scene = GltfImporter::new()
        .import_file(&root.path().join("surface.gltf"), "surface")
        .unwrap();
    let material = &scene.materials[0];
    let surface = material.surface.as_ref().unwrap();

    assert_eq!(surface.metallic, 0.8);
    assert_eq!(surface.roughness, 0.25);
    assert_eq!(surface.emissive, Some([0.0, 0.1, 0.2]));
    assert_eq!(surface.normal_texture.as_ref().unwrap().scale, 1.7);
    assert_eq!(surface.base_color_texture.as_ref().unwrap().tex_coord, 0);
    assert_eq!(
        surface
            .base_color_texture
            .as_ref()
            .unwrap()
            .transform
            .offset,
        [0.25, 0.5]
    );
    assert_eq!(
        surface.base_color_texture.as_ref().unwrap().sampler.wrap_s,
        33071
    );
    assert_eq!(material.alpha_mode, AlphaMode::Mask);
    assert_eq!(material.alpha_cutoff, 0.3);
    assert!(material.double_sided);
}

#[test]
fn skips_a_texture_bound_to_an_unsupported_uv_channel() {
    let root = textured_surface_fixture(1, false);
    let scene = GltfImporter::new()
        .import_file(&root.path().join("surface.gltf"), "surface")
        .unwrap();

    assert!(
        scene.materials[0]
            .surface
            .as_ref()
            .unwrap()
            .base_color_texture
            .is_none()
    );
    assert!(
        scene
            .warnings
            .iter()
            .any(|warning| { warning.code == "ICE3_STUDIO_IMPORT_TEXTURE_COORDINATE_UNSUPPORTED" })
    );
}

#[test]
fn warns_for_emissive_textures_and_unknown_material_extensions() {
    let root = textured_surface_fixture(0, true);
    let scene = GltfImporter::new()
        .import_file(&root.path().join("surface.gltf"), "surface")
        .unwrap();

    assert!(
        scene
            .warnings
            .iter()
            .any(|warning| { warning.code == "ICE3_STUDIO_IMPORT_TEXTURE_TARGET_UNSUPPORTED" })
    );
    assert!(scene.warnings.iter().any(|warning| {
        warning.code == "ICE3_STUDIO_IMPORT_MATERIAL_EXTENSION_UNSUPPORTED"
            && warning.message.contains("KHR_materials_unlit")
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

fn textured_surface_fixture(
    tex_coord: u64,
    unsupported_material_features: bool,
) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("mesh.bin"), triangle_binary()).unwrap();
    let texture_dir = root.path().join("textures");
    std::fs::create_dir(&texture_dir).unwrap();
    for (index, name) in ["base.png", "normal.png", "occlusion.png", "orm.png"]
        .into_iter()
        .enumerate()
    {
        let mut image = PNG_BYTES.to_vec();
        image.push(index as u8);
        std::fs::write(texture_dir.join(name), image).unwrap();
    }
    let mut material = json!({
        "pbrMetallicRoughness": {
            "baseColorFactor": [0.4, 0.5, 0.6, 0.8],
            "metallicFactor": 0.8,
            "roughnessFactor": 0.25,
            "baseColorTexture": {
                "index": 0,
                "texCoord": tex_coord,
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
    });
    if unsupported_material_features {
        material["emissiveTexture"] = json!({"index": 0});
        material["extensions"] = json!({"KHR_materials_unlit": {}});
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
        "materials": [material],
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
