//! Project persistence for ICE3 Studio.

mod lock;
mod path;
mod store;

pub use path::WorkspacePath;
pub use store::{HistoryEntry, ProjectError, ProjectStore};
