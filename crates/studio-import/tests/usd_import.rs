#![cfg(feature = "usd")]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use glam::Mat4;
use serde_json::Value;
use studio_assets::canonical::PreparedGlbOptions;
use studio_import::{NativeImportLimits, SourceFormat, SourceImporter, SourceScene, UsdImporter};

#[test]
fn imports_ascii_and_binary_usd_with_the_same_semantics() {
    let ascii = UsdImporter::new()
        .import_file(&toolchain_fixture("usd/synthetic-plant.usda"), "usd")
        .unwrap();
    let binary = UsdImporter::new()
        .import_file(&toolchain_fixture("usd/synthetic-plant.usdc"), "usd")
        .unwrap();

    assert_eq!(semantic_summary(&ascii), semantic_summary(&binary));
    assert_eq!(ascii.source_format, SourceFormat::Usd);
    let pump = ascii
        .nodes
        .iter()
        .find(|node| node.name == "PrimaryPump")
        .unwrap();
    assert_eq!(
        pump.extras["ice3"]["usd"]["usdPrimPath"],
        "/Plant/TreatmentLine/PrimaryPump"
    );
    assert!(
        pump.extras["ice3"]["selectionId"]
            .as_str()
            .unwrap()
            .starts_with("usd/usd/node/")
    );
}

#[test]
fn preserves_usd_reference_instances() {
    let scene = UsdImporter::new()
        .import_file(
            &toolchain_fixture("usd/synthetic-plant.usda"),
            "usd-instances",
        )
        .unwrap();
    let pump_indices = scene
        .nodes
        .iter()
        .filter(|node| node.name == "PrimaryPump" || node.name == "PumpInstanceA")
        .filter_map(|node| node.mesh)
        .collect::<BTreeSet<_>>();

    assert_eq!(pump_indices.len(), 1);
    let pump = scene
        .nodes
        .iter()
        .find(|node| node.name == "PumpInstanceA")
        .unwrap();
    assert!(
        pump.extras["ice3"]["usd"]["instanceGroup"].is_string(),
        "{pump:#?}"
    );
}

#[test]
fn preserves_usd_point_instancer_identity() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("instanced.usda");
    std::fs::write(
        &root,
        format!(
            "#usda 1.0\n\
             (\n    metersPerUnit = 1\n    upAxis = \"Y\"\n)\n\
             def Xform \"World\"\n{{\n\
                 def PointInstancer \"Pumps\"\n    {{\n\
                     rel prototypes = </World/Prototypes/Pump>\n\
                     point3f[] positions = [(0, 0, 0), (2, 0, 0), (0, 0, 2), (2, 0, 2)]\n\
                     int[] protoIndices = [0, 0, 0, 0]\n\
                 }}\n\
                 def Scope \"Prototypes\"\n    {{\n\
                     def Mesh \"Pump\"\n        {{\n{}\n        }}\n\
                 }}\n\
             }}\n",
            cube_body()
        ),
    )
    .unwrap();

    let scene = UsdImporter::new()
        .import_file(&root, "point-instances")
        .unwrap();
    let mesh_nodes = scene
        .nodes
        .iter()
        .filter(|node| node.mesh.is_some())
        .collect::<Vec<_>>();

    assert_eq!(mesh_nodes.len(), 4);
    assert_eq!(
        mesh_nodes
            .iter()
            .filter_map(|node| node.mesh)
            .collect::<BTreeSet<_>>()
            .len(),
        1
    );
    assert!(mesh_nodes.iter().all(|node| {
        node.extras["ice3"]["usd"]["instanceGroup"]
            == mesh_nodes[0].extras["ice3"]["usd"]["instanceGroup"]
    }));
}

#[test]
fn maps_distinct_point_instancer_prototypes() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("multi-instanced.usda");
    std::fs::write(
        &root,
        format!(
            "#usda 1.0\n\
             def Xform \"World\"\n{{\n\
                 def PointInstancer \"Equipment\"\n    {{\n\
                     rel prototypes = [</World/Prototypes/Pump>, </World/Prototypes/Tank>]\n\
                     point3f[] positions = [(0, 0, 0), (2, 0, 0), (0, 0, 2), (2, 0, 2)]\n\
                     int[] protoIndices = [0, 1, 0, 1]\n\
                 }}\n\
                 def Scope \"Prototypes\"\n    {{\n\
                     def Mesh \"Pump\"\n        {{\n{}\n        }}\n\
                     def Mesh \"Tank\"\n        {{\n{}\n        }}\n\
                 }}\n\
             }}\n",
            cube_body(),
            cube_body()
        ),
    )
    .unwrap();

    let scene = UsdImporter::new()
        .import_file(&root, "multi-point-instances")
        .unwrap();
    let mesh_nodes = scene
        .nodes
        .iter()
        .filter(|node| node.mesh.is_some())
        .collect::<Vec<_>>();
    assert_eq!(mesh_nodes.len(), 4);
    assert_eq!(
        mesh_nodes
            .iter()
            .filter_map(|node| node.mesh)
            .collect::<BTreeSet<_>>()
            .len(),
        2
    );
    assert_eq!(
        mesh_nodes
            .iter()
            .filter_map(|node| { node.extras["ice3"]["usd"]["instanceGroup"].as_str() })
            .collect::<BTreeSet<_>>()
            .len(),
        2
    );
}

