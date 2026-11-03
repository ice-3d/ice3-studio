use std::collections::HashSet;

use serde_json::{Value, json};
use studio_assets::{read_glb_bytes, validate_prepared_profile};
use studio_pipeline::{
    AssetProfile, RenderBatchMode, SemanticProxyMode, build_with_profile, lod::LodAllocationMode,
};

#[test]
fn fine_partition_glb_contains_edge_line_primitives() {
    let source = grid_source(1, 1);
    let profile = AssetProfile::direct_partition(500_000, 33_554_432);
    let artifact = build_with_profile(&source, "partition-cube", &profile).unwrap();
    let part = artifact
        .files
        .iter()
        .find(|file| file.relative_path.starts_with("parts/"))
        .unwrap();
    let parsed = read_glb_bytes(&part.bytes, &std::env::temp_dir()).unwrap();

    validate_prepared_profile(&parsed).unwrap();
    assert!(has_edge_primitive(&parsed.json));
    assert!(
        parsed.json["asset"]["extras"]["ice3"]["stats"]["edgeSegmentCount"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert_eq!(
        parsed.json["asset"]["extras"]["ice3"]["preparation"]["edgesPrepared"],
        true
    );
    assert!(artifact.stats.edge_segment_count > 0);
    assert!(artifact.stats.edge_bytes > 0);
}

#[test]
fn render_batch_glb_contains_edges_for_its_final_geometry() {
    let source = grid_source(2, 2);
    let mut profile = AssetProfile::direct_partition(500_000, 33_554_432);
    profile.max_triangles = 8;
    profile.render_batch = RenderBatchMode::Static;
    let artifact = build_with_profile(&source, "batch-edges", &profile).unwrap();

    assert!(artifact.stats.render_batch_count > 0);
    let parts = artifact
        .files
        .iter()
        .filter(|file| file.relative_path.starts_with("parts/"));
    let mut render_batch_nodes = HashSet::new();
    for part in parts {
        let parsed = read_glb_bytes(&part.bytes, &std::env::temp_dir()).unwrap();
        assert!(has_edge_primitive(&parsed.json));
        assert!(has_batch_primitive(&parsed.json));
        for node in parsed.json["nodes"].as_array().unwrap() {
            if let Some(id) = node
                .pointer("/extras/ice3/id")
                .and_then(Value::as_str)
                .filter(|id| id.contains("/render-batch-node/"))
            {
                assert!(
                    render_batch_nodes.insert(id.to_owned()),
                    "duplicate render-only node ID {id}"
                );
            }
        }
        let semantic_node_ids = parsed
            .json
            .pointer("/asset/extras/ice3/batching/semanticNodeIds")
            .and_then(Value::as_array)
            .unwrap();
        let semantic_selection_ids = parsed
            .json
            .pointer("/asset/extras/ice3/batching/semanticSelectionIds")
            .and_then(Value::as_array)
            .unwrap();
        let nodes = parsed.json["nodes"].as_array().unwrap();
        for (semantic_node_id, semantic_selection_id) in
            semantic_node_ids.iter().zip(semantic_selection_ids)
        {
            let semantic_node_id = semantic_node_id.as_str().unwrap();
            let node = nodes
                .iter()
                .find(|node| {
                    node.pointer("/extras/ice3/id") == Some(&Value::from(semantic_node_id))
                })
                .unwrap_or_else(|| panic!("semantic node {semantic_node_id} is missing"));
            assert_eq!(
                node.pointer("/extras/ice3/selectionId"),
                Some(semantic_selection_id)
            );
        }
    }
}

#[test]
fn lod_and_coarse_outputs_generate_edges_from_their_own_geometry() {
    let source = grid_source(4, 8);
    let mut profile = AssetProfile::agent_console_v1();
    profile.max_triangles = 200;
    profile.lod_ratio = 0.15;
    profile.lod_max_triangles = 512;
    profile.lod_min_unit_triangles = 8;
    profile.render_batch = RenderBatchMode::Off;

    let artifact = build_with_profile(&source, "lod-edges", &profile).unwrap();
    let parts_and_coarse = artifact
        .files
        .iter()
        .filter(|file| {
            file.relative_path.starts_with("parts/") || file.relative_path.starts_with("coarse/")
        })
        .collect::<Vec<_>>();
    assert!(
        parts_and_coarse
            .iter()
            .any(|file| file.relative_path.starts_with("coarse/"))
    );
    for file in parts_and_coarse {
        let parsed = read_glb_bytes(&file.bytes, &std::env::temp_dir()).unwrap();
        assert!(
            has_edge_primitive(&parsed.json),
            "{} has no render edges",
            file.relative_path
        );
        assert!(
            parsed.json["asset"]["extras"]["ice3"]["stats"]["edgeSegmentCount"]
                .as_u64()
                .unwrap()
                > 0,
            "{} has no edge stats",
            file.relative_path
        );
        assert_eq!(
            parsed.json["asset"]["extras"]["ice3"]["preparation"]["edgesPrepared"],
            true
        );
    }

    let coarse = artifact
        .files
        .iter()
        .find(|file| file.relative_path.starts_with("coarse/"))
        .unwrap();
    let parsed = read_glb_bytes(&coarse.bytes, &std::env::temp_dir()).unwrap();
    assert!(
        triangle_position_counts(&parsed.json)
            .into_iter()
            .any(|count| count < 81)
    );
}

#[test]
fn semantic_proxy_edges_follow_proxy_geometry() {
    let source = grid_source(4, 8);
    let mut profile = AssetProfile::agent_console_v1();
    profile.max_triangles = 200;
    profile.lod_ratio = 0.15;
    profile.lod_max_triangles = 512;
    profile.lod_min_unit_triangles = 8;
    profile.lod_allocation = LodAllocationMode::Adaptive;
    profile.semantic_proxy = SemanticProxyMode::Auto;
    profile.render_batch = RenderBatchMode::Off;

    let artifact = build_with_profile(&source, "proxy-edges", &profile).unwrap();
    let coarse = artifact
        .files
        .iter()
        .find(|file| file.relative_path.starts_with("coarse/"))
        .unwrap();
    let parsed = read_glb_bytes(&coarse.bytes, &std::env::temp_dir()).unwrap();

    assert!(has_edge_primitive(&parsed.json));
    let proxy_position_counts = triangle_position_counts(&parsed.json);
    assert!(!proxy_position_counts.is_empty());
    assert!(
        proxy_position_counts.iter().all(|count| *count == 8),
        "semantic proxy position counts: {proxy_position_counts:?}"
    );
}

#[test]
fn edge_bytes_count_toward_the_part_byte_budget() {
    let source = grid_source(1, 1);
    let mut profile = AssetProfile::direct_partition(500_000, 33_554_432);
    profile.generate_edges = false;
    let without_edges = build_with_profile(&source, "budget-edges", &profile).unwrap();
    profile.max_bytes = without_edges.files[0].byte_length;

    let rebuilt = build_with_profile(&source, "budget-edges", &profile).unwrap();
    assert_eq!(rebuilt.files[0].byte_length, profile.max_bytes);

    profile.generate_edges = true;
    let result = build_with_profile(&source, "budget-edges", &profile);
    assert!(result.is_err(), "edge bytes did not exceed maxBytes");
}

#[test]
fn tagged_nodes_use_one_asset_scoped_equipment_selection() {
    let mut source = grid_source(2, 1);
    for node in source.json["nodes"].as_array_mut().unwrap() {
        node["extras"]["ice3"]["metadata"] = json!({
            "tag": "P-101",
            "stage": "intake"
        });
    }
    let profile = AssetProfile::direct_partition(500_000, 33_554_432);
    let artifact = build_with_profile(&source, "tagged-asset", &profile).unwrap();
    let selection_ids = artifact
        .files
        .iter()
        .filter(|file| file.relative_path.starts_with("parts/"))
        .map(|file| read_glb_bytes(&file.bytes, &std::env::temp_dir()).unwrap())
        .flat_map(|parsed| {
            parsed.json["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|node| node.pointer("/extras/ice3/selectionId"))
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert!(!selection_ids.is_empty());
    assert!(
        selection_ids
            .iter()
            .all(|selection_id| selection_id == "tagged-asset/equipment/P-101")
    );
}

#[test]
fn skipping_edges_preserves_material_slots_in_canonical_order() {
    let source = grid_source(1, 1);
    let mut profile = AssetProfile::direct_partition(500_000, 33_554_432);
    profile.generate_edges = false;
    let artifact = build_with_profile(&source, "material-order", &profile).unwrap();
    let parsed = read_glb_bytes(&artifact.files[0].bytes, &std::env::temp_dir()).unwrap();

    let names = parsed.json["materials"]
        .as_array()
        .unwrap()
        .iter()
        .map(|material| material["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            "concrete",
            "concrete edges",
            "default",
            "default edges",
            "vertex colors"
        ]
    );
    assert!(!has_edge_primitive(&parsed.json));
    assert_eq!(
        parsed.json["asset"]["extras"]["ice3"]["stats"]["edgeSegmentCount"],
        0
    );
}

fn has_edge_primitive(json: &Value) -> bool {
    json["meshes"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|mesh| mesh["primitives"].as_array().into_iter().flatten())
        .any(|primitive| primitive["mode"] == 1 && primitive["extras"]["ice3"]["role"] == "edges")
}

fn has_batch_primitive(json: &Value) -> bool {
    json["meshes"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|mesh| mesh["primitives"].as_array().into_iter().flatten())
        .any(|primitive| {
            primitive["mode"] == 4
                && primitive["attributes"]["_ICE3_SEMANTIC_INDEX"]
                    .as_u64()
                    .is_some()
        })
}

fn triangle_position_counts(json: &Value) -> Vec<u64> {
    json["meshes"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|mesh| mesh["primitives"].as_array().into_iter().flatten())
        .filter(|primitive| primitive["mode"].as_u64().unwrap_or(4) == 4)
        .map(|primitive| {
            let accessor = primitive["attributes"]["POSITION"].as_u64().unwrap() as usize;
            json["accessors"][accessor]["count"].as_u64().unwrap()
        })
        .collect()
}

fn grid_source(mesh_count: usize, size: usize) -> studio_assets::SourceGlb {
    let stride = size + 1;
    let mut positions = Vec::new();
    for y in 0..=size {
        for x in 0..=size {
            positions.extend_from_slice(&[
                x as f32,
                y as f32,
                (((x * 3 + y * 5) % 7) as f32) * 0.125,
            ]);
        }
    }
    let mut indices = Vec::new();
    for y in 0..size {
        for x in 0..size {
            let a = (y * stride + x) as u32;
            let b = a + 1;
            let c = a + stride as u32;
            let d = c + 1;
            indices.extend_from_slice(&[a, b, c, b, d, c]);
        }
    }
    let mut binary = bytemuck::cast_slice(&positions).to_vec();
    while !binary.len().is_multiple_of(4) {
        binary.push(0);
    }
    let index_offset = binary.len();
    binary.extend_from_slice(bytemuck::cast_slice(&indices));

    let meshes = (0..mesh_count)
        .map(|index| {
            json!({
                "name": format!("mesh-{index}"),
                "primitives": [{
                    "attributes": {"POSITION": 0},
                    "indices": 1,
                    "material": 0
                }],
                "extras": {"ice3": {"id": format!("mesh-{index}")}}
            })
        })
        .collect::<Vec<_>>();
    let nodes = (0..mesh_count)
        .map(|index| {
            json!({
                "name": format!("unit-{index}"),
                "matrix": [
                    1.0, 0.0, 0.0, 0.0,
                    0.0, 1.0, 0.0, 0.0,
                    0.0, 0.0, 1.0, 0.0,
                    index as f32 * 12.0, 0.0, 0.0, 1.0
                ],
                "mesh": index,
                "extras": {
                    "ice3": {
                        "id": format!("unit-{index}"),
                        "displayName": format!("unit-{index}")
                    }
                }
            })
        })
        .collect::<Vec<_>>();
    let json = json!({
        "asset": {
            "version": "2.0",
            "extras": {
                "ice3": {
                    "profile": "ice3-prepared",
                    "profileVersion": "1.0",
                    "assetId": "partition-edges-grid"
                }
            }
        },
        "scene": 0,
        "scenes": [{"nodes": (0..mesh_count).collect::<Vec<_>>()}],
        "nodes": nodes,
        "meshes": meshes,
        "materials": [{
            "name": "concrete",
            "extras": {"ice3": {"id": "material-concrete", "edgeColor": "#d7e2ea"}}
        }],
        "accessors": [
            {
                "bufferView": 0,
                "componentType": 5126,
                "count": positions.len() / 3,
                "type": "VEC3",
                "min": [0.0, 0.0, 0.0],
                "max": [size as f64, size as f64, 0.75]
            },
            {
                "bufferView": 1,
                "componentType": 5125,
                "count": indices.len(),
                "type": "SCALAR"
            }
        ],
        "bufferViews": [
            {"buffer": 0, "byteOffset": 0, "byteLength": positions.len() * 4},
            {"buffer": 0, "byteOffset": index_offset, "byteLength": indices.len() * 4}
        ],
        "buffers": [{"byteLength": binary.len()}]
    });
    read_glb_bytes(
        &pack_glb(&serde_json::to_vec(&json).unwrap(), &binary),
        &std::env::temp_dir(),
    )
    .unwrap()
}

fn pack_glb(json: &[u8], binary: &[u8]) -> Vec<u8> {
    let mut json_chunk = json.to_vec();
    while !json_chunk.len().is_multiple_of(4) {
        json_chunk.push(b' ');
    }
    let total = 12 + 8 + json_chunk.len() + 8 + binary.len();
    let mut output = Vec::with_capacity(total);
    output.extend_from_slice(&0x4654_6C67_u32.to_le_bytes());
    output.extend_from_slice(&2_u32.to_le_bytes());
    output.extend_from_slice(&(total as u32).to_le_bytes());
    output.extend_from_slice(&(json_chunk.len() as u32).to_le_bytes());
    output.extend_from_slice(&0x4E4F_534A_u32.to_le_bytes());
    output.extend_from_slice(&json_chunk);
    output.extend_from_slice(&(binary.len() as u32).to_le_bytes());
    output.extend_from_slice(&0x004E_4942_u32.to_le_bytes());
    output.extend_from_slice(binary);
    output
}
