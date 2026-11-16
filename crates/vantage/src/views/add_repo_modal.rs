//! Add Repository modal — 448px card built around a local repository path,
//! with a native folder picker and the Vantage group selector.

use gpui::{deferred, div, prelude::*, px, relative, Context, IntoElement, Styled, Window};

use crate::app::{FieldId, VantageApp};
use crate::theme::{FONT_SANS, LH_SM, TEXT_SM};
use crate::ui::{field_label, keyboard_hint, secondary_button, FieldVariant, Txt};
use crate::ui::{icon, IconSize, HIT_TARGET_PRIMARY};

impl VantageApp {
    pub fn render_add_repo_modal(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme;
        let busy = self.modal_is_adding;
        let detected_name = self.modal_detected_repo_name();

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
                            .child("Add Local Repository"),
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

        // Path row: the text field plus a compact native folder picker.
        let path_field = self.text_field(
            FieldId::ModalPath,
            "/Users/you/Documents/GitHub/project",
            FieldVariant::Form,
            window,
            cx,
        );
        let browse = secondary_button(&theme, "Browse", Some("folder-git-2"))
            .id("modal-browse")
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
                .child(div().mt(px(6.0)).child(hint)),
        );

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

        let can_submit = !busy && !self.modal_path.value.trim().is_empty();
        let submit_label = if busy { "Adding…" } else { "Add Repository" };
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
                .on_click(cx.listener(|this, _e, _w, cx| this.submit_add_modal(cx)));
        } else {
            submit = submit.opacity(0.5);
        }

        form = form.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .pt(px(8.0))
                .child(keyboard_hint(&theme, "Enter adds · Esc closes"))
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
        let mut options: Vec<String> = self.groups.clone();
        if options.is_empty() {
            options.push("Ungrouped".to_string());
        }
        options.push("__new__".to_string());
        options
    }
}

#[cfg(test)]
mod tests {
    use gpui::{point, px, AppContext, Modifiers, TestAppContext};

    use crate::app::VantageApp;
    use crate::root::RootView;

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
}
