//! Rounded and bevelled boxes.
//!
//! A rounded box is the Minkowski sum of an inner box `(w-2r, h-2r, d-2r)` and a
//! ball of radius `r`. It is built from three kinds of patches:
//!
//! * 6 flat faces, inset by `r` on the two axes they span;
//! * 12 quarter-cylinder edge strips;
//! * 8 spherical corner octants.
//!
//! `segments = 1` collapses every arc to a single chord, which turns the result
//! into a bevelled (chamfered) box of bevel width `r`. A larger segment count
//! approximates the round shape; the volume converges to
//! `abc + 2(ab+bc+ca)r + π(a+b+c)r² + 4/3·πr³` with `a = w-2r` etc.
//!
//! Patches are emitted with their own vertices so the surface keeps a crisp
//! crease between a face and its neighbouring bevel, and stays smooth inside a
//! curved patch.

use super::{GeometryError, TriangleMesh};

/// Upper bound on the arc subdivisions, to keep triangle counts predictable.
pub const MAX_ROUNDED_SEGMENTS: u32 = 32;

/// Build a rounded box centred on the origin.
pub fn rounded_box(
    width: f32,
    height: f32,
    depth: f32,
    radius: f32,
    segments: u32,
) -> Result<TriangleMesh, GeometryError> {
    let dimensions = [width, height, depth];
    if dimensions
        .iter()
        .any(|value| !value.is_finite() || *value <= 0.0)
        || !radius.is_finite()
        || radius <= 0.0
    {
        return Err(GeometryError::InvalidDimensions);
    }
    let half = [width / 2.0, height / 2.0, depth / 2.0];
    let limit = half[0].min(half[1]).min(half[2]);
    if radius >= limit {
        return Err(GeometryError::InvalidDimensions);
    }
    let inner = [half[0] - radius, half[1] - radius, half[2] - radius];
    let steps = segments.clamp(1, MAX_ROUNDED_SEGMENTS) as usize;

    let mut builder = Builder::default();
    // Faces: one quad per axis and sign, spanning the inner rectangle.
    for axis in 0..3 {
        for sign in [1.0_f32, -1.0] {
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            let mut grid = Vec::with_capacity(2);
            for su in [-1.0_f32, 1.0] {
                let mut row = Vec::with_capacity(2);
                for sv in [-1.0_f32, 1.0] {
                    let mut point = [0.0; 3];
                    point[axis] = sign * half[axis];
                    point[u] = su * inner[u];
                    point[v] = sv * inner[v];
                    row.push(point);
                }
                grid.push(row);
            }
            builder.push_grid(&grid);
        }
    }
    // Edges: quarter cylinder around the axis that the edge runs along.
    for along in 0..3 {
        let (u, v) = ((along + 1) % 3, (along + 2) % 3);
        for sign_u in [1.0_f32, -1.0] {
            for sign_v in [1.0_f32, -1.0] {
                let mut grid = Vec::with_capacity(steps + 1);
                for step in 0..=steps {
                    let angle = std::f32::consts::FRAC_PI_2 * step as f32 / steps as f32;
                    let mut row = Vec::with_capacity(2);
                    for end in [-1.0_f32, 1.0] {
                        let mut point = [0.0; 3];
                        point[u] = sign_u * (inner[u] + radius * angle.cos());
                        point[v] = sign_v * (inner[v] + radius * angle.sin());
                        point[along] = end * inner[along];
                        row.push(point);
                    }
                    grid.push(row);
                }
                builder.push_grid(&grid);
            }
        }
    }
    // Corners: spherical octant, emitted as a fan from the pole plus rings.
    for sign_x in [1.0_f32, -1.0] {
        for sign_y in [1.0_f32, -1.0] {
            for sign_z in [1.0_f32, -1.0] {
                let signs = [sign_x, sign_y, sign_z];
                let centre = [
                    signs[0] * inner[0],
                    signs[1] * inner[1],
                    signs[2] * inner[2],
                ];
                let mut rings: Vec<Vec<[f32; 3]>> = Vec::with_capacity(steps);
                for ring in 1..=steps {
                    let theta = std::f32::consts::FRAC_PI_2 * ring as f32 / steps as f32;
                    let mut row = Vec::with_capacity(steps + 1);
                    for step in 0..=steps {
                        let phi = std::f32::consts::FRAC_PI_2 * step as f32 / steps as f32;
                        let direction = [
                            signs[0] * theta.sin() * phi.cos(),
                            signs[1] * theta.sin() * phi.sin(),
                            signs[2] * theta.cos(),
                        ];
                        row.push([
                            centre[0] + radius * direction[0],
                            centre[1] + radius * direction[1],
                            centre[2] + radius * direction[2],
                        ]);
                    }
                    rings.push(row);
                }
                builder.push_octant(&centre, rings);
            }
        }
    }
    builder.finish()
}

#[derive(Default)]
struct Builder {
    positions: Vec<[f32; 3]>,
    indices: Vec<u32>,
}

