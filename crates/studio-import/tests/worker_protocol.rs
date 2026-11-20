use studio_import::{
    ImportWarning, NativeImportLimits, SourceFormat, SourceMesh, SourceNode, SourcePrimitive,
    SourceResource, SourceScene, WorkerResponse, decode_worker_response, validate_scene_limits,
};

fn fixture_scene() -> SourceScene {
    SourceScene {
        asset_id: "fixture".into(),
        source_format: SourceFormat::Gltf,
        nodes: vec![SourceNode {
            id: "fixture/node/0".into(),
            name: "Fixture".into(),
            parent: None,
            matrix: [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            mesh: Some(0),
            material: None,
            extras: serde_json::json!({}),
        }],
        meshes: vec![SourceMesh {
            id: "fixture/mesh/0".into(),
            primitives: vec![SourcePrimitive {
                positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                normals: None,
                colors: None,
                uvs: None,
                indices: vec![0, 1, 2],
                material: None,
            }],
            extras: serde_json::json!({}),
        }],
        materials: Vec::new(),
        resources: Vec::new(),
        warnings: vec![ImportWarning {
            code: "ICE3_STUDIO_IMPORT_RESOURCE_IGNORED".into(),
            message: "fixture warning".into(),
            path: None,
        }],
        asset_extras: serde_json::json!({}),
    }
}

fn scene_with_triangles(count: usize) -> SourceScene {
    let mut scene = fixture_scene();
    scene.meshes[0].primitives[0].indices = (0..count * 3)
        .map(|index| u32::try_from(index % 3).unwrap())
        .collect();
    scene
}

#[test]
fn protocol_two_round_trips_image_resources() {
    let mut scene = fixture_scene();
    scene.resources.push(SourceResource {
        digest: "a".repeat(64),
        media_type: "image/png".into(),
        bytes: vec![0x89, b'P', b'N', b'G', 0, 1, 2, 3],
    });
    let response = WorkerResponse::V2 {
        source_scene: scene,
        warnings: Vec::new(),
    };
    let encoded = bincode::serde::encode_to_vec(&response, bincode::config::standard()).unwrap();

    let decoded = decode_worker_response(&encoded).unwrap();

    assert_eq!(
        decoded.resources[0].bytes,
        [0x89, b'P', b'N', b'G', 0, 1, 2, 3]
    );
}

#[test]
fn rejects_a_scene_above_the_triangle_limit() {
    let scene = scene_with_triangles(11);
    let limits = NativeImportLimits {
        max_triangles: 10,
        ..NativeImportLimits::default()
    };

    let error = validate_scene_limits(&scene, &limits).unwrap_err();

    assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_LIMIT_EXCEEDED");
}

#[test]
fn rejects_a_scene_above_the_node_limit() {
    let scene = fixture_scene();
    let limits = NativeImportLimits {
        max_nodes: 0,
        ..NativeImportLimits::default()
    };

    let error = validate_scene_limits(&scene, &limits).unwrap_err();

    assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_LIMIT_EXCEEDED");
}

#[test]
fn rejects_a_scene_above_the_memory_limit() {
    let scene = fixture_scene();
    let limits = NativeImportLimits {
        max_memory_bytes: 1,
        ..NativeImportLimits::default()
    };

    let error = validate_scene_limits(&scene, &limits).unwrap_err();

    assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_LIMIT_EXCEEDED");
}

#[test]
fn rejects_image_resources_above_the_scene_memory_limit() {
    let mut scene = empty_scene();
    scene.resources.push(SourceResource {
        digest: "a".repeat(64),
        media_type: "image/png".into(),
        bytes: vec![0; 8 * 1024],
    });
    let limits = NativeImportLimits {
        max_memory_bytes: 4 * 1024,
        ..NativeImportLimits::default()
    };

    let error = validate_scene_limits(&scene, &limits).unwrap_err();

    assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_LIMIT_EXCEEDED");
}

#[test]
fn rejects_more_image_resources_than_the_protocol_allows() {
    let mut scene = empty_scene();
    scene.resources = (0..=studio_assets::MAX_TEXTURE_COUNT)
        .map(|index| SourceResource {
            digest: format!("{index:064x}"),
            media_type: "image/png".into(),
            bytes: Vec::new(),
        })
        .collect();

    let error = validate_scene_limits(&scene, &NativeImportLimits::default()).unwrap_err();

    assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_LIMIT_EXCEEDED");
}

#[test]
fn rejects_a_worker_with_the_wrong_protocol_version() {
    let response = WorkerResponse::V1 {
        source_scene: fixture_scene(),
        warnings: Vec::new(),
    };
    let encoded = bincode::serde::encode_to_vec(&response, bincode::config::standard()).unwrap();

    let error = decode_worker_response(&encoded).unwrap_err();

    assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_WORKER_PROTOCOL_INVALID");
}

fn empty_scene() -> SourceScene {
    SourceScene {
        asset_id: "empty".into(),
        source_format: SourceFormat::Gltf,
        nodes: Vec::new(),
        meshes: Vec::new(),
        materials: Vec::new(),
        resources: Vec::new(),
        warnings: Vec::new(),
        asset_extras: serde_json::Value::Null,
    }
}

#[test]
fn preserves_a_structured_worker_failure_code() {
    let response = WorkerResponse::V2Failure {
        code: "ICE3_STUDIO_IMPORT_INVALID_SOURCE".into(),
        message: "truncated source".into(),
    };
    let encoded = bincode::serde::encode_to_vec(&response, bincode::config::standard()).unwrap();

    let error = decode_worker_response(&encoded).unwrap_err();

    assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_INVALID_SOURCE");
    assert!(error.to_string().contains("truncated source"));
}

#[test]
fn rejects_a_worker_response_with_trailing_bytes() {
    let response = WorkerResponse::V2 {
        source_scene: fixture_scene(),
        warnings: Vec::new(),
    };
    let mut encoded =
        bincode::serde::encode_to_vec(&response, bincode::config::standard()).unwrap();
    encoded.extend_from_slice(b"polluted");

    let error = decode_worker_response(&encoded).unwrap_err();

    assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_WORKER_PROTOCOL_INVALID");
}
