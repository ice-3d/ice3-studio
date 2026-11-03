use serde_json::json;
use studio_assets::canonical::{
    CanonicalMaterial, CanonicalMesh, CanonicalNode, CanonicalPrimitive, CanonicalScene,
    PreparedGlbOptions, write_prepared_glb,
};
use studio_assets::{
    BudgetProfile, BudgetStatus, PrepareOptions, SourceGlb, read_glb_bytes,
    validate_prepared_profile,
};

#[test]
fn default_prepare_emits_edges_and_real_metrics() {
    let artifact = prepare_with_edges(true);

    assert_eq!(artifact.metrics.triangle_count, 1);
    assert_eq!(artifact.metrics.edge_segment_count, 3);
    assert_eq!(artifact.metrics.primitive_count, 2);
    assert_eq!(artifact.metrics.material_count, 5);
    assert!(artifact.preparation.edges_prepared);
    assert_eq!(artifact.budget.status, BudgetStatus::Pass);

    let parsed = read_glb_bytes(&artifact.bytes, &std::env::temp_dir()).unwrap();
    let primitives = parsed.json["meshes"][0]["primitives"].as_array().unwrap();
    assert_eq!(primitives[0]["mode"], 4);
    assert_eq!(primitives[1]["mode"], 1);
    assert_eq!(primitives[1]["extras"]["ice3"]["role"], "edges");
    assert_eq!(primitives[1]["extras"]["ice3"]["widthPx"], 3);
    assert_eq!(parsed.json["materials"][1]["name"], "Glass edges");
    assert_eq!(parsed.json["materials"][3]["name"], "default edges");
}

#[test]
fn skip_edges_removes_edge_primitives_and_reports_zero() {
    let artifact = prepare_with_edges(false);

    assert_eq!(artifact.metrics.edge_segment_count, 0);
    assert_eq!(artifact.metrics.primitive_count, 1);
    assert_eq!(artifact.metrics.material_count, 5);
    assert!(!artifact.preparation.edges_prepared);
}

#[test]
fn per_material_color_and_suppression_are_preserved() {
    let artifact = prepare_with_material_extras(json!({
        "ice3": {
            "id": "glass",
            "edgeColor": "#10203080"
        }
    }));
    let parsed = read_glb_bytes(&artifact.bytes, &std::env::temp_dir()).unwrap();
    assert_eq!(
        parsed.json["materials"][1]["pbrMetallicRoughness"]["baseColorFactor"],
        json!([
            0.062745101749897,
            0.125490203499794,
            0.1882352977991104,
            0.501960813999176
        ])
    );

    let suppressed = prepare_with_material_extras(json!({
        "ice3": {"id": "glass", "edges": false}
    }));
    assert_eq!(suppressed.metrics.edge_segment_count, 0);
    assert_eq!(suppressed.metrics.primitive_count, 1);
    assert_eq!(suppressed.metrics.material_count, 4);
    let parsed = read_glb_bytes(&suppressed.bytes, &std::env::temp_dir()).unwrap();
    assert!(
        !parsed.json["materials"]
            .as_array()
            .unwrap()
            .iter()
            .any(|material| material["name"] == "Glass edges")
    );
}

#[test]
fn unused_materials_do_not_gain_edge_materials() {
    let mut scene = triangle_scene();
    scene.materials.push(CanonicalMaterial {
        id: "unused".into(),
        name: "Unused".into(),
        base_color: [0.1, 0.2, 0.3],
        opacity: 1.0,
        extras: json!({"ice3": {"id": "unused"}}),
    });
    let artifact = studio_assets::prepare_canonical(
        &scene,
        options_with_budget(true, BudgetProfile::interactive_desktop_v1()),
    )
    .unwrap();

    assert_eq!(artifact.metrics.material_count, 6);
    let parsed = read_glb_bytes(&artifact.bytes, &std::env::temp_dir()).unwrap();
    assert_eq!(parsed.json["materials"][3]["name"], "default");
    assert!(
        !parsed.json["materials"]
            .as_array()
            .unwrap()
            .iter()
            .any(|material| material["name"] == "Unused edges")
    );
}

#[test]
fn hard_budget_returns_no_publishable_artifact() {
    let error = studio_assets::prepare_canonical(
        &triangle_scene(),
        options_with_budget(
            true,
            BudgetProfile::from_json(
                r#"{
                  "id":"tiny",
                  "limits":[{
                    "id":"triangleCount","label":"Triangles",
                    "softLimit":0,"hardLimit":0
                  }]
                }"#,
            )
            .unwrap(),
        ),
    )
    .unwrap_err();

    assert_eq!(error.code(), "ICE3_STUDIO_ASSET_BUDGET_EXCEEDED");
}

