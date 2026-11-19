//! Clash and clearance detection primitives.
//!
//! Two phases, both deterministic:
//!
//! 1. **Broad phase** — sweep and prune along X over world AABBs. Each pair is
//!    reduced to a *signed gap*: positive is the shortest separation between the
//!    two boxes, negative is the penetration depth (the smallest axis overlap),
//!    zero means the boxes touch.
//! 2. **Narrow phase** — an optional triangle-triangle overlap test for pairs
//!    whose boxes penetrate. Bounding volumes flag rotated and concave parts
//!    that are actually clear, so the caller can ask for confirmation.
//!
//! Everything here is pure and allocation-light so it can run inside the read
//! path without touching the scene graph.

use crate::geometry::Bounds;

/// Distances below this are treated as zero so that stacked parts that share a
/// face do not fluctuate between "touching" and "clear" on float noise.
pub const CLASH_EPSILON: f32 = 1e-5;

/// Signed separation between two axis-aligned boxes in metres.
///
/// Positive: the shortest gap between the two boxes. Negative: penetrating, and
/// the magnitude is the smallest axis overlap. Zero: touching within
/// [`CLASH_EPSILON`].
pub fn signed_gap(a: &Bounds, b: &Bounds) -> f32 {
    let axis = |a_min: f32, a_max: f32, b_min: f32, b_max: f32| (b_min - a_max).max(a_min - b_max);
    let dx = axis(a.min.x, a.max.x, b.min.x, b.max.x);
    let dy = axis(a.min.y, a.max.y, b.min.y, b.max.y);
    let dz = axis(a.min.z, a.max.z, b.min.z, b.max.z);
    if dx <= 0.0 && dy <= 0.0 && dz <= 0.0 {
        // Penetrating: the smallest overlap is the depth that matters.
        let depth = dx.max(dy).max(dz);
        return if depth.abs() <= CLASH_EPSILON {
            0.0
        } else {
            depth
        };
    }
    let separation = (dx.max(0.0).powi(2) + dy.max(0.0).powi(2) + dz.max(0.0).powi(2)).sqrt();
    if separation <= CLASH_EPSILON {
        0.0
    } else {
        separation
    }
}

/// True when the boxes are closer than `clearance` (or penetrate).
///
/// `clearance` is added to the box extents before the sweep, so a single pass
/// answers both "do they collide" and "is there enough room around them".
pub fn within_clearance(gap: f32, clearance: f32, include_touching: bool) -> bool {
    if gap < 0.0 {
        return true;
    }
    if gap == 0.0 {
        return include_touching || clearance > 0.0;
    }
    gap < clearance + CLASH_EPSILON
}

/// Broad phase: indices of every pair whose boxes are within `clearance`.
///
/// Sweep and prune along X with an early exit once the sorted minimum exceeds
/// the current maximum plus the clearance. Returns pairs as `(low, high)` in
/// ascending order, which keeps downstream sorting stable.
pub fn sweep_pairs(boxes: &[Bounds], clearance: f32) -> Vec<(usize, usize)> {
    let mut order: Vec<usize> = (0..boxes.len()).collect();
    order.sort_by(|left, right| {
        boxes[*left]
            .min
            .x
            .partial_cmp(&boxes[*right].min.x)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.cmp(right))
    });
    let reach = clearance.max(0.0) + CLASH_EPSILON;
    let mut pairs = Vec::new();
    for position in 0..order.len() {
        let left = order[position];
        let left_max = boxes[left].max.x + reach;
        for candidate in order.iter().skip(position + 1) {
            let right = *candidate;
            if boxes[right].min.x > left_max {
                break;
            }
            let gap = signed_gap(&boxes[left], &boxes[right]);
            if within_clearance(gap, clearance, false) {
                pairs.push(if left < right {
                    (left, right)
                } else {
                    (right, left)
                });
            }
        }
    }
    pairs.sort_unstable();
    pairs
}

/// Segment versus triangle (Möller–Trumbore). `segment` is `[from, to]`.
pub fn segment_triangle(segment: [[f32; 3]; 2], triangle: [[f32; 3]; 3]) -> bool {
    const EPSILON: f32 = 1e-9;
    let [from, to] = segment;
    let direction = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    let edge1 = sub(triangle[1], triangle[0]);
    let edge2 = sub(triangle[2], triangle[0]);
    let pvec = cross(direction, edge2);
    let det = dot(edge1, pvec);
    if det.abs() < EPSILON {
        return false;
    }
    let inverse = 1.0 / det;
    let tvec = sub(from, triangle[0]);
    let u = dot(tvec, pvec) * inverse;
    if !(-EPSILON..=1.0 + EPSILON).contains(&u) {
        return false;
    }
    let qvec = cross(tvec, edge1);
    let v = dot(direction, qvec) * inverse;
    if v < -EPSILON || u + v > 1.0 + EPSILON {
        return false;
    }
    let t = dot(edge2, qvec) * inverse;
    (-EPSILON..=1.0 + EPSILON).contains(&t)
}

