use studio_core::geometry::{Bounds, EvaluatedMesh, Vertex};
use studio_core::gpu_contract::GeometryBackendKind;
use studio_core::large_scene::cache::{GeometryCacheEntry, GeometryCacheMetadata, GeometryStore};
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

#[test]
fn persists_backend_kernel_and_parity_metadata() {
    let project = tempfile::tempdir().unwrap();
    let mut cache = GeometryDiskCache::new(project.path()).unwrap();
    let entry = GeometryCacheEntry {
        mesh: EvaluatedMesh {
            vertices: vec![Vertex {
                position: [0.0, 0.0, 0.0],
                normal: [0.0, 1.0, 0.0],
            }],
            indices: vec![0, 0, 0],
            bounds: Bounds {
                min: Vec3::default(),
                max: Vec3::default(),
            },
        },
        metadata: GeometryCacheMetadata {
            backend: GeometryBackendKind::Gpu,
            kernel_version: "geometry-kernel-v1".into(),
            parity_status: "passed".into(),
        },
        resident_geometry_id: None,
    };
    cache.store_entry("gpu-mesh".into(), entry.clone());
    assert_eq!(cache.load_entry("gpu-mesh"), Some(entry));
}
