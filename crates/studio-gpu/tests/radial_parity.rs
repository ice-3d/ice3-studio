use studio_core::document::NodeContent;
use studio_core::geometry::{EvaluatedMesh, Primitive, evaluate_node_content};
use studio_gpu::{MeshReadback, NativeGpuBackend, PrimitiveJob, SweepJob};

fn assert_mesh_parity(gpu: MeshReadback, cpu: EvaluatedMesh) {
    assert_eq!(gpu.indices.len(), cpu.indices.len());
    if let Some((index, (gpu_index, cpu_index))) = gpu
        .indices
        .iter()
        .zip(&cpu.indices)
        .enumerate()
        .find(|(_, (gpu_index, cpu_index))| gpu_index != cpu_index)
    {
        panic!("first index mismatch at {index}: GPU {gpu_index}, CPU {cpu_index}");
    }
    assert_eq!(gpu.vertices.len(), cpu.vertices.len());
    for (gpu_vertex, cpu_vertex) in gpu.vertices.iter().zip(&cpu.vertices) {
        for axis in 0..3 {
            assert!((gpu_vertex.position[axis] - cpu_vertex.position[axis]).abs() <= 1e-5);
            assert!((gpu_vertex.normal[axis] - cpu_vertex.normal[axis]).abs() <= 1e-5);
        }
    }
}

fn assert_radial_parity(primitive: Primitive) {
    let Some(backend) = NativeGpuBackend::for_test_gpu() else {
        return;
    };
    let gpu = backend
        .evaluate_primitive(PrimitiveJob {
            primitive: primitive.clone(),
        })
        .unwrap();
    let cpu = evaluate_node_content(&NodeContent::Primitive(primitive)).unwrap();
    assert_mesh_parity(gpu, cpu);
}

fn assert_sweep_parity(points: &[[f32; 3]], radius: f32, radial_segments: u32) {
    let Some(backend) = NativeGpuBackend::for_test_gpu() else {
        return;
    };
    let gpu = backend
        .evaluate_sweep(SweepJob {
            points: points.to_vec(),
            radius,
            radial_segments,
        })
        .unwrap();
    let cpu = evaluate_node_content(&NodeContent::Sweep {
        points: points.to_vec(),
        radius,
        radial_segments,
    })
    .unwrap();
    assert_mesh_parity(gpu, cpu);
}

#[test]
fn cylinder_sphere_and_sweep_match_cpu_reference() {
    assert_radial_parity(Primitive::Cylinder {
        radius: 2.0,
        height: 5.0,
        segments: 64,
    });
    assert_radial_parity(Primitive::Sphere {
        radius: 3.0,
        segments: 96,
        rings: 48,
    });
    assert_sweep_parity(
        &[
            [0.0, 0.0, 0.0],
            [5.0, 0.0, 0.0],
            [5.0, 0.0, 5.0],
            [10.0, 0.0, 5.0],
        ],
        0.35,
        24,
    );
}

#[test]
fn radial_kernels_reject_invalid_parameters() {
    let Some(backend) = NativeGpuBackend::for_test_gpu() else {
        return;
    };
    assert!(
        backend
            .evaluate_primitive(PrimitiveJob {
                primitive: Primitive::Cylinder {
                    radius: 0.0,
                    height: 5.0,
                    segments: 64,
                },
            })
            .is_err()
    );
    assert!(
        backend
            .evaluate_primitive(PrimitiveJob {
                primitive: Primitive::Sphere {
                    radius: 3.0,
                    segments: 96,
                    rings: 1,
                },
            })
            .is_err()
    );
    assert!(
        backend
            .evaluate_sweep(SweepJob {
                points: vec![[0.0, 0.0, 0.0], [0.0, 0.0, 0.0]],
                radius: 0.35,
                radial_segments: 24,
            })
            .is_err()
    );
}
