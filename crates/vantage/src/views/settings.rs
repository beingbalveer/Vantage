//! Settings modal — v2 preferences only.

use gpui::{div, prelude::*, px, relative, Context, Div, IntoElement, Styled, Window};

use vantage_core::git::refs::PullStrategy;

use crate::app::{FieldId, VantageApp};
use crate::settings::{DefaultView, SettingsSection, SortOrder, ThemeChoice};
use crate::theme::{Theme, FONT_MONO, LH_SM, TEXT_SM};
use crate::ui::{primary_button, secondary_button, FieldVariant, Txt};

impl VantageApp {
    pub fn render_settings_modal(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme;
        let section = self.settings_section;
        if !self.modal_focus.contains_focused(window, cx) {
            window.focus(&self.modal_focus);
        }

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
            .track_focus(&self.modal_focus)
            .key_context(crate::app::MODAL_KEY_CONTEXT)
            .on_key_down(cx.listener(Self::handle_modal_key))
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
                        crate::ui::icon_button(&theme, "x")
                            .id("settings-close")
                            .on_click(cx.listener(|this, _e, _w, cx| this.close_settings(cx))),
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
            SettingsSection::General => self.render_general_settings(window, cx),
            SettingsSection::Appearance => self.render_appearance_settings(window, cx),
            SettingsSection::Git => self.render_git_settings(window, cx),
            SettingsSection::Data => self.render_data_settings(cx),
            SettingsSection::About => self.render_about_settings(cx),
        }
    }

    fn render_general_settings(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::AnyElement> {
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
                crate::ui::choice_button(&theme, label, selected, format!("settings-view-{index}"))
                    .on_click(cx.listener(move |this, _e, _w, cx| this.set_default_view(view, cx))),
            );
        }

        let current_sort = self.settings.sort;
        let mut sort_choices = div().flex().gap(px(6.0));
        for (index, (sort, label)) in [
            (SortOrder::Recent, "Recent activity"),
            (SortOrder::Attention, "Attention first"),
            (SortOrder::Name, "Name"),
            (SortOrder::Changes, "Most changes"),
        ]
        .into_iter()
        .enumerate()
        {
            let selected = current_sort == sort;
            sort_choices = sort_choices.child(
                crate::ui::choice_button(&theme, label, selected, format!("settings-sort-{index}"))
                    .on_click(cx.listener(move |this, _e, _w, cx| this.set_sort(sort, cx))),
            );
        }

        let remember_window = self.settings.remember_window;
        let window_control = div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .child(div().xs().fg(theme.label_muted).child(if remember_window {
                "Remember"
            } else {
                "Always center"
            }))
            .child(
                crate::ui::switch(&theme, remember_window)
                    .id("settings-remember-window")
                    .on_click(cx.listener(|this, _e, _w, cx| {
                        let next = !this.settings.remember_window;
                        this.set_remember_window(next, cx);
                    })),
            );

        let editor = self.text_field(
            FieldId::SettingsEditor,
            "Automatic (VISUAL / EDITOR / code)",
            FieldVariant::Form,
            window,
            cx,
        );

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
                window_control,
            )
            .into_any_element(),
            setting_row(
                &theme,
                "Editor",
                "Command for Open in Editor. Leave empty to use VISUAL, EDITOR or a common \
                 editor; arguments such as `code --wait` are supported.",
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(div().w(px(300.0)).child(editor))
                    .child(
                        secondary_button(&theme, "Save", None)
                            .id("settings-save-editor")
                            .on_click(
                                cx.listener(|this, _e, _w, cx| this.commit_settings_editor(cx)),
                            ),
                    ),
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
                crate::ui::choice_button(
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
                crate::ui::choice_button(
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

        let fetch_on_launch = self.settings.fetch_on_launch;
        let launch_control = div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .child(div().xs().fg(theme.label_muted).child(if fetch_on_launch {
                "Fetch on launch"
            } else {
                "Do not fetch"
            }))
            .child(
                crate::ui::switch(&theme, fetch_on_launch)
                    .id("settings-launch-fetch")
                    .on_click(cx.listener(|this, _e, _w, cx| {
                        let next = !this.settings.fetch_on_launch;
                        this.set_fetch_on_launch(next, cx);
                    })),
            );

        let branch = self.text_field(
            FieldId::SettingsBranch,
            "main",
            FieldVariant::Form,
            window,
            cx,
        );

        let current_pull = self.settings.pull_strategy;
        let mut pull_choices = div().flex().flex_wrap().gap(px(6.0));
        for (index, (strategy, label)) in [
            (PullStrategy::FastForward, "Fast-forward only"),
            (PullStrategy::Merge, "Merge"),
            (PullStrategy::Rebase, "Rebase"),
        ]
        .into_iter()
        .enumerate()
        {
            pull_choices = pull_choices.child(
                crate::ui::choice_button(
                    &theme,
                    label,
                    current_pull == strategy,
                    format!("settings-pull-{index}"),
                )
                .on_click(
                    cx.listener(move |this, _e, _w, cx| this.set_pull_strategy(strategy, cx)),
                ),
            );
        }

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
                launch_control,
            )
            .into_any_element(),
            setting_row(
                &theme,
                "Pull",
                "Fast-forward never merges or rebases behind your back; it reports a diverged \
                 branch instead. Merge and Rebase apply to the branch each repository has \
                 checked out.",
                pull_choices,
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
        let warning = self.state_warning.clone();
        let backups = self.state_backups.clone();
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

        let mut backup_rows = div().flex().flex_col().gap(px(4.0)).w_full();
        if backups.is_empty() {
            backup_rows = backup_rows.child(
                div()
                    .xs()
                    .fg(theme.label_muted)
                    .child("No state backups in the data folder."),
            );
        } else {
            for (index, backup) in backups.iter().enumerate() {
                let backup_path = backup.path.clone();
                let file_name = backup
                    .path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_else(|| backup.path.display().to_string());
                backup_rows = backup_rows.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .w_full()
                        .min_w(px(0.0))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(1.0))
                                .flex_1()
                                .min_w(px(0.0))
                                .child(
                                    div()
                                        .xs()
                                        .medium()
                                        .truncate()
                                        .font_family(FONT_MONO)
                                        .fg(theme.text_body)
                                        .child(file_name),
                                )
                                .child(
                                    div().x11().fg(theme.label_muted).child(backup.kind.label()),
                                ),
                        )
                        .child(
                            secondary_button(&theme, "Reveal", Some("external-link"))
                                .id(gpui::SharedString::from(format!("settings-backup-{index}")))
                                .on_click(cx.listener(move |this, _e, _w, cx| {
                                    this.reveal_state_backup(backup_path.clone(), cx)
                                })),
                        ),
                );
            }
        }

        let mut sections: Vec<gpui::AnyElement> = Vec::new();
        if let Some(warning) = warning {
            sections.push(
                div()
                    .debug_selector(|| "settings-state-warning".to_string())
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .p(px(10.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(theme.error_border)
                    .bg(theme.error_bg)
                    .child(
                        div()
                            .sm()
                            .semibold()
                            .fg(theme.error_text)
                            .child("Saved state needed recovery"),
                    )
                    .child(div().xs().fg(theme.text_body).child(warning))
                    .into_any_element(),
            );
        }

        sections.push(
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
                            secondary_button(&theme, "Open folder", Some("folder-git-2"))
                                .id("settings-open-data-folder")
                                .on_click(
                                    cx.listener(|this, _e, _w, cx| this.open_state_data_folder(cx)),
                                ),
                        ),
                )
                .into_any_element(),
        );
        sections.push(
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .py(px(10.0))
                .border_b_1()
                .border_color(theme.divider)
                .child(div().sm().medium().fg(theme.text_app).child("Backups"))
                .child(div().xs().fg(theme.label_muted).child(
                    "Copies found next to the state file. To restore one, quit Vantage and \
                             copy it over state.json; Vantage never replaces the live state \
                             automatically.",
                ))
                .child(backup_rows)
                .into_any_element(),
        );
        sections.push(
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
        );
        sections
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
