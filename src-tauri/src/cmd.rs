use crate::detect::{detect_editors, detect_shells, system_editor_command, DetectedApp};
use crate::error::AppError;
use crate::git::binary::{get_file_diff_meta, FileDiffMeta};
use crate::git::blob::{get_blob_lines, get_blob_text, get_tree_list, BlobTextResult};
use crate::git::coauthors::{recent_contributors as git_recent_contributors, Contributor};
use crate::git::diff::{
    get_scope_file_diff, get_scope_files, get_working_file_diff, get_working_part_counts,
    WorkingPartCounts,
};
use crate::git::grep::{blame_line as git_blame_line, grep_definitions};
use crate::git::identity::{read_identity, write_identity, GitIdentity, IdentityScope};
use crate::git::log::{get_history_page, get_log_page};
use crate::git::ops::{
    abort_operation as git_abort_operation, amend_commit as git_amend_commit,
    check_pushed as git_check_pushed, commit as git_commit,
    continue_operation as git_continue_operation, discard as git_discard, get_real_git_dir,
    head_commit_info as git_head_commit_info, list_backups, partial_lines,
    reset_to as git_reset_to, restore_backup, revert as git_revert,
    revert_range as git_revert_range, undo_last_commit as git_undo_last_commit, HeadCommitInfo,
    PartialAction, UndoCommitResult,
};
use crate::git::patch::SelectedLine;
use crate::git::refs::{
    checkout_target, create_branch, create_tag as git_create_tag, delete_branch, get_remote_url,
    is_branch_merged, list_branches, list_tags as git_list_tags,
    push_all_tags as git_push_all_tags, push_tag as git_push_tag, rename_branch,
    sync_fetch as git_sync_fetch, sync_pull as git_sync_pull, sync_push as git_sync_push, TagInfo,
};
use crate::git::runner::GitRunner;
use crate::git::stash::{
    list_stashes, stash_apply as git_stash_apply, stash_drop as git_stash_drop,
    stash_pop as git_stash_pop, stash_push as git_stash_push,
};
use crate::git::status::{query_repo_status, rebase_head_branch};
use crate::persist::{PersistedState, StatePersistence};
use crate::state::*;
use crate::term::{TermOpenResult, TerminalManager};
use crate::triage::evaluate_commit_flags;
use crate::watch::RepoWatcherManager;
use chrono::Utc;
use dunce::canonicalize;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tauri::{AppHandle, State};
use tokio::sync::Mutex;
use uuid::Uuid;

pub struct AppState {
    pub runner: Arc<GitRunner>,
    pub persistence: StatePersistence,
    pub term_mgr: TerminalManager,
    pub watcher_mgr: RepoWatcherManager,
    pub data: Arc<Mutex<PersistedState>>,
    pub startup_warning: Option<String>,
    /// Bumped by every move/resize and invalidated on quit so only the newest
    /// debounced window-bounds write lands.
    pub window_bounds_seq: Arc<AtomicU64>,
    /// Geometry loaded from disk before the window exists. `layout` is the
    /// source of truth; the top-level field is a legacy mirror.
    pub startup_window_bounds: Option<WindowBounds>,
}

impl AppState {
    pub fn new() -> Result<Self, AppError> {
        let persistence = StatePersistence::new()?;
        let (mut data, startup_warning) = persistence.load();
        // Values from older or hand-edited state files are clamped instead of
        // being trusted verbatim.
        data.settings = data.settings.clone().sanitized();
        let startup_window_bounds = data
            .layout
            .window_bounds
            .clone()
            .or_else(|| data.window_bounds.clone());
        Ok(Self {
            runner: Arc::new(GitRunner::new()),
            persistence,
            term_mgr: TerminalManager::new(),
            watcher_mgr: RepoWatcherManager::new(),
            data: Arc::new(Mutex::new(data)),
            startup_warning,
            window_bounds_seq: Arc::new(AtomicU64::new(0)),
            startup_window_bounds,
        })
    }
}

// ---------------- Repo Commands ----------------

/// Register a directory that is already on disk as a repo: verify it is a git
/// work tree, canonicalize it, set the initial reviewed marker, persist, and
/// start watching. Shared by "add existing", scan, and clone so the semantics
/// stay identical.
async fn register_repo(
    state: &AppState,
    app_handle: &AppHandle,
    path: String,
    branch: Option<String>,
    unreviewed_last_n: Option<u32>,
) -> Result<RepoEntry, AppError> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err(AppError::path_missing(&path));
    }

    let canon = canonicalize(&p).map_err(AppError::io)?;

    // Verify it is a git repo: git rev-parse --show-toplevel
    let out = state
        .runner
        .run_read(&canon, &["rev-parse", "--show-toplevel"])
        .await?;

    if !out.success() {
        return Err(AppError::not_a_repo(&path));
    }

    let toplevel = out.stdout_string().trim().to_string();
    let top_canon = canonicalize(Path::new(&toplevel)).unwrap_or_else(|_| PathBuf::from(&toplevel));
    let final_path = top_canon.to_string_lossy().to_string();

    let target_branch = branch
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let mut data = state.data.lock().await;

    // An empty repository has no HEAD yet; use the configured default branch
    // name so the first branch view has the name the terminal's first commit
    // will create. Normal repositories keep tracking their checked-out HEAD.
    let default_branch = data.settings.default_branch.clone();
    let has_head = state
        .runner
        .run_read(&top_canon, &["rev-parse", "--verify", "HEAD"])
        .await
        .map(|o| o.success())
        .unwrap_or(false);
    let effective_branch =
        resolve_initial_branch(target_branch.as_deref(), has_head, &default_branch);

    // Duplicate check: both path and branch must match
    if let Some(existing) = data
        .repos
        .iter()
        .find(|r| r.path == final_path && r.branch == effective_branch)
    {
        return Ok(existing.clone());
    }

    let folder_name = top_canon
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "repo".to_string());

    // Determine initial marker
    let rev_target = effective_branch.as_deref().unwrap_or("HEAD");
    let head_out = state
        .runner
        .run_read(&top_canon, &["rev-parse", rev_target])
        .await;

    let marker = if let Ok(head) = head_out {
        if head.success() {
            let mut sha = head.stdout_string().trim().to_string();
            // If user wanted last N unreviewed
            if let Some(n) = unreviewed_last_n {
                if n > 0 {
                    let n_spec = format!("{}~{}", rev_target, n);
                    if let Ok(ancestor_out) = state
                        .runner
                        .run_read(&top_canon, &["rev-parse", &n_spec])
                        .await
                    {
                        if ancestor_out.success() {
                            sha = ancestor_out.stdout_string().trim().to_string();
                        }
                    }
                }
            }

            Some(Marker {
                sha,
                set_at: Utc::now().to_rfc3339(),
                branch: effective_branch.clone(),
            })
        } else {
            None
        }
    } else {
        None
    };

    let next_order = data.repos.iter().map(|r| r.order).max().unwrap_or(0) + 1;
    let repo_id = Uuid::new_v4().to_string();

    let entry = RepoEntry {
        id: repo_id.clone(),
        path: final_path,
        name: folder_name,
        branch: effective_branch.clone(),
        pinned: false,
        order: next_order,
        marker: marker.clone(),
        notes: Vec::new(),
    };

    data.repos.push(entry.clone());
    state.persistence.save(&data)?;

    // Watch repo
    state
        .watcher_mgr
        .watch_repo(
            app_handle.clone(),
            repo_id,
            top_canon,
            effective_branch,
            marker.map(|m| m.sha),
            state.runner.clone(),
        )
        .await;

    Ok(entry)
}

#[tauri::command]
pub async fn repo_add(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    path: String,
    branch: Option<String>,
    unreviewed_last_n: Option<u32>,
) -> Result<RepoEntry, AppError> {
    register_repo(state.inner(), &app_handle, path, branch, unreviewed_last_n).await
}

