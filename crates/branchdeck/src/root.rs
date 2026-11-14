//! Thin window root.
//!
//! GPUI 0.2.2 registers a window invalidator only for entities that are read
//! while drawing. A root view that owns the window's state therefore never
//! invalidates the window from `cx.notify()`. Rendering the dashboard as a
//! child entity restores normal invalidation.

use gpui::{Context, Entity, IntoElement, Render, Window};

use crate::app::BranchDeckApp;

pub struct RootView {
    app: Entity<BranchDeckApp>,
}

impl RootView {
    pub fn new(app: Entity<BranchDeckApp>) -> Self {
        Self { app }
    }
}

impl Render for RootView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // Reading the child entity during draw registers its invalidator, so
        // `cx.notify()` inside `BranchDeckApp` repaints the window.
        self.app.clone()
    }
}
