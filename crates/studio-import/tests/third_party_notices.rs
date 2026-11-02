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
        "Pixar (http://www.pixar.com/).",
        "TOMORROW OPEN SOURCE TECHNOLOGY LICENSE 1.0",
        "Section 6 (\"Trademarks\")",
        "You must give any other recipients of the Work or",
    ] {
        assert!(notices.contains(required), "missing notice: {required}");
    }

    assert!(
        !notices.lines().any(|line| line.ends_with(' ')),
        "third-party notices must not contain trailing whitespace"
    );
}