#[tauri::command]
pub async fn repo_clone(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    clone_id: String,
    url: String,
    dest_parent: String,
    name: Option<String>,
    branch: Option<String>,
) -> Result<RepoEntry, AppError> {
    // Clone into the chosen destination, then register through the same path as
    // "add existing" so marker/watch/duplicate semantics are identical.
    let final_path = crate::git::clone::clone_repo(
        &state.runner,
        &app_handle,
        &clone_id,
        &url,
        &dest_parent,
        name.as_deref(),
        branch.as_deref(),
    )
    .await?;

    register_repo(state.inner(), &app_handle, final_path, branch, None).await
}

#[tauri::command]
pub async fn repo_scan(
    _state: State<'_, AppState>,
    path: String,
    depth: Option<u32>,
) -> Result<Vec<String>, AppError> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err(AppError::path_missing(&path));
    }

    let max_depth = depth.unwrap_or(2);
    let mut found = Vec::new();

    fn scan_dir(dir: &Path, current_depth: u32, max_depth: u32, acc: &mut Vec<String>) {
        if current_depth > max_depth {
            return;
        }

        if dir.join(".git").exists() {
            acc.push(dir.to_string_lossy().to_string());
            return;
        }

        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                if let Ok(ft) = entry.file_type() {
                    if ft.is_dir() {
                        let sub = entry.path();
                        let name = sub.file_name().and_then(|s| s.to_str()).unwrap_or("");
                        if !name.starts_with('.') && name != "node_modules" && name != "target" {
                            scan_dir(&sub, current_depth + 1, max_depth, acc);
                        }
                    }
                }
            }
        }
    }

    scan_dir(&p, 0, max_depth, &mut found);
    Ok(found)
}

#[tauri::command]
pub async fn repo_remove(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    let mut data = state.data.lock().await;
    data.repos.retain(|r| r.id != id);
    state.persistence.save(&data)?;
    state.watcher_mgr.unwatch_repo(&id).await;
    let _ = state.term_mgr.kill(&id).await;
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoUpdatePayload {
    pub name: Option<String>,
    pub branch: Option<String>,
    pub pinned: Option<bool>,
    pub order: Option<u32>,
}

#[tauri::command]
pub async fn repo_update(
    state: State<'_, AppState>,
    id: String,
    update: RepoUpdatePayload,
) -> Result<RepoEntry, AppError> {
    let mut data = state.data.lock().await;
    let entry = data
        .repos
        .iter_mut()
        .find(|r| r.id == id)
        .ok_or_else(|| AppError::not_found("Repository not found"))?;

    if let Some(n) = update.name {
        entry.name = n;
    }
    if let Some(b) = update.branch {
        let trimmed = b.trim();
        entry.branch = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        };
    }
    if let Some(p) = update.pinned {
        entry.pinned = p;
    }
    if let Some(o) = update.order {
        entry.order = o;
    }

    let cloned = entry.clone();
    state.persistence.save(&data)?;
    Ok(cloned)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoReorderItem {
    pub id: String,
    pub order: u32,
    pub pinned: Option<bool>,
}

#[tauri::command]
pub async fn repo_reorder(
    state: State<'_, AppState>,
    items: Vec<RepoReorderItem>,
) -> Result<(), AppError> {
    let mut data = state.data.lock().await;
    for item in items {
        if let Some(entry) = data.repos.iter_mut().find(|r| r.id == item.id) {
            entry.order = item.order;
            if let Some(p) = item.pinned {
                entry.pinned = p;
            }
        }
    }
    data.repos
        .sort_by_key(|r| (if r.pinned { 0 } else { 1 }, r.order));
    state.persistence.save(&data)?;
    Ok(())
}

#[tauri::command]
pub async fn repo_relocate(
    state: State<'_, AppState>,
    id: String,
    new_path: String,
) -> Result<RepoEntry, AppError> {
    let p = PathBuf::from(&new_path);
    if !p.exists() {
        return Err(AppError::path_missing(&new_path));
    }

    let canon = canonicalize(&p).map_err(AppError::io)?;
    let mut data = state.data.lock().await;
    let entry = data
        .repos
        .iter_mut()
        .find(|r| r.id == id)
        .ok_or_else(|| AppError::not_found("Repository not found"))?;

    entry.path = canon.to_string_lossy().to_string();
    let cloned = entry.clone();
    state.persistence.save(&data)?;
    Ok(cloned)
}

#[tauri::command]
pub async fn repo_list(state: State<'_, AppState>) -> Result<Vec<RepoEntry>, AppError> {
    let data = state.data.lock().await;
    Ok(data.repos.clone())
}

#[tauri::command]
pub async fn repo_status(state: State<'_, AppState>, id: String) -> Result<RepoStatus, AppError> {
    let data = state.data.lock().await;
    let entry = data
        .repos
        .iter()
        .find(|r| r.id == id)
        .ok_or_else(|| AppError::not_found("Repository not found"))?;

    let path = PathBuf::from(&entry.path);
    let marker_sha = entry.marker.as_ref().map(|m| m.sha.as_str());
    let target_branch = entry.branch.as_deref();

    Ok(query_repo_status(&state.runner, &id, &path, target_branch, marker_sha).await)
}

#[tauri::command]
pub async fn repo_refresh_all(state: State<'_, AppState>) -> Result<Vec<RepoStatus>, AppError> {
    // Snapshot the repos and release the state lock before doing any Git work.
    // The previous implementation held `data` for the whole sequential sweep,
    // so a slow repository blocked every unrelated command (diff, commit,
    // terminal, settings) until the refresh finished.
    let specs: Vec<(String, PathBuf, Option<String>, Option<String>)> = {
        let data = state.data.lock().await;
        data.repos
            .iter()
            .map(|r| {
                (
                    r.id.clone(),
                    PathBuf::from(&r.path),
                    r.branch.clone(),
                    r.marker.as_ref().map(|m| m.sha.clone()),
                )
            })
            .collect()
    };

    // Refresh in parallel. `JoinSet` bounds nothing by itself but the repo
    // count is small and each task is independent; this is still far better
    // than serializing them behind one mutex.
    let runner = state.runner.clone();
    let mut tasks = tokio::task::JoinSet::new();
    for (id, path, branch, marker) in specs {
        let runner = runner.clone();
        tasks.spawn(async move {
            query_repo_status(&runner, &id, &path, branch.as_deref(), marker.as_deref()).await
        });
    }

    let mut statuses = Vec::new();
    while let Some(result) = tasks.join_next().await {
        match result {
            Ok(status) => statuses.push(status),
            Err(error) => eprintln!("repo status refresh task failed: {error}"),
        }
    }
    Ok(statuses)
}

// ---------------- Marker Commands ----------------

#[tauri::command]
pub async fn marker_set(
    state: State<'_, AppState>,
    id: String,
    sha: String,
) -> Result<(), AppError> {
    let mut data = state.data.lock().await;
    let entry = data
        .repos
        .iter_mut()
        .find(|r| r.id == id)
        .ok_or_else(|| AppError::not_found("Repository not found"))?;

    entry.marker = Some(Marker {
        sha,
        set_at: Utc::now().to_rfc3339(),
        branch: None,
    });

    state.persistence.save(&data)?;
    Ok(())
}

#[tauri::command]
pub async fn marker_set_to_head(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    let (repo_path, target_branch) = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        (PathBuf::from(&entry.path), entry.branch.clone())
    };

    let target_ref = target_branch.as_deref().unwrap_or("HEAD");
    let out = state
        .runner
        .run_read(&repo_path, &["rev-parse", target_ref])
        .await?;

    if !out.success() {
        return Err(AppError::other(out.stderr));
    }

    let head_sha = out.stdout_string().trim().to_string();
    marker_set(state, id, head_sha).await
}

