//! 2-D profile extrusion (sketch → prism) with flat-shaded triangles.

use super::{GeometryError, TriangleMesh};

/// Extrude a closed 2-D polygon (XY) along +Z by `height` (millimetres/metres,
/// same unit as the profile). Output is non-indexed per triangle so the
/// evaluator assigns each vertex its face normal (crisp edges).
pub fn extrude_profile(profile: &[[f32; 2]], height: f32) -> Result<TriangleMesh, GeometryError> {
    if profile.len() < 3 || !height.is_finite() || height <= 0.0 {
        return Err(GeometryError::InvalidProfile);
    }
    if profile.iter().flatten().any(|value| !value.is_finite()) {
        return Err(GeometryError::InvalidProfile);
    }

    // Drop consecutive duplicates (including the closing edge).
    let mut points: Vec<[f32; 2]> = Vec::with_capacity(profile.len());
    for point in profile {
        if points
            .last()
            .is_none_or(|last| distance_squared(*last, *point) > 1e-12)
        {
            points.push(*point);
        }
    }
    while points.len() > 1
        && distance_squared(points[0], *points.last().expect("non-empty")) <= 1e-12
    {
        points.pop();
    }
    if points.len() < 3 {
        return Err(GeometryError::InvalidProfile);
    }

    let area = signed_area(&points);
    if !area.is_finite() || area.abs() <= 1e-9 {
        return Err(GeometryError::InvalidProfile);
    }
    if area < 0.0 {
        points.reverse();
    }

    let triangulation = triangulate(&points);
    if triangulation.is_empty() {
        return Err(GeometryError::InvalidProfile);
    }

    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    // Top cap (+Z), counter-clockwise.
    for triangle in &triangulation {
        push_triangle(
            &mut positions,
            &mut indices,
            [points[triangle[0]][0], points[triangle[0]][1], height],
            [points[triangle[1]][0], points[triangle[1]][1], height],
            [points[triangle[2]][0], points[triangle[2]][1], height],
        );
    }
    // Bottom cap (-Z), reversed.
    for triangle in &triangulation {
        push_triangle(
            &mut positions,
            &mut indices,
            [points[triangle[0]][0], points[triangle[0]][1], 0.0],
            [points[triangle[2]][0], points[triangle[2]][1], 0.0],
            [points[triangle[1]][0], points[triangle[1]][1], 0.0],
        );
    }
    // Side walls, outward-facing for a counter-clockwise profile.
    let count = points.len();
    for index in 0..count {
        let next = (index + 1) % count;
        let a = points[index];
        let b = points[next];
        let bottom_left = [a[0], a[1], 0.0];
        let bottom_right = [b[0], b[1], 0.0];
        let top_left = [a[0], a[1], height];
        let top_right = [b[0], b[1], height];
        push_triangle(
            &mut positions,
            &mut indices,
            bottom_left,
            bottom_right,
            top_right,
        );
        push_triangle(
            &mut positions,
            &mut indices,
            bottom_left,
            top_right,
            top_left,
        );
    }

    Ok(TriangleMesh { positions, indices })
}

fn push_triangle(
    positions: &mut Vec<[f32; 3]>,
    indices: &mut Vec<u32>,
    a: [f32; 3],
    b: [f32; 3],
    c: [f32; 3],
) {
    let base = positions.len() as u32;
    positions.push(a);
    positions.push(b);
    positions.push(c);
    indices.extend_from_slice(&[base, base + 1, base + 2]);
}

fn signed_area(points: &[[f32; 2]]) -> f32 {
    let mut sum = 0.0;
    for index in 0..points.len() {
        let a = points[index];
        let b = points[(index + 1) % points.len()];
        sum += a[0] * b[1] - b[0] * a[1];
    }
    sum * 0.5
}

/// Ear-clipping triangulation of a simple counter-clockwise polygon. Returns
/// index triples into `points`. Self-intersecting input falls back to a fan.
fn triangulate(points: &[[f32; 2]]) -> Vec<[usize; 3]> {
    let count = points.len();
    if count < 3 {
        return Vec::new();
    }
    let mut remaining: Vec<usize> = (0..count).collect();
    let mut output: Vec<[usize; 3]> = Vec::with_capacity(count.saturating_sub(2));
    let mut guard = 0usize;

    while remaining.len() > 3 && guard < count * count {
        guard += 1;
        let length = remaining.len();
        let mut clipped = false;
        for position in 0..length {
            let previous = remaining[(position + length - 1) % length];
            let current = remaining[position];
            let next = remaining[(position + 1) % length];
            let a = points[previous];
            let b = points[current];
            let c = points[next];
            if cross(sub(b, a), sub(c, b)) <= 1e-12 {
                continue;
            }
            let mut contains = false;
            for &candidate in &remaining {
                if candidate == previous || candidate == current || candidate == next {
                    continue;
                }
                if point_in_triangle(points[candidate], a, b, c) {
                    contains = true;
                    break;
                }
            }
            if contains {
                continue;
            }
            output.push([previous, current, next]);
            remaining.remove(position);
            clipped = true;
            break;
        }
        if !clipped {
            break;
        }
    }

    if remaining.len() == 3 {
        output.push([remaining[0], remaining[1], remaining[2]]);
    } else if remaining.len() > 3 {
        // Degenerate / self-intersecting: fall back to a fan over what is left.
        for index in 1..remaining.len() - 1 {
            output.push([remaining[0], remaining[index], remaining[index + 1]]);
        }
    }
    output
}

fn cross(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[1] - a[1] * b[0]
}

fn sub(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn point_in_triangle(point: [f32; 2], a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> bool {
    let d1 = cross(sub(b, a), sub(point, a));
    let d2 = cross(sub(c, b), sub(point, b));
    let d3 = cross(sub(a, c), sub(point, c));
    let has_negative = d1 < -1e-12 || d2 < -1e-12 || d3 < -1e-12;
    let has_positive = d1 > 1e-12 || d2 > 1e-12 || d3 > 1e-12;
    !(has_negative && has_positive)
}

fn distance_squared(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    dx * dx + dy * dy
}
