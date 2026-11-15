//! Right repository detail pane — half of the content area, changes / branches / commits.

use gpui::{deferred, div, prelude::*, px, relative, Context, Div, IntoElement, Styled, Window};

use crate::app::{InspectorTab, RepoOp, VantageApp};
use crate::theme::Theme;
use crate::ui::{
    accent_button, accent_button_with, checkbox, secondary_button, secondary_button_with, spinner,
    Txt,
};
use crate::ui::{icon, IconSize};

impl VantageApp {
    pub fn render_inspector(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        if !self.settings.right_panel_open {
            return None;
        }
        let Some(repo) = self.selected_repo().cloned() else {
            return Some(self.render_empty_detail_panel(cx));
        };
        let tab = self.inspector_tab;
        let expanded = self.expanded_file.clone();
        let copied = self.copied_path;
        let menu_open = self.group_menu_open;
        let theme = self.theme;

        let panel = div()
            .id("repository-detail")
            .flex()
            .flex_col()
            .w(relative(self.settings.right_panel_fraction))
            .min_w(px(420.0))
            .h_full()
            .flex_shrink_0()
            .bg(theme.panel)
            .border_l_1()
            .border_color(theme.divider)
            .child(self.render_inspector_header(&repo, copied, cx))
            .child(self.render_inspector_bar(&repo, cx))
            .child(self.render_inspector_tabs(&repo, menu_open, cx))
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .id("inspector-body")
                    .overflow_y_scroll()
                    .px(px(24.0))
                    .py(px(16.0))
                    .child(match tab {
                        InspectorTab::Changes => self
                            .render_changes_tab(&repo, expanded.as_deref(), cx)
                            .into_any_element(),
                        InspectorTab::Branches => {
                            self.render_branches_tab(&repo, cx).into_any_element()
                        }
                        InspectorTab::Commits => self
                            .render_commits_tab(&repo, window, cx)
                            .into_any_element(),
                    }),
            )
            // Pinned to the bottom of the pane, so a long file list never
            // scrolls the commit controls away. The undo strip sits directly
            // above them, while the commit it offers is still local.
            .when(
                tab == InspectorTab::Changes && self.undo_candidate(&repo).is_some(),
                |panel| panel.child(self.render_undo_strip(&repo, cx)),
            )
            .when(
                tab == InspectorTab::Changes && !repo.uncommitted_files.is_empty(),
                |panel| panel.child(self.render_commit_bar(&repo, window, cx)),
            );

