#[test]
fn documents_every_public_diagnostic_code() {
    let document = include_str!("../../../docs/reference/diagnostics.md");
    for code in studio_core::validation::DIAGNOSTIC_CODES {
        assert!(document.contains(code), "missing diagnostic: {code}");
    }
}
