//! Right repository detail pane — half of the content area, changes / branches / commits.

use gpui::{deferred, div, prelude::*, px, Context, Div, IntoElement, Styled, Window};

use crate::app::{DetailsState, InspectorTab, RepoOp, VantageApp};
use crate::theme::Theme;
use crate::ui::{
    accent_button, accent_button_with, checkbox, disabled_button, label_tooltip, secondary_button,
    secondary_button_with, spinner, Txt, HIT_TARGET_MIN,
};
use crate::ui::{icon, IconSize};

/// The commit bar belongs to the Changes tab. A clean tree normally hides it,
/// but amend must stay reachable because it can rewrite the message alone.
fn shows_commit_bar(tab: InspectorTab, amending: bool, changed_files: usize) -> bool {
    tab == InspectorTab::Changes && (amending || changed_files > 0)
}

impl VantageApp {
    pub fn render_inspector(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        if !self.settings.right_panel_open {
            return None;
        }
        let layout = self.dashboard_layout(window);
        let Some(repo) = self.selected_repo().cloned() else {
            return Some(self.render_empty_detail_panel(layout.inspector_width, cx));
        };
        let tab = self.inspector_tab;
        let expanded = self.expanded_file.clone();
        let copied = self.copied_path;
        let menu_open = self.group_menu_open;
        let theme = self.theme;

        let panel = div()
            .debug_selector(|| "repository-detail".to_string())
            .id("repository-detail")
            .flex()
            .flex_col()
            .w(px(layout.inspector_width))
            .min_w(px(crate::layout::INSPECTOR_MIN_WIDTH))
            .h_full()
            .flex_shrink_0()
            .bg(theme.panel)
            .border_l_1()
            .border_color(theme.divider)
            .child(self.render_inspector_header(&repo, copied, layout.compact_inspector(), cx))
            .child(self.render_inspector_tabs(&repo, menu_open, cx))
            .child({
                // A commit view owns its own scrolling: the file list and the
                // diff each scroll, the tab around them does not.
                let detail_open = tab == InspectorTab::Commits
                    && self
                        .commit_view
                        .as_ref()
                        .is_some_and(|view| view.repo_id == repo.id);
                let content = match tab {
                    InspectorTab::Changes => self
                        .render_changes_tab(&repo, expanded.as_deref(), cx)
                        .into_any_element(),
                    InspectorTab::Branches => {
                        self.render_branches_tab(&repo, cx).into_any_element()
                    }
                    InspectorTab::Commits => self
                        .render_commits_tab(&repo, window, cx)
                        .into_any_element(),
                };
                let body = div().flex_1().min_h(px(0.0)).id("inspector-body");
                if detail_open {
                    body.p(px(12.0)).child(content)
                } else {
                    body.overflow_y_scroll()
                        .px(px(24.0))
                        .py(px(16.0))
                        .child(content)
                }
            })
            // Pinned to the bottom of the pane, so a long file list never
            // scrolls the commit controls away. The undo strip sits directly
            // above them, while the commit it offers is still local.
            .when(
                tab == InspectorTab::Changes && self.undo_candidate(&repo).is_some(),
                |panel| panel.child(self.render_undo_strip(&repo, cx)),
            )
            .when(
                shows_commit_bar(tab, self.composer().amending, repo.uncommitted_files.len()),
                |panel| panel.child(self.render_commit_bar(&repo, window, cx)),
            );

        Some(panel.into_any_element())
    }

