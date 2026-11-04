//! Base-colour textures through the canonical writer and the prepared profile.

use serde_json::json;
use studio_assets::canonical::{
    CanonicalMaterial, CanonicalMesh, CanonicalNode, CanonicalPrimitive, CanonicalScene,
    CanonicalTexture,
};
use studio_assets::{BudgetProfile, PrepareOptions, read_glb_bytes, validate_prepared_profile};

fn scene_with(texture: Option<CanonicalTexture>) -> CanonicalScene {
    CanonicalScene {
        asset_id: "textured".into(),
        source_format: "obj".into(),
        nodes: vec![CanonicalNode {
            id: "panel".into(),
            name: "Panel".into(),
            matrix: [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            parent: None,
            mesh: Some(0),
            material: Some(0),
            extras: json!({"ice3": {"id": "panel"}}),
        }],
        meshes: vec![CanonicalMesh {
            id: "panel-mesh".into(),
            primitives: vec![CanonicalPrimitive {
                positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                normals: Some(vec![[0.0, 0.0, 1.0]; 3]),
                colors: None,
                indices: vec![0, 1, 2],
                material: Some(0),
            }],
            extras: json!({"ice3": {"id": "panel-mesh"}}),
        }],
        materials: vec![CanonicalMaterial {
            id: "steel".into(),
            name: "Steel".into(),
            base_color: [0.6, 0.6, 0.62],
            opacity: 1.0,
            extras: json!({"ice3": {"id": "steel"}}),
            texture,
        }],
        asset_extras: json!({"coordinateSystem": {"upAxis": "y"}}),
    }
}

fn options() -> PrepareOptions {
    PrepareOptions {
        asset_id: "textured".into(),
        source_format: "obj".into(),
        source_digest: "sha256-test".into(),
        crease_angle_degrees: 30.0,
        generate_edges: false,
        budget: BudgetProfile::interactive_desktop_v1(),
    }
}

#[test]
fn a_textured_material_reaches_the_glb_and_passes_the_profile() {
    let scene = scene_with(Some(CanonicalTexture {
        uri: "textures/steel.png".into(),
        uv_scale: [2.0, 3.0],
    }));
    let artifact = studio_assets::prepare_canonical(&scene, options()).expect("prepare");
    let parsed = read_glb_bytes(&artifact.bytes, &std::env::temp_dir()).expect("read glb");

    assert_eq!(parsed.json["images"][0]["uri"], "textures/steel.png");
    assert_eq!(parsed.json["textures"][0]["source"], 0);
    assert_eq!(parsed.json["textures"][0]["sampler"], 0);
    assert_eq!(parsed.json["samplers"][0]["wrapS"], 10497);
    assert_eq!(parsed.json["extensionsUsed"][0], "KHR_texture_transform");
    let pbr = &parsed.json["materials"][0]["pbrMetallicRoughness"];
    assert_eq!(pbr["baseColorTexture"]["index"], 0);
    assert_eq!(
        pbr["baseColorTexture"]["extensions"]["KHR_texture_transform"]["scale"],
        json!([2.0, 3.0])
    );

    validate_prepared_profile(&parsed).expect("profile accepts textures");
}

#[test]
fn an_untextured_material_keeps_the_asset_texture_free() {
    let artifact = studio_assets::prepare_canonical(&scene_with(None), options()).expect("prepare");
    let parsed = read_glb_bytes(&artifact.bytes, &std::env::temp_dir()).expect("read glb");
    assert!(parsed.json.get("images").is_none());
    assert!(parsed.json.get("textures").is_none());
    assert!(parsed.json.get("extensionsUsed").is_none());
    validate_prepared_profile(&parsed).expect("profile accepts plain assets");
}

#[test]
fn an_absolute_texture_path_never_reaches_an_artifact() {
    for uri in [
        "/abs/steel.png",
        "../outside/steel.png",
        "https://example.com/a.png",
    ] {
        let scene = scene_with(Some(CanonicalTexture {
            uri: uri.into(),
            uv_scale: [1.0, 1.0],
        }));
        let error = studio_assets::prepare_canonical(&scene, options())
            .expect_err("unportable texture path must be refused");
        assert!(
            format!("{error}").contains("relative file paths"),
            "{uri}: unexpected error: {error}"
        );
    }
}
