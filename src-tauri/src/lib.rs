pub mod cmd;
pub mod error;
pub mod git;
pub mod persist;
pub mod state;
pub mod term;
pub mod triage;
pub mod watch;

use cmd::*;
use tauri::Manager;

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
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Focused(focused) = event {
                let state = window.state::<AppState>();
                state.watcher_mgr.set_polling_active(*focused);
            }
        })
        .setup(|app| {
            let state = app.state::<AppState>();
            let app_handle = app.handle().clone();

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
            tag_list,
            tag_create,
            tag_push,
            tag_push_all,
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
