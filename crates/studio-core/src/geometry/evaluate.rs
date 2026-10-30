use crate::document::NodeContent;

use super::primitives::{evaluate_primitive, validate_mesh};
use super::sweep::evaluate_sweep;
use super::{EvaluatedMesh, GeometryError};

pub fn evaluate_node_content(content: &NodeContent) -> Result<EvaluatedMesh, GeometryError> {
    match content {
        NodeContent::Group => Ok(EvaluatedMesh::empty()),
        NodeContent::Primitive(primitive) => evaluate_primitive(primitive),
        NodeContent::CustomMesh(mesh) => validate_mesh(mesh),
        NodeContent::Sweep {
            points,
            radius,
            radial_segments,
        } => evaluate_sweep(points, *radius, *radial_segments),
        NodeContent::ComponentInstance(_)
        | NodeContent::InstanceSet(_)
        | NodeContent::Connection(_) => Ok(EvaluatedMesh::empty()),
    }
}
