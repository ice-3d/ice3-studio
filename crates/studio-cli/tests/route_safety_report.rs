use studio_cli::route_safety_report::build_report;
use studio_core::large_scene::route_safety::RouteSafetyStats;

#[test]
fn route_safety_report_serializes_counters_and_diagnostics() {
    let report = build_report(
        7,
        Vec::new(),
        RouteSafetyStats {
            obstacle_owners: 12,
            route_segments: 34,
            broad_phase_candidates: 5,
            narrow_phase_pairs: 2,
            triangle_pairs: 100,
        },
    );
    assert_eq!(report["revision"], 7);
    assert_eq!(report["stats"]["obstacleOwners"], 12);
    assert_eq!(report["stats"]["trianglePairs"], 100);
    assert_eq!(report["diagnostics"], serde_json::json!([]));
}
