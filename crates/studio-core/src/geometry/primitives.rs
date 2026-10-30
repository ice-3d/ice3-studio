use crate::math::Vec3;

use super::{Bounds, EvaluatedMesh, GeometryError, Primitive, TriangleMesh, Vertex};

pub fn evaluate_primitive(primitive: &Primitive) -> Result<EvaluatedMesh, GeometryError> {
    match primitive {
        Primitive::Box {
            width,
            height,
            depth,
        } => box_mesh(*width, *height, *depth),
        Primitive::Plane { width, depth } => plane_mesh(*width, *depth),
        Primitive::Cylinder {
            radius,
            height,
            segments,
        } => cylinder_mesh(*radius, *height, *segments),
        Primitive::Sphere {
            radius,
            segments,
            rings,
        } => sphere_mesh(*radius, *segments, *rings),
    }
}

pub fn validate_mesh(mesh: &TriangleMesh) -> Result<EvaluatedMesh, GeometryError> {
    if mesh.indices.is_empty() || !mesh.indices.len().is_multiple_of(3) {
        return Err(GeometryError::InvalidIndices);
    }
    let vertex_count = mesh.positions.len();
    if vertex_count == 0
        || mesh
            .indices
            .iter()
            .any(|index| *index as usize >= vertex_count)
        || mesh
            .positions
            .iter()
            .flatten()
            .any(|value| !value.is_finite())
    {
        return Err(GeometryError::InvalidIndices);
    }

    let mut normals = vec![[0.0_f32; 3]; vertex_count];
    for triangle in mesh.indices.as_chunks::<3>().0 {
        let [a, b, c] = [
            triangle[0] as usize,
            triangle[1] as usize,
            triangle[2] as usize,
        ];
        let normal = triangle_normal(mesh.positions[a], mesh.positions[b], mesh.positions[c]);
        for index in [a, b, c] {
            for axis in 0..3 {
                normals[index][axis] += normal[axis];
            }
        }
    }
    let vertices = mesh
        .positions
        .iter()
        .copied()
        .zip(normals)
        .map(|(position, normal)| Vertex {
            position,
            normal: normalize(normal),
        })
        .collect();
    Ok(EvaluatedMesh {
        vertices,
        indices: mesh.indices.clone(),
        bounds: bounds(&mesh.positions),
    })
}

fn box_mesh(width: f32, height: f32, depth: f32) -> Result<EvaluatedMesh, GeometryError> {
    validate_positive([width, height, depth])?;
    let [x, y, z] = [width / 2.0, height / 2.0, depth / 2.0];
    let faces = [
        (
            [[x, -y, z], [x, -y, -z], [x, y, -z], [x, y, z]],
            [1.0, 0.0, 0.0],
        ),
        (
            [[-x, -y, -z], [-x, -y, z], [-x, y, z], [-x, y, -z]],
            [-1.0, 0.0, 0.0],
        ),
        (
            [[-x, y, z], [x, y, z], [x, y, -z], [-x, y, -z]],
            [0.0, 1.0, 0.0],
        ),
        (
            [[-x, -y, -z], [x, -y, -z], [x, -y, z], [-x, -y, z]],
            [0.0, -1.0, 0.0],
        ),
        (
            [[-x, -y, z], [x, -y, z], [x, y, z], [-x, y, z]],
            [0.0, 0.0, 1.0],
        ),
        (
            [[x, -y, -z], [-x, -y, -z], [-x, y, -z], [x, y, -z]],
            [0.0, 0.0, -1.0],
        ),
    ];
    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    for (positions, normal) in faces {
        let base = vertices.len() as u32;
        vertices.extend(positions.map(|position| Vertex { position, normal }));
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    Ok(EvaluatedMesh {
        vertices,
        indices,
        bounds: Bounds {
            min: Vec3 {
                x: -x,
                y: -y,
                z: -z,
            },
            max: Vec3 { x, y, z },
        },
    })
}

fn plane_mesh(width: f32, depth: f32) -> Result<EvaluatedMesh, GeometryError> {
    validate_positive([width, depth])?;
    let [x, z] = [width / 2.0, depth / 2.0];
    Ok(EvaluatedMesh {
        vertices: vec![
            Vertex {
                position: [-x, 0.0, z],
                normal: [0.0, 1.0, 0.0],
            },
            Vertex {
                position: [x, 0.0, z],
                normal: [0.0, 1.0, 0.0],
            },
            Vertex {
                position: [x, 0.0, -z],
                normal: [0.0, 1.0, 0.0],
            },
            Vertex {
                position: [-x, 0.0, -z],
                normal: [0.0, 1.0, 0.0],
            },
        ],
        indices: vec![0, 1, 2, 0, 2, 3],
        bounds: Bounds {
            min: Vec3 {
                x: -x,
                y: 0.0,
                z: -z,
            },
            max: Vec3 { x, y: 0.0, z },
        },
    })
}

fn cylinder_mesh(radius: f32, height: f32, segments: u32) -> Result<EvaluatedMesh, GeometryError> {
    validate_positive([radius, height])?;
    if !(3..=256).contains(&segments) {
        return Err(GeometryError::InvalidSegments);
    }
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let half = height / 2.0;
    for ring in 0..=segments {
        let angle = std::f32::consts::TAU * ring as f32 / segments as f32;
        let normal = [angle.cos(), 0.0, angle.sin()];
        for y in [-half, half] {
            vertices.push(Vertex {
                position: [normal[0] * radius, y, normal[2] * radius],
                normal,
            });
        }
    }
    for ring in 0..segments {
        let base = ring * 2;
        indices.extend([base, base + 2, base + 3, base, base + 3, base + 1]);
    }
    Ok(EvaluatedMesh {
        vertices,
        indices,
        bounds: Bounds {
            min: Vec3 {
                x: -radius,
                y: -half,
                z: -radius,
            },
            max: Vec3 {
                x: radius,
                y: half,
                z: radius,
            },
        },
    })
}

fn sphere_mesh(radius: f32, segments: u32, rings: u32) -> Result<EvaluatedMesh, GeometryError> {
    validate_positive([radius])?;
    if !(3..=256).contains(&segments) || !(2..=128).contains(&rings) {
        return Err(GeometryError::InvalidSegments);
    }
    let mut vertices = Vec::new();
    for ring in 0..=rings {
        let polar = std::f32::consts::PI * ring as f32 / rings as f32;
        for segment in 0..=segments {
            let azimuth = std::f32::consts::TAU * segment as f32 / segments as f32;
            let normal = [
                polar.sin() * azimuth.cos(),
                polar.cos(),
                polar.sin() * azimuth.sin(),
            ];
            vertices.push(Vertex {
                position: normal.map(|value| value * radius),
                normal,
            });
        }
    }
    let stride = segments + 1;
    let mut indices = Vec::new();
    for ring in 0..rings {
        for segment in 0..segments {
            let a = ring * stride + segment;
            let b = a + stride;
            indices.extend([a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    Ok(EvaluatedMesh {
        vertices,
        indices,
        bounds: Bounds {
            min: Vec3 {
                x: -radius,
                y: -radius,
                z: -radius,
            },
            max: Vec3 {
                x: radius,
                y: radius,
                z: radius,
            },
        },
    })
}

fn validate_positive<const N: usize>(values: [f32; N]) -> Result<(), GeometryError> {
    if values.iter().all(|value| value.is_finite() && *value > 0.0) {
        Ok(())
    } else {
        Err(GeometryError::InvalidDimensions)
    }
}

fn triangle_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    [
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
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
