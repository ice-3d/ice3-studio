use std::collections::BTreeMap;

use glam::Vec3;
use serde_json::Value;

use crate::{AssetError, AssetResult};

const EDGE_QUANTIZATION: f32 = 100_000.0;
const DEFAULT_EDGE_COLOR: [f32; 4] = [230.0 / 255.0, 242.0 / 255.0, 1.0, 1.0];

type WeldedVertex = (i64, i64, i64);
type EdgeKey = (u32, u32);

#[derive(Debug)]
struct EdgeRecord {
    starts: [u32; 2],
    count: u32,
    normals: Vec<[f32; 3]>,
}

pub fn generate_edge_indices(
    positions: &[[f32; 3]],
    indices: &[u32],
    crease_angle_degrees: f32,
) -> AssetResult<Vec<u32>> {
    if positions.is_empty() {
        return Err(invalid_geometry("positions must not be empty"));
    }
    if indices.is_empty() || !indices.len().is_multiple_of(3) {
        return Err(invalid_geometry(
            "triangle index count must be a non-zero multiple of 3",
        ));
    }
    if !crease_angle_degrees.is_finite() {
        return Err(invalid_geometry("crease angle must be finite"));
    }
    for position in positions {
        if position.iter().any(|value| !value.is_finite()) {
            return Err(invalid_geometry("positions must be finite"));
        }
    }
    if indices
        .iter()
        .any(|index| *index as usize >= positions.len())
    {
        return Err(invalid_geometry(
            "source index is outside the position array",
        ));
    }

    let welded = weld_vertices(positions)?;
    let mut edge_lookup = BTreeMap::<EdgeKey, usize>::new();
    let mut records = Vec::<EdgeRecord>::new();

    for triangle in indices.as_chunks::<3>().0 {
        if triangle[0] == triangle[1] || triangle[1] == triangle[2] || triangle[2] == triangle[0] {
            return Err(invalid_geometry(
                "triangle contains a repeated vertex index",
            ));
        }
        let normal = face_normal(positions, *triangle);
        let welded_triangle = [
            welded[triangle[0] as usize],
            welded[triangle[1] as usize],
            welded[triangle[2] as usize],
        ];
        for edge in 0..3 {
            let first = welded_triangle[edge];
            let second = welded_triangle[(edge + 1) % 3];
            if first == second {
                continue;
            }
            let key = sorted_edge(first, second);
            let record_index = if let Some(index) = edge_lookup.get(&key) {
                *index
            } else {
                let index = records.len();
                records.push(EdgeRecord {
                    starts: [key.0, key.1],
                    count: 0,
                    normals: Vec::new(),
                });
                edge_lookup.insert(key, index);
                index
            };
            let record = &mut records[record_index];
            record.count += 1;
            if normal.length_squared() > f32::EPSILON {
                record.normals.push(normal.to_array());
            }
        }
    }

    let crease_cosine =
        (crease_angle_degrees.clamp(0.0, 180.0) * std::f32::consts::PI / 180.0).cos();
    let mut output = Vec::new();
    for record in records {
        let is_crease = record.normals.first().is_some_and(|first| {
            record
                .normals
                .iter()
                .any(|normal| dot(*first, *normal) <= crease_cosine + 1e-6)
        });
        if record.count == 1 || is_crease {
            output.extend_from_slice(&record.starts);
        }
    }
    Ok(output)
}

pub fn edge_color_from_material(extras: &Value) -> AssetResult<[f32; 4]> {
    let Some(ice3) = extras.get("ice3") else {
        return Ok(DEFAULT_EDGE_COLOR);
    };
    let ice3 = ice3
        .as_object()
        .ok_or_else(|| invalid_geometry("material ice3 extras must be an object"))?;
    let Some(value) = ice3.get("edgeColor") else {
        return Ok(DEFAULT_EDGE_COLOR);
    };
    let value = value
        .as_str()
        .ok_or_else(|| invalid_geometry("edgeColor must be a string"))?;
    parse_hex_color(value)
}

pub fn material_suppresses_edges(extras: &Value) -> bool {
    extras
        .get("ice3")
        .and_then(Value::as_object)
        .and_then(|ice3| ice3.get("edges"))
        .and_then(Value::as_bool)
        == Some(false)
}

fn weld_vertices(positions: &[[f32; 3]]) -> AssetResult<Vec<u32>> {
    let max_coordinate = i64::MAX as f32 / EDGE_QUANTIZATION;
    let mut representatives = BTreeMap::<WeldedVertex, u32>::new();
    let mut welded = Vec::with_capacity(positions.len());
    for (index, position) in positions.iter().enumerate() {
        if position.iter().any(|value| value.abs() > max_coordinate) {
            return Err(invalid_geometry("position is too large to quantize"));
        }
        let key = (
            (position[0] * EDGE_QUANTIZATION).round() as i64,
            (position[1] * EDGE_QUANTIZATION).round() as i64,
            (position[2] * EDGE_QUANTIZATION).round() as i64,
        );
        let representative = *representatives.entry(key).or_insert(index as u32);
        welded.push(representative);
    }
    Ok(welded)
}

fn face_normal(positions: &[[f32; 3]], triangle: [u32; 3]) -> Vec3 {
    let a = Vec3::from_array(positions[triangle[0] as usize]);
    let b = Vec3::from_array(positions[triangle[1] as usize]);
    let c = Vec3::from_array(positions[triangle[2] as usize]);
    let normal = (b - a).cross(c - a);
    if normal.length_squared() <= f32::EPSILON {
        Vec3::ZERO
    } else {
        normal.normalize()
    }
}

fn sorted_edge(first: u32, second: u32) -> EdgeKey {
    if first <= second {
        (first, second)
    } else {
        (second, first)
    }
}

fn dot(left: [f32; 3], right: [f32; 3]) -> f32 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn parse_hex_color(value: &str) -> AssetResult<[f32; 4]> {
    let hex = value
        .strip_prefix('#')
        .ok_or_else(|| invalid_geometry("edgeColor must start with #"))?;
    if hex.len() != 6 && hex.len() != 8 {
        return Err(invalid_geometry(
            "edgeColor must contain 6 or 8 hexadecimal digits",
        ));
    }
    let bytes = hex.as_bytes();
    if !bytes.iter().all(u8::is_ascii_hexdigit) {
        return Err(invalid_geometry(
            "edgeColor contains an invalid hexadecimal digit",
        ));
    }
    let channel = |index: usize| {
        let pair = std::str::from_utf8(&bytes[index..index + 2])
            .map_err(|_| invalid_geometry("edgeColor contains an invalid hexadecimal digit"))?;
        u8::from_str_radix(pair, 16)
            .map(|value| value as f32 / 255.0)
            .map_err(|_| invalid_geometry("edgeColor contains an invalid hexadecimal digit"))
    };
    Ok([
        channel(0)?,
        channel(2)?,
        channel(4)?,
        if hex.len() == 8 { channel(6)? } else { 1.0 },
    ])
}

fn invalid_geometry(message: impl Into<String>) -> AssetError {
    AssetError::InvalidGeometry(message.into())
}
