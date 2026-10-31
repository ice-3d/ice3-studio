use std::collections::BTreeMap;

use studio_core::geometry::Bounds;
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

    pub fn len(&self) -> usize {
        self.node_slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.node_slots.is_empty()
    }

    pub fn gpu_bytes(&self) -> u64 {
        self.records.len() as u64 * 64
    }

    pub fn matrix_data(&self) -> Vec<[f32; 4]> {
        self.records
            .iter()
            .flat_map(|record| {
                let matrix = record.matrix;
                [
                    [matrix[0], matrix[1], matrix[2], matrix[3]],
                    [matrix[4], matrix[5], matrix[6], matrix[7]],
                    [matrix[8], matrix[9], matrix[10], matrix[11]],
                    [matrix[12], matrix[13], matrix[14], matrix[15]],
                ]
            })
            .collect()
    }

    pub fn matrix_range_data(&self, ranges: &[ElementRange]) -> Vec<(ElementRange, Vec<[f32; 4]>)> {
        ranges
            .iter()
            .filter_map(|range| {
                let start = range.start as usize;
                let end = start.checked_add(range.count as usize)?;
                let records = self.records.get(start..end)?;
                let data = records
                    .iter()
                    .flat_map(|record| {
                        let matrix = record.matrix;
                        [
                            [matrix[0], matrix[1], matrix[2], matrix[3]],
                            [matrix[4], matrix[5], matrix[6], matrix[7]],
                            [matrix[8], matrix[9], matrix[10], matrix[11]],
                            [matrix[12], matrix[13], matrix[14], matrix[15]],
                        ]
                    })
                    .collect();
                Some((*range, data))
            })
            .collect()
    }

    pub fn bounds_data(&self, geometry_bounds: &BTreeMap<String, Bounds>) -> Vec<[f32; 4]> {
        self.records
            .iter()
            .map(|record| {
                let Some(bounds) = record
                    .geometry_id
                    .as_ref()
                    .and_then(|id| geometry_bounds.get(id))
                else {
                    return [0.0, 0.0, 0.0, 0.0];
                };
                let centre = [
                    (bounds.min.x + bounds.max.x) * 0.5,
                    (bounds.min.y + bounds.max.y) * 0.5,
                    (bounds.min.z + bounds.max.z) * 0.5,
                ];
                let extent = [
                    (bounds.max.x - bounds.min.x) * 0.5,
                    (bounds.max.y - bounds.min.y) * 0.5,
                    (bounds.max.z - bounds.min.z) * 0.5,
                ];
                let m = record.matrix;
                let world_centre = [
                    m[0] * centre[0] + m[4] * centre[1] + m[8] * centre[2] + m[12],
                    m[1] * centre[0] + m[5] * centre[1] + m[9] * centre[2] + m[13],
                    m[2] * centre[0] + m[6] * centre[1] + m[10] * centre[2] + m[14],
                ];
                let scale = [0usize, 4, 8]
                    .into_iter()
                    .map(|offset| {
                        (m[offset] * m[offset]
                            + m[offset + 1] * m[offset + 1]
                            + m[offset + 2] * m[offset + 2])
                            .sqrt()
                    })
                    .fold(0.0f32, f32::max);
                let radius =
                    (extent[0] * extent[0] + extent[1] * extent[1] + extent[2] * extent[2]).sqrt()
                        * scale;
                [world_centre[0], world_centre[1], world_centre[2], radius]
            })
            .collect()
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
