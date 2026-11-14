//! Merged app header — 42px, full window width.
//!
//! On macOS this row shares space with the native traffic lights, so it keeps
//! an ~84px left inset. The app name lives in the macOS menu bar, so the header
//! starts with the sidebar toggle and breadcrumb instead of a second wordmark.

use gpui::{div, prelude::*, px, rgb, Context, Div, IntoElement, MouseButton, Styled, Window};

use crate::app::{FieldId, VantageApp};
use crate::icons::icon;
use crate::theme::{Theme, SLATE_300, SLATE_700};
use crate::widgets::Txt;

impl VantageApp {
    pub fn render_top_bar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
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
        let left_pad = if cfg!(target_os = "macos") && !window.is_fullscreen() {
            px(84.0)
        } else {
            px(20.0)
        };

        div()
            .relative()
            .flex()
            .items_center()
            .gap(px(12.0))
            .h(crate::theme::HEADER_HEIGHT)
            .flex_shrink_0()
            .pl(left_pad)
            .pr(px(18.0))
            .bg(theme.header_bg)
            .border_b_1()
            .border_color(theme.divider)
            // Drawn first so the buttons and breadcrumb sit above it; empty
            // areas of the bar fall through to this layer and move the window.
            .child(
                div()
                    .id("header-drag-region")
                    .absolute()
                    .top(px(0.0))
                    .left(px(0.0))
                    .w_full()
                    .h_full()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|_, e: &gpui::MouseDownEvent, window, _| {
                            if e.click_count == 2 {
                                window.titlebar_double_click();
                            }
                        }),
                    )
                    .on_mouse_move(cx.listener(|_, e: &gpui::MouseMoveEvent, window, _cx| {
                        if e.dragging() && !window.is_fullscreen() {
                            crate::chrome::start_window_drag();
                        }
                    })),
            )
            .child(
                header_action(&theme, "panel-left")
                    .id("sidebar-toggle")
                    .on_click(cx.listener(|this, _e, _w, cx| this.toggle_sidebar(cx))),
            )
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
                        header_action(&theme, "refresh-cw")
                            .id("refresh-status")
                            .on_click(cx.listener(|this, _e, _w, cx| this.refresh(cx))),
                    )
                    .child(
                        header_action(&theme, "plus")
                            .id("add-repository")
                            .on_click(cx.listener(|this, _e, window, cx| {
                                this.open_add_modal(cx);
                                window.focus(&this.field_focus(FieldId::ModalPath));
                            })),
                    )
                    .child(
                        header_action(&theme, "panel-right")
                            .id("right-panel-toggle")
                            .on_click(cx.listener(|this, _e, window, cx| {
                                this.toggle_right_panel(cx);
                                window.refresh();
                            })),
                    ),
            )
    }
}

/// Uniform icon-only action for the header.
fn header_action(theme: &Theme, icon_name: &'static str) -> Div {
    let hover = theme.nav_hover_bg;

    div()
        .flex()
        .items_center()
        .justify_center()
        .w(px(30.0))
        .h(px(30.0))
        .rounded(px(8.0))
        .flex_shrink_0()
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .child(icon(icon_name, 16.0, theme.label_muted))
}
