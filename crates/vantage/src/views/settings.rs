//! Settings modal — v2 preferences only.

use gpui::{div, prelude::*, px, relative, Context, Div, IntoElement, Styled, Window};

use crate::app::{FieldId, VantageApp};
use crate::settings::{DefaultView, SettingsSection, SortOrder, ThemeChoice};
use crate::theme::{Theme, FONT_MONO, LH_SM, TEXT_SM};
use crate::widgets::{primary_button, secondary_button, FieldVariant, Txt};

impl VantageApp {
    pub fn render_settings_modal(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme;
        let section = self.settings_section;

        let backdrop = div()
            .debug_selector(|| "settings-backdrop".to_string())
            .id("settings-backdrop")
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .w(relative(1.0))
            .h(relative(1.0))
            .bg(theme.overlay)
            .on_click(cx.listener(|this, _e, _w, cx| this.close_settings(cx)));

        let card = div()
            .debug_selector(|| "settings-card".to_string())
            .id("settings-card")
            .occlude()
            .relative()
            .flex()
            .flex_col()
            .w(px(760.0))
            .h(px(580.0))
            .max_h(relative(0.9))
            .bg(theme.panel)
            .border_1()
            .border_color(theme.divider)
            .rounded(px(12.0))
            .shadow_xl()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(20.0))
                    .py(px(16.0))
                    .border_b_1()
                    .border_color(theme.divider)
                    .child(
                        div()
                            .display_base()
                            .semibold()
                            .fg(theme.text_app)
                            .child("Settings"),
                    )
                    .child(
                        div()
                            .id("settings-close")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(32.0))
                            .h(px(32.0))
                            .rounded(px(8.0))
                            .cursor_pointer()
                            .hover(move |s| s.bg(theme.nav_hover_bg))
                            .on_click(cx.listener(|this, _e, _w, cx| this.close_settings(cx)))
                            .child(crate::icons::icon("x", 16.0, theme.label_muted)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(self.render_settings_nav(cx))
                    .child(
                        div()
                            .id("settings-content")
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w(px(0.0))
                            .overflow_y_scroll()
                            .gap(px(14.0))
                            .p(px(20.0))
                            .children(self.render_settings_section(section, window, cx)),
                    ),
            );

