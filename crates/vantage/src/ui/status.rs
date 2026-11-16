//! The one set of Git status indicators the app uses everywhere.
//!
//! A repository is rarely in a single state — it can have uncommitted work and
//! unpushed commits while being behind its remote — so the indicators are a
//! list, not an enum. Every indicator carries an icon and a word, which keeps
//! the meaning readable without relying on colour.

use gpui::{div, prelude::*, px, Div, Rgba, Styled};

use crate::theme::Theme;
use crate::ui::icon::icon;
use crate::ui::text::Txt;
use crate::ui::tokens::IconSize;

/// One indicator a repository currently deserves.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StatusKind {
    Conflict,
    Changes(usize),
    Ahead(i32),
    Behind(i32),
    Clean,
}

impl StatusKind {
    pub fn label(self) -> String {
        match self {
            Self::Conflict => "Conflict".to_string(),
            Self::Changes(1) => "1 change".to_string(),
            Self::Changes(count) => format!("{count} changes"),
            Self::Ahead(1) => "1 ahead".to_string(),
            Self::Ahead(count) => format!("{count} ahead"),
            Self::Behind(1) => "1 behind".to_string(),
            Self::Behind(count) => format!("{count} behind"),
            Self::Clean => "Clean".to_string(),
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::Conflict => "alert-triangle",
            Self::Changes(_) => "file-diff",
            Self::Ahead(_) => "arrow-up",
            Self::Behind(_) => "arrow-down",
            Self::Clean => "circle-check-big",
        }
    }

    pub fn color(self, theme: &Theme) -> Rgba {
        match self {
            Self::Conflict | Self::Behind(_) => theme.status_red,
            Self::Changes(_) => theme.status_amber,
            Self::Ahead(_) => theme.status_blue,
            Self::Clean => theme.status_green,
        }
    }
}

/// Every state the repository is in, in a fixed order so the same combination
/// always reads the same way.
pub fn status_kinds(
    uncommitted: usize,
    ahead: i32,
    behind: i32,
    conflict: bool,
) -> Vec<StatusKind> {
    let mut kinds = Vec::new();
    if conflict {
        kinds.push(StatusKind::Conflict);
    }
    if uncommitted > 0 {
        kinds.push(StatusKind::Changes(uncommitted));
    }
    if ahead > 0 {
        kinds.push(StatusKind::Ahead(ahead));
    }
    if behind > 0 {
        kinds.push(StatusKind::Behind(behind));
    }
    if kinds.is_empty() {
        kinds.push(StatusKind::Clean);
    }
    kinds
}

/// The indicators as a row of icon + word pairs.
pub fn status_badges(
    theme: &Theme,
    uncommitted: usize,
    ahead: i32,
    behind: i32,
    conflict: bool,
) -> Div {
    let mut row = div().flex().items_center().gap(px(8.0)).flex_shrink_0();
    for kind in status_kinds(uncommitted, ahead, behind, conflict) {
        row = row.child(badge(theme, kind));
    }
    row
}

fn badge(theme: &Theme, kind: StatusKind) -> Div {
    let color = kind.color(theme);
    div()
        .flex()
        .items_center()
        .gap(px(4.0))
        .flex_shrink_0()
        .whitespace_nowrap()
        .child(icon(kind.icon(), IconSize::Xs, color))
        .child(div().xs().medium().fg(color).child(kind.label()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_combine_instead_of_replacing_each_other() {
        assert_eq!(
            status_kinds(3, 1, 2, false),
            vec![
                StatusKind::Changes(3),
                StatusKind::Ahead(1),
                StatusKind::Behind(2)
            ],
            "uncommitted, unpushed and behind can all be true at once"
        );
        assert_eq!(
            status_kinds(0, 0, 0, true),
            vec![StatusKind::Conflict],
            "a conflicted working tree is not also called clean"
        );
    }

    #[test]
    fn a_quiet_repository_is_clean() {
        assert_eq!(status_kinds(0, 0, 0, false), vec![StatusKind::Clean]);
    }

    #[test]
    fn labels_read_naturally_for_one_and_many() {
        assert_eq!(StatusKind::Changes(1).label(), "1 change");
        assert_eq!(StatusKind::Changes(4).label(), "4 changes");
        assert_eq!(StatusKind::Ahead(1).label(), "1 ahead");
        assert_eq!(StatusKind::Behind(2).label(), "2 behind");
        assert_eq!(StatusKind::Clean.label(), "Clean");
    }
}
