use studio_core::scene_contract::SceneInstanceChange;
use studio_gpu::InstanceTable;

#[test]
fn transform_update_touches_only_the_matrix_range() {
    let mut table = InstanceTable::default();
    table.apply_changes(vec![instance("node-000001", 0.0)]);
    let update = table.apply_changes(vec![instance("node-000001", 3.0)]);

    assert_eq!(update.changed_ranges.len(), 1);
    assert_eq!(update.changed_ranges[0].start, 0);
    assert_eq!(update.changed_ranges[0].count, 1);
    assert_eq!(update.changed_bytes, 64);
    assert_eq!(table.records()[0].matrix[12], 3.0);
}

#[test]
fn stable_node_ids_reuse_the_same_instance_slot() {
    let mut table = InstanceTable::default();
    table.apply_changes(vec![
        instance("node-000001", 1.0),
        instance("node-000002", 2.0),
    ]);
    let update = table.apply_changes(vec![instance("node-000001", 5.0)]);

    assert_eq!(update.changed_ranges[0].start, 0);
    assert_eq!(table.records()[1].node_id, "node-000002");
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
