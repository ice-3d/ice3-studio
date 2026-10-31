use studio_core::document::NodeContent;
use studio_core::geometry::TriangleMesh;
use studio_core::gpu_contract::GeometryDirtyChannel;
use studio_core::scene_contract::{
    ElementRange, SceneSessionState, SceneSessionStatus, changed_position_ranges,
    scene_content_dirty,
};

#[test]
fn changed_positions_are_compressed_into_contiguous_ranges() {
    let before = TriangleMesh {
        positions: vec![[0.0; 3], [1.0; 3], [2.0; 3], [3.0; 3]],
        indices: vec![0, 1, 2, 1, 2, 3],
    };
    let after = TriangleMesh {
        positions: vec![[0.0; 3], [4.0; 3], [5.0; 3], [3.0; 3]],
        indices: before.indices.clone(),
    };

    assert_eq!(
        changed_position_ranges(&before, &after),
        Some(vec![ElementRange { start: 1, count: 2 }])
    );
}

#[test]
fn topology_changes_do_not_claim_an_incremental_range() {
    let before = TriangleMesh {
        positions: vec![[0.0; 3]; 3],
        indices: vec![0, 1, 2],
    };
    let after = TriangleMesh {
        positions: vec![[0.0; 3]; 4],
        indices: vec![0, 1, 2],
    };

    assert_eq!(changed_position_ranges(&before, &after), None);
}

#[test]
fn custom_mesh_attributes_mark_normals_and_bounds_dirty() {
    let before = NodeContent::CustomMesh(TriangleMesh {
        positions: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        indices: vec![0, 1, 2],
    });
    let after = NodeContent::CustomMesh(TriangleMesh {
        positions: vec![[0.0; 3], [2.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        indices: vec![0, 1, 2],
    });
    let dirty = scene_content_dirty(&before, &after);

    assert!(dirty.contains(GeometryDirtyChannel::Positions));
    assert!(dirty.contains(GeometryDirtyChannel::Normals));
    assert!(dirty.contains(GeometryDirtyChannel::Bounds));
    assert!(!dirty.contains(GeometryDirtyChannel::Topology));
}

#[test]
fn disabled_scene_status_has_stable_schema_defaults() {
    let status = SceneSessionStatus::default();

    assert!(!status.enabled);
    assert_eq!(status.state, SceneSessionState::Idle);
    assert_eq!(status.session_id, None);
    assert_eq!(status.published_revision, 0);
    assert_eq!(status.target_revision, 0);
    assert_eq!(status.geometry_dispatch_count, 0);
}
