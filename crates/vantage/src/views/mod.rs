//! Dashboard views rendered inside the viewport and inspector.
//!
//! Every module here adds `impl VantageApp` methods; the root view composes
//! them without reaching across view modules directly.

pub mod add_repo_modal;
pub mod inspector;
pub mod pending_changes;
pub mod repo_table;
pub mod sidebar;
pub mod topbar;
pub mod viewport;
