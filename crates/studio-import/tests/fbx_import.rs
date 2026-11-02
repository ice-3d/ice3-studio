use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use glam::Mat4;
use serde_json::Value;
use studio_assets::canonical::PreparedGlbOptions;
use studio_import::{FbxImporter, NativeImportLimits, SourceFormat, SourceImporter, SourceScene};

#[test]
fn imports_ascii_and_binary_fbx_with_the_same_semantics() {
    let ascii = FbxImporter::new()
        .import_file(&toolchain_fixture("fbx/synthetic-ascii.fbx"), "fbx")
        .unwrap();
    let binary = FbxImporter::new()
        .import_file(&toolchain_fixture("fbx/synthetic-binary.fbx"), "fbx")
        .unwrap();

    assert_eq!(semantic_summary(&ascii), semantic_summary(&binary));
    assert_eq!(ascii.source_format, SourceFormat::Fbx);
    assert!(ascii.nodes.iter().any(|node| node.name == "Primary Pump"));
    let steel = ascii
        .materials
        .iter()
        .find(|material| material.name == "Pump Steel")
        .unwrap();
    assert!(steel.base_color[2] > steel.base_color[0]);
}

#[test]
fn preserves_fbx_shared_mesh_instances() {
    let scene = FbxImporter::new()
        .import_file(&toolchain_fixture("fbx/synthetic-binary.fbx"), "fbx")
        .unwrap();
    let mesh_counts = scene.nodes.iter().filter_map(|node| node.mesh).fold(
        BTreeMap::new(),
        |mut counts, mesh| {
            *counts.entry(mesh).or_insert(0_usize) += 1;
            counts
        },
    );
    assert!(mesh_counts.values().any(|count| *count > 1));
}

#[test]
fn preserves_custom_properties_and_instance_metadata() {
    let scene = FbxImporter::new()
        .import_file(&toolchain_fixture("fbx/synthetic-binary.fbx"), "fbx")
        .unwrap();
    let pump = scene
        .nodes
        .iter()
        .find(|node| node.name == "Primary Pump")
        .unwrap();
    let fbx = &pump.extras["ice3"]["fbx"];

    assert_eq!(fbx["objectType"], "MESH");
    assert_eq!(fbx["customProperties"]["EquipmentTag"], "P-101");
    assert_eq!(fbx["customProperties"]["DesignFlow"], 12.5);
    assert!(fbx["meshData"].as_str().is_some());
    assert!(fbx["instanceGroup"].as_str().is_some());
}

#[test]
fn uses_one_local_origin_for_georeferenced_fbx() {
    let scene = FbxImporter::new()
        .import_file(&toolchain_fixture("fbx/large-origin.fbx"), "large-fbx")
        .unwrap();
    let origin = scene.asset_extras["ice3"]["fbx"]["sourceOrigin"]
        .as_array()
        .unwrap();
    assert!(origin[0].as_f64().unwrap() > 1_000_000.0);
    assert_eq!(origin[1].as_f64(), Some(0.0));

    let bounds = world_bounds(&scene);
    assert!(bounds.min[0].abs() < 1e-3, "{bounds:?}");
    assert!(bounds.min[2].abs() < 1e-3, "{bounds:?}");
    assert!(bounds.max[0].abs() < 20.0, "{bounds:?}");
    assert!(bounds.max[2].abs() < 20.0, "{bounds:?}");
    assert_eq!(scene.asset_extras["ice3"]["fbx"]["unitAssumption"], "meter");
}

#[test]
fn rejects_empty_truncated_and_corrupt_fbx_with_stable_codes() {
    let empty = FbxImporter::new()
        .import_file(&toolchain_fixture("fbx/empty.fbx"), "empty")
        .unwrap_err();
    assert_eq!(empty.code(), "ICE3_STUDIO_IMPORT_EMPTY_GEOMETRY");

    let truncated = FbxImporter::new()
        .import_file(&toolchain_fixture("fbx/truncated.fbx"), "truncated")
        .unwrap_err();
    assert_eq!(truncated.code(), "ICE3_STUDIO_IMPORT_INVALID_SOURCE");

    let corrupt_path = tempfile::NamedTempFile::new().unwrap().into_temp_path();
    std::fs::write(&corrupt_path, b"not an fbx file").unwrap();
    let corrupt = FbxImporter::new()
        .import_file(&corrupt_path, "corrupt")
        .unwrap_err();
    assert_eq!(corrupt.code(), "ICE3_STUDIO_IMPORT_INVALID_SOURCE");
}

#[test]
fn exposes_the_fbx_adapter_contract() {
    let importer = FbxImporter::new();
    assert_eq!(importer.id(), "fbx");
    assert_eq!(importer.extensions(), &[".fbx"]);
    assert!(importer.capabilities().instances);
    assert!(importer.capabilities().semantic_metadata);
}

