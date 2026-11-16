//! The Commits tab: the commit list and the opened-commit diff.

use gpui::{div, prelude::*, px, Context, Div, IntoElement, Window};

use crate::app::VantageApp;
use crate::ui::{icon, IconSize, Txt};

use super::{preview_row, split_path};

impl VantageApp {
    /// One commit, opened: a thin header, one scrolling row of files, and the
    /// patch filling whatever height is left.
    pub(super) fn render_commit_detail(
        &self,
        view: &crate::app::CommitView,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = self.theme;

        let back = div()
            .id("commit-back")
            .flex()
            .items_center()
            .gap(px(4.0))
            .px(px(6.0))
            .py(px(3.0))
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
                    .fg(theme.label_muted)
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
                        .fg(theme.label_muted)
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
                        .font_family(crate::theme::FONT_MONO)
                        .text_size(px(11.0))
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
                    .w(px(22.0))
                    .h(px(22.0))
                    .rounded(px(5.0))
                    .cursor_pointer()
                    .flex_shrink_0()
                    .hover(move |s| s.bg(theme.nav_hover_bg))
                    .child(icon("panel-right", IconSize::Xs, theme.icon_idle))
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
                    .py(px(6.0))
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
                            .font_family(crate::theme::FONT_MONO)
                            .text_size(px(11.0))
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
                let mut diff = div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .py(px(4.0))
                    .font_family(crate::theme::FONT_MONO)
                    .text_size(px(11.0))
                    .line_height(px(16.0));
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
            None if view.loading => div()
                .py(px(24.0))
                .w_full()
                .flex()
                .justify_center()
                .xs()
                .fg(theme.label_muted)
                .child("Loading the commit…"),
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

    pub(super) fn render_commits_tab(
        &self,
        repo: &crate::model::Repository,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme;

        // One commit opened: it takes the tab, and the diff takes the height.
        if let Some(view) = self
            .commit_view
            .as_ref()
            .filter(|view| view.repo_id == repo.id)
        {
            return self.render_commit_detail(view, cx);
        }

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

            let menu_repo = repo.id.clone();
            let menu_short = commit.sha.clone();
            let menu_subject = commit.message.clone();
            let can_amend = index == 0 && repo.commit_is_local_only(0);
            list = list.child(
                div()
                    .id(gpui::SharedString::from(format!("commit-{}", commit.sha)))
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .px(px(16.0))
                    .py(px(10.0))
                    .w_full()
                    .bg(theme.panel)
                    .on_mouse_down(
                        gpui::MouseButton::Right,
                        cx.listener(move |this, event: &gpui::MouseDownEvent, _w, cx| {
                            this.open_commit_menu(
                                &menu_repo,
                                &menu_short,
                                &menu_subject,
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
                            .gap(px(2.0))
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
                                            .xs()
                                            .medium()
                                            .fg(theme.text_app)
                                            .whitespace_nowrap()
                                            .child(message),
                                    )
                                    .child(
                                        div()
                                            .flex_shrink_0()
                                            .mono_xs()
                                            .fg(theme.label_muted)
                                            .child(short_sha.clone()),
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
                                    .xs()
                                    .fg(theme.label_muted)
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
}
