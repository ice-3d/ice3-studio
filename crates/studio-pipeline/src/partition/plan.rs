use std::cmp::Ordering;

use glam::Vec3;
use studio_core::geometry::Bounds;

use super::{PartitionError, PartitionUnit};

#[derive(Debug, Clone, PartialEq)]
pub struct PartitionPlan {
    pub bounds: Bounds,
    pub triangle_count: u64,
    pub rendered_triangle_count: u64,
    pub root: PartitionPlanNode,
    pub leaves: Vec<PartitionLeaf>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PartitionPlanNode {
    pub bounds: Bounds,
    pub triangle_count: u64,
    pub units: Vec<PartitionUnit>,
    pub children: Vec<PartitionPlanNode>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PartitionLeaf {
    pub id: String,
    pub bounds: Bounds,
    pub triangle_count: u64,
    pub units: Vec<PartitionUnit>,
}

pub fn create_partition_plan(units: &[PartitionUnit], max_triangles: u64) -> PartitionPlan {
    try_create_partition_plan(units, max_triangles)
        .expect("partition plan requires valid non-empty units")
}

pub fn try_create_partition_plan(
    units: &[PartitionUnit],
    max_triangles: u64,
) -> Result<PartitionPlan, PartitionError> {
    if max_triangles == 0 {
        return Err(PartitionError::Options(
            "maxTriangles must be positive".into(),
        ));
    }
    if units.is_empty() {
        return Err(PartitionError::Empty("no partition units".into()));
    }
    let mut units = units.to_vec();
    units.sort_by(unit_order);
    let rendered_triangle_count = units.iter().try_fold(0_u64, |total, unit| {
        let rendered = unit
            .triangle_count
            .checked_mul(unit.node_ids.len() as u64)
            .ok_or_else(|| PartitionError::Invalid("rendered triangle count overflows".into()))?;
        total
            .checked_add(rendered)
            .ok_or_else(|| PartitionError::Invalid("rendered triangle count overflows".into()))
    })?;
    let root = plan_node(units.clone(), max_triangles)?;
    let mut leaves = Vec::new();
    collect_leaves(&root, &mut leaves);
    for (index, leaf) in leaves.iter_mut().enumerate() {
        leaf.id = format!("part-{index:03}");
    }
    Ok(PartitionPlan {
        bounds: root.bounds,
        triangle_count: root.triangle_count,
        rendered_triangle_count,
        root,
        leaves,
    })
}

fn plan_node(
    mut units: Vec<PartitionUnit>,
    max_triangles: u64,
) -> Result<PartitionPlanNode, PartitionError> {
    units.sort_by(unit_order);
    let triangle_count = units.iter().try_fold(0_u64, |total, unit| {
        total
            .checked_add(unit.triangle_count)
            .ok_or_else(|| PartitionError::Invalid("triangle count overflows".into()))
    })?;
    let bounds = union_bounds(units.iter().map(|unit| unit.bounds));
    if triangle_count <= max_triangles || units.len() == 1 {
        return Ok(PartitionPlanNode {
            bounds,
            triangle_count,
            units,
            children: Vec::new(),
        });
    }

    let centers = units
        .iter()
        .map(|unit| center(unit.bounds))
        .collect::<Vec<_>>();
    let mut axis = 0;
    let mut extent = f32::NEG_INFINITY;
    for candidate in 0..3 {
        let values = centers.iter().map(|center| center[candidate]);
        let candidate_extent =
            values.clone().fold(f32::NEG_INFINITY, f32::max) - values.fold(f32::INFINITY, f32::min);
        if candidate_extent > extent {
            extent = candidate_extent;
            axis = candidate;
        }
    }
    units.sort_by(|left, right| {
        center(left.bounds)[axis]
            .total_cmp(&center(right.bounds)[axis])
            .then_with(|| unit_order(left, right))
    });
    let mut cumulative = 0_u64;
    let mut split = 1;
    for (index, unit) in units.iter().enumerate().take(units.len() - 1) {
        cumulative += unit.triangle_count;
        split = index + 1;
        if cumulative.saturating_mul(2) >= triangle_count {
            break;
        }
    }
    let right = units.split_off(split.clamp(1, units.len() - 1));
    let children = vec![
        plan_node(units, max_triangles)?,
        plan_node(right, max_triangles)?,
    ];
    Ok(PartitionPlanNode {
        bounds,
        triangle_count,
        units: children
            .iter()
            .flat_map(|child| child.units.iter().cloned())
            .collect(),
        children,
    })
}

fn collect_leaves(node: &PartitionPlanNode, output: &mut Vec<PartitionLeaf>) {
    if node.children.is_empty() {
        output.push(PartitionLeaf {
            id: String::new(),
            bounds: node.bounds,
            triangle_count: node.triangle_count,
            units: node.units.clone(),
        });
        return;
    }
    for child in &node.children {
        collect_leaves(child, output);
    }
}

fn unit_order(left: &PartitionUnit, right: &PartitionUnit) -> Ordering {
    left.id
        .cmp(&right.id)
        .then_with(|| left.mesh_index.cmp(&right.mesh_index))
}

fn center(bounds: Bounds) -> Vec3 {
    Vec3::new(
        (bounds.min.x + bounds.max.x) * 0.5,
        (bounds.min.y + bounds.max.y) * 0.5,
        (bounds.min.z + bounds.max.z) * 0.5,
    )
}

fn union_bounds(bounds: impl IntoIterator<Item = Bounds>) -> Bounds {
    let mut bounds = bounds.into_iter();
    let first = bounds.next().expect("partition node has units");
    bounds.fold(first, |current, next| Bounds {
        min: studio_core::math::Vec3 {
            x: current.min.x.min(next.min.x),
            y: current.min.y.min(next.min.y),
            z: current.min.z.min(next.min.z),
        },
        max: studio_core::math::Vec3 {
            x: current.max.x.max(next.max.x),
            y: current.max.y.max(next.max.y),
            z: current.max.z.max(next.max.z),
        },
    })
}
