//! Left sidebar — 256px expanded, 56px icon rail collapsed, 56px header,
//! Views, Groups, theme toggle. Toggle with the header button or `Mod+B`.

use gpui::{
    div, img, prelude::*, px, rgb, Context, Div, IntoElement, MouseButton, Stateful, Styled, Window,
};

use crate::app::{FieldId, VantageApp};
use crate::icons::{icon, VANTAGE_ICON};
use crate::model::NavSection;
use crate::theme::{AMBER_400, SLATE_500};
use crate::widgets::{FieldVariant, Txt};

const SIDEBAR_WIDTH: f32 = 256.0;
const SIDEBAR_COLLAPSED_WIDTH: f32 = 56.0;

impl VantageApp {
    pub fn render_sidebar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme;
        let collapsed = self.sidebar_collapsed;

        div()
            .flex()
            .flex_col()
            .justify_between()
            .w(px(if collapsed {
                SIDEBAR_COLLAPSED_WIDTH
            } else {
                SIDEBAR_WIDTH
            }))
            .h_full()
            .flex_shrink_0()
            .bg(theme.panel)
            .border_r_1()
            .border_color(theme.divider)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(px(0.0))
                    .id("sidebar-scroll")
                    .overflow_y_scroll()
                    .child(self.render_sidebar_header(cx))
                    .child(self.render_sidebar_nav(window, cx)),
            )
            .child(self.render_sidebar_footer(cx))
    }

    fn render_sidebar_header(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = self.theme;
        let collapsed = self.sidebar_collapsed;

        if collapsed {
            return div()
                .id("sidebar-expand")
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(2.0))
                .h(px(56.0))
                .flex_shrink_0()
                .border_b_1()
                .border_color(theme.divider)
                .cursor_pointer()
                .hover(move |s| s.bg(theme.nav_hover_bg))
                .on_click(cx.listener(|this, _e, _w, cx| this.toggle_sidebar(cx)))
                .child(img(VANTAGE_ICON).flex_shrink_0().w(px(28.0)).h(px(28.0)))
                .child(icon("panel-left-open", 14.0, theme.label_muted));
        }

        div()
            .id("sidebar-header")
            .flex()
            .items_center()
            .justify_between()
            .h(px(56.0))
            .flex_shrink_0()
            .px(px(20.0))
            .border_b_1()
            .border_color(theme.divider)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .flex_shrink_0()
                    .child(img(VANTAGE_ICON).flex_shrink_0().w(px(28.0)).h(px(28.0)))
                    .child(
                        div()
                            .display_base()
                            .bold()
                            .fg(theme.text_app)
                            .whitespace_nowrap()
                            .flex_shrink_0()
                            .child("Vantage"),
                    ),
            )
            .child(
                div()
                    .id("sidebar-collapse")
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(24.0))
                    .h(px(24.0))
                    .rounded(px(6.0))
                    .flex_shrink_0()
                    .cursor_pointer()
                    .hover(move |s| s.bg(theme.nav_hover_bg))
                    .on_click(cx.listener(|this, _e, _w, cx| this.toggle_sidebar(cx)))
                    .child(icon("panel-left-close", 16.0, theme.label_muted)),
            )
    }

    fn render_sidebar_nav(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let theme = self.theme;
        let collapsed = self.sidebar_collapsed;

        let nav_items: [(NavSection, &'static str, &'static str, usize); 4] = [
            (
                NavSection::Overview,
                "All Repositories",
                "folder-git-2",
                self.repos.len(),
            ),
            (
                NavSection::Attention,
                "Needs Attention",
                "alert-circle",
                self.attention_count(),
            ),
            (
                NavSection::Branches,
                "Branch Matrix",
                "git-branch",
                self.feature_branch_count(),
            ),
            (
                NavSection::Changes,
                "Pending Changes",
                "file-diff",
                self.changed_file_count(),
            ),
        ];

        let mut views_list = div().flex().flex_col().gap(px(4.0));
        for (section, label, icon_name, count) in nav_items {
            let active = self.active_section == section;
            let icon_color = if active {
                theme.accent
            } else {
                theme.icon_idle
            };
            let text_color = if active {
                theme.nav_active_text
            } else {
                theme.nav_idle_text
            };
            let hover_color = theme.nav_hover_text;
            let hover_bg = theme.nav_hover_bg;

            let mut item = div()
                .id(gpui::ElementId::named_usize("nav", section as u8 as usize))
                .flex()
                .items_center()
                .w_full()
                .rounded(px(8.0))
                .cursor_pointer()
                .bg(if active {
                    theme.nav_active_bg
                } else {
                    theme.panel
                })
                .sm()
                .medium()
                .fg(text_color)
                .when(!active, move |d| {
                    d.hover(move |s| s.bg(hover_bg).fg(hover_color))
                })
                .on_click(cx.listener(move |this, _e, _w, cx| {
                    this.select_section(section, cx);
                }));

            if collapsed {
                item = item
                    .justify_center()
                    .py(px(8.0))
                    .child(icon(icon_name, 18.0, icon_color));
            } else {
                item = item
                    .justify_between()
                    .px(px(10.0))
                    .py(px(8.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .flex_shrink_0()
                            .whitespace_nowrap()
                            .child(icon(icon_name, 16.0, icon_color))
                            .child(div().child(label)),
                    )
                    .child(
                        div()
                            .font_family(crate::theme::FONT_MONO)
                            .text_size(px(crate::theme::TEXT_XS))
                            .line_height(crate::theme::LH_XS)
                            .fg(theme.label_muted)
                            .child(count.to_string()),
                    );
            }

            views_list = views_list.child(item);
        }

        let mut nav = div()
            .flex()
            .flex_col()
            .px(px(if collapsed { 8.0 } else { 12.0 }))
            .py(px(if collapsed { 12.0 } else { 20.0 }))
            .gap(px(if collapsed { 8.0 } else { 24.0 }))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .when(!collapsed, |d| {
                        d.child(
                            div()
                                .px(px(10.0))
                                .pb(px(8.0))
                                .xs()
                                .medium()
                                .fg(theme.label_muted)
                                .child("Views"),
                        )
                    })
                    .child(views_list),
            );

        // The groups rail needs its labels to be usable, so it is only drawn
        // in the expanded sidebar.
        if !collapsed {
            nav = nav.child(self.render_groups_block(window, cx));
        }
        nav
    }

    fn render_groups_block(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme;

        let mut block = div()
            .flex()
            .flex_col()
            .pt(px(16.0))
            .border_t_1()
            .border_color(theme.divider)
            .child(
                div()
                    .px(px(10.0))
                    .pb(px(8.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().xs().medium().fg(theme.label_muted).child("Groups"))
                    .child(
                        div()
                            .id("new-group-toggle")
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .cursor_pointer()
                            .whitespace_nowrap()
                            .flex_shrink_0()
                            .xs()
                            .medium()
                            .fg(theme.accent_text)
                            .hover(|s| s.opacity(0.75))
                            .on_click(cx.listener(|this, _e, _w, cx| {
                                this.is_adding_group = !this.is_adding_group;
                                if !this.is_adding_group {
                                    this.group_field.clear();
                                }
                                cx.notify();
                            }))
                            .child(icon("plus", 12.0, theme.accent_text))
                            .child(div().child("New group")),
                    ),
            );

        if self.is_adding_group {
            let theme_local = theme;
            block = block.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .px(px(8.0))
                    .pb(px(10.0))
                    .child(div().flex_1().min_w(px(0.0)).child(self.text_field(
                        FieldId::NewGroup,
                        "Group name...",
                        FieldVariant::Small,
                        window,
                        cx,
                    )))
                    .child(
                        div()
                            .id("new-group-submit")
                            .flex_shrink_0()
                            .px(px(10.0))
                            .py(px(6.0))
                            .rounded(px(6.0))
                            .bg(theme_local.primary_bg)
                            .cursor_pointer()
                            .xs()
                            .medium()
                            .fg(theme_local.primary_text)
                            .hover(move |s| s.bg(theme_local.primary_bg_hover))
                            .on_click(cx.listener(|this, _e, _w, cx| {
                                let name = this.group_field.value.clone();
                                this.group_field.clear();
                                this.is_adding_group = false;
                                this.create_group(name, cx);
                            }))
                            .child("Add"),
                    ),
            );
        }

        let mut list = div().flex().flex_col().gap(px(4.0));

        // "All Groups" row
        let all_active = self.active_group == "All";
        list = list.child(
            div()
                .id("group-all")
                .flex()
                .items_center()
                .justify_between()
                .w_full()
                .px(px(10.0))
                .py(px(8.0))
                .rounded(px(8.0))
                .cursor_pointer()
                .bg(if all_active {
                    theme.nav_active_bg
                } else {
                    theme.panel
                })
                .sm()
                .fg(if all_active {
                    theme.nav_active_text
                } else {
                    theme.nav_idle_text
                })
                .when(all_active, |d| d.medium())
                .when(!all_active, move |d| {
                    d.hover(move |s| s.bg(theme.nav_hover_bg).fg(theme.nav_hover_text))
                })
                .on_click(cx.listener(|this, _e, _w, cx| {
                    this.select_group("All", cx);
                }))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .flex_shrink_0()
                        .whitespace_nowrap()
                        .child(icon("layers", 14.0, theme.icon_idle))
                        .child(div().child("All Groups")),
                )
                .child(
                    div()
                        .font_family(crate::theme::FONT_MONO)
                        .text_size(px(crate::theme::TEXT_XS))
                        .line_height(crate::theme::LH_XS)
                        .fg(theme.label_muted)
                        .child(self.repos.len().to_string()),
                ),
        );

        let groups = self.groups.clone();
        for group in groups {
            let is_active = self.active_group == group;
            let count = self.group_count(&group);
            let can_remove = group != "Ungrouped";
            let theme_row = theme;

            let group_id = group.clone();
            let group_remove_id = group.clone();
            let group_click = group.clone();
            let mut row = div()
                .id(gpui::SharedString::from(format!("group-{group_id}")))
                .group("group/item")
                .relative()
                .flex()
                .items_center()
                .justify_between()
                .w_full()
                .px(px(10.0))
                .py(px(8.0))
                .rounded(px(8.0))
                .cursor_pointer()
                .bg(if is_active {
                    theme.nav_active_bg
                } else {
                    theme.panel
                })
                .sm()
                .fg(if is_active {
                    theme.nav_active_text
                } else {
                    theme.nav_idle_text
                })
                .when(is_active, |d| d.medium())
                .when(!is_active, move |d| {
                    d.hover(move |s| s.bg(theme_row.nav_hover_bg).fg(theme_row.nav_hover_text))
                })
                .on_click(cx.listener(move |this, _e, _w, cx| {
                    this.select_group(group_click.clone(), cx);
                }))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .min_w(px(0.0))
                        .child(icon("layers", 14.0, theme.icon_idle))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .truncate()
                                .child(group.clone()),
                        ),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .font_family(crate::theme::FONT_MONO)
                        .text_size(px(crate::theme::TEXT_XS))
                        .line_height(crate::theme::LH_XS)
                        .fg(theme.label_muted)
                        .group_hover("group/item", |s| s.opacity(0.0))
                        .child(count.to_string()),
                );

            if can_remove {
                let group_name = group.clone();
                row = row.child(
                    div()
                        .id(gpui::SharedString::from(format!(
                            "group-remove-{group_remove_id}"
                        )))
                        .absolute()
                        .right(px(8.0))
                        .top(px(6.0))
                        .p(px(4.0))
                        .rounded(px(4.0))
                        .opacity(0.0)
                        .cursor_pointer()
                        .group_hover("group/item", |s| s.opacity(1.0))
                        .hover(|s| s.fg(theme.status_red))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _e, _w, cx| {
                                cx.stop_propagation();
                                this.delete_group(group_name.clone(), cx);
                            }),
                        )
                        .child(icon("x", 12.0, theme.label_muted)),
                );
            }

            list = list.child(row);
        }

        block.child(list)
    }

    fn render_sidebar_footer(&mut self, cx: &mut Context<Self>) -> Div {
        let theme = self.theme;
        let is_dark = theme.is_dark;
        let collapsed = self.sidebar_collapsed;
        let icon_name = if is_dark { "sun" } else { "moon" };
        let icon_color = if is_dark {
            rgb(AMBER_400)
        } else {
            rgb(SLATE_500)
        };

        let toggle = if collapsed {
            div()
                .id("theme-toggle")
                .flex()
                .items_center()
                .justify_center()
                .w_full()
                .py(px(8.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(theme.secondary_border)
                .bg(theme.subtle_bg)
                .cursor_pointer()
                .hover(move |s| s.bg(theme.secondary_bg_hover))
                .on_click(cx.listener(|this, _e, _w, cx| this.toggle_theme(cx)))
                .child(icon(icon_name, 16.0, icon_color))
        } else {
            div()
                .id("theme-toggle")
                .flex()
                .items_center()
                .justify_between()
                .w_full()
                .px(px(12.0))
                .py(px(8.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(theme.secondary_border)
                .bg(theme.subtle_bg)
                .cursor_pointer()
                .xs()
                .medium()
                .fg(theme.secondary_text)
                .hover(move |s| s.bg(theme.secondary_bg_hover))
                .on_click(cx.listener(|this, _e, _w, cx| this.toggle_theme(cx)))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .whitespace_nowrap()
                        .flex_shrink_0()
                        .child(icon(icon_name, 14.0, icon_color))
                        .child(div().child(if is_dark {
                            "Switch to Light Mode"
                        } else {
                            "Switch to Dark Mode"
                        })),
                )
        };

        div()
            .flex_shrink_0()
            .p(px(if collapsed { 8.0 } else { 12.0 }))
            .border_t_1()
            .border_color(theme.divider)
            .child(toggle)
    }
}
