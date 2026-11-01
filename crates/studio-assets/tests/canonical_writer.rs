use serde_json::json;
use studio_assets::canonical::{
    CanonicalMaterial, CanonicalMesh, CanonicalNode, CanonicalPrimitive, CanonicalScene,
    PreparedGlbOptions, write_prepared_glb,
};
use studio_assets::{read_glb_bytes, validate_prepared_profile};

#[test]
fn writes_hierarchy_selection_ids_and_materials() {
    let scene = fixture_scene();
    let bytes = write_prepared_glb(&scene, &options("writer-fixture")).unwrap();
    let source = read_glb_bytes(&bytes, &std::env::temp_dir()).unwrap();
    let profile = validate_prepared_profile(&source).unwrap();

    assert_eq!(profile.asset_id, "writer-fixture");
    assert_eq!(source.json["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(
        source.json["nodes"][1]["extras"]["ice3"]["selectionId"],
        "plant/equipment/AE-101"
    );
    assert_eq!(source.json["nodes"][0]["children"], json!([1]));
    assert_eq!(source.json["materials"][0]["alphaMode"], "BLEND");
    assert_eq!(source.json["meshes"][0]["primitives"][0]["mode"], 4);
}

#[test]
fn rejects_a_missing_parent() {
    let mut scene = fixture_scene();
    scene.nodes[1].parent = Some("missing".into());
    let error = write_prepared_glb(&scene, &options("invalid")).unwrap_err();
    assert_eq!(error.code(), "ICE3_ASSET_PROFILE_INVALID");
}

#[test]
fn writes_stable_bytes_for_the_same_scene() {
    let scene = fixture_scene();
    let first = write_prepared_glb(&scene, &options("writer-fixture")).unwrap();
    let second = write_prepared_glb(&scene, &options("writer-fixture")).unwrap();
    assert_eq!(first, second);
}

#[test]
fn generates_normals_when_the_source_has_none() {
    let mut scene = fixture_scene();
    scene.meshes[0].primitives[0].normals = None;
    let bytes = write_prepared_glb(&scene, &options("writer-fixture")).unwrap();
    let source = read_glb_bytes(&bytes, &std::env::temp_dir()).unwrap();
    let primitive = &source.json["meshes"][0]["primitives"][0];
    let normal_accessor = primitive["attributes"]["NORMAL"]
        .as_u64()
        .expect("generated NORMAL accessor");
    assert_eq!(
        source.json["accessors"][normal_accessor as usize]["count"],
        3
    );
}

fn options(asset_id: &str) -> PreparedGlbOptions {
    PreparedGlbOptions {
        asset_id: asset_id.into(),
        source_format: "gltf".into(),
        source_digest: "source-digest".into(),
    }
}

fn fixture_scene() -> CanonicalScene {
    CanonicalScene {
        asset_id: "writer-fixture".into(),
        source_format: "gltf".into(),
        nodes: vec![
            CanonicalNode {
                id: "root".into(),
                name: "Root".into(),
                parent: None,
                matrix: identity(),
                mesh: None,
                material: None,
                extras: json!({"ice3": {"id": "root", "selectionId": "plant/root"}}),
            },
            CanonicalNode {
                id: "wall".into(),
                name: "Wall".into(),
                parent: Some("root".into()),
                matrix: identity(),
                mesh: Some(0),
                material: None,
                extras: json!({
                    "ice3": {
                        "id": "wall",
                        "selectionId": "plant/equipment/AE-101"
                    }
                }),
            },
        ],
        meshes: vec![CanonicalMesh {
            id: "triangle".into(),
            primitives: vec![CanonicalPrimitive {
                positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                normals: Some(vec![[0.0, 0.0, 1.0]; 3]),
                colors: None,
                indices: vec![0, 1, 2],
                material: Some(0),
            }],
            extras: json!({"ice3": {"id": "triangle"}}),
        }],
        materials: vec![CanonicalMaterial {
            id: "glass".into(),
            name: "Glass".into(),
            base_color: [0.2, 0.5, 0.9],
            opacity: 0.75,
            extras: json!({"ice3": {"id": "glass"}}),
        }],
        asset_extras: json!({"coordinateSystem": {"upAxis": "y"}}),
    }
}

fn identity() -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}
