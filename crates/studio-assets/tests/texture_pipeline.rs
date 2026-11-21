//! Base-colour textures through the canonical writer and the prepared profile.

use serde_json::json;
use sha2::{Digest, Sha256};
use studio_assets::canonical::{
    CanonicalMaterial, CanonicalMesh, CanonicalNode, CanonicalNormalTexture, CanonicalPrimitive,
    CanonicalResource, CanonicalScene, CanonicalSurface, CanonicalTexture, CanonicalTextureBinding,
};
use studio_assets::surface::{AlphaMode, TextureSampler, TextureTransform};
use studio_assets::{
    BudgetProfile, PrepareOptions, prepare_canonical_bundle, read_glb_bytes,
    validate_prepared_profile,
};

const PNG_BYTES: &[u8] = &[
    0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R',
];

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
                uvs: None,
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
            alpha_mode: AlphaMode::Opaque,
            alpha_cutoff: 0.5,
            double_sided: false,
            surface: None,
            extras: json!({"ice3": {"id": "steel"}}),
            texture,
        }],
        resources: Vec::new(),
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
        texture_namespace: String::new(),
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
    let uv_index = parsed.json["meshes"][0]["primitives"][0]["attributes"]["TEXCOORD_0"]
        .as_u64()
        .unwrap() as usize;
    assert_eq!(parsed.json["accessors"][uv_index]["count"], 3);

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

#[test]
fn writes_four_surface_slots_uvs_and_transform() {
    let mut scene = scene_with(None);
    let base = resource(b"base");
    let normal = resource(b"normal");
    let occlusion = resource(b"occlusion");
    let orm = resource(b"orm");
    scene.resources = vec![base.clone(), normal.clone(), occlusion.clone(), orm.clone()];
    scene.meshes[0].primitives[0].uvs = Some(vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]);
    scene.materials[0].surface = Some(CanonicalSurface {
        metallic: 0.8,
        roughness: 0.25,
        emissive: Some([0.0, 0.1, 0.2]),
        base_color_texture: Some(binding(&base, 0, [2.0, 3.0])),
        normal_texture: Some(CanonicalNormalTexture {
            binding: binding(&normal, 0, [1.0, 1.0]),
            scale: 1.7,
        }),
        occlusion_texture: Some(binding(&occlusion, 0, [1.0, 1.0])),
        metallic_roughness_texture: Some(binding(&orm, 0, [0.5, 0.25])),
    });

    let artifact = studio_assets::prepare_canonical(&scene, options()).unwrap();
    let parsed = read_glb_bytes(&artifact.bytes, &std::env::temp_dir()).unwrap();
    let material = &parsed.json["materials"][0];

    assert!(
        (material["pbrMetallicRoughness"]["metallicFactor"]
            .as_f64()
            .unwrap()
            - 0.8)
            .abs()
            < 1e-6
    );
    assert!(
        (material["pbrMetallicRoughness"]["roughnessFactor"]
            .as_f64()
            .unwrap()
            - 0.25)
            .abs()
            < 1e-6
    );
    assert!((material["normalTexture"]["scale"].as_f64().unwrap() - 1.7).abs() < 1e-6);
    assert!(material["pbrMetallicRoughness"]["baseColorTexture"]["index"].is_number());
    assert!(material["pbrMetallicRoughness"]["metallicRoughnessTexture"]["index"].is_number());
    assert!(material["normalTexture"]["index"].is_number());
    assert!(material["occlusionTexture"]["index"].is_number());
    let uv_index = parsed.json["meshes"][0]["primitives"][0]["attributes"]["TEXCOORD_0"]
        .as_u64()
        .unwrap() as usize;
    assert_eq!(parsed.json["accessors"][uv_index]["count"], 3);
    assert_eq!(parsed.json["accessors"][uv_index]["type"], "VEC2");
}

