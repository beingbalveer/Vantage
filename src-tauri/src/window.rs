//! Window geometry persistence and restoration.
//!
//! The frontend owns the layout payload (`layout_set`) and always sends
//! `windowBounds: null` because it cannot see native window geometry. Window
//! bounds are therefore written from this module's own path (move/resize
//! events and app quit), never from `layout_set`. See
//! [`apply_layout_window_bounds`] for the merge rule that keeps a stray `null`
//! from clobbering the stored geometry.

use crate::cmd::AppState;
use crate::state::{Layout, WindowBounds};
use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, Runtime, WebviewWindow, Window};

/// Mirrors `minWidth` / `minHeight` in `tauri.conf.json`.
pub const MIN_WINDOW_WIDTH: u32 = 900;
pub const MIN_WINDOW_HEIGHT: u32 = 560;

/// A saved window must show at least this much of itself on a monitor before
/// we consider that monitor a valid home. Otherwise a window saved on a
/// disconnected display falls back to the primary monitor.
const MIN_VISIBLE_WIDTH: i64 = 96;
const MIN_VISIBLE_HEIGHT: i64 = 64;

/// Debounce window move/resize events so dragging does not write state.json
/// on every frame.
const SAVE_DEBOUNCE: Duration = Duration::from_millis(400);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonitorRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl MonitorRect {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    fn intersection(&self, bounds: &WindowBounds) -> (i64, i64) {
        let left = (bounds.x as i64).max(self.x as i64);
        let top = (bounds.y as i64).max(self.y as i64);
        let right = (bounds.x as i64 + bounds.width as i64).min(self.x as i64 + self.width as i64);
        let bottom =
            (bounds.y as i64 + bounds.height as i64).min(self.y as i64 + self.height as i64);
        ((right - left).max(0), (bottom - top).max(0))
    }
}

/// Keep a saved geometry usable on the current monitor configuration.
///
/// - A geometry that does not overlap any monitor (e.g. saved on a display
///   that is no longer connected) falls back to the primary monitor.
/// - Width/height are clamped to the target monitor and the configured minimum
///   window size.
/// - The final position is clamped so the whole window is visible.
///
/// Returns `None` when no monitors are available; callers should then leave the
/// platform default geometry alone.
pub fn sanitize_window_bounds(
    saved: &WindowBounds,
    monitors: &[MonitorRect],
    primary: Option<&MonitorRect>,
) -> Option<WindowBounds> {
    let target = monitors
        .iter()
        .filter(|monitor| {
            let (width, height) = monitor.intersection(saved);
            width >= MIN_VISIBLE_WIDTH && height >= MIN_VISIBLE_HEIGHT
        })
        .max_by_key(|monitor| {
            let (width, height) = monitor.intersection(saved);
            width * height
        })
        .or(primary)
        .or_else(|| monitors.first())?;

    let max_width = target.width.max(1);
    let max_height = target.height.max(1);
    let width = saved
        .width
        .clamp(MIN_WINDOW_WIDTH.min(max_width), max_width);
    let height = saved
        .height
        .clamp(MIN_WINDOW_HEIGHT.min(max_height), max_height);

    let max_x = target.x as i64 + target.width as i64 - width as i64;
    let max_y = target.y as i64 + target.height as i64 - height as i64;
    let x = (saved.x as i64).clamp(target.x as i64, max_x) as i32;
    let y = (saved.y as i64).clamp(target.y as i64, max_y) as i32;

    Some(WindowBounds {
        x,
        y,
        width,
        height,
    })
}

/// Merge an incoming layout with the stored geometry.
///
/// `layout_set` receives the whole `Layout` from the frontend, which builds it
/// with `windowBounds: null`. A `null` (or missing) incoming value means "the
/// frontend has no opinion", so the stored value is preserved. Only an explicit
/// `Some(bounds)` replaces it. Keep this rule in one place so a future
/// refactor cannot silently reintroduce the window-bounds clobber.
pub fn merge_window_bounds(
    stored: Option<&WindowBounds>,
    incoming: Option<WindowBounds>,
) -> Option<WindowBounds> {
    incoming.or_else(|| stored.cloned())
}

/// Merge the window-owned field into a layout coming from the frontend.
pub fn apply_layout_window_bounds(stored: &Layout, incoming: Layout) -> Layout {
    let mut merged = incoming;
    merged.window_bounds = merge_window_bounds(stored.window_bounds.as_ref(), merged.window_bounds);
    merged
}