// ---------------- History & Diff Commands ----------------

#[tauri::command]
pub async fn log_page(
    state: State<'_, AppState>,
    id: String,
    skip: u32,
    limit: u32,
    query: Option<String>,
) -> Result<Vec<CommitSummary>, AppError> {
    let (repo_path, marker_sha, target_branch) = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        (
            PathBuf::from(&entry.path),
            entry.marker.as_ref().map(|m| m.sha.clone()),
            entry.branch.clone(),
        )
    };

    get_log_page(
        &state.runner,
        &repo_path,
        target_branch.as_deref(),
        skip,
        limit,
        query.as_deref(),
        marker_sha.as_deref(),
    )
    .await
}

/// Paged history with a total count and an optional message/author/path filter.
#[tauri::command]
pub async fn history_page(
    state: State<'_, AppState>,
    id: String,
    skip: u32,
    limit: u32,
    query: Option<String>,
) -> Result<HistoryPage, AppError> {
    let (repo_path, marker_sha, target_branch) = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        (
            PathBuf::from(&entry.path),
            entry.marker.as_ref().map(|m| m.sha.clone()),
            entry.branch.clone(),
        )
    };

    get_history_page(
        &state.runner,
        &repo_path,
        target_branch.as_deref(),
        skip,
        limit,
        query.as_deref(),
        marker_sha.as_deref(),
    )
    .await
}

#[tauri::command]
pub async fn commit_flags(
    state: State<'_, AppState>,
    id: String,
    shas: Vec<String>,
) -> Result<HashMap<String, Vec<TriageFlag>>, AppError> {
    let repo_path = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        PathBuf::from(&entry.path)
    };

    // Each SHA needs two Git processes (numstat + a -U0 sample). Fan them out
    // with a small concurrency cap so a full history page no longer blocks for
    // 2 × N sequential process spawns, without flooding the machine.
    let runner = state.runner.clone();
    let permits = Arc::new(tokio::sync::Semaphore::new(6));
    let mut tasks = tokio::task::JoinSet::new();

    for sha in shas {
        let runner = runner.clone();
        let repo_path = repo_path.clone();
        let permits = permits.clone();
        tasks.spawn(async move {
            let _permit = permits.acquire_owned().await.ok()?;

            let changes = get_scope_files(
                &runner,
                &repo_path,
                &DiffScope::Commit { sha: sha.clone() },
                false,
            )
            .await
            .ok()?;

            let adds: u32 = changes.iter().map(|c| c.additions).sum();
            let dels: u32 = changes.iter().map(|c| c.deletions).sum();

            // Cheap diff sample for secrets/tests.
            let diff_sample = runner
                .run_read(&repo_path, &["show", "--format=", "-U0", &sha])
                .await
                .map(|o| o.stdout_string())
                .unwrap_or_default();

            let flags = evaluate_commit_flags(
                changes.len() as u32,
                adds,
                dels,
                &changes,
                Some(&diff_sample),
            );
            Some((sha, flags))
        });
    }

    let mut results = HashMap::new();
    while let Some(result) = tasks.join_next().await {
        if let Ok(Some((sha, flags))) = result {
            results.insert(sha, flags);
        }
    }

    Ok(results)
}

#[tauri::command]
pub async fn scope_files(
    state: State<'_, AppState>,
    id: String,
    scope: DiffScope,
    ignore_ws: bool,
) -> Result<Vec<FileChange>, AppError> {
    let repo_path = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        PathBuf::from(&entry.path)
    };

    get_scope_files(&state.runner, &repo_path, &scope, ignore_ws).await
}

// The IPC contract fixes this signature; every argument comes from the webview.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn scope_file_diff(
    state: State<'_, AppState>,
    id: String,
    scope: DiffScope,
    path: String,
    ctx_lines: u32,
    ignore_ws: bool,
    force: bool,
    full_context: bool,
) -> Result<DiffFile, AppError> {
    let repo_path = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        PathBuf::from(&entry.path)
    };

    get_scope_file_diff(
        &state.runner,
        &repo_path,
        &scope,
        &path,
        ctx_lines,
        ignore_ws,
        force,
        full_context,
    )
    .await
}

#[tauri::command]
pub async fn working_file_diff(
    state: State<'_, AppState>,
    id: String,
    path: String,
    part: String,
    ctx_lines: u32,
    ignore_ws: bool,
    force: bool,
) -> Result<DiffFile, AppError> {
    let repo_path = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        PathBuf::from(&entry.path)
    };

    get_working_file_diff(
        &state.runner,
        &repo_path,
        &path,
        &part,
        ctx_lines,
        ignore_ws,
        force,
    )
    .await
}

#[tauri::command]
pub async fn working_part_counts(
    state: State<'_, AppState>,
    id: String,
) -> Result<WorkingPartCounts, AppError> {
    let repo_path = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        PathBuf::from(&entry.path)
    };

    get_working_part_counts(&state.runner, &repo_path).await
}

#[tauri::command]
pub async fn stage_lines(
    state: State<'_, AppState>,
    id: String,
    path: String,
    selection: Vec<SelectedLine>,
    ignore_ws: bool,
) -> Result<(), AppError> {
    let (repo_path, branch) = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        (PathBuf::from(&entry.path), entry.branch.clone())
    };

    verify_branch_checked_out(&state.runner, &repo_path, branch.as_deref()).await?;
    partial_lines(
        &state.runner,
        &repo_path,
        &path,
        &selection,
        PartialAction::Stage,
        ignore_ws,
    )
    .await?;
    Ok(())
}

#[tauri::command]
pub async fn unstage_lines(
    state: State<'_, AppState>,
    id: String,
    path: String,
    selection: Vec<SelectedLine>,
    ignore_ws: bool,
) -> Result<(), AppError> {
    let (repo_path, branch) = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        (PathBuf::from(&entry.path), entry.branch.clone())
    };

    verify_branch_checked_out(&state.runner, &repo_path, branch.as_deref()).await?;
    partial_lines(
        &state.runner,
        &repo_path,
        &path,
        &selection,
        PartialAction::Unstage,
        ignore_ws,
    )
    .await?;
    Ok(())
}

#[tauri::command]
pub async fn discard_lines(
    state: State<'_, AppState>,
    id: String,
    path: String,
    selection: Vec<SelectedLine>,
    ignore_ws: bool,
) -> Result<Option<BackupInfo>, AppError> {
    let (repo_path, branch) = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        (PathBuf::from(&entry.path), entry.branch.clone())
    };

    verify_branch_checked_out(&state.runner, &repo_path, branch.as_deref()).await?;
    partial_lines(
        &state.runner,
        &repo_path,
        &path,
        &selection,
        PartialAction::Discard,
        ignore_ws,
    )
    .await
}

#[tauri::command]
pub async fn blob_lines(
    state: State<'_, AppState>,
    id: String,
    rev: String,
    path: String,
    start: u32,
    end: u32,
) -> Result<Vec<String>, AppError> {
    let repo_path = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        PathBuf::from(&entry.path)
    };

    get_blob_lines(&state.runner, &repo_path, &rev, &path, start, end).await
}

#[tauri::command]
pub async fn blob_text(
    state: State<'_, AppState>,
    id: String,
    rev: String,
    path: String,
) -> Result<BlobTextResult, AppError> {
    let repo_path = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        PathBuf::from(&entry.path)
    };

    get_blob_text(&state.runner, &repo_path, &rev, &path).await
}

#[tauri::command]
pub async fn tree_list(
    state: State<'_, AppState>,
    id: String,
    rev: String,
) -> Result<Vec<String>, AppError> {
    let repo_path = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        PathBuf::from(&entry.path)
    };

    get_tree_list(&state.runner, &repo_path, &rev).await
}

