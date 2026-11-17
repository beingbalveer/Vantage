//! Add Repository modal — 448px card built around a local repository path,
//! with a native folder picker and the Vantage group selector.

use gpui::{deferred, div, prelude::*, px, relative, Context, IntoElement, Styled, Window};

use crate::app::{FieldId, ScanPreview, VantageApp, MODAL_KEY_CONTEXT};
use crate::theme::{Theme, FONT_SANS, LH_SM, TEXT_SM};
use crate::ui::{
    checkbox, field_label, icon, keyboard_hint, secondary_button, spinner, FieldVariant, IconSize,
    Txt, HIT_TARGET_PRIMARY,
};

impl VantageApp {
    pub fn render_add_repo_modal(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme;
        let busy = self.modal_is_adding;
        let detected_name = self.modal_detected_repo_name();
        if !self.modal_focus.contains_focused(window, cx) {
            window.focus(&self.field_focus(FieldId::ModalPath));
        }
        let scan_mode = self.modal_scan.is_some() || self.modal_is_scanning;
        let title = if scan_mode {
            "Add Repositories from Folder"
        } else {
            "Add Local Repository"
        };

        let backdrop = div()
            .debug_selector(|| "add-repo-backdrop".to_string())
            .id("add-repo-backdrop")
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .w(relative(1.0))
            .h(relative(1.0))
            .bg(theme.overlay)
            .on_click(cx.listener(|this, _e, _w, cx| this.close_add_modal(cx)));

        let mut card = div()
            .debug_selector(|| "add-repo-card".to_string())
            .id("add-repo-card")
            // Claim the card's area so clicks on labels and padding do not
            // reach the full-screen backdrop (which dismisses the dialog).
            .occlude()
            .relative()
            .flex()
            .flex_col()
            .w(px(448.0))
            .bg(theme.panel)
            .border_1()
            .border_color(theme.divider)
            .rounded(px(12.0))
            .shadow_xl()
            .track_focus(&self.modal_focus)
            .key_context(MODAL_KEY_CONTEXT)
            .on_key_down(cx.listener(Self::handle_modal_key))
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
                            .display_base()
                            .semibold()
                            .fg(theme.text_app)
                            .child(title),
                    )
                    .child(
                        crate::ui::icon_button(&theme, "x", "close-modal", "Close dialog")
                            .on_click(cx.listener(|this, _e, _w, cx| this.close_add_modal(cx))),
                    ),
            );

        let mut form = div().flex().flex_col().gap(px(16.0)).p(px(20.0)).w_full();

        if let Some(error) = self.modal_error.clone() {
            form = form.child(
                div()
                    .px(px(12.0))
                    .py(px(8.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(theme.error_border)
                    .bg(theme.error_bg)
                    .xs()
                    .fg(theme.error_text)
                    .child(error),
            );
        }

        // Path row: the text field plus a compact native folder picker. A scan
        // replaces it with the checkbox preview below.
        if self.modal_is_scanning {
            form = form.child(self.render_scanning());
        } else if let Some(scan) = self.modal_scan.clone() {
            form = form.child(self.render_scan_preview(&scan, cx));
        } else {
            let path_field = self
                .text_field(
                    FieldId::ModalPath,
                    "/Users/you/Documents/GitHub/project",
                    FieldVariant::Form,
                    window,
                    cx,
                )
                .debug_selector(|| "add-repo-path-field".to_string());
            let browse = secondary_button(&theme, "Browse", Some("folder-git-2"))
                .id("modal-browse")
                .debug_selector(|| "modal-browse".to_string())
                .py(px(10.0))
                .when(busy, |button| button.opacity(0.5))
                .when(!busy, |button| {
                    button.on_click(cx.listener(|this, _e, _w, cx| this.browse_for_repo(cx)))
                });

            let hint = match detected_name {
                Some(name) => div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(icon("check", IconSize::Xs, theme.status_green))
                    .child(
                        div()
                            .xs()
                            .fg(theme.text_body)
                            .child(format!("Ready to add {name}")),
                    ),
                None => div()
                    .xs()
                    .fg(theme.text_meta)
                    .child("Vantage reads the name and branch from this folder."),
            };

            form = form.child(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .child(field_label(&theme, "Repository Path"))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .w_full()
                            .child(div().flex_1().min_w(px(0.0)).child(path_field))
                            .child(browse),
                    )
                    .child(div().mt(px(6.0)).child(hint))
                    .child(
                        div()
                            .mt(px(8.0))
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(icon("folder-git-2", IconSize::Xs, theme.accent))
                            .child(
                                div()
                                    .id("modal-scan-folder")
                                    .cursor_pointer()
                                    .xs()
                                    .medium()
                                    .fg(theme.accent)
                                    .hover(|style| style.underline())
                                    .on_click(
                                        cx.listener(|this, _e, _w, cx| this.pick_scan_folder(cx)),
                                    )
                                    .child("Scan Folder for Repositories…"),
                            ),
                    ),
            );
        }

        form = form.child(
            div()
                .flex()
                .flex_col()
                .w_full()
                .child(field_label(&theme, "Group"))
                .child(self.render_modal_group_select(cx)),
        );

        if self.modal_group_choice == "__new__" {
            form = form.child(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .child(field_label(&theme, "New Group Name"))
                    .child(self.text_field(
                        FieldId::ModalCustomGroup,
                        "",
                        FieldVariant::Form,
                        window,
                        cx,
                    )),
            );
        }

        let scan_count = self
            .modal_scan
            .as_ref()
            .map(ScanPreview::selected_count)
            .unwrap_or(0);
        let can_submit = !busy
            && !self.modal_is_scanning
            && if scan_mode {
                scan_count > 0
            } else {
                !self.modal_path.value.trim().is_empty()
            };
        let submit_label = if busy {
            "Adding…".to_string()
        } else if self.modal_is_scanning {
            "Scanning…".to_string()
        } else if scan_mode {
            format!(
                "Add {scan_count} {}",
                if scan_count == 1 {
                    "Repository"
                } else {
                    "Repositories"
                }
            )
        } else {
            "Add Repository".to_string()
        };
        let mut submit = div()
            .id("modal-submit")
            .flex()
            .items_center()
            .justify_center()
            .h(px(HIT_TARGET_PRIMARY))
            .px(px(16.0))
            .rounded(px(8.0))
            .bg(theme.primary_bg)
            .xs()
            .medium()
            .fg(theme.primary_text)
            .child(submit_label);
        if can_submit {
            submit = submit
                .cursor_pointer()
                .hover(move |s| s.bg(theme.primary_bg_hover))
                .on_click(cx.listener(|this, _e, _w, cx| {
                    if this.modal_scan.is_some() {
                        this.submit_scan_add(cx);
                    } else {
                        this.submit_add_modal(cx);
                    }
                }));
        } else {
            submit = submit.opacity(0.5);
        }

        let mut footer = div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .pt(px(8.0))
            .w_full();
        if scan_mode {
            let choose = secondary_button(&theme, "Choose Folder…", Some("folder-git-2"))
                .id("modal-scan-choose")
                .py(px(8.0))
                .when(busy || self.modal_is_scanning, |button| button.opacity(0.5))
                .when(!busy && !self.modal_is_scanning, |button| {
                    button.on_click(cx.listener(|this, _e, _w, cx| this.pick_scan_folder(cx)))
                });
            footer = footer.child(choose);
        }

        form = form.child(
            footer
                .child(keyboard_hint(&theme, "Enter adds · Esc closes"))
                .child(div().flex_1())
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_end()
                        .gap(px(10.0))
                        .child(
                            div()
                                .id("modal-cancel")
                                .flex()
                                .items_center()
                                .justify_center()
                                .h(px(HIT_TARGET_PRIMARY))
                                .px(px(16.0))
                                .rounded(px(8.0))
                                .border_1()
                                .border_color(theme.secondary_border)
                                .cursor_pointer()
                                .xs()
                                .medium()
                                .fg(theme.secondary_text)
                                .hover(move |s| s.bg(theme.secondary_bg_hover))
                                .on_click(cx.listener(|this, _e, _w, cx| this.close_add_modal(cx)))
                                .child("Cancel"),
                        )
                        .child(submit),
                ),
        );

        card = card.child(form);

        div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .w(relative(1.0))
            .h(relative(1.0))
            .flex()
            .items_center()
            .justify_center()
            .p(px(16.0))
            .child(backdrop)
            .child(card)
    }

    fn render_scanning(&self) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .py(px(4.0))
            .child(spinner(
                "modal-folder-scan",
                "refresh-cw",
                IconSize::Sm,
                theme.accent,
            ))
            .child(div().xs().fg(theme.text_body).child("Scanning folder…"))
    }

    fn render_scan_preview(&self, scan: &ScanPreview, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let mut section = div().flex().flex_col().gap(px(10.0)).w_full().child(
            div()
                .flex()
                .flex_col()
                .w_full()
                .child(field_label(&theme, "Folder"))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(icon("folder-git-2", IconSize::Xs, theme.icon_idle))
                        .child(
                            div()
                                .xs()
                                .fg(theme.text_body)
                                .truncate()
                                .child(scan.folder.clone()),
                        ),
                ),
        );

        if scan.scanned == 0 {
            section = section.child(scan_note(
                &theme,
                "No Git repositories found in this folder. Vantage looks one level down, skipping hidden folders, node_modules and target.",
            ));
        } else if scan.candidates.is_empty() {
            section = section.child(scan_note(
                &theme,
                format!(
                    "All {} repositories in this folder are already in your workspace.",
                    scan.scanned
                ),
            ));
        } else {
            let hidden = scan.scanned.saturating_sub(scan.candidates.len());
            if hidden > 0 {
                section = section.child(
                    div()
                        .xs()
                        .fg(theme.text_dim)
                        .child(format!("{hidden} already in your workspace")),
                );
            }

            let all_selected = scan.all_selected();
            section = section.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .w_full()
                    .child(
                        checkbox(&theme, all_selected)
                            .id("modal-scan-select-all")
                            .on_click(cx.listener(|this, _e, _w, cx| {
                                let select = this
                                    .modal_scan
                                    .as_ref()
                                    .is_some_and(|scan| !scan.all_selected());
                                if let Some(scan) = this.modal_scan.as_mut() {
                                    scan.set_all(select);
                                }
                                cx.notify();
                            })),
                    )
                    .child(div().xs().fg(theme.text_body).child("Select all"))
                    .child(div().flex_1())
                    .child(div().xs().fg(theme.text_dim).child(format!(
                        "{} of {} selected",
                        scan.selected_count(),
                        scan.candidates.len()
                    ))),
            );

            let mut list = div()
                .id("modal-scan-list")
                .flex()
                .flex_col()
                .w_full()
                .max_h(px(232.0))
                .overflow_y_scroll()
                .border_1()
                .border_color(theme.divider)
                .rounded(px(8.0));

            for (index, repo) in scan.candidates.iter().enumerate() {
                let path = repo.path.clone();
                list = list.child(
                    div()
                        .id(gpui::SharedString::from(format!(
                            "modal-scan-option-{index}"
                        )))
                        .debug_selector(move || format!("modal-scan-option-{index}"))
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .px(px(10.0))
                        .py(px(7.0))
                        .when(index > 0, |row| {
                            row.border_t_1().border_color(theme.divider)
                        })
                        .cursor_pointer()
                        .hover(move |style| style.bg(theme.row_hover_bg))
                        .on_click(cx.listener(move |this, _e, _w, cx| {
                            if let Some(scan) = this.modal_scan.as_mut() {
                                scan.toggle(&path);
                            }
                            cx.notify();
                        }))
                        .child(checkbox(&theme, repo.selected))
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
                                        .child(repo.name.clone()),
                                )
                                .child(
                                    div()
                                        .xs()
                                        .fg(theme.text_dim)
                                        .truncate()
                                        .child(repo.path.clone()),
                                ),
                        ),
                );
            }
            section = section.child(list);
        }

        section
    }

    fn render_modal_group_select(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let options = self.modal_group_options();
        let current = self.modal_group_choice.clone();
        let current_label = if current == "__new__" {
            "+ New group...".to_string()
        } else {
            current.clone()
        };

        let mut wrapper = div().relative().w_full().child(
            div()
                .id("modal-group-select")
                .flex()
                .items_center()
                .justify_between()
                .w_full()
                .gap(px(8.0))
                .px(px(12.0))
                .py(px(8.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(theme.field_border)
                .bg(theme.field_bg)
                .cursor_pointer()
                .font_family(FONT_SANS)
                .text_size(px(TEXT_SM))
                .line_height(LH_SM)
                .fg(theme.field_text)
                .on_click(cx.listener(|this, _e, _w, cx| {
                    this.modal_group_menu_open = !this.modal_group_menu_open;
                    cx.notify();
                }))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .truncate()
                        .child(current_label),
                )
                .child(icon("chevron-down", IconSize::Sm, theme.icon_idle)),
        );

        if self.modal_group_menu_open {
            let mut menu = div()
                .absolute()
                .top(px(42.0))
                .left(px(0.0))
                .w_full()
                .flex()
                .flex_col()
                .p(px(4.0))
                .bg(theme.panel)
                .border_1()
                .border_color(theme.divider)
                .rounded(px(8.0))
                .shadow_lg();

            for (index, option) in options.into_iter().enumerate() {
                let selected = option == current;
                let value = option.clone();
                let label = if option == "__new__" {
                    "+ New group...".to_string()
                } else {
                    option
                };
                menu = menu.child(
                    div()
                        .id(gpui::SharedString::from(format!(
                            "modal-group-option-{index}"
                        )))
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(8.0))
                        .px(px(8.0))
                        .py(px(8.0))
                        .rounded(px(4.0))
                        .cursor_pointer()
                        .xs()
                        .medium()
                        .fg(theme.text_soft)
                        .when(selected, |d| d.bg(theme.nav_active_bg))
                        .hover(move |s| s.bg(theme.row_hover_bg))
                        .on_click(cx.listener(move |this, _e, _w, cx| {
                            this.modal_group_choice = value.clone();
                            this.modal_group_menu_open = false;
                            this.modal_error = None;
                            cx.notify();
                        }))
                        .child(label)
                        .when(selected, |d| {
                            d.child(icon("check", IconSize::Xs, theme.accent))
                        }),
                );
            }
            // Deferred so the menu paints above the footer, and its hitboxes
            // win over the controls it overlaps.
            wrapper = wrapper.child(deferred(menu));
        }

        wrapper
    }

    pub fn modal_group_options(&self) -> Vec<String> {
        let mut options = crate::app::group_choices(&self.groups);
        options.push("__new__".to_string());
        options
    }
}

