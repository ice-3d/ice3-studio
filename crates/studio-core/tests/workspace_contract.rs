use studio_core::{API_VERSION, PROJECT_SCHEMA_VERSION};

#[test]
fn exposes_the_initial_contract_versions() {
    assert_eq!(PROJECT_SCHEMA_VERSION, 2);
    assert_eq!(API_VERSION, "1.0");
}
