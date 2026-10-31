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
    assert!(first.ends_with(b"\n"));
    assert!(!first.ends_with(b"\n\n"));
    assert!(first_types.ends_with(b"\n"));
    assert!(!first_types.ends_with(b"\n\n"));
    let text = String::from_utf8_lossy(&first);
    assert!(text.contains("node.create"));
    assert!(text.contains("export.runtimePreview"));
    assert!(text.contains("changeset.impact"));
    let schema: serde_json::Value = serde_json::from_slice(&first).unwrap();
    assert!(schema["types"].get("ChangeSetImpactParams").is_some());
    assert!(schema["types"].get("ChangeSetImpactResult").is_some());
    assert!(schema["types"].get("IterationImpact").is_some());
    let types = String::from_utf8(first_types).unwrap();
    assert!(types.contains("export type ChangeSetImpactResult"));
    assert!(types.contains("export type IterationImpact"));
}
