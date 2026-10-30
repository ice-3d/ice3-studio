use studio_cli::schema;

#[test]
fn writes_deterministic_schema_output() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("schemas");
    schema::generate(&output, false).unwrap();
    let first = std::fs::read(output.join("studio-api.schema.json")).unwrap();
    let first_types = std::fs::read(output.join("studio-types.ts")).unwrap();
    schema::generate(&output, false).unwrap();
    assert_eq!(
        std::fs::read(output.join("studio-api.schema.json")).unwrap(),
        first
    );
    assert_eq!(
        std::fs::read(output.join("studio-types.ts")).unwrap(),
        first_types
    );
    let text = String::from_utf8(first).unwrap();
    assert!(text.contains("node.create"));
    assert!(text.contains("export.runtimePreview"));
}
