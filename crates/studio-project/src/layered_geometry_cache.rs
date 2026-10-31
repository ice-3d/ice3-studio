use std::path::Path;
use std::sync::Mutex;

use studio_core::geometry::EvaluatedMesh;
use studio_core::large_scene::cache::{
    GeometryCacheEntry, GeometryCacheMetadata, GeometryStore, MemoryGeometryStore,
};

use crate::geometry_cache::{GeometryCacheError, GeometryDiskCache};

pub const DEFAULT_LAYERED_CACHE_BYTES: u64 = 256 * 1024 * 1024;

pub struct LayeredGeometryCache {
    memory: Mutex<MemoryGeometryStore>,
    disk: GeometryDiskCache,
}

impl LayeredGeometryCache {
    pub fn new(project_dir: impl AsRef<Path>, max_bytes: u64) -> Result<Self, GeometryCacheError> {
        Ok(Self {
            memory: Mutex::new(MemoryGeometryStore::default()),
            disk: GeometryDiskCache::with_max_bytes(project_dir, max_bytes)?,
        })
    }
}

impl GeometryStore for LayeredGeometryCache {
    fn load(&self, key: &str) -> Option<EvaluatedMesh> {
        self.load_entry(key).map(|entry| entry.mesh)
    }

    fn store(&mut self, key: String, mesh: EvaluatedMesh) {
        self.store_entry(
            key,
            GeometryCacheEntry {
                mesh,
                metadata: GeometryCacheMetadata::cpu(),
                resident_geometry_id: None,
            },
        );
    }

    fn load_entry(&self, key: &str) -> Option<GeometryCacheEntry> {
        let mut memory = self
            .memory
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(entry) = memory.load_entry(key) {
            return Some(entry);
        }
        let entry = self.disk.load_entry(key)?;
        memory.store_entry(key.to_owned(), entry.clone());
        Some(entry)
    }

    fn store_entry(&mut self, key: String, entry: GeometryCacheEntry) {
        self.memory
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .store_entry(key.clone(), entry.clone());
        self.disk.store_entry(key, entry);
    }
}
