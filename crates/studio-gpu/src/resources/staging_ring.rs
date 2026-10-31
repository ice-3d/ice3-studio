use std::sync::Mutex;

use studio_core::gpu_contract::BackendError;
use wgpu::{Buffer, BufferDescriptor, BufferUsages, Device};

use super::BufferClass;

const STAGING_SLOTS: usize = 3;

struct Slot {
    buffer: Option<Buffer>,
    in_flight: bool,
}

struct RingState {
    slots: Vec<Slot>,
}

pub struct StagingLease {
    pub index: usize,
    pub buffer: Buffer,
    _requested_bytes: u64,
}

pub struct StagingRing {
    state: Mutex<RingState>,
}

impl StagingRing {
    pub(crate) fn new() -> Self {
        Self {
            state: Mutex::new(RingState {
                slots: (0..STAGING_SLOTS)
                    .map(|_| Slot {
                        buffer: None,
                        in_flight: false,
                    })
                    .collect(),
            }),
        }
    }

    pub(crate) fn acquire(
        &self,
        device: &Device,
        requested_bytes: u64,
    ) -> Result<StagingLease, BackendError> {
        if requested_bytes == 0 {
            return Err(BackendError::Kernel {
                message: "staging requests must not be empty".into(),
            });
        }
        let required = BufferClass::for_bytes(requested_bytes).allocated_bytes();
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let index = state
            .slots
            .iter()
            .position(|slot| !slot.in_flight)
            .ok_or_else(|| BackendError::Kernel {
                message: "all staging slots are in flight".into(),
            })?;
        let slot = &mut state.slots[index];
        if slot
            .buffer
            .as_ref()
            .is_none_or(|buffer| buffer.size() < required)
        {
            slot.buffer = Some(device.create_buffer(&BufferDescriptor {
                label: Some("ICE3 resident staging slot"),
                size: required,
                usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }));
        }
        slot.in_flight = true;
        Ok(StagingLease {
            index,
            buffer: slot.buffer.as_ref().expect("staging slot buffer").clone(),
            _requested_bytes: requested_bytes,
        })
    }

    pub(crate) fn complete(&self, sequence: u64) {
        if sequence == 0 {
            return;
        }
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for slot in &mut state.slots {
            slot.in_flight = false;
        }
    }

    pub(crate) fn ready_count(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .slots
            .iter()
            .filter(|slot| !slot.in_flight)
            .count()
    }
}
