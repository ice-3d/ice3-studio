use studio_pipeline::{AssetProfile, LodAllocationMode, RenderBatchMode, SemanticProxyMode};

#[test]
fn agent_console_profile_prepares_edges() {
    let profile = AssetProfile::agent_console_v1();
    assert!(profile.generate_edges);
    assert_eq!(profile.crease_angle_degrees, 30.0);
}

#[test]
fn direct_profile_uses_legacy_defaults() {
    let profile = AssetProfile::direct_partition(500_000, 33_554_432);
    assert_eq!(profile.max_triangles, 500_000);
    assert_eq!(profile.max_bytes, 33_554_432);
    assert_eq!(profile.render_batch, RenderBatchMode::Off);
    assert!(profile.generate_edges);
    assert_eq!(profile.crease_angle_degrees, 30.0);
    assert!(!profile.lod_enabled);
    assert_eq!(profile.lod_ratio, 0.15);
    assert_eq!(profile.lod_allocation, LodAllocationMode::Proportional);
    assert_eq!(profile.lod_curve_samples, 8);
}

#[test]
fn adaptive_semantic_proxy_is_rejected_for_proportional_allocation() {
    let mut profile = AssetProfile::agent_console_v1();
    profile.lod_allocation = LodAllocationMode::Proportional;
    profile.semantic_proxy = SemanticProxyMode::Auto;
    assert!(profile.validate().is_err());
}

#[test]
fn semantic_proxy_is_rejected_when_lod_is_disabled() {
    let mut profile = AssetProfile::direct_partition(500_000, 33_554_432);
    profile.lod_allocation = LodAllocationMode::Adaptive;
    profile.semantic_proxy = SemanticProxyMode::Auto;
    assert!(profile.validate().is_err());
}

#[test]
fn profile_validation_rejects_every_invalid_range() {
    let cases: Vec<AssetProfile> = vec![
        mutate(|profile| profile.max_triangles = 0),
        mutate(|profile| profile.max_bytes = 0),
        mutate(|profile| profile.crease_angle_degrees = f32::NAN),
        mutate(|profile| profile.crease_angle_degrees = 180.1),
        mutate(|profile| profile.lod_ratio = 0.0),
        mutate(|profile| profile.lod_ratio = 1.0),
        mutate(|profile| profile.lod_min_unit_triangles = 251_000),
        mutate(|profile| profile.lod_curve_samples = 3),
        mutate(|profile| profile.lod_curve_samples = 33),
        mutate(|profile| profile.lod_max_memory_bytes = 0),
        mutate(|profile| profile.coarse_chunk_max_bytes = 0),
    ];
    for profile in cases {
        assert!(profile.validate().is_err(), "profile should be invalid");
    }
}

#[test]
fn invalid_profile_uses_the_stable_diagnostic_code() {
    let profile = AssetProfile::direct_partition(0, 1);
    assert_eq!(
        profile.validate().unwrap_err().code(),
        "ICE3_STUDIO_ASSET_INVALID_PROFILE"
    );
}

#[test]
fn profile_validation_accepts_boundary_values() {
    let mut profile = AssetProfile::direct_partition(u64::MAX, 1);
    profile.crease_angle_degrees = 0.0;
    profile.lod_ratio = f64::MIN_POSITIVE;
    profile.lod_min_unit_triangles = profile.lod_max_triangles;
    profile.lod_curve_samples = 4;
    assert!(profile.validate().is_ok());

    profile.crease_angle_degrees = 180.0;
    profile.lod_ratio = 0.999_999;
    profile.lod_curve_samples = 32;
    assert!(profile.validate().is_ok());
}

fn mutate(change: impl FnOnce(&mut AssetProfile)) -> AssetProfile {
    let mut profile = AssetProfile::agent_console_v1();
    change(&mut profile);
    profile
}
