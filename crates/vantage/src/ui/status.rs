//! The one set of Git status indicators the app uses everywhere.
//!
//! A repository is rarely in a single state — it can have uncommitted work and
//! unpushed commits while being behind its remote — so the indicators are a
//! list, not an enum. Every indicator carries an icon and a word, which keeps
//! the meaning readable without relying on colour.
//!
//! In the narrow repository list the words do not fit beside a useful amount of
//! the repository name. [`BadgeDensity::Compact`] keeps the icon and a count
//! (or the one-word conflict/clean state), and moves the full sentence into the
//! hover tooltip.

use gpui::{div, prelude::*, px, Div, Rgba, SharedString, Styled};

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

    /// The short form used in a narrow row. Counts keep only their number;
    /// the states that have no number keep their one word so they stay
    /// readable without hovering.
    pub fn compact_label(self) -> Option<String> {
        match self {
            Self::Conflict => Some("Conflict".to_string()),
            Self::Changes(count) => Some(count.to_string()),
            Self::Ahead(count) => Some(count.to_string()),
            Self::Behind(count) => Some(count.to_string()),
            Self::Clean => Some("Clean".to_string()),
        }
    }

    /// Stable id fragment for tests and hover targets.
    pub fn key(self) -> &'static str {
        match self {
            Self::Conflict => "conflict",
            Self::Changes(_) => "changes",
            Self::Ahead(_) => "ahead",
            Self::Behind(_) => "behind",
            Self::Clean => "clean",
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

/// How much room a status cluster has for words.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BadgeDensity {
    /// Icon + label, as in the wide list.
    Full,
    /// Icon + count for multi-state rows; conflict and clean keep their word.
    Compact,
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

/// The indicators as a wrapping row of icon + word pairs.
///
/// `id` identifies the cluster; individual badges get `{id}-{kind}` ids in
/// compact mode so their tooltips are addressable.
pub fn status_badges(
    theme: &Theme,
    uncommitted: usize,
    ahead: i32,
    behind: i32,
    conflict: bool,
    density: BadgeDensity,
    id: &str,
) -> gpui::Stateful<Div> {
    let theme = *theme;
    let gap = match density {
        BadgeDensity::Full => 8.0,
        BadgeDensity::Compact => 6.0,
    };
    // Wrapping is the last line of defence: even an absurd count cannot push
    // the badges through the right edge of the row.
    let mut row = div()
        .debug_selector(|| id.to_string())
        .id(SharedString::from(id.to_string()))
        .flex()
        .items_center()
        .flex_wrap()
        .justify_end()
        .gap(px(gap))
        .min_w(px(0.0))
        .flex_shrink();

    for kind in status_kinds(uncommitted, ahead, behind, conflict) {
        let badge = badge(&theme, kind, density);
        if density == BadgeDensity::Compact {
            let label = kind.label();
            row = row.child(
                badge
                    .id(SharedString::from(format!("{id}-{}", kind.key())))
                    .tooltip(move |_window, cx| {
                        cx.new(|_| crate::ui::TextTooltip::new(theme, label.clone()))
                            .into()
                    }),
            );
        } else {
            row = row.child(badge);
        }
    }
    row
}

fn badge(theme: &Theme, kind: StatusKind, density: BadgeDensity) -> Div {
    let color = kind.color(theme);
    let label = match density {
        BadgeDensity::Full => kind.label(),
        BadgeDensity::Compact => kind.compact_label().unwrap_or_default(),
    };
    let mut badge = div()
        .flex()
        .items_center()
        .gap(px(4.0))
        .flex_shrink_0()
        .whitespace_nowrap()
        .child(icon(kind.icon(), IconSize::Xxs, color));
    if !label.is_empty() {
        badge = badge.child(
            div()
                .text_size(crate::theme::scaled_px(
                    11.0,
                    crate::theme::TextArea::Interface,
                ))
                .medium()
                .fg(color)
                .child(label),
        );
    }
    badge
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

    #[test]
    fn compact_badges_keep_only_the_count_and_the_tooltip_carries_the_rest() {
        assert_eq!(StatusKind::Changes(4).compact_label().as_deref(), Some("4"));
        assert_eq!(StatusKind::Ahead(1).compact_label().as_deref(), Some("1"));
        assert_eq!(StatusKind::Behind(2).compact_label().as_deref(), Some("2"));
        assert_eq!(
            StatusKind::Conflict.compact_label().as_deref(),
            Some("Conflict")
        );
        assert_eq!(StatusKind::Clean.compact_label().as_deref(), Some("Clean"));
        assert_eq!(StatusKind::Changes(4).label(), "4 changes");
    }
}