    fn render_empty_detail_panel(&self, width: f32, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme;
        div()
            .id("repository-detail-empty")
            .flex()
            .flex_col()
            .w(px(width))
            .min_w(px(crate::layout::INSPECTOR_MIN_WIDTH))
            .h_full()
            .flex_shrink_0()
            .bg(theme.panel)
            .border_l_1()
            .border_color(theme.divider)
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
                            .display_sm()
                            .semibold()
                            .fg(theme.text_app)
                            .child("Repository Details"),
                    )
                    .child(
                        crate::ui::icon_button(
                            &theme,
                            "x",
                            "close-right-panel",
                            "Close details panel",
                        )
                        .on_click(cx.listener(|this, _e, _w, cx| this.close_right_panel(cx))),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .flex_1()
                    .gap(px(6.0))
                    .child(icon("panel-right", IconSize::Xl, theme.text_dim))
                    .child(
                        div()
                            .sm()
                            .medium()
                            .fg(theme.text_body)
                            .child("Select a repository"),
                    )
                    .child(
                        div()
                            .xs()
                            .fg(theme.label_muted)
                            .child("Its changes, branches and commits will appear here."),
                    ),
            )
            .into_any_element()
    }

    fn render_inspector_header(
        &self,
        repo: &crate::model::Repository,
        copied: bool,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme;
        let path = repo.local_path.clone();

        let title = div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .flex_1()
            .min_w(px(0.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .display_lg()
                    .font_weight(crate::theme::W_MEDIUM)
                    .fg(theme.text_app)
                    .truncate()
                    .child(repo.name.clone()),
            );

        // The branch gives way first; the path keeps a minimum column so it
        // stays identifiable even beside a long branch name.
        let branch_tip = repo.current_branch.clone();
        let branch = div()
            .flex()
            .items_center()
            .gap(px(5.0))
            .flex_shrink()
            .min_w(px(0.0))
            .child(icon("git-branch", IconSize::Xxs, theme.text_dim))
            .child(
                div()
                    .id("inspector-branch")
                    .min_w(px(0.0))
                    .text_size(crate::theme::scaled_px(
                        10.0,
                        crate::theme::TextArea::Interface,
                    ))
                    .line_height(px(14.0))
                    .font_family(crate::theme::FONT_MONO)
                    .fg(theme.label_muted)
                    .truncate()
                    .tooltip(move |_window, cx| {
                        cx.new(|_| crate::ui::TextTooltip::new(theme, branch_tip.clone()))
                            .into()
                    })
                    .child(repo.current_branch.clone()),
            );

        let path_for_click = path.clone();
        let path_for_tip = path.clone();
        let path_link = div()
            .debug_selector(|| "copy-path".to_string())
            .id("copy-path")
            .flex()
            .items_center()
            .gap(px(4.0))
            .flex_1()
            .min_w(px(96.0))
            .cursor_pointer()
            .text_size(crate::theme::scaled_px(
                10.0,
                crate::theme::TextArea::Interface,
            ))
            .line_height(px(14.0))
            .font_family(crate::theme::FONT_MONO)
            .fg(theme.text_dim)
            .hover(move |s| s.fg(theme.text_app))
            .on_click(cx.listener(move |this, _e, _w, cx| {
                this.copy_local_path(path_for_click.clone(), cx);
            }))
            .tooltip(move |_window, cx| {
                cx.new(|_| crate::ui::TextTooltip::new(theme, path_for_tip.clone()))
                    .into()
            })
            .child(div().flex_1().min_w(px(0.0)).truncate().child(path.clone()))
            .child(icon(
                if copied { "check" } else { "copy" },
                IconSize::Xs,
                if copied {
                    theme.status_green
                } else {
                    theme.icon_idle
                },
            ));

        let meta = div()
            .debug_selector(|| "inspector-path".to_string())
            .flex()
            .items_center()
            .gap(px(8.0))
            .w_full()
            .min_w(px(0.0))
            .xs()
            .fg(theme.label_muted)
            .child(branch)
            .child(div().flex_shrink_0().child("·"))
            .child(path_link);

        let menu = div()
            .debug_selector(|| "repo-menu".to_string())
            .id("repo-menu")
            .flex()
            .items_center()
            .justify_center()
            .w(px(30.0))
            .h(px(30.0))
            .flex_shrink_0()
            .rounded(px(8.0))
            .cursor_pointer()
            .hover(move |s| s.bg(theme.nav_hover_bg))
            .child(icon("more-horizontal", IconSize::Sm, theme.icon_idle))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener({
                    let menu_repo = repo.id.clone();
                    let menu_name = repo.name.clone();
                    let uncommitted = repo.uncommitted_files.len();
                    let stashes = repo.stash_count;
                    move |this, event: &gpui::MouseDownEvent, _w, cx| {
                        this.open_repo_menu(
                            &menu_repo,
                            &menu_name,
                            uncommitted,
                            stashes,
                            false,
                            f32::from(event.position.x),
                            f32::from(event.position.y),
                            cx,
                        );
                    }
                }),
            );

        let close = crate::ui::icon_button(&theme, "x", "close-inspector", "Close details panel")
            .debug_selector(|| "close-inspector".to_string())
            .on_click(cx.listener(|this, _e, _w, cx| this.close_right_panel(cx)));
        let actions = self.render_action_buttons(repo, compact, cx);

        let header = div()
            .debug_selector(|| "inspector-header".to_string())
            .flex()
            .flex_col()
            .px(px(16.0))
            .py(px(10.0))
            .border_b_1()
            .border_color(theme.divider);

        if compact {
            // Two rows: identity above (with the menu and close always in the
            // corner), path and compact git actions below. Nothing can paint
            // under the action buttons because they own their own column.
            return header
                .gap(px(8.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .w_full()
                        .min_w(px(0.0))
                        .child(title)
                        .child(menu)
                        .child(close),
                )
                .child(
                    div()
                        .debug_selector(|| "inspector-actions".to_string())
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .w_full()
                        .min_w(px(0.0))
                        .child(meta.flex_1())
                        .child(actions),
                );
        }

        header
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(title)
                    .child(div().mt(px(4.0)).w_full().min_w(px(0.0)).child(meta)),
            )
            .child(
                div()
                    .debug_selector(|| "inspector-actions".to_string())
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .flex_shrink_0()
                    .child(menu)
                    .child(actions)
                    .child(close),
            )
    }

    fn render_group_dropdown(
        &self,
        repo: &crate::model::Repository,
        menu_open: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme;
        let repo_id = repo.id.clone();
        let current = repo.workspace.clone();
        let options = crate::app::group_choices_including(&self.groups, &current);

        let mut wrapper = div()
            .relative()
            .flex()
            .items_center()
            .flex_shrink_0()
            .child(
                div()
                    .id("group-dropdown")
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(4.0))
                    .h(px(HIT_TARGET_MIN))
                    .px(px(8.0))
                    .rounded(px(5.0))
                    .cursor_pointer()
                    .xs()
                    .fg(theme.text_meta)
                    .whitespace_nowrap()
                    .hover(move |s| s.bg(theme.nav_hover_bg).fg(theme.text_soft))
                    .on_click(cx.listener(|this, _e, _w, cx| {
                        this.group_menu_open = !this.group_menu_open;
                        cx.notify();
                    }))
                    .child(current.clone())
                    .child(icon("chevron-down", IconSize::Xs, theme.label_muted)),
            );

        if menu_open {
            let mut menu = div()
                .absolute()
                .top(px(HIT_TARGET_MIN + 2.0))
                .right(px(0.0))
                .min_w(px(160.0))
                .flex()
                .flex_col()
                .p(px(4.0))
                .bg(theme.panel)
                .border_1()
                .border_color(theme.divider)
                .rounded(px(8.0))
                .shadow_lg();
            for group in options {
                let group_id = repo_id.clone();
                let group_name = group.clone();
                let selected = group == current;
                menu = menu.child(
                    div()
                        .id(gpui::SharedString::from(format!("group-option-{}", group)))
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(8.0))
                        .px(px(8.0))
                        .py(px(8.0))
                        .rounded(px(4.0))
                        .cursor_pointer()
                        .xs()
                        .medium()
                        .fg(theme.text_soft)
                        .when(selected, |d| d.bg(theme.nav_active_bg))
                        .hover(move |s| s.bg(theme.row_hover_bg))
                        .on_click(cx.listener(move |this, _e, _w, cx| {
                            this.group_menu_open = false;
                            this.move_repo_group(&group_id, group_name.clone(), cx);
                        }))
                        .child(group.clone())
                        .when(selected, |d| {
                            d.child(icon("check", IconSize::Xs, theme.accent))
                        }),
                );
            }
            // Deferred so the menu paints above the body and its hitboxes win
            // over the controls it overlaps.
            wrapper = wrapper.child(deferred(menu));
        }

        wrapper
    }

    fn render_inspector_tabs(
        &self,
        repo: &crate::model::Repository,
        menu_open: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme;
        let details_state = self.details_state(&repo.id);
        let details_label = |name: &str, count: usize| match details_state {
            DetailsState::Loading => format!("{name} (…)"),
            DetailsState::Error => format!("{name} (!)"),
            DetailsState::Ready => format!("{name} ({count})"),
        };
        let tabs = [
            (
                InspectorTab::Changes,
                format!("Changes ({})", repo.uncommitted_files.len()),
            ),
            (
                InspectorTab::Commits,
                details_label("Commits", repo.recent_commits.len()),
            ),
            (
                InspectorTab::Branches,
                details_label("Branches", repo.branches.len()),
            ),
        ];

        let mut tab_row = div().flex().items_center().gap(px(24.0)).min_w(px(0.0));
        for (tab, label) in tabs {
            let active = self.inspector_tab == tab;
            tab_row = tab_row.child(
                crate::ui::tab(&theme, label, active, ("inspector-tab", tab as u8 as usize))
                    .on_click(cx.listener(move |this, _e, _w, cx| {
                        this.inspector_tab = tab;
                        cx.notify();
                    })),
            );
        }

        // The group selector lives here rather than in its own band: it is a
        // rare action, and this row already has the space for it.
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(16.0))
            .px(px(24.0))
            .pt(px(12.0))
            .border_b_1()
            .border_color(theme.divider)
            .child(tab_row)
            .child(self.render_group_dropdown(repo, menu_open, cx))
    }

    fn render_changes_tab(
        &self,
        repo: &crate::model::Repository,
        expanded: Option<&str>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme;

        if repo.uncommitted_files.is_empty() {
            return div()
                .flex()
                .justify_center()
                .py(px(48.0))
                .xs()
                .fg(theme.label_muted)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .child("Working tree is clean on")
                        .child(
                            div()
                                .mono_xs()
                                .fg(theme.text_body)
                                .child(format!("{}.", repo.current_branch)),
                        ),
                );
        }

        // One control for the whole list: ticked means every file is staged, so
        // a click either fills or clears the index in a single git call.
        let staged_count = self.ticked_count(repo);
        let total = repo.uncommitted_files.len();
        let all_staged = staged_count == total;
        let select_label = if all_staged {
            format!("All {total} staged")
        } else if staged_count == 0 {
            format!("Select all {total} files")
        } else {
            format!("{staged_count} of {total} staged")
        };
        let all_repo_id = repo.id.clone();

        let mut list = div()
            .flex()
            .flex_col()
            .w_full()
            .border_1()
            .border_color(theme.divider)
            .rounded(px(8.0))
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(10.0))
                    .w_full()
                    .px(px(14.0))
                    .py(px(4.0))
                    .bg(theme.table_head_bg)
                    .border_b_1()
                    .border_color(theme.row_border)
                    .on_mouse_down(
                        gpui::MouseButton::Right,
                        cx.listener({
                            let menu_repo = repo.id.clone();
                            let menu_name = repo.name.clone();
                            let uncommitted = repo.uncommitted_files.len();
                            let stashes = repo.stash_count;
                            move |this, event: &gpui::MouseDownEvent, _w, cx| {
                                this.open_repo_menu(
                                    &menu_repo,
                                    &menu_name,
                                    uncommitted,
                                    stashes,
                                    true,
                                    f32::from(event.position.x),
                                    f32::from(event.position.y),
                                    cx,
                                );
                            }
                        }),
                    )
                    .child(
                        checkbox(&theme, all_staged)
                            .id("stage-all")
                            .on_click(cx.listener(move |this, _e, _w, cx| {
                                this.toggle_all_staged(&all_repo_id, cx);
                            })),
                    )
                    .child(
                        div().flex_1().min_w(px(0.0)).child(
                            div()
                                .xs()
                                .medium()
                                .fg(theme.text_soft)
                                .whitespace_nowrap()
                                .child(select_label),
                        ),
                    )
                    .when(self.composer().amending, |d| {
                        d.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .flex_shrink_0()
                                .child(
                                    div()
                                        .xs()
                                        .medium()
                                        .fg(theme.accent_text)
                                        .whitespace_nowrap()
                                        .child("Amending the last commit"),
                                )
                                .child(
                                    div()
                                        .id("cancel-amend")
                                        .flex()
                                        .items_center()
                                        .h(px(HIT_TARGET_MIN))
                                        .px(px(8.0))
                                        .rounded(px(4.0))
                                        .cursor_pointer()
                                        .xs()
                                        .medium()
                                        .fg(theme.text_meta)
                                        .hover(move |s| s.fg(theme.status_red).underline())
                                        .child("Cancel")
                                        .on_click(cx.listener(|this, _e, _w, cx| {
                                            this.cancel_amend(cx);
                                        })),
                                ),
                        )
                    })
                    .child(
                        div()
                            .id("changes-menu")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(HIT_TARGET_MIN))
                            .h(px(HIT_TARGET_MIN))
                            .rounded(px(5.0))
                            .flex_shrink_0()
                            .cursor_pointer()
                            .hover(move |s| s.bg(theme.nav_hover_bg))
                            .tooltip(label_tooltip(theme, "Changes actions"))
                            .child(icon("more-horizontal", IconSize::Sm, theme.label_muted))
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener({
                                    let menu_repo = repo.id.clone();
                                    let menu_name = repo.name.clone();
                                    let uncommitted = repo.uncommitted_files.len();
                                    let stashes = repo.stash_count;
                                    move |this, event: &gpui::MouseDownEvent, _w, cx| {
                                        this.open_repo_menu(
                                            &menu_repo,
                                            &menu_name,
                                            uncommitted,
                                            stashes,
                                            true,
                                            f32::from(event.position.x),
                                            f32::from(event.position.y),
                                            cx,
                                        );
                                    }
                                }),
                            ),
                    ),
            );

        for (index, file) in repo.uncommitted_files.iter().enumerate() {
            let status_color = match file.status {
                crate::model::FileChangeStatus::Modified => theme.status_amber,
                crate::model::FileChangeStatus::Added => theme.add_text,
                crate::model::FileChangeStatus::Deleted => theme.del_text,
                crate::model::FileChangeStatus::Untracked => theme.nav_idle_text,
                crate::model::FileChangeStatus::Conflicted => theme.del_text,
            };
            let is_expanded = expanded == Some(file.path.as_str());
            let repo_id = repo.id.clone();
            let file_path = file.path.clone();
            let preview_repo_id = repo.id.clone();
            let preview_file = file.clone();

            let mut row = div()
                .flex()
                .flex_col()
                .w_full()
                .bg(theme.panel)
                .when(index > 0, |d| d.border_t_1().border_color(theme.row_border))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(12.0))
                        .px(px(14.0))
                        .py(px(4.0))
                        .xs()
                        .hover(move |s| s.bg(theme.row_hover_bg))
                        .on_mouse_down(
                            gpui::MouseButton::Right,
                            cx.listener({
                                let menu_repo = repo.id.clone();
                                let menu_path = file.path.clone();
                                move |this, event: &gpui::MouseDownEvent, _w, cx| {
                                    this.open_file_menu(
                                        &menu_repo,
                                        &menu_path,
                                        f32::from(event.position.x),
                                        f32::from(event.position.y),
                                        cx,
                                    );
                                }
                            }),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(10.0))
                                .min_w(px(0.0))
                                .child(
                                    checkbox(&theme, file.staged)
                                        .id(gpui::SharedString::from(format!(
                                            "stage-{}",
                                            file.path
                                        )))
                                        .tooltip(label_tooltip(
                                            theme,
                                            if file.staged {
                                                "Unstage file"
                                            } else {
                                                "Stage file"
                                            },
                                        ))
                                        .on_click(cx.listener(move |this, _e, _w, cx| {
                                            this.toggle_file_stage(&repo_id, &file_path, cx);
                                        })),
                                )
                                .child(
                                    div()
                                        .w(px(16.0))
                                        .flex_shrink_0()
                                        .text_center()
                                        .mono_xs()
                                        .semibold()
                                        .fg(status_color)
                                        .child(crate::model::file_status_code(file.status)),
                                )
                                .child({
                                    let path = file.path.clone();
                                    // File name first (what you look for), its
                                    // folder second (where it lives).
                                    let (directory, name) = split_path(&file.path);
                                    let mut label = div()
                                        .id(gpui::SharedString::from(format!(
                                            "file-toggle-{}",
                                            file.path
                                        )))
                                        .flex()
                                        .items_center()
                                        .gap(px(6.0))
                                        .flex_1()
                                        .min_w(px(0.0))
                                        .h(px(HIT_TARGET_MIN))
                                        .cursor_pointer()
                                        .hover(|s| s.underline())
                                        .on_click(cx.listener(move |this, _e, _w, cx| {
                                            let opening = this.expanded_file.as_deref()
                                                != Some(path.as_str());
                                            this.expanded_file =
                                                if opening { Some(path.clone()) } else { None };
                                            if opening {
                                                this.request_file_preview(
                                                    preview_repo_id.clone(),
                                                    &preview_file,
                                                    cx,
                                                );
                                            }
                                            cx.notify();
                                        }));
                                    if !directory.is_empty() {
                                        label = label.child(
                                            div()
                                                .flex_shrink()
                                                .min_w(px(0.0))
                                                .truncate()
                                                .mono_xs()
                                                .fg(theme.label_muted)
                                                .child(format!("{directory}/")),
                                        );
                                    }
                                    label.child(
                                        div()
                                            .flex_shrink()
                                            .min_w(px(0.0))
                                            .truncate()
                                            .xs()
                                            .medium()
                                            .fg(theme.text_app)
                                            .child(name),
                                    )
                                }),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(12.0))
                                .flex_shrink_0()
                                .font_family(crate::theme::FONT_MONO)
                                .text_size(crate::theme::scaled_px(
                                    crate::theme::TEXT_XS,
                                    crate::theme::TextArea::Interface,
                                ))
                                .line_height(crate::theme::LH_XS)
                                .child(
                                    div()
                                        .fg(theme.add_text)
                                        .child(format!("+{}", file.additions)),
                                )
                                .child(
                                    div()
                                        .fg(theme.del_text)
                                        .child(format!("-{}", file.deletions)),
                                ),
                        ),
                );

            if is_expanded && is_image_path(&file.path) {
                // Pictures get a picture: git has no lines to show for them.
                let full = crate::app::expand_home(&repo.local_path).join(&file.path);
                row = row.child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(6.0))
                        .w_full()
                        .px(px(14.0))
                        .py(px(12.0))
                        .bg(theme.diff_bg)
                        .border_t_1()
                        .border_color(theme.divider)
                        .child(
                            gpui::img(full)
                                .max_h(px(260.0))
                                .max_w(px(420.0))
                                .object_fit(gpui::ObjectFit::Contain),
                        )
                        .child(
                            div()
                                .xs()
                                .fg(theme.label_muted)
                                .child("Image changes have no line diff."),
                        ),
                );
            } else if is_expanded {
                let mut diff = div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .py(px(6.0))
                    .bg(theme.diff_bg)
                    .border_t_1()
                    .border_color(theme.divider)
                    .id(gpui::SharedString::from(format!("diff-{}", file.path)))
                    .overflow_x_scroll()
                    .mono_xs();

                let empty_message =
                    |theme: &crate::theme::Theme| {
                        div().px(px(12.0)).py(px(2.0)).fg(theme.text_meta).child(
                            match file.status {
                                crate::model::FileChangeStatus::Untracked => {
                                    "New file — nothing to diff against yet."
                                }
                                crate::model::FileChangeStatus::Conflicted => {
                                    "Conflicted file — resolve it in your editor to see a diff."
                                }
                                _ => "No textual changes.",
                            },
                        )
                    };

                if let Some(preview) = self.file_preview(&repo.id, file) {
                    if preview.error.is_some() {
                        diff = diff.child(
                            div()
                                .px(px(12.0))
                                .py(px(2.0))
                                .fg(theme.text_dim)
                                .child("Preview unavailable."),
                        );
                    } else if preview.lines.is_empty() {
                        diff = diff.child(empty_message(&theme));
                    } else {
                        for line in &preview.lines {
                            diff = diff.child(preview_row(&theme, line));
                        }
                        if preview.truncated {
                            diff = diff.child(
                                div()
                                    .px(px(10.0))
                                    .py(px(4.0))
                                    .xs()
                                    .fg(theme.label_muted)
                                    .child("Preview stops here — open the file for the rest."),
                            );
                        }
                    }
                } else if self.file_preview_pending(&repo.id, &file.path) {
                    diff = diff.child(
                        div()
                            .px(px(12.0))
                            .py(px(2.0))
                            .fg(theme.text_dim)
                            .child("Loading preview…"),
                    );
                } else if file.diff_preview.is_empty() {
                    diff = diff.child(empty_message(&theme));
                } else {
                    for line in &file.diff_preview {
                        diff = diff.child(preview_row(&theme, line));
                    }
                    if file.preview_truncated {
                        diff = diff.child(
                            div()
                                .px(px(10.0))
                                .py(px(4.0))
                                .xs()
                                .fg(theme.label_muted)
                                .child("Preview stops here — open the file for the rest."),
                        );
                    }
                }
                row = row.child(diff);
            }

            list = list.child(row);
        }

        list
    }

    /// Sticky commit footer: the message and its button.
    fn render_commit_bar(
        &self,
        repo: &crate::model::Repository,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme;
        let staged = self.ticked_count(repo);
        let running = self.repo_op(&repo.id);
        let committing = running == Some(RepoOp::Commit);
        let busy = running.is_some();
        let message_ready = !self.composer().message.value.trim().is_empty();
        let amending = self.composer().amending;
        // Amending can be message-only, so files are optional there.
        let can_commit = !busy && message_ready && (amending || staged > 0);

        let label = if committing || amending && busy {
            if amending {
                RepoOp::Amend.in_progress_label().to_string()
            } else {
                RepoOp::Commit.in_progress_label().to_string()
            }
        } else if amending {
            if staged == 1 {
                "Amend 1 file".to_string()
            } else if staged > 1 {
                format!("Amend {staged} files")
            } else {
                "Amend commit".to_string()
            }
        } else if staged == 0 {
            "Commit".to_string()
        } else if staged == 1 {
            "Commit 1 file".to_string()
        } else {
            format!("Commit {staged} files")
        };

        // Three faces: running, available, unavailable. The unavailable one is
        // a flat muted chip rather than a faded blue button.
        let commit_button: gpui::AnyElement = if committing {
            accent_button_with(
                &theme,
                label,
                Some(
                    spinner(
                        format!("commit-{}", repo.id),
                        "refresh-cw",
                        IconSize::Sm,
                        theme.primary_text,
                    )
                    .into_any_element(),
                ),
            )
            .id("commit-staged")
            .into_any_element()
        } else if can_commit {
            accent_button(&theme, label)
                .id("commit-staged")
                .on_click(cx.listener(|this, _e, _w, cx| this.commit_selected(cx)))
                .into_any_element()
        } else {
            disabled_button(&theme, label)
                .id("commit-staged")
                .into_any_element()
        };

        // Left of the message field: the account this commit will be
        // attributed to. Hovering the tile names that account. Until the
        // lazy identity read lands, show a spinner rather than a misleading
        // "identity missing" warning.
        let identity: gpui::AnyElement = if self.details_state(&repo.id) == DetailsState::Loading {
            div()
                .flex()
                .items_center()
                .justify_center()
                .w(px(crate::ui::AVATAR_SM))
                .h(px(crate::ui::AVATAR_SM))
                .flex_shrink_0()
                .child(spinner(
                    format!("commit-identity-{}", repo.id),
                    "refresh-cw",
                    IconSize::Xs,
                    theme.icon_idle,
                ))
                .into_any_element()
        } else {
            match &repo.author {
                Some(author) => {
                    let name = author.name.clone();
                    let email = author.email.clone();
                    crate::ui::avatar(
                        &theme,
                        &author.name,
                        &author.email,
                        self.avatar_for(&author.email),
                        crate::ui::AVATAR_SM,
                    )
                    .id("commit-author-avatar")
                    .tooltip(move |_window, cx| {
                        cx.new(|_| {
                            crate::ui::IdentityTooltip::new(theme, name.clone(), email.clone())
                        })
                        .into()
                    })
                    .into_any_element()
                }
                None => {
                    let warning = div()
                        .id("commit-identity-missing")
                        .flex()
                        .items_center()
                        .justify_center()
                        .w(px(crate::ui::AVATAR_SM))
                        .h(px(crate::ui::AVATAR_SM))
                        .rounded(px(crate::ui::AVATAR_SM / 2.0))
                        .flex_shrink_0()
                        .bg(crate::theme::alpha(theme.status_amber, 0.15))
                        .child(icon("alert-triangle", IconSize::Xs, theme.status_amber));
                    match self.details_error(&repo.id) {
                        Some(error) => {
                            let error = error.to_string();
                            warning
                                .tooltip(move |_window, cx| {
                                    cx.new(|_| {
                                        crate::ui::TextTooltip::new(
                                            theme,
                                            format!("Could not load repository details: {error}"),
                                        )
                                    })
                                    .into()
                                })
                                .into_any_element()
                        }
                        None => warning.into_any_element(),
                    }
                }
            }
        };

        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .w_full()
            .px(px(24.0))
            .py(px(10.0))
            .border_t_1()
            .border_color(theme.divider)
            .bg(theme.table_head_bg)
            // Message row: who commits, what it says, and the button.
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .w_full()
                    .child(identity)
                    .child(div().flex_1().min_w(px(0.0)).child(self.text_field(
                        crate::app::FieldId::CommitMessage,
                        "Commit message",
                        crate::ui::FieldVariant::Row,
                        window,
                        cx,
                    )))
                    .child(commit_button),
            )
            // Optional description, one quiet line under the summary.
            .child(div().flex().items_center().gap(px(10.0)).w_full().child(
                div().flex_1().min_w(px(0.0)).child(self.text_field(
                    crate::app::FieldId::CommitBody,
                    "Description (optional)",
                    crate::ui::FieldVariant::Row,
                    window,
                    cx,
                )),
            ))
    }

    fn render_undo_strip(
        &self,
        repo: &crate::model::Repository,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        let Some(offer) = self.undo_candidate(repo) else {
            return div().into_any_element();
        };
        // "Committed just now" for our own commit, otherwise how many local
        // commits the remote does not have yet.
        let headline = if offer.just_now {
            "Committed just now".to_string()
        } else if offer.unpushed == 1 {
            "1 commit not pushed".to_string()
        } else {
            format!("{} commits not pushed", offer.unpushed)
        };
        let summary = offer.summary.clone();
        let busy = self.repo_op(&repo.id).is_some();

        let mut undo = secondary_button(&theme, "Undo", None).id("undo-commit");
        if busy {
            undo = undo.opacity(0.5);
        } else {
            undo = undo.on_click(cx.listener(|this, _e, _w, cx| {
                let Some(repo) = this.selected_repo().cloned() else {
                    return;
                };
                this.undo_last_commit(&repo, cx);
            }));
        }

        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .w_full()
            .px(px(24.0))
            .py(px(10.0))
            .border_t_1()
            .border_color(theme.divider)
            .bg(theme.panel)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .min_w(px(0.0))
                    .child(div().xs().fg(theme.text_meta).child(headline))
                    .child(
                        div()
                            .xs()
                            .medium()
                            .fg(theme.text_app)
                            .truncate()
                            .child(summary),
                    ),
            )
            .child(undo)
            .into_any_element()
    }

    /// One commit, opened: a thin header, one scrolling row of files, and the
    /// patch filling whatever height is left.
    fn render_commit_detail(&self, view: &crate::app::CommitView, cx: &mut Context<Self>) -> Div {
        let theme = self.theme;

        let back = div()
            .id("commit-back")
            .flex()
            .items_center()
            .justify_center()
            .gap(px(4.0))
            .h(px(HIT_TARGET_MIN))
            .px(px(8.0))
            .rounded(px(5.0))
            .cursor_pointer()
            .xs()
            .medium()
            .fg(theme.text_soft)
            .hover(move |s| s.bg(theme.nav_hover_bg))
            .child(icon("chevron-right", IconSize::Xs, theme.label_muted))
            .child("Commits")
            .on_click(cx.listener(|this, _e, _w, cx| this.close_commit_view(cx)));

        let header = div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .w_full()
            .px(px(10.0))
            .py(px(8.0))
            .bg(theme.table_head_bg)
            .border_b_1()
            .border_color(theme.row_border)
            .child(back)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .xs()
                    .medium()
                    .fg(theme.text_app)
                    .child(view.subject.clone()),
            )
            .child(
                div()
                    .mono_xs()
                    .fg(theme.text_meta)
                    .flex_shrink_0()
                    .child(view.sha.clone()),
            )
            .when(!view.comparing, |d| {
                d.child(crate::ui::avatar(
                    &theme,
                    &view.author,
                    &view.author_email,
                    self.avatar_for(&view.author_email),
                    16.0,
                ))
                .child(
                    div()
                        .flex_shrink_0()
                        .xs()
                        .fg(theme.text_meta)
                        .whitespace_nowrap()
                        .child(view.time.clone()),
                )
            })
            .when(view.files.get(view.selected).is_some(), |d| {
                let file = &view.files[view.selected];
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .flex_shrink_0()
                        .mono_xs()
                        .child(
                            div()
                                .fg(theme.add_text)
                                .child(format!("+{}", file.additions)),
                        )
                        .child(
                            div()
                                .fg(theme.del_text)
                                .child(format!("-{}", file.deletions)),
                        ),
                )
            })
            .child(
                div()
                    .id("commit-wide")
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(HIT_TARGET_MIN))
                    .h(px(HIT_TARGET_MIN))
                    .rounded(px(5.0))
                    .cursor_pointer()
                    .flex_shrink_0()
                    .hover(move |s| s.bg(theme.nav_hover_bg))
                    .tooltip(label_tooltip(theme, "Toggle wide commit view"))
                    .child(icon("panel-right", IconSize::Sm, theme.label_muted))
                    .on_click(cx.listener(|this, _e, _w, cx| this.toggle_commit_wide(cx))),
            );

        // Files on the left, diff on the right: the diff keeps the room and the
        // list stays a thin, scrollable column.
        let mut files = div()
            .id("commit-files")
            .flex()
            .flex_col()
            .w(px(196.0))
            .flex_shrink_0()
            .min_h(px(0.0))
            .h_full()
            .overflow_y_scroll()
            .border_r_1()
            .border_color(theme.row_border);
        for (index, file) in view.files.iter().enumerate() {
            let selected = index == view.selected;
            let (_, name) = split_path(&file.path);
            // Same shell as a change row: status letter, then the file name.
            let added = file
                .lines
                .iter()
                .filter(|l| l.kind == crate::model::PreviewKind::Added)
                .count();
            let removed = file
                .lines
                .iter()
                .filter(|l| l.kind == crate::model::PreviewKind::Removed)
                .count();
            let (code, code_color) = if removed == 0 && added > 0 {
                ("A", theme.add_text)
            } else if added == 0 && removed > 0 {
                ("D", theme.del_text)
            } else {
                ("M", theme.status_amber)
            };
            let tooltip_path = file.path.clone();
            files = files.child(
                div()
                    .id(gpui::SharedString::from(format!("commit-file-{index}")))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .w_full()
                    .px(px(10.0))
                    .py(px(8.0))
                    .cursor_pointer()
                    .when(selected, |d| d.bg(theme.accent_soft))
                    .when(!selected, |d| d.hover(move |s| s.bg(theme.row_hover_bg)))
                    // No room for the folder: it appears on hover.
                    .tooltip(move |_window, cx| {
                        cx.new(|_| crate::ui::TextTooltip::new(theme, tooltip_path.clone()))
                            .into()
                    })
                    .on_click(cx.listener(move |this, _e, _w, cx| {
                        this.select_commit_file(index, cx);
                    }))
                    .child(
                        div()
                            .w(px(12.0))
                            .flex_shrink_0()
                            .text_center()
                            .mono_xs()
                            .semibold()
                            .fg(code_color)
                            .child(code),
                    )
                    .child(
                        div()
                            .flex_shrink()
                            .min_w(px(0.0))
                            .truncate()
                            .xs()
                            .medium()
                            .fg(theme.text_app)
                            .child(name),
                    ),
            );
        }

        // Diff for the selected file.
        let body = match view.files.get(view.selected) {
            Some(file) => {
                let mut diff = div().flex().flex_col().w_full().py(px(4.0)).mono_xs();
                for line in &file.lines {
                    diff = diff.child(preview_row(&theme, line));
                }
                if file.truncated {
                    diff = diff.child(
                        div()
                            .px(px(10.0))
                            .py(px(4.0))
                            .xs()
                            .fg(theme.label_muted)
                            .child("Preview stops here — open the file for the rest."),
                    );
                }
                diff
            }
            None if view.loading => {
                let label = if view.comparing {
                    "Loading the comparison…"
                } else {
                    "Loading the commit…"
                };
                div()
                    .py(px(24.0))
                    .w_full()
                    .flex()
                    .justify_center()
                    .xs()
                    .fg(theme.label_muted)
                    .child(label)
            }
            None if view.error.is_some() => {
                let label = if view.comparing {
                    "Could not load this comparison"
                } else {
                    "Could not load this commit"
                };
                let error = view.error.as_deref().unwrap_or_default();
                div()
                    .px(px(12.0))
                    .py(px(24.0))
                    .w_full()
                    .xs()
                    .fg(theme.status_red)
                    .child(format!("{label}: {error}"))
            }
            None => div()
                .py(px(24.0))
                .w_full()
                .flex()
                .justify_center()
                .xs()
                .fg(theme.label_muted)
                .child("This commit has no textual changes."),
        };

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .border_1()
            .border_color(theme.divider)
            .rounded(px(8.0))
            .overflow_hidden()
            .child(header)
            .child(
                div()
                    .flex()
                    .flex_1()
                    .w_full()
                    .min_h(px(0.0))
                    .when(!view.files.is_empty(), |d| d.child(files))
                    .child(
                        div()
                            .id("commit-diff")
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w(px(0.0))
                            .bg(theme.diff_bg)
                            .overflow_scroll()
                            .child(body),
                    ),
            )
    }

    fn render_details_notice(
        &self,
        title: impl Into<String>,
        error: Option<&str>,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = self.theme;
        let mut notice = div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(6.0))
            .py(px(48.0))
            .child(
                div()
                    .sm()
                    .medium()
                    .text_center()
                    .fg(theme.text_body)
                    .child(title.into()),
            );
        if let Some(error) = error {
            notice = notice
                .child(
                    div()
                        .max_w(px(360.0))
                        .text_center()
                        .xs()
                        .fg(theme.label_muted)
                        .child(error.to_string()),
                )
                .child(
                    div()
                        .id("retry-details")
                        .mt(px(4.0))
                        .cursor_pointer()
                        .xs()
                        .medium()
                        .fg(theme.accent_text)
                        .hover(move |style| style.underline())
                        .child("Retry")
                        .on_click(cx.listener(|this, _e, _w, cx| this.retry_details(cx))),
                );
        }
        notice
    }

    fn render_commits_tab(
        &self,
        repo: &crate::model::Repository,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let theme = self.theme;

        // One commit opened: it takes the tab, and the diff takes the height.
        if let Some(view) = self
            .commit_view
            .as_ref()
            .filter(|view| view.repo_id == repo.id)
        {
            return self.render_commit_detail(view, cx).into_any_element();
        }

        match self.details_state(&repo.id) {
            DetailsState::Loading => {
                return self
                    .render_details_notice("Loading commits…", None, cx)
                    .into_any_element();
            }
            DetailsState::Error => {
                return self
                    .render_details_notice(
                        "Could not load commits",
                        self.details_error(&repo.id),
                        cx,
                    )
                    .into_any_element();
            }
            DetailsState::Ready if repo.recent_commits.is_empty() => {
                return self
                    .render_details_notice("No commits yet.", None, cx)
                    .into_any_element();
            }
            DetailsState::Ready => {}
        }

        let panel_width = self.dashboard_layout(window).inspector_width;
        let message_width =
            (px(panel_width) - px(24.0 * 2.0 + 16.0 * 2.0 + 16.0 + 12.0)).max(px(40.0));
        let local_only_count = repo.local_only_commits.len();
        let local_only_prefix = repo.local_only_prefix_len();
        // The count is the whole set; the divider only follows the leading run,
        // because a diverged history can put an upstream commit between two
        // local ones. Badges on the rows mark any local commit below it.
        let divider_label = match local_only_count {
            0 => String::new(),
            1 => "1 commit not pushed".to_string(),
            count => format!("{count} commits not pushed"),
        };
        let mut list = div()
            .flex()
            .flex_col()
            .w_full()
            .border_1()
            .border_color(theme.divider)
            .rounded(px(8.0))
            .overflow_hidden();

        for (index, commit) in repo.recent_commits.iter().enumerate() {
            let short_sha: String = commit.sha.chars().take(7).collect();
            let message = crate::text::ellipsize(
                cx,
                &commit.message,
                crate::theme::FONT_SANS,
                crate::theme::W_MEDIUM,
                crate::theme::scale_size(crate::theme::TEXT_XS, crate::theme::TextArea::Commit),
                message_width,
            );

            let menu_repo = repo.id.clone();
            let menu_short = commit.sha.clone();
            let menu_subject = commit.message.clone();
            let menu_body = commit.body.clone();
            let can_amend = index == 0 && repo.commit_is_local_only(commit);
            list = list.child(
                div()
                    .id(gpui::SharedString::from(format!("commit-{}", commit.sha)))
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .px(px(16.0))
                    .py(px(12.0))
                    .w_full()
                    .bg(theme.panel)
                    .on_mouse_down(
                        gpui::MouseButton::Right,
                        cx.listener(move |this, event: &gpui::MouseDownEvent, _w, cx| {
                            this.open_commit_menu(
                                &menu_repo,
                                &menu_short,
                                &menu_subject,
                                &menu_body,
                                can_amend,
                                f32::from(event.position.x),
                                f32::from(event.position.y),
                                cx,
                            );
                        }),
                    )
                    .when(index > 0, |d| d.border_t_1().border_color(theme.row_border))
                    .cursor_pointer()
                    .hover(move |s| s.bg(theme.row_hover_bg))
                    .on_click(cx.listener({
                        let open_repo = repo.clone();
                        let open_commit = commit.clone();
                        move |this, _e, _w, cx| {
                            this.open_commit_view(&open_repo, &open_commit, cx);
                        }
                    }))
                    .child(
                        div()
                            .flex()
                            .justify_center()
                            .w(px(16.0))
                            .flex_shrink_0()
                            .child(icon("git-commit-horizontal", IconSize::Sm, theme.icon_idle)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w(px(0.0))
                            .gap(px(4.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap(px(12.0))
                                    .w_full()
                                    .min_w(px(0.0))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w(px(0.0))
                                            .commit_subject()
                                            .medium()
                                            .fg(theme.text_app)
                                            .whitespace_nowrap()
                                            .child(message),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap(px(12.0))
                                    .w_full()
                                    .min_w(px(0.0))
                                    .commit_xs()
                                    .fg(theme.text_meta)
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(6.0))
                                            .flex_1()
                                            .min_w(px(0.0))
                                            .child(crate::ui::avatar(
                                                &theme,
                                                &commit.author,
                                                &commit.author_email,
                                                self.avatar_for(&commit.author_email),
                                                16.0,
                                            ))
                                            .child(div().truncate().child(commit.author.clone()))
                                            .child(
                                                div()
                                                    .commit_mono_xs()
                                                    .flex_shrink_0()
                                                    .px(px(5.0))
                                                    .py(px(1.0))
                                                    .rounded(px(4.0))
                                                    .bg(theme.subtle_bg)
                                                    .fg(theme.label_muted)
                                                    .child(short_sha.clone()),
                                            ),
                                    )
                                    .child({
                                        // The badge exists only on commits the
                                        // remote does not have; the times stay
                                        // flush right either way, because this
                                        // cluster is right-aligned by the row.
                                        let mut trailing = div()
                                            .flex()
                                            .items_center()
                                            .gap(px(8.0))
                                            .flex_shrink_0();
                                        if repo.commit_is_local_only(commit) {
                                            // GitHub Desktop's marker: a quiet
                                            // round chip with an up arrow, sat
                                            // where the row's trailing controls
                                            // live.
                                            trailing = trailing.child(
                                                div()
                                                    .id(gpui::SharedString::from(format!(
                                                        "local-only-{index}"
                                                    )))
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .w(px(22.0))
                                                    .h(px(22.0))
                                                    .rounded(px(11.0))
                                                    .flex_shrink_0()
                                                    .bg(crate::theme::alpha(
                                                        theme.label_muted,
                                                        0.16,
                                                    ))
                                                    .tooltip(label_tooltip(
                                                        theme,
                                                        "Not pushed to remote",
                                                    ))
                                                    .child(icon(
                                                        "arrow-up",
                                                        IconSize::Xs,
                                                        theme.text_body,
                                                    )),
                                            );
                                        }
                                        trailing.child(
                                            div()
                                                .whitespace_nowrap()
                                                .child(commit.relative_time.clone()),
                                        )
                                    }),
                            ),
                    ),
            );

            // The divider belongs at the boundary between the leading local
            // run and the history the upstream already has. Only the leading
            // run positions it; every row after it keeps its own badge.
            if local_only_prefix > 0 && index + 1 == local_only_prefix {
                list = list.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .w_full()
                        .px(px(14.0))
                        .py(px(8.0))
                        .bg(theme.table_head_bg)
                        .border_t_1()
                        .border_color(theme.row_border)
                        .child(icon("arrow-up", IconSize::Xs, theme.status_blue))
                        .child(
                            div()
                                .commit_xs()
                                .medium()
                                .fg(theme.status_blue)
                                .child(divider_label.clone()),
                        ),
                );
            }
        }

        list.into_any_element()
    }

    /// The branch list: every branch row carries its upstream, divergence and
    /// the actions that used to live only behind a right-click.
    fn render_branches_tab(
        &self,
        repo: &crate::model::Repository,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        match self.details_state(&repo.id) {
            DetailsState::Loading => {
                return self
                    .render_details_notice("Loading branches…", None, cx)
                    .into_any_element();
            }
            DetailsState::Error => {
                return self
                    .render_details_notice(
                        "Could not load branches",
                        self.details_error(&repo.id),
                        cx,
                    )
                    .into_any_element();
            }
            DetailsState::Ready if repo.branches.is_empty() => {
                return self
                    .render_details_notice("No local branches yet.", None, cx)
                    .into_any_element();
            }
            DetailsState::Ready => {}
        }

        let theme = self.theme;

        if repo.branches.is_empty() {
            return div()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(6.0))
                .py(px(48.0))
                .child(icon("git-branch", IconSize::Xl, theme.text_dim))
                .child(
                    div()
                        .sm()
                        .medium()
                        .fg(theme.text_body)
                        .child("No branches yet"),
                )
                .child(
                    div()
                        .xs()
                        .fg(theme.label_muted)
                        .child("This repository gets its first branch when it has a commit."),
                )
                .into_any_element();
        }

        let mut list = div()
            .flex()
            .flex_col()
            .w_full()
            .border_1()
            .border_color(theme.divider)
            .rounded(px(8.0))
            .overflow_hidden();

        for (index, branch) in repo.branches.iter().enumerate() {
            let menu_repo = repo.id.clone();
            let menu_name = branch.name.clone();
            let menu_is_current = branch.is_current;
            let menu_is_default = branch.is_default;
            // Remote-tracking names are shown as `origin/name`; local branches
            // keep their own name. Nothing is rendered under the row.
            let display_name = branch
                .name
                .strip_prefix("remotes/")
                .map(str::to_string)
                .unwrap_or_else(|| branch.name.clone());
            let tooltip = branch_tooltip(branch);

            list = list.child(
                div()
                    .id(gpui::SharedString::from(format!(
                        "branch-row-{}",
                        branch.name
                    )))
                    .flex()
                    .flex_col()
                    .gap(px(9.0))
                    .px(px(16.0))
                    .py(px(12.0))
                    .w_full()
                    .bg(theme.panel)
                    .hover(move |s| s.bg(theme.row_hover_bg))
                    .tooltip(move |_window, cx| {
                        cx.new(|_| crate::ui::TextTooltip::new(theme, tooltip.clone()))
                            .into()
                    })
                    .on_mouse_down(
                        gpui::MouseButton::Right,
                        cx.listener(move |this, event: &gpui::MouseDownEvent, _w, cx| {
                            this.open_branch_menu(
                                &menu_repo,
                                &menu_name,
                                menu_is_current,
                                menu_is_default,
                                f32::from(event.position.x),
                                f32::from(event.position.y),
                                cx,
                            );
                        }),
                    )
                    .when(index > 0, |d| d.border_t_1().border_color(theme.row_border))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .min_w(px(0.0))
                            .child(icon("git-branch", IconSize::Sm, theme.icon_idle))
                            .child(
                                div()
                                    .mono_xs()
                                    .medium()
                                    .fg(if branch.is_current {
                                        theme.accent_text
                                    } else {
                                        theme.text_app
                                    })
                                    .truncate()
                                    .child(display_name),
                            ),
                    ),
            );
        }

        list.into_any_element()
    }

    /// The git actions for the repository, sized for the header band.
    ///
    /// In the compact header the buttons keep their icon and count but drop
    /// the words, so the path column beside them keeps enough room to identify
    /// the repository. The tooltip still spells out the action.
    fn render_action_buttons(
        &self,
        repo: &crate::model::Repository,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme;
        let running = self.repo_op(&repo.id);
        let busy = running.is_some();
        let can_pull = !busy && repo.has_remote && repo.has_upstream && repo.behind_count > 0;
        let can_push = !busy
            && repo.push_blocked_reason().is_none()
            && repo.has_remote
            && (repo.ahead_count > 0 || !repo.has_upstream);
        let can_fetch = !busy && repo.has_remote;
        let ahead = repo.ahead_count;
        let behind = repo.behind_count;
        // The buttons name what they would move: commits waiting locally for a
        // push, and commits on the remote waiting for a pull.
        let push_label = if compact {
            if ahead > 0 {
                ahead.to_string()
            } else {
                String::new()
            }
        } else if ahead > 0 {
            format!("Push {ahead}")
        } else {
            "Push".to_string()
        };
        let pull_label = if compact {
            if behind > 0 {
                behind.to_string()
            } else {
                String::new()
            }
        } else if behind > 0 {
            format!("Pull {behind}")
        } else {
            "Pull".to_string()
        };
        let push_tip = if ahead > 0 {
            format!("Push {ahead} commits to origin")
        } else {
            "Push to origin".to_string()
        };
        let pull_tip = if behind > 0 {
            format!("Pull {behind} commits from origin")
        } else {
            "Pull from origin".to_string()
        };
        let fetch_id = repo.id.clone();
        let gap = if compact { 4.0 } else { 8.0 };

        let spinner_leading = |theme: &Theme, kind: RepoOp, id: &str| {
            spinner(
                format!("{id}-{kind:?}"),
                "refresh-cw",
                IconSize::Sm,
                theme.secondary_text,
            )
            .into_any_element()
        };

        let progress_label = |kind: RepoOp| {
            if compact {
                String::new()
            } else {
                kind.in_progress_label().to_string()
            }
        };

        div()
            .flex()
            .items_center()
            .gap(px(gap))
            .flex_shrink_0()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(gap))
                    .flex_shrink_0()
                    .child(if running == Some(RepoOp::Pull) {
                        secondary_button_with(
                            &theme,
                            progress_label(RepoOp::Pull),
                            Some(spinner_leading(&theme, RepoOp::Pull, &repo.id)),
                        )
                        .debug_selector(|| "pull-selected".to_string())
                        .id("pull-selected")
                        .tooltip(move |_window, cx| {
                            cx.new(|_| crate::ui::TextTooltip::new(theme, "Pulling…"))
                                .into()
                        })
                        .into_any_element()
                    } else {
                        secondary_button(&theme, pull_label, Some("arrow-down"))
                            .debug_selector(|| "pull-selected".to_string())
                            .id("pull-selected")
                            .tooltip(move |_window, cx| {
                                cx.new(|_| crate::ui::TextTooltip::new(theme, pull_tip.clone()))
                                    .into()
                            })
                            .when(!can_pull, |d| d.opacity(0.5))
                            .when(can_pull, |d| {
                                d.on_click(cx.listener(|this, _e, _w, cx| this.pull_selected(cx)))
                            })
                            .into_any_element()
                    })
                    .child(if running == Some(RepoOp::Push) {
                        secondary_button_with(
                            &theme,
                            progress_label(RepoOp::Push),
                            Some(spinner_leading(&theme, RepoOp::Push, &repo.id)),
                        )
                        .debug_selector(|| "push-selected".to_string())
                        .id("push-selected")
                        .tooltip(move |_window, cx| {
                            cx.new(|_| crate::ui::TextTooltip::new(theme, "Pushing…"))
                                .into()
                        })
                        .into_any_element()
                    } else {
                        secondary_button(&theme, push_label, Some("arrow-up"))
                            .debug_selector(|| "push-selected".to_string())
                            .id("push-selected")
                            .tooltip(move |_window, cx| {
                                cx.new(|_| crate::ui::TextTooltip::new(theme, push_tip.clone()))
                                    .into()
                            })
                            .when(!can_push, |d| d.opacity(0.5))
                            .when(can_push, |d| {
                                d.on_click(cx.listener(|this, _e, _w, cx| this.push_selected(cx)))
                            })
                            .into_any_element()
                    })
                    .child(if running == Some(RepoOp::Fetch) {
                        secondary_button_with(
                            &theme,
                            progress_label(RepoOp::Fetch),
                            Some(spinner_leading(&theme, RepoOp::Fetch, &repo.id)),
                        )
                        .debug_selector(|| "fetch-origin".to_string())
                        .id("fetch-origin")
                        .tooltip(move |_window, cx| {
                            cx.new(|_| crate::ui::TextTooltip::new(theme, "Fetching…"))
                                .into()
                        })
                        .into_any_element()
                    } else {
                        secondary_button(
                            &theme,
                            if compact {
                                String::new()
                            } else {
                                "Fetch Origin".to_string()
                            },
                            Some("refresh-cw"),
                        )
                        .debug_selector(|| "fetch-origin".to_string())
                        .id("fetch-origin")
                        .tooltip(move |_window, cx| {
                            cx.new(|_| crate::ui::TextTooltip::new(theme, "Fetch origin"))
                                .into()
                        })
                        .when(!can_fetch, |d| d.opacity(0.5))
                        .when(can_fetch, |d| {
                            d.on_click(cx.listener(move |this, _e, _w, cx| {
                                let Some(repo) =
                                    this.repos.iter().find(|r| r.id == fetch_id).cloned()
                                else {
                                    return;
                                };
                                this.fetch_repo(&repo, cx);
                            }))
                        })
                        .into_any_element()
                    }),
            )
    }
}

