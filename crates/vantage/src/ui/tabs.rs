//! Tabs and segmented choice chips.

use gpui::{div, prelude::*, px, rgba, ElementId, IntoElement, SharedString, Stateful, Styled};

use crate::theme::Theme;

use super::text::Txt;
use super::tokens::RADIUS_MD;

/// Underline tab used inside detail panels.
pub fn tab(
    theme: &Theme,
    label: impl IntoElement,
    active: bool,
    id: impl Into<ElementId>,
) -> Stateful<gpui::Div> {
    let active_color = theme.accent;
    let text = if active {
        theme.text_app
    } else {
        theme.label_muted
    };
    let hover_text = theme.text_soft;

    div()
        .id(id)
        .pb(px(10.0))
        .border_b_2()
        .border_color(if active {
            active_color
        } else {
            rgba(0x00000000)
        })
        .cursor_pointer()
        .whitespace_nowrap()
        .flex_shrink_0()
        .xs()
        .medium()
        .fg(text)
        .when(!active, move |d| d.hover(move |s| s.fg(hover_text)))
        .child(label)
}

/// Segmented option chip used in Settings.
pub fn choice_button(
    theme: &Theme,
    label: &'static str,
    selected: bool,
    id: String,
) -> Stateful<gpui::Div> {
    let border = if selected {
        theme.accent
    } else {
        theme.secondary_border
    };
    let bg = if selected {
        theme.accent_soft
    } else {
        theme.field_bg
    };
    let fg = if selected {
        theme.accent_text
    } else {
        theme.secondary_text
    };
    let hover = theme.secondary_bg_hover;

    div()
        .id(SharedString::from(id))
        .px(px(10.0))
        .py(px(6.0))
        .rounded(px(RADIUS_MD))
        .border_1()
        .border_color(border)
        .bg(bg)
        .fg(fg)
        .cursor_pointer()
        .xs()
        .medium()
        .when(!selected, move |d| d.hover(move |s| s.bg(hover)))
        .child(label)
}
