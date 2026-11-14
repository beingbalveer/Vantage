//! Icon and icon-button primitives.

use gpui::{div, prelude::*, px, Div, IntoElement, Rgba, Styled};

use crate::icons::icon as raw_icon;
use crate::theme::Theme;

use super::tokens::{IconSize, CONTROL_MD, RADIUS_MD};

/// Render an icon from the central icon registry at one of the design sizes.
pub fn icon(name: &'static str, size: IconSize, color: Rgba) -> impl IntoElement {
    raw_icon(name, size.px(), color)
}

/// A square, borderless toolbar button. Callers add the id and click handler.
pub fn icon_button(theme: &Theme, name: &'static str) -> Div {
    let hover = theme.nav_hover_bg;

    div()
        .flex()
        .items_center()
        .justify_center()
        .w(px(CONTROL_MD))
        .h(px(CONTROL_MD))
        .rounded(px(RADIUS_MD))
        .flex_shrink_0()
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .child(icon(name, IconSize::Md, theme.label_muted))
}
