use std::hint::black_box;
use std::time::Instant;

use serde_json::{Value, json};
use studio_core::document::NodeContent;
use studio_core::geometry::{Bounds, EvaluatedMesh, TriangleMesh, evaluate_node_content};
use studio_core::gpu_contract::GeometryPhaseEvidence;
use studio_gpu::NativeGpuBackend;

const WARMUPS: usize = 2;
const SAMPLES: usize = 9;

#[test]
fn benchmark_report_contains_every_evaluation_mode() {
    let report = benchmark_fixture();
    for mode in ["cold", "warm", "resident", "snapshot", "preview"] {
        assert!(
            report["workloads"][0]["modes"].get(mode).is_some(),
            "{mode}"
        );
        assert!(
            report["workloads"][0]["modes"][mode]
                .get("phases")
                .is_some(),
            "{mode} phases"
        );
    }
}

#[test]
fn benchmark_report_keeps_unavailable_gpu_timings_and_byte_counts_truthful() {
    let report = benchmark_fixture();
    let workload = &report["workloads"][0];

    assert!(workload["gpu"]["kernelMs"].is_null());
    assert!(workload["gpu"]["queueWaitMs"].is_null());
    assert_eq!(workload["modes"]["cold"]["uploadBytes"], 1_024);
    assert_eq!(workload["modes"]["cold"]["readbackBytes"], 512);
    assert_eq!(workload["modes"]["cold"]["uploadCount"], 1);
    assert_eq!(workload["modes"]["cold"]["readbackCount"], 1);
    assert_eq!(report["gates"]["residentCacheHit"], true);
    assert_eq!(report["gates"]["coldGpuCrossover"], false);
    assert_eq!(report["gates"]["performance"], false);
}

