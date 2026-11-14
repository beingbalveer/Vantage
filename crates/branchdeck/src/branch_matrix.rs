//! Branch Matrix — filter tabs, search field, six-column table.

use gpui::{div, prelude::*, px, relative, Context, Div, IntoElement, Styled, Window};

use crate::app::{BranchDeckApp, BranchFilter, FieldId};
use crate::icons::icon;
use crate::theme::Theme;
use crate::widgets::{FieldVariant, Txt};

const COL_BRANCH: f32 = 0.185;
const COL_REPO: f32 = 0.19;
const COL_STATE: f32 = 0.135;
const COL_SYNC: f32 = 0.105;
const COL_UPDATED: f32 = 0.08;
const COL_LATEST_MAX: f32 = 320.0;

struct BranchRow {
    repo_id: String,
    repo_name: String,
    workspace: String,
    uncommitted: usize,
    name: String,
    is_current: bool,
    is_default: bool,
    ahead: i32,
    behind: i32,
    last_commit_msg: String,
    last_commit_time: String,
}

fn cell(width: f32) -> Div {
    div().w(relative(width)).flex_shrink_0()
}

impl BranchDeckApp {
    pub fn render_branch_matrix(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme;

        let container_width = crate::text::container_width(window.viewport_size().width);
        let latest_width =
            crate::text::column_width(container_width, 0.305, 16.0).min(px(COL_LATEST_MAX));
        let branch_width =
            (crate::text::column_width(container_width, COL_BRANCH, 16.0) - px(24.0)).max(px(24.0));

        let mut rows: Vec<BranchRow> = Vec::new();
        for repo in self.grouped_repos() {
            for branch in &repo.branches {
                rows.push(BranchRow {
                    repo_id: repo.id.clone(),
                    repo_name: repo.name.clone(),
                    workspace: repo.workspace.clone(),
                    uncommitted: if branch.is_current {
                        repo.uncommitted_files.len()
                    } else {
                        0
                    },
                    name: branch.name.clone(),
                    is_current: branch.is_current,
                    is_default: branch.is_default,
                    ahead: branch.ahead,
                    behind: branch.behind,
                    last_commit_msg: branch.last_commit_msg.clone(),
                    last_commit_time: branch.last_commit_time.clone(),
                });
            }
        }

        let query = self.branch_query.value.trim().to_lowercase();
        rows.retain(|row| {
            if self.branch_filter == BranchFilter::CheckedOut && !row.is_current {
                return false;
            }
            if self.branch_filter == BranchFilter::Feature && row.is_default {
                return false;
            }
            if !query.is_empty() {
                return row.name.to_lowercase().contains(&query)
                    || row.repo_name.to_lowercase().contains(&query)
                    || row.last_commit_msg.to_lowercase().contains(&query);
            }
            true
        });

        let mut container = div()
            .flex()
            .flex_col()
            .w_full()
            .bg(theme.panel)
            .border_1()
            .border_color(theme.divider)
            .rounded(px(12.0))
            .overflow_hidden();

        container = container.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .p(px(16.0))
                .border_b_1()
                .border_color(theme.divider)
                .child(self.render_filter_tabs(cx))
                .child(div().w(px(256.0)).child(self.text_field(
                    FieldId::BranchSearch,
                    "Search branch or repo...",
                    FieldVariant::Search,
                    window,
                    cx,
                ))),
        );

        if rows.is_empty() {
            return container.child(
                div()
                    .py(px(56.0))
                    .px(px(24.0))
                    .flex()
                    .justify_center()
                    .xs()
                    .fg(theme.label_muted)
                    .child("No branches match the current filter."),
            );
        }

        container = container.child(self.head_row(theme));

        let mut body = div().flex().flex_col().w_full();
        for (index, row) in rows.iter().enumerate() {
            body = body.child(self.row(theme, row, index > 0, latest_width, branch_width, cx));
        }

