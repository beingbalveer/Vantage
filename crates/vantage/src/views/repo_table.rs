//! All Repositories / Needs Attention list: one bordered container of compact
//! two-line rows inside collapsible group headers.

use gpui::{div, prelude::*, px, relative, Context, Div, IntoElement, Styled};

use crate::app::{SummaryFilter, VantageApp, REPO_LIST_KEY_CONTEXT};
use crate::model::RepoSyncState;
use crate::theme::Theme;
use crate::ui::{icon, status_badges, IconSize};
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
        let list_focus = self.repo_list_focus();

        let mut table = div()
            .flex()
            .flex_col()
            .w_full()
            .gap(px(10.0))
            .track_focus(&list_focus)
            .key_context(REPO_LIST_KEY_CONTEXT)
            .child(self.render_summary_strip(theme, cx));

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
                    .px(px(12.0))
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

        div()
            .id(gpui::SharedString::from(format!("repo-row-{}", row.id)))
            .debug_selector({
                let id = row.id.clone();
                move || format!("repo-row-{id}")
            })
            .group("repo-row")
            .flex()
            .flex_col()
            .w_full()
            .gap(px(5.0))
            .px(px(12.0))
            .py(px(10.0))
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
                // A row click makes the list the active keyboard context, so
                // the following arrow keys belong to it.
                this.focus_repo_list(window);
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
                            .font_weight(crate::theme::W_REGULAR)
                            .fg(theme.text_app)
                            .truncate()
                            .child(row.name.clone()),
                    )
                    .child(status_badges(
                        &theme,
                        row.uncommitted,
                        row.ahead,
                        row.behind,
                        row.sync == RepoSyncState::Conflict,
                    )),
            )
            // Branch, then how long ago it moved, then the row action.
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .w_full()
                    .child(icon("git-branch", IconSize::Xxs, theme.text_dim))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .text_size(px(10.0))
                            .line_height(px(14.0))
                            .font_family(crate::theme::FONT_MONO)
                            .fg(theme.label_muted)
                            .truncate()
                            .child(row.branch.clone()),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(11.0))
                            .fg(theme.text_dim)
                            .whitespace_nowrap()
                            .child(row.activity.clone()),
                    ),
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

/// One quiet line of totals above the list: no cards, no chrome. Each item is a
/// filter, and the active one is marked so the list state is never ambiguous.
impl VantageApp {
    fn render_summary_strip(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let summary = self.repo_summary();
        let active = self.summary_filter;
        let mut strip = div()
            .flex()
            .items_center()
            .flex_wrap()
            .gap(px(6.0))
            .w_full();

        let items = [
            (
                SummaryFilter::All,
                summary.total,
                "repositories",
                theme.text_dim,
            ),
            (
                SummaryFilter::Uncommitted,
                summary.uncommitted,
                "uncommitted",
                theme.status_amber,
            ),
            (
                SummaryFilter::Unpushed,
                summary.unpushed,
                "unpushed",
                theme.status_blue,
            ),
            (
                SummaryFilter::Clean,
                summary.clean,
                "clean",
                theme.status_green,
            ),
        ];

        for (filter, count, label, color) in items {
            let selected = active == Some(filter);
            strip = strip.child(
                div()
                    .id(gpui::SharedString::from(format!("summary-{label}")))
                    .flex()
                    .items_center()
                    .gap(px(5.0))
                    .px(px(6.0))
                    .py(px(3.0))
                    .rounded(px(5.0))
                    .cursor_pointer()
                    .whitespace_nowrap()
                    .when(selected, |d| d.bg(theme.nav_active_bg))
                    .when(!selected, |d| d.hover(move |s| s.bg(theme.nav_hover_bg)))
                    .child(div().w(px(6.0)).h(px(6.0)).rounded(px(3.0)).bg(color))
                    .child(
                        div()
                            .xs()
                            .medium()
                            .fg(if selected {
                                theme.text_app
                            } else {
                                theme.text_body
                            })
                            .child(count.to_string()),
                    )
                    .child(div().xs().fg(theme.label_muted).child(label))
                    .on_click(cx.listener(move |this, _e, _w, cx| {
                        this.toggle_summary_filter(filter, cx);
                    })),
            );
        }

        if active.is_some() {
            strip = strip.child(
                div()
                    .id("summary-clear")
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .px(px(6.0))
                    .py(px(3.0))
                    .rounded(px(5.0))
                    .cursor_pointer()
                    .hover(move |s| s.bg(theme.nav_hover_bg))
                    .child(icon("x", IconSize::Xs, theme.label_muted))
                    .child(div().xs().medium().fg(theme.text_soft).child("Clear"))
                    .on_click(cx.listener(|this, _e, _w, cx| this.clear_summary_filter(cx))),
            );
        }

        strip
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
        activity: row.activity.clone(),
    }
}
