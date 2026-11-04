//! PBR surface parameters, texture references and generated UVs.

use studio_core::apply::{ApplyRequest, apply_transaction};
use studio_core::document::{Document, Material, MaterialSurface, NodeContent, SceneNode};
use studio_core::export::glb::export_glb;
use studio_core::geometry::Primitive;
use studio_core::ids::{MaterialId, NodeId, Revision};
use studio_core::operation::Operation;
use studio_core::patch::MaterialPatch;
use studio_core::validation::validate_document;

fn material(surface: Option<MaterialSurface>) -> Material {
    Material {
        id: MaterialId::from("material-000001"),
        name: "Brushed steel".into(),
        base_color: [0.6, 0.6, 0.62, 1.0],
        opacity: 1.0,
        edge_color: None,
        shading: None,
        surface,
    }
}

fn document_with(surface: Option<MaterialSurface>) -> Document {
    let mut document = Document::new("pbr");
    document.materials.push(material(surface));
    let mut node = SceneNode::group(NodeId::from("node-000001"), "Plate");
    node.material_id = Some(MaterialId::from("material-000001"));
    node.content = NodeContent::Primitive(Primitive::Box {
        width: 2.0,
        height: 1.0,
        depth: 0.5,
    });
    document.scene.nodes_mut().push(node);
    document
}

fn gltf_json(document: &Document) -> serde_json::Value {
    let bytes = export_glb(document).expect("export");
    let json_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    serde_json::from_slice(&bytes[20..20 + json_len]).expect("json chunk")
}

#[test]
fn metallic_and_roughness_reach_the_gltf_material() {
    let document = document_with(Some(MaterialSurface {
        metallic: 1.0,
        roughness: 0.25,
        ..MaterialSurface::dielectric()
    }));
    let gltf = gltf_json(&document);
    let pbr = &gltf["materials"][0]["pbrMetallicRoughness"];
    assert_eq!(pbr["metallicFactor"], 1.0);
    assert_eq!(pbr["roughnessFactor"], 0.25);
}

#[test]
fn a_material_without_surface_keeps_the_dielectric_defaults() {
    let gltf = gltf_json(&document_with(None));
    let pbr = &gltf["materials"][0]["pbrMetallicRoughness"];
    assert_eq!(pbr["metallicFactor"], 0.0);
    assert!((pbr["roughnessFactor"].as_f64().unwrap() - 0.9).abs() < 1e-6);
    assert!(gltf["materials"][0]["emissiveFactor"].is_null());
    assert!(gltf["textures"].is_null(), "no texture, no texture array");
}

#[test]
fn emissive_and_texture_are_exported_together() {
    let document = document_with(Some(MaterialSurface {
        metallic: 0.0,
        roughness: 0.5,
        emissive: Some([0.0, 0.4, 0.8]),
        uv_scale: [2.0, 3.0],
        base_color_texture: Some("textures/steel.png".into()),
        ..MaterialSurface::dielectric()
    }));
    let gltf = gltf_json(&document);
    let emissive = gltf["materials"][0]["emissiveFactor"]
        .as_array()
        .expect("emissive array");
    for (value, expected) in emissive.iter().zip([0.0, 0.4, 0.8]) {
        assert!((value.as_f64().unwrap() - expected).abs() < 1e-6);
    }
    let texture = &gltf["materials"][0]["pbrMetallicRoughness"]["baseColorTexture"];
    assert_eq!(texture["index"], 0);
    assert_eq!(
        texture["extensions"]["KHR_texture_transform"]["scale"],
        serde_json::json!([2.0, 3.0])
    );
    assert_eq!(gltf["images"][0]["uri"], "textures/steel.png");
    assert_eq!(gltf["textures"][0]["source"], 0);
    assert_eq!(gltf["textures"][0]["sampler"], 0);
    assert_eq!(gltf["samplers"][0]["wrapS"], 10497);
    assert_eq!(gltf["extensionsUsed"][0], "KHR_texture_transform");
}

