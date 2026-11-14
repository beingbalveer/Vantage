//! Main viewport shell: `max-w-[1440px] mx-auto px-6 py-5`, no extra headings.

use gpui::{div, prelude::*, px, Context, IntoElement, Styled, Window};

use crate::app::BranchDeckApp;
use crate::model::NavSection;

impl BranchDeckApp {
    pub fn render_viewport(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let content = match self.active_section {
            NavSection::Branches => self.render_branch_matrix(window, cx).into_any_element(),
            NavSection::Changes => self.render_pending_changes(cx).into_any_element(),
            NavSection::Overview | NavSection::Attention => {
                self.render_repo_table(window, cx).into_any_element()
            }
        };

        div()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .id("viewport-scroll")
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
                    .child(content),
            )
    }
}
