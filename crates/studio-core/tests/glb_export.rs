use studio_core::document::{Document, Material, MaterialShading, NodeContent, SceneNode};
use studio_core::export::glb::export_glb;
use studio_core::geometry::Primitive;
use studio_core::ids::{MaterialId, NodeId};

#[test]
fn exports_a_standard_glb_header_and_json_chunk() {
    let bytes = export_glb(&Document::new("empty")).unwrap();
    assert_eq!(&bytes[0..4], b"glTF");
    assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 2);
    let json_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
    assert!(json_len > 0);
}

#[test]
fn exports_stable_bytes_for_the_same_revision() {
    let document = Document::new("stable");
    assert_eq!(
        export_glb(&document).unwrap(),
        export_glb(&document).unwrap(),
    );
}

#[test]
fn exports_material_opacity_as_a_gltf_blend_material() {
    let mut document = Document::new("materials");
    let material_id = MaterialId::from("mat-000001");
    document.materials.push(Material {
        id: material_id.clone(),
        name: "Translucent water".into(),
        base_color: [0.1, 0.3, 0.5, 1.0],
        opacity: 0.4,
        edge_color: None,
        shading: None,
        surface: None,
    });
    let mut node = SceneNode::group(NodeId::from("node-000001"), "Water");
    node.material_id = Some(material_id);
    node.content = NodeContent::Primitive(Primitive::Box {
        width: 1.0,
        height: 1.0,
        depth: 1.0,
    });
    document.scene.nodes_mut().push(node);

    let bytes = export_glb(&document).unwrap();
    let json_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    let gltf: serde_json::Value = serde_json::from_slice(&bytes[20..20 + json_len]).unwrap();
    let opacity = gltf["materials"][0]["pbrMetallicRoughness"]["baseColorFactor"][3]
        .as_f64()
        .unwrap();
    assert!((opacity - 0.4).abs() < 1e-6);
    assert_eq!(gltf["materials"][0]["alphaMode"], "BLEND");
    assert_eq!(gltf["materials"][0]["extras"]["ice3"]["edges"], false);
    assert!(gltf["materials"][0]["pbrMetallicRoughness"]["alphaMode"].is_null());
}

#[test]
fn exports_material_edge_color_and_directional_shading_metadata() {
    let mut document = Document::new("material-metadata");
    let material_id = MaterialId::from("mat-000001");
    document.materials.push(Material {
        id: material_id.clone(),
        name: "Blue steel".into(),
        base_color: [0.2, 0.4, 0.6, 1.0],
        opacity: 1.0,
        edge_color: Some([0.1, 0.2, 0.3, 1.0]),
        shading: None,
        surface: None,
    });
    let mut node = SceneNode::group(NodeId::from("node-000001"), "Equipment");
    node.material_id = Some(material_id);
    node.content = NodeContent::Primitive(Primitive::Box {
        width: 1.0,
        height: 1.0,
        depth: 1.0,
    });
    document.scene.nodes_mut().push(node);

    let bytes = export_glb(&document).unwrap();
    let json_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    let gltf: serde_json::Value = serde_json::from_slice(&bytes[20..20 + json_len]).unwrap();
    assert_eq!(
        gltf["materials"][0]["extras"]["ice3"]["edgeColor"],
        "#1a334d"
    );
    assert_eq!(gltf["materials"][0]["extras"]["ice3"]["edges"], true);
    assert_eq!(
        gltf["materials"][0]["extras"]["ice3"]["shading"],
        "directional"
    );
}

#[test]
fn exports_water_materials_with_a_liquid_surface_shading_mode() {
    let mut document = Document::new("water-material");
    let material_id = MaterialId::from("mat-000001");
    document.materials.push(Material {
        id: material_id.clone(),
        name: "Process water".into(),
        base_color: [0.2, 0.4, 0.6, 1.0],
        opacity: 1.0,
        edge_color: None,
        shading: Some(MaterialShading::Water),
        surface: None,
    });
    let mut node = SceneNode::group(NodeId::from("node-000001"), "Basin");
    node.material_id = Some(material_id);
    node.content = NodeContent::Primitive(Primitive::Box {
        width: 1.0,
        height: 1.0,
        depth: 1.0,
    });
    document.scene.nodes_mut().push(node);

    let bytes = export_glb(&document).unwrap();
    let json_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    let gltf: serde_json::Value = serde_json::from_slice(&bytes[20..20 + json_len]).unwrap();
    assert_eq!(gltf["materials"][0]["extras"]["ice3"]["shading"], "water");
}
