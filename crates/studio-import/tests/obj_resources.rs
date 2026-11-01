use std::path::PathBuf;

use studio_import::obj::{parse_mtl, read_obj_resources};

#[test]
fn ignores_material_paths_outside_the_obj_directory() {
    let resources = read_obj_resources(&fixture("escaping-material.obj")).unwrap();
    assert!(resources.materials.is_empty());
    assert!(
        resources
            .warnings
            .iter()
            .any(|warning| warning.code == "ICE3_STUDIO_IMPORT_OBJ_RESOURCE_IGNORED")
    );
}

#[test]
fn parses_mtl_color_and_opacity() {
    let parsed = parse_mtl(
        br#"
newmtl steel
Kd 0.2 0.5 0.9
d 0.75
"#,
    )
    .unwrap();
    assert_eq!(parsed["steel"].base_color, [0.2, 0.5, 0.9]);
    assert_eq!(parsed["steel"].opacity, 0.75);
}

#[test]
fn reports_a_missing_material_library() {
    let resources = read_obj_resources(&fixture("missing-material.obj")).unwrap();
    assert!(resources.materials.is_empty());
    assert!(
        resources
            .warnings
            .iter()
            .any(|warning| { warning.code == "ICE3_STUDIO_IMPORT_OBJ_MATERIAL_UNAVAILABLE" })
    );
}

#[test]
fn rejects_invalid_mtl_numbers() {
    let error = parse_mtl(b"newmtl bad\nKd NaN 0 0\n").unwrap_err();
    assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_INVALID_SOURCE");
}

#[cfg(unix)]
#[test]
fn ignores_a_material_symlink_that_escapes_the_source_directory() {
    use std::os::unix::fs::symlink;

    let source_dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("outside.mtl"), "newmtl leaked\n").unwrap();
    symlink(
        outside.path().join("outside.mtl"),
        source_dir.path().join("linked.mtl"),
    )
    .unwrap();
    let obj = source_dir.path().join("linked.obj");
    std::fs::write(
        &obj,
        "mtllib linked.mtl\no linked\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
    )
    .unwrap();

    let resources = read_obj_resources(&obj).unwrap();
    assert!(resources.materials.is_empty());
    assert!(
        resources
            .warnings
            .iter()
            .any(|warning| warning.code == "ICE3_STUDIO_IMPORT_OBJ_RESOURCE_IGNORED")
    );
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/obj")
        .join(name)
}
