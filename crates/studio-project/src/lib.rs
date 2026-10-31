//! Project persistence for ICE3 Studio.

pub mod archive;
pub mod changeset;
mod changeset_store;
pub mod geometry_cache;
pub mod layered_geometry_cache;
mod lock;
mod path;
pub mod reference_asset_store;
mod store;

pub use changeset_store::{ChangeSetStore, ChangeSetStoreDiagnostic};
pub use path::WorkspacePath;
pub use store::{HistoryEntry, MAX_UNDO_DEPTH, ProjectError, ProjectStore};
