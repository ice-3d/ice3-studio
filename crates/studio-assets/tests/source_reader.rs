mod common;

use common::{minimal_binary, pack_glb, packed_fixture};
use serde_json::json;
use studio_assets::{read_accessor, read_glb, validate_prepared_profile};

#[test]
fn reads_glb_buffers_and_accessors() {
    let binary = minimal_binary();
    let (_directory, path) = packed_fixture("studio-prepared.json", &binary);
    let source = read_glb(&path).unwrap();
    let positions = read_accessor(&source, 0).unwrap();
    let indices = read_accessor(&source, 1).unwrap();

    assert_eq!(positions.components(), 3);
    assert_eq!(
        positions.as_f32(),
        Some(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0][..])
    );
    assert_eq!(indices.as_u16(), Some(&[0, 1, 2][..]));
    assert_eq!(source.digest.len(), 64);
}

#[test]
fn rejects_truncated_glb() {
    let error = studio_assets::read_glb_bytes(&[0x67, 0x6C, 0x54, 0x46], &std::env::temp_dir())
        .unwrap_err();
    assert_eq!(error.code(), "ICE3_ASSET_INVALID_GLB");
}

#[test]
fn normalized_unsigned_texcoords_decode_to_float_unit_range() {
    let values = [0_u16, 0, u16::MAX, 32_768];
    let binary = values
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    let gltf = json!({
        "asset": {"version": "2.0"},
        "buffers": [{"byteLength": binary.len()}],
        "bufferViews": [{
            "buffer": 0,
            "byteOffset": 0,
            "byteLength": binary.len()
        }],
        "accessors": [{
            "bufferView": 0,
            "componentType": 5123,
            "count": 2,
            "type": "VEC2",
            "normalized": true
        }]
    });
    let glb = pack_glb(&serde_json::to_vec(&gltf).unwrap(), &binary);
    let source = studio_assets::read_glb_bytes(&glb, &std::env::temp_dir()).unwrap();

    let values = studio_assets::read_accessor(&source, 0).unwrap();

    assert_eq!(
        values.as_f32().unwrap(),
        [0.0, 0.0, 1.0, 32_768.0 / 65_535.0]
    );
}

#[test]
fn prepared_profile_preserves_asset_identity() {
    let binary = minimal_binary();
    let (_directory, path) = packed_fixture("studio-prepared.json", &binary);
    let source = read_glb(&path).unwrap();
    let profile = validate_prepared_profile(&source).unwrap();
    assert_eq!(profile.profile_version, "1.0");
    assert_eq!(profile.project_id.as_deref(), Some("studio-prepared"));
}

#[test]
fn decodes_draco_geometry_before_reading_accessors() {
    use draco_gltf::{
        ComponentType, CompressionOptions, GeometryEncoding, GeometryWriteOptions, Import,
        PackedAttribute, PackedGeometry, PackedIndices, PrimitiveMode, ValidationProfile,
    };

    let positions = [[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]
        .into_iter()
        .flatten()
        .flat_map(f32::to_le_bytes)
        .collect();
    let attribute =
        PackedAttribute::new("POSITION", 3, 3, ComponentType::F32, false, positions).unwrap();
    let indices = PackedIndices::new(
        3,
        ComponentType::U16,
        [0_u16, 1, 2]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect(),
    )
    .unwrap();
    let geometry =
        PackedGeometry::new(PrimitiveMode::Triangles, vec![attribute], Some(indices)).unwrap();
    let import = Import::from_geometry(
        &geometry,
        ValidationProfile::Gltf20,
        GeometryWriteOptions {
            encoding: GeometryEncoding::Draco(CompressionOptions::default()),
        },
    )
    .unwrap();
    let glb = import.to_bytes(draco_gltf::OutputFormat::GlbV2).unwrap();

    let source = studio_assets::read_glb_bytes(&glb, &std::env::temp_dir()).unwrap();
    let position_accessor = source.json["meshes"][0]["primitives"][0]["attributes"]["POSITION"]
        .as_u64()
        .unwrap() as usize;
    let positions = read_accessor(&source, position_accessor).unwrap();
    assert_eq!(
        positions.as_f32().unwrap()[..9],
        [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]
    );
}
