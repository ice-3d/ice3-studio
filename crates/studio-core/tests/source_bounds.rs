use studio_core::large_scene::evaluate::{
    aggregate_source_bounds, evaluate_large_scene, evaluated_scene_bounds,
};

mod support;

use support::shared_pump_document;

#[test]
fn aggregates_derived_part_bounds_per_source_owner() {
    let document = shared_pump_document();
    let scene = evaluate_large_scene(&document).expect("scene evaluates");
    let derived_bounds = evaluated_scene_bounds(&scene);
    let source_bounds = aggregate_source_bounds(&scene, &derived_bounds);

    // 两台泵实例的源节点获得聚合 world bounds。
    for source_id in ["node-000001", "node-000002"] {
        let bounds = source_bounds
            .get(source_id)
            .unwrap_or_else(|| panic!("missing aggregated bounds for {source_id}"));
        assert!(bounds.min.x < bounds.max.x);
        assert!(bounds.min.y < bounds.max.y);
        assert!(bounds.min.z < bounds.max.z);
    }

    // 聚合 bounds 包含该源节点所有派生部件的 bounds。
    for node in &scene.nodes {
        let owner = node
            .owner_id
            .split('/')
            .next()
            .filter(|candidate| !candidate.is_empty())
            .unwrap_or(node.owner_id.as_str());
        let Some(part_bounds) = derived_bounds.get(node.id.as_str()) else {
            continue;
        };
        let Some(aggregate) = source_bounds.get(owner) else {
            panic!("derived part {} has no aggregated owner {owner}", node.id);
        };
        assert!(
            aggregate.min.x <= part_bounds.min.x
                && aggregate.min.y <= part_bounds.min.y
                && aggregate.min.z <= part_bounds.min.z
                && part_bounds.max.x <= aggregate.max.x
                && part_bounds.max.y <= aggregate.max.y
                && part_bounds.max.z <= aggregate.max.z,
            "aggregated bounds for {owner} do not contain part {}",
            node.id
        );
    }
}

#[test]
fn aggregate_bounds_on_an_empty_scene_is_empty() {
    let mut document = shared_pump_document();
    document.scene.nodes_mut().clear();
    let scene = evaluate_large_scene(&document).expect("scene evaluates");
    let derived_bounds = evaluated_scene_bounds(&scene);
    let source_bounds = aggregate_source_bounds(&scene, &derived_bounds);
    assert!(source_bounds.is_empty());
}
