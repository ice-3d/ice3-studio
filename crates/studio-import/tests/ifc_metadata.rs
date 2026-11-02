use std::path::PathBuf;

use studio_import::{NativeImportLimits, extract_ifc_metadata};

#[test]
fn extracts_ifc4x3_space_properties_quantities_and_systems() {
    let bytes = std::fs::read(fixture("ifc/synthetic-plant.ifc")).unwrap();
    let metadata = extract_ifc_metadata(&bytes, "ifc-fixture", NativeImportLimits::default())
        .expect("IFC4X3 metadata should import");

    assert_eq!(metadata.schema, "IFC4X3");
    let pump = metadata
        .products
        .values()
        .find(|product| product.name == "Primary Pump")
        .unwrap();
    assert_eq!(pump.ifc_type, "IFCPUMP");
    assert_eq!(pump.selection_id, "ifc-fixture/ifc/1iwOd0pZv1BAvoVKqbjYQK");
    assert!(
        pump.property_sets
            .iter()
            .any(|set| set.name == "Pset_PumpTypeCommon")
    );
    assert!(
        pump.quantity_sets
            .iter()
            .any(|set| set.name == "Qto_PumpBaseQuantities")
    );
    assert!(
        pump.systems
            .iter()
            .any(|system| system.name == "Sludge Transfer")
    );
    let pump_pset = pump
        .property_sets
        .iter()
        .find(|set| set.name == "Pset_PumpTypeCommon")
        .unwrap();
    assert_eq!(pump_pset.properties["FlowRate"], 12.5);
    assert_eq!(pump_pset.properties["PumpType"], "CENTRIFUGAL");
    assert_eq!(pump_pset.properties["IsVariableSpeed"], true);
}

#[test]
fn keeps_duplicate_and_missing_global_ids_distinct() {
    let duplicate = extract_ifc_metadata(
        &std::fs::read(fixture("ifc/duplicate-global-id.ifc")).unwrap(),
        "dup",
        NativeImportLimits::default(),
    )
    .unwrap();
    let duplicate_ids = duplicate
        .products
        .values()
        .map(|product| product.selection_id.as_str())
        .collect::<Vec<_>>();
    assert!(duplicate_ids.contains(&"dup/ifc/1iwOd0pZv1BAvoVKqbjYQK"));
    assert!(
        duplicate_ids
            .iter()
            .any(|id| id.starts_with("dup/ifc/1iwOd0pZv1BAvoVKqbjYQK/express/"))
    );
    assert!(
        duplicate
            .warnings
            .iter()
            .any(|warning| warning.code == "ICE3_STUDIO_IMPORT_IFC_DUPLICATE_GLOBAL_ID")
    );

    let missing = extract_ifc_metadata(
        &std::fs::read(fixture("ifc/missing-global-id.ifc")).unwrap(),
        "missing",
        NativeImportLimits::default(),
    )
    .unwrap();
    assert!(
        missing
            .products
            .values()
            .any(|product| product.selection_id.starts_with("missing/ifc/express/"))
    );
}

#[test]
fn rejects_non_ifc4x3_schema() {
    let bytes = std::fs::read(fixture("ifc/ifc4-legacy.ifc")).unwrap();
    let error = extract_ifc_metadata(&bytes, "legacy", NativeImportLimits::default()).unwrap_err();
    assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_IFC_SCHEMA_UNSUPPORTED");
}

fn fixture(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../ice3-toolchain/tests/fixtures")
        .join(relative)
}
