use std::collections::BTreeMap;
use std::process::Command;

use studio_core::document::{Document, NodeContent, SceneNode};
use studio_core::geometry::TriangleMesh;
use studio_core::ids::NodeId;
use studio_core::math::{Transform, Vec3};
use studio_project::{ProjectStore, WorkspacePath};

#[test]
fn exports_a_runtime_tileset_bundle() {
    let root = tempfile::tempdir().unwrap();
    let projects = root.path().join("projects");
    std::fs::create_dir_all(&projects).unwrap();
    let project = projects.join("cli-fixture");
    let workspace = WorkspacePath::new(&projects, "cli-fixture").unwrap();
    {
        let store = ProjectStore::create(workspace).unwrap();
        store.write_snapshot(&fixture_document()).unwrap();
    }
    let output = root.path().join("runtime");
    let binary = env!("CARGO_BIN_EXE_studio-cli");
    let result = Command::new(binary)
        .args([
            "export",
            "tileset",
            "--project",
            project.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--profile",
            "agent-console-v1",
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    for item in [
        "tileset.json",
        "parts",
        "coarse",
        "bindings.json",
        "export.json",
    ] {
        assert!(output.join(item).exists(), "{item} is missing");
    }
    assert!(!output.join("source.glb").exists());
    assert!(!output.join("parts/source.glb").exists());
}

#[test]
fn exports_a_runtime_tileset_with_a_consumer_asset_id() {
    let root = tempfile::tempdir().unwrap();
    let projects = root.path().join("projects");
    std::fs::create_dir_all(&projects).unwrap();
    let project = projects.join("cli-fixture");
    let workspace = WorkspacePath::new(&projects, "cli-fixture").unwrap();
    {
        let store = ProjectStore::create(workspace).unwrap();
        store.write_snapshot(&fixture_document()).unwrap();
    }
    let output = root.path().join("runtime");
    let result = Command::new(env!("CARGO_BIN_EXE_studio-cli"))
        .args([
            "export",
            "tileset",
            "--project",
            project.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--asset-id",
            "consumer-asset",
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );

    let tileset: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output.join("tileset.json")).unwrap()).unwrap();
    assert_eq!(
        tileset["asset"]["extras"]["ice3"]["assetId"],
        "consumer-asset"
    );
    let bindings: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output.join("bindings.json")).unwrap()).unwrap();
    assert_eq!(bindings["assetId"], "consumer-asset");
    assert_eq!(
        bindings["bindings"][0]["selectionId"],
        "consumer-asset/equipment/AE-101"
    );
}

#[test]
fn unsupported_profile_uses_the_tileset_diagnostic() {
    let root = tempfile::tempdir().unwrap();
    let projects = root.path().join("projects");
    std::fs::create_dir_all(&projects).unwrap();
    let project = projects.join("cli-fixture");
    let workspace = WorkspacePath::new(&projects, "cli-fixture").unwrap();
    ProjectStore::create(workspace)
        .unwrap()
        .write_snapshot(&fixture_document())
        .unwrap();

    let result = Command::new(env!("CARGO_BIN_EXE_studio-cli"))
        .args([
            "export",
            "tileset",
            "--project",
            project.to_str().unwrap(),
            "--output",
            root.path().join("runtime").to_str().unwrap(),
            "--profile",
            "unsupported",
        ])
        .output()
        .unwrap();

    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("ICE3_STUDIO_ASSET_TILESET_INVALID"));
}

fn fixture_document() -> Document {
    let mut document = Document::new("cli-fixture");
    document.scene.nodes_mut().push(SceneNode {
        id: NodeId::new("node", 1).unwrap(),
        name: "wall".into(),
        parent_id: None,
        transform: Transform {
            translation: Vec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            ..Transform::IDENTITY
        },
        material_id: None,
        content: NodeContent::CustomMesh(TriangleMesh {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            indices: vec![0, 1, 2],
        }),
        metadata: BTreeMap::from([("equipmentTag".into(), "plant/equipment/AE-101".into())]),
        visible: true,
    });
    document
}
