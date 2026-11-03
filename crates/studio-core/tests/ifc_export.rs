//! IFC4X3 export round-trip: parse the emitted file with `oxideav-ifc` and
//! verify units, spatial hierarchy, styles, Unicode names and geometry.

use std::collections::BTreeMap;

use studio_core::document::{Document, Material};
use studio_core::export::ifc::export_ifc;
use studio_core::export::step::StepExportError;
use studio_core::geometry::{Bounds, EvaluatedMesh, Vertex};
use studio_core::ids::MaterialId;
use studio_core::large_scene::evaluate::{EvaluatedScene, EvaluatedSceneNode, LargeSceneStats};
use studio_core::math::{Transform, Vec3};

fn triangle_mesh() -> EvaluatedMesh {
    EvaluatedMesh {
        vertices: vec![
            Vertex {
                position: [0.0, 0.0, 0.0],
                normal: [0.0, 0.0, 1.0],
            },
            Vertex {
                position: [1.0, 0.0, 0.0],
                normal: [0.0, 0.0, 1.0],
            },
            Vertex {
                position: [0.0, 1.0, 0.0],
                normal: [0.0, 0.0, 1.0],
            },
        ],
        indices: vec![0, 1, 2],
        bounds: Bounds {
            min: Vec3::default(),
            max: Vec3::default(),
        },
    }
}

fn instance_node(
    id: &str,
    name: &str,
    x: f32,
    material: Option<&MaterialId>,
) -> EvaluatedSceneNode {
    EvaluatedSceneNode {
        id: id.to_string(),
        owner_id: id.to_string(),
        name: name.to_string(),
        parent_id: None,
        transform: Transform {
            translation: Vec3 { x, y: 0.0, z: 0.0 },
            ..Transform::IDENTITY
        },
        material_id: material.cloned(),
        mesh_key: Some("mesh".to_string()),
        resident_geometry_id: None,
        metadata: BTreeMap::new(),
        visible: true,
    }
}

fn document_with_material(material_id: &MaterialId) -> Document {
    let mut document = Document::new("test");
    document.materials.push(Material {
        id: material_id.clone(),
        name: "red translucent".to_string(),
        base_color: [1.0, 0.0, 0.0, 1.0],
        opacity: 0.5,
        edge_color: None,
        shading: None,
    });
    document
}

fn scene(material_id: &MaterialId) -> EvaluatedScene {
    let mut meshes = BTreeMap::new();
    meshes.insert("mesh".to_string(), triangle_mesh());
    EvaluatedScene {
        nodes: vec![
            instance_node("a", "泵房", 0.0, Some(material_id)),
            instance_node("b", "pump-2", 5.0, Some(material_id)),
        ],
        meshes,
        resident_geometry_ids: BTreeMap::new(),
        stats: LargeSceneStats::default(),
    }
}

#[test]
fn ifc_round_trips_spatial_styles_unicode_and_geometry() {
    let material_id = MaterialId::new("mat", 1).expect("valid id");
    let document = document_with_material(&material_id);
    let scene = scene(&material_id);

    let ifc = export_ifc(&document, &scene).expect("ifc export");
    assert!(ifc.contains("FILE_SCHEMA(('IFC4X3'))"));
    assert!(ifc.contains("IFCSITE"));
    assert!(ifc.contains("IFCBUILDING"));
    assert!(ifc.contains("IFCBUILDINGSTOREY"));
    assert!(ifc.contains("IFCRELAGGREGATES"));
    assert!(ifc.contains("IFCRELCONTAINEDINSPATIALSTRUCTURE"));
    assert!(ifc.contains("IFCSTYLEDITEM"));
    assert!(ifc.contains("IFCCOLOURRGB"));
    assert!(ifc.contains("\\X2\\"));

    let step = oxideav_ifc::parse_step(ifc.as_bytes()).expect("emitted IFC parses");
    let scale = oxideav_ifc::length_unit_scale(&step).expect("length unit declared");
    assert!((scale - 0.001).abs() < 1e-9, "millimetres, got {scale}");

    // Spatial hierarchy: project → site → building → storey → 2 products.
    let model = oxideav_ifc::Model::from_step(&step);
    let project = model.project_id().expect("project root");
    let sites = model.aggregated_children(project);
    assert_eq!(sites.len(), 1, "project aggregates one site");
    let buildings = model.aggregated_children(sites[0]);
    assert_eq!(buildings.len(), 1, "site aggregates one building");
    let storeys = model.aggregated_children(buildings[0]);
    assert_eq!(storeys.len(), 1, "building aggregates one storey");
    let contained = model.contained_elements(storeys[0]);
    assert_eq!(
        contained.len(),
        2,
        "both products are contained in the storey"
    );

    // Unicode name survives the `\X2\` encoding round trip.
    let mut names = Vec::new();
    for product in step.instances_of("IFCBUILDINGELEMENTPROXY") {
        if let Some(oxideav_ifc::Value::String(name)) = product.args.get(2) {
            names.push(name.clone());
        }
    }
    assert!(names.iter().any(|name| name == "泵房"), "got {names:?}");

    // Geometry is still a single shared tessellated body.
    let representation = step
        .instances_of("IFCSHAPEREPRESENTATION")
        .next()
        .expect("shape representation")
        .id;
    let mesh = oxideav_ifc::mesh_from_shape_representation(&step, representation)
        .expect("tessellated body meshes");
    assert_eq!(mesh.triangle_count(), 1, "shared geometry written once");
}

#[test]
fn empty_scene_is_rejected() {
    let document = Document::new("test");
    let scene = EvaluatedScene {
        nodes: Vec::new(),
        meshes: BTreeMap::new(),
        resident_geometry_ids: BTreeMap::new(),
        stats: LargeSceneStats::default(),
    };
    assert!(matches!(
        export_ifc(&document, &scene),
        Err(StepExportError::Empty)
    ));
}

/// Opt-in: parse a real exported IFC file when `STUDIO_IFC_SAMPLE` points at one.
#[test]
fn parses_exported_ifc_sample_when_available() {
    let Ok(path) = std::env::var("STUDIO_IFC_SAMPLE") else {
        return;
    };
    let bytes = std::fs::read(path).expect("sample IFC file");
    let step = oxideav_ifc::parse_step(&bytes).expect("sample IFC parses");
    assert!(step.instances_of("IFCBUILDINGELEMENTPROXY").count() > 0);
    let scale = oxideav_ifc::length_unit_scale(&step).expect("sample declares units");
    assert!((scale - 0.001).abs() < 1e-9, "millimetres, got {scale}");
    let model = oxideav_ifc::Model::from_step(&step);
    let project = model.project_id().expect("sample has a project");
    let sites = model.aggregated_children(project);
    assert_eq!(sites.len(), 1, "sample site");
    let storeys = model.aggregated_children(model.aggregated_children(sites[0])[0]);
    assert_eq!(
        model.contained_elements(storeys[0]).len(),
        4950,
        "all products contained"
    );
    let mut triangles = 0usize;
    for representation in step.instances_of("IFCSHAPEREPRESENTATION") {
        if let Ok(mesh) = oxideav_ifc::mesh_from_shape_representation(&step, representation.id) {
            triangles += mesh.triangle_count();
        }
    }
    assert!(triangles > 0, "sample meshes triangles");
}
