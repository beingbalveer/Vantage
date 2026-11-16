//! Modal dialogs owned by the application shell: stashes, repository
//! creation, clone, destructive confirms and branch prompts.

use gpui::{div, prelude::*, px, Context, IntoElement, MouseButton, Window};

use super::{FieldId, VantageApp};
use crate::ui::Txt;

/// The stash list, opened as a dialog.
#[derive(Clone, Debug)]
pub struct StashModal {
    pub repo_id: String,
    pub entries: Vec<vantage_core::state::StashEntry>,
}

/// A destructive action waiting for confirmation.
#[derive(Clone, Debug)]
pub struct ConfirmPrompt {
    pub title: String,
    pub body: String,
    pub label: &'static str,
    pub action: ConfirmAction,
}

#[derive(Clone, Debug)]
pub enum ConfirmAction {
    DiscardRepository { repo_id: String, paths: Vec<String> },
    ForgetRepository { repo_id: String },
    MergeBranch { repo_id: String, target: String },
    UpdateFromDefault { repo_id: String },
    ResetToCommit { repo_id: String, sha: String },
}

/// An in-progress branch action, answered by the inline row in the Branches tab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BranchPrompt {
    /// Create a branch from an existing one.
    Create { base: String },
    /// Rename the current branch.
    Rename { from: String },
    /// Delete a branch after a confirm.
    Delete { name: String },
}

impl VantageApp {
    /// The stash list, with restore and drop per entry.
    pub(super) fn render_stash_modal(&self, cx: &mut Context<Self>) -> gpui::Div {
        let theme = self.theme;
        let Some(modal) = self.stash_modal.clone() else {
            return div();
        };

        let mut list = div().flex().flex_col().gap(px(2.0)).w_full();
        if modal.entries.is_empty() {
            list = list.child(
                div()
                    .py(px(10.0))
                    .xs()
                    .fg(theme.label_muted)
                    .child("No stashes in this repository."),
            );
        }
        for entry in &modal.entries {
            let index = entry.index;
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(10.0))
                    .w_full()
                    .px(px(8.0))
                    .py(px(6.0))
                    .rounded(px(6.0))
                    .hover(move |s| s.bg(theme.row_hover_bg))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w(px(0.0))
                            .child(
                                div()
                                    .xs()
                                    .medium()
                                    .fg(theme.text_app)
                                    .truncate()
                                    .child(entry.subject.clone()),
                            )
                            .child(
                                div()
                                    .mono_xs()
                                    .fg(theme.label_muted)
                                    .child(format!("stash@{{{index}}} \u{00b7} {}", entry.date)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .flex_shrink_0()
                            .child(
                                div()
                                    .id(gpui::SharedString::from(format!("stash-pop-{index}")))
                                    .px(px(6.0))
                                    .py(px(2.0))
                                    .rounded(px(4.0))
                                    .cursor_pointer()
                                    .xs()
                                    .medium()
                                    .fg(theme.accent_text)
                                    .hover(move |s| s.bg(theme.nav_hover_bg))
                                    .child("Restore")
                                    .on_click(cx.listener(move |this, _e, _w, cx| {
                                        this.pop_stash_entry(index, cx);
                                    })),
                            )
                            .child(
                                div()
                                    .id(gpui::SharedString::from(format!("stash-drop-{index}")))
                                    .px(px(6.0))
                                    .py(px(2.0))
                                    .rounded(px(4.0))
                                    .cursor_pointer()
                                    .xs()
                                    .fg(theme.label_muted)
                                    .hover(move |s| s.fg(theme.status_red).underline())
                                    .child("Drop")
                                    .on_click(cx.listener(move |this, _e, _w, cx| {
                                        this.drop_stash_entry(index, cx);
                                    })),
                            ),
                    ),
            );
        }

        let card = div()
            .w(px(460.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .p(px(20.0))
            .bg(theme.panel)
            .border_1()
            .border_color(theme.divider)
            .rounded(px(12.0))
            .shadow_lg()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_this, _e, _w, cx| cx.stop_propagation()),
            )
            .child(div().sm().medium().fg(theme.text_app).child("Stashes"))
            .child(list)
            .child(
                div().flex().items_center().justify_end().child(
                    crate::ui::secondary_button(&theme, "Close", None)
                        .id("stash-close")
                        .on_click(cx.listener(|this, _e, _w, cx| this.close_stash_modal(cx))),
                ),
            );

