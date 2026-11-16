//! Left sidebar — primary views, groups and the Settings entry. Toggle from the
//! app header or `Mod+B`; a collapsed sidebar keeps the view icons, an expand
//! control and the footer.

use gpui::{div, prelude::*, px, rgba, Context, Div, IntoElement, MouseButton, Styled, Window};

use crate::app::{FieldId, VantageApp};
use crate::ui::{icon, IconSize};
use crate::ui::{FieldVariant, TextTooltip, Txt};

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
                self.sidebar_width()
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
                    .child(self.render_sidebar_nav(window, cx)),
            )
            .child(self.render_sidebar_footer(cx))
    }

    fn render_sidebar_nav(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let theme = self.theme;

        if !self.sidebar_collapsed {
            return div()
                .flex()
                .flex_col()
                .px(px(12.0))
                .py(px(20.0))
                .child(self.render_primary_nav(cx))
                .child(
                    div()
                        .mt(px(18.0))
                        .pt(px(16.0))
                        .border_t_1()
                        .border_color(theme.divider)
                        .child(self.render_groups_block(window, cx)),
                );
        }

        // The rail keeps every destination visible as an icon with its count,
        // so collapsing the sidebar never hides navigation behind the menu.
        div()
            .flex()
            .flex_col()
            .items_center()
            .py(px(12.0))
            .gap(px(4.0))
            .child(
                crate::ui::icon_button(&theme, "panel-left")
                    .id("sidebar-expand")
                    .on_click(cx.listener(|this, _e, _w, cx| this.toggle_sidebar(cx))),
            )
            .child(self.render_collapsed_nav(cx))
    }

    /// The three main destinations with live counts, above the group list.
    fn render_primary_nav(&mut self, cx: &mut Context<Self>) -> Div {
        let theme = self.theme;
        let items = self.nav_items();

        let block = div().flex().flex_col().child(
            div()
                .px(px(10.0))
                .pb(px(8.0))
                .xs()
                .medium()
                .fg(theme.label_muted)
                .child("Views"),
        );
        let mut list = div().flex().flex_col().gap(px(4.0));

        for item in items {
            let section = item.section;
            let active = item.active;
            let icon_color = if active {
                theme.nav_active_text
            } else {
                theme.icon_idle
            };

            list = list.child(
                div()
                    .id(gpui::SharedString::from(format!(
                        "nav-section-{}",
                        section.key()
                    )))
                    .flex()
                    .items_center()
                    .justify_between()
                    .w_full()
                    .px(px(10.0))
                    .py(px(8.0))
                    .rounded(px(8.0))
                    .cursor_pointer()
                    .bg(if active {
                        theme.nav_active_bg
                    } else {
                        theme.panel
                    })
                    .sm()
                    .fg(if active {
                        theme.nav_active_text
                    } else {
                        theme.nav_idle_text
                    })
                    // Weight plus background, so the active view is not
                    // signalled by colour alone.
                    .when(active, |d| d.medium())
                    .when(!active, move |d| {
                        d.hover(move |s| s.bg(theme.nav_hover_bg).fg(theme.nav_hover_text))
                    })
                    .on_click(cx.listener(move |this, _e, _w, cx| {
                        this.select_section(section, cx);
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .min_w(px(0.0))
                            .child(icon(section.icon(), IconSize::Sm, icon_color))
                            .child(div().truncate().child(section.label())),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .font_family(crate::theme::FONT_MONO)
                            .text_size(px(crate::theme::TEXT_XS))
                            .line_height(crate::theme::LH_XS)
                            .fg(if active {
                                theme.nav_active_text
                            } else {
                                theme.label_muted
                            })
                            .when(active, |d| d.medium())
                            .child(item.count.to_string()),
                    ),
            );
        }

        block.child(list)
    }

    /// Icon-only destinations for the collapsed 56px rail.
    fn render_collapsed_nav(&mut self, cx: &mut Context<Self>) -> Div {
        let theme = self.theme;
        let items = self.nav_items();
        let mut rail = div().flex().flex_col().items_center().gap(px(4.0));

        for item in items {
            let section = item.section;
            let active = item.active;
            let tooltip = format!("{} · {}", section.label(), item.count);

            rail = rail.child(
                div()
                    .id(gpui::SharedString::from(format!(
                        "nav-rail-{}",
                        section.key()
                    )))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(1.0))
                    .w(px(44.0))
                    .h(px(46.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(rgba(0x00000000))
                    .cursor_pointer()
                    .when(active, |d| {
                        d.bg(theme.nav_active_bg).border_color(theme.border_strong)
                    })
                    .when(!active, move |d| d.hover(move |s| s.bg(theme.nav_hover_bg)))
                    .tooltip(move |_window, cx| {
                        cx.new(|_| TextTooltip::new(theme, tooltip.clone())).into()
                    })
                    .on_click(cx.listener(move |this, _e, _w, cx| {
                        this.select_section(section, cx);
                    }))
                    .child(icon(
                        section.icon(),
                        IconSize::Sm,
                        if active {
                            theme.nav_active_text
                        } else {
                            theme.icon_idle
                        },
                    ))
                    .child(
                        div()
                            .mono_xs()
                            .fg(if active {
                                theme.nav_active_text
                            } else {
                                theme.label_muted
                            })
                            .when(active, |d| d.medium())
                            .child(item.count.to_string()),
                    ),
            );
        }

        rail
    }

    fn render_groups_block(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme;

        let mut block = div().flex().flex_col().child(
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
                        .child(icon("plus", IconSize::Xs, theme.accent_text))
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
                        .child(icon("layers", IconSize::Sm, theme.icon_idle))
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
                        .child(icon("layers", IconSize::Sm, theme.icon_idle))
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
                        .child(icon("x", IconSize::Xs, theme.label_muted)),
                );
            }

            list = list.child(row);
        }

        block.child(list)
    }

    fn render_sidebar_footer(&mut self, cx: &mut Context<Self>) -> Div {
        let theme = self.theme;
        let collapsed = self.sidebar_collapsed;

        let mut settings = div()
            .id("settings-open")
            .flex()
            .items_center()
            .w_full()
            .rounded(px(8.0))
            .border_1()
            .border_color(theme.secondary_border)
            .bg(theme.subtle_bg)
            .cursor_pointer()
            .hover(move |s| s.bg(theme.secondary_bg_hover))
            .on_click(cx.listener(|this, _e, _w, cx| this.open_settings(cx)));

        if collapsed {
            settings = settings.justify_center().py(px(8.0)).child(icon(
                "settings",
                IconSize::Md,
                theme.label_muted,
            ));
        } else {
            settings = settings
                .gap(px(8.0))
                .whitespace_nowrap()
                .px(px(12.0))
                .py(px(8.0))
                .xs()
                .medium()
                .fg(theme.secondary_text)
                .child(icon("settings", IconSize::Sm, theme.label_muted))
                .child(div().child("Settings"));
        }

        div()
            .flex_shrink_0()
            .p(px(if collapsed { 8.0 } else { 12.0 }))
            .border_t_1()
            .border_color(theme.divider)
            .child(settings)
    }
}
