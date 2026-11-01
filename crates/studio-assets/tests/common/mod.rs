use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;
use tempfile::TempDir;

pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

pub fn packed_fixture(name: &str, binary: &[u8]) -> (TempDir, PathBuf) {
    let json = fs::read(fixture(name)).expect("fixture JSON");
    let glb = pack_glb(&json, binary);
    let directory = tempfile::tempdir().expect("temporary fixture directory");
    let path = directory.path().join(name.replace(".json", ".glb"));
    fs::write(&path, glb).expect("write GLB fixture");
    (directory, path)
}

pub fn pack_glb(json: &[u8], binary: &[u8]) -> Vec<u8> {
    let mut json_chunk = json.to_vec();
    while json_chunk.len() % 4 != 0 {
        json_chunk.push(b' ');
    }
    let mut binary_chunk = binary.to_vec();
    while binary_chunk.len() % 4 != 0 {
        binary_chunk.push(0);
    }
    let total = 12 + 8 + json_chunk.len() + 8 + binary_chunk.len();
    let mut glb = Vec::with_capacity(total);
    glb.extend_from_slice(&0x4654_6C67_u32.to_le_bytes());
    glb.extend_from_slice(&2_u32.to_le_bytes());
    glb.extend_from_slice(&(total as u32).to_le_bytes());
    glb.extend_from_slice(&(json_chunk.len() as u32).to_le_bytes());
    glb.extend_from_slice(&0x4E4F_534A_u32.to_le_bytes());
    glb.extend_from_slice(&json_chunk);
    glb.extend_from_slice(&(binary_chunk.len() as u32).to_le_bytes());
    glb.extend_from_slice(&0x004E_4942_u32.to_le_bytes());
    glb.extend_from_slice(&binary_chunk);
    glb
}

pub fn minimal_binary() -> Vec<u8> {
    let positions: [[f32; 3]; 3] = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    let indices: [u16; 3] = [0, 1, 2];
    let mut binary = Vec::new();
    for value in positions.into_iter().flatten() {
        binary.extend_from_slice(&value.to_le_bytes());
    }
    for value in indices {
        binary.extend_from_slice(&value.to_le_bytes());
    }
    while binary.len() % 4 != 0 {
        binary.push(0);
    }
    binary
}

#[allow(dead_code)]
pub fn json_asset(name: &str) -> Value {
    serde_json::from_slice(&fs::read(fixture(name)).expect("fixture JSON")).expect("valid JSON")
}
