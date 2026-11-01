use serde_json::{Value, json};
use studio_assets::{SourceGlb, read_glb_bytes};

pub fn fixture_source(meshes: Value, nodes: Value) -> SourceGlb {
    let glb = scene_glb(meshes, nodes, 3);
    read_glb_bytes(&glb, &std::env::temp_dir()).unwrap()
}

pub fn scene_glb(meshes: Value, nodes: Value, triangle_count: usize) -> Vec<u8> {
    let index_count = triangle_count * 3;
    let mut binary = Vec::new();
    for point in [[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
        for value in point {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    for index in 0..index_count {
        binary.extend_from_slice(&((index % 3) as u16).to_le_bytes());
    }
    while !binary.len().is_multiple_of(4) {
        binary.push(0);
    }
    let position_bytes = 36;
    let index_bytes = index_count * 2;
    let json = json!({
        "asset": {
            "version": "2.0",
            "extras": {
                "ice3": {
                    "profile": "ice3-prepared",
                    "profileVersion": "1.0",
                    "assetId": "partition-fixture",
                    "bounds": {"min": [0.0, 0.0, 0.0], "max": [4.0, 1.0, 0.0]}
                }
            }
        },
        "scene": 0,
        "scenes": [{"nodes": node_roots(&nodes)}],
        "nodes": nodes,
        "meshes": meshes,
        "materials": [
            {"name": "steel", "extras": {"ice3": {"id": "material-steel"}}},
            {"name": "concrete", "extras": {"ice3": {"id": "material-concrete"}}}
        ],
        "accessors": [
            {
                "bufferView": 0,
                "componentType": 5126,
                "count": 3,
                "type": "VEC3",
                "min": [0.0, 0.0, 0.0],
                "max": [1.0, 1.0, 0.0]
            },
            {
                "bufferView": 1,
                "componentType": 5123,
                "count": index_count,
                "type": "SCALAR"
            }
        ],
        "bufferViews": [
            {"buffer": 0, "byteOffset": 0, "byteLength": position_bytes},
            {"buffer": 0, "byteOffset": position_bytes, "byteLength": index_bytes}
        ],
        "buffers": [{"byteLength": binary.len()}]
    });
    pack_glb(&serde_json::to_vec(&json).unwrap(), &binary)
}

pub fn mesh(id: &str) -> Value {
    json!({
        "name": id,
        "primitives": [{"attributes": {"POSITION": 0}, "indices": 1}],
        "extras": {"ice3": {"id": id}}
    })
}

#[allow(dead_code)]
pub fn mesh_with_material(id: &str, material: usize) -> Value {
    json!({
        "name": id,
        "primitives": [{
            "attributes": {"POSITION": 0},
            "indices": 1,
            "material": material
        }],
        "extras": {"ice3": {"id": id}}
    })
}

pub fn node(id: &str, mesh: usize, x: f32) -> Value {
    json!({
        "name": id,
        "matrix": [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            x, 0.0, 0.0, 1.0
        ],
        "mesh": mesh,
        "extras": {"ice3": {"id": id, "displayName": id}}
    })
}

fn node_roots(nodes: &Value) -> Vec<usize> {
    (0..nodes.as_array().unwrap().len()).collect()
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
