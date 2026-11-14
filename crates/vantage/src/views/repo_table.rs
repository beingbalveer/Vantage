//! All Repositories / Needs Attention table: one bordered container, collapsible
//! group headers, six columns.

use gpui::{div, prelude::*, px, relative, Context, Div, IntoElement, Styled};

use crate::app::VantageApp;
use crate::model::RepoSyncState;
use crate::theme::Theme;
use crate::ui::{icon, IconSize};
use crate::ui::{primary_button, Txt};
use crate::widgets::{status_text, sync_arrows};

const COL_REPO: f32 = 0.17;
const COL_BRANCH: f32 = 0.19;
const COL_TREE: f32 = 0.165;
const COL_SYNC: f32 = 0.085;
const COL_UPDATED: f32 = 0.075;
const COL_LATEST: f32 = 1.0 - (COL_REPO + COL_BRANCH + COL_TREE + COL_SYNC + COL_UPDATED);
const COL_LATEST_MAX: f32 = 320.0; // `max-w-xs` on the browser cell

fn repo_column(compact: bool) -> f32 {
    if compact {
        0.42
    } else {
        COL_REPO
    }
}

fn branch_column(compact: bool) -> f32 {
    if compact {
        0.30
    } else {
        COL_BRANCH
    }
}

fn tree_column(compact: bool) -> f32 {
    if compact {
        0.28
    } else {
        COL_TREE
    }
}

struct TableRow {
    id: String,
    name: String,
    workspace: String,
    branch: String,
    sync: RepoSyncState,
    uncommitted: usize,
    ahead: i32,
    behind: i32,
    latest: String,
    activity: String,
}

struct TableSection {
    key: String,
    label: String,
    rows: Vec<TableRow>,
}

fn cell(width: f32) -> Div {
    div().w(relative(width)).flex_shrink_0()
}

fn make_row(repo: &crate::model::Repository) -> TableRow {
    TableRow {
        id: repo.id.clone(),
        name: repo.name.clone(),
        workspace: repo.workspace.clone(),
        branch: repo.current_branch.clone(),
        sync: repo.sync_state,
        uncommitted: repo.uncommitted_files.len(),
        ahead: repo.ahead_count,
        behind: repo.behind_count,
        latest: repo
            .recent_commits
            .first()
            .map(|c| c.message.clone())
            .unwrap_or_else(|| "—".to_string()),
        activity: repo.last_activity_label.clone(),
    }
}

impl VantageApp {
    pub fn render_repo_table(
        &mut self,
        cx: &mut Context<Self>,
        viewport_width: gpui::Pixels,
    ) -> impl IntoElement {
        let theme = self.theme;
        let wide = viewport_width >= px(1050.0);
        let compact = viewport_width < px(900.0);
        let container_width = (viewport_width - px(48.0)).max(px(0.0)).min(px(1440.0));
        let latest_width =
            crate::text::column_width(container_width, COL_LATEST, 16.0).min(px(COL_LATEST_MAX));
        let branch_width =
            (crate::text::column_width(container_width, branch_column(compact), 16.0) - px(20.0))
                .max(px(24.0));
        let repos = self.filtered_repos();
        let rows: Vec<TableRow> = repos.iter().map(|r| make_row(r)).collect();
        drop(repos);

        let mut sections: Vec<TableSection> = Vec::new();
        let mut assigned = vec![false; rows.len()];
        for group in self.groups.clone() {
            let mut items = Vec::new();
            for (index, row) in rows.iter().enumerate() {
                if row.workspace == group {
                    assigned[index] = true;
                    items.push(clone_row(row));
                }
            }
            if items.is_empty() {
                continue;
            }
            sections.push(TableSection {
                key: group.clone(),
                label: group,
                rows: items,
            });
        }
        for (index, row) in rows.iter().enumerate() {
            if assigned[index] {
                continue;
            }
            if let Some(existing) = sections.iter_mut().find(|s| s.key == row.workspace) {
                existing.rows.push(clone_row(row));
            } else {
                sections.push(TableSection {
                    key: row.workspace.clone(),
                    label: row.workspace.clone(),
                    rows: vec![clone_row(row)],
                });
            }
        }

        let is_empty = sections.iter().all(|s| s.rows.is_empty());

        let mut table = div().flex().flex_col().w_full().gap(px(10.0));

        if self.is_refreshing {
            return table.child(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .bg(theme.panel)
                    .border_1()
                    .border_color(theme.divider)
                    .rounded(px(12.0))
                    .overflow_hidden()
                    .child(self.render_skeleton()),
            );
        }

        if is_empty {
            return table.child(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .bg(theme.panel)
                    .border_1()
                    .border_color(theme.divider)
                    .rounded(px(12.0))
                    .overflow_hidden()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .py(px(48.0))
                            .px(px(24.0))
                            .child(
                                div()
                                    .xs()
                                    .fg(theme.label_muted)
                                    .child("No matching repositories."),
                            )
                            .child(
                                div().mt(px(12.0)).child(
                                    primary_button(&theme, "Reset filters", None)
                                        .id("reset-filters")
                                        .on_click(cx.listener(|this, _e, _w, cx| {
                                            this.reset_filters(cx);
                                        })),
                                ),
                            ),
                    ),
            );
        }

