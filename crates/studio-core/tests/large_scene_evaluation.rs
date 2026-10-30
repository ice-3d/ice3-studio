mod support;

use studio_core::large_scene::evaluate::evaluate_large_scene;
use support::shared_pump_document;

#[test]
fn identical_instances_share_one_mesh_with_stable_part_identity() {
    let document = shared_pump_document();
    let evaluated = evaluate_large_scene(&document).unwrap();
    assert_eq!(evaluated.nodes.len(), 2);
    assert_eq!(evaluated.meshes.len(), 1);
    assert_eq!(evaluated.nodes[0].id, "node-000001/body");
    assert_eq!(evaluated.nodes[0].owner_id, "node-000001");
    assert_eq!(evaluated.nodes[1].id, "node-000002/body");
    assert_eq!(evaluated.stats.effective_triangles, 24);
    assert_eq!(evaluated.stats.unique_triangles, 12);
}