/// Extensions git renders as "Binary files differ"; we show the picture itself.
pub(crate) fn is_image_path(path: &str) -> bool {
    matches!(
        path.rsplit_once('.').map(|(_, ext)| ext.to_ascii_lowercase()),
        Some(ext) if matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp")
    )
}

/// Repo-relative path split into `(folder, file name)` for the row label.
fn split_path(path: &str) -> (String, String) {
    match path.rsplit_once('/') {
        Some((directory, name)) => (directory.to_string(), name.to_string()),
        None => (String::new(), path.to_string()),
    }
}

/// One rendered patch line: gutter numbers, sign and content.
///
/// Colour carries the meaning (added / removed / hunk) and a faint tint marks
/// the changed rows, so the block reads the same in light and dark mode instead
/// of being a single dark slab.
fn preview_row(theme: &Theme, line: &crate::model::PreviewLine) -> Div {
    use crate::model::PreviewKind;

    let (sign, color, background) = match line.kind {
        PreviewKind::Added => (
            "+",
            theme.diff_add,
            Some(crate::theme::alpha(theme.diff_add, 0.12)),
        ),
        PreviewKind::Removed => (
            "-",
            theme.diff_del,
            Some(crate::theme::alpha(theme.diff_del, 0.12)),
        ),
        PreviewKind::Hunk => (
            "",
            theme.diff_hunk,
            Some(crate::theme::alpha(theme.diff_hunk, 0.10)),
        ),
        PreviewKind::Context => (" ", theme.text_body, None),
    };

    let mut row = div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .w_full()
        .px(px(10.0))
        .whitespace_nowrap()
        .flex_shrink_0();
    if let Some(background) = background {
        row = row.bg(background);
    }

    // A hunk header spans the row; it has no line on either side.
    if line.kind == PreviewKind::Hunk {
        return row.child(div().flex_1().fg(color).child(line.text.clone()));
    }

    let gutter = |number: Option<u32>| {
        div()
            .w(px(34.0))
            .flex_shrink_0()
            .text_right()
            .fg(theme.diff_meta)
            .child(number.map(|n| n.to_string()).unwrap_or_default())
    };

    row.child(gutter(line.old_line))
        .child(gutter(line.new_line))
        .child(
            div()
                .w(px(6.0))
                .flex_shrink_0()
                .text_center()
                .fg(color)
                .child(sign),
        )
        .child(div().flex_1().fg(color).child(line.text.clone()))
}

