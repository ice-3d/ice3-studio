mod support;

use studio_core::large_scene::cache::MemoryGeometryStore;
use studio_core::large_scene::evaluate::evaluate_large_scene_with_cache;
use studio_core::math::Vec3;
use support::shared_pump_document;

#[test]
fn transform_only_change_reuses_the_unchanged_shared_mesh() {
    let mut store = MemoryGeometryStore::default();
    let first_document = shared_pump_document();
    let first = evaluate_large_scene_with_cache(&first_document, &mut store).unwrap();

    let mut second_document = first_document;
    second_document.scene.nodes_mut()[1].transform.translation = Vec3 {
        x: 9.0,
        y: 0.0,
        z: 0.0,
    };
    let second = evaluate_large_scene_with_cache(&second_document, &mut store).unwrap();

    assert_eq!(second.stats.cache_hits, 1);
    assert_eq!(second.stats.generated_meshes, 0);
    assert_eq!(
        first.meshes.keys().collect::<Vec<_>>(),
        second.meshes.keys().collect::<Vec<_>>()
    );
}
