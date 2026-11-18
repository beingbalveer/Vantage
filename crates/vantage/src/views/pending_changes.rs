//! Pending Changes — per-repo grouped file lists.

use gpui::{div, prelude::*, px, Context, IntoElement, Styled};

use crate::app::VantageApp;
use crate::model::{file_status_code, FileChangeStatus};
use crate::theme::{FONT_MONO, LH_XS, TEXT_XS};
use crate::ui::Txt;
use crate::ui::{icon, IconSize};

struct PendingFile {
    path: String,
    code: &'static str,
    additions: u32,
    deletions: u32,
    staged: bool,
    status: FileChangeStatus,
}

struct PendingRepo {
    id: String,
    name: String,
    branch: String,
    additions: u32,
    deletions: u32,
    files: Vec<PendingFile>,
}

impl VantageApp {
    pub fn render_pending_changes(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;

        let mut repos: Vec<PendingRepo> = Vec::new();
        for repo in self.grouped_repos() {
            if repo.uncommitted_files.is_empty() {
                continue;
            }
            let files: Vec<PendingFile> = repo
                .uncommitted_files
                .iter()
                .map(|f| PendingFile {
                    path: f.path.clone(),
                    code: file_status_code(f.status),
                    additions: f.additions,
                    deletions: f.deletions,
                    staged: f.staged,
                    status: f.status,
                })
                .collect();
            repos.push(PendingRepo {
                id: repo.id.clone(),
                name: repo.name.clone(),
                branch: repo.current_branch.clone(),
                additions: repo.uncommitted_files.iter().map(|f| f.additions).sum(),
                deletions: repo.uncommitted_files.iter().map(|f| f.deletions).sum(),
                files,
            });
        }

        if repos.is_empty() {
            return div()
                .flex()
                .flex_col()
                .items_center()
                .w_full()
                .p(px(48.0))
                .bg(theme.panel)
                .border_1()
                .border_color(theme.divider)
                .rounded(px(12.0))
                .child(
                    div()
                        .display_sm()
                        .semibold()
                        .fg(theme.text_app)
                        .child("All working trees are clean"),
                )
                .child(
                    div()
                        .mt(px(4.0))
                        .xs()
                        .fg(theme.label_muted)
                        .child("There are no uncommitted or untracked changes across your active repositories."),
                );
        }

        let mut stack = div().flex().flex_col().gap(px(16.0)).w_full();

        for repo in repos {
            let repo_id = repo.id.clone();
            let mut card = div()
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
                        .items_center()
                        .justify_between()
                        .gap(px(12.0))
                        .px(px(20.0))
                        .py(px(14.0))
                        .bg(theme.table_head_bg)
                        .border_b_1()
                        .border_color(theme.divider)
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .sm()
                                .child(
                                    div()
                                        .display_sm()
                                        .semibold()
                                        .fg(theme.text_app)
                                        .child(repo.name.clone()),
                                )
                                .child(div().fg(theme.text_dim).child("·"))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(4.0))
                                        .mono_xs()
                                        .fg(theme.text_body)
                                        .child(icon("git-branch", IconSize::Sm, theme.icon_idle))
                                        .child(repo.branch.clone()),
                                )
                                .child(div().fg(theme.text_dim).child("·"))
                                .child(
                                    div()
                                        .mono_xs()
                                        .fg(theme.add_text)
                                        .child(format!("+{}", repo.additions)),
                                )
                                .child(
                                    div()
                                        .mono_xs()
                                        .fg(theme.del_text)
                                        .child(format!("-{}", repo.deletions)),
                                ),
                        )
                        .child(
                            div()
                                .id(gpui::SharedString::from(format!(
                                    "open-inspector-{}",
                                    repo.id
                                )))
                                .flex()
                                .items_center()
                                .justify_center()
                                .gap(px(4.0))
                                .h(px(crate::ui::HIT_TARGET_MIN))
                                .px(px(8.0))
                                .rounded(px(6.0))
                                .flex_shrink_0()
                                .whitespace_nowrap()
                                .cursor_pointer()
                                .xs()
                                .medium()
                                .fg(theme.accent_text)
                                .hover(|s| s.opacity(0.75))
                                .on_click(cx.listener(move |this, _e, _w, cx| {
                                    this.select_repo(repo_id.clone(), cx);
                                }))
                                .child("Open inspector")
                                .child(icon("chevron-right", IconSize::Sm, theme.accent_text)),
                        ),
                );

            let mut list = div().flex().flex_col().w_full();
            for (index, file) in repo.files.iter().enumerate() {
                let color = match file.status {
                    FileChangeStatus::Modified => theme.status_amber,
                    FileChangeStatus::Added => theme.add_text,
                    FileChangeStatus::Deleted => theme.del_text,
                    FileChangeStatus::Untracked => theme.nav_idle_text,
                    FileChangeStatus::Conflicted => theme.del_text,
                };

                list = list.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(16.0))
                        .w_full()
                        .px(px(20.0))
                        .py(px(12.0))
                        .xs()
                        .when(index > 0, |d| d.border_t_1().border_color(theme.row_border))
                        .hover(move |s| s.bg(theme.row_hover_bg))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(12.0))
                                .min_w(px(0.0))
                                .child(
                                    div()
                                        .w(px(16.0))
                                        .flex_shrink_0()
                                        .text_center()
                                        .font_family(FONT_MONO)
                                        .text_size(crate::theme::scaled_px(
                                            TEXT_XS,
                                            crate::theme::TextArea::Interface,
                                        ))
                                        .line_height(LH_XS)
                                        .semibold()
                                        .fg(color)
                                        .child(file.code),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w(px(0.0))
                                        .font_family(FONT_MONO)
                                        .text_size(crate::theme::scaled_px(
                                            TEXT_XS,
                                            crate::theme::TextArea::Interface,
                                        ))
                                        .line_height(LH_XS)
                                        .fg(theme.text_soft)
                                        .truncate()
                                        .child(file.path.clone()),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(16.0))
                                .flex_shrink_0()
                                .child(div().xs().fg(theme.label_muted).child(if file.staged {
                                    "Staged"
                                } else {
                                    "Unstaged"
                                }))
                                .child(
                                    div()
                                        .w(px(80.0))
                                        .flex()
                                        .justify_end()
                                        .font_family(FONT_MONO)
                                        .text_size(crate::theme::scaled_px(
                                            TEXT_XS,
                                            crate::theme::TextArea::Interface,
                                        ))
                                        .line_height(LH_XS)
                                        .child(
                                            div()
                                                .fg(theme.add_text)
                                                .child(format!("+{}", file.additions)),
                                        )
                                        .child(div().mx(px(4.0)).fg(theme.text_dim).child("/"))
                                        .child(
                                            div()
                                                .fg(theme.del_text)
                                                .child(format!("-{}", file.deletions)),
                                        ),
                                ),
                        ),
                );
            }

            card = card.child(list);
            stack = stack.child(card);
        }

        stack
    }
}
