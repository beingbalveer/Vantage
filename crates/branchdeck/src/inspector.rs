//! Right slide-over inspector — 576px panel, changes / branches / commits.

use gpui::{deferred, div, prelude::*, px, relative, rgba, Context, IntoElement, Styled};

use crate::app::{BranchDeckApp, InspectorTab};
use crate::icons::icon;
use crate::theme::{alpha, Theme};
use crate::widgets::{checkbox, primary_button, secondary_button, status_text, Txt};

impl BranchDeckApp {
    pub fn render_inspector(&mut self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let repo = self.selected_repo()?.clone();
        let theme = self.theme;
        let tab = self.inspector_tab;
        let expanded = self.expanded_file.clone();
        let copied = self.copied_path;
        let menu_open = self.group_menu_open;

        let backdrop = div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .w(relative(1.0))
            .h(relative(1.0))
            .bg(theme.overlay)
            .id("inspector-backdrop")
            .on_click(cx.listener(|this, _e, _w, cx| this.close_inspector(cx)));

        let panel = div()
            // Keep clicks on the panel's labels and padding from reaching the
            // full-screen backdrop, which closes the inspector.
            .occlude()
            .relative()
            .flex()
            .flex_col()
            .w(px(576.0))
            .h_full()
            .bg(theme.panel)
            .border_l_1()
            .border_color(theme.divider)
            .shadow_xl();

        let panel = panel
            .child(self.render_inspector_header(&repo, copied, cx))
            .child(self.render_inspector_meta(&repo, menu_open, cx))
            .child(self.render_inspector_tabs(&repo, cx))
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
                        InspectorTab::Commits => self.render_commits_tab(&repo).into_any_element(),
                    }),
            )
            .child(self.render_inspector_footer(&repo, cx));

        if menu_open {
            // The menu lives in the meta bar; nothing extra to layer here.
        }

        let overlay = div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .w(relative(1.0))
            .h(relative(1.0))
            .flex()
            .justify_end()
            .child(backdrop)
            .child(panel);

        Some(overlay.into_any_element())
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
                            .child(status_text(
                                &theme,
                                repo.sync_state,
                                repo.uncommitted_files.len(),
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
                            .child(repo.primary_language.clone())
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
                                        12.0,
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
                div()
                    .id("close-inspector")
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .w(px(32.0))
                    .h(px(32.0))
                    .rounded(px(8.0))
                    .cursor_pointer()
                    .hover(move |s| s.bg(theme.nav_hover_bg))
                    .on_click(cx.listener(|this, _e, _w, cx| this.close_inspector(cx)))
                    .child(icon("x", 16.0, theme.label_muted)),
            )
    }

    fn render_inspector_meta(
        &self,
        repo: &crate::model::Repository,
        menu_open: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme;
        let ahead_color = if repo.ahead_count > 0 {
            theme.accent_text
        } else {
            theme.label_muted
        };
        let behind_color = if repo.behind_count > 0 {
            theme.status_red
        } else {
            theme.label_muted
        };

        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .flex_wrap()
            .px(px(24.0))
            .py(px(12.0))
            .bg(theme.table_head_bg)
            .border_b_1()
            .border_color(theme.divider)
            .xs()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .mono_xs()
                    .fg(theme.text_soft)
                    .child(icon("git-branch", 14.0, theme.icon_idle))
                    .child(repo.current_branch.clone()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(16.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.0))
                            .mono_xs()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(2.0))
                                    .fg(ahead_color)
                                    .when(repo.ahead_count > 0, |d| d.medium())
                                    .child(icon("arrow-up", 12.0, ahead_color))
                                    .child(repo.ahead_count.to_string()),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(2.0))
                                    .fg(behind_color)
                                    .when(repo.behind_count > 0, |d| d.medium())
                                    .child(icon("arrow-down", 12.0, behind_color))
                                    .child(repo.behind_count.to_string()),
                            ),
                    )
                    .child(self.render_group_dropdown(repo, menu_open, cx)),
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
            .gap(px(6.0))
            .flex_shrink_0()
            .child(div().xs().fg(theme.label_muted).child("Group"))
            .child(
                div()
                    .id("group-dropdown")
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .py(px(4.0))
                    .px(px(8.0))
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(theme.secondary_border)
                    .bg(theme.panel)
                    .cursor_pointer()
                    .xs()
                    .medium()
                    .fg(theme.text_soft)
                    .whitespace_nowrap()
                    .on_click(cx.listener(|this, _e, _w, cx| {
                        this.group_menu_open = !this.group_menu_open;
                        cx.notify();
                    }))
                    .child(current.clone())
                    .child(icon("chevron-down", 12.0, theme.icon_idle)),
            );

        if menu_open {
            let mut menu = div()
                .absolute()
                .top(px(30.0))
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
                        .when(selected, |d| d.child(icon("check", 12.0, theme.accent))),
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

        let mut row = div()
            .flex()
            .items_center()
            .gap(px(24.0))
            .px(px(24.0))
            .pt(px(12.0))
            .border_b_1()
            .border_color(theme.divider);

        for (tab, label) in tabs {
            let active = self.inspector_tab == tab;
            row = row.child(
                div()
                    .id(("inspector-tab", tab as u8 as usize))
                    .pb(px(10.0))
                    .border_b_2()
                    .border_color(if active {
                        theme.accent
                    } else {
                        rgba(0x00000000)
                    })
                    .cursor_pointer()
                    .whitespace_nowrap()
                    .flex_shrink_0()
                    .xs()
                    .medium()
                    .fg(if active {
                        theme.text_app
                    } else {
                        theme.label_muted
                    })
                    .when(!active, move |d| d.hover(move |s| s.fg(theme.text_soft)))
                    .on_click(cx.listener(move |this, _e, _w, cx| {
                        this.inspector_tab = tab;
                        cx.notify();
                    }))
                    .child(label),
            );
        }
        row
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

        let mut list = div()
            .flex()
            .flex_col()
            .w_full()
            .border_1()
            .border_color(theme.divider)
            .rounded(px(8.0))
            .overflow_hidden();

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
                    .px(px(14.0))
                    .py(px(10.0))
                    .bg(theme.diff_bg)
                    .border_t_1()
                    .border_color(theme.divider)
                    .id(gpui::SharedString::from(format!("diff-{}", file.path)))
                    .overflow_x_scroll()
                    .font_family(crate::theme::FONT_MONO)
                    .text_size(px(11.0))
                    .line_height(px(16.0));

                if file.diff_preview.is_empty() {
                    diff = diff.child(div().fg(theme.text_dim).child("No diff lines."));
                } else {
                    for line in &file.diff_preview {
                        let color = if line.starts_with('+') {
                            theme.diff_add
                        } else if line.starts_with('-') {
                            theme.diff_del
                        } else if line.starts_with("@@") {
                            theme.diff_hunk
                        } else {
                            theme.diff_meta
                        };
                        diff = diff.child(
                            div()
                                .flex_shrink_0()
                                .whitespace_nowrap()
                                .fg(color)
                                .child(line.clone()),
                        );
                    }
                }
                row = row.child(diff);
            }

            list = list.child(row);
        }

        list
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
                                    .child(icon("git-branch", 14.0, theme.icon_idle))
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

    fn render_commits_tab(&self, repo: &crate::model::Repository) -> impl IntoElement {
        let theme = self.theme;
        let mut list = div()
            .flex()
            .flex_col()
            .w_full()
            .border_1()
            .border_color(theme.divider)
            .rounded(px(8.0))
            .overflow_hidden();

        for (index, commit) in repo.recent_commits.iter().enumerate() {
            list = list.child(
                div()
                    .flex()
                    .items_start()
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
                                    .child(icon("git-commit-horizontal", 14.0, theme.icon_idle))
                                    .child(
                                        div()
                                            .xs()
                                            .medium()
                                            .fg(theme.text_app)
                                            .child(commit.message.clone()),
                                    ),
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
                                            .font_family(crate::theme::FONT_MONO)
                                            .child(commit.sha.clone()),
                                    )
                                    .child(div().child("·"))
                                    .child(div().child(commit.author.clone()))
                                    .child(div().child("·"))
                                    .child(
                                        div()
                                            .font_family(crate::theme::FONT_MONO)
                                            .child(commit.branch.clone()),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .xs()
                            .fg(theme.label_muted)
                            .whitespace_nowrap()
                            .flex_shrink_0()
                            .child(commit.relative_time.clone()),
                    ),
            );
        }

        list
    }

    fn render_inspector_footer(
        &self,
        repo: &crate::model::Repository,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme;

        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .flex_shrink_0()
            .px(px(24.0))
            .py(px(14.0))
            .border_t_1()
            .border_color(theme.divider)
            .bg(theme.table_head_bg)
            .child(
                div()
                    .w(px(212.0))
                    .flex_shrink_0()
                    .mono_xs()
                    .fg(theme.label_muted)
                    .whitespace_nowrap()
                    .child(crate::text::ellipsize(
                        cx,
                        &repo.remote_repo,
                        crate::theme::FONT_MONO,
                        crate::theme::W_REGULAR,
                        crate::theme::TEXT_XS,
                        px(212.0),
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .flex_shrink_0()
                    .child(
                        secondary_button(&theme, "Fetch Origin", Some("refresh-cw"))
                            .id("fetch-origin")
                            .on_click(cx.listener(|this, _e, _w, cx| {
                                let Some(repo) = this.selected_repo().cloned() else {
                                    return;
                                };
                                this.fetch_repo(&repo, cx);
                            })),
                    )
                    .child(
                        primary_button(&theme, "Open in GitHub Desktop", Some("external-link"))
                            .id("open-desktop")
                            .on_click(cx.listener(|this, _e, _w, cx| {
                                let Some(repo) = this.selected_repo().cloned() else {
                                    return;
                                };
                                this.open_in_desktop(&repo, cx);
                            })),
                    ),
            )
    }
}

#[allow(dead_code)]
fn _unused(theme: &Theme) -> gpui::Rgba {
    alpha(theme.panel, 1.0)
}
