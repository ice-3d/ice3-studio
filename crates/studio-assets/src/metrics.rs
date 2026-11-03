use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssetMetrics {
    pub encoded_bytes: u64,
    pub json_chunk_bytes: u64,
    pub bin_chunk_bytes: u64,
    pub node_count: u64,
    pub max_node_depth: u64,
    pub mesh_count: u64,
    pub primitive_count: u64,
    pub triangle_count: u64,
    pub edge_segment_count: u64,
    pub material_count: u64,
    pub selection_group_count: u64,
}