fn scan_note(theme: &Theme, text: impl Into<String>) -> impl IntoElement {
    div()
        .flex()
        .items_start()
        .gap(px(8.0))
        .px(px(10.0))
        .py(px(9.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(theme.divider)
        .bg(theme.field_bg)
        .child(icon("alert-circle", IconSize::Xs, theme.text_dim))
        .child(div().xs().fg(theme.text_dim).child(text.into()))
}

#[cfg(test)]
mod tests {
    use gpui::{point, px, size, AppContext, Modifiers, TestAppContext};
    use std::path::{Path, PathBuf};

    use crate::app::{ScanPreview, VantageApp};
    use crate::root::RootView;

    fn draw(cx: &mut gpui::VisualTestContext) {
        cx.update(|window, cx| {
            window.draw(cx).clear();
        });
    }

    #[gpui::test]
    fn long_path_stays_inside_the_modal_and_scrolls_to_the_caret(cx: &mut TestAppContext) {
        let app = cx.new(VantageApp::new);
        let (_root, cx) = cx.add_window_view(|_window, _cx| RootView::new(app.clone()));

        let long_path = format!(
            "/private/var/folders/{}/{}",
            "x".repeat(120),
            "répo-日本語-with-a-long-name"
        );
        cx.update(|window, cx| {
            app.update(cx, |app, cx| {
                app.open_add_modal(cx);
                app.modal_path.set(long_path.clone());
            });
            window.draw(cx).clear();
        });
        cx.run_until_parked();
        draw(cx);

        // At both the minimum and a wide window the field itself has to stay
        // inside the dialog and clear of Browse, whatever the path length.
        for (width, height) in [(1040.0, 640.0), (1470.0, 900.0)] {
            cx.simulate_resize(size(px(width), px(height)));
            cx.run_until_parked();
            draw(cx);

            let card = cx.debug_bounds("add-repo-card").expect("modal card bounds");
            let field = cx
                .debug_bounds("add-repo-path-field")
                .expect("path field bounds");
            let browse = cx.debug_bounds("modal-browse").expect("browse bounds");
            assert!(
                field.left() >= card.left() && field.right() <= card.right(),
                "path field {field:?} must stay inside card {card:?}"
            );
            assert!(
                field.right() <= browse.left(),
                "path field {field:?} must not paint over Browse {browse:?}"
            );
            assert!(
                browse.right() <= card.right(),
                "Browse {browse:?} must stay inside card {card:?}"
            );
        }

        // Focus the field: its content must scroll left far enough to keep the
        // caret (parked at the end) inside the visible text area.
        cx.simulate_resize(size(px(1040.0), px(640.0)));
        cx.run_until_parked();
        draw(cx);
        let field = cx
            .debug_bounds("add-repo-path-field")
            .expect("path field bounds");
        cx.simulate_click(field.center(), Modifiers::none());
        cx.run_until_parked();
        draw(cx);

        let offset_at_end = cx.update(|_window, cx| app.read(cx).modal_path.scroll_offset_x());
        assert!(
            offset_at_end < px(0.0),
            "the focused field should scroll to reveal the caret, offset {offset_at_end:?}"
        );

        // Home collapses the caret to index zero; the next frame brings the
        // beginning of the path back into view.
        app.update(cx, |app, _cx| app.modal_path.home());
        draw(cx);
        let offset_at_home = cx.update(|_window, cx| app.read(cx).modal_path.scroll_offset_x());
        assert_eq!(
            offset_at_home,
            px(0.0),
            "scrolling back to the start must reset the offset"
        );
    }

    #[gpui::test]
    fn dialog_ignores_clicks_on_its_own_body(cx: &mut TestAppContext) {
        let app = cx.new(VantageApp::new);
        let (_root, cx) = cx.add_window_view(|_window, _cx| RootView::new(app.clone()));

        cx.update(|window, cx| {
            app.update(cx, |app, cx| app.open_add_modal(cx));
            window.draw(cx).clear();
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear();
        });

        let card = cx.debug_bounds("add-repo-card").expect("modal card bounds");
        // Card padding: inside the dialog, but not on an interactive child.
        cx.simulate_click(
            point(card.origin.x + px(6.0), card.center().y),
            Modifiers::none(),
        );
        assert!(
            cx.update(|_window, cx| app.read(cx).is_add_modal_open),
            "a click on the dialog body must not dismiss it"
        );

        // The full-screen backdrop still dismisses when clicked directly.
        cx.simulate_click(point(px(2.0), px(2.0)), Modifiers::none());
        assert!(
            !cx.update(|_window, cx| app.read(cx).is_add_modal_open),
            "a click outside the dialog should dismiss it"
        );
    }

    #[gpui::test]
    fn add_modal_offers_ungrouped_and_defaults_to_it(cx: &mut TestAppContext) {
        let app = cx.new(VantageApp::new);
        app.update(cx, |app, cx| {
            app.groups = vec!["SE".to_string()];
            app.active_group = "All".to_string();
            app.open_add_modal(cx);

            assert_eq!(app.modal_group_choice, "Ungrouped");
            assert_eq!(
                app.modal_group_options(),
                vec![
                    "SE".to_string(),
                    "Ungrouped".to_string(),
                    "__new__".to_string()
                ],
                "the selector must offer Ungrouped even when the state omits it"
            );
        });
    }

    #[gpui::test]
    fn add_modal_defaults_to_the_group_being_viewed(cx: &mut TestAppContext) {
        let app = cx.new(VantageApp::new);
        app.update(cx, |app, cx| {
            app.groups = vec!["SE".to_string()];
            app.active_group = "SE".to_string();
            app.open_add_modal(cx);

            assert_eq!(
                app.modal_group_choice, "SE",
                "adding while a group is selected should use that group"
            );
        });
    }

    #[gpui::test]
    fn escape_from_the_path_field_closes_the_dialog(cx: &mut TestAppContext) {
        use crate::app::FieldId;

        let app = cx.new(VantageApp::new);
        let (_root, cx) = cx.add_window_view(|_window, _cx| RootView::new(app.clone()));

        cx.update(|window, cx| {
            app.update(cx, |app, cx| app.open_add_modal(cx));
            let focus = app.read(cx).field_focus(FieldId::ModalPath);
            window.focus(&focus);
            window.draw(cx).clear();
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear();
        });

        cx.simulate_keystrokes("escape");
        assert!(
            !cx.update(|_window, cx| app.read(cx).is_add_modal_open),
            "Esc from the focused path field should close the dialog"
        );
    }

    #[gpui::test]
    fn scan_preview_row_toggles_its_repository(cx: &mut TestAppContext) {
        let app = cx.new(VantageApp::new);
        let (_root, cx) = cx.add_window_view(|_window, _cx| RootView::new(app.clone()));

        cx.update(|window, cx| {
            app.update(cx, |app, cx| {
                app.open_add_modal(cx);
                app.modal_scan = Some(ScanPreview::new(
                    Path::new("/tmp/workspace"),
                    2,
                    vec![
                        PathBuf::from("/tmp/workspace/one"),
                        PathBuf::from("/tmp/workspace/two"),
                    ],
                ));
                cx.notify();
            });
            window.draw(cx).clear();
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear();
        });

        let row = cx
            .debug_bounds("modal-scan-option-0")
            .expect("first scanned repository");
        cx.simulate_click(row.center(), Modifiers::none());

        assert!(
            cx.update(|_window, cx| {
                !app.read(cx).modal_scan.as_ref().unwrap().candidates[0].selected
            }),
            "clicking a scanned row unticks it"
        );
        assert!(
            cx.update(|_window, cx| {
                app.read(cx).modal_scan.as_ref().unwrap().candidates[1].selected
            }),
            "the other row keeps its tick"
        );
    }
}