/// Old/new sizes (and bounded image previews) for one changed file. Lets the
/// diff viewer show an honest binary state instead of "No changes in this file".
#[tauri::command]
pub async fn file_diff_meta(
    state: State<'_, AppState>,
    id: String,
    scope: DiffScope,
    path: String,
    old_path: Option<String>,
) -> Result<FileDiffMeta, AppError> {
    let repo_path = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        PathBuf::from(&entry.path)
    };

    get_file_diff_meta(
        &state.runner,
        &repo_path,
        &scope,
        &path,
        old_path.as_deref(),
    )
    .await
}

#[tauri::command]
pub async fn blame_line(
    state: State<'_, AppState>,
    id: String,
    rev: String,
    path: String,
    line: u32,
) -> Result<BlameInfo, AppError> {
    let repo_path = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        PathBuf::from(&entry.path)
    };

    git_blame_line(&state.runner, &repo_path, &rev, &path, line).await
}

#[tauri::command]
pub async fn grep_defs(
    state: State<'_, AppState>,
    id: String,
    rev: String,
    ident: String,
    ext_hint: Option<String>,
) -> Result<Vec<DefHit>, AppError> {
    let repo_path = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        PathBuf::from(&entry.path)
    };

    grep_definitions(&state.runner, &repo_path, &rev, &ident, ext_hint.as_deref()).await
}

// ---------------- Writes Commands ----------------

async fn verify_branch_checked_out(
    runner: &GitRunner,
    repo_path: &Path,
    target_branch: Option<&str>,
) -> Result<(), AppError> {
    if let Some(tb) = target_branch {
        let head_out = runner
            .run_read(repo_path, &["rev-parse", "--abbrev-ref", "HEAD"])
            .await?;
        let current_branch = head_out.stdout_string().trim().to_string();
        if current_branch != tb {
            // A rebase intentionally detaches HEAD. Accept the write when the
            // branch being rebased is the one this repo view targets, otherwise
            // rebase continue/abort would be impossible to run (ALPH-156).
            let detached_for_rebase = if current_branch == "HEAD" {
                match get_real_git_dir(runner, repo_path).await {
                    Ok(git_dir) => rebase_head_branch(&git_dir).as_deref() == Some(tb),
                    Err(_) => false,
                }
            } else {
                false
            };
            if !detached_for_rebase {
                return Err(AppError::other(format!(
                    "Cannot perform write on inactive branch view '{}' (current checked-out branch is '{}'). Checkout branch first.",
                    tb, current_branch
                )));
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn commit(
    state: State<'_, AppState>,
    id: String,
    paths: Vec<String>,
    summary: String,
    body: Option<String>,
    co_authors: Option<Vec<String>>,
) -> Result<CommitSummary, AppError> {
    let (repo_path, branch) = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        (PathBuf::from(&entry.path), entry.branch.clone())
    };

    verify_branch_checked_out(&state.runner, &repo_path, branch.as_deref()).await?;
    git_commit(
        &state.runner,
        &repo_path,
        &paths,
        &summary,
        body.as_deref(),
        co_authors.as_deref().unwrap_or(&[]),
    )
    .await
}

#[tauri::command]
pub async fn recent_contributors(
    state: State<'_, AppState>,
    id: String,
    limit: Option<u32>,
) -> Result<Vec<Contributor>, AppError> {
    let repo_path = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        PathBuf::from(&entry.path)
    };

    git_recent_contributors(&state.runner, &repo_path, limit.unwrap_or(200)).await
}

#[tauri::command]
pub async fn head_commit_info(
    state: State<'_, AppState>,
    id: String,
) -> Result<HeadCommitInfo, AppError> {
    let repo_path = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        PathBuf::from(&entry.path)
    };

    git_head_commit_info(&state.runner, &repo_path).await
}

#[tauri::command]
pub async fn amend_commit(
    state: State<'_, AppState>,
    id: String,
    paths: Vec<String>,
    summary: String,
    body: Option<String>,
) -> Result<CommitSummary, AppError> {
    let (repo_path, branch) = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        (PathBuf::from(&entry.path), entry.branch.clone())
    };

    verify_branch_checked_out(&state.runner, &repo_path, branch.as_deref()).await?;
    git_amend_commit(&state.runner, &repo_path, &paths, &summary, body.as_deref()).await
}

#[tauri::command]
pub async fn undo_last_commit(
    state: State<'_, AppState>,
    id: String,
) -> Result<UndoCommitResult, AppError> {
    let (repo_path, branch) = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        (PathBuf::from(&entry.path), entry.branch.clone())
    };

    verify_branch_checked_out(&state.runner, &repo_path, branch.as_deref()).await?;
    git_undo_last_commit(&state.runner, &repo_path).await
}

#[tauri::command]
pub async fn discard(
    state: State<'_, AppState>,
    id: String,
    paths: Vec<String>,
) -> Result<BackupInfo, AppError> {
    let (repo_path, branch) = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        (PathBuf::from(&entry.path), entry.branch.clone())
    };

    verify_branch_checked_out(&state.runner, &repo_path, branch.as_deref()).await?;
    git_discard(&state.runner, &repo_path, &paths).await
}

#[tauri::command]
pub async fn revert(
    state: State<'_, AppState>,
    id: String,
    sha: String,
) -> Result<CommitSummary, AppError> {
    let (repo_path, branch) = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        (PathBuf::from(&entry.path), entry.branch.clone())
    };

    verify_branch_checked_out(&state.runner, &repo_path, branch.as_deref()).await?;
    git_revert(&state.runner, &repo_path, &sha).await
}

#[tauri::command]
pub async fn revert_range(
    state: State<'_, AppState>,
    id: String,
    from: String,
    to: String,
) -> Result<Vec<CommitSummary>, AppError> {
    let (repo_path, branch) = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        (PathBuf::from(&entry.path), entry.branch.clone())
    };

    verify_branch_checked_out(&state.runner, &repo_path, branch.as_deref()).await?;
    git_revert_range(&state.runner, &repo_path, &from, &to).await
}

/// Finish an in-progress merge/rebase/cherry-pick/revert (ALPH-156).
#[tauri::command]
pub async fn operation_continue(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    let (repo_path, branch) = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        (PathBuf::from(&entry.path), entry.branch.clone())
    };

    verify_branch_checked_out(&state.runner, &repo_path, branch.as_deref()).await?;
    git_continue_operation(&state.runner, &repo_path).await
}

/// Abandon an in-progress merge/rebase/cherry-pick/revert (ALPH-156).
#[tauri::command]
pub async fn operation_abort(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    let (repo_path, branch) = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        (PathBuf::from(&entry.path), entry.branch.clone())
    };

    verify_branch_checked_out(&state.runner, &repo_path, branch.as_deref()).await?;
    git_abort_operation(&state.runner, &repo_path).await
}

#[tauri::command]
pub async fn reset_to(
    state: State<'_, AppState>,
    id: String,
    sha: String,
    also_discard_dirty: bool,
) -> Result<BackupInfo, AppError> {
    let (repo_path, branch) = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        (PathBuf::from(&entry.path), entry.branch.clone())
    };

    verify_branch_checked_out(&state.runner, &repo_path, branch.as_deref()).await?;
    git_reset_to(&state.runner, &repo_path, &sha, also_discard_dirty).await
}

#[tauri::command]
pub async fn backup_list(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<BackupInfo>, AppError> {
    let repo_path = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        PathBuf::from(&entry.path)
    };

    list_backups(&state.runner, &repo_path).await
}

#[tauri::command]
pub async fn backup_restore(
    state: State<'_, AppState>,
    id: String,
    r#ref: String,
) -> Result<(), AppError> {
    let repo_path = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        PathBuf::from(&entry.path)
    };

    restore_backup(&state.runner, &repo_path, &r#ref).await
}