fn monitor_rect(monitor: &tauri::Monitor) -> MonitorRect {
    let position = monitor.position();
    let size = monitor.size();
    MonitorRect::new(position.x, position.y, size.width, size.height)
}

/// Current native window geometry in physical pixels.
pub fn current_window_bounds<R: Runtime>(window: &Window<R>) -> Option<WindowBounds> {
    let position = window.outer_position().ok()?;
    let size = window.outer_size().ok()?;
    if size.width == 0 || size.height == 0 {
        return None;
    }
    Some(WindowBounds {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    })
}

fn current_webview_bounds<R: Runtime>(window: &WebviewWindow<R>) -> Option<WindowBounds> {
    let position = window.outer_position().ok()?;
    let size = window.outer_size().ok()?;
    if size.width == 0 || size.height == 0 {
        return None;
    }
    Some(WindowBounds {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    })
}

/// Debounced save for move/resize events. Each event bumps a generation; only
/// the newest event's task actually writes, so a drag produces one save.
pub fn schedule_window_bounds_save<R: Runtime>(window: &Window<R>) {
    let state = window.state::<AppState>();
    let generation = state.window_bounds_seq.fetch_add(1, Ordering::SeqCst) + 1;
    let latest = state.window_bounds_seq.clone();
    let window = window.clone();

    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(SAVE_DEBOUNCE).await;
        if latest.load(Ordering::SeqCst) == generation {
            persist_window_bounds(&window).await;
        }
    });
}

/// Purpose-built write path for native window geometry. It deliberately does
/// not go through `layout_set`, so the frontend's `windowBounds: null` can
/// never erase it.
pub async fn persist_window_bounds<R: Runtime>(window: &Window<R>) {
    let Some(bounds) = current_window_bounds(window) else {
        return;
    };

    let state = window.state::<AppState>();
    let mut data = state.data.lock().await;
    data.layout.window_bounds = Some(bounds.clone());
    // Legacy mirror kept for forward/backward compatibility with older state
    // files. `layout.windowBounds` remains the source of truth.
    data.window_bounds = Some(bounds);
    if let Err(error) = state.persistence.save(&data) {
        eprintln!("failed to persist window bounds: {error}");
    }
}

/// Restore saved geometry on startup, validating it against the monitors that
/// are actually connected right now.
pub fn restore_window<R: Runtime>(
    app: &AppHandle<R>,
    window: &WebviewWindow<R>,
    saved: Option<&WindowBounds>,
) {
    let Some(saved) = saved else {
        return;
    };

    let monitors: Vec<MonitorRect> = app
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(monitor_rect)
        .collect();
    let primary = app
        .primary_monitor()
        .ok()
        .flatten()
        .map(|monitor| monitor_rect(&monitor));

    let Some(bounds) = sanitize_window_bounds(saved, &monitors, primary.as_ref()) else {
        return;
    };

    // Set size first so the OS does not clamp the position against the old
    // size on the way in, then move into the validated rectangle.
    let _ = window.set_size(PhysicalSize::new(bounds.width, bounds.height));
    let _ = window.set_position(PhysicalPosition::new(bounds.x, bounds.y));
}

fn main_window<R: Runtime>(app: &AppHandle<R>) -> Option<WebviewWindow<R>> {
    app.get_webview_window("main")
        .or_else(|| app.webview_windows().into_values().next())
}

