//! STL/OBJ export tests: verify structure and that empty scenes are rejected.

use std::collections::BTreeMap;

use studio_core::export::mesh::{export_binary_stl, export_obj, write_binary_stl, write_obj};
use studio_core::export::step::{StepExportError, StepMeshBucket};
use studio_core::large_scene::evaluate::{EvaluatedScene, LargeSceneStats};

fn cube() -> StepMeshBucket {
    let mut positions = Vec::new();
    for z in 0..2 {
        for y in 0..2 {
            for x in 0..2 {
                positions.push([x as f32, y as f32, z as f32]);
            }
        }
    }
    let triangles = vec![
        [0, 1, 3],
        [0, 3, 2],
        [4, 6, 7],
        [4, 7, 5],
        [0, 4, 5],
        [0, 5, 1],
        [2, 3, 7],
        [2, 7, 6],
        [0, 2, 6],
        [0, 6, 4],
        [1, 5, 7],
        [1, 7, 3],
    ];
    StepMeshBucket {
        name: "cube".to_string(),
        positions,
        triangles,
    }
}

fn empty_scene() -> EvaluatedScene {
    EvaluatedScene {
        nodes: Vec::new(),
        meshes: BTreeMap::new(),
        resident_geometry_ids: BTreeMap::new(),
        stats: LargeSceneStats::default(),
    }
}

#[test]
fn binary_stl_header_and_triangle_count() {
    let bytes = write_binary_stl(&[cube()]);
    assert_eq!(bytes.len(), 84 + 12 * 50, "80B header + u32 + 12 * 50B");
    let count = u32::from_le_bytes(bytes[80..84].try_into().unwrap());
    assert_eq!(count, 12);

    // The first triangle normal must be a unit-ish vector (z = -1 face is first).
    let nx = f32::from_le_bytes(bytes[84..88].try_into().unwrap());
    let ny = f32::from_le_bytes(bytes[88..92].try_into().unwrap());
    let nz = f32::from_le_bytes(bytes[92..96].try_into().unwrap());
    let length = (nx * nx + ny * ny + nz * nz).sqrt();
    assert!(
        (length - 1.0).abs() < 1e-3,
        "normal is unit length, got {length}"
    );
}

#[test]
fn obj_emits_one_object_vertices_and_faces() {
    let obj = write_obj(&[cube()]);
    assert_eq!(obj.lines().filter(|line| line.starts_with("v ")).count(), 8);
    assert_eq!(
        obj.lines().filter(|line| line.starts_with("f ")).count(),
        12
    );
    assert_eq!(obj.lines().filter(|line| line.starts_with("o ")).count(), 1);
    assert!(obj.contains("o cube"));
}

#[test]
fn empty_scene_is_rejected() {
    let scene = empty_scene();
    assert!(matches!(
        export_binary_stl(&scene),
        Err(StepExportError::Empty)
    ));
    assert!(matches!(export_obj(&scene), Err(StepExportError::Empty)));
}
