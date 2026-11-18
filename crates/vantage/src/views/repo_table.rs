//! All Repositories / Needs Attention list: one bordered container of compact
//! two-line rows inside collapsible group headers.

use gpui::{div, prelude::*, px, relative, Context, Div, IntoElement, Styled, Window};

use crate::app::{SummaryFilter, VantageApp, REPO_LIST_KEY_CONTEXT};
use crate::model::RepoSyncState;
use crate::theme::Theme;
use crate::ui::{icon, status_badges, BadgeDensity, IconSize};
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
    pub fn render_repo_table(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme;
        let layout = self.dashboard_layout(window);
        let compact = layout.compact_list();
        let compact_summary = layout.compact_summary();
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
            .debug_selector(|| "repo-table".to_string())
            .id("repo-table")
            .flex()
            .flex_col()
            .w_full()
            .min_w(px(0.0))
            .gap(px(10.0))
            .track_focus(&list_focus)
            .key_context(REPO_LIST_KEY_CONTEXT)
            .child(self.render_summary_strip(theme, compact_summary, cx));

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
                .min_w(px(0.0))
                .bg(theme.panel)
                .border_1()
                .border_color(theme.divider)
                .rounded(px(12.0))
                .overflow_hidden()
                .child(self.render_group_header(theme, section, cx));

            if !self.collapsed_groups.contains(&section.key) {
                for row in &section.rows {
                    card = card.child(self.render_row(theme, row, compact, cx));
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

    fn render_row(
        &self,
        theme: Theme,
        row: &TableRow,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected = self.selected_repo_id.as_deref() == Some(row.id.as_str());
        let repo_id = row.id.clone();
        let density = if compact {
            BadgeDensity::Compact
        } else {
            BadgeDensity::Full
        };
        let badge_id = format!("repo-badges-{}", row.id);
        let top_id = format!("repo-row-top-{}", row.id);

        div()
            .debug_selector({
                let id = format!("repo-row-{}", row.id);
                move || id.clone()
            })
            .id(gpui::SharedString::from(format!("repo-row-{}", row.id)))
            .debug_selector({
                let id = row.id.clone();
                move || format!("repo-row-{id}")
            })
            .group("repo-row")
            .flex()
            .flex_col()
            .w_full()
            .min_w(px(0.0))
            .gap(px(if compact { 4.0 } else { 5.0 }))
            .px(px(if compact { 10.0 } else { 12.0 }))
            .py(px(if compact { 8.0 } else { 10.0 }))
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
            // Name first: it is what the eye looks for. When the list is
            // narrow the badges shrink to icon + count before the name loses
            // all its room, and they wrap instead of overflowing the card.
            .child(
                div()
                    .debug_selector({
                        let id = format!("repo-row-top-{}", row.id);
                        move || id.clone()
                    })
                    .id(gpui::SharedString::from(top_id))
                    .flex()
                    .items_center()
                    .gap(px(if compact { 8.0 } else { 10.0 }))
                    .w_full()
                    .min_w(px(0.0))
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
                    .child(status_badges(
                        &theme,
                        row.uncommitted,
                        row.ahead,
                        row.behind,
                        row.sync == RepoSyncState::Conflict,
                        density,
                        &badge_id,
                    )),
            )
            // Branch, then how long ago it moved, then the row action.
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .w_full()
                    .min_w(px(0.0))
                    .child(icon("git-branch", IconSize::Xxs, theme.text_dim))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .mono_xs()
                            .fg(theme.text_meta)
                            .truncate()
                            .child(row.branch.clone()),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .xs()
                            .fg(theme.text_meta)
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
    fn render_summary_strip(&self, theme: Theme, compact: bool, cx: &mut Context<Self>) -> Div {
        let summary = self.repo_summary();
        let active = self.summary_filter;
        let mut strip = div()
            .flex()
            .items_center()
            .flex_wrap()
            .gap(px(if compact { 3.0 } else { 5.0 }))
            .w_full()
            .min_w(px(0.0));

        // Wide labels say what the numbers mean; the compact list swaps in one
        // word per filter so all four still fit on a single line beside a
        // two-thirds inspector.
        let items = [
            (
                SummaryFilter::All,
                summary.total,
                "repositories",
                "repos",
                "all",
                theme.text_dim,
            ),
            (
                SummaryFilter::Uncommitted,
                summary.uncommitted,
                "uncommitted",
                "uncommitted",
                "dirty",
                theme.status_amber,
            ),
            (
                SummaryFilter::Unpushed,
                summary.unpushed,
                "unpushed",
                "unpushed",
                "unpushed",
                theme.status_blue,
            ),
            (
                SummaryFilter::Clean,
                summary.clean,
                "clean",
                "clean",
                "clean",
                theme.status_green,
            ),
        ];

        for (filter, count, id_label, label, short_label, color) in items {
            let selected = active == Some(filter);
            let label = if compact { short_label } else { label };
            let mut item = div()
                .debug_selector({
                    let id = format!("summary-{id_label}");
                    move || id.clone()
                })
                .id(gpui::SharedString::from(format!("summary-{id_label}")))
                .flex()
                .items_center()
                .gap(px(if compact { 3.0 } else { 5.0 }))
                .px(px(if compact { 3.0 } else { 5.0 }))
                .py(px(if compact { 2.0 } else { 3.0 }))
                .rounded(px(5.0))
                .cursor_pointer()
                .whitespace_nowrap()
                .when(selected, |d| d.bg(theme.nav_active_bg))
                .when(!selected, |d| d.hover(move |s| s.bg(theme.nav_hover_bg)));
            // The colour dot is the first thing to go when the row is tight;
            // the count keeps the colour so the categories stay distinct.
            if !compact {
                item = item.child(div().w(px(6.0)).h(px(6.0)).rounded(px(3.0)).bg(color));
            }
            item = item
                .child(
                    div()
                        .xs()
                        .medium()
                        .fg(if selected {
                            theme.text_app
                        } else if compact {
                            color
                        } else {
                            theme.text_body
                        })
                        .child(count.to_string()),
                )
                .child(div().xs().fg(theme.label_muted).child(label))
                .on_click(cx.listener(move |this, _e, _w, cx| {
                    this.toggle_summary_filter(filter, cx);
                }));
            strip = strip.child(item);
        }

        if active.is_some() {
            strip = strip.child(
                div()
                    .id("summary-clear")
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(4.0))
                    .px(px(if compact { 3.0 } else { 6.0 }))
                    .py(px(if compact { 2.0 } else { 3.0 }))
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

#[cfg(test)]
mod tests {
    use gpui::{px, size, AppContext, TestAppContext};

    use crate::app::VantageApp;
    use crate::model::{ChangedFile, FileChangeStatus, NavSection, RepoSyncState, Repository};
    use crate::root::RootView;

    fn fixture_repo() -> Repository {
        Repository {
            id: "repo-1".to_string(),
            name: "a-repository-name-that-is-long-enough-to-need-truncation".to_string(),
            workspace: "Ungrouped".to_string(),
            local_path: "/private/var/folders/very/long/workspace/vantage-fixture".to_string(),
            head_sha: None,
            has_remote: true,
            stash_count: 0,
            current_branch: "feature/a-branch-name-that-also-needs-truncation".to_string(),
            author: None,
            sync_state: RepoSyncState::Modified,
            ahead_count: 3,
            behind_count: 2,
            has_upstream: true,
            is_detached: false,
            last_activity_label: "2h ago".to_string(),
            last_activity_minutes: 120,
            uncommitted_files: vec![ChangedFile {
                path: "crates/vantage/src/views/inspector.rs".to_string(),
                status: FileChangeStatus::Modified,
                additions: 4,
                deletions: 2,
                orig_path: None,
                staged: false,
                diff_preview: Vec::new(),
                preview_truncated: false,
            }],
            branches: Vec::new(),
            recent_commits: Vec::new(),
            local_only_commits: std::collections::HashSet::new(),
        }
    }

    fn draw(cx: &mut gpui::VisualTestContext, app: &gpui::Entity<VantageApp>) {
        cx.update(|window, cx| {
            app.update(cx, |app, cx| {
                app.repos = vec![fixture_repo()];
                app.selected_repo_id = Some("repo-1".to_string());
                app.settings.right_panel_open = true;
                app.settings.right_panel_fraction = 0.67;
                app.settings.sidebar_width = 256.0;
                app.sidebar_collapsed = false;
                app.active_section = NavSection::Overview;
                app.is_refreshing = false;
                app.is_scanning = false;
                cx.notify();
            });
            window.draw(cx).clear();
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear();
        });
    }

    fn bounds(cx: &mut gpui::VisualTestContext, id: &'static str) -> gpui::Bounds<gpui::Pixels> {
        cx.debug_bounds(id)
            .unwrap_or_else(|| panic!("{id} should be rendered"))
    }

    fn right(bounds: gpui::Bounds<gpui::Pixels>) -> f32 {
        f32::from(bounds.right())
    }

    fn left(bounds: gpui::Bounds<gpui::Pixels>) -> f32 {
        f32::from(bounds.left())
    }

    #[gpui::test]
    fn the_dashboard_stays_inside_the_supported_window_widths(cx: &mut TestAppContext) {
        let app = cx.new(VantageApp::new);
        let (_root, cx) = cx.add_window_view(|_window, _cx| RootView::new(app.clone()));
        draw(cx, &app);

        for (width, height) in [(1040.0, 640.0), (1470.0, 900.0)] {
            cx.simulate_resize(size(px(width), px(height)));
            cx.update(|window, cx| {
                window.draw(cx).clear();
            });
            cx.run_until_parked();
            cx.update(|window, cx| {
                window.draw(cx).clear();
            });

            let table = bounds(cx, "repo-table");
            let layout = cx.update(|window, cx| app.read(cx).dashboard_layout(window));
            assert_eq!(
                layout.compact_list(),
                width <= 1040.0,
                "the minimum window should use the compact list, the wide one should not"
            );
            assert_eq!(
                layout.compact_inspector(),
                width <= 1040.0,
                "the narrow inspector needs the compact header"
            );
            assert!(
                layout.compact_summary(),
                "the summary strip should be compact beside the inspector at {width}px"
            );
            assert!(
                right(table) <= width + 0.5,
                "the repository list should stay inside a {width}px window: {table:?}"
            );

            // Summary filters: all four stay inside the list column. The
            // layout helper check above guarantees the compact form is in use
            // when the column is tight.
            for id in [
                "summary-repositories",
                "summary-uncommitted",
                "summary-unpushed",
                "summary-clean",
            ] {
                let item = bounds(cx, id);
                assert!(
                    left(item) >= left(table) - 0.5 && right(item) <= right(table) + 0.5,
                    "{id} should stay inside the list card: {item:?}"
                );
            }

            // A row with changes + ahead + behind keeps the badges inside the
            // card instead of letting them push through the right edge.
            let row = bounds(cx, "repo-row-repo-1");
            let top = bounds(cx, "repo-row-top-repo-1");
            let badges = bounds(cx, "repo-badges-repo-1");
            assert!(
                right(top) <= right(row) + 0.5,
                "the row's first line should not overflow its card: {top:?} vs {row:?}"
            );
            assert!(
                right(badges) <= right(row) - 8.0,
                "badges should stay inside the row padding: {badges:?} vs {row:?}"
            );

            // Inspector header: every desktop action remains on screen and the
            // path stops before the action cluster.
            let panel = bounds(cx, "repository-detail");
            assert!(
                right(panel) <= width + 0.5,
                "the inspector should stay inside the window: {panel:?}"
            );
            assert!(
                f32::from(panel.size.width) >= crate::layout::INSPECTOR_MIN_WIDTH - 0.5,
                "the inspector should keep its minimum width: {panel:?}"
            );

            let mut action_left = f32::INFINITY;
            for id in [
                "repo-menu",
                "pull-selected",
                "push-selected",
                "fetch-origin",
                "close-inspector",
            ] {
                let action = bounds(cx, id);
                assert!(
                    left(action) >= left(panel) - 0.5 && right(action) <= right(panel) + 0.5,
                    "{id} should stay inside the inspector: {action:?}"
                );
                assert!(
                    f32::from(action.size.width) > 0.0 && f32::from(action.size.height) > 0.0,
                    "{id} should have a clickable box: {action:?}"
                );
                if matches!(id, "pull-selected" | "push-selected" | "fetch-origin") {
                    action_left = action_left.min(left(action));
                }
            }

            let path = bounds(cx, "copy-path");
            assert!(
                right(path) <= action_left + 0.5,
                "the path line should stop before the action buttons: {path:?} vs {action_left}"
            );
        }
    }
}
