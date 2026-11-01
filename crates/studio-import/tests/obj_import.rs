use std::path::{Path, PathBuf};

use studio_import::{ObjImporter, SourceFormat};

#[test]
fn imports_all_obj_face_forms_and_triangulates_polygons() {
    let scene = ObjImporter::new()
        .import_file(&fixture("faces.obj"), "faces")
        .unwrap();
    let primitive = &scene.meshes[0].primitives[0];
    assert_eq!(scene.source_format, SourceFormat::Obj);
    assert_eq!(primitive.indices.len(), 9);
    assert_eq!(primitive.positions.len(), 4);
    assert_eq!(primitive.normals.as_ref().unwrap().len(), 4);
    assert_eq!(scene.materials[0].name, "steel");
    assert_eq!(
        scene.nodes[0].extras["ice3"]["selectionId"],
        "faces/obj/node/0"
    );
    assert_eq!(scene.nodes[0].name, "OBJ Root");
}

#[test]
fn resolves_negative_obj_indices() {
    let scene = ObjImporter::new()
        .import_file(&fixture("negative-faces.obj"), "negative")
        .unwrap();
    assert_eq!(scene.meshes[0].primitives[0].indices.len(), 6);
    assert_eq!(scene.meshes[0].primitives[0].positions.len(), 4);
}

#[test]
fn shifts_large_world_coordinates_to_the_source_origin() {
    let scene = ObjImporter::new()
        .import_file(&toolchain_fixture("obj/large-origin.obj"), "large")
        .unwrap();
    let bounds = bounds(&scene);
    assert!(bounds.min[0].abs() < 1e-5);
    assert!(bounds.min[2].abs() < 1e-5);
    assert!(bounds.max[0].is_finite());
    let mesh_node = scene.nodes.iter().find(|node| node.mesh.is_some()).unwrap();
    let origin = mesh_node.extras["ice3"]["obj"]["sourceOrigin"]
        .as_array()
        .unwrap();
    assert!((origin[0].as_f64().unwrap() - 1_000_010.8).abs() < 0.1);
    assert_eq!(origin[1].as_f64(), Some(0.0));
    assert!((origin[2].as_f64().unwrap() + 0.65).abs() < 1e-5);
}

#[test]
fn rejects_an_obj_without_faces() {
    let error = ObjImporter::new()
        .import_file(&toolchain_fixture("obj/empty.obj"), "empty")
        .unwrap_err();
    assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_EMPTY_GEOMETRY");
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/obj")
        .join(name)
}

fn toolchain_fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../ice3-toolchain/tests/fixtures")
        .join(name)
}

fn bounds(scene: &studio_import::SourceScene) -> Bounds {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for mesh in &scene.meshes {
        for primitive in &mesh.primitives {
            for point in &primitive.positions {
                for axis in 0..3 {
                    min[axis] = min[axis].min(point[axis]);
                    max[axis] = max[axis].max(point[axis]);
                }
            }
        }
    }
    Bounds { min, max }
}

struct Bounds {
    min: [f32; 3],
    max: [f32; 3],
}
