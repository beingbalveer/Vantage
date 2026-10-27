//! Native application menu.
//!
//! Every custom item below maps 1:1 to a frontend command id in
//! `src/lib/menu.ts`. Menu items carry no labels that the frontend also owns,
//! so there is a single dispatch contract: the id. On click we emit
//! `menu://command` and the frontend routes it through the same stores and
//! command functions used by the palette and keyboard shortcuts.
//!
//! # Registration point
//!
//! Add new menu items here (and a matching branch in `src/lib/menu.ts`) only
//! when the command exists in this tree. This branch predates clone, branch
//! create/rename/delete, and tag actions, so those entries are deliberately
//! omitted instead of shipping dead menus. When those commands land, append
//! them to [`MenuAction`], add the item to the relevant submenu, and wire the
//! id in `src/lib/menu.ts`.

use tauri::menu::{
    AboutMetadataBuilder, Menu, MenuItem, MenuItemBuilder, PredefinedMenuItem, Submenu,
};
use tauri::{AppHandle, Runtime};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    Settings,
    Quit,
    AddExistingRepo,
    OpenInEditor,
    ShowInFolder,
    CheckoutBranch,
    Fetch,
    Pull,
    Push,
    SplitPane,
    ClosePane,
    FocusPane1,
    FocusPane2,
    FocusPane3,
    ToggleTerminal,
    ToggleSidebar,
    HelpShortcuts,
}

impl MenuAction {
    pub const ALL: &'static [MenuAction] = &[
        MenuAction::Settings,
        MenuAction::Quit,
        MenuAction::AddExistingRepo,
        MenuAction::OpenInEditor,
        MenuAction::ShowInFolder,
        MenuAction::CheckoutBranch,
        MenuAction::Fetch,
        MenuAction::Pull,
        MenuAction::Push,
        MenuAction::SplitPane,
        MenuAction::ClosePane,
        MenuAction::FocusPane1,
        MenuAction::FocusPane2,
        MenuAction::FocusPane3,
        MenuAction::ToggleTerminal,
        MenuAction::ToggleSidebar,
        MenuAction::HelpShortcuts,
    ];

    /// Stable id shared with the frontend dispatcher.
    pub fn id(self) -> &'static str {
        match self {
            MenuAction::Settings => "settings",
            MenuAction::Quit => "quit",
            MenuAction::AddExistingRepo => "repo-add-existing",
            MenuAction::OpenInEditor => "repo-open-editor",
            MenuAction::ShowInFolder => "repo-show-folder",
            MenuAction::CheckoutBranch => "branch-checkout",
            MenuAction::Fetch => "remote-fetch",
            MenuAction::Pull => "remote-pull",
            MenuAction::Push => "remote-push",
            MenuAction::SplitPane => "view-split-pane",
            MenuAction::ClosePane => "view-close-pane",
            MenuAction::FocusPane1 => "view-focus-pane-1",
            MenuAction::FocusPane2 => "view-focus-pane-2",
            MenuAction::FocusPane3 => "view-focus-pane-3",
            MenuAction::ToggleTerminal => "view-toggle-terminal",
            MenuAction::ToggleSidebar => "view-toggle-sidebar",
            MenuAction::HelpShortcuts => "help-shortcuts",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            MenuAction::Settings => "Settings…",
            MenuAction::Quit => "Quit Vantage",
            MenuAction::AddExistingRepo => "Add Existing Repository…",
            MenuAction::OpenInEditor => "Open in Editor",
            MenuAction::ShowInFolder => "Show in Folder",
            MenuAction::CheckoutBranch => "Checkout Branch…",
            MenuAction::Fetch => "Fetch",
            MenuAction::Pull => "Pull",
            MenuAction::Push => "Push",
            MenuAction::SplitPane => "Split Pane (Cycle 1-3)",
            MenuAction::ClosePane => "Close Tab or Pane",
            MenuAction::FocusPane1 => "Focus Pane 1",
            MenuAction::FocusPane2 => "Focus Pane 2",
            MenuAction::FocusPane3 => "Focus Pane 3",
            MenuAction::ToggleTerminal => "Toggle Terminal",
            MenuAction::ToggleSidebar => "Toggle Sidebar",
            MenuAction::HelpShortcuts => "Keyboard Shortcuts",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|action| action.id() == id)
    }
}

fn item<R: Runtime>(
    app: &AppHandle<R>,
    action: MenuAction,
    accelerator: Option<&str>,
) -> tauri::Result<MenuItem<R>> {
    let mut builder = MenuItemBuilder::with_id(action.id(), action.label());
    if let Some(accelerator) = accelerator {
        builder = builder.accelerator(accelerator);
    }
    builder.build(app)
}

pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let about_metadata = AboutMetadataBuilder::new()
        .name(Some("Vantage"))
        .version(Some(app.package_info().version.to_string()))
        .copyright(Some("Copyright © 2026 Balveer Dhanoriya"))
        .build();
    let about = PredefinedMenuItem::about(app, Some("About Vantage"), Some(about_metadata))?;

    // Accelerators mirror src/lib/keys.ts (Mod == Cmd on macOS, Ctrl elsewhere).
    let settings = item(app, MenuAction::Settings, Some("CmdOrCtrl+,"))?;
    let quit = item(app, MenuAction::Quit, Some("CmdOrCtrl+Q"))?;

    let add_existing = item(app, MenuAction::AddExistingRepo, Some("CmdOrCtrl+O"))?;
    let open_editor = item(app, MenuAction::OpenInEditor, None)?;
    let show_folder = item(app, MenuAction::ShowInFolder, None)?;
    let fetch = item(app, MenuAction::Fetch, Some("CmdOrCtrl+Shift+F"))?;
    let pull = item(app, MenuAction::Pull, None)?;
    let push = item(app, MenuAction::Push, None)?;

    let checkout_branch = item(app, MenuAction::CheckoutBranch, None)?;

    let split_pane = item(app, MenuAction::SplitPane, Some("CmdOrCtrl+\\"))?;
    let close_pane = item(app, MenuAction::ClosePane, Some("CmdOrCtrl+W"))?;
    let focus_pane_1 = item(app, MenuAction::FocusPane1, Some("CmdOrCtrl+1"))?;
    let focus_pane_2 = item(app, MenuAction::FocusPane2, Some("CmdOrCtrl+2"))?;
    let focus_pane_3 = item(app, MenuAction::FocusPane3, Some("CmdOrCtrl+3"))?;
    let toggle_terminal = item(app, MenuAction::ToggleTerminal, Some("CmdOrCtrl+`"))?;
    let toggle_sidebar = item(app, MenuAction::ToggleSidebar, Some("CmdOrCtrl+B"))?;

    let help_shortcuts = item(app, MenuAction::HelpShortcuts, None)?;

    // Standard window controls. These are predefined OS items: they have no
    // MenuAction id and are handled natively rather than dispatched to the
    // frontend. `WINDOW_SUBMENU_ID` matches Tauri's default menu convention.
    let window_menu = Submenu::with_id_and_items(
        app,
        tauri::menu::WINDOW_SUBMENU_ID,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::maximize(app, None)?,
            #[cfg(target_os = "macos")]
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::close_window(app, None)?,
        ],
    )?;

    // Registering the submenu as NSApp's windows menu lets macOS append the
    // standard window list and "Bring All to Front" automatically.
    #[cfg(target_os = "macos")]
    window_menu.set_as_windows_menu_for_nsapp()?;

    #[cfg(target_os = "macos")]
    let app_menu = Submenu::with_items(
        app,
        "Vantage",
        true,
        &[
            &about,
            &PredefinedMenuItem::separator(app)?,
            &settings,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::services(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::show_all(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    // Windows/Linux have no macOS-style application menu, so the same app
    // actions live under File.
    #[cfg(not(target_os = "macos"))]
    let app_menu = Submenu::with_items(
        app,
        "File",
        true,
        &[
            &about,
            &PredefinedMenuItem::separator(app)?,
            &settings,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let edit_menu = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;

    let repository_menu = Submenu::with_items(
        app,
        "Repository",
        true,
        &[
            &add_existing,
            &PredefinedMenuItem::separator(app)?,
            &open_editor,
            &show_folder,
            &PredefinedMenuItem::separator(app)?,
            &fetch,
            &pull,
            &push,
        ],
    )?;

    let branch_menu = Submenu::with_items(app, "Branch", true, &[&checkout_branch])?;

    let view_menu = Submenu::with_items(
        app,
        "View",
        true,
        &[
            &split_pane,
            &close_pane,
            &PredefinedMenuItem::separator(app)?,
            &focus_pane_1,
            &focus_pane_2,
            &focus_pane_3,
            &PredefinedMenuItem::separator(app)?,
            &toggle_terminal,
            &toggle_sidebar,
        ],
    )?;

    let help_menu = Submenu::with_items(app, "Help", true, &[&help_shortcuts])?;

    Menu::with_items(
        app,
        &[
            &app_menu,
            &edit_menu,
            &repository_menu,
            &branch_menu,
            &view_menu,
            &window_menu,
            &help_menu,
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    // These tests cover only custom, dispatchable actions. Predefined items
    // (Minimize/Zoom/Close Window, Edit, About, …) are handled by the OS and
    // intentionally have no MenuAction id, so they must not be added to `ALL`
    // or routed through the frontend dispatcher.

    #[test]
    fn action_ids_are_unique() {
        let ids: HashSet<&str> = MenuAction::ALL.iter().map(|action| action.id()).collect();
        assert_eq!(ids.len(), MenuAction::ALL.len());
    }

    #[test]
    fn every_action_round_trips_through_its_id() {
        for action in MenuAction::ALL {
            assert_eq!(MenuAction::from_id(action.id()), Some(*action));
        }
        assert_eq!(MenuAction::from_id("not-a-real-command"), None);
    }

    #[test]
    fn predefined_and_submenu_ids_do_not_dispatch() {
        // The Window submenu is native UI, not a frontend command.
        assert_eq!(MenuAction::from_id(tauri::menu::WINDOW_SUBMENU_ID), None);
    }

    #[test]
    fn labels_are_not_empty() {
        for action in MenuAction::ALL {
            assert!(!action.label().trim().is_empty());
        }
    }
}