        div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme.overlay)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _e, _w, cx| this.close_stash_modal(cx)),
            )
            .child(card)
    }

    /// Dialog for starting a repository: name, folder, and a starter ignore file.
    pub(super) fn render_new_repo_modal(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let theme = self.theme;
        if !self.new_repo_name_focus.is_focused(window) {
            window.focus(&self.new_repo_name_focus);
        }

        let label = if self.is_creating_repo {
            "Creating…"
        } else {
            "Create repository"
        };
        let mut confirm = crate::ui::accent_button(&theme, label)
            .id("new-repo-confirm")
            .on_click(cx.listener(|this, _e, _w, cx| this.submit_new_repo(cx)));
        if self.is_creating_repo {
            confirm = confirm.opacity(0.6);
        }

        let ignore = self.new_repo_gitignore;
        let toggle = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                crate::ui::checkbox(&theme, ignore)
                    .id("new-repo-gitignore")
                    .on_click(cx.listener(|this, _e, _w, cx| {
                        this.new_repo_gitignore = !this.new_repo_gitignore;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .xs()
                    .fg(theme.text_body)
                    .child("Add a starter .gitignore"),
            );

        let card = div()
            .w(px(460.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .p(px(20.0))
            .bg(theme.panel)
            .border_1()
            .border_color(theme.divider)
            .rounded(px(12.0))
            .shadow_lg()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_this, _e, _w, cx| cx.stop_propagation()),
            )
            .child(
                div()
                    .sm()
                    .medium()
                    .fg(theme.text_app)
                    .child("New repository"),
            )
            .child(div().xs().fg(theme.label_muted).child(
                "A folder is created inside the destination, initialised as a Git repository and \
                 added to the list.",
            ))
            .child(self.text_field(
                FieldId::NewRepoName,
                "Repository name",
                crate::ui::FieldVariant::Form,
                window,
                cx,
            ))
            .child(self.text_field(
                FieldId::NewRepoParent,
                "Destination folder",
                crate::ui::FieldVariant::Form,
                window,
                cx,
            ))
            .child(toggle)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        crate::ui::secondary_button(&theme, "Cancel", None)
                            .id("new-repo-cancel")
                            .on_click(
                                cx.listener(|this, _e, _w, cx| this.close_new_repo_modal(cx)),
                            ),
                    )
                    .child(confirm),
            );

        div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme.overlay)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _e, _w, cx| this.close_new_repo_modal(cx)),
            )
            .child(card)
    }

    /// Dialog for cloning a repository: where from, and into which folder.
    pub(super) fn render_clone_modal(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let theme = self.theme;
        if !self.clone_url_focus.is_focused(window) {
            window.focus(&self.clone_url_focus);
        }

        let label = if self.is_cloning {
            "Cloning…"
        } else {
            "Clone"
        };
        let mut confirm = crate::ui::accent_button(&theme, label)
            .id("clone-confirm")
            .on_click(cx.listener(|this, _e, _w, cx| this.submit_clone(cx)));
        if self.is_cloning {
            confirm = confirm.opacity(0.6);
        }

        let card = div()
            .w(px(460.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .p(px(20.0))
            .bg(theme.panel)
            .border_1()
            .border_color(theme.divider)
            .rounded(px(12.0))
            .shadow_lg()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_this, _e, _w, cx| cx.stop_propagation()),
            )
            .child(
                div()
                    .sm()
                    .medium()
                    .fg(theme.text_app)
                    .child("Clone a repository"),
            )
            .child(div().xs().fg(theme.label_muted).child(
                "A URL, or a GitHub shorthand like owner/repo. The folder is created inside the \
                 destination.",
            ))
            .child(self.text_field(
                FieldId::CloneUrl,
                "https://github.com/owner/repo.git",
                crate::ui::FieldVariant::Form,
                window,
                cx,
            ))
            .child(self.text_field(
                FieldId::CloneParent,
                "Destination folder",
                crate::ui::FieldVariant::Form,
                window,
                cx,
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        crate::ui::secondary_button(&theme, "Cancel", None)
                            .id("clone-cancel")
                            .on_click(cx.listener(|this, _e, _w, cx| this.close_clone_modal(cx))),
                    )
                    .child(confirm),
            );

        div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme.overlay)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _e, _w, cx| this.close_clone_modal(cx)),
            )
            .child(card)
    }

    /// Small dialog for a destructive action: what will happen, and one way out.
    pub(super) fn render_confirm_modal(&self, cx: &mut Context<Self>) -> gpui::Div {
        let theme = self.theme;
        let Some(prompt) = self.confirm.clone() else {
            return div();
        };

        let card = div()
            .w(px(420.0))
            .flex()
            .flex_col()
            .gap(px(14.0))
            .p(px(20.0))
            .bg(theme.panel)
            .border_1()
            .border_color(theme.divider)
            .rounded(px(12.0))
            .shadow_lg()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_this, _e, _w, cx| cx.stop_propagation()),
            )
            .child(div().sm().medium().fg(theme.text_app).child(prompt.title))
            .child(div().xs().fg(theme.text_body).child(prompt.body))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        crate::ui::secondary_button(&theme, "Cancel", None)
                            .id("confirm-cancel")
                            .on_click(cx.listener(|this, _e, _w, cx| this.cancel_confirm(cx))),
                    )
                    .child(
                        crate::ui::accent_button(&theme, prompt.label)
                            .id("confirm-ok")
                            .bg(theme.status_red)
                            .hover(move |s| s.bg(theme.del_text))
                            .on_click(cx.listener(|this, _e, _w, cx| this.submit_confirm(cx))),
                    ),
            );

        div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme.overlay)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _e, _w, cx| this.cancel_confirm(cx)),
            )
            .child(card)
    }

    /// Small dialog for a branch action: a name to type, or a delete to confirm.
    pub(super) fn render_branch_modal(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        let Some(prompt) = self.branch_prompt.clone() else {
            return div().into_any_element();
        };

        // The dialog owns the keyboard while it is open.
        if !self.branch_focus.is_focused(window) {
            window.focus(&self.branch_focus);
        }

        let (title, action_label, destructive) = match &prompt {
            BranchPrompt::Create { base } => {
                (format!("New branch from {base}"), "Create branch", false)
            }
            BranchPrompt::Rename { from } => (format!("Rename {from}"), "Rename", false),
            BranchPrompt::Delete { name } => (format!("Delete {name}?"), "Delete", true),
        };

        let mut body = div().flex().flex_col().gap(px(12.0)).w_full();
        match &prompt {
            BranchPrompt::Delete { name } => {
                body = body.child(div().xs().fg(theme.text_body).child(format!(
                    "{name} will be removed. Git refuses when the branch has unmerged work."
                )))
            }
            _ => {
                body = body.child(self.text_field(
                    FieldId::BranchName,
                    "Branch name",
                    crate::ui::FieldVariant::Form,
                    window,
                    cx,
                ))
            }
        }

        let mut confirm = crate::ui::accent_button(&theme, action_label)
            .id("branch-modal-confirm")
            .on_click(cx.listener(|this, _e, _w, cx| this.submit_branch_prompt(cx)));
        if destructive {
            confirm = confirm
                .bg(theme.status_red)
                .hover(move |s| s.bg(theme.del_text));
        }

        let card = div()
            .w(px(420.0))
            .flex()
            .flex_col()
            .gap(px(14.0))
            .p(px(20.0))
            .bg(theme.panel)
            .border_1()
            .border_color(theme.divider)
            .rounded(px(12.0))
            .shadow_lg()
            // A click inside the dialog is not a click on the backdrop.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_this, _e, _w, cx| cx.stop_propagation()),
            )
            .child(div().sm().medium().fg(theme.text_app).child(title))
            .child(body)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        crate::ui::secondary_button(&theme, "Cancel", None)
                            .id("branch-modal-cancel")
                            .on_click(
                                cx.listener(|this, _e, _w, cx| this.cancel_branch_prompt(cx)),
                            ),
                    )
                    .child(confirm),
            );

        // One absolute layer covers the window: dim it, centre the card in it,
        // and dismiss on any press that the card itself did not take.
        div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme.overlay)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _e, _w, cx| this.cancel_branch_prompt(cx)),
            )
            .child(card)
            .into_any_element()
    }
}
