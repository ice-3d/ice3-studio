use meshopt::{SimplifyOptions as MeshoptOptions, VertexDataAdapter, simplify, simplify_scale};

#[derive(Debug, Clone, Copy, Default)]
pub struct Simplifier;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimplifyOptions {
    pub target_triangles: usize,
    pub minimum_triangles: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LodGeometry {
    pub positions: Vec<f32>,
    pub indices: Vec<u32>,
    pub error_meters: f32,
    pub source_triangles: usize,
    pub triangle_count: usize,
    pub preserved_source: bool,
    pub warning: Option<String>,
}

pub fn simplify_geometry(
    positions: &[f32],
    indices: &[u32],
    options: &SimplifyOptions,
    _simplifier: &Simplifier,
) -> LodGeometry {
    let source_triangles = indices.len() / 3;
    if !valid_geometry(positions, indices) || options.target_triangles == 0 {
        return preserve(
            positions,
            indices,
            source_triangles,
            "geometry buffers are invalid",
        );
    }
    let minimum = options.minimum_triangles.max(1).min(source_triangles);
    if source_triangles <= minimum {
        return preserve(
            positions,
            indices,
            source_triangles,
            "source is already small",
        );
    }
    let target = options.target_triangles.clamp(minimum, source_triangles);
    let welded = weld(positions, indices);
    let bytes = bytemuck::cast_slice(&welded.positions);
    let Ok(vertices) = VertexDataAdapter::new(bytes, 12, 0) else {
        return preserve(
            positions,
            indices,
            source_triangles,
            "geometry layout is unsupported",
        );
    };
    let scale = simplify_scale(&vertices);
    let mut relative_error = 0.0_f32;
    let simplified = simplify(
        &welded.indices,
        &vertices,
        target * 3,
        1.0,
        MeshoptOptions::LockBorder | MeshoptOptions::ErrorClamped,
        Some(&mut relative_error),
    );
    if simplified.is_empty()
        || !simplified.len().is_multiple_of(3)
        || simplified
            .iter()
            .any(|index| *index as usize >= welded.positions.len() / 3)
        || !relative_error.is_finite()
    {
        return preserve(
            positions,
            indices,
            source_triangles,
            "simplifier rejected the geometry",
        );
    }
    let compacted = compact(&welded.positions, &simplified);
    LodGeometry {
        positions: compacted.0,
        indices: compacted.1,
        error_meters: (relative_error * scale).max(0.0),
        source_triangles,
        triangle_count: simplified.len() / 3,
        preserved_source: false,
        warning: None,
    }
}

fn valid_geometry(positions: &[f32], indices: &[u32]) -> bool {
    if positions.is_empty()
        || !positions.len().is_multiple_of(3)
        || positions.iter().any(|value| !value.is_finite())
        || indices.is_empty()
        || !indices.len().is_multiple_of(3)
    {
        return false;
    }
    indices
        .iter()
        .all(|index| (*index as usize) < positions.len() / 3)
}

fn weld(positions: &[f32], indices: &[u32]) -> LodGeometryBuffers {
    let mut keys = std::collections::BTreeMap::<[i32; 3], u32>::new();
    let mut welded_positions = Vec::new();
    let mut welded_indices = Vec::with_capacity(indices.len());
    for source in indices {
        let offset = *source as usize * 3;
        let point = [
            positions[offset],
            positions[offset + 1],
            positions[offset + 2],
        ];
        let key = point.map(quantize);
        let target = if let Some(index) = keys.get(&key) {
            *index
        } else {
            let index = (welded_positions.len() / 3) as u32;
            welded_positions.extend_from_slice(&point);
            keys.insert(key, index);
            index
        };
        welded_indices.push(target);
    }
    LodGeometryBuffers {
        positions: welded_positions,
        indices: welded_indices,
    }
}

fn compact(positions: &[f32], indices: &[u32]) -> (Vec<f32>, Vec<u32>) {
    let mut remap = std::collections::BTreeMap::<u32, u32>::new();
    let mut output_positions = Vec::new();
    let mut output_indices = Vec::with_capacity(indices.len());
    for source in indices {
        let target = if let Some(index) = remap.get(source) {
            *index
        } else {
            let target = (output_positions.len() / 3) as u32;
            let offset = *source as usize * 3;
            output_positions.extend_from_slice(&positions[offset..offset + 3]);
            remap.insert(*source, target);
            target
        };
        output_indices.push(target);
    }
    (output_positions, output_indices)
}

fn preserve(
    positions: &[f32],
    indices: &[u32],
    source_triangles: usize,
    warning: &str,
) -> LodGeometry {
    LodGeometry {
        positions: positions.to_vec(),
        indices: indices.to_vec(),
        error_meters: 0.0,
        source_triangles,
        triangle_count: source_triangles,
        preserved_source: true,
        warning: Some(warning.into()),
    }
}

fn quantize(value: f32) -> i32 {
    (value * 100_000.0).round() as i32
}

struct LodGeometryBuffers {
    positions: Vec<f32>,
    indices: Vec<u32>,
}
