use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;
use studio_project::reference_asset_store::{
    ReferenceAssetError, ReferenceAssetSource, ReferenceAssetStore,
};
use tempfile::TempDir;

struct Fixture {
    root: TempDir,
}

impl Fixture {
    fn new() -> Self {
        let root = TempDir::new().unwrap();
        fs::create_dir_all(root.path().join("project")).unwrap();
        fs::create_dir_all(root.path().join("refs/tiled")).unwrap();
        fs::write(
            root.path().join("refs/tiled/tileset.json"),
            br#"{"asset":{"version":"1.1"}}"#,
        )
        .unwrap();
        write_manifest(&root.path().join("refs/equipment.json"));
        write_glb(&root.path().join("refs/source.glb"), None);
        write_glb(&root.path().join("refs/prepared.glb"), None);
        Self { root }
    }

    fn project(&self) -> PathBuf {
        self.root.path().join("project")
    }

    fn source(&self) -> ReferenceAssetSource {
        ReferenceAssetSource {
            source_path: PathBuf::from("refs/source.glb"),
            prepared_path: Some(PathBuf::from("refs/prepared.glb")),
            tileset_path: Some(PathBuf::from("refs/tiled")),
            equipment_manifest_path: PathBuf::from("refs/equipment.json"),
        }
    }

    fn store(&self) -> ReferenceAssetStore {
        ReferenceAssetStore::new(self.project())
    }
}

#[test]
fn registers_hashes_and_pages_a_reference_asset() {
    let fixture = Fixture::new();
    let store = fixture.store();
    let registered = store.register(&fixture.source()).unwrap();

    assert_eq!(registered.descriptor.stats.equipment, 204);
    assert_eq!(registered.descriptor.stats.effective_triangles, 5_757_962);
    assert_eq!(
        store.get("asset-000001").unwrap().descriptor,
        registered.descriptor
    );
    let page = store.bindings("asset-000001", None, 25).unwrap();
    assert_eq!(page.items.len(), 25);
    assert_eq!(
        page.items[0].selection_id,
        "ice3-virtual-wwtp/equipment/AE-101"
    );
}

#[test]
fn rejects_paths_and_symlinks_outside_the_workspace() {
    let fixture = Fixture::new();
    let escaped = ReferenceAssetSource {
        source_path: PathBuf::from("../../outside.glb"),
        ..fixture.source()
    };
    let error = fixture.store().register(&escaped).unwrap_err();
    assert!(matches!(error, ReferenceAssetError::PathEscape));

    let outside = fixture
        .root
        .path()
        .parent()
        .unwrap()
        .join(format!("ice3-reference-outside-{}.glb", std::process::id()));
    fs::write(&outside, b"outside").unwrap();
    std::os::unix::fs::symlink(&outside, fixture.root.path().join("refs/linked.glb")).unwrap();
    let linked = ReferenceAssetSource {
        source_path: PathBuf::from("refs/linked.glb"),
        ..fixture.source()
    };
    let error = fixture.store().register(&linked).unwrap_err();
    assert!(matches!(error, ReferenceAssetError::PathEscape));
    fs::remove_file(outside).unwrap();
}

#[test]
fn replacement_with_mismatched_digest_is_atomic() {
    let fixture = Fixture::new();
    write_glb(
        &fixture.root.path().join("refs/tampered.glb"),
        Some("0".repeat(64)),
    );
    let mismatched = ReferenceAssetSource {
        source_path: PathBuf::from("refs/tampered.glb"),
        ..fixture.source()
    };

    let error = fixture.store().register(&mismatched).unwrap_err();
    assert!(matches!(error, ReferenceAssetError::DigestMismatch));
    assert!(!fixture.project().join("references/asset-000001").exists());
}

fn write_manifest(path: &Path) {
    let mut equipment = Vec::new();
    for index in 0..204 {
        let tag = if index == 0 {
            "AE-101".to_owned()
        } else {
            format!("EQ-{index:03}")
        };
        equipment.push(json!({
            "selectionId": format!("ice3-virtual-wwtp/equipment/{tag}"),
            "tag": tag,
            "name": format!("Equipment {index:03}"),
            "stage": "biological",
            "area": "BiologicalTrainA",
            "bounds": {
                "min": [index as f64, 0.0, 0.0],
                "max": [index as f64 + 2.0, 2.0, 2.0]
            }
        }));
    }
    fs::write(
        path,
        serde_json::to_vec(&json!({
            "assetId": "ice3-virtual-wwtp",
            "equipment": equipment
        }))
        .unwrap(),
    )
    .unwrap();
}

fn write_glb(path: &Path, digest_override: Option<String>) {
    let mut ice3 = json!({
        "assetId": "ice3-virtual-wwtp",
        "bounds": {
            "min": [-625.0, -1.0, -390.0],
            "max": [625.0, 31.0, 390.0]
        },
        "stats": {
            "nodeCount": 1761,
            "meshCount": 1632,
            "materialCount": 75
        }
    });
    if let Some(digest) = digest_override {
        ice3["contentDigest"] = json!({
            "algorithm": "sha256",
            "value": digest,
            "covers": "binary-chunk"
        });
    }
    let document = json!({
        "asset": {
            "version": "2.0",
            "extras": { "ice3": ice3 }
        },
        "accessors": [
            { "count": 17_273_886, "componentType": 5125, "type": "SCALAR" }
        ],
        "meshes": [{
            "primitives": [{
                "attributes": { "POSITION": 0 },
                "indices": 0,
                "mode": 4
            }]
        }],
        "nodes": [{ "mesh": 0 }]
    });
    let mut json_bytes = serde_json::to_vec(&document).unwrap();
    while json_bytes.len() % 4 != 0 {
        json_bytes.push(b' ');
    }
    let mut glb = Vec::new();
    glb.extend_from_slice(b"glTF");
    glb.extend_from_slice(&2u32.to_le_bytes());
    glb.extend_from_slice(&((20 + json_bytes.len()) as u32).to_le_bytes());
    glb.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
    glb.extend_from_slice(&0x4E4F_534A_u32.to_le_bytes());
    glb.extend_from_slice(&json_bytes);
    fs::write(path, glb).unwrap();
}
