pub mod cmd;
pub mod error;
pub mod git;
pub mod menu;
pub mod persist;
pub mod state;
pub mod term;
pub mod triage;
pub mod watch;
pub mod window;

use cmd::*;
use menu::MenuAction;
use tauri::{Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_state = match AppState::new() {
        Ok(s) => s,
        Err(e) => panic!("Failed to initialize app state: {}", e),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(app_state)
        .menu(|app| crate::menu::build(app))
        .on_menu_event(|app, event| {
            let Some(action) = MenuAction::from_id(event.id().as_ref()) else {
                // Predefined items (About/Edit/etc.) are handled by the OS.
                return;
            };

            match action {
                MenuAction::Quit => crate::window::save_and_quit(app),
                _ => {
                    // Dispatch through the same frontend store/command layer
                    // used by the palette and keyboard shortcuts.
                    if let Err(error) = app.emit("menu://command", action.id()) {
                        eprintln!("failed to dispatch menu command {}: {error}", action.id());
                    }
                }
            }
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::Focused(focused) => {
                let state = window.state::<AppState>();
                state.watcher_mgr.set_polling_active(*focused);
            }
            tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_) => {
                crate::window::schedule_window_bounds_save(window);
            }
            tauri::WindowEvent::CloseRequested { api, .. } => {
                // Give the window-bounds write a chance to land before the
                // window is destroyed. `destroy` does not re-enter this
                // handler, so there is no close loop.
                api.prevent_close();
                let window = window.clone();
                tauri::async_runtime::spawn(async move {
                    crate::window::persist_window_bounds(&window).await;
                    let _ = window.destroy();
                });
            }
            _ => {}
        })
        .setup(|app| {
            let state = app.state::<AppState>();
            let app_handle = app.handle().clone();

            // Restore native geometry before any background work so the
            // window does not visibly jump.
            let saved_bounds = state.startup_window_bounds.clone();
            let main_window = app
                .get_webview_window("main")
                .or_else(|| app.webview_windows().into_values().next());
            if let Some(window) = main_window {
                crate::window::restore_window(app.handle(), &window, saved_bounds.as_ref());
            }

            let runner = state.runner.clone();
            let data_arc = state.data.clone();
            let watcher_mgr = state.watcher_mgr.clone();

            tauri::async_runtime::spawn(async move {
                let repos = {
                    let data = data_arc.lock().await;
                    data.repos.clone()
                };

                for repo in repos {
                    let p = std::path::PathBuf::from(&repo.path);
                    if p.exists() {
                        // Prune backup refs older than 30 days on startup
                        crate::git::ops::prune_old_backups(&runner, &p).await;

                        // Actively watch repo immediately on startup
                        watcher_mgr
                            .watch_repo(
                                app_handle.clone(),
                                repo.id,
                                p,
                                repo.branch,
                                repo.marker.map(|m| m.sha),
                                runner.clone(),
                            )
                            .await;
                    }
                }

                // Start status polling loop
                watcher_mgr.start_poller(app_handle, runner, data_arc);
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            repo_add,
            repo_scan,
            repo_remove,
            repo_update,
            repo_relocate,
            repo_list,
            repo_status,
            repo_refresh_all,
            marker_set,
            marker_set_to_head,
            log_page,
            commit_flags,
            scope_files,
            scope_file_diff,
            blob_lines,
            blob_text,
            tree_list,
            blame_line,
            grep_defs,
            commit,
            discard,
            revert,
            revert_range,
            reset_to,
            check_pushed,
            backup_list,
            backup_restore,
            branch_list,
            checkout,
            sync_fetch,
            sync_pull,
            sync_push,
            remote_url,
            term_open,
            term_input,
            term_resize,
            term_restart,
            term_kill,
            term_paste,
            note_add,
            note_update,
            note_delete,
            note_clear_all,
            settings_get,
            settings_set,
            layout_get,
            layout_set,
            open_external_editor,
            reveal_in_file_manager,
            run_commit_message_cmd,
            env_check,
            pick_directory,
            repo_detect_stack,
            pick_icon_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Vantage application");
}
