use std::collections::BTreeMap;

use studio_core::scene_contract::{ElementRange, SceneInstanceChange};

#[derive(Debug, Clone, PartialEq)]
pub struct InstanceRecord {
    pub node_id: String,
    pub geometry_id: Option<String>,
    pub matrix: [f32; 16],
    pub material_id: Option<String>,
    pub visible: bool,
}

impl Default for InstanceRecord {
    fn default() -> Self {
        let mut matrix = [0.0; 16];
        matrix[0] = 1.0;
        matrix[5] = 1.0;
        matrix[10] = 1.0;
        matrix[15] = 1.0;
        Self {
            node_id: String::new(),
            geometry_id: None,
            matrix,
            material_id: None,
            visible: true,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstanceTableUpdate {
    pub changed_ranges: Vec<ElementRange>,
    pub changed_bytes: u64,
    pub removed_ranges: Vec<ElementRange>,
}

#[derive(Debug, Default)]
pub struct InstanceTable {
    records: Vec<InstanceRecord>,
    node_slots: BTreeMap<String, usize>,
    free_slots: Vec<usize>,
}

impl InstanceTable {
    pub fn apply_changes(&mut self, changes: Vec<SceneInstanceChange>) -> InstanceTableUpdate {
        let mut changed_slots = Vec::new();
        let mut changed_bytes = 0u64;
        for change in changes {
            let next = InstanceRecord {
                node_id: change.node_id.clone(),
                geometry_id: change.geometry_id,
                matrix: change.matrix,
                material_id: change.material_id,
                visible: change.visible,
            };
            let slot = if let Some(slot) = self.node_slots.get(&change.node_id).copied() {
                slot
            } else {
                let slot = self.free_slots.pop().unwrap_or_else(|| {
                    self.records.push(InstanceRecord::default());
                    self.records.len() - 1
                });
                self.node_slots.insert(change.node_id, slot);
                slot
            };
            if self.records[slot] == next {
                continue;
            }
            self.records[slot] = next;
            changed_slots.push(slot);
            changed_bytes = changed_bytes.saturating_add(64);
        }
        InstanceTableUpdate {
            changed_ranges: compress_slots(&changed_slots),
            changed_bytes,
            removed_ranges: Vec::new(),
        }
    }

    pub fn remove_nodes(&mut self, node_ids: &[String]) -> InstanceTableUpdate {
        let mut removed_slots = Vec::new();
        for node_id in node_ids {
            let Some(slot) = self.node_slots.remove(node_id) else {
                continue;
            };
            self.records[slot] = InstanceRecord::default();
            self.free_slots.push(slot);
            removed_slots.push(slot);
        }
        InstanceTableUpdate {
            changed_ranges: Vec::new(),
            changed_bytes: 0,
            removed_ranges: compress_slots(&removed_slots),
        }
    }

    pub fn records(&self) -> &[InstanceRecord] {
        &self.records
    }

    pub fn gpu_bytes(&self) -> u64 {
        self.records.len() as u64 * 64
    }
}

fn compress_slots(slots: &[usize]) -> Vec<ElementRange> {
    let mut sorted = slots.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut ranges = Vec::<ElementRange>::new();
    for slot in sorted {
        let slot = slot as u32;
        match ranges.last_mut() {
            Some(range) if range.start.saturating_add(range.count) == slot => range.count += 1,
            _ => ranges.push(ElementRange {
                start: slot,
                count: 1,
            }),
        }
    }
    ranges
}
