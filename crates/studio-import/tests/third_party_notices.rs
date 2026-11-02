use std::path::Path;

#[test]
fn declares_the_native_source_import_licenses() {
    let notices = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../THIRD_PARTY_NOTICES.md"),
    )
    .unwrap();

    for required in [
        "fbx-rs",
        "MIT OR Apache-2.0",
        "ufbx",
        "openusd",
        "Universal Scene Description",
        "Copyright 2016 Pixar",
    ] {
        assert!(notices.contains(required), "missing notice: {required}");
    }
}
