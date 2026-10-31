use studio_core::document::NodeContent;
use studio_core::geometry::TriangleMesh;
use studio_core::gpu_contract::GeometryDirtyChannel;
use studio_core::scene_contract::changed_position_ranges;

#[test]
fn stable_topology_update_reuses_indices_and_uploads_only_changed_positions() {
    let Some(backend) = studio_gpu::NativeGpuBackend::for_test_gpu() else {
        return;
    };
    let first = mesh(1.0);
    let second = mesh(2.0);
    let seed = backend.evaluate_custom_mesh_resident(&first).unwrap();
    let ranges = changed_position_ranges(&first, &second).unwrap();
    let updated = backend
        .update_resident_geometry(&seed.id, &NodeContent::CustomMesh(second.clone()), ranges)
        .unwrap();
    backend.evaluate_resident_handle(&updated.id).unwrap();
    let snapshot = backend.snapshot_geometry(&updated.id).unwrap();

    assert_eq!(snapshot.indices, second.indices);
    assert_eq!(snapshot.vertices.len(), second.positions.len());
    assert!(updated.evidence.stats.upload_bytes < (second.positions.len() * 16) as u64);
    assert!(!updated.dirty.contains(GeometryDirtyChannel::Topology));
}

#[test]
fn changed_index_count_allocates_new_topology() {
    let Some(backend) = studio_gpu::NativeGpuBackend::for_test_gpu() else {
        return;
    };
    let first = mesh(1.0);
    let seed = backend.evaluate_custom_mesh_resident(&first).unwrap();
    let mut second = mesh(2.0);
    second.positions.push([4.0, 4.0, 0.0]);
    second.indices.extend([0, 3, 1]);
    let error =
        backend.update_resident_geometry(&seed.id, &NodeContent::CustomMesh(second), Vec::new());

    assert!(error.is_err());
}

#[test]
fn referenced_topology_is_never_an_eviction_candidate() {
    let mut references = studio_gpu::ResourceReferences::default();
    references.add_geometry("geometry-1", "topology-1", "attribute-1");
    references.add_geometry("geometry-2", "topology-1", "attribute-2");
    references.remove_geometry("geometry-1");

    assert!(references.topology_is_referenced("topology-1"));
    assert_eq!(
        references.evictable_attributes().collect::<Vec<_>>(),
        vec!["attribute-1"]
    );
    assert!(references.evictable_topologies().next().is_none());
}

fn mesh(width: f32) -> TriangleMesh {
    TriangleMesh {
        positions: vec![[0.0, 0.0, 0.0], [width, 0.0, 0.0], [0.0, width, 0.0]],
        indices: vec![0, 1, 2],
    }
}
