use crate::git::refs::sync_fetch as git_sync_fetch;
use crate::git::runner::GitRunner;
use crate::git::status::query_repo_status;
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct RepoWatcherManager {
    watchers: Arc<Mutex<HashMap<String, RecommendedWatcher>>>,
    active_poll: Arc<AtomicBool>,
    last_known_head: Arc<Mutex<HashMap<String, String>>>,
}

impl Default for RepoWatcherManager {
    fn default() -> Self {
        Self::new()
    }
}

impl RepoWatcherManager {
    pub fn new() -> Self {
        Self {
            watchers: Arc::new(Mutex::new(HashMap::new())),
            active_poll: Arc::new(AtomicBool::new(true)),
            last_known_head: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn set_polling_active(&self, active: bool) {
        self.active_poll.store(active, Ordering::SeqCst);
    }

    pub async fn watch_repo(
        &self,
        app_handle: AppHandle,
        repo_id: String,
        repo_path: PathBuf,
        branch: Option<String>,
        marker_sha: Option<String>,
        runner: Arc<GitRunner>,
    ) {
        let mut watchers = self.watchers.lock().await;
        // Don't duplicate watcher
        if watchers.contains_key(&repo_id) {
            return;
        }

        // Resolve git dir (handles standard .git dir, gitdir file for submodules/worktrees)
        let git_dir = if let Ok(out) = runner
            .run_read(&repo_path, &["rev-parse", "--git-dir"])
            .await
        {
            if out.success() {
                let s = out.stdout_string().trim().to_string();
                let p = PathBuf::from(&s);
                if p.is_absolute() {
                    p
                } else {
                    repo_path.join(p)
                }
            } else {
                repo_path.join(".git")
            }
        } else {
            repo_path.join(".git")
        };

        if !git_dir.exists() {
            return;
        }

        // Record initial HEAD
        let target_ref = branch.as_deref().unwrap_or("HEAD");
        if let Ok(out) = runner
            .run_read(&repo_path, &["rev-parse", target_ref])
            .await
        {
            if out.success() {
                let ref_sha = out.stdout_string().trim().to_string();
                let mut heads = self.last_known_head.lock().await;
                heads.insert(repo_id.clone(), ref_sha);
            }
        }

        let app_clone = app_handle.clone();
        let r_id = repo_id.clone();
        let r_path = repo_path.clone();
        let r_branch = branch.clone();
        let m_sha = marker_sha.clone();
        let run_clone = runner.clone();
        let head_tracker = self.last_known_head.clone();
        let tokio_handle = tokio::runtime::Handle::current();

        let mut watcher = match RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    let affects_git = event.paths.iter().any(|p| {
                        let s = p.to_string_lossy();
                        s.contains("HEAD")
                            || s.contains("refs")
                            || s.contains("index")
                            || s.contains("packed-refs")
                    });

                    if affects_git {
                        let app = app_clone.clone();
                        let id = r_id.clone();
                        let p = r_path.clone();
                        let b = r_branch.clone();
                        let m = m_sha.clone();
                        let r = run_clone.clone();
                        let tracker = head_tracker.clone();

                        tokio_handle.spawn(async move {
                            // Debounce brief moment
                            tokio::time::sleep(Duration::from_millis(250)).await;
                            let status =
                                query_repo_status(&r, &id, &p, b.as_deref(), m.as_deref()).await;
                            let _ = app.emit("repo://status", &status);

                            // Check target ref sha
                            let target_ref = b.as_deref().unwrap_or("HEAD");
                            if let Ok(out) = r.run_read(&p, &["rev-parse", target_ref]).await {
                                if out.success() {
                                    let ref_sha = out.stdout_string().trim().to_string();
                                    let mut heads = tracker.lock().await;
                                    let prev_sha = heads.get(&id).cloned();
                                    let changed = match prev_sha {
                                        Some(ref s) => s != &ref_sha,
                                        None => false,
                                    };
                                    heads.insert(id.clone(), ref_sha.clone());
                                    if changed {
                                        let _ = app.emit(
                                            "repo://head-changed",
                                            serde_json::json!({
                                                "repoId": id,
                                                "sha": ref_sha
                                            }),
                                        );
                                    }
                                }
                            }
                        });
                    }
                }
            },
            Config::default(),
        ) {
            Ok(w) => w,
            Err(_) => return,
        };

