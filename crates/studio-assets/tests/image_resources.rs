use std::path::Path;

use base64::Engine;
use serde_json::json;
use studio_assets::SourceGlb;
use studio_assets::image::{read_image_buffer_view, read_image_uri, validate_image_bytes};

const PNG: &[u8] = &[
    0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R',
];

#[test]
fn identifies_supported_images_from_bytes() {
    let image = validate_image_bytes(PNG.to_vec(), Some("image/png"), 1024).unwrap();

    assert_eq!(image.media_type, "image/png");
    assert_eq!(image.digest.len(), 64);
}

#[test]
fn rejects_unknown_image_bytes() {
    let error = validate_image_bytes(b"<svg></svg>".to_vec(), None, 1024).unwrap_err();

    assert!(error.to_string().contains("unsupported image"));
}

#[test]
fn rejects_declared_media_type_mismatch() {
    let error = validate_image_bytes(PNG.to_vec(), Some("image/jpeg"), 1024).unwrap_err();

    assert!(error.to_string().contains("does not match"));
}

#[test]
fn rejects_parent_directory_escape() {
    let root = tempfile::tempdir().unwrap();
    let error = read_image_uri("../outside.png", root.path(), None, 1024).unwrap_err();

    assert!(error.to_string().contains("escapes the source directory"));
}

#[cfg(unix)]
#[test]
fn rejects_a_symlink_that_escapes_the_source_directory() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("image.png"), PNG).unwrap();
    symlink(outside.path(), root.path().join("linked")).unwrap();

    let error = read_image_uri("linked/image.png", root.path(), None, 1024).unwrap_err();

    assert!(error.to_string().contains("escapes the source directory"));
}

#[test]
fn rejects_an_external_file_above_the_limit_before_reading_it() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("image.png"), PNG).unwrap();

    let error = read_image_uri("image.png", root.path(), None, 8).unwrap_err();

    assert!(error.to_string().contains("size limit"));
}

#[test]
fn rejects_a_data_uri_above_the_limit() {
    let encoded = base64::engine::general_purpose::STANDARD.encode(PNG);
    let uri = format!("data:image/png;base64,{encoded}");

    let error = read_image_uri(&uri, Path::new("."), None, 8).unwrap_err();

    assert!(error.to_string().contains("size limit"));
}

#[test]
fn reads_an_embedded_buffer_view_image() {
    let source = SourceGlb {
        json: json!({
            "images": [{
                "bufferView": 0,
                "mimeType": "image/png"
            }],
            "bufferViews": [{
                "buffer": 0,
                "byteOffset": 0,
                "byteLength": PNG.len()
            }]
        }),
        binary: Vec::new(),
        buffers: vec![PNG.to_vec()],
        base_dir: std::env::temp_dir(),
        digest: "source".into(),
        source_bytes: Vec::new(),
    };

    let image = read_image_buffer_view(&source, &source.json["images"][0], 1024).unwrap();

    assert_eq!(image.bytes, PNG);
    assert_eq!(image.media_type, "image/png");
}

#[test]
fn rejects_an_out_of_range_buffer_view_image() {
    let source = SourceGlb {
        json: json!({
            "images": [{
                "bufferView": 4,
                "mimeType": "image/png"
            }],
            "bufferViews": []
        }),
        binary: Vec::new(),
        buffers: vec![PNG.to_vec()],
        base_dir: std::env::temp_dir(),
        digest: "source".into(),
        source_bytes: Vec::new(),
    };

    let error = read_image_buffer_view(&source, &source.json["images"][0], 1024).unwrap_err();

    assert!(error.to_string().contains("buffer view"));
}