/// Persist the latest geometry, then quit. Used by the menu Quit item so
/// Cmd/Ctrl+Q does not race the debounced move/resize save.
pub fn save_and_quit<R: Runtime>(app: &AppHandle<R>) {
    let bounds = main_window(app).and_then(|window| current_webview_bounds(&window));

    if let Some(state) = app.try_state::<AppState>() {
        // Invalidate any debounce task that is still sleeping.
        state.window_bounds_seq.fetch_add(1, Ordering::SeqCst);
    }

    let handle = app.clone();
    std::thread::spawn(move || {
        tauri::async_runtime::block_on(async {
            if let Some(bounds) = bounds {
                let state = handle.state::<AppState>();
                let mut data = state.data.lock().await;
                data.layout.window_bounds = Some(bounds.clone());
                data.window_bounds = Some(bounds);
                if let Err(error) = state.persistence.save(&data) {
                    eprintln!("failed to persist window bounds before quit: {error}");
                }
            }
        });
        handle.exit(0);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::PaneLayout;

    fn monitor(x: i32, y: i32, width: u32, height: u32) -> MonitorRect {
        MonitorRect::new(x, y, width, height)
    }

    fn bounds(x: i32, y: i32, width: u32, height: u32) -> WindowBounds {
        WindowBounds {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn no_monitors_means_leave_geometry_alone() {
        assert_eq!(
            sanitize_window_bounds(&bounds(10, 10, 1200, 800), &[], None),
            None
        );
    }

    #[test]
    fn on_screen_geometry_is_unchanged() {
        let monitors = [monitor(0, 0, 1920, 1080)];
        let saved = bounds(100, 80, 1200, 800);
        assert_eq!(
            sanitize_window_bounds(&saved, &monitors, Some(&monitors[0])),
            Some(saved)
        );
    }

    #[test]
    fn disconnected_display_falls_back_to_primary_and_stays_visible() {
        let monitors = [monitor(0, 0, 1920, 1080)];
        let saved = bounds(3200, 2100, 1200, 800);
        let restored = sanitize_window_bounds(&saved, &monitors, Some(&monitors[0])).unwrap();

        assert_eq!(restored.width, 1200);
        assert_eq!(restored.height, 800);
        assert_eq!(restored.x, 720); // 1920 - 1200
        assert_eq!(restored.y, 280); // 1080 - 800
    }

    #[test]
    fn oversized_geometry_is_clamped_to_the_monitor() {
        let monitors = [monitor(0, 0, 1920, 1080)];
        let saved = bounds(-2000, -2000, 3000, 2000);
        let restored = sanitize_window_bounds(&saved, &monitors, Some(&monitors[0])).unwrap();

        assert_eq!(restored, bounds(0, 0, 1920, 1080));
    }

    #[test]
    fn minimum_window_size_is_enforced() {
        let monitors = [monitor(0, 0, 1920, 1080)];
        let saved = bounds(100, 100, 200, 100);
        let restored = sanitize_window_bounds(&saved, &monitors, Some(&monitors[0])).unwrap();

        assert_eq!(restored.width, MIN_WINDOW_WIDTH);
        assert_eq!(restored.height, MIN_WINDOW_HEIGHT);
        assert_eq!(restored.x, 100);
        assert_eq!(restored.y, 100);
    }

    #[test]
    fn partially_off_screen_geometry_is_clamped_on_that_monitor() {
        let monitors = [monitor(0, 0, 1920, 1080), monitor(1920, 0, 1920, 1080)];
        let saved = bounds(3500, 100, 1200, 800);
        let restored = sanitize_window_bounds(&saved, &monitors, Some(&monitors[0])).unwrap();

        assert_eq!(restored.x, 2640); // (1920 + 1920) - 1200
        assert_eq!(restored.y, 100);
    }

    #[test]
    fn monitor_with_largest_intersection_wins() {
        let monitors = [monitor(0, 0, 1920, 1080), monitor(1920, 0, 1920, 1080)];
        let saved = bounds(1700, 100, 800, 600);
        let restored = sanitize_window_bounds(&saved, &monitors, Some(&monitors[0])).unwrap();

        // Most of the window is on the secondary monitor, so it is clamped
        // there rather than yanked back to the primary.
        assert_eq!(restored.x, 1920);
        assert_eq!(restored.y, 100);
    }

    #[test]
    fn null_incoming_bounds_preserve_the_stored_geometry() {
        let stored = bounds(120, 80, 1280, 820);
        assert_eq!(
            merge_window_bounds(Some(&stored), None),
            Some(stored.clone())
        );

        let layout = Layout {
            window_bounds: Some(stored.clone()),
            ..Layout::default()
        };
        let incoming = Layout {
            pane_count: 2,
            panes: vec![PaneLayout {
                repo_id: Some("repo".into()),
                tab: "files".into(),
                widths: None,
            }],
            sidebar_width: 300.0,
            sidebar_collapsed: true,
            terminal_height: 200.0,
            terminal_open: true,
            window_bounds: None,
        };

        let merged = apply_layout_window_bounds(&layout, incoming);
        assert_eq!(merged.window_bounds, Some(stored));
        assert_eq!(merged.pane_count, 2);
        assert_eq!(merged.sidebar_width, 300.0);
    }

    #[test]
    fn explicit_incoming_bounds_replace_the_stored_geometry() {
        let stored = bounds(120, 80, 1280, 820);
        let replacement = bounds(400, 200, 1000, 700);
        assert_eq!(
            merge_window_bounds(Some(&stored), Some(replacement.clone())),
            Some(replacement)
        );
    }

    #[test]
    fn missing_stored_bounds_stay_missing_for_null_incoming() {
        assert_eq!(merge_window_bounds(None, None), None);
    }
}