        container.child(body)
    }

    fn render_filter_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let tabs = [
            (BranchFilter::All, "All Branches"),
            (BranchFilter::Feature, "Feature & Experiment"),
            (BranchFilter::CheckedOut, "Checked Out"),
        ];

        let mut segmented = div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .p(px(4.0))
            .bg(theme.segmented_bg)
            .rounded(px(8.0))
            .flex_shrink_0();

        for (filter, label) in tabs {
            let active = self.branch_filter == filter;
            segmented = segmented.child(
                div()
                    .id(("branch-tab", filter as u8 as usize))
                    .px(px(12.0))
                    .py(px(6.0))
                    .rounded(px(6.0))
                    .cursor_pointer()
                    .whitespace_nowrap()
                    .flex_shrink_0()
                    .xs()
                    .medium()
                    .bg(if active {
                        theme.segmented_active_bg
                    } else {
                        crate::theme::alpha(theme.panel, 0.0)
                    })
                    .fg(if active {
                        theme.segmented_active_text
                    } else {
                        theme.nav_idle_text
                    })
                    .when(active, |d| d.shadow_xs())
                    .when(!active, move |d| {
                        d.hover(move |s| s.fg(theme.nav_hover_text))
                    })
                    .on_click(cx.listener(move |this, _e, _w, cx| {
                        this.branch_filter = filter;
                        cx.notify();
                    }))
                    .child(label),
            );
        }
        segmented
    }

    fn head_row(&self, theme: Theme) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .w_full()
            .border_b_1()
            .border_color(theme.divider)
            .bg(theme.table_head_bg)
            .xs()
            .medium()
            .fg(theme.label_muted)
            .child(
                cell(COL_BRANCH)
                    .py(px(10.0))
                    .pl(px(20.0))
                    .pr(px(16.0))
                    .whitespace_nowrap()
                    .child("Branch"),
            )
            .child(
                cell(COL_REPO)
                    .py(px(10.0))
                    .px(px(16.0))
                    .whitespace_nowrap()
                    .child("Repository"),
            )
            .child(
                cell(COL_STATE)
                    .py(px(10.0))
                    .px(px(16.0))
                    .whitespace_nowrap()
                    .child("State"),
            )
            .child(
                cell(COL_SYNC)
                    .py(px(10.0))
                    .px(px(16.0))
                    .text_right()
                    .whitespace_nowrap()
                    .child("Ahead / Behind"),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .py(px(10.0))
                    .px(px(16.0))
                    .whitespace_nowrap()
                    .child("Last Commit"),
            )
            .child(
                cell(COL_UPDATED)
                    .py(px(10.0))
                    .pl(px(16.0))
                    .pr(px(20.0))
                    .text_right()
                    .whitespace_nowrap()
                    .child("Updated"),
            )
    }

    fn row(
        &self,
        theme: Theme,
        row: &BranchRow,
        bordered: bool,
        latest_width: gpui::Pixels,
        branch_width: gpui::Pixels,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let repo_id = row.repo_id.clone();

        let state_cell = if row.is_current {
            div()
                .flex()
                .items_center()
                .gap(px(2.0))
                .xs()
                .medium()
                .fg(theme.accent_text)
                .whitespace_nowrap()
                .child("Checked out")
                .when(row.uncommitted > 0, |d| {
                    d.child(format!(" · {} uncommitted", row.uncommitted))
                })
        } else if row.is_default {
            div()
                .xs()
                .fg(theme.label_muted)
                .whitespace_nowrap()
                .child("Default branch")
        } else {
            div()
                .xs()
                .fg(theme.text_body)
                .whitespace_nowrap()
                .child("Local branch")
        };

        div()
            .id(gpui::SharedString::from(format!("branch-row-{}", row.name)))
            .flex()
            .items_center()
            .w_full()
            .cursor_pointer()
            .when(bordered, |d| d.border_t_1().border_color(theme.row_border))
            .hover(move |s| s.bg(theme.row_hover_bg))
            .on_click(cx.listener(move |this, _e, _w, cx| {
                this.select_repo(repo_id.clone(), cx);
            }))
            .child({
                let name = crate::text::ellipsize(
                    cx,
                    &row.name,
                    crate::theme::FONT_MONO,
                    crate::theme::W_MEDIUM,
                    crate::theme::TEXT_XS,
                    branch_width,
                );
                cell(COL_BRANCH)
                    .py(px(12.0))
                    .pl(px(20.0))
                    .pr(px(16.0))
                    .overflow_hidden()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .mono_xs()
                            .medium()
                            .fg(theme.text_app)
                            .whitespace_nowrap()
                            .child(icon("git-branch", 14.0, theme.icon_idle))
                            .child(name),
                    )
            })
            .child(
                cell(COL_REPO).py(px(12.0)).px(px(16.0)).child(
                    div()
                        .flex()
                        .items_center()
                        .child(
                            div()
                                .xs()
                                .medium()
                                .fg(theme.text_soft)
                                .whitespace_nowrap()
                                .child(row.repo_name.clone()),
                        )
                        .child(
                            div()
                                .ml(px(6.0))
                                .xs()
                                .fg(theme.label_muted)
                                .whitespace_nowrap()
                                .child(format!("· {}", row.workspace)),
                        ),
                ),
            )
            .child(cell(COL_STATE).py(px(12.0)).px(px(16.0)).child(state_cell))
            .child(
                cell(COL_SYNC)
                    .py(px(12.0))
                    .px(px(16.0))
                    .child(crate::widgets::sync_arrows(&theme, row.ahead, row.behind)),
            )
            .child({
                let message = crate::text::ellipsize(
                    cx,
                    &row.last_commit_msg,
                    crate::theme::FONT_SANS,
                    crate::theme::W_REGULAR,
                    crate::theme::TEXT_XS,
                    latest_width,
                );
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .max_w(px(COL_LATEST_MAX))
                    .py(px(12.0))
                    .px(px(16.0))
                    .xs()
                    .fg(theme.text_body)
                    .whitespace_nowrap()
                    .child(message)
            })
            .child(
                cell(COL_UPDATED)
                    .py(px(12.0))
                    .pl(px(16.0))
                    .pr(px(20.0))
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .xs()
                            .fg(theme.label_muted)
                            .whitespace_nowrap()
                            .child(row.last_commit_time.clone()),
                    ),
            )
    }
}