        Some(panel.into_any_element())
    }

    fn render_empty_detail_panel(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme;
        div()
            .id("repository-detail-empty")
            .flex()
            .flex_col()
            .w(relative(self.settings.right_panel_fraction))
            .min_w(px(420.0))
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
                        crate::ui::icon_button(&theme, "x")
                            .id("close-right-panel")
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
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme;
        let path = repo.local_path.clone();

        div()
            .flex()
            .items_start()
            .justify_between()
            .gap(px(16.0))
            .px(px(24.0))
            .py(px(16.0))
            .border_b_1()
            .border_color(theme.divider)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .display_lg()
                                    .semibold()
                                    .fg(theme.text_app)
                                    .truncate()
                                    .child(repo.name.clone()),
                            )
                            .child(crate::ui::status_badges(
                                &theme,
                                repo.uncommitted_files.len(),
                                repo.ahead_count,
                                repo.behind_count,
                                repo.sync_state == crate::model::RepoSyncState::Conflict,
                            )),
                    )
                    .child(
                        div()
                            .mt(px(4.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .xs()
                            .fg(theme.label_muted)
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(5.0))
                                    .child(icon("git-branch", IconSize::Xs, theme.icon_idle))
                                    .child(
                                        div()
                                            .mono_xs()
                                            .medium()
                                            .fg(theme.text_soft)
                                            .whitespace_nowrap()
                                            .child(repo.current_branch.clone()),
                                    ),
                            )
                            .child(div().child("·"))
                            .child(
                                div()
                                    .id("copy-path")
                                    .flex()
                                    .items_center()
                                    .gap(px(4.0))
                                    .cursor_pointer()
                                    .mono_x11()
                                    .fg(theme.text_body)
                                    .hover(move |s| s.fg(theme.text_app))
                                    .on_click(cx.listener(move |this, _e, _w, cx| {
                                        this.copy_local_path(path.clone(), cx);
                                    }))
                                    .child(div().truncate().child(repo.local_path.clone()))
                                    .child(icon(
                                        if copied { "check" } else { "copy" },
                                        IconSize::Xs,
                                        if copied {
                                            theme.status_green
                                        } else {
                                            theme.icon_idle
                                        },
                                    )),
                            ),
                    ),
            )
            .child(
                crate::ui::icon_button(&theme, "x")
                    .id("close-inspector")
                    .on_click(cx.listener(|this, _e, _w, cx| this.close_right_panel(cx))),
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
        let groups = self.groups.clone();
        let current = repo.workspace.clone();
        let mut options = groups.clone();
        if !options.contains(&current) {
            options.push(current.clone());
        }

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
                    .gap(px(4.0))
                    .px(px(6.0))
                    .py(px(3.0))
                    .rounded(px(5.0))
                    .cursor_pointer()
                    .xs()
                    .fg(theme.label_muted)
                    .whitespace_nowrap()
                    .hover(move |s| s.bg(theme.nav_hover_bg).fg(theme.text_soft))
                    .on_click(cx.listener(|this, _e, _w, cx| {
                        this.group_menu_open = !this.group_menu_open;
                        cx.notify();
                    }))
                    .child(current.clone())
                    .child(icon("chevron-down", IconSize::Xs, theme.icon_idle)),
            );

        if menu_open {
            let mut menu = div()
                .absolute()
                .top(px(24.0))
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
                        .py(px(6.0))
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
        let tabs = [
            (
                InspectorTab::Changes,
                format!("Changes ({})", repo.uncommitted_files.len()),
            ),
            (
                InspectorTab::Branches,
                format!("Branches ({})", repo.branches.len()),
            ),
            (
                InspectorTab::Commits,
                format!("Commits ({})", repo.recent_commits.len()),
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
                    .gap(px(10.0))
                    .w_full()
                    .px(px(14.0))
                    .py(px(8.0))
                    .bg(theme.table_head_bg)
                    .border_b_1()
                    .border_color(theme.row_border)
                    .child(
                        checkbox(&theme, all_staged)
                            .id("stage-all")
                            .on_click(cx.listener(move |this, _e, _w, cx| {
                                this.toggle_all_staged(&all_repo_id, cx);
                            })),
                    )
                    .child(
                        div()
                            .xs()
                            .medium()
                            .fg(theme.text_soft)
                            .whitespace_nowrap()
                            .child(select_label),
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
                        .py(px(10.0))
                        .xs()
                        .hover(move |s| s.bg(theme.row_hover_bg))
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
                                        .on_click(cx.listener(move |this, _e, _w, cx| {
                                            this.toggle_file_stage(&repo_id, &file_path, cx);
                                        })),
                                )
                                .child(
                                    div()
                                        .w(px(16.0))
                                        .flex_shrink_0()
                                        .text_center()
                                        .font_family(crate::theme::FONT_MONO)
                                        .text_size(px(crate::theme::TEXT_XS))
                                        .line_height(crate::theme::LH_XS)
                                        .semibold()
                                        .fg(status_color)
                                        .child(crate::model::file_status_code(file.status)),
                                )
                                .child({
                                    let path = file.path.clone();
                                    div()
                                        .id(gpui::SharedString::from(format!(
                                            "file-toggle-{}",
                                            file.path
                                        )))
                                        .mono_xs()
                                        .fg(theme.text_soft)
                                        .truncate()
                                        .cursor_pointer()
                                        .hover(|s| s.underline())
                                        .on_click(cx.listener(move |this, _e, _w, cx| {
                                            this.expanded_file = if this.expanded_file.as_deref()
                                                == Some(path.as_str())
                                            {
                                                None
                                            } else {
                                                Some(path.clone())
                                            };
                                            cx.notify();
                                        }))
                                        .child(file.path.clone())
                                }),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(12.0))
                                .flex_shrink_0()
                                .font_family(crate::theme::FONT_MONO)
                                .text_size(px(crate::theme::TEXT_XS))
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

            if is_expanded {
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
                    .font_family(crate::theme::FONT_MONO)
                    .text_size(px(11.0))
                    .line_height(px(16.0));

                if file.diff_preview.is_empty() {
                    diff = diff.child(div().px(px(12.0)).py(px(2.0)).fg(theme.text_dim).child(
                        match file.status {
                            crate::model::FileChangeStatus::Untracked => {
                                "New file — nothing to diff against yet."
                            }
                            crate::model::FileChangeStatus::Conflicted => {
                                "Conflicted file — resolve it in your editor to see a diff."
                            }
                            _ => "No textual changes.",
                        },
                    ));
                } else {
                    for line in &file.diff_preview {
                        diff = diff.child(preview_row(&theme, line));
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
        let message_ready = !self.commit_field.value.trim().is_empty();
        let can_commit = !busy && staged > 0 && message_ready;

        let label = if committing {
            RepoOp::Commit.in_progress_label().to_string()
        } else if staged == 1 {
            "Commit 1 file".to_string()
        } else {
            format!("Commit {staged} files")
        };

        let mut commit_button = if committing {
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
        } else {
            accent_button(&theme, label).id("commit-staged")
        };
        if can_commit {
            commit_button =
                commit_button.on_click(cx.listener(|this, _e, _w, cx| this.commit_selected(cx)));
        } else {
            commit_button = commit_button.opacity(0.5);
        }

        // Left of the message field: the account this commit will be
        // attributed to. Hovering the tile names that account.
        let identity: gpui::AnyElement = match &repo.author {
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
                    cx.new(|_| crate::ui::IdentityTooltip::new(theme, name.clone(), email.clone()))
                        .into()
                })
                .into_any_element()
            }
            None => div()
                .flex()
                .items_center()
                .justify_center()
                .w(px(crate::ui::AVATAR_SM))
                .h(px(crate::ui::AVATAR_SM))
                .rounded(px(crate::ui::AVATAR_SM / 2.0))
                .flex_shrink_0()
                .bg(crate::theme::alpha(theme.status_amber, 0.15))
                .child(icon("alert-triangle", IconSize::Xs, theme.status_amber))
                .into_any_element(),
        };

        div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .w_full()
            .px(px(24.0))
            .py(px(12.0))
            .border_t_1()
            .border_color(theme.divider)
            .bg(theme.table_head_bg)
            .child(identity)
            .child(div().flex_1().min_w(px(0.0)).child(self.text_field(
                crate::app::FieldId::CommitMessage,
                "Commit message",
                crate::ui::FieldVariant::Row,
                window,
                cx,
            )))
            .child(commit_button)
    }

    /// "Committed just now · summary [Undo]" strip, for a commit that is still
    /// local. It disappears as soon as the tip moves away from that commit.
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
                    .child(div().xs().fg(theme.label_muted).child(headline))
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

    fn render_branches_tab(
        &self,
        repo: &crate::model::Repository,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme;
        let mut list = div()
            .flex()
            .flex_col()
            .w_full()
            .border_1()
            .border_color(theme.divider)
            .rounded(px(8.0))
            .overflow_hidden();

        for (index, branch) in repo.branches.iter().enumerate() {
            let repo_id = repo.id.clone();
            let branch_name = branch.name.clone();

            let mut right = div()
                .flex()
                .items_center()
                .gap(px(12.0))
                .flex_shrink_0()
                .child(
                    div()
                        .mono_xs()
                        .fg(theme.label_muted)
                        .whitespace_nowrap()
                        .child(format!(
                            "\u{2191}{} \u{2193}{}",
                            branch.ahead, branch.behind
                        )),
                );

            if !branch.is_current {
                right = right.child(
                    div()
                        .id(gpui::SharedString::from(format!(
                            "checkout-{}",
                            branch.name
                        )))
                        .px(px(10.0))
                        .py(px(4.0))
                        .rounded(px(6.0))
                        .border_1()
                        .border_color(theme.secondary_border)
                        .cursor_pointer()
                        .whitespace_nowrap()
                        .xs()
                        .medium()
                        .fg(theme.secondary_text)
                        .hover(move |s| s.bg(theme.secondary_bg_hover))
                        .on_click(cx.listener(move |this, _e, _w, cx| {
                            this.switch_branch(&repo_id, &branch_name, cx);
                        }))
                        .child("Checkout"),
                );
            }

            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(16.0))
                    .px(px(16.0))
                    .py(px(12.0))
                    .w_full()
                    .bg(theme.panel)
                    .when(index > 0, |d| d.border_t_1().border_color(theme.row_border))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w(px(0.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(icon("git-branch", IconSize::Sm, theme.icon_idle))
                                    .child(
                                        div()
                                            .mono_xs()
                                            .medium()
                                            .fg(theme.text_app)
                                            .truncate()
                                            .child(branch.name.clone()),
                                    )
                                    .when(branch.is_current, |d| {
                                        d.child(
                                            div()
                                                .xs()
                                                .medium()
                                                .fg(theme.accent_text)
                                                .whitespace_nowrap()
                                                .child("\u{00b7} Current"),
                                        )
                                    })
                                    .when(branch.is_default, |d| {
                                        d.child(
                                            div()
                                                .xs()
                                                .fg(theme.label_muted)
                                                .whitespace_nowrap()
                                                .child("\u{00b7} Default"),
                                        )
                                    }),
                            )
                            .child(
                                div()
                                    .mt(px(4.0))
                                    .w_full()
                                    .xs()
                                    .fg(theme.label_muted)
                                    .truncate()
                                    .child(format!(
                                        "{} \u{00b7} {}",
                                        branch.last_commit_msg, branch.last_commit_time
                                    )),
                            ),
                    )
                    .child(right),
            );
        }

        list
    }

    fn render_commits_tab(
        &self,
        repo: &crate::model::Repository,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme;
        let sidebar_width = if self.sidebar_collapsed { 56.0 } else { 256.0 };
        let content_width = f32::from(window.viewport_size().width) - sidebar_width;
        let panel_width = (content_width * self.settings.right_panel_fraction).max(420.0);
        let message_width =
            (px(panel_width) - px(24.0 * 2.0 + 16.0 * 2.0 + 16.0 + 12.0)).max(px(40.0));
        let unpushed = repo.ahead_count.max(0) as usize;
        let mut list = div()
            .flex()
            .flex_col()
            .w_full()
            .border_1()
            .border_color(theme.divider)
            .rounded(px(8.0))
            .overflow_hidden();

        // Names the badges below: everything above this line is still local.
        if repo.has_remote && unpushed > 0 {
            let label = if unpushed == 1 {
                "1 commit not pushed".to_string()
            } else {
                format!("{unpushed} commits not pushed")
            };
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .w_full()
                    .px(px(14.0))
                    .py(px(8.0))
                    .bg(theme.table_head_bg)
                    .border_b_1()
                    .border_color(theme.row_border)
                    .child(icon("arrow-up", IconSize::Xs, theme.status_blue))
                    .child(div().xs().medium().fg(theme.status_blue).child(label)),
            );
        }

        for (index, commit) in repo.recent_commits.iter().enumerate() {
            let short_sha: String = commit.sha.chars().take(7).collect();
            let message = crate::text::ellipsize(
                cx,
                &commit.message,
                crate::theme::FONT_SANS,
                crate::theme::W_MEDIUM,
                crate::theme::TEXT_XS,
                message_width,
            );

            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .px(px(16.0))
                    .py(px(10.0))
                    .w_full()
                    .bg(theme.panel)
                    .when(index > 0, |d| d.border_t_1().border_color(theme.row_border))
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
                            .gap(px(2.0))
                            .child(
                                div()
                                    .xs()
                                    .medium()
                                    .fg(theme.text_app)
                                    .whitespace_nowrap()
                                    .child(message),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap(px(12.0))
                                    .w_full()
                                    .min_w(px(0.0))
                                    .xs()
                                    .fg(theme.label_muted)
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(6.0))
                                            .flex_1()
                                            .min_w(px(0.0))
                                            .child(
                                                div()
                                                    .font_family(crate::theme::FONT_MONO)
                                                    .flex_shrink_0()
                                                    .child(short_sha),
                                            )
                                            .child(div().flex_shrink_0().child("·"))
                                            .child(div().truncate().child(commit.author.clone())),
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
                                        if repo.commit_is_local_only(index) {
                                            // GitHub Desktop's marker: a quiet
                                            // round chip with an up arrow, sat
                                            // where the row's trailing controls
                                            // live.
                                            trailing = trailing.child(
                                                div()
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
        }

        list
    }

    /// Toolbar under the identity block: the three git actions for this
    /// repository. Branch, counts and tree status live in the header above, so
    /// nothing here repeats them.
    fn render_inspector_bar(
        &self,
        repo: &crate::model::Repository,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme;
        let running = self.repo_op(&repo.id);
        let busy = running.is_some();
        let can_pull = !busy && repo.has_remote && repo.has_upstream && repo.behind_count > 0;
        let can_push = !busy && repo.has_remote && (repo.ahead_count > 0 || !repo.has_upstream);
        let can_fetch = !busy && repo.has_remote;
        let ahead = repo.ahead_count;
        let behind = repo.behind_count;
        // The buttons name what they would move: commits waiting locally for a
        // push, and commits on the remote waiting for a pull.
        let push_label = if ahead > 0 {
            format!("Push {ahead}")
        } else {
            "Push".to_string()
        };
        let pull_label = if behind > 0 {
            format!("Pull {behind}")
        } else {
            "Pull".to_string()
        };
        let fetch_id = repo.id.clone();

        let spinner_leading = |theme: &Theme, kind: RepoOp, id: &str| {
            spinner(
                format!("{id}-{kind:?}"),
                "refresh-cw",
                IconSize::Sm,
                theme.secondary_text,
            )
            .into_any_element()
        };

        div()
            .flex()
            .items_center()
            .justify_end()
            .flex_wrap()
            .gap(px(8.0))
            .px(px(24.0))
            .py(px(8.0))
            .border_b_1()
            .border_color(theme.divider)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .flex_shrink_0()
                    .child(if running == Some(RepoOp::Pull) {
                        secondary_button_with(
                            &theme,
                            RepoOp::Pull.in_progress_label(),
                            Some(spinner_leading(&theme, RepoOp::Pull, &repo.id)),
                        )
                        .id("pull-selected")
                        .into_any_element()
                    } else {
                        secondary_button(&theme, pull_label, Some("arrow-down"))
                            .id("pull-selected")
                            .when(!can_pull, |d| d.opacity(0.5))
                            .when(can_pull, |d| {
                                d.on_click(cx.listener(|this, _e, _w, cx| this.pull_selected(cx)))
                            })
                            .into_any_element()
                    })
                    .child(if running == Some(RepoOp::Push) {
                        secondary_button_with(
                            &theme,
                            RepoOp::Push.in_progress_label(),
                            Some(spinner_leading(&theme, RepoOp::Push, &repo.id)),
                        )
                        .id("push-selected")
                        .into_any_element()
                    } else {
                        secondary_button(&theme, push_label, Some("arrow-up"))
                            .id("push-selected")
                            .when(!can_push, |d| d.opacity(0.5))
                            .when(can_push, |d| {
                                d.on_click(cx.listener(|this, _e, _w, cx| this.push_selected(cx)))
                            })
                            .into_any_element()
                    })
                    .child(if running == Some(RepoOp::Fetch) {
                        secondary_button_with(
                            &theme,
                            RepoOp::Fetch.in_progress_label(),
                            Some(spinner_leading(&theme, RepoOp::Fetch, &repo.id)),
                        )
                        .id("fetch-origin")
                        .into_any_element()
                    } else {
                        secondary_button(&theme, "Fetch Origin", Some("refresh-cw"))
                            .id("fetch-origin")
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