#[tauri::command]
pub async fn branch_list(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<BranchInfo>, AppError> {
    let repo_path = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        PathBuf::from(&entry.path)
    };

    list_branches(&state.runner, &repo_path).await
}

#[tauri::command]
pub async fn checkout(
    state: State<'_, AppState>,
    id: String,
    target: String,
    dirty_action: Option<String>,
    stash_message: Option<String>,
    include_untracked: Option<bool>,
) -> Result<CheckoutResult, AppError> {
    let repo_path = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        PathBuf::from(&entry.path)
    };

    let target = target.trim().to_string();
    if target.is_empty() {
        return Err(AppError::other(
            "No branch or commit specified to check out",
        ));
    }

    checkout_target(
        &state.runner,
        &repo_path,
        &target,
        dirty_action.as_deref(),
        stash_message.as_deref(),
        include_untracked.unwrap_or(false),
    )
    .await
}

// ---------------- Stash Commands ----------------

async fn repo_path_for(state: &State<'_, AppState>, id: &str) -> Result<PathBuf, AppError> {
    let data = state.data.lock().await;
    let entry = data
        .repos
        .iter()
        .find(|r| r.id == id)
        .ok_or_else(|| AppError::not_found("Repository not found"))?;
    Ok(PathBuf::from(&entry.path))
}

#[tauri::command]
pub async fn stash_list(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<StashEntry>, AppError> {
    let repo_path = repo_path_for(&state, &id).await?;
    list_stashes(&state.runner, &repo_path).await
}

#[tauri::command]
pub async fn stash_push(
    state: State<'_, AppState>,
    id: String,
    message: Option<String>,
    include_untracked: Option<bool>,
) -> Result<StashPushResult, AppError> {
    let repo_path = repo_path_for(&state, &id).await?;
    git_stash_push(
        &state.runner,
        &repo_path,
        message.as_deref(),
        include_untracked.unwrap_or(false),
    )
    .await
}

#[tauri::command]
pub async fn stash_pop(state: State<'_, AppState>, id: String, index: u32) -> Result<(), AppError> {
    let repo_path = repo_path_for(&state, &id).await?;
    git_stash_pop(&state.runner, &repo_path, index).await
}

#[tauri::command]
pub async fn stash_apply(
    state: State<'_, AppState>,
    id: String,
    index: u32,
) -> Result<(), AppError> {
    let repo_path = repo_path_for(&state, &id).await?;
    git_stash_apply(&state.runner, &repo_path, index).await
}

#[tauri::command]
pub async fn stash_drop(
    state: State<'_, AppState>,
    id: String,
    index: u32,
) -> Result<(), AppError> {
    let repo_path = repo_path_for(&state, &id).await?;
    git_stash_drop(&state.runner, &repo_path, index).await
}

#[tauri::command]
pub async fn branch_create(
    state: State<'_, AppState>,
    id: String,
    name: String,
    start_point: Option<String>,
    checkout: bool,
) -> Result<(), AppError> {
    let repo_path = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        PathBuf::from(&entry.path)
    };

    create_branch(
        &state.runner,
        &repo_path,
        &name,
        start_point.as_deref(),
        checkout,
    )
    .await
}

#[tauri::command]
pub async fn branch_rename(
    state: State<'_, AppState>,
    id: String,
    from: Option<String>,
    to: String,
) -> Result<(), AppError> {
    let repo_path = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        PathBuf::from(&entry.path)
    };

    rename_branch(&state.runner, &repo_path, from.as_deref(), &to).await
}

#[tauri::command]
pub async fn branch_delete(
    state: State<'_, AppState>,
    id: String,
    name: String,
    force: bool,
) -> Result<(), AppError> {
    let repo_path = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        PathBuf::from(&entry.path)
    };

    delete_branch(&state.runner, &repo_path, &name, force).await
}

#[tauri::command]
pub async fn branch_is_merged(
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> Result<bool, AppError> {
    let repo_path = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        PathBuf::from(&entry.path)
    };

    is_branch_merged(&state.runner, &repo_path, &name).await
}

#[tauri::command]
pub async fn sync_fetch(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    let repo_path = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        PathBuf::from(&entry.path)
    };

    git_sync_fetch(&state.runner, &repo_path).await
}

#[tauri::command]
pub async fn sync_pull(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    let (repo_path, branch) = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        (PathBuf::from(&entry.path), entry.branch.clone())
    };

    git_sync_pull(&state.runner, &repo_path, branch.as_deref()).await
}

#[tauri::command]
pub async fn sync_push(
    state: State<'_, AppState>,
    id: String,
    publish: bool,
) -> Result<(), AppError> {
    let (repo_path, branch) = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        (PathBuf::from(&entry.path), entry.branch.clone())
    };

    git_sync_push(&state.runner, &repo_path, publish, branch.as_deref()).await
}

#[tauri::command]
pub async fn remote_url(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<String>, AppError> {
    let repo_path = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        PathBuf::from(&entry.path)
    };

    get_remote_url(&state.runner, &repo_path).await
}

#[tauri::command]
pub async fn check_pushed(
    state: State<'_, AppState>,
    id: String,
    sha: String,
) -> Result<bool, AppError> {
    let repo_path = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        PathBuf::from(&entry.path)
    };

    git_check_pushed(&state.runner, &repo_path, &sha).await
}

// ---------------- Tag Commands ----------------

#[tauri::command]
pub async fn tag_list(state: State<'_, AppState>, id: String) -> Result<Vec<TagInfo>, AppError> {
    let repo_path = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        PathBuf::from(&entry.path)
    };

    git_list_tags(&state.runner, &repo_path).await
}

#[tauri::command]
pub async fn tag_create(
    state: State<'_, AppState>,
    id: String,
    name: String,
    target: String,
    annotated: bool,
    message: Option<String>,
) -> Result<(), AppError> {
    let repo_path = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        PathBuf::from(&entry.path)
    };

    // Tags are independent of the checked-out branch, so no branch guard here.
    git_create_tag(
        &state.runner,
        &repo_path,
        &name,
        Some(&target),
        annotated,
        message.as_deref(),
    )
    .await
}

#[tauri::command]
pub async fn tag_push(
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> Result<(), AppError> {
    let repo_path = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        PathBuf::from(&entry.path)
    };

    git_push_tag(&state.runner, &repo_path, &name).await
}

#[tauri::command]
pub async fn tag_push_all(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    let repo_path = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        PathBuf::from(&entry.path)
    };

    git_push_all_tags(&state.runner, &repo_path).await
}

// ---------------- Terminal Commands ----------------

#[tauri::command]
pub async fn term_open(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    id: String,
    cols: u16,
    rows: u16,
) -> Result<TermOpenResult, AppError> {
    let (repo_path, shell) = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        (PathBuf::from(&entry.path), data.settings.shell.clone())
    };

    state
        .term_mgr
        .open(app_handle, id, repo_path, shell, cols, rows)
        .await
}

#[tauri::command]
pub async fn term_input(
    state: State<'_, AppState>,
    id: String,
    data: String,
) -> Result<(), AppError> {
    state.term_mgr.input(&id, &data).await
}

#[tauri::command]
pub async fn term_resize(
    state: State<'_, AppState>,
    id: String,
    cols: u16,
    rows: u16,
) -> Result<(), AppError> {
    state.term_mgr.resize(&id, cols, rows).await
}

#[tauri::command]
pub async fn term_restart(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    id: String,
) -> Result<TermOpenResult, AppError> {
    state.term_mgr.kill(&id).await?;
    term_open(state, app_handle, id, 80, 24).await
}

#[tauri::command]
pub async fn term_kill(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    state.term_mgr.kill(&id).await
}

#[tauri::command]
pub async fn term_paste(
    state: State<'_, AppState>,
    id: String,
    text: String,
) -> Result<(), AppError> {
    state.term_mgr.paste(&id, &text).await
}

