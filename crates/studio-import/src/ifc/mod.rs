mod adapter;
mod geometry;
mod metadata;

pub use adapter::IfcImporter;
pub use metadata::{
    IfcMetadata, IfcNodeMetadata, IfcProductMetadata, IfcPropertySetMetadata,
    IfcQuantitySetMetadata, IfcSystemMetadata, IfcSystemReference, extract_ifc_metadata,
};