#[test]
fn one_image_with_two_samplers_gets_two_texture_indices() {
    let mut scene = scene_with(None);
    let shared = resource(b"shared");
    scene.resources = vec![shared.clone()];
    let first = binding(&shared, 0, [1.0, 1.0]);
    let mut second = first.clone();
    second.sampler.wrap_s = 33071;
    scene.materials[0].surface = Some(CanonicalSurface {
        metallic: 0.0,
        roughness: 1.0,
        emissive: None,
        base_color_texture: Some(first),
        normal_texture: None,
        occlusion_texture: Some(second),
        metallic_roughness_texture: None,
    });

    let artifact = studio_assets::prepare_canonical(&scene, options()).unwrap();
    let parsed = read_glb_bytes(&artifact.bytes, &std::env::temp_dir()).unwrap();

    assert_eq!(parsed.json["images"].as_array().unwrap().len(), 1);
    assert_eq!(parsed.json["samplers"].as_array().unwrap().len(), 2);
    assert_eq!(parsed.json["textures"].as_array().unwrap().len(), 2);
    assert_ne!(
        parsed.json["materials"][0]["pbrMetallicRoughness"]["baseColorTexture"]["index"],
        parsed.json["materials"][0]["occlusionTexture"]["index"]
    );
}

#[test]
fn rejects_resource_bytes_with_an_unsupported_image_format() {
    let mut scene = scene_with(None);
    let invalid = CanonicalResource {
        digest: format!("{:x}", Sha256::digest(b"not-an-image")),
        media_type: "image/png".into(),
        bytes: b"not-an-image".to_vec(),
    };
    scene.resources = vec![invalid.clone()];
    scene.materials[0].surface = Some(CanonicalSurface {
        metallic: 0.0,
        roughness: 1.0,
        emissive: None,
        base_color_texture: Some(binding(&invalid, 0, [1.0, 1.0])),
        normal_texture: None,
        occlusion_texture: None,
        metallic_roughness_texture: None,
    });

    let error = studio_assets::prepare_canonical(&scene, options()).unwrap_err();

    assert_eq!(error.code(), "ICE3_ASSET_PROFILE_INVALID");
}

#[test]
fn bundle_names_resources_inside_the_requested_directory() {
    let mut scene = scene_with(None);
    let base = resource(PNG_BYTES);
    scene.resources = vec![base.clone()];
    let mut options = options();
    options.texture_namespace = "plant.textures".into();
    scene.materials[0].surface = Some(CanonicalSurface {
        metallic: 0.0,
        roughness: 1.0,
        emissive: None,
        base_color_texture: Some(binding(&base, 0, [1.0, 1.0])),
        normal_texture: None,
        occlusion_texture: None,
        metallic_roughness_texture: None,
    });

    let bundle = prepare_canonical_bundle(&scene, options).unwrap();
    let parsed = read_glb_bytes(&bundle.glb, &std::env::temp_dir()).unwrap();
    let expected_uri = format!("plant.textures/{}.png", base.digest);

    assert_eq!(parsed.json["images"][0]["uri"], expected_uri);
    assert_eq!(bundle.resources[0].uri, expected_uri);
    assert_eq!(bundle.resources[0].bytes, base.bytes);
}

fn resource(bytes: &[u8]) -> CanonicalResource {
    let mut image_bytes = PNG_BYTES.to_vec();
    image_bytes.extend_from_slice(bytes);
    CanonicalResource {
        digest: format!("{:x}", Sha256::digest(&image_bytes)),
        media_type: "image/png".into(),
        bytes: image_bytes,
    }
}

fn binding(
    resource: &CanonicalResource,
    tex_coord: u32,
    scale: [f32; 2],
) -> CanonicalTextureBinding {
    CanonicalTextureBinding {
        resource: resource.digest.clone(),
        tex_coord,
        transform: TextureTransform {
            scale,
            ..TextureTransform::default()
        },
        sampler: TextureSampler::default(),
    }
}