#[test]
fn expands_every_mesh_in_a_point_instancer_prototype() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("multi-mesh-prototype.usda");
    std::fs::write(
        &root,
        format!(
            "#usda 1.0\n\
             def Xform \"World\"\n{{\n\
                 def PointInstancer \"Assemblies\"\n    {{\n\
                     rel prototypes = </World/Prototypes/Assembly>\n\
                     point3f[] positions = [(0, 0, 0), (5, 0, 0)]\n\
                     int[] protoIndices = [0, 0]\n\
                 }}\n\
                 def Scope \"Prototypes\"\n    {{\n\
                     def Xform \"Assembly\"\n        {{\n\
                         def Mesh \"Pump\"\n            {{\n{}\n            }}\n\
                         def Mesh \"Tank\"\n            {{\n{}\n            }}\n\
                     }}\n\
                 }}\n\
             }}\n",
            cube_body(),
            cube_body()
        ),
    )
    .unwrap();

    let scene = UsdImporter::new()
        .import_file(&root, "multi-mesh-prototype")
        .unwrap();
    let mesh_nodes = scene
        .nodes
        .iter()
        .filter(|node| node.mesh.is_some())
        .collect::<Vec<_>>();

    assert_eq!(mesh_nodes.len(), 4, "{:#?}", scene.nodes);
    assert_eq!(
        mesh_nodes
            .iter()
            .filter_map(|node| node.mesh)
            .collect::<BTreeSet<_>>()
            .len(),
        2
    );
    let group_sizes = mesh_nodes
        .iter()
        .filter_map(|node| {
            node.extras["ice3"]["usd"]["instanceGroup"]
                .as_str()
                .map(str::to_owned)
        })
        .fold(BTreeMap::new(), |mut counts, group| {
            *counts.entry(group).or_insert(0_usize) += 1;
            counts
        });
    assert_eq!(group_sizes.len(), 2, "{group_sizes:#?}");
    assert!(
        group_sizes.values().all(|count| *count == 2),
        "{group_sizes:#?}"
    );
}

#[test]
fn preserves_materials_display_colors_and_metadata() {
    let scene = UsdImporter::new()
        .import_file(
            &toolchain_fixture("usd/synthetic-plant.usda"),
            "usd-materials",
        )
        .unwrap();
    let pump = scene
        .nodes
        .iter()
        .find(|node| node.name == "PrimaryPump")
        .unwrap();
    let usd = &pump.extras["ice3"]["usd"];

    assert_eq!(usd["objectType"], "MESH");
    assert_eq!(pump.extras["ice3"]["displayName"], "PrimaryPump");
    assert_eq!(usd["customProperties"]["EquipmentTag"], "P-101");
    let treatment_line = scene
        .nodes
        .iter()
        .find(|node| node.name == "TreatmentLine")
        .unwrap();
    assert_eq!(
        treatment_line.extras["ice3"]["usd"]["customProperties"]["ProcessArea"],
        "water-line"
    );
    assert_eq!(usd["usdPrimPath"], "/Plant/TreatmentLine/PrimaryPump");
    assert_eq!(scene.materials[0].base_color, [0.12, 0.38, 0.68]);
    let primitive = &scene.meshes[pump.mesh.unwrap()].primitives[0];
    assert_eq!(primitive.colors.as_ref().unwrap()[0], [0.12, 0.38, 0.68]);
}

