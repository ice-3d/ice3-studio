#![cfg(feature = "step")]

use std::path::PathBuf;

use glam::{Mat4, Vec3};
use studio_import::{SourceFormat, StepImporter};

#[test]
fn imports_ap242_hierarchy_and_geometry() {
    let scene = StepImporter::new()
        .import_file(&fixture("step/synthetic-skid.step"), "step-fixture")
        .unwrap();
    let names = scene
        .nodes
        .iter()
        .map(|node| node.name.as_str())
        .collect::<Vec<_>>();
    assert!(names.contains(&"SyntheticSkid"));
    assert!(names.contains(&"PrimaryTank"));
    assert!(names.contains(&"TransferPump"));
    assert_eq!(scene.meshes.len(), 2);
    assert_eq!(scene.source_format, SourceFormat::Step);
    assert_eq!(
        scene
            .nodes
            .iter()
            .filter(|node| node.mesh.is_some())
            .count(),
        2
    );
    assert!(
        scene
            .nodes
            .iter()
            .all(|node| node.id.starts_with("step-fixture/step/node/"))
    );
}

#[test]
fn preserves_per_face_colors() {
    let scene = StepImporter::new()
        .import_file(&fixture("step/synthetic-skid.step"), "step-fixture")
        .unwrap();
    let colors = scene
        .materials
        .iter()
        .map(|material| material.base_color)
        .collect::<Vec<_>>();
    assert!(colors.iter().any(|color| close(*color, [0.16, 0.48, 0.72])));
    assert!(colors.iter().any(|color| close(*color, [0.92, 0.56, 0.12])));
}

#[test]
fn applies_millimeter_units_and_shared_origin_once() {
    let scene = StepImporter::new()
        .import_file(&fixture("step/synthetic-skid.step"), "step-fixture")
        .unwrap();
    let extras = &scene.asset_extras["ice3"]["step"];
    assert_eq!(extras["schema"], "ap242");
    assert_eq!(extras["unitAssumption"], "meter");
    assert_eq!(extras["sourceOrigin"].as_array().unwrap()[1], 0.0);
    // The legacy bounds are [-0.7, 0.2598] on Z; after the shared origin
    // shift the native extent is the same 0.9598 m span starting at zero.
    assert_world_bounds(&scene, [0.0, 0.0, 0.0], [2.01, 0.9, 0.95987]);
}

fn fixture(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../ice3-toolchain/tests/fixtures")
        .join(relative)
}

fn close(left: [f32; 3], right: [f32; 3]) -> bool {
    left.iter()
        .zip(right)
        .all(|(left, right)| (*left - right).abs() <= 1e-5)
}

fn assert_world_bounds(
    scene: &studio_import::SourceScene,
    expected_min: [f32; 3],
    expected_max: [f32; 3],
) {
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
    for axis in 0..3 {
        let extent = (max[axis] - min[axis]).abs();
        let tolerance = 0.0001_f32.max(extent * 0.00001);
        assert!(
            (min[axis] - expected_min[axis]).abs() <= tolerance,
            "min axis {axis}: {} != {} (bounds {min:?}..{max:?})",
            min[axis],
            expected_min[axis]
        );
        assert!(
            (max[axis] - expected_max[axis]).abs() <= tolerance,
            "max axis {axis}: {} != {} (bounds {min:?}..{max:?})",
            max[axis],
            expected_max[axis]
        );
    }
}
