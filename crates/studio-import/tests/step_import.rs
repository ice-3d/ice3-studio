#![cfg(feature = "step")]

use std::path::PathBuf;

use studio_import::{SourceFormat, StepImporter};

#[test]
fn imports_ap242_hierarchy_and_geometry() {
    let scene = StepImporter::new()
        .import_file(&fixture("step/synthetic-skid.step"), "step-fixture")
        .unwrap();
    let names = scene
        .nodes
        .iter()
        .map(|node| node.name.as_str())
        .collect::<Vec<_>>();
    assert!(names.contains(&"SyntheticSkid"));
    assert!(names.contains(&"PrimaryTank"));
    assert!(names.contains(&"TransferPump"));
    assert_eq!(scene.meshes.len(), 2);
    assert_eq!(scene.source_format, SourceFormat::Step);
    assert_eq!(
        scene
            .nodes
            .iter()
            .filter(|node| node.mesh.is_some())
            .count(),
        2
    );
    assert!(
        scene
            .nodes
            .iter()
            .all(|node| node.id.starts_with("step-fixture/step/node/"))
    );
}

fn fixture(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../ice3-toolchain/tests/fixtures")
        .join(relative)
}