#[test]
fn converts_non_meter_and_z_up_stage_once() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("units.usda");
    std::fs::write(
        &root,
        format!(
            "#usda 1.0\n\
             (\n    metersPerUnit = 0.01\n    upAxis = \"Z\"\n)\n\
             def Xform \"Plant\"\n{{\n\
                 def Mesh \"Pump\"\n    {{\n{}\n    }}\n\
             }}\n",
            cube_body()
        ),
    )
    .unwrap();

    let scene = UsdImporter::new().import_file(&root, "units").unwrap();
    assert_eq!(
        scene.asset_extras["ice3"]["usd"]["originalMetersPerUnit"],
        Value::from(0.01)
    );
    assert_eq!(scene.asset_extras["ice3"]["usd"]["sourceUpAxis"], "Z");
    assert_eq!(
        scene.asset_extras["coordinateSystem"]["linearUnit"],
        "meter"
    );
    let bounds = world_bounds(&scene);
    assert!((bounds.max[1] - 0.01).abs() < 1e-5, "{bounds:?}");
    assert!(bounds.max[0] < 0.021, "{bounds:?}");
}

#[test]
fn normalizes_a_large_usd_origin_once() {
    let scene = UsdImporter::new()
        .import_file(&toolchain_fixture("usd/large-origin.usda"), "large-usd")
        .unwrap();
    let origin = scene.asset_extras["ice3"]["usd"]["sourceOrigin"]
        .as_array()
        .unwrap();
    assert!(origin[0].as_f64().unwrap() > 1_000_000.0);
    assert_eq!(origin[1].as_f64(), Some(0.0));
    assert_eq!(
        scene.asset_extras["ice3"]["usd"]["originRebased"],
        Value::Bool(true)
    );
    let bounds = world_bounds(&scene);
    assert!(
        bounds.min[0].abs() < 1e-3,
        "{bounds:?} nodes={:#?}",
        scene.nodes
    );
    assert!(
        bounds.min[2].abs() < 1e-3,
        "{bounds:?} nodes={:#?}",
        scene.nodes
    );
    assert!(bounds.max[0] < 20.0, "{bounds:?}");
    assert!(bounds.max[2] < 20.0, "{bounds:?}");
}

#[test]
fn reports_unsupported_resources_deterministically() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("camera.usda");
    std::fs::write(
        &root,
        format!(
            "#usda 1.0\n\
             def Xform \"Plant\"\n{{\n\
                 def Camera \"ReviewCamera\" {{}}\n\
                 def Mesh \"Pump\"\n    {{\n{}\n    }}\n\
             }}\n",
            cube_body()
        ),
    )
    .unwrap();

    let scene = UsdImporter::new().import_file(&root, "camera").unwrap();
    assert_eq!(
        scene.warnings[0].code,
        "ICE3_STUDIO_IMPORT_USD_UNSUPPORTED_RESOURCE"
    );
    assert!(scene.warnings[0].message.contains("Camera"));
}

#[test]
fn warns_for_every_unsupported_typed_prim() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("unsupported-schema.usda");
    std::fs::write(
        &root,
        format!(
            "#usda 1.0\n\
             def Xform \"Plant\"\n{{\n\
                 def Sphere \"TankDome\" {{}}\n\
                 def Mesh \"Pump\"\n    {{\n{}\n    }}\n\
             }}\n",
            cube_body()
        ),
    )
    .unwrap();

    let scene = UsdImporter::new()
        .import_file(&root, "unsupported-schema")
        .unwrap();
    assert_eq!(
        scene.warnings[0].code,
        "ICE3_STUDIO_IMPORT_USD_UNSUPPORTED_RESOURCE"
    );
    assert!(
        scene.warnings[0].message.contains("Sphere"),
        "{:#?}",
        scene.warnings
    );
}

#[test]
fn rejects_malformed_stage_coordinate_metadata() {
    let directory = tempfile::tempdir().unwrap();
    for (name, metadata) in [
        ("bad-meter", "metersPerUnit = \"not-a-number\""),
        ("bad-up-axis", "upAxis = 1"),
        ("bad-handedness", "handedness = false"),
    ] {
        let root = directory.path().join(format!("{name}.usda"));
        std::fs::write(
            &root,
            format!(
                "#usda 1.0\n\
                 (\n    {metadata}\n)\n\
                 def Xform \"Plant\"\n{{\n\
                     def Mesh \"Pump\"\n    {{\n{}\n    }}\n\
                 }}\n",
                cube_body()
            ),
        )
        .unwrap();

        let error = UsdImporter::new().import_file(&root, name).unwrap_err();
        assert_eq!(
            error.code(),
            "ICE3_STUDIO_IMPORT_INVALID_SOURCE",
            "{name}: {error}"
        );
    }
}

