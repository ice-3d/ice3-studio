#![cfg(feature = "jt")]

use std::path::Path;

use studio_import::{JtImporter, SourceImporter, SourceImporterRegistry};

#[test]
fn resolves_jt_by_extension_and_explicit_id() {
    let registry = SourceImporterRegistry::try_new(vec![Box::new(JtImporter::new())]).unwrap();

    assert_eq!(
        registry
            .resolve(Path::new("assembly.JT"), None)
            .unwrap()
            .id(),
        "jt"
    );
    assert_eq!(
        registry
            .resolve(Path::new("assembly.jt"), Some("jt"))
            .unwrap()
            .id(),
        "jt"
    );
    assert_eq!(JtImporter::new().extensions(), [".jt"]);
}

#[test]
fn advertises_jt_source_capabilities() {
    let capabilities = JtImporter::new().capabilities();

    assert!(capabilities.hierarchy);
    assert!(!capabilities.vertex_colors);
    assert!(capabilities.material_colors);
    assert!(capabilities.instances);
    assert!(capabilities.semantic_metadata);
}

#[test]
fn rejects_explicit_jt_for_other_extensions() {
    let registry = SourceImporterRegistry::try_new(vec![Box::new(JtImporter::new())]).unwrap();

    let error = match registry.resolve(Path::new("assembly.glb"), Some("jt")) {
        Ok(_) => panic!("JT must reject a non-JT extension"),
        Err(error) => error,
    };
    assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_ADAPTER_UNAVAILABLE");
}
