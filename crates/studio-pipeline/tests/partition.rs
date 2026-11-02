mod common;

use common::{fixture_source, mesh, node, scene_glb};
use serde_json::{Value, json};
use studio_assets::read_glb_bytes;
use studio_pipeline::{analyze_units, create_partition_plan};

#[test]
fn groups_nodes_that_share_one_mesh() {
    let source = fixture_source(
        json!([mesh("mesh-shared")]),
        json!([node("node-a", 0, 0.0), node("node-b", 0, 2.0)]),
    );
    let units = analyze_units(&source).unwrap();
    assert_eq!(units.len(), 1);
    assert_eq!(units[0].mesh_keys.len(), 2);
    assert_eq!(units[0].node_ids, ["node-a", "node-b"]);
}

#[test]
fn partition_plan_is_stable_when_node_order_changes() {
    let meshes = json!([mesh("mesh-a"), mesh("mesh-b")]);
    let first_nodes = json!([node("node-a", 0, 0.0), node("node-b", 1, 2.0)]);
    let second_nodes = json!([node("node-b", 1, 2.0), node("node-a", 0, 0.0)]);
    let first = create_partition_plan(
        &analyze_units(&fixture_source(meshes.clone(), first_nodes)).unwrap(),
        500_000,
    );
    let second = create_partition_plan(
        &analyze_units(&fixture_source(meshes, second_nodes)).unwrap(),
        500_000,
    );
    assert_eq!(first, second);
}

#[test]
fn splits_on_longest_axis_under_triangle_budget() {
    let meshes = json!([
        mesh("mesh-a"),
        mesh("mesh-b"),
        mesh("mesh-c"),
        mesh("mesh-d")
    ]);
    let nodes = json!([
        node("node-a", 0, 0.0),
        node("node-b", 1, 1.0),
        node("node-c", 2, 2.0),
        node("node-d", 3, 3.0)
    ]);
    let source = read_glb_bytes(&scene_glb(meshes, nodes, 600), &std::env::temp_dir()).unwrap();
    let plan = create_partition_plan(&analyze_units(&source).unwrap(), 1_000);
    assert!(plan.leaves.iter().all(|leaf| leaf.triangle_count <= 1_000));
    assert!(!plan.leaves.is_empty());
}

#[test]
fn rejects_a_scene_without_mesh_geometry() {
    let empty_mesh: Value = json!({
        "name": "empty",
        "primitives": [],
        "extras": {"ice3": {"id": "empty"}}
    });
    let source = fixture_source(json!([empty_mesh]), json!([node("node-empty", 0, 0.0)]));
    assert_eq!(
        analyze_units(&source).unwrap_err().code(),
        "ICE3_STUDIO_PARTITION_EMPTY"
    );
}
