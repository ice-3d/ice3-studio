use studio_core::geometry::Primitive;
use studio_gpu::{BufferClass, NativeGpuBackend, PrimitiveJob};

#[test]
fn capacity_classes_round_up_without_unbounded_allocations() {
    assert_eq!(BufferClass::for_bytes(1), BufferClass::OneMiB);
    assert_eq!(BufferClass::for_bytes(1_048_577), BufferClass::FourMiB);
    assert_eq!(
        BufferClass::for_bytes(80 * 1024 * 1024),
        BufferClass::TwoHundredFiftySixMiB
    );
}

#[test]
fn repeated_device_evaluations_reuse_pipelines_and_buffers() {
    let Some(backend) = NativeGpuBackend::for_test_gpu() else {
        return;
    };
    backend.warm_pipelines_for_test().unwrap();
    let first = backend.resource_stats();
    for _ in 0..3 {
        backend.warm_pipelines_for_test().unwrap();
    }
    let second = backend.resource_stats();
    assert_eq!(first.pipeline_count, second.pipeline_count);
    assert_eq!(
        first.buffer_allocation_count,
        second.buffer_allocation_count
    );
    assert_eq!(backend.ready_staging_slots().unwrap(), 3);
}

#[test]
fn repeated_primitive_evaluations_reuse_pooled_buffers() {
    let Some(backend) = NativeGpuBackend::for_test_gpu() else {
        return;
    };
    let job = PrimitiveJob {
        primitive: Primitive::Box {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        },
    };
    backend.evaluate_primitive(job.clone()).unwrap();
    let first = backend.resource_stats();
    backend.evaluate_primitive(job).unwrap();
    let second = backend.resource_stats();
    assert!(first.buffer_allocation_count > 0);
    assert_eq!(
        first.buffer_allocation_count,
        second.buffer_allocation_count
    );
}
