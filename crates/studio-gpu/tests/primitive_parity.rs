use studio_core::document::NodeContent;
use studio_core::geometry::{Primitive, evaluate_node_content};
use studio_gpu::{NativeGpuBackend, PrimitiveJob};

fn assert_parity(primitive: Primitive) {
    let Some(backend) = NativeGpuBackend::for_test_gpu() else {
        return;
    };
    let gpu = backend
        .evaluate_primitive(PrimitiveJob {
            primitive: primitive.clone(),
        })
        .unwrap();
    let cpu = evaluate_node_content(&NodeContent::Primitive(primitive)).unwrap();
    assert_eq!(gpu.indices, cpu.indices);
    assert_eq!(gpu.vertices.len(), cpu.vertices.len());
    for (gpu_vertex, cpu_vertex) in gpu.vertices.iter().zip(&cpu.vertices) {
        for axis in 0..3 {
            assert!((gpu_vertex.position[axis] - cpu_vertex.position[axis]).abs() <= 1e-5);
            assert!((gpu_vertex.normal[axis] - cpu_vertex.normal[axis]).abs() <= 1e-5);
        }
    }
}

#[test]
fn box_and_plane_match_cpu_reference() {
    assert_parity(Primitive::Box {
        width: 3.0,
        height: 2.0,
        depth: 4.0,
    });
    assert_parity(Primitive::Plane {
        width: 8.0,
        depth: 6.0,
    });
}
