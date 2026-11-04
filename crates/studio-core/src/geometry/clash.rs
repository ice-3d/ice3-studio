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
}