// ---------------- Review Notes Commands ----------------

#[tauri::command]
pub async fn note_add(
    state: State<'_, AppState>,
    id: String,
    note: ReviewNote,
) -> Result<ReviewNote, AppError> {
    let mut data = state.data.lock().await;
    let entry = data
        .repos
        .iter_mut()
        .find(|r| r.id == id)
        .ok_or_else(|| AppError::not_found("Repository not found"))?;

    entry.notes.push(note.clone());
    state.persistence.save(&data)?;
    Ok(note)
}

#[tauri::command]
pub async fn note_update(
    state: State<'_, AppState>,
    id: String,
    note: ReviewNote,
) -> Result<ReviewNote, AppError> {
    let mut data = state.data.lock().await;
    let entry = data
        .repos
        .iter_mut()
        .find(|r| r.id == id)
        .ok_or_else(|| AppError::not_found("Repository not found"))?;

    if let Some(existing) = entry.notes.iter_mut().find(|n| n.id == note.id) {
        *existing = note.clone();
    } else {
        entry.notes.push(note.clone());
    }

    state.persistence.save(&data)?;
    Ok(note)
}

#[tauri::command]
pub async fn note_delete(
    state: State<'_, AppState>,
    id: String,
    note_id: String,
) -> Result<(), AppError> {
    let mut data = state.data.lock().await;
    let entry = data
        .repos
        .iter_mut()
        .find(|r| r.id == id)
        .ok_or_else(|| AppError::not_found("Repository not found"))?;

    entry.notes.retain(|n| n.id != note_id);
    state.persistence.save(&data)?;
    Ok(())
}

#[tauri::command]
pub async fn note_clear_all(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    let mut data = state.data.lock().await;
    let entry = data
        .repos
        .iter_mut()
        .find(|r| r.id == id)
        .ok_or_else(|| AppError::not_found("Repository not found"))?;

    entry.notes.clear();
    state.persistence.save(&data)?;
    Ok(())
}

// ---------------- Misc & Settings Commands ----------------

#[tauri::command]
pub async fn settings_get(state: State<'_, AppState>) -> Result<Settings, AppError> {
    let data = state.data.lock().await;
    Ok(data.settings.clone())
}

#[tauri::command]
pub async fn settings_set(state: State<'_, AppState>, settings: Settings) -> Result<(), AppError> {
    let mut data = state.data.lock().await;
    let mut incoming = settings;
    // Preserve unknown settings written by newer versions; the frontend only
    // knows the fields it was built with.
    for (key, value) in data.settings.extra.iter() {
        incoming
            .extra
            .entry(key.clone())
            .or_insert_with(|| value.clone());
    }
    data.settings = incoming.sanitized();
    // Settings change one field at a time from the UI; coalesce disk writes.
    state.persistence.save_debounced(&data)?;
    Ok(())
}

#[tauri::command]
pub async fn layout_get(state: State<'_, AppState>) -> Result<Layout, AppError> {
    let data = state.data.lock().await;
    Ok(data.layout.clone())
}

#[tauri::command]
pub async fn layout_set(state: State<'_, AppState>, layout: Layout) -> Result<(), AppError> {
    let mut data = state.data.lock().await;

    // MERGE RULE: `windowBounds` is owned by the native window event path in
    // `window.rs`, not by the frontend. The frontend builds `Layout` with
    // `windowBounds: null`, so a `null` here means "no opinion" and MUST
    // preserve the stored geometry. Only an explicit `Some(bounds)` replaces
    // it. Do not blanket-assign `data.layout = layout` again; that silently
    // clobbers restored window geometry.
    let merged = crate::window::apply_layout_window_bounds(&data.layout, layout);
    data.layout = merged;
    // Keep the legacy mirror in sync when a caller explicitly supplies bounds.
    if let Some(bounds) = data.layout.window_bounds.clone() {
        data.window_bounds = Some(bounds);
    }

    // Layout is persisted on a 500ms debounce in the UI already; coalesce again
    // in the backend so resizing does not hammer the disk.
    state.persistence.save_debounced(&data)?;
    Ok(())
}

#[tauri::command]
pub async fn open_external_editor(
    state: State<'_, AppState>,
    id: String,
    path: String,
    line: Option<u32>,
) -> Result<(), AppError> {
    let (repo_path, cmd_template) = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        (
            entry.path.clone(),
            data.settings.external_editor_cmd.clone(),
        )
    };

    // No configured editor means "system default app", which is always
    // launchable, rather than a hard-coded `code` that may not be installed.
    let template = cmd_template.unwrap_or_else(|| system_editor_command().to_string());
    let full_path = PathBuf::from(&repo_path).join(&path);
    let line_str = line.unwrap_or(1).to_string();

    let tokens = split_command_template(&template);
    if tokens.is_empty() {
        return Err(AppError::new(
            "Validation",
            "Editor command template is empty. Choose an editor in Settings → Integrations.",
        ));
    }

    let program = &tokens[0];
    let args: Vec<String> = tokens[1..]
        .iter()
        .map(|tok| {
            tok.replace("{repo}", &repo_path)
                .replace("{file}", &path)
                .replace("{line}", &line_str)
                .replace("{full}", &full_path.to_string_lossy())
        })
        .collect();

    tokio::process::Command::new(program)
        .args(&args)
        .spawn()
        .map_err(|e| {
            AppError::new(
                "EditorLaunch",
                format!(
                    "Could not launch editor '{}': {}. Choose an installed editor in Settings → Integrations.",
                    program, e
                ),
            )
        })?;

    Ok(())
}

/// Split an editor command template into arguments. Supports single and
/// double quotes (so macOS app-bundle paths with spaces work) and backslash
/// escapes. Deliberately does not run a shell.
pub fn split_command_template(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut has_token = false;

    for ch in input.chars() {
        if escaped {
            current.push(ch);
            has_token = true;
            escaped = false;
            continue;
        }
        match ch {
            '\\' if quote != Some('\'') => {
                escaped = true;
                has_token = true;
            }
            '"' | '\'' => {
                if quote == Some(ch) {
                    quote = None;
                } else if quote.is_none() {
                    quote = Some(ch);
                    has_token = true;
                } else {
                    current.push(ch);
                    has_token = true;
                }
            }
            c if c.is_whitespace() && quote.is_none() => {
                if has_token {
                    tokens.push(std::mem::take(&mut current));
                    has_token = false;
                }
            }
            c => {
                current.push(c);
                has_token = true;
            }
        }
    }

    if has_token {
        tokens.push(current);
    }
    tokens
}

#[tauri::command]
pub async fn reveal_in_file_manager(path: String) -> Result<(), AppError> {
    let p = PathBuf::from(&path);
    #[cfg(target_os = "macos")]
    {
        tokio::process::Command::new("open")
            .arg("-R")
            .arg(&p)
            .spawn()
            .map_err(AppError::io)?;
    }
    #[cfg(target_os = "windows")]
    {
        tokio::process::Command::new("explorer")
            .arg(format!("/select,{}", p.display()))
            .spawn()
            .map_err(AppError::io)?;
    }
    #[cfg(target_os = "linux")]
    {
        let parent = p.parent().unwrap_or(&p);
        tokio::process::Command::new("xdg-open")
            .arg(parent)
            .spawn()
            .map_err(AppError::io)?;
    }
    Ok(())
}

/// Only absolute http(s) URLs may be handed to the OS: a rendered document must
/// never be able to launch `file:`, `javascript:` or arbitrary app schemes.
fn validate_open_url(url: &str) -> Result<&str, AppError> {
    let trimmed = url.trim();
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        Ok(trimmed)
    } else {
        Err(AppError::new(
            "Validation",
            "Only http and https links can be opened.",
        ))
    }
}

