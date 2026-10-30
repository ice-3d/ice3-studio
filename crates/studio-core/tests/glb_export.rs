use studio_core::document::Document;
use studio_core::export::glb::export_glb;

#[test]
fn exports_a_standard_glb_header_and_json_chunk() {
    let bytes = export_glb(&Document::new("empty")).unwrap();
    assert_eq!(&bytes[0..4], b"glTF");
    assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 2);
    let json_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
    assert!(json_len > 0);
}

#[test]
fn exports_stable_bytes_for_the_same_revision() {
    let document = Document::new("stable");
    assert_eq!(
        export_glb(&document).unwrap(),
        export_glb(&document).unwrap(),
    );
}
