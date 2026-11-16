//! Checkbox and switch controls.

use gpui::{div, prelude::*, px, Div, Styled};

use crate::theme::Theme;

use super::icon::icon;
use super::tokens::{hit_target, IconSize, HIT_TARGET_MIN};

/// Painted size of the checkbox square.
pub const CHECKBOX_SIZE: f32 = 16.0;

/// A checkbox with a 16 px glyph inside a 32 px hit target.
///
/// The caller's id and click handler land on the outer box, so the whole
/// target is clickable while the row keeps its compact visual rhythm.
pub fn checkbox(theme: &Theme, checked: bool) -> Div {
    let target = hit_target(CHECKBOX_SIZE);
    let hover_bg = theme.row_hover_bg;
    let base = div()
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .w(px(CHECKBOX_SIZE))
        .h(px(CHECKBOX_SIZE))
        .rounded(px(4.0))
        .border_1();

    let glyph = if checked {
        base.bg(theme.accent).border_color(theme.accent).child(icon(
            "check",
            IconSize::Xxs,
            theme.primary_text,
        ))
    } else {
        base.border_color(theme.border_strong).bg(theme.panel)
    };

    div()
        .flex()
        .items_center()
        .justify_center()
        .w(px(target))
        .h(px(target))
        .flex_shrink_0()
        .rounded(px(6.0))
        .cursor_pointer()
        .hover(move |s| s.bg(hover_bg))
        .child(glyph)
}

/// Compact on/off switch. Callers add the id and click handler.
///
/// The 36 × 20 track sits inside a 44 × 32 target so it is easy to hit without
/// drawing a larger switch.
pub fn switch(theme: &Theme, on: bool) -> Div {
    let track = div()
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
        .when(on, |d| d.justify_end())
        .when(!on, |d| d.justify_start())
        .child(
            div()
                .w(px(16.0))
                .h(px(16.0))
                .rounded(px(8.0))
                .bg(theme.primary_text),
        );

    div()
        .flex()
        .items_center()
        .justify_center()
        .w(px(44.0))
        .h(px(HIT_TARGET_MIN))
        .flex_shrink_0()
        .cursor_pointer()
        .child(track)
}
