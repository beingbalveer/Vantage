//! Labeled button variants.

use gpui::{div, prelude::*, px, Div, Styled};

use crate::theme::Theme;

use super::icon::icon;
use super::text::Txt;
use super::tokens::{IconSize, RADIUS_MD, ROW_CONTROL_HEIGHT};

/// Solid call-to-action. The label takes anything that is an element, so a
/// caller can put a count in it (`Commit 3 files`).
pub fn primary_button(
    theme: &Theme,
    label: impl IntoElement,
    icon_name: Option<&'static str>,
) -> Div {
    let mut button = div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .px(px(14.0))
        .py(px(6.0))
        .rounded(px(RADIUS_MD))
        .bg(theme.primary_bg)
        .cursor_pointer()
        .whitespace_nowrap()
        .flex_shrink_0()
        .xs()
        .medium()
        .fg(theme.primary_text)
        .hover(move |s| s.bg(theme.primary_bg_hover));

    if let Some(name) = icon_name {
        button = button.child(icon(name, IconSize::Sm, theme.primary_text));
    }
    button.child(label)
}

pub fn secondary_button(
    theme: &Theme,
    label: &'static str,
    icon_name: Option<&'static str>,
) -> Div {
    let leading =
        icon_name.map(|name| icon(name, IconSize::Sm, theme.secondary_text).into_any_element());
    secondary_button_with(theme, label, leading)
}

/// [`secondary_button`] with a caller-supplied leading element.
///
/// Used where the glyph has to change with state rather than be a fixed icon —
/// `Fetch Origin` swaps its static icon for a spinner while a fetch runs.
pub fn secondary_button_with(
    theme: &Theme,
    label: &'static str,
    leading: Option<gpui::AnyElement>,
) -> Div {
    let mut button = div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .px(px(12.0))
        .py(px(6.0))
        .rounded(px(RADIUS_MD))
        .border_1()
        .border_color(theme.secondary_border)
        .bg(theme.secondary_bg)
        .cursor_pointer()
        .whitespace_nowrap()
        .flex_shrink_0()
        .xs()
        .medium()
        .fg(theme.secondary_text)
        .hover(move |s| s.bg(theme.secondary_bg_hover));

    if let Some(leading) = leading {
        button = button.child(leading);
    }
    button.child(label)
}

/// Accent-filled button sized to [`ROW_CONTROL_HEIGHT`].
///
/// The commit action uses the same blue the rest of the UI accents with (the
/// active tab, links, focus rings) instead of the near-black `primary_bg`
/// reserved for modal confirmations, so it sits quietly in a row of controls.
pub fn accent_button(theme: &Theme, label: impl IntoElement) -> Div {
    accent_button_with(theme, label, None)
}

/// [`accent_button`] with a caller-supplied leading element, so the commit
/// button can spin while git commits.
pub fn accent_button_with(
    theme: &Theme,
    label: impl IntoElement,
    leading: Option<gpui::AnyElement>,
) -> Div {
    // Blue-600 with white text in both appearances: light mode takes the accent
    // colour, dark mode the already-blue primary pair.
    let (background, hover) = if theme.is_dark {
        (theme.primary_bg, theme.primary_bg_hover)
    } else {
        (theme.accent, theme.accent_text)
    };

    let mut button = div()
        .flex()
        .items_center()
        .justify_center()
        .gap(px(6.0))
        .h(px(ROW_CONTROL_HEIGHT))
        .px(px(14.0))
        .rounded(px(RADIUS_MD))
        .bg(background)
        .cursor_pointer()
        .whitespace_nowrap()
        .flex_shrink_0()
        .xs()
        .medium()
        .fg(theme.primary_text)
        .hover(move |s| s.bg(hover));

    if let Some(leading) = leading {
        button = button.child(leading);
    }
    button.child(label)
}
