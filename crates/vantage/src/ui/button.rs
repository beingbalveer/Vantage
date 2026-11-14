//! Labeled button variants.

use gpui::{div, prelude::*, px, Div, Styled};

use crate::theme::Theme;

use super::icon::icon;
use super::text::Txt;
use super::tokens::{IconSize, RADIUS_MD};

pub fn primary_button(theme: &Theme, label: &'static str, icon_name: Option<&'static str>) -> Div {
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

    if let Some(name) = icon_name {
        button = button.child(icon(name, IconSize::Sm, theme.secondary_text));
    }
    button.child(label)
}
