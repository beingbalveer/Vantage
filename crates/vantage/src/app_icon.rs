//! Launcher / Dock icon for unbundled builds.
//!
//! Vantage ships as a bare binary (`cargo run -p vantage`, and the release
//! archive contains only `target/release/vantage`). Without an `.app` bundle
//! there is no `Info.plist` icon for macOS to fall back to, so the Dock draws
//! the generic executable tile — in debug and in release alike.
//!
//! Setting `NSApplication.applicationIconImage` at startup gives the process
//! the same brand icon the bundled app would get. The PNG is the one already
//! embedded by the GPUI asset source, so no extra file is shipped.

/// 1024×1024 brand icon, shared with the GPUI asset source.
const ICON_PNG: &[u8] = include_bytes!("../assets/images/vantage-icon.png");

/// Install the brand icon as the Dock / app-switcher tile.
///
/// Must run on the main thread after `NSApplication` exists; safe to call
/// more than once.
#[cfg(target_os = "macos")]
pub fn install() {
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject};
    use std::ffi::c_void;

    let Some(_mtm) = objc2::MainThreadMarker::new() else {
        return;
    };

    unsafe {
        let (Some(data_class), Some(image_class), Some(app_class)) = (
            AnyClass::get(c"NSData"),
            AnyClass::get(c"NSImage"),
            AnyClass::get(c"NSApplication"),
        ) else {
            return;
        };

        let data: *mut AnyObject = msg_send![
            data_class,
            dataWithBytes: ICON_PNG.as_ptr() as *const c_void,
            length: ICON_PNG.len()
        ];
        if data.is_null() {
            return;
        }

        let image: *mut AnyObject = msg_send![image_class, alloc];
        if image.is_null() {
            return;
        }
        let image: *mut AnyObject = msg_send![image, initWithData: data];
        if image.is_null() {
            return;
        }

        let app: *mut AnyObject = msg_send![app_class, sharedApplication];
        if !app.is_null() {
            let _: () = msg_send![app, setApplicationIconImage: image];
        }

        // `alloc`/`init` handed us the only ownership of `image`.
        let _: () = msg_send![image, release];
    }
}

/// Other platforms derive the window icon from the binary or a `.desktop`
/// file, so there is nothing to install at runtime.
#[cfg(not(target_os = "macos"))]
pub fn install() {}