        // Watch .git/HEAD, .git/refs, .git/index, .git/logs/HEAD, .git/packed-refs
        let head_path = git_dir.join("HEAD");
        if head_path.exists() {
            let _ = watcher.watch(&head_path, RecursiveMode::NonRecursive);
        }
        let refs_path = git_dir.join("refs");
        if refs_path.exists() {
            let _ = watcher.watch(&refs_path, RecursiveMode::Recursive);
        }
        let index_path = git_dir.join("index");
        if index_path.exists() {
            let _ = watcher.watch(&index_path, RecursiveMode::NonRecursive);
        }
        let logs_head = git_dir.join("logs").join("HEAD");
        if logs_head.exists() {
            let _ = watcher.watch(&logs_head, RecursiveMode::NonRecursive);
        }
        let packed_refs = git_dir.join("packed-refs");
        if packed_refs.exists() {
            let _ = watcher.watch(&packed_refs, RecursiveMode::NonRecursive);
        }

        watchers.insert(repo_id, watcher);
    }

    pub async fn unwatch_repo(&self, repo_id: &str) {
        let mut watchers = self.watchers.lock().await;
        watchers.remove(repo_id);
        let mut heads = self.last_known_head.lock().await;
        heads.remove(repo_id);
    }

    /// Run `git fetch --prune` on configured repos at the interval stored in
    /// settings. Fetches go through the same `sync_fetch` path as the manual
    /// button (and therefore the same per-repo lock), and every affected view
    /// gets a fresh status event afterwards. `autoFetchMinutes == 0` disables
    /// the loop; changing the interval takes effect on the next tick without a
    /// restart.
    pub fn start_auto_fetcher(
        &self,
        app_handle: AppHandle,
        runner: Arc<GitRunner>,
        state_data: Arc<Mutex<crate::persist::PersistedState>>,
    ) {
        tauri::async_runtime::spawn(async move {
            let mut last_fetch: HashMap<String, tokio::time::Instant> = HashMap::new();
            let in_flight: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
            let mut ticker = tokio::time::interval(Duration::from_secs(5));

            loop {
                ticker.tick().await;

                let (minutes, repos) = {
                    let data = state_data.lock().await;
                    (data.settings.auto_fetch_minutes, data.repos.clone())
                };

                if minutes == 0 {
                    last_fetch.clear();
                    continue;
                }

                // Branch views share a working copy: fetch once per path, then
                // refresh the status of every view that points at it.
                let mut groups: HashMap<String, Vec<crate::state::RepoEntry>> = HashMap::new();
                for repo in repos {
                    groups.entry(repo.path.clone()).or_default().push(repo);
                }
                let active_paths: HashSet<String> = groups.keys().cloned().collect();
                last_fetch.retain(|path, _| active_paths.contains(path));

                let now = tokio::time::Instant::now();
                for (path, views) in groups {
                    if in_flight.lock().await.contains(&path) {
                        continue;
                    }

                    match last_fetch.get(&path) {
                        None => {
                            // First observation: start the clock instead of
                            // fetching immediately on launch.
                            last_fetch.insert(path, now);
                            continue;
                        }
                        Some(previous)
                            if !auto_fetch_due(now.duration_since(*previous).as_secs(), minutes) =>
                        {
                            continue;
                        }
                        _ => {}
                    }

                    last_fetch.insert(path.clone(), now);
                    in_flight.lock().await.insert(path.clone());

                    let app = app_handle.clone();
                    let r = runner.clone();
                    let flags = in_flight.clone();
                    tauri::async_runtime::spawn(async move {
                        let repo_path = PathBuf::from(&path);
                        // Errors (offline, auth, no remote) stay silent: the
                        // next interval retries and the status row remains the
                        // source of truth.
                        let _ = git_sync_fetch(&r, &repo_path).await;

                        for view in views {
                            let marker = view.marker.as_ref().map(|m| m.sha.as_str());
                            let status = query_repo_status(
                                &r,
                                &view.id,
                                &repo_path,
                                view.branch.as_deref(),
                                marker,
                            )
                            .await;
                            let _ = app.emit("repo://status", &status);
                        }

                        flags.lock().await.remove(&path);
                    });
                }
            }
        });
    }

    pub fn start_poller(
        &self,
        app_handle: AppHandle,
        runner: Arc<GitRunner>,
        state_data: Arc<Mutex<crate::persist::PersistedState>>,
    ) {
        let active_poll = self.active_poll.clone();
        let last_known_head = self.last_known_head.clone();

        tauri::async_runtime::spawn(async move {
            let mut last_polled: HashMap<String, tokio::time::Instant> = HashMap::new();
            let running: Arc<Mutex<std::collections::HashSet<String>>> =
                Arc::new(Mutex::new(std::collections::HashSet::new()));
            let slow_intervals: Arc<Mutex<HashMap<String, Duration>>> =
                Arc::new(Mutex::new(HashMap::new()));

            let mut interval_timer = tokio::time::interval(Duration::from_millis(1000));

            loop {
                interval_timer.tick().await;

                if !active_poll.load(Ordering::SeqCst) {
                    continue;
                }

                // Snapshot repos and layout from state_data
                let (repos, visible_ids) = {
                    let data = state_data.lock().await;
                    let visible: std::collections::HashSet<String> = data
                        .layout
                        .panes
                        .iter()
                        .take(data.layout.pane_count as usize)
                        .filter_map(|p| p.repo_id.clone())
                        .collect();
                    (data.repos.clone(), visible)
                };

                let now = tokio::time::Instant::now();

                for repo in repos {
                    let is_visible = visible_ids.contains(&repo.id);
                    let base_interval = if is_visible {
                        Duration::from_secs(4)
                    } else {
                        Duration::from_secs(20)
                    };

                    let target_interval = {
                        let slows = slow_intervals.lock().await;
                        slows
                            .get(&repo.id)
                            .copied()
                            .unwrap_or(base_interval)
                            .max(base_interval)
                    };

                    let should_poll = match last_polled.get(&repo.id) {
                        Some(last) => now.duration_since(*last) >= target_interval,
                        None => {
                            // On first sighting, record timestamp so background polling waits
                            // its normal interval instead of colliding with the initial UI load.
                            last_polled.insert(repo.id.clone(), now);
                            false
                        }
                    };

                    if !should_poll {
                        continue;
                    }

                    // Skip cycle if previous status call is still running for this repo
                    {
                        let running_set = running.lock().await;
                        if running_set.contains(&repo.id) {
                            continue;
                        }
                    }

                    last_polled.insert(repo.id.clone(), now);

                    let r_id = repo.id.clone();
                    let r_path = PathBuf::from(&repo.path);
                    let r_branch = repo.branch.clone();
                    let m_sha = repo.marker.as_ref().map(|m| m.sha.clone());
                    let app = app_handle.clone();
                    let r = runner.clone();
                    let run_flag = running.clone();
                    let slow_flag = slow_intervals.clone();
                    let head_tracker = last_known_head.clone();

                    tauri::async_runtime::spawn(async move {
                        {
                            let mut set = run_flag.lock().await;
                            set.insert(r_id.clone());
                        }

                        let start_t = tokio::time::Instant::now();
                        let status = query_repo_status(
                            &r,
                            &r_id,
                            &r_path,
                            r_branch.as_deref(),
                            m_sha.as_deref(),
                        )
                        .await;
                        let elapsed = start_t.elapsed();

                        // If status call exceeds 5s or returned an error (e.g. permission/disk issue),
                        // back off interval to avoid hammering the filesystem.
                        {
                            let mut slows = slow_flag.lock().await;
                            if elapsed > Duration::from_secs(5) || status.error.is_some() {
                                let curr = slows.entry(r_id.clone()).or_insert(base_interval);
                                *curr = (*curr * 2).min(Duration::from_secs(60));
                            } else {
                                slows.remove(&r_id);
                            }
                        }

                        let _ = app.emit("repo://status", &status);

                        // Check target ref sha
                        let target_ref = r_branch.as_deref().unwrap_or("HEAD");
                        if let Ok(out) = r.run_read(&r_path, &["rev-parse", target_ref]).await {
                            if out.success() {
                                let ref_sha = out.stdout_string().trim().to_string();
                                let mut heads = head_tracker.lock().await;
                                let prev_sha = heads.get(&r_id).cloned();
                                let changed = match prev_sha {
                                    Some(ref s) => s != &ref_sha,
                                    None => false,
                                };
                                heads.insert(r_id.clone(), ref_sha.clone());
                                if changed {
                                    let _ = app.emit(
                                        "repo://head-changed",
                                        serde_json::json!({
                                            "repoId": r_id,
                                            "sha": ref_sha
                                        }),
                                    );
                                }
                            }
                        }

                        {
                            let mut set = run_flag.lock().await;
                            set.remove(&r_id);
                        }
                    });
                }
            }
        });
    }
}

/// Whether an auto-fetch is due. `0` minutes disables fetching entirely.
pub fn auto_fetch_due(elapsed_secs: u64, interval_minutes: u32) -> bool {
    interval_minutes > 0 && elapsed_secs >= u64::from(interval_minutes) * 60
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_fetch_interval_is_honored() {
        assert!(!auto_fetch_due(0, 0));
        assert!(!auto_fetch_due(10_000, 0), "zero means off");
        assert!(!auto_fetch_due(299, 5));
        assert!(auto_fetch_due(300, 5));
        assert!(auto_fetch_due(60, 1));
        assert!(auto_fetch_due(3_600, 60));
    }
}
