//! Main viewport shell: `max-w-[1440px] mx-auto px-6 py-5`, no extra headings.

use gpui::{div, prelude::*, px, Context, IntoElement, Styled, Window};

use crate::app::VantageApp;
use crate::model::NavSection;
use crate::ui::{primary_button, Txt};

/// Errors listed before the rest collapse into a "+N more" line.
const MAX_SCAN_ERRORS: usize = 5;

impl VantageApp {
    pub fn render_viewport(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let banner = self.render_scan_error_banner();
        let content = if self.repos.is_empty() {
            self.render_empty_repos(cx).into_any_element()
        } else {
            match self.active_section {
                NavSection::Changes => self.render_pending_changes(cx).into_any_element(),
                NavSection::Overview | NavSection::Attention => {
                    self.render_repo_table(cx).into_any_element()
                }
            }
        };

        div()
            .flex_1()
            .min_w(px(0.0))
            .min_h(px(0.0))
            .id(gpui::SharedString::from(format!(
                "viewport-scroll-{}-{:.3}",
                self.settings.right_panel_open, self.settings.right_panel_fraction
            )))
            .overflow_y_scroll()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .max_w(px(1440.0))
                    .mx_auto()
                    .px(px(24.0))
                    .py(px(20.0))
                    .gap(px(16.0))
                    .when_some(banner, |shell, banner| shell.child(banner))
                    .child(content),
            )
    }

    /// Scan failures from the last refresh, shown above whichever section is
    /// active (including the empty state).
    fn render_scan_error_banner(&self) -> Option<gpui::AnyElement> {
        if self.scan_errors.is_empty() {
            return None;
        }
        let theme = self.theme;
        let mut banner = div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .w_full()
            .px(px(16.0))
            .py(px(12.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(theme.error_border)
            .bg(theme.error_bg)
            .child(
                div()
                    .sm()
                    .semibold()
                    .fg(theme.error_text)
                    .child("Some repositories could not be scanned"),
            );

        for error in self.scan_errors.iter().take(MAX_SCAN_ERRORS) {
            banner = banner.child(div().xs().fg(theme.error_text).child(error.clone()));
        }
        if self.scan_errors.len() > MAX_SCAN_ERRORS {
            banner = banner.child(div().xs().fg(theme.error_text).child(format!(
                "+{} more",
                self.scan_errors.len() - MAX_SCAN_ERRORS
            )));
        }

        Some(banner.into_any_element())
    }

    /// Empty inventory: centered card with the add-repository action, or the
    /// scanning copy while the first scan is still in flight.
    fn render_empty_repos(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let card = div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .w_full()
            .px(px(24.0))
            .py(px(72.0))
            .bg(theme.panel)
            .border_1()
            .border_color(theme.divider)
            .rounded(px(12.0));

        if self.is_scanning {
            return card.child(
                div()
                    .sm()
                    .fg(theme.label_muted)
                    .child("Scanning repositories…"),
            );
        }

        card.child(
            div()
                .display_lg()
                .semibold()
                .fg(theme.text_app)
                .child("No repositories yet"),
        )
        .child(
            div()
                .mt(px(4.0))
                .xs()
                .fg(theme.label_muted)
                .child("Add a local Git repository to get started."),
        )
        .child(
            div().mt(px(16.0)).child(
                primary_button(&theme, "Add Repository", Some("plus"))
                    .id("empty-add-repository")
                    .on_click(cx.listener(|this, _e, _w, cx| this.open_add_modal(cx))),
            ),
        )
    }
}