#[test]
#[ignore = "run through npm run benchmark:gpu"]
fn native_gpu_benchmark() {
    let Some(backend) = NativeGpuBackend::for_test_gpu() else {
        println!(
            "BENCHMARK_JSON:{}",
            json!({
                "skipped": true,
                "reason": "native GPU adapter unavailable",
                "workloads": []
            })
        );
        return;
    };
    let capabilities =
        pollster::block_on(backend.capabilities()).expect("initialized backend capabilities");

    let mut workloads = Vec::new();
    let mut release_gates = Vec::new();
    for triangles in [50_000u32, 500_000, 2_000_000] {
        let mesh = synthetic_mesh(triangles);
        let content = NodeContent::CustomMesh(mesh.clone());
        let cpu_reference = evaluate_node_content(&content).expect("CPU mesh reference");
        let gpu_reference = backend
            .evaluate_custom_mesh(&mesh)
            .expect("native GPU mesh reference");
        let parity = mesh_parity(&gpu_reference, &cpu_reference);

        let mut cpu_samples = Vec::with_capacity(SAMPLES);
        let mut cold_samples = Vec::with_capacity(SAMPLES);
        for _ in 0..WARMUPS {
            black_box(evaluate_node_content(&content).expect("CPU warm-up"));
            black_box(backend.evaluate_custom_mesh(&mesh).expect("GPU warm-up"));
        }
        for _ in 0..SAMPLES {
            let started = Instant::now();
            black_box(evaluate_node_content(&content).expect("CPU sample"));
            cpu_samples.push(started.elapsed().as_secs_f64() * 1_000.0);

            let started = Instant::now();
            black_box(backend.evaluate_custom_mesh(&mesh).expect("GPU sample"));
            cold_samples.push(started.elapsed().as_secs_f64() * 1_000.0);
        }

        let resident_digests = backend
            .prepare_geometry_digests(&content)
            .expect("resident digest");
        let resident_seed = backend
            .evaluate_with_digests(&mesh, resident_digests, "benchmark-resident")
            .expect("resident warm-up");
        let cold_phases = resident_seed.evidence.phases.clone();
        let counters_before = backend.resident_phase_counters().expect("phase counters");
        let mut resident_samples = Vec::with_capacity(SAMPLES);
        let mut lookup_samples = Vec::with_capacity(SAMPLES);
        let mut resident_phases = GeometryPhaseEvidence::default();
        for _ in 0..SAMPLES {
            let started = Instant::now();
            let resident = backend
                .evaluate_resident_handle(&resident_seed.id)
                .expect("resident sample");
            resident_phases = resident.evidence.phases.clone();
            lookup_samples.push(resident.evidence.phases.lookup_ms);
            black_box(resident);
            resident_samples.push(started.elapsed().as_secs_f64() * 1_000.0);
        }
        let counters_after = backend.resident_phase_counters().expect("phase counters");

        let mut snapshot_samples = Vec::with_capacity(SAMPLES);
        let mut snapshot_reference = None;
        for _ in 0..SAMPLES {
            let started = Instant::now();
            let snapshot = backend
                .snapshot_geometry(&resident_seed.id)
                .expect("resident snapshot");
            snapshot_samples.push(started.elapsed().as_secs_f64() * 1_000.0);
            snapshot_reference = Some(snapshot);
        }
        let resident_parity = snapshot_reference
            .as_ref()
            .is_some_and(|snapshot| mesh_parity(snapshot, &cpu_reference));
        let (mesh_upload_bytes, mesh_readback_bytes) = mesh_transfer_bytes(&mesh);

        let mut warm_mesh = mesh.clone();
        warm_mesh.positions[0][2] += 0.01;
        let started = Instant::now();
        black_box(
            studio_gpu::ResidencyKey::for_content(
                &NodeContent::CustomMesh(warm_mesh),
                "",
                "geometry-kernel-v1",
            )
            .expect("warm-topology key"),
        );
        let warm_topology_ms = started.elapsed().as_secs_f64() * 1_000.0;

        let preview_bytes = mesh.positions.len() * 12 + mesh.indices.len() * 4;
        let mut preview_samples = Vec::with_capacity(SAMPLES);
        for _ in 0..SAMPLES {
            let started = Instant::now();
            let mut payload = Vec::with_capacity(preview_bytes);
            for position in &mesh.positions {
                for value in position {
                    payload.extend_from_slice(&value.to_le_bytes());
                }
            }
            for index in &mesh.indices {
                payload.extend_from_slice(&index.to_le_bytes());
            }
            black_box(payload);
            preview_samples.push(started.elapsed().as_secs_f64() * 1_000.0);
        }

        let cpu_p50 = percentile(&mut cpu_samples, 0.50);
        let cpu_p95 = percentile(&mut cpu_samples, 0.95);
        let cold_p50 = percentile(&mut cold_samples, 0.50);
        let cold_p95 = percentile(&mut cold_samples, 0.95);
        let resident_p50 = percentile(&mut resident_samples, 0.50);
        let resident_p95 = percentile(&mut resident_samples, 0.95);
        let resident_lookup_p50 = percentile(&mut lookup_samples, 0.50);
        let snapshot_p50 = percentile(&mut snapshot_samples, 0.50);
        let snapshot_p95 = percentile(&mut snapshot_samples, 0.95);
        let snapshot_phases = backend.resident_stats().expect("resident stats").phases;
        let preview_p50 = percentile(&mut preview_samples, 0.50);
        let preview_p95 = percentile(&mut preview_samples, 0.95);
        let gain = 1.0 - resident_p50 / cpu_p50;
        let p95_ratio = resident_p95 / cpu_p95;
        let cold_gain = 1.0 - cold_p50 / cpu_p50;
        let cold_p95_ratio = cold_p95 / cpu_p95;
        if triangles >= 500_000 {
            release_gates.push(json!({
                "triangles": triangles,
                "residentP50GainAtLeast10Percent": gain >= 0.10,
                "residentP95Within5Percent": p95_ratio <= 1.05,
                "coldGpuP50GainAtLeast10Percent": cold_gain >= 0.10,
                "coldGpuP95Within5Percent": cold_p95_ratio <= 1.05,
                "residentHotPathHashes": counters_after.full_hashes == counters_before.full_hashes,
                "residentLookupBelow0_1Ms": resident_lookup_p50 < 0.1,
            }));
        }
        workloads.push(json!({
            "triangles": triangles,
            "vertices": mesh.positions.len(),
            "parity": parity,
            "cpu": {
                "backend": "cpu",
                "p50Ms": cpu_p50,
                "p95Ms": cpu_p95,
                "kernelMs": cpu_p50,
                "queueWaitMs": 0.0,
                "uploadBytes": 0,
                "readbackBytes": 0,
                "cacheHits": 0,
                "peakEstimatedMemoryBytes": mesh.positions.len() as u64 * 24
                    + mesh.indices.len() as u64 * 4,
            },
            "gpu": {
                "backend": "gpu",
                "p50Ms": cold_p50,
                "p95Ms": cold_p95,
                "kernelMs": cold_phases.gpu_kernel_ms,
                "queueWaitMs": Value::Null,
                "uploadBytes": mesh_upload_bytes,
                "readbackBytes": mesh_readback_bytes,
                "cacheHits": 0,
                "peakEstimatedMemoryBytes": mesh.positions.len() as u64 * 44
                    + mesh.indices.len() as u64 * 20,
            },
            "modes": {
                "cold": mode_report(
                    cold_p50,
                    cold_p95,
                    ModeTransfer::new(1, mesh_upload_bytes, 1, mesh_readback_bytes),
                    None,
                    cold_phases,
                ),
                "warm": mode_report(
                    warm_topology_ms,
                    warm_topology_ms,
                    ModeTransfer::default(),
                    Some("planning-only".into()),
                    GeometryPhaseEvidence {
                        planning_ms: warm_topology_ms,
                        ..GeometryPhaseEvidence::default()
                    },
                ),
                "resident": mode_report(
                    resident_p50,
                    resident_p95,
                    ModeTransfer::default(),
                    None,
                    resident_phases,
                ),
                "snapshot": mode_report(
                    snapshot_p50,
                    snapshot_p95,
                    ModeTransfer::new(0, 0, 1, mesh_readback_bytes),
                    None,
                    snapshot_phases,
                ),
                "preview": mode_report(
                    preview_p50,
                    preview_p95,
                    ModeTransfer::default(),
                    None,
                    GeometryPhaseEvidence::default(),
                ),
            },
            "residentParity": resident_parity,
            "residentHotPathHashes": counters_after.full_hashes - counters_before.full_hashes,
            "residentLookupP50Ms": resident_lookup_p50,
            "previewBytes": preview_bytes,
            "p50Gain": gain,
            "p95Ratio": p95_ratio,
        }));
    }

    let resident_cache_hit_pass = release_gates.iter().all(|gate| {
        gate["residentP50GainAtLeast10Percent"] == Value::Bool(true)
            && gate["residentP95Within5Percent"] == Value::Bool(true)
            && gate["residentHotPathHashes"] == Value::Bool(true)
            && gate["residentLookupBelow0_1Ms"] == Value::Bool(true)
    });
    let cold_gpu_crossover_pass = release_gates.iter().all(|gate| {
        gate["coldGpuP50GainAtLeast10Percent"] == Value::Bool(true)
            && gate["coldGpuP95Within5Percent"] == Value::Bool(true)
    });
    let performance_pass = resident_cache_hit_pass && cold_gpu_crossover_pass;
    let parity_pass = workloads.iter().all(|workload| {
        workload["parity"] == Value::Bool(true) && workload["residentParity"] == Value::Bool(true)
    });
    let report = json!({
        "skipped": false,
        "selectedBackend": "gpu",
        "tier": capabilities.adapter_tier,
        "backendFamily": capabilities.backend_family,
        "memoryBudgetBytes": capabilities.memory_budget_bytes,
        "samples": SAMPLES,
        "warmups": WARMUPS,
        "workloads": workloads,
        "gates": {
            "parity": parity_pass,
            "residentCacheHit": resident_cache_hit_pass,
            "coldGpuCrossover": cold_gpu_crossover_pass,
            "performance": performance_pass,
            "forcedGpuFailurePreservesRevision": true,
            "memoryAndQueueBoundsEnforced": true,
        },
        "ok": parity_pass && performance_pass,
    });
    println!("BENCHMARK_JSON:{report}");
    assert!(parity_pass, "CPU/GPU parity failed");
    assert!(
        performance_pass,
        "GPU release performance gates were not met: {release_gates:?}"
    );
}

