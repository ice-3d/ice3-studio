use studio_import::{
    ImportWarning, NativeImportLimits, SourceFormat, SourceMesh, SourceNode, SourcePrimitive,
    SourceScene, WorkerResponse, decode_worker_response, validate_scene_limits,
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
                indices: vec![0, 1, 2],
                material: None,
            }],
            extras: serde_json::json!({}),
        }],
        materials: Vec::new(),
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
fn rejects_a_worker_with_the_wrong_protocol_version() {
    let response = WorkerResponse::V2 {
        source_scene: fixture_scene(),
        warnings: Vec::new(),
    };
    let encoded = bincode::serde::encode_to_vec(&response, bincode::config::standard()).unwrap();

    let error = decode_worker_response(&encoded).unwrap_err();

    assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_WORKER_PROTOCOL_INVALID");
}

#[test]
fn preserves_a_structured_worker_failure_code() {
    let response = WorkerResponse::V1Failure {
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
    let response = WorkerResponse::V1 {
        source_scene: fixture_scene(),
        warnings: Vec::new(),
    };
    let mut encoded =
        bincode::serde::encode_to_vec(&response, bincode::config::standard()).unwrap();
    encoded.extend_from_slice(b"polluted");

    let error = decode_worker_response(&encoded).unwrap_err();

    assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_WORKER_PROTOCOL_INVALID");
}
