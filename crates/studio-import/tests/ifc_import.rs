#![cfg(feature = "ifc")]

use std::path::PathBuf;

use studio_import::{IfcImporter, SourceFormat};

#[test]
fn imports_ifc4x3_hierarchy_geometry_and_metadata() {
    let scene = IfcImporter::new()
        .import_file(&source_fixture("ifc/synthetic-plant.ifc"), "ifc-fixture")
        .unwrap();

    assert_eq!(scene.source_format, SourceFormat::Ifc);
    assert_eq!(scene.meshes.len(), 2);
    assert_eq!(
        scene.asset_extras["ice3"]["coordinateSystem"]["upAxis"],
        "y"
    );
    assert_eq!(
        scene.asset_extras["ice3"]["coordinateSystem"]["linearUnit"],
        "meter"
    );
    let source_origin = scene.asset_extras["ice3"]["ifc"]["sourceOrigin"]
        .as_array()
        .expect("IFC source origin");
    assert_eq!(source_origin.len(), 3);
    assert_eq!(source_origin[1], 0.0);
    assert!(
        source_origin
            .iter()
            .all(|value| { value.as_f64().is_some_and(f64::is_finite) })
    );
    assert!(scene.nodes.iter().any(|node| {
        node.name == "Treatment Building" && node.extras["ice3"]["ifc"]["ifcType"] == "IFCBUILDING"
    }));

    let pump = scene
        .nodes
        .iter()
        .find(|node| node.name == "Primary Pump")
        .expect("Primary Pump node");
    assert_eq!(pump.id, "ifc-fixture/ifc/1iwOd0pZv1BAvoVKqbjYQK");
    assert_eq!(
        pump.extras["ice3"]["ifc"]["propertySets"][0]["name"],
        "Pset_PumpTypeCommon"
    );
    assert_eq!(
        pump.extras["ice3"]["ifc"]["quantitySets"][0]["name"],
        "Qto_PumpBaseQuantities"
    );
    assert_eq!(
        pump.extras["ice3"]["ifc"]["systems"][0]["name"],
        "Sludge Transfer"
    );
    assert!(scene.materials.iter().any(|material| {
        (material.base_color[0] - 0.12).abs() < 0.01
            && (material.base_color[1] - 0.42).abs() < 0.01
            && (material.base_color[2] - 0.78).abs() < 0.01
    }));
}

#[test]
fn preserves_ifc_representation_map_instances() {
    let scene = IfcImporter::new()
        .import_file(&local_fixture("ifc/instanced.ifc"), "ifc-instances")
        .unwrap();

    let mesh_nodes = scene
        .nodes
        .iter()
        .filter(|node| node.mesh.is_some())
        .collect::<Vec<_>>();
    assert_eq!(mesh_nodes.len(), 2);
    assert_eq!(mesh_nodes[0].mesh, mesh_nodes[1].mesh);
    assert_ne!(mesh_nodes[0].id, mesh_nodes[1].id);
    assert!(mesh_nodes.iter().all(|node| {
        node.extras["ice3"]["ifc"]["instanceGroup"]
            .as_str()
            .is_some_and(|group| group.starts_with("ifc/instance/"))
    }));
}

fn source_fixture(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/source-import")
        .join(relative)
}

fn local_fixture(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(relative)
}
