//! DXF export structure: one `3DFACE` per triangle plus a minimal header.

use std::collections::BTreeMap;

use studio_core::export::dxf::{export_dxf, write_dxf};
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
        [0, 1, 3], [0, 3, 2],
        [4, 6, 7], [4, 7, 5],
        [0, 4, 5], [0, 5, 1],
        [2, 3, 7], [2, 7, 6],
        [0, 2, 6], [0, 6, 4],
        [1, 5, 7], [1, 7, 3],
    ];
    StepMeshBucket { name: "cube".to_string(), positions, triangles }
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
fn dxf_emits_one_3dface_per_triangle() {
    let dxf = write_dxf(&[cube()]);
    assert!(dxf.starts_with("0\nSECTION"));
    assert!(dxf.contains("$ACADVER"));
    assert_eq!(dxf.lines().filter(|line| *line == "3DFACE").count(), 12);
    // 12 faces × (0/8 + 4 corners × 3 coordinate pairs) = 12 × 26 group pairs.
    assert_eq!(dxf.lines().filter(|line| *line == "ICE3").count(), 12);
    assert!(dxf.trim_end().ends_with("EOF"));
}

#[test]
fn empty_scene_is_rejected() {
    assert!(matches!(export_dxf(&empty_scene()), Err(StepExportError::Empty)));
}
