mod common;

use common::{fixture_source, mesh, node, scene_glb};
use serde_json::json;
use studio_assets::{read_glb_bytes, validate_prepared_profile};
use studio_pipeline::{BuildOptions, build_artifact};

#[test]
fn writes_relative_tileset_content_uris() {
    let source = fixture_source(
        json!([mesh("mesh-a"), mesh("mesh-b")]),
        json!([node("node-a", 0, 0.0), node("node-b", 1, 2.0)]),
    );
    let artifact = build_artifact(
        &source,
        &BuildOptions {
            asset_id: "two-leaves".into(),
            max_triangles: 1,
            max_bytes: 64 * 1024 * 1024,
        },
    )
    .unwrap();
    let json = artifact.tileset_json();
    assert!(json.contains("\"uri\":\"parts/part-000.glb\""));
    assert!(json.contains("\"uri\":\"parts/part-001.glb\""));
    assert!(!json.contains("/Users/"));
    assert_eq!(artifact.files.len(), 2);
}

#[test]
fn preserves_total_source_triangles() {
    let source = read_glb_bytes(
        &scene_glb(
            json!([mesh("mesh-a"), mesh("mesh-b")]),
            json!([node("node-a", 0, 0.0), node("node-b", 1, 2.0)]),
            2,
        ),
        &std::env::temp_dir(),
    )
    .unwrap();
    let artifact = build_artifact(
        &source,
        &BuildOptions {
            asset_id: "ordered".into(),
            max_triangles: 500_000,
            max_bytes: 64 * 1024 * 1024,
        },
    )
    .unwrap();
    assert_eq!(artifact.stats.triangle_count, 4);
    assert_eq!(artifact.files.len(), 1);
    let part = read_glb_bytes(&artifact.files[0].bytes, &std::env::temp_dir()).unwrap();
    assert_eq!(
        validate_prepared_profile(&part).unwrap().asset_id,
        "ordered"
    );
}

#[test]
fn rejects_an_empty_partition() {
    let empty_mesh = json!({
        "name": "empty",
        "primitives": [],
        "extras": {"ice3": {"id": "empty"}}
    });
    let source = fixture_source(json!([empty_mesh]), json!([node("node-empty", 0, 0.0)]));
    let error = build_artifact(
        &source,
        &BuildOptions {
            asset_id: "empty".into(),
            max_triangles: 500_000,
            max_bytes: 64 * 1024 * 1024,
        },
    )
    .unwrap_err();
    assert_eq!(error.code(), "ICE3_STUDIO_PARTITION_EMPTY");
}