/// Shortest distance between a segment and a triangle.
pub fn segment_triangle_distance(segment: [[f32; 3]; 2], triangle: [[f32; 3]; 3]) -> f32 {
    if segment_triangle(segment, triangle) {
        return 0.0;
    }
    let mut distance = f32::INFINITY;
    for point in segment {
        distance = distance.min(point_triangle_distance(point, triangle));
    }
    for vertex in triangle {
        distance = distance.min(point_segment_distance(vertex, segment));
    }
    for edge in triangle_edges(triangle) {
        distance = distance.min(segment_segment_distance(segment, edge));
    }
    distance
}

/// Shortest distance between a segment and an axis-aligned box.
pub fn segment_bounds_distance(segment: [[f32; 3]; 2], bounds: &Bounds) -> f32 {
    if segment_aabb_intersects(segment, bounds) {
        return 0.0;
    }
    let mut distance =
        point_aabb_distance(segment[0], bounds).min(point_aabb_distance(segment[1], bounds));
    let corners = bounds_corners(bounds);
    for corner in corners {
        distance = distance.min(point_segment_distance(corner, segment));
    }
    for [from, to] in bounds_edges(&corners) {
        distance = distance.min(segment_segment_distance(segment, [from, to]));
    }
    distance
}

/// Triangle overlap by testing each triangle's edges against the other.
///
/// Coplanar overlaps are not detected here; callers treat "no evidence of
/// penetration between penetrating boxes" as a bounding-volume artefact, so a
/// coplanar pair falls back to the conservative answer.
pub fn triangles_intersect(a: [[f32; 3]; 3], b: [[f32; 3]; 3]) -> bool {
    for index in 0..3 {
        let next = (index + 1) % 3;
        if segment_triangle([a[index], a[next]], b) {
            return true;
        }
        if segment_triangle([b[index], b[next]], a) {
            return true;
        }
    }
    false
}