#[test]
fn soft_budget_is_reported_without_rejecting_the_artifact() {
    let artifact = studio_assets::prepare_canonical(
        &triangle_scene(),
        options_with_budget(
            true,
            BudgetProfile::from_json(
                r#"{
                  "id":"soft",
                  "limits":[{
                    "id":"triangleCount","label":"Triangles",
                    "softLimit":0,"hardLimit":100
                  }]
                }"#,
            )
            .unwrap(),
        ),
    )
    .unwrap();

    assert_eq!(artifact.budget.status, BudgetStatus::Warning);
    assert_eq!(artifact.budget.warning_count, 1);
}

#[test]
fn prepared_profile_round_trips_metrics_and_preparation() {
    let artifact = prepare_with_edges(true);
    let parsed = read_glb_bytes(&artifact.bytes, &std::env::temp_dir()).unwrap();
    let profile = validate_prepared_profile(&parsed).unwrap();

    assert!(profile.preparation.unwrap().edges_prepared);
    assert_eq!(profile.metrics.edge_segment_count, 3);
    assert_eq!(profile.metrics.material_count, 5);
}

#[test]
fn legacy_writer_remains_edge_free_and_valid() {
    let bytes = write_prepared_glb(
        &triangle_scene(),
        &PreparedGlbOptions {
            asset_id: "asset".into(),
            source_format: "gltf".into(),
            source_digest: "source".into(),
        },
    )
    .unwrap();
    let parsed = read_glb_bytes(&bytes, &std::env::temp_dir()).unwrap();
    let profile = validate_prepared_profile(&parsed).unwrap();
    assert!(!profile.preparation.unwrap().edges_prepared);
    assert_eq!(profile.metrics.edge_segment_count, 0);
}

#[test]
fn identical_prepare_calls_are_byte_deterministic() {
    let first = prepare_with_edges(true);
    let second = prepare_with_edges(true);
    assert_eq!(first.bytes, second.bytes);
    assert_eq!(first.content_digest, second.content_digest);
}

#[test]
fn invalid_crease_angle_is_rejected_even_when_edges_are_skipped() {
    for crease_angle_degrees in [-1.0, 180.1, f32::NAN] {
        let error = studio_assets::prepare_canonical(
            &triangle_scene(),
            PrepareOptions {
                asset_id: "asset".into(),
                source_format: "gltf".into(),
                source_digest: "source".into(),
                crease_angle_degrees,
                generate_edges: false,
                budget: BudgetProfile::interactive_desktop_v1(),
            },
        )
        .unwrap_err();
        assert_eq!(error.code(), "ICE3_STUDIO_ASSET_INVALID_GEOMETRY");
    }
}

#[test]
fn prepared_profile_rejects_invalid_edge_materials_and_accessors() {
    let artifact = prepare_with_edges(true);
    let mut source = read_glb_bytes(&artifact.bytes, &std::env::temp_dir()).unwrap();
    source.json["meshes"][0]["primitives"][1]["material"] = json!(999);
    assert_eq!(
        validate_prepared_profile(&source).unwrap_err().code(),
        "ICE3_ASSET_PROFILE_INVALID"
    );

    let mut source = read_glb_bytes(&artifact.bytes, &std::env::temp_dir()).unwrap();
    let accessor = source.json["meshes"][0]["primitives"][1]["indices"]
        .as_u64()
        .unwrap() as usize;
    source.json["accessors"][accessor]["componentType"] = json!(5126);
    assert_eq!(
        validate_prepared_profile(&source).unwrap_err().code(),
        "ICE3_ASSET_PROFILE_INVALID"
    );
}

#[test]
fn prepared_profile_rejects_inconsistent_preparation_options() {
    let artifact = prepare_with_edges(true);
    let mut source = read_glb_bytes(&artifact.bytes, &std::env::temp_dir()).unwrap();
    source.json["asset"]["extras"]["ice3"]["preparation"]["options"]["edges"] = json!(false);

    assert_eq!(
        validate_prepared_profile(&source).unwrap_err().code(),
        "ICE3_ASSET_PROFILE_INVALID"
    );
}

#[test]
fn prepared_profile_decodes_accessors_and_rejects_range_errors() {
    let artifact = prepare_with_edges(true);
    let mut source = read_glb_bytes(&artifact.bytes, &std::env::temp_dir()).unwrap();
    let accessor = source.json["meshes"][0]["primitives"][1]["indices"]
        .as_u64()
        .unwrap() as usize;
    source.json["accessors"][accessor]["count"] = json!(1_000_000);

    assert_eq!(
        validate_prepared_profile(&source).unwrap_err().code(),
        "ICE3_ASSET_PROFILE_INVALID"
    );
}