#[test]
fn meshes_carry_generated_texture_coordinates() {
    let document = document_with(None);
    let gltf = gltf_json(&document);
    let attributes = &gltf["meshes"][0]["primitives"][0]["attributes"];
    let uv_accessor = attributes["TEXCOORD_0"]
        .as_u64()
        .expect("TEXCOORD_0 accessor");
    let position_accessor = attributes["POSITION"].as_u64().unwrap();
    let uv = &gltf["accessors"][uv_accessor as usize];
    let position = &gltf["accessors"][position_accessor as usize];
    assert_eq!(uv["type"], "VEC2");
    assert_eq!(uv["count"], position["count"], "one UV per vertex");
}

#[test]
fn invalid_surface_values_are_rejected_by_validation() {
    let bad_metallic = document_with(Some(MaterialSurface {
        metallic: 1.5,
        ..MaterialSurface::dielectric()
    }));
    assert!(!validate_document(&bad_metallic).is_empty());

    let bad_texture = document_with(Some(MaterialSurface {
        base_color_texture: Some("/abs/steel.png".into()),
        ..MaterialSurface::dielectric()
    }));
    assert!(!validate_document(&bad_texture).is_empty());

    let bad_scale = document_with(Some(MaterialSurface {
        uv_scale: [0.0, 1.0],
        ..MaterialSurface::dielectric()
    }));
    assert!(!validate_document(&bad_scale).is_empty());

    let good = document_with(Some(MaterialSurface {
        metallic: 0.8,
        roughness: 0.2,
        base_color_texture: Some("textures/steel.png".into()),
        ..MaterialSurface::dielectric()
    }));
    assert!(validate_document(&good).is_empty());
}

fn patch(surface: Option<MaterialSurface>, clear: bool) -> ApplyRequest {
    ApplyRequest {
        expected_revision: Revision::new(0),
        dry_run: false,
        idempotency_key: "material-surface".into(),
        operations: vec![Operation::MaterialPatch(MaterialPatch {
            material_id: MaterialId::from("material-000001"),
            name: None,
            base_color: None,
            opacity: None,
            edge_color: None,
            clear_edge_color: false,
            surface,
            clear_surface: clear,
        })],
    }
}

#[test]
fn a_patch_can_set_and_clear_the_surface() {
    let mut document = document_with(None);
    apply_transaction(
        &mut document,
        patch(
            Some(MaterialSurface {
                metallic: 0.9,
                roughness: 0.1,
                ..MaterialSurface::dielectric()
            }),
            false,
        ),
    )
    .expect("set surface");
    let surface = document.materials[0].surface.as_ref().expect("surface");
    assert_eq!(surface.metallic, 0.9);

    let mut document = document_with(Some(MaterialSurface {
        metallic: 0.9,
        roughness: 0.1,
        ..MaterialSurface::dielectric()
    }));
    apply_transaction(&mut document, patch(None, true)).expect("clear surface");
    assert!(document.materials[0].surface.is_none());
}

#[test]
fn conflicting_surface_and_clear_surface_is_refused() {
    let mut document = document_with(None);
    let outcome = apply_transaction(
        &mut document,
        patch(Some(MaterialSurface::dielectric()), true),
    );
    assert!(outcome.is_err());
    assert!(
        document.materials[0].surface.is_none(),
        "document untouched"
    );
}

#[test]
fn a_material_without_surface_round_trips_without_the_key() {
    let json = serde_json::to_value(material(None)).expect("serialize");
    assert!(
        json.get("surface").is_none(),
        "absent surface must not add a key: {json}"
    );
    let decoded: Material = serde_json::from_value(json).expect("deserialize");
    assert!(decoded.surface.is_none());
}