        div()
            .id("settings-overlay")
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .w(relative(1.0))
            .h(relative(1.0))
            .flex()
            .items_center()
            .justify_center()
            .p(px(16.0))
            .child(backdrop)
            .child(card)
    }

    fn render_settings_nav(&mut self, cx: &mut Context<Self>) -> Div {
        let theme = self.theme;
        let current = self.settings_section;
        let mut nav = div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .w(px(168.0))
            .p(px(12.0))
            .gap(px(4.0))
            .border_r_1()
            .border_color(theme.divider)
            .bg(theme.subtle_bg);

        for section in SettingsSection::ALL {
            let selected = section == current;
            nav = nav.child(
                div()
                    .id(gpui::SharedString::from(format!(
                        "settings-nav-{}",
                        section.label()
                    )))
                    .flex()
                    .items_center()
                    .w_full()
                    .px(px(10.0))
                    .py(px(8.0))
                    .rounded(px(8.0))
                    .cursor_pointer()
                    .sm()
                    .medium()
                    .fg(if selected {
                        theme.nav_active_text
                    } else {
                        theme.nav_idle_text
                    })
                    .when(selected, |d| d.bg(theme.nav_active_bg))
                    .when(!selected, move |d| {
                        d.hover(move |s| s.bg(theme.nav_hover_bg).fg(theme.nav_hover_text))
                    })
                    .on_click(cx.listener(move |this, _e, _w, cx| {
                        this.settings_section = section;
                        cx.notify();
                    }))
                    .child(section.label()),
            );
        }
        nav
    }

    fn render_settings_section(
        &mut self,
        section: SettingsSection,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        match section {
            SettingsSection::General => self.render_general_settings(cx),
            SettingsSection::Appearance => self.render_appearance_settings(window, cx),
            SettingsSection::Git => self.render_git_settings(window, cx),
            SettingsSection::Data => self.render_data_settings(cx),
            SettingsSection::About => self.render_about_settings(cx),
        }
    }

    fn render_general_settings(&mut self, cx: &mut Context<Self>) -> Vec<gpui::AnyElement> {
        let theme = self.theme;
        let current_view = self.settings.default_view;
        let mut view_choices = div().flex().gap(px(6.0));
        for (index, (view, label)) in [
            (DefaultView::Overview, "All Repositories"),
            (DefaultView::Attention, "Needs Attention"),
            (DefaultView::Changes, "Pending Changes"),
        ]
        .into_iter()
        .enumerate()
        {
            let selected = current_view == view;
            view_choices = view_choices.child(
                option_button(&theme, label, selected, format!("settings-view-{index}"))
                    .on_click(cx.listener(move |this, _e, _w, cx| this.set_default_view(view, cx))),
            );
        }

        let current_sort = self.settings.sort;
        let mut sort_choices = div().flex().gap(px(6.0));
        for (index, (sort, label)) in [
            (SortOrder::Recent, "Recent activity"),
            (SortOrder::Name, "Name"),
            (SortOrder::Changes, "Most changes"),
        ]
        .into_iter()
        .enumerate()
        {
            let selected = current_sort == sort;
            sort_choices = sort_choices.child(
                option_button(&theme, label, selected, format!("settings-sort-{index}"))
                    .on_click(cx.listener(move |this, _e, _w, cx| this.set_sort(sort, cx))),
            );
        }

        let remember_window = self.settings.remember_window;
        let mut window_choices = div().flex().gap(px(6.0));
        for (index, (enabled, label)) in [(true, "Remember"), (false, "Always center")]
            .into_iter()
            .enumerate()
        {
            window_choices = window_choices.child(
                option_button(
                    &theme,
                    label,
                    remember_window == enabled,
                    format!("settings-window-{index}"),
                )
                .on_click(
                    cx.listener(move |this, _e, _w, cx| this.set_remember_window(enabled, cx)),
                ),
            );
        }

        vec![
            setting_row(
                &theme,
                "Default view",
                "Which view opens when Vantage starts.",
                view_choices,
            )
            .into_any_element(),
            setting_row(
                &theme,
                "Repository sort",
                "Rows are sorted inside each group.",
                sort_choices,
            )
            .into_any_element(),
            setting_row(
                &theme,
                "Window memory",
                "Restore the last normal window size and position.",
                window_choices,
            )
            .into_any_element(),
            setting_row(
                &theme,
                "Sidebar",
                "The sidebar remembers its last state automatically.",
                div()
                    .xs()
                    .fg(theme.label_muted)
                    .child("Use the header button or Mod+B to toggle it."),
            )
            .into_any_element(),
        ]
    }

    fn render_appearance_settings(
        &mut self,
        _window: &Window,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        let theme = self.theme;
        let current = self.settings.theme;
        let mut choices = div().flex().gap(px(6.0));
        for (index, (choice, label)) in [
            (ThemeChoice::System, "System"),
            (ThemeChoice::Light, "Light"),
            (ThemeChoice::Dark, "Dark"),
        ]
        .into_iter()
        .enumerate()
        {
            choices = choices.child(
                option_button(
                    &theme,
                    label,
                    current == choice,
                    format!("settings-theme-{index}"),
                )
                .on_click(cx.listener(move |this, _e, window, cx| {
                    this.set_theme_choice(choice, window.appearance(), cx)
                })),
            );
        }

        vec![setting_row(
            &theme,
            "Theme",
            "System follows the macOS appearance and updates live.",
            choices,
        )
        .into_any_element()]
    }

    fn render_git_settings(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        let theme = self.theme;
        let current = self.settings.auto_fetch_minutes;
        let mut intervals = div().flex().flex_wrap().gap(px(6.0));
        for (index, (minutes, label)) in [
            (0, "Off"),
            (1, "1 min"),
            (5, "5 min"),
            (15, "15 min"),
            (30, "30 min"),
            (60, "60 min"),
        ]
        .into_iter()
        .enumerate()
        {
            intervals = intervals.child(
                option_button(
                    &theme,
                    label,
                    current == minutes,
                    format!("settings-fetch-{index}"),
                )
                .on_click(
                    cx.listener(move |this, _e, _w, cx| this.set_auto_fetch_minutes(minutes, cx)),
                ),
            );
        }

        let mut launch_choices = div().flex().gap(px(6.0));
        for (index, (enabled, label)) in [(true, "Fetch on launch"), (false, "Do not fetch")]
            .into_iter()
            .enumerate()
        {
            launch_choices = launch_choices.child(
                option_button(
                    &theme,
                    label,
                    self.settings.fetch_on_launch == enabled,
                    format!("settings-launch-fetch-{index}"),
                )
                .on_click(
                    cx.listener(move |this, _e, _w, cx| this.set_fetch_on_launch(enabled, cx)),
                ),
            );
        }

        let branch = self.text_field(
            FieldId::SettingsBranch,
            "main",
            FieldVariant::Form,
            window,
            cx,
        );

        vec![
            setting_row(
                &theme,
                "Auto-fetch",
                "Fetch every repository in the background. Failures are reported quietly.",
                intervals,
            )
            .into_any_element(),
            setting_row(
                &theme,
                "On launch",
                "Fetch all repositories once after the first scan.",
                launch_choices,
            )
            .into_any_element(),
            setting_row(
                &theme,
                "Default branch",
                "Shown for a repository with no commits and no origin HEAD.",
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(div().w(px(220.0)).child(branch))
                    .child(
                        secondary_button(&theme, "Save", None)
                            .id("settings-save-branch")
                            .on_click(
                                cx.listener(|this, _e, _w, cx| this.commit_settings_branch(cx)),
                            ),
                    ),
            )
            .into_any_element(),
        ]
    }

    fn render_data_settings(&mut self, cx: &mut Context<Self>) -> Vec<gpui::AnyElement> {
        let theme = self.theme;
        let path = self.state_path().display().to_string();
        let mut reset = div().flex().flex_col().gap(px(6.0)).w_full();
        if self.settings_reset_confirm {
            reset = reset
                .child(
                    div()
                        .xs()
                        .fg(theme.status_red)
                        .child("Reset theme, view, sort, sidebar, fetch and window preferences?"),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            secondary_button(&theme, "Cancel", None)
                                .id("settings-reset-cancel")
                                .on_click(cx.listener(|this, _e, _w, cx| {
                                    this.settings_reset_confirm = false;
                                    cx.notify();
                                })),
                        )
                        .child(
                            primary_button(&theme, "Reset preferences", None)
                                .id("settings-reset-confirm")
                                .on_click(cx.listener(|this, _e, window, cx| {
                                    this.reset_settings(window.appearance(), cx)
                                })),
                        ),
                );
        } else {
            reset = reset.child(
                secondary_button(&theme, "Reset preferences…", None)
                    .id("settings-reset")
                    .on_click(cx.listener(|this, _e, _w, cx| {
                        this.settings_reset_confirm = true;
                        cx.notify();
                    })),
            );
        }

        vec![
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .py(px(10.0))
                .border_b_1()
                .border_color(theme.divider)
                .child(div().sm().medium().fg(theme.text_app).child("State file"))
                .child(
                    div()
                        .xs()
                        .fg(theme.label_muted)
                        .child("Repositories, groups, notes and preferences live here."),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .w_full()
                        .min_w(px(0.0))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .truncate()
                                .font_family(FONT_MONO)
                                .text_size(px(TEXT_SM))
                                .line_height(LH_SM)
                                .child(path),
                        )
                        .child(
                            secondary_button(&theme, "Reveal", Some("external-link"))
                                .id("settings-reveal")
                                .on_click(
                                    cx.listener(|this, _e, _w, cx| this.reveal_state_path(cx)),
                                ),
                        ),
                )
                .into_any_element(),
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .py(px(10.0))
                .border_b_1()
                .border_color(theme.divider)
                .child(div().sm().medium().fg(theme.text_app).child("Reset"))
                .child(
                    div()
                        .xs()
                        .fg(theme.label_muted)
                        .child("This never removes repositories, groups or legacy v1 data."),
                )
                .child(reset)
                .into_any_element(),
        ]
    }

    fn render_about_settings(&mut self, cx: &mut Context<Self>) -> Vec<gpui::AnyElement> {
        let theme = self.theme;
        let diagnostics = self.diagnostics.clone().unwrap_or_default();
        let mut rows = div().flex().flex_col().gap(px(6.0));
        for (label, value) in [
            ("Vantage", diagnostics.version),
            ("Build", diagnostics.build),
            ("Git", diagnostics.git),
            ("System", diagnostics.os),
            ("Architecture", diagnostics.arch),
            ("State", diagnostics.state_path),
        ] {
            rows = rows.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .w_full()
                    .min_w(px(0.0))
                    .child(
                        div()
                            .xs()
                            .flex_shrink_0()
                            .fg(theme.label_muted)
                            .child(label),
                    )
                    .child(
                        div()
                            .xs()
                            .min_w(px(0.0))
                            .truncate()
                            .font_family(FONT_MONO)
                            .fg(theme.text_body)
                            .child(value),
                    ),
            );
        }

        let mut links = div().flex().items_center().gap(px(8.0));
        for (index, (label, url)) in [
            ("GitHub", "https://github.com/beingbalveer/Vantage"),
            (
                "License",
                "https://github.com/beingbalveer/Vantage/blob/main/LICENSE",
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let url = url.to_string();
            links = links.child(
                secondary_button(&theme, label, Some("external-link"))
                    .id(gpui::SharedString::from(format!("settings-link-{index}")))
                    .on_click(cx.listener(move |this, _e, _w, cx| this.open_url(&url, cx))),
            );
        }

        vec![div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .p(px(12.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(theme.divider)
            .bg(theme.subtle_bg)
            .child(
                div()
                    .sm()
                    .semibold()
                    .fg(theme.text_app)
                    .child("Diagnostics"),
            )
            .child(rows)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .pt(px(4.0))
                    .child(
                        secondary_button(&theme, "Copy diagnostics", Some("copy"))
                            .id("settings-copy-diagnostics")
                            .on_click(cx.listener(|this, _e, _w, cx| this.copy_diagnostics(cx))),
                    )
                    .child(links),
            )
            .into_any_element()]
    }
}

