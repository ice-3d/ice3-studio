mod support;

use serde_json::Value;
use studio_core::export::glb::export_glb;
use support::shared_pump_document;

fn read_glb_json(bytes: &[u8]) -> Value {
    assert_eq!(&bytes[0..4], b"glTF");
    let json_length = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    let chunk_type = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
    assert_eq!(chunk_type, 0x4E4F_534A);
    serde_json::from_slice(&bytes[20..20 + json_length]).unwrap()
}

#[test]
fn exports_identical_instances_through_one_shared_mesh() {
    let document = shared_pump_document();
    let bytes = export_glb(&document).unwrap();
    let json = read_glb_json(&bytes);
    assert_eq!(json["meshes"].as_array().unwrap().len(), 1);
    assert_eq!(json["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(json["nodes"][0]["mesh"], 0);
    assert_eq!(json["nodes"][1]["mesh"], 0);
    assert_eq!(json["nodes"][0]["extras"]["ice3"]["id"], "node-000001/body");
}
