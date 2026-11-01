#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticProxyMode {
    Off,
    Auto,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FitSamples {
    pub vertices: Vec<f32>,
    pub indices: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SemanticProxy {
    pub kind: String,
    pub positions: Vec<f32>,
    pub indices: Vec<u32>,
    pub triangle_count: usize,
    pub error_meters: f32,
}

pub fn create_semantic_proxy(
    samples: &FitSamples,
    mode: SemanticProxyMode,
) -> Result<Option<SemanticProxy>, crate::PipelineError> {
    if mode == SemanticProxyMode::Off {
        return Ok(None);
    }
    if samples.vertices.is_empty()
        || samples.vertices.len() % 3 != 0
        || samples.vertices.iter().any(|value| !value.is_finite())
        || samples.indices.len() < 3
        || samples.indices.len() % 3 != 0
    {
        return Err(crate::PipelineError::Tileset(
            "semantic proxy samples are invalid".into(),
        ));
    }
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for point in samples.vertices.chunks_exact(3) {
        for axis in 0..3 {
            min[axis] = min[axis].min(point[axis]);
            max[axis] = max[axis].max(point[axis]);
        }
    }
    let center = [
        (min[0] + max[0]) * 0.5,
        (min[1] + max[1]) * 0.5,
        (min[2] + max[2]) * 0.5,
    ];
    let half = [
        ((max[0] - min[0]) * 0.5).max(1e-6),
        ((max[1] - min[1]) * 0.5).max(1e-6),
        ((max[2] - min[2]) * 0.5).max(1e-6),
    ];
    let mut positions = Vec::with_capacity(24);
    for z in [-1.0, 1.0] {
        for y in [-1.0, 1.0] {
            for x in [-1.0, 1.0] {
                positions.extend_from_slice(&[
                    center[0] + x * half[0],
                    center[1] + y * half[1],
                    center[2] + z * half[2],
                ]);
            }
        }
    }
    Ok(Some(SemanticProxy {
        kind: "obb".into(),
        positions,
        indices: vec![
            0, 2, 1, 1, 2, 3, 4, 5, 6, 5, 7, 6, 0, 1, 4, 1, 5, 4, 2, 6, 3, 3, 6, 7, 0, 4, 2, 2, 4,
            6, 1, 3, 5, 3, 7, 5,
        ],
        triangle_count: 12,
        error_meters: 0.0,
    }))
}
