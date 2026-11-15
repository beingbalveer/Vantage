//! All Repositories / Needs Attention list: one bordered container of compact
//! two-line rows inside collapsible group headers.

use gpui::{div, prelude::*, px, relative, Context, Div, IntoElement, Rgba, Styled};

use crate::app::{RepoOp, VantageApp};
use crate::model::RepoSyncState;
use crate::theme::Theme;
use crate::ui::{icon, icon_button_sm, spinner, IconSize};
use crate::ui::{primary_button, Txt};

struct TableRow {
    id: String,
    name: String,
    workspace: String,
    branch: String,
    sync: RepoSyncState,
    uncommitted: usize,
    ahead: i32,
    behind: i32,
    /// Drives the row's fetch enabled state.
    has_remote: bool,
    activity: String,
}

struct TableSection {
    key: String,
    label: String,
    rows: Vec<TableRow>,
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
        has_remote: repo.has_remote,
        activity: repo.last_activity_label.clone(),
    }
}

impl VantageApp {
    pub fn render_repo_table(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
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
                    card = card.child(self.render_row(theme, row, cx));
                }
            }

            table = table.child(card);
        }

        table
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

    fn render_row(&self, theme: Theme, row: &TableRow, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.selected_repo_id.as_deref() == Some(row.id.as_str());
        let repo_id = row.id.clone();

        // Fetch control: the only row-level action, so it stays on the row.
        let fetching = self.repo_op(&row.id) == Some(RepoOp::Fetch);
        let busy = self.repo_op(&row.id).is_some();
        let fetch_id = row.id.clone();
        let fetch_control: gpui::AnyElement = if fetching {
            div()
                .flex()
                .items_center()
                .justify_center()
                .w(px(22.0))
                .h(px(22.0))
                .flex_shrink_0()
                .child(spinner(
                    format!("row-fetch-{}", row.id),
                    "refresh-cw",
                    IconSize::Xs,
                    theme.accent_text,
                ))
                .into_any_element()
        } else if row.has_remote && !busy {
            icon_button_sm(&theme, "refresh-cw")
                .id(gpui::SharedString::from(format!("row-fetch-{}", row.id)))
                // Stop the click so it never also selects the row underneath.
                .on_click(cx.listener(move |this, _e, _w, cx| {
                    cx.stop_propagation();
                    if let Some(repo) = this.repos.iter().find(|r| r.id == fetch_id).cloned() {
                        this.fetch_repo(&repo, cx);
                    }
                }))
                .into_any_element()
        } else {
            icon_button_sm(&theme, "refresh-cw")
                .opacity(0.35)
                .into_any_element()
        };

        div()
            .id(gpui::SharedString::from(format!("repo-row-{}", row.id)))
            .group("repo-row")
            .flex()
            .flex_col()
            .w_full()
            .gap(px(3.0))
            .px(px(16.0))
            .py(px(8.0))
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
            }))
            // Name first: it is what the eye looks for.
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .w_full()
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .sm()
                            .medium()
                            .fg(theme.text_app)
                            .truncate()
                            .child(row.name.clone()),
                    )
                    .child(status_badges(&theme, row)),
            )
            // Branch, then how long ago it moved, then the row action.
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .w_full()
                    .child(icon("git-branch", IconSize::Xs, theme.icon_idle))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .mono_xs()
                            .fg(theme.text_body)
                            .truncate()
                            .child(row.branch.clone()),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .xs()
                            .fg(theme.label_muted)
                            .whitespace_nowrap()
                            .child(row.activity.clone()),
                    )
                    .child(fetch_control),
            )
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

/// Compact status line for a row: every state the repository is in, not just
/// one, each with an icon and a word so colour is never the only signal.
fn status_badges(theme: &Theme, row: &TableRow) -> Div {
    let mut badges = div().flex().items_center().gap(px(10.0)).flex_shrink_0();

    if row.sync == RepoSyncState::Conflict {
        badges = badges.child(badge("alert-triangle", "Conflict", theme.status_red));
    }
    if row.uncommitted > 0 {
        let label = if row.uncommitted == 1 {
            "1 change".to_string()
        } else {
            format!("{} changes", row.uncommitted)
        };
        badges = badges.child(badge("file-diff", &label, theme.status_amber));
    }
    if row.ahead > 0 {
        badges = badges.child(badge(
            "arrow-up",
            &format!("{} ahead", row.ahead),
            theme.status_blue,
        ));
    }
    if row.behind > 0 {
        badges = badges.child(badge(
            "arrow-down",
            &format!("{} behind", row.behind),
            theme.status_red,
        ));
    }
    if row.sync == RepoSyncState::Clean {
        badges = badges.child(badge("circle-check-big", "Clean", theme.status_green));
    }

    badges
}

fn badge(icon_name: &'static str, label: &str, color: Rgba) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(4.0))
        .flex_shrink_0()
        .whitespace_nowrap()
        .child(icon(icon_name, IconSize::Xs, color))
        .child(div().xs().medium().fg(color).child(label.to_string()))
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
        has_remote: row.has_remote,
        activity: row.activity.clone(),
    }
}
