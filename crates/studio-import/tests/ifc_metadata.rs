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
fn preserves_spatial_hierarchy_containment_and_orphan_products() {
    let bytes = std::fs::read(fixture("ifc/synthetic-plant.ifc")).unwrap();
    let metadata = extract_ifc_metadata(&bytes, "ifc-fixture", NativeImportLimits::default())
        .expect("IFC4X3 metadata should import");
    let project = metadata
        .nodes
        .values()
        .find(|node| node.ifc_type == "IFCPROJECT")
        .unwrap();
    let site = metadata
        .nodes
        .values()
        .find(|node| node.ifc_type == "IFCSITE")
        .unwrap();
    let building = metadata
        .nodes
        .values()
        .find(|node| node.ifc_type == "IFCBUILDING")
        .unwrap();
    let storey = metadata
        .nodes
        .values()
        .find(|node| node.ifc_type == "IFCBUILDINGSTOREY")
        .unwrap();

    assert_eq!(project.name, "Synthetic IFC Plant");
    assert_eq!(project.parent_id, None);
    assert_eq!(site.parent_id, Some(project.express_id));
    assert_eq!(building.parent_id, Some(site.express_id));
    assert_eq!(storey.parent_id, Some(building.express_id));
    assert!(project.path.ends_with("Synthetic IFC Plant"));
    assert!(site.path.ends_with("Synthetic IFC Plant/Synthetic Site"));
    assert!(
        building
            .path
            .ends_with("Synthetic IFC Plant/Synthetic Site/Treatment Building")
    );
    assert!(storey.path.ends_with("Treatment Building/Process Floor"));
    assert!(project.children.contains(&site.express_id));
    assert!(site.children.contains(&building.express_id));
    assert!(building.children.contains(&storey.express_id));
    assert!(storey.elements.contains(&18));
    assert!(storey.elements.contains(&19));

    let orphan = extract_ifc_metadata(
        &std::fs::read(fixture("ifc/orphan-product.ifc")).unwrap(),
        "orphan",
        NativeImportLimits::default(),
    )
    .unwrap();
    let orphan_pump = orphan
        .products
        .values()
        .find(|product| product.name == "Orphan Pump")
        .expect("geometry-only orphan product should be retained");
    assert_eq!(orphan_pump.ifc_type, "IFCPUMP");
    assert!(
        orphan
            .nodes
            .values()
            .all(|node| !node.elements.contains(&orphan_pump.express_id))
    );

    let semantic_only = extract_ifc_metadata(
        &std::fs::read(fixture("ifc/semantic-only-product.ifc")).unwrap(),
        "semantic",
        NativeImportLimits::default(),
    )
    .unwrap();
    assert!(
        semantic_only
            .products
            .values()
            .any(|product| product.name == "Primary Pump")
    );
}

#[test]
fn rejects_entity_limit_before_full_metadata_extraction() {
    let bytes = std::fs::read(fixture("ifc/synthetic-plant.ifc")).unwrap();
    let limits = NativeImportLimits {
        max_nodes: 2,
        ..NativeImportLimits::default()
    };
    let error = extract_ifc_metadata(&bytes, "limited", limits).unwrap_err();
    assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_LIMIT_EXCEEDED");
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
