#![cfg(feature = "jt")]

use std::path::PathBuf;
use std::time::Duration;

use glam::{Mat4, Vec3};
use studio_import::{ImportError, JtImporter, SourceFormat};

#[test]
fn preserves_hierarchy_instances_materials_and_metadata() {
    let scene = importer()
        .import_file(&fixture("synthetic-contract.jt"), "synthetic-jt")
        .unwrap();

    assert_eq!(scene.source_format, SourceFormat::Jt);
    assert_eq!(scene.meshes.len(), 2);
    assert_eq!(
        scene
            .nodes
            .iter()
            .filter(|node| node.mesh.is_some())
            .count(),
        3
    );

    let pump = scene
        .nodes
        .iter()
        .find(|node| node.name == "PrimaryPump")
        .unwrap();
    let instance = scene
        .nodes
        .iter()
        .find(|node| node.name == "PumpInstanceA")
        .unwrap();
    let root = scene
        .nodes
        .iter()
        .find(|node| node.name == "TreatmentLine")
        .unwrap();
    assert_eq!(root.id, "synthetic-jt/jt/node/0");
    assert_eq!(root.extras["ice3"]["jt"]["objectType"], "EMPTY");
    assert_eq!(root.extras["ice3"]["jt"]["sourceName"], "TreatmentLine");
    assert_eq!(
        root.extras["ice3"]["jt"]["materialNames"],
        serde_json::json!([])
    );
    assert_eq!(pump.mesh, instance.mesh);
    assert_eq!(pump.id, "synthetic-jt/jt/node/1");
    assert_eq!(instance.id, "synthetic-jt/jt/node/2");
    assert_eq!(pump.extras["ice3"]["jt"]["objectType"], "MESH");
    assert_eq!(pump.extras["ice3"]["jt"]["objectId"], "JT-PUMP-101");
    assert_eq!(
        pump.extras["ice3"]["jt"]["assemblyPath"],
        "/Plant/TreatmentLine/PrimaryPump"
    );
    assert_eq!(
        pump.extras["ice3"]["jt"]["customProperties"]["EquipmentTag"],
        "P-101"
    );
    assert_eq!(
        pump.extras["ice3"]["jt"]["materialNames"],
        serde_json::json!(["PumpBlue"])
    );
    assert_eq!(
        instance.extras["ice3"]["jt"]["instanceGroup"],
        pump.extras["ice3"]["jt"]["instanceGroup"]
    );
    assert_eq!(pump.extras["ice3"]["selectionId"], "synthetic-jt/jt/node/1");
    let origin = scene.asset_extras["ice3"]["jt"]["sourceOrigin"]
        .as_array()
        .unwrap();
    assert_eq!(origin[0], -3.0);
    assert_eq!(origin[2], -4.0);

    let materials = scene
        .materials
        .iter()
        .map(|material| (material.name.as_str(), material.base_color))
        .collect::<Vec<_>>();
    assert!(
        materials
            .iter()
            .any(|(name, color)| { *name == "PumpBlue" && close(*color, [0.12, 0.38, 0.68]) })
    );
    assert!(
        materials
            .iter()
            .any(|(name, color)| { *name == "TankTaupe" && close(*color, [0.62, 0.55, 0.4]) })
    );
}

#[test]
fn rebases_large_origin_once_before_f32_mapping() {
    let scene = importer()
        .import_file(&fixture("large-origin.jt"), "large-jt")
        .unwrap();

    let origin = scene.asset_extras["ice3"]["jt"]["sourceOrigin"]
        .as_array()
        .unwrap();
    assert!(origin[0].as_f64().unwrap() > 1_000_000.0);
    assert_eq!(origin[1], 0.0);
    assert!(origin[2].as_f64().unwrap() > 400_000.0);

    let root = scene
        .nodes
        .iter()
        .find(|node| node.name == "TreatmentLine")
        .unwrap();
    assert!(root.matrix[12].abs() < 20.0);
    assert!(root.matrix[14].abs() < 20.0);
    assert_world_bounds_near_zero(&scene);

    assert_eq!(
        scene.asset_extras["ice3"]["coordinateSystem"],
        serde_json::json!({
            "handedness": "right",
            "upAxis": "y",
            "linearUnit": "meter"
        })
    );
    assert_eq!(
        scene.asset_extras["ice3"]["jt"]["converterContractVersion"],
        "1.0"
    );
}

#[test]
fn empty_converter_geometry_is_a_stable_empty_geometry_error() {
    let error = importer()
        .import_file(&fixture("empty.jt"), "empty-jt")
        .unwrap_err();
    assert!(
        matches!(error, ImportError::EmptyGeometry),
        "unexpected empty-source error: {error:?}"
    );
    assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_EMPTY_GEOMETRY");
}

fn importer() -> JtImporter {
    JtImporter::with_converter_and_supervisor(
        fixture("converter.mjs"),
        PathBuf::from(env!("CARGO_BIN_EXE_studio-import-worker")),
        Duration::from_secs(2),
    )
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/source-import/jt")
        .join(name)
}

fn close(left: [f32; 3], right: [f32; 3]) -> bool {
    left.iter()
        .zip(right)
        .all(|(left, right)| (*left - right).abs() <= 1e-5)
}

fn assert_world_bounds_near_zero(scene: &studio_import::SourceScene) {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    let mut worlds = std::collections::BTreeMap::new();
    for node in &scene.nodes {
        let local = Mat4::from_cols_array(&node.matrix);
        let world = node
            .parent
            .as_ref()
            .and_then(|parent| worlds.get(parent).copied())
            .map_or(local, |parent| parent * local);
        worlds.insert(node.id.clone(), world);
        let Some(mesh_index) = node.mesh else {
            continue;
        };
        for primitive in &scene.meshes[mesh_index].primitives {
            for position in &primitive.positions {
                let point = world.transform_point3(Vec3::from_array(*position));
                min = min.min(point);
                max = max.max(point);
            }
        }
    }
    assert!((min.x - 0.0).abs() < 0.001, "min x was {}", min.x);
    assert!(min.z.abs() < 0.001, "min z was {}", min.z);
    assert!(max.x > 0.0);
    assert!(max.z > 0.0);
}