#[test]
fn prepared_profile_rejects_out_of_range_decoded_edge_indices() {
    let artifact = prepare_with_edges(true);
    let mut source = read_glb_bytes(&artifact.bytes, &std::env::temp_dir()).unwrap();
    write_first_edge_index(&mut source, u32::MAX);

    assert_eq!(
        validate_prepared_profile(&source).unwrap_err().code(),
        "ICE3_ASSET_PROFILE_INVALID"
    );
}

#[test]
fn edge_primitive_requires_preparation_metadata() {
    let artifact = prepare_with_edges(true);
    let mut source = read_glb_bytes(&artifact.bytes, &std::env::temp_dir()).unwrap();
    source.json["asset"]["extras"]["ice3"]
        .as_object_mut()
        .unwrap()
        .remove("preparation");

    assert_eq!(
        validate_prepared_profile(&source).unwrap_err().code(),
        "ICE3_ASSET_PROFILE_INVALID"
    );
}

#[test]
fn edge_free_legacy_profile_can_omit_preparation_metadata() {
    let bytes = write_prepared_glb(
        &triangle_scene(),
        &PreparedGlbOptions {
            asset_id: "asset".into(),
            source_format: "gltf".into(),
            source_digest: "source".into(),
        },
    )
    .unwrap();
    let mut source = read_glb_bytes(&bytes, &std::env::temp_dir()).unwrap();
    source.json["asset"]["extras"]["ice3"]
        .as_object_mut()
        .unwrap()
        .remove("preparation");

    validate_prepared_profile(&source).unwrap();
}

#[test]
fn edge_material_identity_not_display_name_is_validated() {
    let artifact = prepare_with_edges(true);
    let mut source = read_glb_bytes(&artifact.bytes, &std::env::temp_dir()).unwrap();
    let material = source.json["meshes"][0]["primitives"][1]["material"]
        .as_u64()
        .unwrap() as usize;

    source.json["materials"][material]["name"] = json!("ordinary material");
    validate_prepared_profile(&source).unwrap();

    source.json["materials"][material]["name"] = json!("Glass edges");
    source.json["materials"][material]["extras"]["ice3"]["id"] = json!("asset/material/glass");
    assert_eq!(
        validate_prepared_profile(&source).unwrap_err().code(),
        "ICE3_ASSET_PROFILE_INVALID"
    );
}

fn prepare_with_edges(generate_edges: bool) -> studio_assets::PrepareArtifact {
    studio_assets::prepare_canonical(
        &triangle_scene(),
        options_with_budget(generate_edges, BudgetProfile::interactive_desktop_v1()),
    )
    .unwrap()
}

fn prepare_with_material_extras(extras: serde_json::Value) -> studio_assets::PrepareArtifact {
    let mut scene = triangle_scene();
    scene.materials[0].extras = extras;
    studio_assets::prepare_canonical(
        &scene,
        options_with_budget(true, BudgetProfile::interactive_desktop_v1()),
    )
    .unwrap()
}

fn options_with_budget(generate_edges: bool, budget: BudgetProfile) -> PrepareOptions {
    PrepareOptions {
        asset_id: "asset".into(),
        source_format: "gltf".into(),
        source_digest: "source".into(),
        crease_angle_degrees: 30.0,
        generate_edges,
        budget,
    }
}

fn triangle_scene() -> CanonicalScene {
    CanonicalScene {
        asset_id: "asset".into(),
        source_format: "gltf".into(),
        nodes: vec![CanonicalNode {
            id: "asset/node/0".into(),
            name: "Triangle".into(),
            parent: None,
            matrix: identity(),
            mesh: Some(0),
            material: None,
            extras: json!({
                "ice3": {
                    "id": "asset/node/0",
                    "selectionId": "asset/node/0"
                }
            }),
        }],
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
            opacity: 1.0,
            extras: json!({"ice3": {"id": "glass"}}),
        }],
        asset_extras: json!({"coordinateSystem": {"upAxis": "y"}}),
    }
}

fn write_first_edge_index(source: &mut SourceGlb, value: u32) {
    let accessor_index = source.json["meshes"][0]["primitives"][1]["indices"]
        .as_u64()
        .unwrap() as usize;
    let accessor = source.json["accessors"][accessor_index]
        .as_object()
        .unwrap();
    let view_index = accessor["bufferView"].as_u64().unwrap() as usize;
    let accessor_offset = accessor
        .get("byteOffset")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as usize;
    let component_type = accessor["componentType"].as_u64().unwrap();
    let view = source.json["bufferViews"][view_index].as_object().unwrap();
    let view_offset = view
        .get("byteOffset")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as usize;
    let start = view_offset + accessor_offset;
    match component_type {
        5123 => {
            source.buffers[0][start..start + 2].copy_from_slice(&(value as u16).to_le_bytes());
        }
        5125 => {
            source.buffers[0][start..start + 4].copy_from_slice(&value.to_le_bytes());
        }
        other => panic!("unexpected edge index component type {other}"),
    }
}

fn identity() -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}
