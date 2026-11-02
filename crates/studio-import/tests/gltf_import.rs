use std::path::{Path, PathBuf};

use studio_import::{GltfImporter, SourceFormat};

#[test]
fn imports_gltf_geometry_hierarchy_and_selection_ids() {
    let scene = GltfImporter::new()
        .import_file(&fixture("minimal.glb"), "gltf-fixture")
        .unwrap();
    assert_eq!(scene.source_format, SourceFormat::Gltf);
    assert_eq!(scene.nodes.len(), 1);
    assert_eq!(scene.meshes.len(), 1);
    assert_eq!(scene.meshes[0].primitives[0].indices, [0, 1, 2]);
    assert_eq!(
        scene.nodes[0].extras["ice3"]["selectionId"],
        "gltf-fixture/node/node-000000"
    );
}

#[test]
fn preserves_a_mesh_shared_by_two_nodes() {
    let scene = GltfImporter::new()
        .import_file(&fixture("shared-mesh.gltf"), "shared")
        .unwrap();
    assert_eq!(scene.meshes.len(), 1);
    assert_eq!(
        scene
            .nodes
            .iter()
            .filter(|node| node.mesh == Some(0))
            .count(),
        2
    );
    assert_eq!(scene.nodes[0].parent, None);
    assert_eq!(scene.nodes[1].parent, None);
}

fn fixture(name: &str) -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    match name {
        "shared-mesh.gltf" => manifest.join("tests/fixtures/gltf/shared-mesh.gltf"),
        "minimal.glb" => manifest.join("../../fixtures/source-import/gltf/minimal.glb"),
        _ => panic!("unknown fixture"),
    }
}
