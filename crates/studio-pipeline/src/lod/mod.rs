mod semantic_proxy;
mod simplify;
mod workspace;

pub use semantic_proxy::{FitSamples, SemanticProxy, SemanticProxyMode, create_semantic_proxy};
pub use simplify::{LodGeometry, Simplifier, SimplifyOptions, simplify_geometry};
pub use workspace::{GeometryWorkspace, WorkspaceGeometry};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LodAllocationMode {
    Proportional,
    Adaptive,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LodTarget {
    pub target_triangles: u64,
    pub minimum_triangles: u64,
}

pub fn allocate_lod_targets(
    source_triangles: &[u64],
    ratio: f64,
    maximum_triangles: u64,
    minimum_per_unit: u64,
) -> Vec<LodTarget> {
    let mut targets = source_triangles
        .iter()
        .map(|source| {
            let minimum = minimum_per_unit.min(*source);
            LodTarget {
                target_triangles: ((*source as f64 * ratio).floor() as u64).max(minimum),
                minimum_triangles: minimum,
            }
        })
        .collect::<Vec<_>>();
    let mut total = targets
        .iter()
        .map(|target| target.target_triangles)
        .sum::<u64>();
    while total > maximum_triangles {
        let Some((index, _)) = targets
            .iter()
            .enumerate()
            .filter(|(_, target)| target.target_triangles > target.minimum_triangles)
            .max_by_key(|(index, target)| {
                (
                    target.target_triangles - target.minimum_triangles,
                    usize::MAX - *index,
                )
            })
        else {
            break;
        };
        targets[index].target_triangles -= 1;
        total -= 1;
    }
    targets
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoarseChunkUnit {
    pub unit_id: String,
    pub bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoarseChunk {
    pub unit_ids: Vec<String>,
    pub bytes: usize,
    pub oversized: bool,
}

pub fn create_coarse_chunks(
    units: &[CoarseChunkUnit],
    maximum_bytes: usize,
) -> Result<Vec<CoarseChunk>, crate::PipelineError> {
    if maximum_bytes == 0 || units.iter().any(|unit| unit.unit_id.is_empty()) {
        return Err(crate::PipelineError::Tileset(
            "coarse chunk options are invalid".into(),
        ));
    }
    let mut units = units.to_vec();
    units.sort_by(|left, right| left.unit_id.cmp(&right.unit_id));
    let mut chunks = Vec::new();
    let mut current = Vec::new();
    let mut bytes = 0usize;
    for unit in units {
        if unit.bytes > maximum_bytes {
            if !current.is_empty() {
                chunks.push(CoarseChunk {
                    unit_ids: std::mem::take(&mut current),
                    bytes,
                    oversized: false,
                });
                bytes = 0;
            }
            chunks.push(CoarseChunk {
                unit_ids: vec![unit.unit_id],
                bytes: unit.bytes,
                oversized: true,
            });
            continue;
        }
        if !current.is_empty() && bytes + unit.bytes > maximum_bytes {
            chunks.push(CoarseChunk {
                unit_ids: std::mem::take(&mut current),
                bytes,
                oversized: false,
            });
            bytes = 0;
        }
        bytes += unit.bytes;
        current.push(unit.unit_id);
    }
    if !current.is_empty() {
        chunks.push(CoarseChunk {
            unit_ids: current,
            bytes,
            oversized: false,
        });
    }
    Ok(chunks)
}