/// True when any triangle of `a` overlaps any triangle of `b`.
///
/// `budget` bounds the number of triangle pairs examined; exhausting it returns
/// `None` so the caller can report the pair as unconfirmed instead of stalling.
pub fn meshes_intersect(a: &[[[f32; 3]; 3]], b: &[[[f32; 3]; 3]], budget: usize) -> Option<bool> {
    let mut examined = 0usize;
    for left in a {
        for right in b {
            if examined >= budget {
                return None;
            }
            examined += 1;
            if triangles_intersect(*left, *right) {
                return Some(true);
            }
        }
    }
    Some(false)
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn point_triangle_distance(point: [f32; 3], triangle: [[f32; 3]; 3]) -> f32 {
    let normal = cross(sub(triangle[1], triangle[0]), sub(triangle[2], triangle[0]));
    let normal_length = length(normal);
    if normal_length > f32::EPSILON {
        let unit_normal = scale(normal, 1.0 / normal_length);
        let signed_distance = dot(unit_normal, sub(point, triangle[0]));
        let projected = [
            point[0] - unit_normal[0] * signed_distance,
            point[1] - unit_normal[1] * signed_distance,
            point[2] - unit_normal[2] * signed_distance,
        ];
        if point_in_triangle(projected, triangle) {
            return signed_distance.abs();
        }
    }
    triangle_edges(triangle)
        .into_iter()
        .map(|edge| point_segment_distance(point, edge))
        .fold(f32::INFINITY, f32::min)
}

fn point_in_triangle(point: [f32; 3], triangle: [[f32; 3]; 3]) -> bool {
    let normal = cross(sub(triangle[1], triangle[0]), sub(triangle[2], triangle[0]));
    if length(normal) <= f32::EPSILON {
        return false;
    }
    let signs = [
        dot(
            normal,
            cross(sub(triangle[1], triangle[0]), sub(point, triangle[0])),
        ),
        dot(
            normal,
            cross(sub(triangle[2], triangle[1]), sub(point, triangle[1])),
        ),
        dot(
            normal,
            cross(sub(triangle[0], triangle[2]), sub(point, triangle[2])),
        ),
    ];
    signs.iter().all(|sign| *sign >= -1e-7) || signs.iter().all(|sign| *sign <= 1e-7)
}

fn point_segment_distance(point: [f32; 3], segment: [[f32; 3]; 2]) -> f32 {
    let direction = sub(segment[1], segment[0]);
    let denominator = dot(direction, direction);
    if denominator <= f32::EPSILON {
        return length(sub(point, segment[0]));
    }
    let t = (dot(sub(point, segment[0]), direction) / denominator).clamp(0.0, 1.0);
    let closest = [
        segment[0][0] + direction[0] * t,
        segment[0][1] + direction[1] * t,
        segment[0][2] + direction[2] * t,
    ];
    length(sub(point, closest))
}

fn segment_segment_distance(left: [[f32; 3]; 2], right: [[f32; 3]; 2]) -> f32 {
    let first = sub(left[1], left[0]);
    let second = sub(right[1], right[0]);
    let offset = sub(left[0], right[0]);
    let first_length = dot(first, first);
    let second_length = dot(second, second);
    let second_offset = dot(second, offset);
    let (mut s, mut t);
    if first_length <= f32::EPSILON && second_length <= f32::EPSILON {
        return length(offset);
    }
    if first_length <= f32::EPSILON {
        s = 0.0;
        t = (second_offset / second_length).clamp(0.0, 1.0);
    } else {
        let first_offset = dot(first, offset);
        if second_length <= f32::EPSILON {
            t = 0.0;
            s = (-first_offset / first_length).clamp(0.0, 1.0);
        } else {
            let cross_offset = dot(first, second);
            let denominator = first_length * second_length - cross_offset * cross_offset;
            s = if denominator.abs() > f32::EPSILON {
                ((cross_offset * second_offset - first_offset * second_length) / denominator)
                    .clamp(0.0, 1.0)
            } else {
                0.0
            };
            t = (cross_offset * s + second_offset) / second_length;
            if t < 0.0 {
                t = 0.0;
                s = (-first_offset / first_length).clamp(0.0, 1.0);
            } else if t > 1.0 {
                t = 1.0;
                s = ((cross_offset - first_offset) / first_length).clamp(0.0, 1.0);
            }
        }
    }
    let left_point = add_scaled(left[0], first, s);
    let right_point = add_scaled(right[0], second, t);
    length(sub(left_point, right_point))
}

fn segment_aabb_intersects(segment: [[f32; 3]; 2], bounds: &Bounds) -> bool {
    if point_in_aabb(segment[0], bounds) || point_in_aabb(segment[1], bounds) {
        return true;
    }
    let direction = sub(segment[1], segment[0]);
    let minimum = [bounds.min.x, bounds.min.y, bounds.min.z];
    let maximum = [bounds.max.x, bounds.max.y, bounds.max.z];
    let mut near = 0.0_f32;
    let mut far = 1.0_f32;
    for axis in 0..3 {
        if direction[axis].abs() <= f32::EPSILON {
            if segment[0][axis] < minimum[axis] || segment[0][axis] > maximum[axis] {
                return false;
            }
            continue;
        }
        let inverse = 1.0 / direction[axis];
        let mut first = (minimum[axis] - segment[0][axis]) * inverse;
        let mut second = (maximum[axis] - segment[0][axis]) * inverse;
        if first > second {
            std::mem::swap(&mut first, &mut second);
        }
        near = near.max(first);
        far = far.min(second);
        if near > far {
            return false;
        }
    }
    true
}

fn point_in_aabb(point: [f32; 3], bounds: &Bounds) -> bool {
    point[0] >= bounds.min.x
        && point[0] <= bounds.max.x
        && point[1] >= bounds.min.y
        && point[1] <= bounds.max.y
        && point[2] >= bounds.min.z
        && point[2] <= bounds.max.z
}

fn point_aabb_distance(point: [f32; 3], bounds: &Bounds) -> f32 {
    let closest = [
        point[0].clamp(bounds.min.x, bounds.max.x),
        point[1].clamp(bounds.min.y, bounds.max.y),
        point[2].clamp(bounds.min.z, bounds.max.z),
    ];
    length(sub(point, closest))
}

fn bounds_corners(bounds: &Bounds) -> [[f32; 3]; 8] {
    let mut corners = [[0.0; 3]; 8];
    let mut index = 0;
    for x in [bounds.min.x, bounds.max.x] {
        for y in [bounds.min.y, bounds.max.y] {
            for z in [bounds.min.z, bounds.max.z] {
                corners[index] = [x, y, z];
                index += 1;
            }
        }
    }
    corners
}

fn bounds_edges(corners: &[[f32; 3]; 8]) -> [[[f32; 3]; 2]; 12] {
    [
        [corners[0], corners[1]],
        [corners[0], corners[2]],
        [corners[0], corners[4]],
        [corners[1], corners[3]],
        [corners[1], corners[5]],
        [corners[2], corners[3]],
        [corners[2], corners[6]],
        [corners[3], corners[7]],
        [corners[4], corners[5]],
        [corners[4], corners[6]],
        [corners[5], corners[7]],
        [corners[6], corners[7]],
    ]
}

fn triangle_edges(triangle: [[f32; 3]; 3]) -> [[[f32; 3]; 2]; 3] {
    [
        [triangle[0], triangle[1]],
        [triangle[1], triangle[2]],
        [triangle[2], triangle[0]],
    ]
}

fn add_scaled(point: [f32; 3], direction: [f32; 3], factor: f32) -> [f32; 3] {
    [
        point[0] + direction[0] * factor,
        point[1] + direction[1] * factor,
        point[2] + direction[2] * factor,
    ]
}

fn scale(value: [f32; 3], factor: f32) -> [f32; 3] {
    [value[0] * factor, value[1] * factor, value[2] * factor]
}

fn length(value: [f32; 3]) -> f32 {
    dot(value, value).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Vec3;

    fn bounds(min: [f32; 3], max: [f32; 3]) -> Bounds {
        Bounds {
            min: Vec3 {
                x: min[0],
                y: min[1],
                z: min[2],
            },
            max: Vec3 {
                x: max[0],
                y: max[1],
                z: max[2],
            },
        }
    }

    #[test]
    fn signed_gap_reports_separation_and_depth() {
        let a = bounds([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        assert!((signed_gap(&a, &bounds([3.0, 0.0, 0.0], [4.0, 1.0, 1.0])) - 2.0).abs() < 1e-5);
        assert!((signed_gap(&a, &bounds([0.5, 0.0, 0.0], [2.0, 1.0, 1.0])) + 0.5).abs() < 1e-5);
        assert_eq!(
            signed_gap(&a, &bounds([1.0, 0.0, 0.0], [2.0, 1.0, 1.0])),
            0.0
        );
    }

    #[test]
    fn sweep_pairs_finds_close_pairs_only() {
        let boxes = [
            bounds([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]),
            bounds([0.2, 0.0, 0.0], [1.2, 1.0, 1.0]),
            bounds([10.0, 0.0, 0.0], [11.0, 1.0, 1.0]),
        ];
        assert_eq!(sweep_pairs(&boxes, 0.0), vec![(0, 1)]);
    }

    #[test]
    fn sweep_pairs_honours_the_clearance_band() {
        let boxes = [
            bounds([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]),
            bounds([1.4, 0.0, 0.0], [2.0, 1.0, 1.0]),
        ];
        assert!(sweep_pairs(&boxes, 0.3).is_empty());
        assert_eq!(sweep_pairs(&boxes, 0.5), vec![(0, 1)]);
    }

    #[test]
    fn triangles_intersect_when_they_cross() {
        let flat = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let upright = [[0.2, 0.2, -1.0], [0.2, 0.2, 1.0], [0.8, 0.2, 1.0]];
        assert!(triangles_intersect(flat, upright));
    }

    #[test]
    fn separated_triangles_do_not_intersect() {
        let flat = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let above = [[0.0, 0.0, 1.0], [1.0, 0.0, 1.0], [0.0, 1.0, 1.0]];
        assert!(!triangles_intersect(flat, above));
    }

    #[test]
    fn mesh_budget_reports_exhaustion() {
        let a = vec![[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]; 4];
        let b = vec![[[0.0, 0.0, 5.0], [1.0, 0.0, 5.0], [0.0, 1.0, 5.0]]; 4];
        assert_eq!(meshes_intersect(&a, &b, 100), Some(false));
        assert_eq!(meshes_intersect(&a, &b, 3), None);
    }

    #[test]
    fn segment_triangle_distance_reports_crossing_and_clearance() {
        let triangle = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        assert_eq!(
            segment_triangle_distance([[0.2, 0.2, -1.0], [0.2, 0.2, 1.0]], triangle),
            0.0,
        );
        let near = segment_triangle_distance([[0.2, 0.2, 0.5], [0.8, 0.2, 0.5]], triangle);
        assert!((near - 0.5).abs() < 1e-5, "{near}");

        let bounds = bounds([0.0, 0.0, -1.0], [1.0, 1.0, 1.0]);
        assert_eq!(
            segment_bounds_distance([[0.2, 0.2, -2.0], [0.2, 0.2, 2.0]], &bounds),
            0.0,
        );
    }
}
