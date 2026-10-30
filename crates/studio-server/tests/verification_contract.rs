#[test]
fn documents_every_public_diagnostic_code() {
    let document = include_str!("../../../docs/reference/diagnostics.md");
    assert_eq!(
        studio_core::DIAGNOSTIC_CODES,
        studio_core::validation::DIAGNOSTIC_CODES
    );
    for code in studio_core::DIAGNOSTIC_CODES {
        assert!(document.contains(code), "missing diagnostic: {code}");
    }
}
