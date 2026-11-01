use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub struct WorkspaceGeometry {
    pub positions: Vec<f32>,
    pub indices: Vec<u32>,
    pub colors: Option<Vec<f32>>,
}

#[derive(Debug)]
pub struct GeometryWorkspace {
    max_memory_bytes: usize,
    memory_bytes: usize,
    spilled_bytes: usize,
    root: PathBuf,
    records: BTreeMap<String, WorkspaceRecord>,
    disposed: bool,
}

#[derive(Debug)]
enum WorkspaceRecord {
    Memory(WorkspaceGeometry),
    Disk(PathBuf),
}

impl GeometryWorkspace {
    pub fn new(max_memory_bytes: usize) -> Self {
        let root = std::env::temp_dir().join(format!(
            "ice3-lod-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        Self {
            max_memory_bytes,
            memory_bytes: 0,
            spilled_bytes: 0,
            root,
            records: BTreeMap::new(),
            disposed: false,
        }
    }

    pub fn write(
        &mut self,
        key: impl Into<String>,
        geometry: WorkspaceGeometry,
    ) -> Result<(), crate::PipelineError> {
        let key = key.into();
        if self.disposed || key.is_empty() {
            return Err(crate::PipelineError::Tileset(
                "geometry workspace is unavailable".into(),
            ));
        }
        let bytes = geometry_bytes(&geometry);
        if self.memory_bytes + bytes.len() <= self.max_memory_bytes {
            self.memory_bytes += bytes.len();
            self.records.insert(key, WorkspaceRecord::Memory(geometry));
            return Ok(());
        }
        std::fs::create_dir_all(&self.root)?;
        let path = self.root.join(format!("{}.bin", self.records.len()));
        std::fs::write(&path, bytes)?;
        self.spilled_bytes += geometry_size(&geometry);
        self.records.insert(key, WorkspaceRecord::Disk(path));
        Ok(())
    }

    pub fn read(&self, key: &str) -> Result<WorkspaceGeometry, crate::PipelineError> {
        if self.disposed {
            return Err(crate::PipelineError::Tileset(
                "geometry workspace is disposed".into(),
            ));
        }
        match self.records.get(key) {
            Some(WorkspaceRecord::Memory(geometry)) => Ok(geometry.clone()),
            Some(WorkspaceRecord::Disk(path)) => {
                let bytes = std::fs::read(path)?;
                decode_geometry(&bytes)
            }
            None => Err(crate::PipelineError::Tileset(format!(
                "geometry workspace key is missing: {key}"
            ))),
        }
    }

    pub const fn spilled_bytes(&self) -> usize {
        self.spilled_bytes
    }

    pub fn dispose(&mut self) -> Result<(), crate::PipelineError> {
        self.disposed = true;
        self.records.clear();
        self.memory_bytes = 0;
        if self.root.exists() {
            std::fs::remove_dir_all(&self.root)?;
        }
        Ok(())
    }
}

impl Drop for GeometryWorkspace {
    fn drop(&mut self) {
        if !self.disposed {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}

fn geometry_bytes(geometry: &WorkspaceGeometry) -> Vec<u8> {
    let color_bytes = geometry
        .colors
        .as_ref()
        .map_or(0, |colors| colors.len() * 4);
    let mut output = Vec::with_capacity(12 + geometry_size(geometry));
    output.extend_from_slice(&(geometry.positions.len() * 4).to_le_bytes());
    output.extend_from_slice(&(geometry.indices.len() * 4).to_le_bytes());
    output.extend_from_slice(&color_bytes.to_le_bytes());
    output.extend_from_slice(bytemuck::cast_slice(&geometry.positions));
    output.extend_from_slice(bytemuck::cast_slice(&geometry.indices));
    if let Some(colors) = &geometry.colors {
        output.extend_from_slice(bytemuck::cast_slice(colors));
    }
    output
}

fn decode_geometry(bytes: &[u8]) -> Result<WorkspaceGeometry, crate::PipelineError> {
    if bytes.len() < 12 {
        return Err(crate::PipelineError::Tileset(
            "spilled geometry is truncated".into(),
        ));
    }
    let position_bytes = u32::from_le_bytes(bytes[0..4].try_into().expect("four bytes")) as usize;
    let index_bytes = u32::from_le_bytes(bytes[4..8].try_into().expect("four bytes")) as usize;
    let color_bytes = u32::from_le_bytes(bytes[8..12].try_into().expect("four bytes")) as usize;
    if 12 + position_bytes + index_bytes + color_bytes != bytes.len()
        || position_bytes % 4 != 0
        || index_bytes % 4 != 0
        || color_bytes % 4 != 0
    {
        return Err(crate::PipelineError::Tileset(
            "spilled geometry has an invalid layout".into(),
        ));
    }
    let positions = cast_vec(&bytes[12..12 + position_bytes])?;
    let indices = cast_vec(&bytes[12 + position_bytes..12 + position_bytes + index_bytes])?;
    let colors = if color_bytes == 0 {
        None
    } else {
        Some(cast_vec(&bytes[12 + position_bytes + index_bytes..])?)
    };
    Ok(WorkspaceGeometry {
        positions,
        indices,
        colors,
    })
}

fn cast_vec<T: bytemuck::Pod>(bytes: &[u8]) -> Result<Vec<T>, crate::PipelineError> {
    bytemuck::try_cast_slice(bytes)
        .map(<[T]>::to_vec)
        .map_err(|_| crate::PipelineError::Tileset("spilled geometry cast failed".into()))
}

fn geometry_size(geometry: &WorkspaceGeometry) -> usize {
    geometry.positions.len() * 4
        + geometry.indices.len() * 4
        + geometry
            .colors
            .as_ref()
            .map_or(0, |colors| colors.len() * 4)
}
