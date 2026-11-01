mod common;

use common::{fixture_source, mesh, mesh_with_material, node};
use serde_json::json;
use studio_pipeline::lod::{
    CoarseChunkUnit, FitSamples, SemanticProxyMode, Simplifier, SimplifyOptions,
    create_coarse_chunks, create_semantic_proxy, simplify_geometry,
};
use studio_pipeline::{AssetProfile, build_with_profile};

#[test]
fn simplification_respects_device_triangle_budget() {
    let (positions, indices) = grid(20);
    let result = simplify_geometry(
        &positions,
        &indices,
        &SimplifyOptions {
            target_triangles: 120,
            minimum_triangles: 1,
        },
        &Simplifier,
    );
    assert!(result.triangle_count <= 120);
    assert!(result.error_meters.is_finite());
}

#[test]
fn preserves_source_when_simplifier_rejects_geometry() {
    let result = simplify_geometry(
        &[f32::NAN, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        &[0, 1, 2],
        &SimplifyOptions {
            target_triangles: 1,
            minimum_triangles: 1,
        },
        &Simplifier,
    );
    assert!(result.preserved_source);
    assert_eq!(result.source_triangles, result.triangle_count);
    assert!(result.warning.is_some());
}

#[test]
fn creates_an_opt_in_semantic_proxy() {
    let proxy = create_semantic_proxy(
        &FitSamples {
            vertices: vec![-1.0, -0.5, -1.0, 1.0, -0.5, -1.0, 0.0, 0.5, 0.0],
            indices: vec![0, 1, 2],
        },
        SemanticProxyMode::Auto,
    )
    .unwrap()
    .unwrap();
    assert_eq!(proxy.kind, "obb");
    assert_eq!(proxy.triangle_count, 12);
}

#[test]
fn coarse_chunks_keep_selection_identity() {
    let source = fixture_source(
        json!([
            mesh_with_material("mesh-a", 0),
            mesh_with_material("mesh-b", 1)
        ]),
        json!([
            node("node-000001/wall-a", 0, 0.0),
            node("node-000001/wall-b", 1, 2.0)
        ]),
    );
    let artifact = build_with_profile(
        &source,
        "agent-console-test",
        &AssetProfile::agent_console_v1(),
    )
    .unwrap();
    assert!(artifact.all_selection_ids().contains("node-000001/wall-a"));
    assert!(artifact.all_selection_ids().contains("node-000001/wall-b"));
}

#[test]
fn static_batching_reduces_draws_without_changing_selection() {
    let source = fixture_source(
        json!([
            mesh_with_material("mesh-a", 0),
            mesh_with_material("mesh-b", 1)
        ]),
        json!([node("node-a", 0, 0.0), node("node-b", 1, 2.0)]),
    );
    let before = vec!["node-a".to_owned(), "node-b".to_owned()];
    let artifact =
        build_with_profile(&source, "batchable", &AssetProfile::agent_console_v1()).unwrap();
    assert!(artifact.stats.render_batch_count > 1);
    assert_eq!(artifact.sorted_selection_ids(), before);
}

#[test]
fn coarse_files_fall_back_when_a_unit_cannot_be_simplified() {
    let source = fixture_source(
        json!([mesh("mesh-a"), mesh("mesh-b")]),
        json!([node("node-a", 0, 0.0), node("node-b", 1, 2.0)]),
    );
    let mut profile = AssetProfile::agent_console_v1();
    profile.max_triangles = 4;
    profile.lod_max_triangles = 4;
    profile.lod_min_unit_triangles = 1;
    let artifact = build_with_profile(&source, "coarse", &profile).unwrap();
    let coarse = artifact
        .files
        .iter()
        .filter(|file| file.relative_path.starts_with("coarse/"))
        .collect::<Vec<_>>();
    assert!(!coarse.is_empty());
    let triangles = coarse
        .iter()
        .map(|file| part_triangles(&file.bytes))
        .sum::<u64>();
    assert_eq!(
        triangles,
        artifact.stats.source_triangle_count,
        "coarse triangles {triangles}, files {:?}",
        coarse
            .iter()
            .map(|file| (&file.relative_path, file.byte_length))
            .collect::<Vec<_>>()
    );
}

#[test]
fn coarse_chunk_planning_respects_the_byte_budget() {
    let chunks = create_coarse_chunks(
        &[
            CoarseChunkUnit {
                unit_id: "a".into(),
                bytes: 3,
            },
            CoarseChunkUnit {
                unit_id: "b".into(),
                bytes: 3,
            },
            CoarseChunkUnit {
                unit_id: "c".into(),
                bytes: 8,
            },
        ],
        5,
    )
    .unwrap();
    assert!(chunks.iter().any(|chunk| chunk.oversized));
    assert!(
        chunks
            .iter()
            .filter(|chunk| !chunk.oversized)
            .all(|chunk| chunk.bytes <= 5)
    );
}

fn grid(size: usize) -> (Vec<f32>, Vec<u32>) {
    let mut positions = Vec::new();
    for y in 0..=size {
        for x in 0..=size {
            positions.extend_from_slice(&[x as f32, y as f32, ((x + y) % 3) as f32 * 0.1]);
        }
    }
    let mut indices = Vec::new();
    let stride = size + 1;
    for y in 0..size {
        for x in 0..size {
            let a = (y * stride + x) as u32;
            let b = a + 1;
            let c = a + stride as u32;
            let d = c + 1;
            indices.extend_from_slice(&[a, b, c, b, d, c]);
        }
    }
    (positions, indices)
}

fn part_triangles(bytes: &[u8]) -> u64 {
    let source = studio_assets::read_glb_bytes(bytes, &std::env::temp_dir()).unwrap();
    source.json["accessors"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|accessor| accessor["type"] == "SCALAR")
        .filter_map(|accessor| accessor["count"].as_u64())
        .sum::<u64>()
        / 3
}
