//! Top bar — 56px, breadcrumb on the left, the two primary actions on the right.

use gpui::{div, prelude::*, px, rgb, Context, IntoElement, Styled};

use crate::app::{BranchDeckApp, FieldId};
use crate::icons::icon;
use crate::theme::{SLATE_300, SLATE_700};
use crate::widgets::{primary_button, secondary_button, Txt};

impl BranchDeckApp {
    pub fn render_top_bar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let group_label = if self.active_group == "All" {
            "All Groups".to_string()
        } else {
            self.active_group.clone()
        };
        let view_label = self.active_section.label();
        let slash_color = if theme.is_dark {
            rgb(SLATE_700)
        } else {
            rgb(SLATE_300)
        };

        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(16.0))
            .h(px(56.0))
            .flex_shrink_0()
            .px(px(24.0))
            .bg(theme.header_bg)
            .border_b_1()
            .border_color(theme.divider)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .flex_1()
                    .min_w(px(0.0))
                    .sm()
                    .child(
                        div()
                            .flex_shrink_0()
                            .whitespace_nowrap()
                            .fg(theme.label_muted)
                            .child(group_label),
                    )
                    .child(div().flex_shrink_0().fg(slash_color).child("/"))
                    .child(
                        div()
                            .display_sm()
                            .semibold()
                            .truncate()
                            .fg(theme.text_app)
                            .child(view_label),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .flex_shrink_0()
                    .child(
                        secondary_button(&theme, "Refresh Status", Some("refresh-cw"))
                            .id("refresh-status")
                            .on_click(cx.listener(|this, _e, _w, cx| this.refresh(cx))),
                    )
                    .child(
                        primary_button(&theme, "Add Repository", Some("plus"))
                            .id("add-repository")
                            .on_click(cx.listener(|this, _e, window, cx| {
                                this.open_add_modal(cx);
                                window.focus(&this.field_focus(FieldId::ModalPath));
                            })),
                    ),
            )
    }
}

#[allow(dead_code)]
fn _keep(_: &dyn Fn() -> gpui::Div) {
    let _ = icon;
}
