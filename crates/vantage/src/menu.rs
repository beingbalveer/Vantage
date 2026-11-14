//! Native application menu bar.
//!
//! GPUI only shows the macOS menu bar when the app installs one. These menus
//! keep the standard app-level commands (hide, services, quit) alongside the
//! few Vantage actions that make sense from the menu bar.

use gpui::{actions, App, Entity, KeyBinding, Menu, MenuItem, SystemMenuType, Window};

use crate::app::VantageApp;
use crate::model::NavSection;

actions!(
    vantage_menu,
    [
        About,
        AddRepository,
        RefreshStatus,
        FetchOrigin,
        ToggleTheme,
        ToggleFullscreen,
        Minimize,
        Zoom,
        BringAllToFront,
        CloseWindow,
        Hide,
        HideOthers,
        ShowAll,
        Quit,
        OpenRepository,
        SectionOverview,
        SectionAttention,
        SectionBranches,
        SectionChanges,
    ]
);

pub fn install(cx: &mut App, app: Entity<VantageApp>) {
    cx.bind_keys([
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-h", Hide, None),
        KeyBinding::new("cmd-alt-h", HideOthers, None),
        KeyBinding::new("cmd-o", AddRepository, None),
        KeyBinding::new("cmd-r", RefreshStatus, None),
        KeyBinding::new("cmd-shift-f", FetchOrigin, None),
        KeyBinding::new("cmd-shift-l", ToggleTheme, None),
        KeyBinding::new("ctrl-cmd-f", ToggleFullscreen, None),
        KeyBinding::new("cmd-m", Minimize, None),
        KeyBinding::new("cmd-w", CloseWindow, None),
        KeyBinding::new("cmd-1", SectionOverview, None),
        KeyBinding::new("cmd-2", SectionAttention, None),
        KeyBinding::new("cmd-3", SectionBranches, None),
        KeyBinding::new("cmd-4", SectionChanges, None),
    ]);

    cx.on_action({
        let app = app.clone();
        move |_: &About, cx: &mut App| {
            #[cfg(target_os = "macos")]
            {
                let _ = (&app, cx);
                show_about_panel();
            }
            #[cfg(not(target_os = "macos"))]
            app.update(cx, |app, cx| app.show_about(cx));
        }
    });
    cx.on_action({
        let app = app.clone();
        move |_: &AddRepository, cx: &mut App| {
            app.update(cx, |app, cx| app.open_add_modal(cx));
        }
    });
    cx.on_action({
        let app = app.clone();
        move |_: &RefreshStatus, cx: &mut App| {
            app.update(cx, |app, cx| app.refresh(cx));
        }
    });
    cx.on_action({
        let app = app.clone();
        move |_: &FetchOrigin, cx: &mut App| {
            app.update(cx, |app, cx| app.fetch_selected(cx));
        }
    });
    cx.on_action({
        let app = app.clone();
        move |_: &ToggleTheme, cx: &mut App| {
            app.update(cx, |app, cx| app.toggle_theme(cx));
        }
    });
    cx.on_action({
        let app = app.clone();
        move |_: &SectionOverview, cx: &mut App| {
            app.update(cx, |app, cx| app.select_section(NavSection::Overview, cx));
        }
    });
    cx.on_action({
        let app = app.clone();
        move |_: &SectionAttention, cx: &mut App| {
            app.update(cx, |app, cx| app.select_section(NavSection::Attention, cx));
        }
    });
    cx.on_action({
        let app = app.clone();
        move |_: &SectionBranches, cx: &mut App| {
            app.update(cx, |app, cx| app.select_section(NavSection::Branches, cx));
        }
    });
    cx.on_action({
        let app = app.clone();
        move |_: &SectionChanges, cx: &mut App| {
            app.update(cx, |app, cx| app.select_section(NavSection::Changes, cx));
        }
    });

    cx.on_action(|_: &Hide, cx: &mut App| cx.hide());
    cx.on_action(|_: &HideOthers, cx: &mut App| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx: &mut App| cx.unhide_other_apps());
    cx.on_action(|_: &Quit, cx: &mut App| cx.quit());

    cx.on_action(|_: &BringAllToFront, cx: &mut App| {
        cx.activate(true);
    });
    cx.on_action(|_: &ToggleFullscreen, cx: &mut App| {
        with_active_window(cx, |window, _| window.toggle_fullscreen());
    });
    cx.on_action(|_: &Minimize, cx: &mut App| {
        with_active_window(cx, |window, _| window.minimize_window());
    });
    cx.on_action(|_: &Zoom, cx: &mut App| {
        with_active_window(cx, |window, _| window.zoom_window());
    });
    cx.on_action(|_: &CloseWindow, cx: &mut App| {
        with_active_window(cx, |window, _| window.remove_window());
    });
    cx.on_action(|_: &OpenRepository, _cx: &mut App| {
        open_repository_page();
    });

    cx.set_menus(vec![
        Menu {
            name: "Vantage".into(),
            items: vec![
                MenuItem::action("About Vantage", About),
                MenuItem::separator(),
                MenuItem::os_submenu("Services", SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action("Hide Vantage", Hide),
                MenuItem::action("Hide Others", HideOthers),
                MenuItem::action("Show All", ShowAll),
                MenuItem::separator(),
                MenuItem::action("Quit Vantage", Quit),
            ],
        },
        Menu {
            name: "File".into(),
            items: vec![
                MenuItem::action("Add Repository…", AddRepository),
                MenuItem::separator(),
                MenuItem::action("Refresh Status", RefreshStatus),
                MenuItem::action("Fetch Origin", FetchOrigin),
                MenuItem::separator(),
                MenuItem::action("Close Window", CloseWindow),
            ],
        },
        Menu {
            name: "View".into(),
            items: vec![
                MenuItem::action("All Repositories", SectionOverview),
                MenuItem::action("Needs Attention", SectionAttention),
                MenuItem::action("Branch Matrix", SectionBranches),
                MenuItem::action("Pending Changes", SectionChanges),
                MenuItem::separator(),
                MenuItem::action("Toggle Light/Dark Mode", ToggleTheme),
                MenuItem::action("Toggle Full Screen", ToggleFullscreen),
            ],
        },
        Menu {
            name: "Window".into(),
            items: vec![
                MenuItem::action("Minimize", Minimize),
                MenuItem::action("Zoom", Zoom),
                MenuItem::separator(),
                MenuItem::action("Bring All to Front", BringAllToFront),
            ],
        },
        Menu {
            name: "Help".into(),
            items: vec![MenuItem::action("Vantage on GitHub", OpenRepository)],
        },
    ]);
}

fn with_active_window(cx: &mut App, action: impl FnOnce(&mut Window, &mut App)) {
    if let Some(handle) = cx.active_window() {
        let _ = handle.update(cx, |_, window, cx| action(window, cx));
    }
}

#[cfg(target_os = "macos")]
fn show_about_panel() {
    if let Some(mtm) = objc2::MainThreadMarker::new() {
        let app = objc2_app_kit::NSApplication::sharedApplication(mtm);
        app.orderFrontStandardAboutPanel(None);
    }
}

#[cfg(target_os = "macos")]
fn open_repository_page() {
    let _ = std::process::Command::new("open")
        .arg("https://github.com/beingbalveer/Vantage")
        .spawn();
}

#[cfg(not(target_os = "macos"))]
fn open_repository_page() {}
