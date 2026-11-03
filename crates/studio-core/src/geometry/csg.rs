//! BSP-based constructive solid geometry: subtract one triangle mesh from
//! another. Ported from the classic csg.js algorithm (Evan Wallace, MIT).
//!
//! The cut solids are small primitives (box / cylinder / sphere); the base part
//! can be larger. Triangle counts are capped to keep the BSP build bounded.

use super::{GeometryError, TriangleMesh};

const EPSILON: f32 = 1e-5;
const MAX_BASE_TRIANGLES: usize = 60_000;
const MAX_CUT_TRIANGLES: usize = 20_000;

/// `base - cut`, returning a closed triangle mesh.
pub fn csg_subtract(
    base: &TriangleMesh,
    cut: &TriangleMesh,
) -> Result<TriangleMesh, GeometryError> {
    if base.indices.len() / 3 > MAX_BASE_TRIANGLES || cut.indices.len() / 3 > MAX_CUT_TRIANGLES {
        return Err(GeometryError::CsgTooLarge);
    }
    let mut a = Node::from_polygons(polygons_of(base));
    let mut b = Node::from_polygons(polygons_of(cut));

    a.invert();
    a.clip_to(&b);
    b.clip_to(&a);
    b.invert();
    b.clip_to(&a);
    b.invert();
    a.build(b.all_polygons());
    a.invert();

    let polygons = a.all_polygons();
    let mesh = mesh_of(&polygons);
    if mesh.indices.is_empty() {
        return Err(GeometryError::CsgFailed);
    }
    Ok(mesh)
}

#[derive(Clone, Copy)]
struct Plane {
    normal: [f32; 3],
    w: f32,
}

impl Plane {
    fn from_points(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> Option<Self> {
        let normal = cross(sub(b, a), sub(c, a));
        let length = dot(normal, normal).sqrt();
        if length <= 1e-12 || !length.is_finite() {
            return None;
        }
        let normal = [normal[0] / length, normal[1] / length, normal[2] / length];
        Some(Self {
            normal,
            w: dot(normal, a),
        })
    }

    fn flip(&mut self) {
        self.normal = [-self.normal[0], -self.normal[1], -self.normal[2]];
        self.w = -self.w;
    }

    fn split_polygon(
        &self,
        polygon: &Polygon,
        coplanar_front: &mut Vec<Polygon>,
        coplanar_back: &mut Vec<Polygon>,
        front: &mut Vec<Polygon>,
        back: &mut Vec<Polygon>,
    ) {
        let mut polygon_type = 0u8;
        let mut types = Vec::with_capacity(polygon.vertices.len());
        for vertex in &polygon.vertices {
            let distance = dot(self.normal, *vertex) - self.w;
            let side = if distance < -EPSILON {
                2
            } else if distance > EPSILON {
                1
            } else {
                0
            };
            polygon_type |= side;
            types.push(side);
        }
        match polygon_type {
            0 => {
                if dot(self.normal, polygon.plane.normal) > 0.0 {
                    coplanar_front.push(polygon.clone());
                } else {
                    coplanar_back.push(polygon.clone());
                }
            }
            1 => front.push(polygon.clone()),
            2 => back.push(polygon.clone()),
            _ => {
                let mut front_vertices: Vec<[f32; 3]> = Vec::new();
                let mut back_vertices: Vec<[f32; 3]> = Vec::new();
                let count = polygon.vertices.len();
                for index in 0..count {
                    let next = (index + 1) % count;
                    let (type_a, type_b) = (types[index], types[next]);
                    let a = polygon.vertices[index];
                    let b = polygon.vertices[next];
                    if type_a != 2 {
                        front_vertices.push(a);
                    }
                    if type_a != 1 {
                        back_vertices.push(a);
                    }
                    if (type_a | type_b) == 3 {
                        let direction = sub(b, a);
                        let denominator = dot(self.normal, direction);
                        if denominator.abs() > 1e-12 {
                            let t = (self.w - dot(self.normal, a)) / denominator;
                            let point = [
                                a[0] + t * direction[0],
                                a[1] + t * direction[1],
                                a[2] + t * direction[2],
                            ];
                            front_vertices.push(point);
                            back_vertices.push(point);
                        }
                    }
                }
                if front_vertices.len() >= 3 {
                    front.push(Polygon {
                        vertices: front_vertices,
                        plane: polygon.plane,
                    });
                }
                if back_vertices.len() >= 3 {
                    back.push(Polygon {
                        vertices: back_vertices,
                        plane: polygon.plane,
                    });
                }
            }
        }
    }
}

#[derive(Clone)]
struct Polygon {
    vertices: Vec<[f32; 3]>,
    plane: Plane,
}

struct Node {
    plane: Option<Plane>,
    front: Option<Box<Node>>,
    back: Option<Box<Node>>,
    polygons: Vec<Polygon>,
}

impl Node {
    fn new() -> Self {
        Self {
            plane: None,
            front: None,
            back: None,
            polygons: Vec::new(),
        }
    }

