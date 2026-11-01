mod mtl;
mod parser;
mod resources;

pub use mtl::parse_mtl;
pub use parser::ObjImporter;
pub use resources::{ObjResources, read_obj_resources};
