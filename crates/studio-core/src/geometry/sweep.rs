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
    let mut previous_tangent = unit(subtract(points[1], points[0]));
    let initial_reference = if previous_tangent[1].abs() > 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let mut side = unit(cross(previous_tangent, initial_reference));
    let mut up = unit(cross(side, previous_tangent));
    for (index, point) in points.iter().enumerate() {
        if index > 0 {
            let tangent = unit(subtract(points[index], points[index - 1]));
            let axis = cross(previous_tangent, tangent);
            let axis_length = length(axis);
            if axis_length > f32::EPSILON {
                let angle = dot(previous_tangent, tangent).clamp(-1.0, 1.0).acos();
                let axis = scale(axis, 1.0 / axis_length);
                side = rotate_around_axis(side, axis, angle);
            }
            side = unit(subtract(side, scale(tangent, dot(side, tangent))));
            up = unit(cross(side, tangent));
            previous_tangent = tangent;
        }
        for segment in 0..radial_segments {
            let angle = std::f32::consts::TAU * segment as f32 / radial_segments as f32;
            let normal = unit([
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

fn distance(left: [f32; 3], right: [f32; 3]) -> f32 {
    let delta = [right[0] - left[0], right[1] - left[1], right[2] - left[2]];
    (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt()
}

fn subtract(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

fn scale(value: [f32; 3], factor: f32) -> [f32; 3] {
    [value[0] * factor, value[1] * factor, value[2] * factor]
}

fn dot(left: [f32; 3], right: [f32; 3]) -> f32 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn length(value: [f32; 3]) -> f32 {
    dot(value, value).sqrt()
}

fn cross(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

fn unit(value: [f32; 3]) -> [f32; 3] {
    let magnitude = length(value);
    if magnitude <= f32::EPSILON {
        [1.0, 0.0, 0.0]
    } else {
        value.map(|component| component / magnitude)
    }
}

fn rotate_around_axis(value: [f32; 3], axis: [f32; 3], angle: f32) -> [f32; 3] {
    let cosine = angle.cos();
    let sine = angle.sin();
    let rotated_cross = cross(axis, value);
    let axis_dot = dot(axis, value) * (1.0 - cosine);
    [
        value[0] * cosine + rotated_cross[0] * sine + axis[0] * axis_dot,
        value[1] * cosine + rotated_cross[1] * sine + axis[1] * axis_dot,
        value[2] * cosine + rotated_cross[2] * sine + axis[2] * axis_dot,
    ]
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