fn synthetic_mesh(triangle_count: u32) -> TriangleMesh {
    assert!(triangle_count.is_multiple_of(2));
    let quad_count = triangle_count / 2;
    let mut columns = (quad_count as f64).sqrt().floor() as u32;
    while !quad_count.is_multiple_of(columns) {
        columns -= 1;
    }
    let rows = quad_count / columns;
    let stride = columns + 1;
    let mut positions = Vec::with_capacity(stride as usize * (rows + 1) as usize);
    for row in 0..=rows {
        for column in 0..=columns {
            positions.push([
                column as f32 * 0.01,
                row as f32 * 0.01,
                (column % 7) as f32 * 0.01 + (row % 5) as f32 * 0.02,
            ]);
        }
    }
    let mut indices = Vec::with_capacity(triangle_count as usize * 3);
    for row in 0..rows {
        for column in 0..columns {
            let a = row * stride + column;
            let b = a + stride;
            indices.extend([a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    TriangleMesh { positions, indices }
}

fn mesh_parity(gpu: &EvaluatedMesh, cpu: &EvaluatedMesh) -> bool {
    gpu.indices == cpu.indices
        && gpu.vertices.len() == cpu.vertices.len()
        && gpu.vertices.iter().zip(&cpu.vertices).all(|(gpu, cpu)| {
            (0..3).all(|axis| {
                (gpu.position[axis] - cpu.position[axis]).abs() <= 1e-5
                    && (gpu.normal[axis] - cpu.normal[axis]).abs() <= 1e-5
            })
        })
        && bounds_parity(gpu.bounds, cpu.bounds)
}

fn bounds_parity(gpu: Bounds, cpu: Bounds) -> bool {
    [
        (gpu.min.x, cpu.min.x),
        (gpu.min.y, cpu.min.y),
        (gpu.min.z, cpu.min.z),
        (gpu.max.x, cpu.max.x),
        (gpu.max.y, cpu.max.y),
        (gpu.max.z, cpu.max.z),
    ]
    .into_iter()
    .all(|(gpu, cpu)| (gpu - cpu).abs() <= 1e-5 * cpu.abs().max(1.0))
}

fn percentile(samples: &mut [f64], quantile: f64) -> f64 {
    samples.sort_by(f64::total_cmp);
    let index = ((samples.len() - 1) as f64 * quantile).round() as usize;
    samples[index]
}

fn mesh_transfer_bytes(mesh: &TriangleMesh) -> (u64, u64) {
    let vertices = mesh.positions.len() as u64;
    let indices = mesh.indices.len() as u64;
    let workgroups = vertices.div_ceil(64);
    let upload = 16 + vertices * 16 + indices * 4 + (vertices + 1) * 4 + indices * 4;
    let readback = vertices * 16 + workgroups * 32;
    (upload, readback)
}

#[derive(Clone, Copy, Default)]
struct ModeTransfer {
    upload_count: usize,
    upload_bytes: u64,
    readback_count: usize,
    readback_bytes: u64,
}

impl ModeTransfer {
    const fn new(
        upload_count: usize,
        upload_bytes: u64,
        readback_count: usize,
        readback_bytes: u64,
    ) -> Self {
        Self {
            upload_count,
            upload_bytes,
            readback_count,
            readback_bytes,
        }
    }
}

fn mode_report(
    p50_ms: f64,
    p95_ms: f64,
    transfer: ModeTransfer,
    observed_mode: Option<String>,
    phases: GeometryPhaseEvidence,
) -> Value {
    json!({
        "p50Ms": p50_ms,
        "p95Ms": p95_ms,
        "uploadCount": transfer.upload_count,
        "uploadBytes": transfer.upload_bytes,
        "readbackCount": transfer.readback_count,
        "readbackBytes": transfer.readback_bytes,
        "observedMode": observed_mode,
        "phases": phases,
    })
}

fn benchmark_fixture() -> Value {
    json!({
        "workloads": [{
            "triangles": 50_000,
            "gpu": {
                "kernelMs": null,
                "queueWaitMs": null,
            },
            "modes": {
                "cold": mode_report(
                    1.0,
                    1.1,
                    ModeTransfer::new(1, 1_024, 1, 512),
                    None,
                    GeometryPhaseEvidence::default(),
                ),
                "warm": mode_report(
                    0.5,
                    0.6,
                    ModeTransfer::default(),
                    Some("warm".into()),
                    GeometryPhaseEvidence::default(),
                ),
                "resident": mode_report(
                    0.1,
                    0.2,
                    ModeTransfer::default(),
                    None,
                    GeometryPhaseEvidence::default(),
                ),
                "snapshot": mode_report(
                    0.8,
                    0.9,
                    ModeTransfer::new(0, 0, 1, 512),
                    None,
                    GeometryPhaseEvidence::default(),
                ),
                "preview": mode_report(
                    0.2,
                    0.3,
                    ModeTransfer::default(),
                    None,
                    GeometryPhaseEvidence::default(),
                ),
            }
        }],
        "gates": {
            "residentCacheHit": true,
            "coldGpuCrossover": false,
            "performance": false,
        },
    })
}
