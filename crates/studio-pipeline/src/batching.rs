use std::collections::BTreeMap;

use studio_assets::SourceGlb;

use crate::PartitionUnit;
use crate::PipelineError;

#[derive(Debug, Clone, PartialEq)]
pub struct RenderBatch {
    pub id: String,
    pub unit_ids: Vec<String>,
    pub selection_ids: Vec<String>,
    pub triangle_count: u64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RenderBatching {
    pub batches: Vec<RenderBatch>,
}

pub fn build_static_batches(
    _source: &SourceGlb,
    units: &[PartitionUnit],
    maximum_triangles: u64,
) -> Result<RenderBatching, PipelineError> {
    if maximum_triangles == 0 {
        return Err(PipelineError::Tileset(
            "render batch triangle budget must be positive".into(),
        ));
    }
    let mut groups = BTreeMap::<u64, Vec<&PartitionUnit>>::new();
    let mut triangles = 0_u64;
    for unit in units {
        let bucket = triangles / maximum_triangles;
        groups.entry(bucket).or_default().push(unit);
        triangles = triangles.saturating_add(unit.triangle_count);
    }
    let batches = groups
        .into_iter()
        .enumerate()
        .map(|(index, (_, units))| RenderBatch {
            id: format!("batch-{index:03}"),
            unit_ids: units.iter().map(|unit| unit.id.clone()).collect(),
            selection_ids: units
                .iter()
                .flat_map(|unit| unit.node_ids.iter().cloned())
                .collect(),
            triangle_count: units.iter().map(|unit| unit.triangle_count).sum(),
        })
        .collect();
    Ok(RenderBatching { batches })
}
