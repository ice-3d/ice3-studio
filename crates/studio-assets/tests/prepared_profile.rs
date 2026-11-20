mod common;

use common::{json_asset, minimal_binary, pack_glb, packed_fixture};
use serde_json::json;
use studio_assets::{read_glb, validate_prepared_profile};

#[test]
fn accepts_a_studio_prepared_glb() {
    let binary = minimal_binary();
    let (_directory, path) = packed_fixture("studio-prepared.json", &binary);
    let source = read_glb(&path).unwrap();
    let profile = validate_prepared_profile(&source).unwrap();
    assert_eq!(profile.asset_id, "studio-prepared");
}

#[test]
fn rejects_an_unprepared_glb() {
    let (_directory, path) = packed_fixture("plain.json", &[]);
    let error = read_glb(&path)
        .and_then(|source| validate_prepared_profile(&source))
        .unwrap_err();
    assert_eq!(error.code(), "ICE3_ASSET_PROFILE_UNSUPPORTED");
}

#[test]
fn rejects_external_buffers_outside_base_directory() {
    let error = read_glb(&common::fixture("external-escape.gltf")).unwrap_err();
    assert_eq!(error.code(), "ICE3_ASSET_INVALID_GLB");
}

#[test]
fn rejects_nonzero_texcoord_and_invalid_texture_transform() {
    let mut json = json_asset("studio-prepared.json");
    json["images"] = json!([{"uri": "textures/base.png", "mimeType": "image/png"}]);
    json["samplers"] = json!([{"wrapS": 10497, "wrapT": 10497}]);
    json["textures"] = json!([{"source": 0, "sampler": 0}]);
    json["materials"][0]["normalTexture"] = json!({
        "index": 0,
        "texCoord": 1,
        "extensions": {
            "KHR_texture_transform": {
                "scale": [0.0, 1.0]
            }
        }
    });

    let source = read_glb_bytes(&pack_glb(
        &serde_json::to_vec(&json).unwrap(),
        &minimal_binary(),
    ))
    .unwrap();

    let error = validate_prepared_profile(&source).unwrap_err();
    assert_eq!(error.code(), "ICE3_ASSET_PROFILE_INVALID");
}

#[test]
fn rejects_invalid_sampler_values_at_the_gltf_boundary() {
    let mut json = json_asset("studio-prepared.json");
    json["images"] = json!([{"uri": "textures/base.png", "mimeType": "image/png"}]);
    json["samplers"] = json!([{"wrapS": 1, "wrapT": 10497}]);
    json["textures"] = json!([{"source": 0, "sampler": 0}]);
    json["materials"][0]["occlusionTexture"] = json!({"index": 0});

    let result = read_glb_bytes(&pack_glb(
        &serde_json::to_vec(&json).unwrap(),
        &minimal_binary(),
    ));

    assert_eq!(result.unwrap_err().code(), "ICE3_ASSET_INVALID_GLB");
}

#[test]
fn rejects_a_texture_mime_extension_mismatch() {
    let mut json = json_asset("studio-prepared.json");
    json["images"] = json!([{"uri": "textures/base.jpg", "mimeType": "image/png"}]);
    json["samplers"] = json!([{"wrapS": 10497, "wrapT": 10497}]);
    json["textures"] = json!([{"source": 0, "sampler": 0}]);
    json["materials"][0]["occlusionTexture"] = json!({"index": 0});
    let source = read_glb_bytes(&pack_glb(
        &serde_json::to_vec(&json).unwrap(),
        &minimal_binary(),
    ))
    .unwrap();

    let error = validate_prepared_profile(&source).unwrap_err();
    assert_eq!(error.code(), "ICE3_ASSET_PROFILE_INVALID");
}

fn read_glb_bytes(bytes: &[u8]) -> studio_assets::AssetResult<studio_assets::SourceGlb> {
    studio_assets::read_glb_bytes(bytes, &std::env::temp_dir())
}
