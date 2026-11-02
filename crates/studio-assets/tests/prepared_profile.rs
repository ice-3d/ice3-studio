mod common;

use common::{minimal_binary, packed_fixture};
use studio_assets::{read_glb, validate_prepared_profile};

#[test]
fn accepts_a_studio_prepared_glb() {
    let binary = minimal_binary();
    let (_directory, path) = packed_fixture("studio-prepared.json", &binary);
    let source = read_glb(&path).unwrap();
    let profile = validate_prepared_profile(&source).unwrap();
    assert_eq!(profile.asset_id, "studio-prepared");
}

#[test]
fn rejects_an_unprepared_glb() {
    let (_directory, path) = packed_fixture("plain.json", &[]);
    let error = read_glb(&path)
        .and_then(|source| validate_prepared_profile(&source))
        .unwrap_err();
    assert_eq!(error.code(), "ICE3_ASSET_PROFILE_UNSUPPORTED");
}

#[test]
fn rejects_external_buffers_outside_base_directory() {
    let error = read_glb(&common::fixture("external-escape.gltf")).unwrap_err();
    assert_eq!(error.code(), "ICE3_ASSET_INVALID_GLB");
}
