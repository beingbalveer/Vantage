//! Checkbox and switch controls.

use gpui::{div, prelude::*, px, Div, Styled};

use crate::theme::Theme;

use super::icon::icon;
use super::tokens::IconSize;

pub fn checkbox(theme: &Theme, checked: bool) -> Div {
    let base = div()
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .w(px(16.0))
        .h(px(16.0))
        .rounded(px(4.0))
        .border_1()
        .cursor_pointer();

    if checked {
        base.bg(theme.accent).border_color(theme.accent).child(icon(
            "check",
            IconSize::Xxs,
            theme.primary_text,
        ))
    } else {
        base.border_color(theme.border_strong).bg(theme.panel)
    }
}

/// Compact on/off switch. Callers add the id and click handler.
pub fn switch(theme: &Theme, on: bool) -> Div {
    div()
        .flex()
        .items_center()
        .w(px(36.0))
        .h(px(20.0))
        .rounded(px(10.0))
        .px(px(2.0))
        .bg(if on {
            theme.accent
        } else {
            theme.border_strong
        })
        .cursor_pointer()
        .when(on, |d| d.justify_end())
        .when(!on, |d| d.justify_start())
        .child(
            div()
                .w(px(16.0))
                .h(px(16.0))
                .rounded(px(8.0))
                .bg(theme.primary_text),
        )
}