#[test]
fn rejects_empty_truncated_missing_and_cyclic_composition() {
    let empty = UsdImporter::new()
        .import_file(&toolchain_fixture("usd/empty.usda"), "empty")
        .unwrap_err();
    assert_eq!(empty.code(), "ICE3_STUDIO_IMPORT_EMPTY_GEOMETRY");

    let truncated = UsdImporter::new()
        .import_file(&toolchain_fixture("usd/truncated.usda"), "truncated")
        .unwrap_err();
    assert_eq!(truncated.code(), "ICE3_STUDIO_IMPORT_INVALID_SOURCE");

    let directory = tempfile::tempdir().unwrap();
    let missing = directory.path().join("missing.usda");
    std::fs::write(
        &missing,
        "#usda 1.0\ndef Xform \"Plant\" (references = @absent.usda@</Plant>) {}\n",
    )
    .unwrap();
    let error = UsdImporter::new()
        .import_file(&missing, "missing")
        .unwrap_err();
    assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_INVALID_SOURCE");

    let a = directory.path().join("a.usda");
    let b = directory.path().join("b.usda");
    std::fs::write(
        &a,
        "#usda 1.0\ndef Xform \"Plant\" (references = @b.usda@</Plant>) {}\n",
    )
    .unwrap();
    std::fs::write(
        &b,
        "#usda 1.0\ndef Xform \"Plant\" (references = @a.usda@</Plant>) {}\n",
    )
    .unwrap();
    let error = UsdImporter::new().import_file(&a, "cycle").unwrap_err();
    assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_INVALID_SOURCE");

    let corrupt = directory.path().join("corrupt.usdc");
    let mut bytes = std::fs::read(toolchain_fixture("usd/synthetic-plant.usdc")).unwrap();
    bytes.truncate(16);
    std::fs::write(&corrupt, bytes).unwrap();
    let error = UsdImporter::new()
        .import_file(&corrupt, "corrupt")
        .unwrap_err();
    assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_INVALID_SOURCE");
}

#[test]
fn rejects_unsafe_and_deep_composition() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source");
    std::fs::create_dir(&source).unwrap();
    let outside = directory.path().join("outside.usda");
    std::fs::write(
        &outside,
        format!("#usda 1.0\ndef Xform \"Plant\" {{\n{}\n}}\n", cube_body()),
    )
    .unwrap();

    let traversal = source.join("traversal.usda");
    std::fs::write(
        &traversal,
        "#usda 1.0\ndef Xform \"Plant\" (references = @../outside.usda@</Plant>) {}\n",
    )
    .unwrap();
    let error = UsdImporter::new()
        .import_file(&traversal, "escape")
        .unwrap_err();
    assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_INVALID_SOURCE");

    let absolute = source.join("absolute.usda");
    std::fs::write(
        &absolute,
        format!(
            "#usda 1.0\ndef Xform \"Plant\" (references = @{}@</Plant>) {{}}\n",
            outside.display()
        ),
    )
    .unwrap();
    let error = UsdImporter::new()
        .import_file(&absolute, "absolute")
        .unwrap_err();
    assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_INVALID_SOURCE");

    let asset_path = source.join("asset-path.usda");
    std::fs::write(
        &asset_path,
        format!(
            "#usda 1.0\n\
             def Xform \"Plant\"\n{{\n\
                 def Mesh \"Pump\"\n    {{\n{}\n\
                     asset inputs:file = @../outside.png@\n\
                 }}\n\
             }}\n",
            cube_body()
        ),
    )
    .unwrap();
    let error = UsdImporter::new()
        .import_file(&asset_path, "asset-path")
        .unwrap_err();
    assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_INVALID_SOURCE");

    for index in 0..34 {
        let body = if index == 33 {
            format!("def Xform \"Level\"\n{{\n{}\n}}\n", cube_body())
        } else {
            format!(
                "def Xform \"Level\" (references = @layer-{}.usda@</Level>) {{}}\n",
                index + 1
            )
        };
        std::fs::write(
            source.join(format!("layer-{index}.usda")),
            format!("#usda 1.0\n{body}"),
        )
        .unwrap();
    }
    let error = UsdImporter::new()
        .import_file(&source.join("layer-0.usda"), "deep")
        .unwrap_err();
    assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_INVALID_SOURCE");
}

