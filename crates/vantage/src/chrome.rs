//! Window chrome helpers for the merged header.
//!
//! GPUI 0.2.2 does not implement `start_window_move` for macOS, so a custom
//! title bar has to ask AppKit to begin the drag itself.

/// Begin a native window drag from the header's drag region.
///
/// Call this from a mouse-move handler while the left button is down; calling
/// it on mouse-down would swallow the first click of a double-click.
#[cfg(target_os = "macos")]
pub fn start_window_drag() {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    let Some(_mtm) = objc2::MainThreadMarker::new() else {
        return;
    };

    unsafe {
        let app: *mut AnyObject = msg_send![objc2::class!(NSApplication), sharedApplication];
        if app.is_null() {
            return;
        }
        let window: *mut AnyObject = msg_send![app, keyWindow];
        if window.is_null() {
            return;
        }
        let event: *mut AnyObject = msg_send![app, currentEvent];
        if event.is_null() {
            return;
        }
        let _: () = msg_send![window, performWindowDragWithEvent: event];
    }
}

/// Non-macOS platforms keep their native decorations, so there is nothing to do.
#[cfg(not(target_os = "macos"))]
pub fn start_window_drag() {}