/// Open an http(s) URL in the system browser.
#[tauri::command]
pub async fn open_url(url: String) -> Result<(), AppError> {
    let trimmed = validate_open_url(&url)?;

    #[cfg(target_os = "macos")]
    {
        tokio::process::Command::new("open")
            .arg(trimmed)
            .spawn()
            .map_err(AppError::io)?;
    }
    #[cfg(target_os = "windows")]
    {
        tokio::process::Command::new("cmd")
            .args(["/C", "start", "", trimmed])
            .spawn()
            .map_err(AppError::io)?;
    }
    #[cfg(target_os = "linux")]
    {
        tokio::process::Command::new("xdg-open")
            .arg(trimmed)
            .spawn()
            .map_err(AppError::io)?;
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitMsgResult {
    pub summary: String,
    pub body: Option<String>,
}

#[tauri::command]
pub async fn run_commit_message_cmd(
    state: State<'_, AppState>,
    id: String,
    paths: Vec<String>,
) -> Result<CommitMsgResult, AppError> {
    let (repo_path, cmd_str) = {
        let data = state.data.lock().await;
        let entry = data
            .repos
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::not_found("Repository not found"))?;
        let cmd = data
            .settings
            .commit_message_cmd
            .as_ref()
            .ok_or_else(|| AppError::other("Commit message command is not configured"))?
            .clone();
        (PathBuf::from(&entry.path), cmd)
    };

    // Get diff of selected paths using literal pathspecs
    let mut diff_args = vec!["diff", "HEAD", "--"];
    for p in &paths {
        diff_args.push(p.as_str());
    }
    let diff_out = state
        .runner
        .run_read_literal(&repo_path, &diff_args)
        .await?;
    let diff_text = diff_out.stdout_string();

    use std::process::Stdio;
    use std::time::Duration;
    use tokio::io::AsyncWriteExt;

    #[cfg(windows)]
    let mut cmd = tokio::process::Command::new("cmd");
    #[cfg(windows)]
    cmd.arg("/C").arg(&cmd_str);

    #[cfg(not(windows))]
    let mut cmd = tokio::process::Command::new("sh");
    #[cfg(not(windows))]
    cmd.arg("-c").arg(&cmd_str);

    let mut child = cmd
        .current_dir(&repo_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(AppError::io)?;

    if let Some(mut stdin) = child.stdin.take() {
        let diff_bytes = diff_text.into_bytes();
        tokio::spawn(async move {
            let _ = stdin.write_all(&diff_bytes).await;
            let _ = stdin.flush().await;
            drop(stdin);
        });
    }

    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();

    let wait_fut = async {
        let mut stdout_buf = Vec::new();
        let mut stderr_buf = Vec::new();
        let read_out = async {
            if let Some(mut s) = stdout_pipe {
                use tokio::io::AsyncReadExt;
                let _ = s.read_to_end(&mut stdout_buf).await;
            }
        };
        let read_err = async {
            if let Some(mut s) = stderr_pipe {
                use tokio::io::AsyncReadExt;
                let _ = s.read_to_end(&mut stderr_buf).await;
            }
        };
        let (status, _, _) = tokio::join!(child.wait(), read_out, read_err);
        (status, stdout_buf, stderr_buf)
    };

    let (status_res, stdout_buf, stderr_buf) =
        match tokio::time::timeout(Duration::from_secs(30), wait_fut).await {
            Ok(res) => res,
            Err(_) => {
                let _ = child.start_kill();
                let _ = child.wait().await;
                return Err(AppError::timeout(
                    "Commit message draft command timed out after 30s",
                ));
            }
        };

    let status = status_res.map_err(AppError::io)?;

    if !status.success() {
        return Err(AppError::other(String::from_utf8_lossy(&stderr_buf)));
    }

    let out_str = String::from_utf8_lossy(&stdout_buf).to_string();
    let mut lines = out_str.lines();
    let summary = lines.next().unwrap_or("").trim().to_string();
    let body_lines: Vec<&str> = lines.collect();
    let body = if body_lines.is_empty() {
        None
    } else {
        Some(body_lines.join("\n").trim().to_string())
    };

    Ok(CommitMsgResult { summary, body })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvInfo {
    pub git_version: String,
    pub shell: String,
    pub startup_warning: Option<String>,
    pub default_shell: String,
    pub shells: Vec<DetectedApp>,
    pub editors: Vec<DetectedApp>,
}

#[tauri::command]
pub async fn env_check(state: State<'_, AppState>) -> Result<EnvInfo, AppError> {
    let out = state
        .runner
        .run(None, &["--version"], 5, false)
        .await
        .map_err(|_| AppError::git_missing())?;

    let git_version = out.stdout_string().trim().to_string();
    let default_shell = TerminalManager::default_shell();
    let shells = detect_shells(&default_shell);
    let editors = detect_editors();
    let startup_warning = state.startup_warning.clone();

    Ok(EnvInfo {
        git_version,
        shell: default_shell.clone(),
        startup_warning,
        default_shell,
        shells,
        editors,
    })
}

#[tauri::command]
pub async fn git_identity_get(
    state: State<'_, AppState>,
    id: Option<String>,
) -> Result<GitIdentity, AppError> {
    let repo_path = match id {
        Some(id) => {
            let data = state.data.lock().await;
            let entry = data
                .repos
                .iter()
                .find(|r| r.id == id)
                .ok_or_else(|| AppError::not_found("Repository not found"))?;
            Some(PathBuf::from(&entry.path))
        }
        None => None,
    };

    read_identity(&state.runner, repo_path.as_deref()).await
}

#[tauri::command]
pub async fn git_identity_set(
    state: State<'_, AppState>,
    id: Option<String>,
    scope: String,
    name: Option<String>,
    email: Option<String>,
) -> Result<GitIdentity, AppError> {
    let scope = IdentityScope::parse(&scope)?;
    let repo_path = match (scope, id) {
        (IdentityScope::Repo, Some(id)) => {
            let data = state.data.lock().await;
            let entry = data
                .repos
                .iter()
                .find(|r| r.id == id)
                .ok_or_else(|| AppError::not_found("Repository not found"))?;
            Some(PathBuf::from(&entry.path))
        }
        (IdentityScope::Repo, None) => {
            return Err(AppError::new(
                "Validation",
                "Select a repository to set repository-specific identity.",
            ));
        }
        (IdentityScope::Global, _) => None,
    };

    write_identity(
        &state.runner,
        repo_path.as_deref(),
        scope,
        name.as_deref(),
        email.as_deref(),
    )
    .await?;

    read_identity(&state.runner, repo_path.as_deref()).await
}

#[tauri::command]
pub async fn pick_directory() -> Result<Option<String>, AppError> {
    let folder = rfd::AsyncFileDialog::new()
        .set_title("Select Git Repository")
        .pick_folder()
        .await;

    Ok(folder.map(|handle| {
        dunce::canonicalize(handle.path())
            .unwrap_or_else(|_| handle.path().to_path_buf())
            .to_string_lossy()
            .to_string()
    }))
}

fn to_base64(data: &[u8]) -> String {
    const CHARSET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };
        result.push(CHARSET[(b0 >> 2) as usize] as char);
        result.push(CHARSET[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARSET[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARSET[(b2 & 0x3f) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoStackInfo {
    pub detected: Option<String>,
    pub label: Option<String>,
    pub matched_file: Option<String>,
}

#[tauri::command]
pub async fn repo_detect_stack(path: String) -> Result<RepoStackInfo, AppError> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Ok(RepoStackInfo {
            detected: None,
            label: None,
            matched_file: None,
        });
    }

    // 1. Rust
    if p.join("Cargo.toml").exists() {
        return Ok(RepoStackInfo {
            detected: Some("rust".to_string()),
            label: Some("Rust".to_string()),
            matched_file: Some("Cargo.toml".to_string()),
        });
    }

    // 2. Node / JS / TS ecosystem
    if p.join("package.json").exists() {
        if let Ok(content) = tokio::fs::read_to_string(p.join("package.json")).await {
            let lower = content.to_lowercase();
            if lower.contains("\"next\"") {
                return Ok(RepoStackInfo {
                    detected: Some("nextjs".to_string()),
                    label: Some("Next.js".to_string()),
                    matched_file: Some("package.json".to_string()),
                });
            }
            if lower.contains("\"svelte\"") {
                return Ok(RepoStackInfo {
                    detected: Some("svelte".to_string()),
                    label: Some("Svelte".to_string()),
                    matched_file: Some("package.json".to_string()),
                });
            }
            if lower.contains("\"vue\"") || lower.contains("\"nuxt\"") {
                return Ok(RepoStackInfo {
                    detected: Some("vue".to_string()),
                    label: Some("Vue".to_string()),
                    matched_file: Some("package.json".to_string()),
                });
            }
            if lower.contains("\"react\"") {
                return Ok(RepoStackInfo {
                    detected: Some("react".to_string()),
                    label: Some("React".to_string()),
                    matched_file: Some("package.json".to_string()),
                });
            }
            if lower.contains("\"@angular\"") {
                return Ok(RepoStackInfo {
                    detected: Some("angular".to_string()),
                    label: Some("Angular".to_string()),
                    matched_file: Some("package.json".to_string()),
                });
            }
        }
        if p.join("tsconfig.json").exists() {
            return Ok(RepoStackInfo {
                detected: Some("typescript".to_string()),
                label: Some("TypeScript".to_string()),
                matched_file: Some("tsconfig.json".to_string()),
            });
        }
        return Ok(RepoStackInfo {
            detected: Some("nodejs".to_string()),
            label: Some("Node.js".to_string()),
            matched_file: Some("package.json".to_string()),
        });
    }

    // 3. Go
    if p.join("go.mod").exists() {
        return Ok(RepoStackInfo {
            detected: Some("go".to_string()),
            label: Some("Go".to_string()),
            matched_file: Some("go.mod".to_string()),
        });
    }

    // 4. Python
    if p.join("pyproject.toml").exists()
        || p.join("requirements.txt").exists()
        || p.join("Pipfile").exists()
        || p.join("setup.py").exists()
    {
        return Ok(RepoStackInfo {
            detected: Some("python".to_string()),
            label: Some("Python".to_string()),
            matched_file: Some("pyproject.toml".to_string()),
        });
    }

    // 5. Flutter / Dart
    if p.join("pubspec.yaml").exists() {
        return Ok(RepoStackInfo {
            detected: Some("flutter".to_string()),
            label: Some("Flutter".to_string()),
            matched_file: Some("pubspec.yaml".to_string()),
        });
    }

    // 6. Swift / iOS
    if p.join("Package.swift").exists() {
        return Ok(RepoStackInfo {
            detected: Some("swift".to_string()),
            label: Some("Swift".to_string()),
            matched_file: Some("Package.swift".to_string()),
        });
    }

    // 7. Java / Kotlin
    if p.join("build.gradle.kts").exists()
        || p.join("build.gradle").exists()
        || p.join("pom.xml").exists()
    {
        return Ok(RepoStackInfo {
            detected: Some("java".to_string()),
            label: Some("Java/Kotlin".to_string()),
            matched_file: Some("build.gradle".to_string()),
        });
    }

    // 8. Docker
    if p.join("Dockerfile").exists()
        || p.join("docker-compose.yml").exists()
        || p.join("compose.yaml").exists()
    {
        return Ok(RepoStackInfo {
            detected: Some("docker".to_string()),
            label: Some("Docker".to_string()),
            matched_file: Some("Dockerfile".to_string()),
        });
    }

    // 9. Ruby
    if p.join("Gemfile").exists() {
        return Ok(RepoStackInfo {
            detected: Some("ruby".to_string()),
            label: Some("Ruby".to_string()),
            matched_file: Some("Gemfile".to_string()),
        });
    }

    // 10. PHP
    if p.join("composer.json").exists() {
        return Ok(RepoStackInfo {
            detected: Some("php".to_string()),
            label: Some("PHP".to_string()),
            matched_file: Some("composer.json".to_string()),
        });
    }

    Ok(RepoStackInfo {
        detected: None,
        label: None,
        matched_file: None,
    })
}

#[tauri::command]
pub async fn pick_icon_file() -> Result<Option<String>, AppError> {
    let file = rfd::AsyncFileDialog::new()
        .set_title("Select Repository Icon")
        .add_filter("Images", &["png", "svg", "ico", "jpg", "jpeg", "webp"])
        .pick_file()
        .await;

    if let Some(handle) = file {
        let path = handle.path();
        let bytes = tokio::fs::read(path).await.map_err(AppError::io)?;
        if bytes.len() > 512 * 1024 {
            return Err(AppError::new(
                "Validation",
                "Icon image must be smaller than 512KB",
            ));
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("png")
            .to_lowercase();
        let mime = match ext.as_str() {
            "svg" => "image/svg+xml",
            "ico" => "image/x-icon",
            "jpg" | "jpeg" => "image/jpeg",
            "webp" => "image/webp",
            _ => "image/png",
        };
        let b64 = format!("data:{};base64,{}", mime, to_base64(&bytes));
        Ok(Some(b64))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::split_command_template;

    #[test]
    fn splits_simple_commands() {
        assert_eq!(
            split_command_template("code -g {full}:{line}"),
            vec!["code", "-g", "{full}:{line}"]
        );
    }

    #[test]
    fn splits_quoted_paths_with_spaces() {
        assert_eq!(
            split_command_template(
                "\"/Applications/Visual Studio Code.app/bin/code\" -g {full}:{line}"
            ),
            vec![
                "/Applications/Visual Studio Code.app/bin/code",
                "-g",
                "{full}:{line}"
            ]
        );
        assert_eq!(
            split_command_template("'/Applications/Sublime Text.app/subl' {full}"),
            vec!["/Applications/Sublime Text.app/subl", "{full}"]
        );
    }

    #[test]
    fn handles_escapes_and_empty_input() {
        assert_eq!(
            split_command_template("my\\ editor {full}"),
            vec!["my editor", "{full}"]
        );
        assert!(split_command_template("   ").is_empty());
        assert_eq!(split_command_template("open \"\""), ["open", ""]);
    }

    #[test]
    fn reorders_repos_by_order_and_pinned() {
        let mut repos = [
            crate::state::RepoEntry {
                id: "1".into(),
                path: "/a".into(),
                name: "a".into(),
                branch: None,
                pinned: false,
                order: 0,
                marker: None,
                notes: vec![],
            },
            crate::state::RepoEntry {
                id: "2".into(),
                path: "/b".into(),
                name: "b".into(),
                branch: None,
                pinned: true,
                order: 1,
                marker: None,
                notes: vec![],
            },
        ];
        repos.sort_by_key(|r| (if r.pinned { 0 } else { 1 }, r.order));
        assert_eq!(repos[0].id, "2");
        assert_eq!(repos[1].id, "1");
    }
}

#[cfg(test)]
mod open_url_tests {
    use super::validate_open_url;

    #[test]
    fn accepts_http_and_https() {
        assert!(validate_open_url("https://example.com/a?b=c").is_ok());
        assert!(validate_open_url("  http://localhost:3000  ").is_ok());
    }

    #[test]
    fn rejects_other_schemes_and_relative_targets() {
        for bad in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/html,<script>alert(1)</script>",
            "ssh://host",
            "/relative/path",
            "",
        ] {
            assert!(validate_open_url(bad).is_err(), "{bad} should be rejected");
        }
    }
}
