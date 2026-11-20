use serde_json::json;
use studio_assets::surface::AlphaMode;
use studio_import::SourceMaterial;

#[test]
fn source_material_defaults_keep_old_records_compatible() {
    let material: SourceMaterial = serde_json::from_value(json!({
        "id": "steel",
        "name": "Steel",
        "base_color": [0.6, 0.6, 0.62],
        "opacity": 1.0,
        "extras": "{}"
    }))
    .unwrap();

    assert_eq!(material.alpha_mode, AlphaMode::Opaque);
    assert_eq!(material.alpha_cutoff, 0.5);
    assert!(!material.double_sided);
    assert!(material.surface.is_none());
}
