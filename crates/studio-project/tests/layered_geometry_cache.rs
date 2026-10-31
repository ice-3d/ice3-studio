use studio_core::geometry::{Bounds, EvaluatedMesh, Vertex};
use studio_core::gpu_contract::{GeometryBackendKind, ResidentGeometryId};
use studio_core::large_scene::cache::{GeometryCacheEntry, GeometryCacheMetadata, GeometryStore};
use studio_core::math::Vec3;
use studio_project::layered_geometry_cache::LayeredGeometryCache;

fn entry() -> GeometryCacheEntry {
    GeometryCacheEntry {
        mesh: EvaluatedMesh {
            vertices: vec![Vertex {
                position: [1.0, 2.0, 3.0],
                normal: [0.0, 1.0, 0.0],
            }],
            indices: vec![0, 0, 0],
            bounds: Bounds {
                min: Vec3 {
                    x: 1.0,
                    y: 2.0,
                    z: 3.0,
                },
                max: Vec3 {
                    x: 1.0,
                    y: 2.0,
                    z: 3.0,
                },
            },
        },
        metadata: GeometryCacheMetadata {
            backend: GeometryBackendKind::Gpu,
            kernel_version: "geometry-kernel-v1".into(),
            parity_status: "passed".into(),
        },
        resident_geometry_id: Some(ResidentGeometryId("resident-1".into())),
    }
}

#[test]
fn memory_hit_survives_removal_of_the_disk_entry() {
    let temp = tempfile::tempdir().unwrap();
    let mut cache = LayeredGeometryCache::new(temp.path(), 1024 * 1024).unwrap();
    cache.store_entry("scene-cache".into(), entry());
    std::fs::remove_file(temp.path().join("cache/geometry/scene-cache.mesh")).unwrap();

    let loaded = cache.load_entry("scene-cache").unwrap();
    assert_eq!(loaded.mesh.vertices[0].position, [1.0, 2.0, 3.0]);
    assert_eq!(loaded.resident_geometry_id.unwrap().0, "resident-1");
}

#[test]
fn cold_load_populates_memory_from_disk() {
    let temp = tempfile::tempdir().unwrap();
    let mut first = LayeredGeometryCache::new(temp.path(), 1024 * 1024).unwrap();
    first.store_entry("scene-cache".into(), entry());
    drop(first);

    let second = LayeredGeometryCache::new(temp.path(), 1024 * 1024).unwrap();
    assert!(second.load_entry("scene-cache").is_some());
    std::fs::remove_file(temp.path().join("cache/geometry/scene-cache.mesh")).unwrap();
    assert!(second.load_entry("scene-cache").is_some());
}
