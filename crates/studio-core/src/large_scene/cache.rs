use std::collections::BTreeMap;

use crate::geometry::EvaluatedMesh;

pub trait GeometryStore {
    fn load(&self, key: &str) -> Option<EvaluatedMesh>;
    fn store(&mut self, key: String, mesh: EvaluatedMesh);
}

#[derive(Default)]
pub struct MemoryGeometryStore {
    entries: BTreeMap<String, EvaluatedMesh>,
}

impl GeometryStore for MemoryGeometryStore {
    fn load(&self, key: &str) -> Option<EvaluatedMesh> {
        self.entries.get(key).cloned()
    }

    fn store(&mut self, key: String, mesh: EvaluatedMesh) {
        self.entries.entry(key).or_insert(mesh);
    }
}
