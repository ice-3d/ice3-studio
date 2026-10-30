use crate::math::Vec3;

use super::{Bounds, EvaluatedMesh, GeometryError, Vertex};

pub fn evaluate_sweep(
    points: &[[f32; 3]],
    radius: f32,
    radial_segments: u32,
) -> Result<EvaluatedMesh, GeometryError> {
    if points.len() < 2
        || !(radius.is_finite() && radius > 0.0)
        || !(3..=128).contains(&radial_segments)
        || points.iter().flatten().any(|value| !value.is_finite())
    {
        return Err(GeometryError::InvalidSweep);
    }
    for pair in points.windows(2) {
        if distance(pair[0], pair[1]) <= f32::EPSILON {
            return Err(GeometryError::InvalidSweep);
        }
    }

    let mut vertices = Vec::with_capacity(points.len() * radial_segments as usize);
    let mut indices = Vec::new();
    for (index, point) in points.iter().enumerate() {
        let tangent = tangent_at(points, index);
        let reference = if tangent[1].abs() > 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let side = normalize(cross(tangent, reference));
        let up = normalize(cross(side, tangent));
        for segment in 0..radial_segments {
            let angle = std::f32::consts::TAU * segment as f32 / radial_segments as f32;
            let normal = normalize([
                side[0] * angle.cos() + up[0] * angle.sin(),
                side[1] * angle.cos() + up[1] * angle.sin(),
                side[2] * angle.cos() + up[2] * angle.sin(),
            ]);
            vertices.push(Vertex {
                position: [
                    point[0] + normal[0] * radius,
                    point[1] + normal[1] * radius,
                    point[2] + normal[2] * radius,
                ],
                normal,
            });
        }
    }
    for ring in 0..points.len() - 1 {
        for segment in 0..radial_segments {
            let next = (segment + 1) % radial_segments;
            let a = ring as u32 * radial_segments + segment;
            let b = ring as u32 * radial_segments + next;
            let c = (ring + 1) as u32 * radial_segments + segment;
            let d = (ring + 1) as u32 * radial_segments + next;
            indices.extend([a, c, b, b, c, d]);
        }
    }
    cap(&mut indices, 0, radial_segments, false);
    cap(
        &mut indices,
        (points.len() - 1) as u32 * radial_segments,
        radial_segments,
        true,
    );
    let positions: Vec<_> = vertices.iter().map(|vertex| vertex.position).collect();
    Ok(EvaluatedMesh {
        vertices,
        indices,
        bounds: bounds(&positions),
    })
}

fn cap(indices: &mut Vec<u32>, base: u32, segments: u32, reverse: bool) {
    for segment in 1..segments - 1 {
        if reverse {
            indices.extend([base, base + segment + 1, base + segment]);
        } else {
            indices.extend([base, base + segment, base + segment + 1]);
        }
    }
}

fn tangent_at(points: &[[f32; 3]], index: usize) -> [f32; 3] {
    let previous = points[index.saturating_sub(1)];
    let next = points[(index + 1).min(points.len() - 1)];
    normalize([
        next[0] - previous[0],
        next[1] - previous[1],
        next[2] - previous[2],
    ])
}

fn distance(left: [f32; 3], right: [f32; 3]) -> f32 {
    let delta = [right[0] - left[0], right[1] - left[1], right[2] - left[2]];
    (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt()
}

fn cross(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

fn normalize(value: [f32; 3]) -> [f32; 3] {
    let length = (value[0] * value[0] + value[1] * value[1] + value[2] * value[2]).sqrt();
    if length <= f32::EPSILON {
        [0.0, 1.0, 0.0]
    } else {
        value.map(|component| component / length)
    }
}

fn bounds(positions: &[[f32; 3]]) -> Bounds {
    let mut minimum = [f32::INFINITY; 3];
    let mut maximum = [f32::NEG_INFINITY; 3];
    for position in positions {
        for axis in 0..3 {
            minimum[axis] = minimum[axis].min(position[axis]);
            maximum[axis] = maximum[axis].max(position[axis]);
        }
    }
    Bounds {
        min: Vec3 {
            x: minimum[0],
            y: minimum[1],
            z: minimum[2],
        },
        max: Vec3 {
            x: maximum[0],
            y: maximum[1],
            z: maximum[2],
        },
    }
}
