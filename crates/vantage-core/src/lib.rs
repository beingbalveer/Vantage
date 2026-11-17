//! Vantage's git engine, shared by the native GPUI desktop app and any future
//! surface.
//!
//! Nothing in here depends on a UI toolkit: git is driven through the real
//! `git` binary (`git::runner`), with progress reported through an
//! `events::Sink`.

pub mod detect;
pub mod error;
pub mod events;
pub mod git;
pub mod persist;
pub mod repos;
pub mod state;

pub use error::AppError;
pub use events::Sink;
pub use git::runner::{CancelToken, GitRunner};
pub use state::{RepoEntry, RepoStatus};
