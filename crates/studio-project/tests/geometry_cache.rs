use studio_core::geometry::{Bounds, EvaluatedMesh, Vertex};
use studio_core::large_scene::cache::GeometryStore;
use studio_core::math::Vec3;
use studio_project::geometry_cache::GeometryDiskCache;

#[test]
fn round_trips_an_evaluated_mesh_and_rejects_corruption() {
    let project = tempfile::tempdir().unwrap();
    let mut cache = GeometryDiskCache::new(project.path()).unwrap();
    let mesh = EvaluatedMesh {
        vertices: vec![Vertex {
            position: [0.0, 0.0, 0.0],
            normal: [0.0, 1.0, 0.0],
        }],
        indices: vec![0, 0, 0],
        bounds: Bounds {
            min: Vec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            max: Vec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
        },
    };
    cache.store("abc".into(), mesh.clone());
    assert_eq!(cache.load("abc"), Some(mesh));

    std::fs::write(project.path().join("cache/geometry/abc.mesh"), b"broken").unwrap();
    assert_eq!(cache.load("abc"), None);
}