#[test]
fn applies_the_native_triangle_limit_before_mapping() {
    let error = FbxImporter::new()
        .import_file_with_limits(
            &toolchain_fixture("fbx/synthetic-binary.fbx"),
            "limited",
            NativeImportLimits {
                max_triangles: 1,
                ..NativeImportLimits::default()
            },
        )
        .unwrap_err();
    assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_LIMIT_EXCEEDED");
}

#[test]
fn maps_to_a_valid_prepared_glb_with_studio_assets() {
    let scene = FbxImporter::new()
        .import_file(
            &toolchain_fixture("fbx/synthetic-binary.fbx"),
            "fbx-prepared",
        )
        .unwrap();
    let canonical = studio_import::to_canonical(&scene);
    let bytes = studio_assets::canonical::write_prepared_glb(
        &canonical,
        &PreparedGlbOptions {
            asset_id: "fbx-prepared".into(),
            source_format: "fbx".into(),
            source_digest: "fixture".into(),
        },
    )
    .unwrap();
    let source = studio_assets::read_glb_bytes(&bytes, &std::env::temp_dir()).unwrap();
    let profile = studio_assets::validate_prepared_profile(&source).unwrap();
    assert_eq!(profile.asset_id, "fbx-prepared");
    assert_eq!(
        source.json["nodes"][5]["extras"]["ice3"]["selectionId"],
        "fbx-prepared/fbx/node/5"
    );
}

fn semantic_summary(scene: &SourceScene) -> SemanticSummary {
    SemanticSummary {
        nodes: scene
            .nodes
            .iter()
            .map(|node| {
                (
                    node.name.clone(),
                    node.parent.clone(),
                    node.mesh,
                    quantized_matrix(node.matrix),
                    node.extras["ice3"]["selectionId"]
                        .as_str()
                        .unwrap()
                        .to_owned(),
                )
            })
            .collect(),
        meshes: scene
            .meshes
            .iter()
            .map(|mesh| {
                (
                    mesh.id.clone(),
                    mesh.primitives
                        .iter()
                        .map(|primitive| {
                            (
                                primitive.positions.len(),
                                primitive.indices.len(),
                                primitive.material,
                            )
                        })
                        .collect(),
                )
            })
            .collect(),
        materials: scene
            .materials
            .iter()
            .map(|material| {
                (
                    material.id.clone(),
                    material.name.clone(),
                    material.base_color.map(quantize),
                    quantize(material.opacity),
                )
            })
            .collect(),
        bounds: quantized_bounds(world_bounds(scene)),
        warnings: scene
            .warnings
            .iter()
            .map(|warning| warning.code.clone())
            .collect::<BTreeSet<_>>(),
        source_origin: scene.asset_extras["ice3"]["fbx"]["sourceOrigin"].clone(),
    }
}

#[derive(Debug, PartialEq)]
struct SemanticSummary {
    nodes: Vec<NodeSummary>,
    meshes: Vec<MeshSummary>,
    materials: Vec<(String, String, [i64; 3], i64)>,
    bounds: [i64; 6],
    warnings: BTreeSet<String>,
    source_origin: Value,
}

type NodeSummary = (String, Option<String>, Option<usize>, [i64; 16], String);
type PrimitiveSummary = (usize, usize, Option<usize>);
type MeshSummary = (String, Vec<PrimitiveSummary>);

fn quantized_matrix(matrix: [f32; 16]) -> [i64; 16] {
    matrix.map(quantize)
}

fn quantize(value: f32) -> i64 {
    (f64::from(value) * 1_000_000.0).round() as i64
}

fn world_bounds(scene: &SourceScene) -> Bounds {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    let indices = scene
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.id.as_str(), index))
        .collect::<BTreeMap<_, _>>();
    let mut world_matrices = Vec::with_capacity(scene.nodes.len());
    for node in &scene.nodes {
        let local = Mat4::from_cols_array(&node.matrix);
        let world = node
            .parent
            .as_deref()
            .and_then(|parent| indices.get(parent))
            .map_or(local, |parent| world_matrices[*parent] * local);
        world_matrices.push(world);
        let Some(mesh_index) = node.mesh else {
            continue;
        };
        for primitive in &scene.meshes[mesh_index].primitives {
            for position in &primitive.positions {
                let point = world.transform_point3(glam::Vec3::from_array(*position));
                for axis in 0..3 {
                    min[axis] = min[axis].min(point[axis]);
                    max[axis] = max[axis].max(point[axis]);
                }
            }
        }
    }
    Bounds { min, max }
}

fn quantized_bounds(bounds: Bounds) -> [i64; 6] {
    [
        quantize(bounds.min[0]),
        quantize(bounds.min[1]),
        quantize(bounds.min[2]),
        quantize(bounds.max[0]),
        quantize(bounds.max[1]),
        quantize(bounds.max[2]),
    ]
}

#[derive(Debug)]
struct Bounds {
    min: [f32; 3],
    max: [f32; 3],
}

fn toolchain_fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../ice3-toolchain/tests/fixtures")
        .join(name)
}
