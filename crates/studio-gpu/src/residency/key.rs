use sha2::{Digest, Sha256};
use studio_core::document::NodeContent;
use studio_core::gpu_contract::BackendError;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResidencyKey {
    pub topology: String,
    pub attribute: String,
    pub result: String,
}

impl ResidencyKey {
    pub fn for_content(
        content: &NodeContent,
        dependency_digest: &str,
        kernel_version: &str,
    ) -> Result<Self, BackendError> {
        let topology = match content {
            NodeContent::Group => hash_parts(&["geometry-topology-v1".as_bytes(), b"group"]),
            NodeContent::Primitive(primitive) => match primitive {
                studio_core::geometry::Primitive::Box { .. } => {
                    hash_parts(&["geometry-topology-v1".as_bytes(), b"box"])
                }
                studio_core::geometry::Primitive::Plane { .. } => {
                    hash_parts(&["geometry-topology-v1".as_bytes(), b"plane"])
                }
                studio_core::geometry::Primitive::Cylinder { segments, .. } => hash_parts(&[
                    "geometry-topology-v1".as_bytes(),
                    b"cylinder",
                    &segments.to_le_bytes(),
                ]),
                studio_core::geometry::Primitive::Sphere {
                    segments, rings, ..
                } => hash_parts(&[
                    "geometry-topology-v1".as_bytes(),
                    b"sphere",
                    &segments.to_le_bytes(),
                    &rings.to_le_bytes(),
                ]),
            },
            NodeContent::CustomMesh(mesh) => {
                let mut hash = Sha256::new();
                hash.update(b"geometry-topology-v1");
                hash.update(b"custom-mesh");
                hash.update((mesh.positions.len() as u64).to_le_bytes());
                for index in &mesh.indices {
                    hash.update(index.to_le_bytes());
                }
                format!("{:x}", hash.finalize())
            }
            NodeContent::Sweep {
                points,
                radial_segments,
                ..
            } => hash_parts(&[
                "geometry-topology-v1".as_bytes(),
                b"sweep",
                &(points.len() as u64).to_le_bytes(),
                &radial_segments.to_le_bytes(),
            ]),
            NodeContent::ComponentInstance(_)
            | NodeContent::InstanceSet(_)
            | NodeContent::Connection(_) => canonical_hash("geometry-topology-v1", content)?,
        };

        let attribute = canonical_hash(
            "geometry-attribute-v1",
            &(dependency_digest, content, kernel_version),
        )?;
        let result = hash_parts(&[
            "geometry-result-v1".as_bytes(),
            topology.as_bytes(),
            attribute.as_bytes(),
            kernel_version.as_bytes(),
            b"parity-1e-5-v1",
        ]);
        Ok(Self {
            topology,
            attribute,
            result,
        })
    }
}

fn canonical_hash<T: serde::Serialize>(domain: &str, value: &T) -> Result<String, BackendError> {
    let bytes = serde_json::to_vec(value).map_err(|error| BackendError::Kernel {
        message: format!("residency key serialization failed: {error}"),
    })?;
    let mut hash = Sha256::new();
    hash.update(domain.as_bytes());
    hash.update(bytes);
    Ok(format!("{:x}", hash.finalize()))
}

fn hash_parts(parts: &[&[u8]]) -> String {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update((part.len() as u64).to_le_bytes());
        hash.update(part);
    }
    format!("{:x}", hash.finalize())
}
