use std::collections::BTreeMap;

use studio_assets::read_glb_bytes;
use studio_core::document::{Document, NodeContent, SceneNode};
use studio_core::geometry::{Bounds, EvaluatedMesh, TriangleMesh, Vertex};
use studio_core::ids::{NodeId, Revision};
use studio_core::large_scene::evaluate::{EvaluatedScene, EvaluatedSceneNode};
use studio_core::math::{Transform, Vec3 as StudioVec3};
use studio_project::{ProjectStore, WorkspacePath};
use studio_server::runtime_tileset::{
    BindingManifest, RuntimeTilesetError, RuntimeTilesetExporter,
};

#[test]
fn groups_all_parts_sharing_a_tag_into_one_binding() {
    let mut scene = fixture_scene();
    scene.nodes[0]
        .metadata
        .insert("equipmentTag".into(), "plant/equipment/AE-101".into());
    scene.nodes[0]
        .metadata
        .insert("train".into(), "Train1".into());
    scene.nodes[0].name = "AE-101 wall".into();
    scene.nodes[1]
        .metadata
        .insert("equipmentTag".into(), "plant/equipment/AE-101".into());
    scene.nodes[1]
        .metadata
        .insert("train".into(), "Train1".into());
    scene.nodes[1].name = "AE-101 platform".into();

    let manifest = BindingManifest::from_evaluated_scene("fixture", 1, &scene);
    assert_eq!(manifest.bindings.len(), 1);
    assert_eq!(
        manifest.bindings[0].selection_id,
        "fixture/equipment/AE-101"
    );
    assert_eq!(
        manifest.bindings[0].node_selection_ids,
        ["node-000001/wall-a", "node-000001/wall-b"]
    );
    assert_eq!(manifest.bindings[0].area.as_deref(), Some("Train1"));
}

#[test]
fn export_is_deterministic_and_contains_no_source_glb() {
    let first = fixture_export("first").unwrap();
    let second = fixture_export("second").unwrap();
    assert_eq!(first.tileset_digest, second.tileset_digest);
    assert_eq!(first.bundle_digest, second.bundle_digest);
    assert!(!first.output_dir.join("source.glb").exists());
    assert!(first.output_dir.join("parts").is_dir());
    assert!(first.output_dir.join("coarse").exists());
}

#[test]
fn runtime_tileset_parts_and_coarse_include_render_edges() {
    let export = fixture_export("edges").unwrap();
    let geometry_files = export
        .files
        .iter()
        .filter(|file| file.path.starts_with("parts/") || file.path.starts_with("coarse/"))
        .collect::<Vec<_>>();
    assert!(!geometry_files.is_empty());
    for file in geometry_files {
        let bytes = std::fs::read(export.output_dir.join(&file.path)).unwrap();
        let parsed = read_glb_bytes(&bytes, &std::env::temp_dir()).unwrap();
        assert!(
            parsed.json["meshes"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|mesh| mesh["primitives"].as_array().unwrap())
                .any(|primitive| primitive["mode"] == 1),
            "{} has no render edges",
            file.path
        );
    }
    assert!(export.stats.edge_segment_count > 0);
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(export.output_dir.join("export.json")).unwrap())
            .unwrap();
    assert_eq!(
        manifest["stats"]["edgeSegmentCount"],
        export.stats.edge_segment_count
    );
}

#[test]
fn runtime_tileset_diagnostics_use_the_studio_asset_code() {
    assert_eq!(
        studio_pipeline::PipelineError::Tileset("invalid".into()).code(),
        "ICE3_STUDIO_ASSET_TILESET_INVALID"
    );
}

