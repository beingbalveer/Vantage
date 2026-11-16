//! Icon and icon-button primitives.

use std::time::Duration;

use gpui::{
    div, prelude::*, px, Animation, AnimationExt, Div, ElementId, IntoElement, Rgba, SharedString,
    Stateful, Styled, Transformation,
};

use crate::icons::{icon as raw_icon, icon_svg};
use crate::theme::Theme;

use super::tokens::{IconSize, CONTROL_MD, RADIUS_MD};
use super::tooltip::label_tooltip;

/// One full turn of the spinner. Slow enough to read as "working", fast enough
/// to look alive on a short fetch.
const SPIN_CYCLE: Duration = Duration::from_millis(900);

/// Render an icon from the central icon registry at one of the design sizes.
pub fn icon(name: &'static str, size: IconSize, color: Rgba) -> impl IntoElement {
    raw_icon(name, size.px(), color)
}

/// A square, borderless toolbar button.
///
/// The label is shown as a tooltip: it is the only name GPUI 0.2.2 can render
/// for an icon-only control. Taking the id here makes it impossible to build an
/// icon button without one.
pub fn icon_button(
    theme: &Theme,
    name: &'static str,
    id: impl Into<ElementId>,
    label: &'static str,
) -> Stateful<Div> {
    let hover = theme.nav_hover_bg;
    let theme = *theme;

    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .w(px(CONTROL_MD))
        .h(px(CONTROL_MD))
        .rounded(px(RADIUS_MD))
        .flex_shrink_0()
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .tooltip(label_tooltip(theme, label))
        .child(icon(name, IconSize::Md, theme.label_muted))
}

/// A glyph that turns for as long as it stays mounted — the in-flight state of
/// a fetch/pull/push.
///
/// Only render it while work is actually running: a repeating animation asks
/// for a frame on every paint, so an always-mounted spinner keeps the window
/// redrawing for nothing. `key` must be stable per spinner (screen position and
/// target) or the animation restarts from zero each render.
pub fn spinner(
    key: impl Into<SharedString>,
    name: &'static str,
    size: IconSize,
    color: Rgba,
) -> impl IntoElement {
    let key: SharedString = key.into();
    icon_svg(name, size.px(), color).with_animation(
        ElementId::Name(format!("spinner-{key}").into()),
        Animation::new(SPIN_CYCLE).repeat(),
        |glyph, delta| {
            glyph.with_transformation(Transformation::rotate(gpui::radians(
                delta * std::f32::consts::TAU,
            )))
        },
    )
}
