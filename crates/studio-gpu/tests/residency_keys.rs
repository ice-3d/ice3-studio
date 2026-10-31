use studio_core::document::NodeContent;
use studio_core::geometry::{Primitive, TriangleMesh};
use studio_core::gpu_contract::GeometryDirtyChannel;
use studio_gpu::{DirtyPlan, ResidencyKey};

#[test]
fn box_dimension_change_reuses_topology_and_normals() {
    let before = NodeContent::Primitive(Primitive::Box {
        width: 1.0,
        height: 1.0,
        depth: 1.0,
    });
    let after = NodeContent::Primitive(Primitive::Box {
        width: 3.0,
        height: 1.0,
        depth: 1.0,
    });
    let plan = DirtyPlan::for_transition(Some(&before), &after);
    assert!(!plan.dirty.contains(GeometryDirtyChannel::Topology));
    assert!(plan.dirty.contains(GeometryDirtyChannel::Positions));
    assert!(!plan.dirty.contains(GeometryDirtyChannel::Normals));
    assert!(plan.dirty.contains(GeometryDirtyChannel::Bounds));
}

#[test]
fn radial_topology_changes_rebuild_connectivity() {
    let cylinder_before = NodeContent::Primitive(Primitive::Cylinder {
        radius: 1.0,
        height: 2.0,
        segments: 16,
    });
    let cylinder_after = NodeContent::Primitive(Primitive::Cylinder {
        radius: 1.0,
        height: 2.0,
        segments: 32,
    });
    assert!(
        DirtyPlan::for_transition(Some(&cylinder_before), &cylinder_after)
            .requires_topology_rebuild()
    );

    let sphere_before = NodeContent::Primitive(Primitive::Sphere {
        radius: 2.0,
        segments: 32,
        rings: 16,
    });
    let sphere_after = NodeContent::Primitive(Primitive::Sphere {
        radius: 2.0,
        segments: 32,
        rings: 24,
    });
    assert!(
        DirtyPlan::for_transition(Some(&sphere_before), &sphere_after).requires_topology_rebuild()
    );
}

#[test]
fn sweep_radius_updates_normals_without_rebuilding_topology() {
    let points = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, 1.0]];
    let before = NodeContent::Sweep {
        points: points.clone(),
        radius: 0.25,
        radial_segments: 12,
    };
    let after = NodeContent::Sweep {
        points,
        radius: 0.5,
        radial_segments: 12,
    };
    let plan = DirtyPlan::for_transition(Some(&before), &after);
    assert!(!plan.dirty.contains(GeometryDirtyChannel::Topology));
    assert!(plan.dirty.contains(GeometryDirtyChannel::Normals));
    assert!(plan.dirty.contains(GeometryDirtyChannel::Positions));
}

#[test]
fn custom_mesh_indices_and_positions_invalidate_independent_channels() {
    let base = TriangleMesh {
        positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        indices: vec![0, 1, 2],
    };
    let position_change = NodeContent::CustomMesh(TriangleMesh {
        positions: vec![[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        indices: base.indices.clone(),
    });
    let position_plan = DirtyPlan::for_transition(
        Some(&NodeContent::CustomMesh(base.clone())),
        &position_change,
    );
    assert!(!position_plan.dirty.contains(GeometryDirtyChannel::Topology));
    assert!(position_plan.dirty.contains(GeometryDirtyChannel::Normals));

    let index_change = NodeContent::CustomMesh(TriangleMesh {
        positions: base.positions.clone(),
        indices: vec![0, 1, 0],
    });
    let index_plan = DirtyPlan::for_transition(Some(&NodeContent::CustomMesh(base)), &index_change);
    assert!(index_plan.dirty.contains(GeometryDirtyChannel::Topology));
    assert!(!index_plan.dirty.contains(GeometryDirtyChannel::Positions));
}

#[test]
fn transform_and_material_updates_skip_geometry_evaluation() {
    let transform = DirtyPlan::for_transform_change();
    assert!(transform.dirty.contains(GeometryDirtyChannel::Transform));
    assert!(transform.dirty.contains(GeometryDirtyChannel::Bounds));
    assert!(!transform.requires_topology_rebuild());

    let material = DirtyPlan::for_material_change();
    assert_eq!(material.dirty.bits(), 1 << 6);
    assert!(!material.requires_topology_rebuild());
}

#[test]
fn residency_keys_separate_topology_attributes_and_dependencies() {
    let content = NodeContent::Primitive(Primitive::Box {
        width: 1.0,
        height: 2.0,
        depth: 3.0,
    });
    let first = ResidencyKey::for_content(&content, "dependency-a", "kernel-v1").unwrap();
    let same = ResidencyKey::for_content(&content, "dependency-a", "kernel-v1").unwrap();
    let dependency = ResidencyKey::for_content(&content, "dependency-b", "kernel-v1").unwrap();
    let kernel = ResidencyKey::for_content(&content, "dependency-a", "kernel-v2").unwrap();
    assert_eq!(first, same);
    assert_eq!(first.topology, dependency.topology);
    assert_ne!(first.attribute, dependency.attribute);
    assert_ne!(first.result, dependency.result);
    assert_ne!(first.result, kernel.result);
}