    fn from_polygons(polygons: Vec<Polygon>) -> Self {
        let mut node = Self::new();
        node.build(polygons);
        node
    }

    fn invert(&mut self) {
        for polygon in &mut self.polygons {
            polygon.plane.flip();
            polygon.vertices.reverse();
        }
        if let Some(plane) = &mut self.plane {
            plane.flip();
        }
        if let Some(front) = &mut self.front {
            front.invert();
        }
        if let Some(back) = &mut self.back {
            back.invert();
        }
        std::mem::swap(&mut self.front, &mut self.back);
    }

    fn clip_polygons(&self, polygons: Vec<Polygon>) -> Vec<Polygon> {
        let Some(plane) = self.plane else {
            return polygons;
        };
        let mut front = Vec::new();
        let mut back = Vec::new();
        let mut coplanar_front = Vec::new();
        let mut coplanar_back = Vec::new();
        for polygon in polygons {
            plane.split_polygon(
                &polygon,
                &mut coplanar_front,
                &mut coplanar_back,
                &mut front,
                &mut back,
            );
        }
        front.extend(coplanar_front);
        back.extend(coplanar_back);
        if let Some(node) = &self.front {
            front = node.clip_polygons(front);
        }
        if let Some(node) = &self.back {
            back = node.clip_polygons(back);
        } else {
            back.clear();
        }
        front.extend(back);
        front
    }

    fn clip_to(&mut self, other: &Node) {
        self.polygons = other.clip_polygons(std::mem::take(&mut self.polygons));
        if let Some(front) = &mut self.front {
            front.clip_to(other);
        }
        if let Some(back) = &mut self.back {
            back.clip_to(other);
        }
    }

    fn all_polygons(&self) -> Vec<Polygon> {
        let mut polygons = self.polygons.clone();
        if let Some(front) = &self.front {
            polygons.extend(front.all_polygons());
        }
        if let Some(back) = &self.back {
            polygons.extend(back.all_polygons());
        }
        polygons
    }

    fn build(&mut self, polygons: Vec<Polygon>) {
        if polygons.is_empty() {
            return;
        }
        let plane = *self.plane.get_or_insert(polygons[0].plane);
        let mut front = Vec::new();
        let mut back = Vec::new();
        let mut coplanar_front = Vec::new();
        let mut coplanar_back = Vec::new();
        for polygon in polygons {
            plane.split_polygon(
                &polygon,
                &mut coplanar_front,
                &mut coplanar_back,
                &mut front,
                &mut back,
            );
        }
        self.polygons.extend(coplanar_front);
        self.polygons.extend(coplanar_back);
        if !front.is_empty() {
            self.front
                .get_or_insert_with(|| Box::new(Node::new()))
                .build(front);
        }
        if !back.is_empty() {
            self.back
                .get_or_insert_with(|| Box::new(Node::new()))
                .build(back);
        }
    }
}

fn polygons_of(mesh: &TriangleMesh) -> Vec<Polygon> {
    let mut polygons = Vec::with_capacity(mesh.indices.len() / 3);
    for triangle in mesh.indices.as_chunks::<3>().0 {
        let a = mesh.positions[triangle[0] as usize];
        let b = mesh.positions[triangle[1] as usize];
        let c = mesh.positions[triangle[2] as usize];
        if let Some(plane) = Plane::from_points(a, b, c) {
            polygons.push(Polygon {
                vertices: vec![a, b, c],
                plane,
            });
        }
    }
    polygons
}

fn mesh_of(polygons: &[Polygon]) -> TriangleMesh {
    let mut positions = Vec::new();
    let mut indices = Vec::new();
    for polygon in polygons {
        if polygon.vertices.len() < 3 {
            continue;
        }
        for index in 1..polygon.vertices.len() - 1 {
            let base = positions.len() as u32;
            positions.push(polygon.vertices[0]);
            positions.push(polygon.vertices[index]);
            positions.push(polygon.vertices[index + 1]);
            indices.extend_from_slice(&[base, base + 1, base + 2]);
        }
    }
    TriangleMesh { positions, indices }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