/// Extra branch information for the hover card. The row itself stays a single
/// branch name, while upstream, divergence and activity remain discoverable.
fn branch_tooltip(branch: &crate::model::BranchInfo) -> String {
    let mut lines = Vec::new();
    if let Some(upstream) = &branch.upstream {
        let state = match (branch.ahead, branch.behind) {
            (Some(0), Some(0)) => "up to date".to_string(),
            (Some(ahead), Some(behind)) => format!("{ahead} ahead, {behind} behind"),
            _ => "upstream gone".to_string(),
        };
        lines.push(format!("{upstream} · {state}"));
    } else {
        lines.push("No upstream".to_string());
    }
    if branch.is_current {
        lines.push("Current branch".to_string());
    }
    if branch.is_default {
        lines.push("Default branch".to_string());
    }
    if !branch.last_commit_time.is_empty() {
        lines.push(format!("Last commit {}", branch.last_commit_time));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use gpui::{px, size, AppContext, TestAppContext};

    use super::{shows_commit_bar, split_path};
    use crate::app::{InspectorTab, VantageApp};
    use crate::model::{RepoSyncState, Repository};
    use crate::root::RootView;

    #[test]
    fn split_path_keeps_the_file_name() {
        assert_eq!(
            split_path("src/views/inspector.rs"),
            ("src/views".to_string(), "inspector.rs".to_string())
        );
        assert_eq!(
            split_path("README.md"),
            (String::new(), "README.md".to_string())
        );
    }

    #[test]
    fn the_commit_bar_stays_visible_for_a_clean_tree_amend() {
        assert!(shows_commit_bar(InspectorTab::Changes, true, 0));
        assert!(shows_commit_bar(InspectorTab::Changes, false, 1));
        assert!(!shows_commit_bar(InspectorTab::Changes, false, 0));
        assert!(!shows_commit_bar(InspectorTab::Commits, true, 0));
    }

    #[gpui::test]
    fn long_path_stays_clear_of_the_header_actions(cx: &mut TestAppContext) {
        let app = cx.new(VantageApp::new);
        let (_root, cx) = cx.add_window_view(|_window, _cx| RootView::new(app.clone()));

        let long_path = format!(
            "/private/var/folders/{}/{}",
            "x".repeat(120),
            "vantage-repository"
        );
        app.update(cx, |app, _cx| {
            app.settings.right_panel_open = true;
            app.repos.push(Repository {
                id: "repo-1".to_string(),
                name: "vantage".to_string(),
                workspace: "Ungrouped".to_string(),
                local_path: long_path,
                head_sha: None,
                has_remote: true,
                stash_count: 0,
                current_branch: format!("feature/{}", "long-branch-".repeat(10)),
                author: None,
                sync_state: RepoSyncState::Ahead,
                ahead_count: 2,
                behind_count: 1,
                has_upstream: true,
                is_detached: false,
                last_activity_label: "just now".to_string(),
                last_activity_minutes: 0,
                uncommitted_files: Vec::new(),
                branches: Vec::new(),
                recent_commits: Vec::new(),
                local_only_commits: std::collections::HashSet::new(),
            });
            app.selected_repo_id = Some("repo-1".to_string());
        });

        for (width, height) in [(1040.0, 640.0), (1470.0, 900.0)] {
            cx.simulate_resize(size(px(width), px(height)));
            cx.run_until_parked();
            cx.update(|window, cx| {
                window.draw(cx).clear();
            });

            let header = cx
                .debug_bounds("inspector-header")
                .expect("inspector header bounds");
            let path = cx
                .debug_bounds("inspector-path")
                .expect("inspector path bounds");
            let actions = cx
                .debug_bounds("inspector-actions")
                .expect("inspector action bounds");
            assert!(
                path.left() >= header.left() && path.right() <= header.right(),
                "path {path:?} must stay inside the header {header:?}"
            );
            if actions.left() > path.left() + px(1.0) {
                assert!(
                    path.right() <= actions.left(),
                    "path {path:?} must stay left of the action cluster {actions:?}"
                );
            }
            assert!(
                actions.right() <= header.right(),
                "actions {actions:?} must stay inside the header {header:?}"
            );
        }
    }
}
