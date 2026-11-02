use crate::PipelineError;
use crate::lod::{LodAllocationMode, SemanticProxyMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderBatchMode {
    Off,
    Static,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssetProfile {
    pub id: String,
    pub max_triangles: u64,
    pub max_bytes: usize,
    pub crease_angle_degrees: f32,
    pub generate_edges: bool,
    pub lod_enabled: bool,
    pub lod_ratio: f64,
    pub lod_allocation: LodAllocationMode,
    pub lod_max_triangles: u64,
    pub lod_min_unit_triangles: u64,
    pub lod_curve_samples: u32,
    pub lod_max_memory_bytes: usize,
    pub coarse_chunk_max_bytes: usize,
    pub semantic_proxy: SemanticProxyMode,
    pub render_batch: RenderBatchMode,
}

impl AssetProfile {
    pub fn agent_console_v1() -> Self {
        Self {
            id: "agent-console-v1".into(),
            max_triangles: 60_000,
            max_bytes: 67_108_864,
            crease_angle_degrees: 30.0,
            generate_edges: true,
            lod_enabled: true,
            lod_ratio: 0.5,
            lod_allocation: LodAllocationMode::Adaptive,
            lod_max_triangles: 120_000,
            lod_min_unit_triangles: 8,
            lod_curve_samples: 8,
            lod_max_memory_bytes: 536_870_912,
            coarse_chunk_max_bytes: 4_194_304,
            semantic_proxy: SemanticProxyMode::Off,
            render_batch: RenderBatchMode::Static,
        }
    }

    pub fn direct_partition(max_triangles: u64, max_bytes: usize) -> Self {
        Self {
            id: "direct-partition-v1".into(),
            max_triangles,
            max_bytes,
            crease_angle_degrees: 30.0,
            generate_edges: true,
            lod_enabled: false,
            lod_ratio: 0.15,
            lod_allocation: LodAllocationMode::Proportional,
            lod_max_triangles: 250_000,
            lod_min_unit_triangles: 8,
            lod_curve_samples: 8,
            lod_max_memory_bytes: 67_108_864,
            coarse_chunk_max_bytes: max_bytes,
            semantic_proxy: SemanticProxyMode::Off,
            render_batch: RenderBatchMode::Off,
        }
    }

    pub fn validate(&self) -> Result<(), PipelineError> {
        if self.id.trim().is_empty() {
            return Err(invalid("profile id must not be empty"));
        }
        if self.max_triangles == 0 {
            return Err(invalid("max triangles must be greater than zero"));
        }
        if self.max_bytes == 0 {
            return Err(invalid("max bytes must be greater than zero"));
        }
        if !self.crease_angle_degrees.is_finite()
            || !(0.0..=180.0).contains(&self.crease_angle_degrees)
        {
            return Err(invalid("crease angle must be between 0 and 180 degrees"));
        }
        if !self.lod_ratio.is_finite() || !(0.0 < self.lod_ratio && self.lod_ratio < 1.0) {
            return Err(invalid("LOD ratio must be between 0 and 1"));
        }
        if self.lod_max_triangles == 0 {
            return Err(invalid("LOD max triangles must be greater than zero"));
        }
        if self.lod_min_unit_triangles > self.lod_max_triangles {
            return Err(invalid(
                "LOD min unit triangles must not exceed max triangles",
            ));
        }
        if !(4..=32).contains(&self.lod_curve_samples) {
            return Err(invalid("LOD curve samples must be between 4 and 32"));
        }
        if self.lod_max_memory_bytes == 0 {
            return Err(invalid("LOD max memory bytes must be greater than zero"));
        }
        if self.coarse_chunk_max_bytes == 0 {
            return Err(invalid("coarse chunk max bytes must be greater than zero"));
        }
        if self.semantic_proxy == SemanticProxyMode::Auto
            && (!self.lod_enabled || self.lod_allocation != LodAllocationMode::Adaptive)
        {
            return Err(invalid(
                "semantic proxy auto requires LOD and adaptive allocation",
            ));
        }
        Ok(())
    }
}

fn invalid(message: impl Into<String>) -> PipelineError {
    PipelineError::InvalidProfile(message.into())
}
