//! Mesh interchange export: binary STL and Wavefront OBJ.
//!
//! Both formats are unitless; Studio exports coordinates in **millimetres**
//! (consistent with the STEP exporter) after baking node world transforms.

use std::fmt::Write as _;

use glam::Vec3;

use crate::export::step::{buckets_from_scene, StepExportError, StepMeshBucket};
use crate::large_scene::evaluate::EvaluatedScene;

/// Binary STL: 80-byte header, u32 triangle count, then 50 bytes per triangle.
pub fn export_binary_stl(scene: &EvaluatedScene) -> Result<Vec<u8>, StepExportError> {
    let buckets = buckets_from_scene(scene);
    if buckets.is_empty() {
        return Err(StepExportError::Empty);
    }
    Ok(write_binary_stl(&buckets))
}

pub fn write_binary_stl(buckets: &[StepMeshBucket]) -> Vec<u8> {
    let triangle_count: u32 = buckets
        .iter()
        .map(|bucket| bucket.triangles.len() as u32)
        .sum();
    let mut out = Vec::with_capacity(84 + triangle_count as usize * 50);
    let mut header = [0u8; 80];
    let label = b"ICE3 Studio binary STL (millimetres)";
    header[..label.len()].copy_from_slice(label);
    out.extend_from_slice(&header);
    out.extend_from_slice(&triangle_count.to_le_bytes());

    for bucket in buckets {
        for triangle in &bucket.triangles {
            let a = to_vec3(bucket.positions[triangle[0] as usize]);
            let b = to_vec3(bucket.positions[triangle[1] as usize]);
            let c = to_vec3(bucket.positions[triangle[2] as usize]);
            let normal = (b - a).cross(c - a).normalize_or_zero();
            for value in [normal.x, normal.y, normal.z] {
                out.extend_from_slice(&value.to_le_bytes());
            }
            for point in [a, b, c] {
                for value in [point.x, point.y, point.z] {
                    out.extend_from_slice(&value.to_le_bytes());
                }
            }
            out.extend_from_slice(&0u16.to_le_bytes());
        }
    }
    out
}

pub fn export_obj(scene: &EvaluatedScene) -> Result<String, StepExportError> {
    let buckets = buckets_from_scene(scene);
    if buckets.is_empty() {
        return Err(StepExportError::Empty);
    }
    Ok(write_obj(&buckets))
}

pub fn write_obj(buckets: &[StepMeshBucket]) -> String {
    let mut out = String::new();
    out.push_str("# ICE3 Studio OBJ export (millimetres)\n");
    let mut offset: usize = 0;
    for bucket in buckets {
        if bucket.positions.is_empty() || bucket.triangles.is_empty() {
            continue;
        }
        let _ = writeln!(out, "o {}", sanitize_obj_name(&bucket.name));
        for position in &bucket.positions {
            let _ = writeln!(
                out,
                "v {:.6} {:.6} {:.6}",
                position[0], position[1], position[2]
            );
        }
        for triangle in &bucket.triangles {
            // OBJ face indices are 1-based and relative to the whole file.
            let _ = writeln!(
                out,
                "f {} {} {}",
                offset + triangle[0] as usize + 1,
                offset + triangle[1] as usize + 1,
                offset + triangle[2] as usize + 1
            );
        }
        offset += bucket.positions.len();
    }
    out
}

fn to_vec3(position: [f32; 3]) -> Vec3 {
    Vec3::new(position[0], position[1], position[2])
}

fn sanitize_obj_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for character in name.chars() {
        if character.is_whitespace() {
            out.push('_');
        } else {
            out.push(character);
        }
    }
    if out.is_empty() {
        out.push_str("part");
    }
    out
}
