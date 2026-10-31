mod buffer_pool;
mod pipeline_cache;
mod staging_ring;

use std::sync::Arc;

use studio_core::gpu_contract::BackendError;
use wgpu::{Device, Queue};

pub(crate) use buffer_pool::BufferLease;
pub use buffer_pool::{BufferClass, BufferPool, BufferRole};
pub use pipeline_cache::{PipelineCache, PipelineKey};
pub(crate) use staging_ring::StagingRing;

use crate::GpuHostLimits;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResourceStats {
    pub pipeline_count: usize,
    pub buffer_allocation_count: usize,
    pub pooled_bytes: u64,
    pub peak_resident_bytes: u64,
}

pub struct DeviceResources {
    pipelines: PipelineCache,
    buffers: BufferPool,
    staging: StagingRing,
}

impl DeviceResources {
    pub fn create(
        device: &Device,
        _queue: &Queue,
        limits: &GpuHostLimits,
    ) -> Result<Arc<Self>, BackendError> {
        Ok(Arc::new(Self {
            pipelines: PipelineCache::new(device)?,
            buffers: BufferPool::new(limits.max_buffer_bytes),
            staging: StagingRing::new(),
        }))
    }

    pub fn stats(&self) -> ResourceStats {
        let (allocations, pooled_bytes, peak_resident_bytes) = self.buffers.stats();
        ResourceStats {
            pipeline_count: self.pipelines.count(),
            buffer_allocation_count: allocations,
            pooled_bytes,
            peak_resident_bytes,
        }
    }

    pub(crate) fn pipelines(&self) -> &PipelineCache {
        &self.pipelines
    }

    pub(crate) fn buffers(&self) -> &BufferPool {
        &self.buffers
    }

    pub(crate) fn staging(&self) -> &StagingRing {
        &self.staging
    }

    pub fn ready_staging_slots(&self) -> usize {
        self.staging.ready_count()
    }
}