        table = table.child(self.render_table_head(theme, wide, compact));

        for section in &sections {
            let mut card = div()
                .flex()
                .flex_col()
                .w_full()
                .bg(theme.panel)
                .border_1()
                .border_color(theme.divider)
                .rounded(px(12.0))
                .overflow_hidden()
                .child(self.render_group_header(theme, section, cx));

            if !self.collapsed_groups.contains(&section.key) {
                for row in &section.rows {
                    card = card.child(self.render_row(
                        theme,
                        row,
                        wide,
                        compact,
                        latest_width,
                        branch_width,
                        cx,
                    ));
                }
            }

            table = table.child(card);
        }

        table
    }

    fn render_table_head(&self, theme: Theme, wide: bool, compact: bool) -> impl IntoElement {
        let mut head = div()
            .flex()
            .items_center()
            .w_full()
            .flex_shrink_0()
            .xs()
            .medium()
            .fg(theme.label_muted);

        head = head
            .child(
                cell(repo_column(compact))
                    .py(px(10.0))
                    .pl(px(20.0))
                    .pr(px(16.0))
                    .whitespace_nowrap()
                    .child("Repository"),
            )
            .child(
                cell(branch_column(compact))
                    .py(px(10.0))
                    .px(px(16.0))
                    .whitespace_nowrap()
                    .child("Current Branch"),
            )
            .child(
                cell(tree_column(compact))
                    .py(px(10.0))
                    .px(px(16.0))
                    .whitespace_nowrap()
                    .child("Working Tree"),
            );

        if !compact {
            head = head.child(
                cell(COL_SYNC)
                    .py(px(10.0))
                    .px(px(16.0))
                    .text_right()
                    .whitespace_nowrap()
                    .child("Remote Sync"),
            );
        }

        if wide {
            head = head.child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .py(px(10.0))
                    .px(px(16.0))
                    .whitespace_nowrap()
                    .child("Latest Commit"),
            );
        }

        if !compact {
            head = head.child(
                cell(COL_UPDATED)
                    .py(px(10.0))
                    .pl(px(16.0))
                    .pr(px(20.0))
                    .text_right()
                    .whitespace_nowrap()
                    .child("Updated"),
            );
        }
        head
    }

    fn render_group_header(
        &self,
        theme: Theme,
        section: &TableSection,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let collapsed = self.collapsed_groups.contains(&section.key);
        let key = section.key.clone();
        let count = section.rows.len();

        div()
            .flex()
            .items_center()
            .w_full()
            .bg(theme.group_head_bg)
            .child(
                div()
                    .id(gpui::SharedString::from(format!("group-toggle-{key}")))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(16.0))
                    .w_full()
                    .px(px(20.0))
                    .py(px(9.0))
                    .cursor_pointer()
                    .hover(move |s| s.bg(theme.nav_hover_bg))
                    .on_click(cx.listener(move |this, _e, _w, cx| {
                        this.toggle_group_collapsed(key.clone(), cx);
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .display_xs()
                                    .semibold()
                                    .fg(theme.text_soft)
                                    .child(section.label.clone()),
                            )
                            .child(
                                div()
                                    .mono_xs()
                                    .fg(theme.label_muted)
                                    .child(format!("({count})")),
                            ),
                    )
                    .child(icon(
                        if collapsed {
                            "chevron-right"
                        } else {
                            "chevron-down"
                        },
                        IconSize::Sm,
                        theme.icon_idle,
                    )),
            )
    }

    #[allow(clippy::too_many_arguments)]
    fn render_row(
        &self,
        theme: Theme,
        row: &TableRow,
        wide: bool,
        compact: bool,
        latest_width: gpui::Pixels,
        branch_width: gpui::Pixels,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected = self.selected_repo_id.as_deref() == Some(row.id.as_str());
        let repo_id = row.id.clone();
        let activity_color = theme.text_soft;

        let mut line = div()
            .id(gpui::SharedString::from(format!("repo-row-{}", row.id)))
            .group("repo-row")
            .flex()
            .items_center()
            .w_full()
            .cursor_pointer()
            .border_t_1()
            .border_color(theme.row_border)
            .bg(if selected {
                theme.accent_soft
            } else {
                theme.panel
            })
            .when(!selected, move |d| {
                d.hover(move |s| s.bg(theme.row_hover_bg))
            })
            .on_click(cx.listener(move |this, _e, window, cx| {
                this.select_repo(repo_id.clone(), cx);
                window.refresh();
            }));

        // Repository name only: a repository can contain several languages,
        // so a single stack label would be misleading.
        line = line.child(
            cell(repo_column(compact))
                .py(px(12.0))
                .pl(px(20.0))
                .pr(px(16.0))
                .flex()
                .items_center()
                .child(
                    div()
                        .sm()
                        .medium()
                        .fg(theme.text_app)
                        .whitespace_nowrap()
                        .child(row.name.clone()),
                ),
        );

        // Current branch (ellipsized to the column, like `truncate` in the source)
        let branch = crate::text::ellipsize(
            cx,
            &row.branch,
            crate::theme::FONT_MONO,
            crate::theme::W_MEDIUM,
            crate::theme::TEXT_XS,
            branch_width,
        );
        line = line.child(
            cell(branch_column(compact))
                .py(px(12.0))
                .px(px(16.0))
                .overflow_hidden()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .mono_xs()
                        .medium()
                        .fg(theme.text_soft)
                        .whitespace_nowrap()
                        .child(icon("git-branch", IconSize::Sm, theme.icon_idle))
                        .child(branch),
                ),
        );

        // Working tree status
        line = line.child(
            cell(tree_column(compact))
                .py(px(12.0))
                .px(px(16.0))
                .child(status_text(&theme, row.sync, row.uncommitted)),
        );

        // Remote sync
        if !compact {
            line = line.child(
                cell(COL_SYNC)
                    .py(px(12.0))
                    .px(px(16.0))
                    .child(sync_arrows(&theme, row.ahead, row.behind)),
            );
        }

        if wide {
            let latest = crate::text::ellipsize(
                cx,
                &row.latest,
                crate::theme::FONT_SANS,
                crate::theme::W_REGULAR,
                crate::theme::TEXT_XS,
                latest_width,
            );
            line = line.child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .py(px(12.0))
                    .px(px(16.0))
                    .xs()
                    .fg(theme.text_body)
                    .whitespace_nowrap()
                    .child(latest),
            );
        }

        // Updated + disclosure chevron
        if !compact {
            line = line.child(
                cell(COL_UPDATED)
                    .py(px(14.0))
                    .pl(px(16.0))
                    .pr(px(20.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap(px(6.0))
                            .xs()
                            .fg(theme.label_muted)
                            .whitespace_nowrap()
                            .child(row.activity.clone())
                            .child(
                                div()
                                    .group_hover("repo-row", move |s| s.fg(activity_color))
                                    .child(icon("chevron-right", IconSize::Sm, theme.icon_idle)),
                            ),
                    ),
            );
        }

        line
    }

    fn render_skeleton(&self) -> impl IntoElement {
        let theme = self.theme;
        let mut list = div().flex().flex_col().w_full();

        for index in 0..6 {
            let mut row = div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(24.0))
                .w_full()
                .px(px(20.0))
                .py(px(14.0));
            if index > 0 {
                row = row.border_t_1().border_color(theme.divider);
            }
            list = list.child(
                row.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .w(relative(0.33))
                        .child(
                            div()
                                .h(px(16.0))
                                .w(px(160.0))
                                .rounded(px(4.0))
                                .bg(theme.divider),
                        )
                        .child(
                            div()
                                .h(px(12.0))
                                .w(px(224.0))
                                .rounded(px(4.0))
                                .bg(crate::theme::alpha(theme.divider, 0.6)),
                        ),
                )
                .child(
                    div()
                        .h(px(16.0))
                        .w(px(128.0))
                        .rounded(px(4.0))
                        .bg(theme.divider),
                )
                .child(
                    div()
                        .h(px(16.0))
                        .w(px(96.0))
                        .rounded(px(4.0))
                        .bg(theme.divider),
                )
                .child(
                    div()
                        .h(px(16.0))
                        .w(px(80.0))
                        .rounded(px(4.0))
                        .bg(theme.divider),
                ),
            );
        }
        list
    }
}

fn clone_row(row: &TableRow) -> TableRow {
    TableRow {
        id: row.id.clone(),
        name: row.name.clone(),
        workspace: row.workspace.clone(),
        branch: row.branch.clone(),
        sync: row.sync,
        uncommitted: row.uncommitted,
        ahead: row.ahead,
        behind: row.behind,
        latest: row.latest.clone(),
        activity: row.activity.clone(),
    }
}