fn option_button(
    theme: &Theme,
    label: &'static str,
    selected: bool,
    id: String,
) -> gpui::Stateful<Div> {
    div()
        .id(gpui::SharedString::from(id))
        .px(px(10.0))
        .py(px(6.0))
        .rounded(px(7.0))
        .border_1()
        .border_color(if selected {
            theme.accent
        } else {
            theme.secondary_border
        })
        .bg(if selected {
            theme.accent_soft
        } else {
            theme.field_bg
        })
        .fg(if selected {
            theme.accent_text
        } else {
            theme.secondary_text
        })
        .cursor_pointer()
        .xs()
        .medium()
        .when(!selected, move |d| {
            d.hover(move |s| s.bg(theme.secondary_bg_hover))
        })
        .child(label)
}

fn setting_row(
    theme: &Theme,
    title: &'static str,
    description: &'static str,
    control: impl IntoElement,
) -> Div {
    div()
        .flex()
        .items_start()
        .justify_between()
        .gap(px(20.0))
        .py(px(10.0))
        .border_b_1()
        .border_color(theme.divider)
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .flex_1()
                .min_w(px(0.0))
                .child(div().sm().medium().fg(theme.text_app).child(title))
                .child(div().xs().fg(theme.label_muted).child(description)),
        )
        .child(div().flex_shrink_0().child(control))
}

