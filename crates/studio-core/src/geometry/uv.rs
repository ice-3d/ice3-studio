//! Texture coordinates for meshes that were never authored with them.
//!
//! Studio generates geometry parametrically, so a mesh carries positions and
//! normals but no UV attribute. Rather than baking a per-primitive layout into
//! every builder (which would be wrong the moment a boolean or an imported mesh
//! shows up), UVs are derived from the surface itself with a box projection:
//! each vertex is projected along the dominant axis of its normal.
//!
//! The result is in metres, so one UV unit is one metre of surface and texel
//! density stays uniform across parts of different size. Tiling is a material
//! concern (`KHR_texture_transform` in glTF), which keeps geometry independent
//! of which texture a part happens to wear.

use crate::geometry::EvaluatedMesh;

/// Box-projected UVs in metres, one pair per vertex.
///
/// Vertices on a flat face share a normal, so the dominant axis is stable and
/// the projection is seamless inside a face. Edges between faces keep the same
/// UV as the face they belong to because builders emit per-face vertices; where
/// a mesh does share vertices across a crease the averaged normal still picks a
/// sensible side.
pub fn box_projected_uvs(mesh: &EvaluatedMesh) -> Vec<[f32; 2]> {
    mesh.vertices
        .iter()
        .map(|vertex| project(vertex.position, vertex.normal))
        .collect()
}

fn project(position: [f32; 3], normal: [f32; 3]) -> [f32; 2] {
    let [x, y, z] = position;
    let (ax, ay, az) = (normal[0].abs(), normal[1].abs(), normal[2].abs());
    if ax + ay + az < 1e-6 {
        // A degenerate normal says nothing about orientation; the XY plane is the
        // least surprising default and keeps the vertex inside the atlas.
        return [x, y];
    }
    if ax >= ay && ax >= az {
        // Project onto the YZ plane; flip U on the −X face so the texture is not
        // mirrored when looked at from outside.
        if normal[0] >= 0.0 { [-z, y] } else { [z, y] }
    } else if ay >= az {
        if normal[1] >= 0.0 { [x, z] } else { [x, -z] }
    } else if normal[2] >= 0.0 {
        [x, y]
    } else {
        [-x, y]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Bounds, Vertex};
    use crate::math::Vec3;

    fn mesh(vertices: Vec<Vertex>) -> EvaluatedMesh {
        EvaluatedMesh {
            vertices,
            indices: Vec::new(),
            bounds: Bounds {
                min: Vec3::default(),
                max: Vec3::default(),
            },
        }
    }

    fn vertex(position: [f32; 3], normal: [f32; 3]) -> Vertex {
        Vertex { position, normal }
    }

    #[test]
    fn each_face_uses_its_own_plane() {
        let mesh = mesh(vec![
            vertex([1.0, 2.0, 3.0], [1.0, 0.0, 0.0]),
            vertex([1.0, 2.0, 3.0], [0.0, 1.0, 0.0]),
            vertex([1.0, 2.0, 3.0], [0.0, 0.0, 1.0]),
        ]);
        let uvs = box_projected_uvs(&mesh);
        assert_eq!(uvs[0], [-3.0, 2.0], "x face uses (z, y)");
        assert_eq!(uvs[1], [1.0, 3.0], "y face uses (x, z)");
        assert_eq!(uvs[2], [1.0, 2.0], "z face uses (x, y)");
    }

    #[test]
    fn opposite_faces_are_not_mirrored() {
        let mesh = mesh(vec![
            vertex([1.0, 2.0, 3.0], [-1.0, 0.0, 0.0]),
            vertex([1.0, 2.0, 3.0], [0.0, -1.0, 0.0]),
            vertex([1.0, 2.0, 3.0], [0.0, 0.0, -1.0]),
        ]);
        let uvs = box_projected_uvs(&mesh);
        assert_eq!(uvs[0], [3.0, 2.0]);
        assert_eq!(uvs[1], [1.0, -3.0]);
        assert_eq!(uvs[2], [-1.0, 2.0]);
    }

    #[test]
    fn dominant_axis_wins_on_a_tie() {
        let mesh = mesh(vec![vertex([1.0, 1.0, 1.0], [0.7, -0.7, 0.1])]);
        let uvs = box_projected_uvs(&mesh);
        assert_eq!(uvs[0], [-1.0, 1.0], "x ties with y, x wins");
    }

    #[test]
    fn a_degenerate_normal_falls_back_to_the_top_face() {
        let mesh = mesh(vec![vertex([1.0, 1.0, 1.0], [0.0, 0.0, 0.0])]);
        let uvs = box_projected_uvs(&mesh);
        assert_eq!(uvs[0], [1.0, 1.0], "zero normal projects like +z");
    }
}