#[test]
fn surface_defaults_fill_in_for_a_minimal_document() {
    let surface: MaterialSurface = serde_json::from_str("{}").expect("defaults");
    assert_eq!(surface, MaterialSurface::dielectric());
    let decoded: MaterialSurface =
        serde_json::from_str(r#"{"metallic":0.3,"baseColorTexture":"a.png"}"#).expect("partial");
    assert_eq!(decoded.metallic, 0.3);
    assert_eq!(decoded.roughness, 0.9);
    assert_eq!(decoded.uv_scale, [1.0, 1.0]);
}

#[test]
fn a_normal_map_reaches_the_gltf_material_with_its_strength() {
    let document = document_with(Some(MaterialSurface {
        metallic: 0.4,
        roughness: 0.35,
        base_color_texture: Some("textures/steel.png".into()),
        normal_texture: Some("textures/steel-normal.png".into()),
        normal_scale: Some(0.6),
        ..MaterialSurface::dielectric()
    }));
    let gltf = gltf_json(&document);
    // Image indices follow the deduplicated path table, so resolve each slot
    // back through the material instead of assuming declaration order.
    let base = gltf["materials"][0]["pbrMetallicRoughness"]["baseColorTexture"]["index"]
        .as_u64()
        .unwrap() as usize;
    let normal = gltf["materials"][0]["normalTexture"]["index"]
        .as_u64()
        .unwrap() as usize;
    assert_eq!(gltf["images"][base]["uri"], "textures/steel.png");
    assert_eq!(gltf["images"][normal]["uri"], "textures/steel-normal.png");
    assert_ne!(base, normal);
    assert!(
        (gltf["materials"][0]["normalTexture"]["scale"]
            .as_f64()
            .unwrap()
            - 0.6)
            .abs()
            < 1e-6
    );
    assert_eq!(gltf["textures"].as_array().unwrap().len(), 2);
}

#[test]
fn an_illegal_normal_scale_or_path_is_rejected() {
    let bad_scale = document_with(Some(MaterialSurface {
        normal_texture: Some("textures/steel-normal.png".into()),
        normal_scale: Some(3.0),
        ..MaterialSurface::dielectric()
    }));
    assert!(!validate_document(&bad_scale).is_empty());

    let bad_path = document_with(Some(MaterialSurface {
        normal_texture: Some("../outside/normal.png".into()),
        ..MaterialSurface::dielectric()
    }));
    assert!(!validate_document(&bad_path).is_empty());
}

#[test]
fn an_orm_map_fills_both_gltf_slots_from_one_image() {
    let document = document_with(Some(MaterialSurface {
        metallic: 0.8,
        roughness: 0.4,
        orm_texture: Some("textures/steel-orm.png".into()),
        uv_scale: [0.5, 0.25],
        ..MaterialSurface::dielectric()
    }));
    let gltf = gltf_json(&document);
    let pbr = &gltf["materials"][0]["pbrMetallicRoughness"];
    let metallic_roughness = pbr["metallicRoughnessTexture"]["index"].as_u64().unwrap();
    let occlusion = gltf["materials"][0]["occlusionTexture"]["index"]
        .as_u64()
        .unwrap();
    // 一张图两个槽位：索引必须一致，否则运行时会当成两张贴图各采样一次。
    assert_eq!(metallic_roughness, occlusion);
    assert_eq!(
        gltf["images"][metallic_roughness as usize]["uri"],
        "textures/steel-orm.png"
    );
    // 平铺倍率跟着 uvScale 走，和底色贴图共用同一套 UV。
    assert_eq!(
        pbr["metallicRoughnessTexture"]["extensions"]["KHR_texture_transform"]["scale"],
        serde_json::json!([0.5, 0.25])
    );
    assert_eq!(
        gltf["materials"][0]["occlusionTexture"]["extensions"]["KHR_texture_transform"]["scale"],
        serde_json::json!([0.5, 0.25])
    );
    // 因子仍然写进材质：贴图与标量相乘才是最终值。
    // f32 存 0.8 会带出 0.8000000119，按容差比。
    assert!((pbr["metallicFactor"].as_f64().unwrap() - 0.8).abs() < 1e-6);
    assert!((pbr["roughnessFactor"].as_f64().unwrap() - 0.4).abs() < 1e-6);
}

#[test]
fn an_orm_path_must_stay_relative() {
    let bad = document_with(Some(MaterialSurface {
        orm_texture: Some("/abs/orm.png".into()),
        ..MaterialSurface::dielectric()
    }));
    assert!(!validate_document(&bad).is_empty());
}