#[test]
fn force_publish_failure_keeps_previous_bundle() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("runtime");
    std::fs::create_dir_all(&output).unwrap();
    std::fs::write(output.join("keep"), "keep").unwrap();
    let project_root = root.path().join("projects");
    std::fs::create_dir_all(&project_root).unwrap();
    let mut document = Document::new("invalid-tileset");
    document.scene.nodes_mut().push(SceneNode {
        id: NodeId::new("node", 1).unwrap(),
        name: "empty group".into(),
        parent_id: None,
        transform: Transform::IDENTITY,
        material_id: None,
        content: NodeContent::Group,
        metadata: BTreeMap::new(),
        visible: true,
    });
    let workspace = WorkspacePath::new(&project_root, "invalid-tileset").unwrap();
    ProjectStore::create(workspace.clone())
        .unwrap()
        .write_snapshot(&document)
        .unwrap();
    let error = RuntimeTilesetExporter::new(output.clone(), true)
        .export(&document, workspace.project_dir())
        .unwrap_err();
    assert!(matches!(error, RuntimeTilesetError::Pipeline(_)));
    assert_eq!(
        std::fs::read_to_string(output.join("keep")).unwrap(),
        "keep"
    );
}

fn fixture_export(
    name: &str,
) -> Result<studio_server::runtime_tileset::RuntimeTilesetExport, RuntimeTilesetError> {
    let root = tempfile::tempdir().unwrap().keep();
    let project_root = root.join("projects");
    std::fs::create_dir_all(&project_root).unwrap();
    let workspace = WorkspacePath::new(&project_root, "runtime-fixture").unwrap();
    let store = ProjectStore::create(workspace).unwrap();
    let mut document = fixture_document();
    document.revision = Revision::new(0);
    store.write_snapshot(&document).unwrap();
    RuntimeTilesetExporter::new(root.join(name), false).export(&document, store.project_dir())
}

fn fixture_document() -> Document {
    let mut document = Document::new("runtime-fixture");
    for (index, name) in ["wall-a", "wall-b"].into_iter().enumerate() {
        let mut metadata = BTreeMap::new();
        metadata.insert("equipmentTag".into(), "plant/equipment/AE-101".into());
        metadata.insert("train".into(), "Train1".into());
        document.scene.nodes_mut().push(SceneNode {
            id: NodeId::new("node", index as u64 + 1).unwrap(),
            name: name.into(),
            parent_id: None,
            transform: Transform {
                translation: StudioVec3 {
                    x: index as f32 * 2.0,
                    ..StudioVec3::default()
                },
                ..Transform::IDENTITY
            },
            material_id: None,
            content: NodeContent::CustomMesh(triangle_mesh()),
            metadata,
            visible: true,
        });
    }
    document
}

fn fixture_scene() -> EvaluatedScene {
    let mesh = mesh();
    EvaluatedScene {
        nodes: vec![
            evaluated_node("node-000001/wall-a", "owner-a", "wall-a", 0.0),
            evaluated_node("node-000001/wall-b", "owner-b", "wall-b", 2.0),
        ],
        meshes: BTreeMap::from([("mesh".into(), mesh)]),
        ..EvaluatedScene::default()
    }
}

fn evaluated_node(id: &str, owner: &str, name: &str, x: f32) -> EvaluatedSceneNode {
    EvaluatedSceneNode {
        id: id.into(),
        owner_id: owner.into(),
        name: name.into(),
        parent_id: None,
        transform: Transform {
            translation: StudioVec3 { x, y: 0.0, z: 0.0 },
            ..Transform::IDENTITY
        },
        material_id: None,
        mesh_key: Some("mesh".into()),
        resident_geometry_id: None,
        metadata: BTreeMap::new(),
        visible: true,
    }
}

fn triangle_mesh() -> TriangleMesh {
    TriangleMesh {
        positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        indices: vec![0, 1, 2],
    }
}

fn mesh() -> EvaluatedMesh {
    let positions = triangle_mesh().positions;
    EvaluatedMesh {
        vertices: positions
            .into_iter()
            .map(|position| Vertex {
                position,
                normal: [0.0, 0.0, 1.0],
            })
            .collect(),
        indices: vec![0, 1, 2],
        bounds: Bounds {
            min: StudioVec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            max: StudioVec3 {
                x: 1.0,
                y: 1.0,
                z: 0.0,
            },
        },
    }
}