#[test]
fn exposes_usd_adapter_contract_and_limits() {
    let importer = UsdImporter::new();
    assert_eq!(importer.id(), "usd");
    assert_eq!(importer.extensions(), &[".usda", ".usdc"]);
    assert!(importer.capabilities().instances);
    assert!(importer.capabilities().vertex_colors);

    let error = importer
        .import_file_with_limits(
            &toolchain_fixture("usd/synthetic-plant.usda"),
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
fn rejects_point_instancer_node_growth_before_expansion() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("node-budget.usda");
    std::fs::write(
        &root,
        format!(
            "#usda 1.0\n\
             def Xform \"World\"\n{{\n\
                 def PointInstancer \"Pumps\"\n    {{\n\
                     rel prototypes = </World/Prototypes/Pump>\n\
                     point3f[] positions = [(0, 0, 0), (1, 0, 0), (2, 0, 0), (3, 0, 0)]\n\
                     int[] protoIndices = [0, 0, 0, 0]\n\
                 }}\n\
                 def Scope \"Prototypes\"\n    {{\n\
                     def Mesh \"Pump\"\n        {{\n{}\n        }}\n\
                 }}\n\
             }}\n",
            cube_body()
        ),
    )
    .unwrap();

    let error = UsdImporter::new()
        .import_file_with_limits(
            &root,
            "node-budget",
            NativeImportLimits {
                max_nodes: 3,
                ..NativeImportLimits::default()
            },
        )
        .unwrap_err();

    assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_LIMIT_EXCEEDED");
}

#[test]
fn rejects_mesh_output_before_exceeding_the_memory_budget() {
    let error = UsdImporter::new()
        .import_file_with_limits(
            &toolchain_fixture("usd/synthetic-plant.usda"),
            "memory-budget",
            NativeImportLimits {
                max_memory_bytes: 64,
                ..NativeImportLimits::default()
            },
        )
        .unwrap_err();

    assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_LIMIT_EXCEEDED");
}

#[test]
fn maps_to_a_valid_prepared_glb() {
    let scene = UsdImporter::new()
        .import_file(
            &toolchain_fixture("usd/synthetic-plant.usda"),
            "usd-prepared",
        )
        .unwrap();
    let canonical = studio_import::to_canonical(&scene);
    let bytes = studio_assets::canonical::write_prepared_glb(
        &canonical,
        &PreparedGlbOptions {
            asset_id: "usd-prepared".into(),
            source_format: "usd".into(),
            source_digest: "fixture".into(),
        },
    )
    .unwrap();
    let source = studio_assets::read_glb_bytes(&bytes, &std::env::temp_dir()).unwrap();
    let profile = studio_assets::validate_prepared_profile(&source).unwrap();
    assert_eq!(profile.asset_id, "usd-prepared");
}

#[cfg(unix)]
#[test]
fn rejects_symlink_escape() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source");
    std::fs::create_dir(&source).unwrap();
    let outside = directory.path().join("outside.usda");
    std::fs::write(
        &outside,
        format!("#usda 1.0\ndef Xform \"Plant\" {{\n{}\n}}\n", cube_body()),
    )
    .unwrap();
    symlink(&outside, source.join("link.usda")).unwrap();
    let root = source.join("root.usda");
    std::fs::write(
        &root,
        "#usda 1.0\ndef Xform \"Plant\" (references = @link.usda@</Plant>) {}\n",
    )
    .unwrap();

    let error = UsdImporter::new()
        .import_file(&root, "symlink")
        .unwrap_err();
    assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_INVALID_SOURCE");
}

fn cube_body() -> &'static str {
    "            int[] faceVertexCounts = [3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3]\n\
     \x20           int[] faceVertexIndices = [1, 2, 0, 3, 6, 2, 7, 4, 6, 5, 0, 4, 6, 0, 2, 3, 5, 7, 1, 3, 2, 3, 7, 6, 7, 5, 4, 5, 1, 0, 6, 4, 0, 3, 1, 5]\n\
     \x20           point3f[] points = [(-1, -1, -1), (-1, -1, 1), (-1, 1, -1), (-1, 1, 1), (1, -1, -1), (1, -1, 1), (1, 1, -1), (1, 1, 1)]\n\
     \x20           color3f[] primvars:displayColor = [(0.12, 0.38, 0.68)] (\n\
     \x20               interpolation = \"constant\"\n\
     \x20           )"
}

fn toolchain_fixture(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../ice3-toolchain/tests/fixtures")
        .join(relative)
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
                    node.extras["ice3"]["usd"]["usdPrimPath"]
                        .as_str()
                        .unwrap_or_default()
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
            .collect(),
    }
}

#[derive(Debug, PartialEq)]
struct SemanticSummary {
    nodes: Vec<NodeSummary>,
    meshes: Vec<MeshSummary>,
    materials: Vec<(String, [i64; 3], i64)>,
    bounds: [i64; 6],
    warnings: BTreeSet<String>,
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

#[derive(Debug)]
struct Bounds {
    min: [f32; 3],
    max: [f32; 3],
}
