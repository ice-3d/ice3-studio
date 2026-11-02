use std::path::Path;

#[test]
fn declares_the_native_source_import_licenses() {
    let notices = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../THIRD_PARTY_NOTICES.md"),
    )
    .unwrap();
    let mpl = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../third-party-licenses/MPL-2.0.txt"),
    )
    .unwrap();

    for required in [
        "fbx-rs",
        "MIT OR Apache-2.0",
        "ufbx",
        "oxideav-step",
        "oxideav-ifc",
        "Karpelès Lab Inc.",
        "openusd",
        "Universal Scene Description",
        "Copyright 2016 Pixar",
        "Pixar (http://www.pixar.com/).",
        "TOMORROW OPEN SOURCE TECHNOLOGY LICENSE 1.0",
        "Section 6 (\"Trademarks\")",
        "You must give any other recipients of the Work or",
        "ifc-lite-core",
        "ifc-lite-geometry",
        "ifc-lite-processing",
        "Mozilla Public License Version 2.0",
    ] {
        assert!(notices.contains(required), "missing notice: {required}");
    }
    assert!(mpl.contains("Mozilla Public License Version 2.0"));
    assert!(mpl.contains("3.2. Distribution of Executable Form"));

    assert!(
        !notices.lines().any(|line| line.ends_with(' ')),
        "third-party notices must not contain trailing whitespace"
    );
}
