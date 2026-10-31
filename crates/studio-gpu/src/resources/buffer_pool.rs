use std::collections::BTreeMap;
use std::sync::Mutex;

use studio_core::gpu_contract::BackendError;
use wgpu::{Buffer, BufferDescriptor, BufferUsages, Device};

const MIB: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BufferClass {
    OneMiB,
    FourMiB,
    SixteenMiB,
    SixtyFourMiB,
    TwoHundredFiftySixMiB,
    Exact(u64),
}

impl BufferClass {
    pub const fn for_bytes(bytes: u64) -> Self {
        if bytes <= MIB {
            Self::OneMiB
        } else if bytes <= 4 * MIB {
            Self::FourMiB
        } else if bytes <= 16 * MIB {
            Self::SixteenMiB
        } else if bytes <= 64 * MIB {
            Self::SixtyFourMiB
        } else if bytes <= 256 * MIB {
            Self::TwoHundredFiftySixMiB
        } else {
            Self::Exact(bytes)
        }
    }

    pub const fn allocated_bytes(self) -> u64 {
        match self {
            Self::OneMiB => MIB,
            Self::FourMiB => 4 * MIB,
            Self::SixteenMiB => 16 * MIB,
            Self::SixtyFourMiB => 64 * MIB,
            Self::TwoHundredFiftySixMiB => 256 * MIB,
            Self::Exact(bytes) => bytes,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BufferRole {
    StaticTopology,
    DynamicAttribute,
    DeviceOutput,
    ReadbackStaging,
    Command,
}

#[derive(Debug, Clone)]
pub struct BufferLease {
    pub buffer: Buffer,
    pub class: BufferClass,
    pub role: BufferRole,
    pub generation: u64,
    pub requested_bytes: u64,
}

#[derive(Debug, Default)]
struct PoolState {
    free: BTreeMap<(BufferRole, BufferClass), Vec<Buffer>>,
    allocations: usize,
    allocation_bytes: u64,
    pooled_bytes: u64,
    peak_bytes: u64,
    generation: u64,
}

pub struct BufferPool {
    max_bytes: u64,
    state: Mutex<PoolState>,
}

impl BufferPool {
    pub(crate) fn new(max_bytes: u64) -> Self {
        Self {
            max_bytes,
            state: Mutex::new(PoolState::default()),
        }
    }

    pub(crate) fn acquire(
        &self,
        device: &Device,
        role: BufferRole,
        usage: BufferUsages,
        bytes: u64,
        label: &str,
    ) -> Result<BufferLease, BackendError> {
        if bytes == 0 || bytes > self.max_bytes {
            return Err(BackendError::BudgetExceeded);
        }
        let class = BufferClass::for_bytes(bytes);
        if class.allocated_bytes() > self.max_bytes {
            return Err(BackendError::BudgetExceeded);
        }
        let key = (role, class);
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let buffer =
            if let Some(buffer) = state.free.get_mut(&key).and_then(|buffers| buffers.pop()) {
                state.pooled_bytes = state.pooled_bytes.saturating_sub(buffer.size());
                buffer
            } else {
                if state
                    .allocation_bytes
                    .saturating_add(class.allocated_bytes())
                    > self.max_bytes
                {
                    return Err(BackendError::BudgetExceeded);
                }
                let buffer = device.create_buffer(&BufferDescriptor {
                    label: Some(label),
                    size: class.allocated_bytes(),
                    usage,
                    mapped_at_creation: false,
                });
                state.allocations += 1;
                state.allocation_bytes = state.allocation_bytes.saturating_add(buffer.size());
                state.peak_bytes = state.peak_bytes.max(state.allocation_bytes);
                buffer
            };
        state.generation = state.generation.wrapping_add(1);
        Ok(BufferLease {
            buffer,
            class,
            role,
            generation: state.generation,
            requested_bytes: bytes,
        })
    }

    pub(crate) fn release(&self, lease: BufferLease) {
        let _generation = lease.generation;
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.pooled_bytes = state.pooled_bytes.saturating_add(lease.buffer.size());
        state
            .free
            .entry((lease.role, lease.class))
            .or_default()
            .push(lease.buffer);
    }

    pub(crate) fn stats(&self) -> (usize, u64, u64) {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        (state.allocations, state.pooled_bytes, state.peak_bytes)
    }
}
