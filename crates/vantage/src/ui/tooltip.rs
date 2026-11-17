//! Hover card for an identity: who the avatar belongs to.

use gpui::{div, prelude::*, px, AnyView, App, Context, IntoElement, Render, Window};

use crate::theme::Theme;
use crate::ui::Txt;

/// Name and email shown when the pointer rests on an avatar.
///
/// Built on demand by the tooltip builder, so it owns its strings and never
/// borrows the row that spawned it.
pub struct IdentityTooltip {
    theme: Theme,
    name: String,
    email: String,
}

impl IdentityTooltip {
    pub fn new(theme: Theme, name: impl Into<String>, email: impl Into<String>) -> Self {
        Self {
            theme,
            name: name.into(),
            email: email.into(),
        }
    }
}

impl Render for IdentityTooltip {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .px(px(10.0))
            .py(px(7.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(self.theme.divider)
            .bg(self.theme.panel)
            .shadow_lg()
            .child(
                div()
                    .xs()
                    .medium()
                    .fg(self.theme.text_app)
                    .max_w(px(360.0))
                    .truncate()
                    .child(self.name.clone()),
            )
            .child(
                div()
                    .mono_xs()
                    .fg(self.theme.label_muted)
                    .max_w(px(360.0))
                    .truncate()
                    .child(self.email.clone()),
            )
    }
}

/// A single muted line, used where the full text does not fit in the row.
pub struct TextTooltip {
    theme: Theme,
    text: String,
}

impl TextTooltip {
    pub fn new(theme: Theme, text: impl Into<String>) -> Self {
        Self {
            theme,
            text: text.into(),
        }
    }
}

impl Render for TextTooltip {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(8.0))
            .py(px(4.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(self.theme.divider)
            .bg(self.theme.panel)
            .shadow_lg()
            .mono_xs()
            .fg(self.theme.text_soft)
            .max_w(px(360.0))
            .truncate()
            .child(self.text.clone())
    }
}

/// Build a [`TextTooltip`] for an icon-only control.
///
/// GPUI 0.2.2 exposes no accessibility tree, so this visible hover label is
/// the only accessible-name affordance the app can attach today. Give every
/// icon-only button one.
pub fn label_tooltip(
    theme: Theme,
    label: &'static str,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    move |_window, cx| cx.new(|_| TextTooltip::new(theme, label)).into()
}