#[test]
fn textures_ship_beside_the_tiles_that_reference_them() {
    use studio_core::document::{Material, MaterialSurface};
    use studio_core::ids::MaterialId;

    let root = tempfile::tempdir().unwrap().keep();
    let project_root = root.join("projects");
    std::fs::create_dir_all(&project_root).unwrap();
    let workspace = WorkspacePath::new(&project_root, "textured").unwrap();
    let store = ProjectStore::create(workspace).unwrap();

    let image = b"\x89PNG\r\n\x1a\nnot-really-a-png".to_vec();
    std::fs::create_dir_all(store.project_dir().join("textures")).unwrap();
    std::fs::write(store.project_dir().join("textures/steel.png"), &image).unwrap();

    let mut document = fixture_document();
    document.revision = Revision::new(0);
    let material_id = MaterialId::from("material-000001");
    document.materials.push(Material {
        id: material_id.clone(),
        name: "Steel".into(),
        base_color: [0.6, 0.6, 0.62, 1.0],
        opacity: 1.0,
        edge_color: None,
        shading: None,
        surface: Some(MaterialSurface {
            base_color_texture: Some("textures/steel.png".into()),
            ..MaterialSurface::dielectric()
        }),
    });
    for node in document.scene.nodes_mut() {
        node.material_id = Some(material_id.clone());
    }
    store.write_snapshot(&document).unwrap();

    let output = root.join("bundle");
    let export = RuntimeTilesetExporter::new(output.clone(), false)
        .export(&document, store.project_dir())
        .expect("export");

    let copied = output.join("parts/textures/steel.png");
    assert_eq!(std::fs::read(&copied).unwrap(), image);
    // Every directory that holds tiles needs its own copy: glTF resolves an
    // image relative to the file that references it, and the same source GLB is
    // baked into both the detailed and the coarse LOD.
    for directory in ["parts", "coarse"] {
        let path = output.join(directory);
        let has_tiles = std::fs::read_dir(&path)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .any(|entry| entry.path().extension().is_some_and(|ext| ext == "glb"))
            })
            .unwrap_or(false);
        if !has_tiles {
            continue;
        }
        assert_eq!(
            std::fs::read(path.join("textures/steel.png")).unwrap(),
            image,
            "tile directory {directory} is missing its texture copy",
        );
    }
    let listed = export
        .files
        .iter()
        .find(|file| file.path == "parts/textures/steel.png")
        .expect("texture is listed in the manifest");
    assert_eq!(listed.byte_length as usize, image.len());

    let part = std::fs::read(output.join("parts/part-000.glb")).expect("part glb");
    let parsed = read_glb_bytes(&part, &std::env::temp_dir()).expect("parse part");
    assert_eq!(parsed.json["images"][0]["uri"], "textures/steel.png");
    assert_eq!(
        parsed.json["materials"][0]["pbrMetallicRoughness"]["baseColorTexture"]["index"],
        0
    );
}

#[test]
fn scenery_stays_out_of_the_binding_manifest() {
    // Ground, roads and planting are context: they must not become selectable
    // entities just because they have geometry.
    let mut document = fixture_document();
    // fixture_document tags both walls, so untag one and force the other.
    let nodes = document.scene.nodes_mut();
    nodes[0].metadata.remove("equipmentTag");
    nodes[1].metadata.insert("pickable".into(), "false".into());
    document.revision = Revision::new(0);

    let scene = studio_core::large_scene::evaluate::evaluate_large_scene(&document).unwrap();
    let manifest = BindingManifest::from_evaluated_scene("asset", 0, &scene);
    assert!(
        manifest.bindings.is_empty(),
        "untagged scenery and explicitly unpickable nodes must not be bound",
    );

    let mut document = fixture_document();
    document.revision = Revision::new(0);
    for node in document.scene.nodes_mut() {
        node.metadata.remove("equipmentTag");
    }
    document.scene.nodes_mut()[0]
        .metadata
        .insert("pickable".into(), "true".into());
    let scene = studio_core::large_scene::evaluate::evaluate_large_scene(&document).unwrap();
    let manifest = BindingManifest::from_evaluated_scene("asset", 0, &scene);
    assert_eq!(
        manifest.bindings.len(),
        1,
        "pickable=true keeps a tag-less node"
    );
}
