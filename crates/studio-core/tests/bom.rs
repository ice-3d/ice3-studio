//! `scene.bom` contract: grouping, quantities and CSV projection.

mod support;

use std::collections::BTreeMap;

use studio_core::agent_contract::{BomRequest, build_bom};
use studio_core::export::bom::bom_csv;
use studio_core::large_scene::evaluate::evaluate_large_scene;
use support::shared_pump_document;

fn request(group_by: Option<&str>, limit: Option<usize>) -> BomRequest {
    BomRequest {
        group_by: group_by.map(|value| value.to_string()),
        limit,
    }
}

#[test]
fn groups_identical_instances_into_one_row() {
    let document = shared_pump_document();
    let scene = evaluate_large_scene(&document).unwrap();
    let result = build_bom(&document, &scene, &request(None, None));

    assert_eq!(result.group_by, "definition");
    assert_eq!(result.rows.len(), 1);
    let row = &result.rows[0];
    assert_eq!(row.key, "component-000001");
    assert_eq!(row.label, "Pump");
    assert_eq!(row.count, 2, "two evaluated part nodes");
    assert_eq!(row.owners, 2, "two distinct source instances");
    assert!(
        (row.volume - 4.0).abs() < 1e-4,
        "two 2x1x1 boxes, got {}",
        row.volume
    );
    assert!(row.bounds.is_some(), "group bounds cover both instances");
    assert!(!result.truncated);
}

#[test]
fn grouping_by_owner_splits_instances() {
    let document = shared_pump_document();
    let scene = evaluate_large_scene(&document).unwrap();
    let result = build_bom(&document, &scene, &request(Some("owner"), None));
    assert_eq!(result.rows.len(), 2);
    for row in &result.rows {
        assert_eq!(row.count, 1);
        assert_eq!(row.owners, 1);
        assert_eq!(row.key, row.label);
    }
}

#[test]
fn grouping_by_material_uses_unassigned_bucket() {
    let document = shared_pump_document();
    let scene = evaluate_large_scene(&document).unwrap();
    let result = build_bom(&document, &scene, &request(Some("material"), None));
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].key, "unassigned");
    assert_eq!(result.rows[0].count, 2);
}

#[test]
fn grouping_by_metadata_key_partitions_parts() {
    let mut document = shared_pump_document();
    for (index, part) in document.components[0].parts.iter_mut().enumerate() {
        part.metadata.insert(
            "zone".to_string(),
            if index == 0 { "north" } else { "south" }.to_string(),
        );
    }
    document.components[0]
        .parts
        .push(studio_core::large_scene::component::ComponentPart {
            id: "flange".into(),
            parent_id: None,
            transform: studio_core::large_scene::component::PartTransform::default(),
            content: studio_core::large_scene::component::PartContent::Box {
                width: studio_core::large_scene::component::ScalarBinding::Literal(1.0),
                height: studio_core::large_scene::component::ScalarBinding::Literal(1.0),
                depth: studio_core::large_scene::component::ScalarBinding::Literal(1.0),
            },
            subtract: Vec::new(),
            repeat: None,
            material_role: None,
            metadata: BTreeMap::from([("zone".to_string(), "south".to_string())]),
            visible: true,
        });

    let scene = evaluate_large_scene(&document).unwrap();
    let result = build_bom(&document, &scene, &request(Some("metadata:zone"), None));
    let labelled: BTreeMap<&str, usize> = result
        .rows
        .iter()
        .map(|row| (row.key.as_str(), row.count))
        .collect();
    assert_eq!(labelled.get("north"), Some(&2), "body of both instances");
    assert_eq!(
        labelled.get("south"),
        Some(&2),
        "flange of both instances on the same row as the unzoned body"
    );
}

#[test]
fn limit_truncates_and_flags_the_result() {
    let document = shared_pump_document();
    let scene = evaluate_large_scene(&document).unwrap();
    let result = build_bom(&document, &scene, &request(Some("owner"), Some(1)));
    assert_eq!(result.total, 2);
    assert!(result.truncated);
    assert_eq!(result.rows.len(), 1);
}

#[test]
fn csv_projection_carries_header_and_rows() {
    let document = shared_pump_document();
    let scene = evaluate_large_scene(&document).unwrap();
    let result = build_bom(&document, &scene, &request(None, None));
    let csv = bom_csv(&result);
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].starts_with("key,label,count,owners,volumeM3"));
    assert!(lines[1].starts_with("component-000001,Pump,2,2,4"));
}

#[test]
fn csv_escapes_commas_and_quotes() {
    let document = shared_pump_document();
    let scene = evaluate_large_scene(&document).unwrap();
    let mut result = build_bom(&document, &scene, &request(None, None));
    result.rows[0].label = "Pump, \"A\"".to_string();
    let csv = bom_csv(&result);
    assert!(
        csv.contains("\"Pump, \"\"A\"\"\""),
        "row must be quoted: {csv}"
    );
}

#[test]
fn request_json_matches_the_contract() {
    let request: BomRequest =
        serde_json::from_str(r#"{"groupBy":"metadata:zone","limit":10}"#).expect("parses");
    assert_eq!(request.group_by.as_deref(), Some("metadata:zone"));
    assert_eq!(request.limit, Some(10));
}
