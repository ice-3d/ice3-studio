//! ASCII DXF export: one `3DFACE` per triangle (world-baked, millimetres).
//!
//! Minimal R12-compatible structure: HEADER (`$ACADVER = AC1015`) + ENTITIES.
//! `3DFACE` is a four-corner face; a triangle repeats its third corner.

use std::fmt::Write as _;

use crate::export::step::{buckets_from_scene, StepExportError, StepMeshBucket};
use crate::large_scene::evaluate::EvaluatedScene;

pub fn export_dxf(scene: &EvaluatedScene) -> Result<String, StepExportError> {
    let buckets = buckets_from_scene(scene);
    if buckets.is_empty() {
        return Err(StepExportError::Empty);
    }
    Ok(write_dxf(&buckets))
}

pub fn write_dxf(buckets: &[StepMeshBucket]) -> String {
    let mut out = String::new();
    pair(&mut out, 0, "SECTION");
    pair(&mut out, 2, "HEADER");
    pair(&mut out, 9, "$ACADVER");
    pair(&mut out, 1, "AC1015");
    pair(&mut out, 0, "ENDSEC");

    pair(&mut out, 0, "SECTION");
    pair(&mut out, 2, "ENTITIES");
    for bucket in buckets {
        for triangle in &bucket.triangles {
            let a = bucket.positions[triangle[0] as usize];
            let b = bucket.positions[triangle[1] as usize];
            let c = bucket.positions[triangle[2] as usize];
            pair(&mut out, 0, "3DFACE");
            pair(&mut out, 8, "ICE3");
            write_corner(&mut out, 10, a);
            write_corner(&mut out, 11, b);
            write_corner(&mut out, 12, c);
            write_corner(&mut out, 13, c);
        }
    }
    pair(&mut out, 0, "ENDSEC");
    pair(&mut out, 0, "EOF");
    out
}

fn write_corner(out: &mut String, base: i32, position: [f32; 3]) {
    pair(out, base, &format!("{:.6}", position[0]));
    pair(out, base + 10, &format!("{:.6}", position[1]));
    pair(out, base + 20, &format!("{:.6}", position[2]));
}

fn pair(out: &mut String, code: i32, value: &str) {
    let _ = writeln!(out, "{code}");
    let _ = writeln!(out, "{value}");
}
