use std::path::PathBuf;

use studio_assets::{
    AssetBuildReport, AssetMetrics, BudgetProfile, BudgetStatus, PreparationMetadata,
    PrepareWarning, evaluate_budget,
};

fn metrics(triangles: u64) -> AssetMetrics {
    AssetMetrics {
        encoded_bytes: 0,
        json_chunk_bytes: 0,
        bin_chunk_bytes: 0,
        node_count: 1,
        max_node_depth: 0,
        mesh_count: 1,
        primitive_count: 2,
        triangle_count: triangles,
        edge_segment_count: 12,
        material_count: 3,
        selection_group_count: 1,
    }
}

fn warning(code: &str, message: &str) -> PrepareWarning {
    PrepareWarning {
        code: code.into(),
        message: message.into(),
        path: Some(PathBuf::from("fixture.gltf")),
    }
}

#[test]
fn classifies_soft_and_hard_limits_inclusively() {
    let profile = BudgetProfile::interactive_desktop_v1();
    assert_eq!(
        evaluate_budget(metrics(5_000_000), &profile).status,
        BudgetStatus::Pass
    );
    assert_eq!(
        evaluate_budget(metrics(5_000_001), &profile).status,
        BudgetStatus::Warning
    );
    assert_eq!(
        evaluate_budget(metrics(20_000_001), &profile).status,
        BudgetStatus::Error
    );
}

#[test]
fn parses_a_custom_budget_and_rejects_invalid_limits() {
    let valid = BudgetProfile::from_json(
        r#"{
          "id":"custom",
          "limits":[{
            "id":"triangleCount","label":"Triangles",
            "softLimit":1000,"hardLimit":2000
          }]
        }"#,
    )
    .unwrap();
    assert_eq!(valid.id(), "custom");

    for invalid in [
        r#"{"id":"x","limits":[{"id":"triangleCount","label":"Triangles","softLimit":2,"hardLimit":1}]}"#,
        r#"{"id":"x","limits":[{"id":"bogus","label":"Bogus","softLimit":1,"hardLimit":2}]}"#,
        r#"{"id":"x","limits":[]}"#,
    ] {
        assert_eq!(
            BudgetProfile::from_json(invalid).unwrap_err().code(),
            "ICE3_STUDIO_ASSET_INVALID_BUDGET"
        );
    }
}

#[test]
fn report_is_versioned_and_deterministic() {
    let report = AssetBuildReport::new(
        "build",
        "sha256:source",
        "sha256:content",
        "asset",
        metrics(12),
        PreparationMetadata::edges(30.0),
        evaluate_budget(metrics(12), &BudgetProfile::interactive_desktop_v1()),
        vec![warning("B", "second"), warning("A", "first")],
    );
    let first = serde_json::to_vec(&report).unwrap();
    let second = serde_json::to_vec(&report).unwrap();
    assert_eq!(first, second);
    assert_eq!(report.kind, "ice3-asset-build-report");
    assert_eq!(report.schema_version, "1.0");
    assert_eq!(report.warnings[0].code, "A");
}
