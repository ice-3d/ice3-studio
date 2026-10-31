use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::document::NodeContent;
use crate::geometry::EvaluatedMesh;
use crate::gpu_contract::{BackendError, GeometryBackendKind, ResidentGeometryId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GeometryCacheMetadata {
    pub backend: GeometryBackendKind,
    pub kernel_version: String,
    pub parity_status: String,
}

impl GeometryCacheMetadata {
    pub fn cpu() -> Self {
        Self {
            backend: GeometryBackendKind::Cpu,
            kernel_version: "geometry-kernel-v1".into(),
            parity_status: "reference".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeometryCacheEntry {
    pub mesh: EvaluatedMesh,
    pub metadata: GeometryCacheMetadata,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeometryEvaluation {
    pub mesh: EvaluatedMesh,
    pub metadata: GeometryCacheMetadata,
    pub resident_geometry_id: Option<ResidentGeometryId>,
}

pub trait GeometryEvaluator: Send + Sync {
    fn select_cache_metadata(&self, content: &NodeContent) -> GeometryCacheMetadata;
    fn evaluate(&self, content: &NodeContent) -> Result<GeometryEvaluation, BackendError>;
    fn resident_geometry_id(&self, _content: &NodeContent) -> Option<ResidentGeometryId> {
        None
    }
}

pub trait GeometryStore {
    fn load(&self, key: &str) -> Option<EvaluatedMesh>;
    fn store(&mut self, key: String, mesh: EvaluatedMesh);
    fn load_entry(&self, key: &str) -> Option<GeometryCacheEntry> {
        self.load(key).map(|mesh| GeometryCacheEntry {
            mesh,
            metadata: GeometryCacheMetadata::cpu(),
        })
    }
    fn store_entry(&mut self, key: String, entry: GeometryCacheEntry) {
        self.store(key, entry.mesh);
    }
}

#[derive(Default)]
pub struct MemoryGeometryStore {
    entries: BTreeMap<String, GeometryCacheEntry>,
}

impl GeometryStore for MemoryGeometryStore {
    fn load(&self, key: &str) -> Option<EvaluatedMesh> {
        self.entries.get(key).map(|entry| entry.mesh.clone())
    }

    fn store(&mut self, key: String, mesh: EvaluatedMesh) {
        self.store_entry(
            key,
            GeometryCacheEntry {
                mesh,
                metadata: GeometryCacheMetadata::cpu(),
            },
        );
    }

    fn load_entry(&self, key: &str) -> Option<GeometryCacheEntry> {
        self.entries.get(key).cloned()
    }

    fn store_entry(&mut self, key: String, entry: GeometryCacheEntry) {
        self.entries.entry(key).or_insert(entry);
    }
}