#[cfg(test)]
mod tests {
    use gpui::{point, px, AppContext, Modifiers, TestAppContext};

    use crate::app::VantageApp;
    use crate::root::RootView;

    #[gpui::test]
    fn settings_modal_opens_and_closes(cx: &mut TestAppContext) {
        let app = cx.new(VantageApp::new);
        let (_root, cx) = cx.add_window_view(|_window, _cx| RootView::new(app.clone()));

        cx.update(|window, cx| {
            app.update(cx, |app, cx| app.open_settings(cx));
            window.draw(cx).clear();
        });
        assert!(cx.update(|_window, cx| app.read(cx).is_settings_open));
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear();
        });

        assert!(
            cx.debug_bounds("settings-card").is_some(),
            "Settings should render a card"
        );

        let card = cx
            .debug_bounds("settings-card")
            .expect("settings card bounds");
        cx.simulate_click(
            point(card.origin.x + px(6.0), card.center().y),
            Modifiers::none(),
        );
        assert!(
            cx.update(|_window, cx| app.read(cx).is_settings_open),
            "a click on the card body must not dismiss Settings"
        );

        cx.simulate_click(point(px(2.0), px(2.0)), Modifiers::none());
        assert!(
            !cx.update(|_window, cx| app.read(cx).is_settings_open),
            "a click outside the card should dismiss Settings"
        );
    }
}