impl Builder {
    /// Append one rectangular patch, orienting it away from the origin.
    fn push_grid(&mut self, grid: &[Vec<[f32; 3]>]) {
        let base = self.positions.len() as u32;
        let rows = grid.len();
        let columns = grid[0].len();
        for row in grid {
            self.positions.extend_from_slice(row);
        }
        let mut local = Vec::new();
        for row in 0..rows - 1 {
            for column in 0..columns - 1 {
                let a = base + (row * columns + column) as u32;
                let b = base + (row * columns + column + 1) as u32;
                let c = base + ((row + 1) * columns + column + 1) as u32;
                let d = base + ((row + 1) * columns + column) as u32;
                local.extend([a, b, c, a, c, d]);
            }
        }
        self.push_oriented(local);
    }

    /// Fan the pole of a spherical octant out to the first ring, then bridge the
    /// rest as quads so the pole does not emit degenerate triangles.
    fn push_octant(&mut self, pole: &[f32; 3], rings: Vec<Vec<[f32; 3]>>) {
        let pole = {
            let index = self.positions.len() as u32;
            self.positions.push(*pole);
            index
        };
        let mut local = Vec::new();
        let mut first_ring = 0u32;
        for (index, ring) in rings.iter().enumerate() {
            let base = self.positions.len() as u32;
            self.positions.extend_from_slice(ring);
            let columns = ring.len();
            if index == 0 {
                first_ring = base;
            } else {
                let previous = base - columns as u32;
                for column in 0..columns - 1 {
                    let a = previous + column as u32;
                    let b = previous + column as u32 + 1;
                    let c = base + column as u32 + 1;
                    let d = base + column as u32;
                    local.extend([a, b, c, a, c, d]);
                }
            }
        }
        let columns = rings[0].len();
        for column in 0..columns - 1 {
            local.extend([
                pole,
                first_ring + column as u32,
                first_ring + column as u32 + 1,
            ]);
        }
        self.push_oriented(local);
    }

    /// Keep the winding that makes the patch contribute positive volume about
    /// the origin, which is exactly "normals point outward" for a convex body.
    fn push_oriented(&mut self, mut triangles: Vec<u32>) {
        if self.volume_contribution(&triangles) < 0.0 {
            for triangle in triangles.as_chunks_mut::<3>().0 {
                triangle.swap(1, 2);
            }
        }
        self.indices.extend(triangles);
    }

    fn volume_contribution(&self, triangles: &[u32]) -> f32 {
        let mut total = 0.0;
        for triangle in triangles.as_chunks::<3>().0 {
            let a = self.positions[triangle[0] as usize];
            let b = self.positions[triangle[1] as usize];
            let c = self.positions[triangle[2] as usize];
            total += (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                + a[2] * (b[0] * c[1] - b[1] * c[0]))
                / 6.0;
        }
        total
    }

    fn finish(self) -> Result<TriangleMesh, GeometryError> {
        if self.indices.is_empty() {
            return Err(GeometryError::InvalidIndices);
        }
        Ok(TriangleMesh {
            positions: self.positions,
            indices: self.indices,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signed_volume(mesh: &TriangleMesh) -> f32 {
        let mut total = 0.0;
        for triangle in mesh.indices.as_chunks::<3>().0 {
            let a = mesh.positions[triangle[0] as usize];
            let b = mesh.positions[triangle[1] as usize];
            let c = mesh.positions[triangle[2] as usize];
            total += (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                + a[2] * (b[0] * c[1] - b[1] * c[0]))
                / 6.0;
        }
        total
    }

    fn analytic_volume(w: f32, h: f32, d: f32, r: f32) -> f32 {
        let (a, b, c) = (w - 2.0 * r, h - 2.0 * r, d - 2.0 * r);
        a * b * c
            + 2.0 * (a * b + b * c + c * a) * r
            + std::f32::consts::PI * (a + b + c) * r * r
            + 4.0 / 3.0 * std::f32::consts::PI * r * r * r
    }

    #[test]
    fn rounded_box_volume_converges_to_the_analytic_value() {
        let mesh = rounded_box(4.0, 2.0, 1.0, 0.2, 16).expect("mesh");
        let volume = signed_volume(&mesh);
        let expected = analytic_volume(4.0, 2.0, 1.0, 0.2);
        assert!(
            (volume - expected).abs() / expected < 0.01,
            "volume {volume} vs {expected}"
        );
    }

    #[test]
    fn bevel_is_smaller_than_the_box_but_bigger_than_the_inner_box() {
        let mesh = rounded_box(2.0, 2.0, 2.0, 0.25, 1).expect("mesh");
        let volume = signed_volume(&mesh);
        assert!(volume > 1.5 * 1.5 * 1.5, "volume {volume}");
        assert!(volume < 8.0, "volume {volume}");
    }

    #[test]
    fn mesh_stays_inside_the_original_box() {
        let mesh = rounded_box(3.0, 2.0, 1.0, 0.1, 4).expect("mesh");
        for position in &mesh.positions {
            assert!(position[0].abs() <= 1.5 + 1e-4);
            assert!(position[1].abs() <= 1.0 + 1e-4);
            assert!(position[2].abs() <= 0.5 + 1e-4);
        }
    }

    #[test]
    fn zero_radius_and_oversized_radius_are_rejected() {
        assert!(rounded_box(1.0, 1.0, 1.0, 0.0, 4).is_err());
        assert!(rounded_box(1.0, 1.0, 1.0, 0.5, 4).is_err());
        assert!(rounded_box(0.0, 1.0, 1.0, 0.1, 4).is_err());
    }
}
