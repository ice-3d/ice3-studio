use studio_core::document::NodeContent;
use studio_core::geometry::TriangleMesh;
use studio_core::scene_contract::{
    SceneGeometryChange, SceneInstanceChange, ScenePatch, SceneSessionState,
};
use studio_gpu::{NativeGpuBackend, ResidencyKey};

#[test]
fn pending_revisions_publish_in_order_and_transform_updates_skip_geometry() {
    let Some(backend) = NativeGpuBackend::for_test_gpu() else {
        return;
    };
    let session = backend.create_scene_session("scene-test").unwrap();
    let first = backend
        .apply_scene_patch(
            &session,
            patch(
                1,
                0,
                true,
                Some(geometry("geometry-1", 1.0)),
                instance("node-000001", 0.0),
            ),
        )
        .unwrap();
    assert_eq!(first.submission_count, 1);

    let second = backend
        .apply_scene_patch(
            &session,
            patch(2, 1, false, None, instance("node-000001", 4.0)),
        )
        .unwrap();
    assert_eq!(second.geometry_dispatch_count, 0);
    assert_eq!(second.readback_bytes, 0);

    let ready = backend.wait_scene_revision(&session, 2).unwrap();
    assert_eq!(ready.published_revision, 2);
    assert_eq!(ready.target_revision, 2);
    assert_eq!(ready.state, SceneSessionState::Idle);
}

#[test]
fn device_failure_never_publishes_the_target_revision() {
    let Some(backend) = NativeGpuBackend::for_test_gpu() else {
        return;
    };
    let session = backend.create_scene_session("failure-test").unwrap();
    let first = patch(
        1,
        0,
        true,
        Some(geometry("geometry-1", 1.0)),
        instance("node-000001", 0.0),
    );
    backend.apply_scene_patch(&session, first).unwrap();
    backend.wait_scene_revision(&session, 1).unwrap();

    backend.fail_scene_for_test(&session, "simulated device loss");
    let second = patch(2, 1, false, None, instance("node-000001", 3.0));
    let error = backend.apply_scene_patch(&session, second);
    assert!(error.is_err());
    let status = backend.scene_status(&session).unwrap();
    assert_eq!(status.published_revision, 1);
    assert_eq!(status.target_revision, 2);
    assert_eq!(status.state, SceneSessionState::Failed);
}

fn geometry(id: &str, width: f32) -> SceneGeometryChange {
    let content = NodeContent::CustomMesh(TriangleMesh {
        positions: vec![[0.0, 0.0, 0.0], [width, 0.0, 0.0], [0.0, width, 0.0]],
        indices: vec![0, 1, 2],
    });
    let key = ResidencyKey::for_content(&content, "", "geometry-kernel-v1").unwrap();
    SceneGeometryChange {
        geometry_id: id.into(),
        previous_geometry_id: None,
        content,
        digests: key.digest_set("", "geometry-kernel-v1"),
        dirty: Default::default(),
        position_ranges: Vec::new(),
    }
}

fn instance(node_id: &str, x: f32) -> SceneInstanceChange {
    let mut matrix = [0.0; 16];
    matrix[0] = 1.0;
    matrix[5] = 1.0;
    matrix[10] = 1.0;
    matrix[15] = 1.0;
    matrix[12] = x;
    SceneInstanceChange {
        node_id: node_id.into(),
        geometry_id: Some("geometry-1".into()),
        transform: studio_core::math::Transform::IDENTITY,
        matrix,
        material_id: None,
        visible: true,
    }
}

fn patch(
    target_revision: u64,
    base_revision: u64,
    full_rebuild: bool,
    geometry: Option<SceneGeometryChange>,
    instance: SceneInstanceChange,
) -> ScenePatch {
    ScenePatch {
        base_revision,
        target_revision,
        full_rebuild,
        geometry: geometry.into_iter().collect(),
        instances: vec![instance],
        removed_geometry_ids: Vec::new(),
        removed_node_ids: Vec::new(),
    }
}
