//! View-specific shared pieces.
//!
//! Generic design-system primitives live in [`crate::ui`]; this module keeps
//! the repository-status helpers that only the dashboard views use.

use gpui::{div, prelude::*, px, Div, Styled};

use crate::model::RepoSyncState;
use crate::theme::Theme;
use crate::ui::{icon, IconSize, Txt};

// ---- status cell ---------------------------------------------------------

/// `StatusText.tsx` — icon + text, no badge chrome.
pub fn status_text(theme: &Theme, state: RepoSyncState, uncommitted: usize) -> Div {
    let (name, color, text) = match state {
        RepoSyncState::Conflict => ("alert-triangle", theme.status_red, "Conflict".to_string()),
        RepoSyncState::Behind => ("arrow-down", theme.status_red, "Behind origin".to_string()),
        RepoSyncState::Modified => (
            "file-diff",
            theme.status_amber,
            format!("{uncommitted} uncommitted"),
        ),
        RepoSyncState::Ahead => (
            "arrow-up",
            theme.status_blue,
            "Unpushed commits".to_string(),
        ),
        RepoSyncState::Clean => (
            "circle-check-big",
            theme.status_green,
            "Clean & synced".to_string(),
        ),
    };

    div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .flex_shrink_0()
        .whitespace_nowrap()
        .xs()
        .medium()
        .fg(color)
        .child(icon(name, IconSize::Sm, color))
        .child(div().child(text))
}
