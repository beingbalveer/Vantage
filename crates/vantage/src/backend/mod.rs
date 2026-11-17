//! Adapter from Vantage's async core engine to Vantage's view model.
//!
//! `vantage-core` drives git through `tokio::process`, while Vantage renders
//! on GPUI's executor. The adapter owns a private [`Runtime`] and exposes
//! synchronous methods that `block_on` internally, so callers do all git work on
//! a background thread and hand the finished model back to the UI thread.
//!
//! Workspace state is read and written through `vantage_core::repos`, which
//! means this dashboard and the Vantage app share one `state.json`: a repo added
//! in either surface shows up in both.

pub mod details;
pub mod repo_map;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use tokio::runtime::Runtime;
use vantage_core::git::ops;
pub use vantage_core::git::refs::BranchDeleteOutcome;
use vantage_core::git::refs::{
    checkout_branch, create_branch, delete_branch, has_upstream, is_commit_on_remote,
    list_branches, merge_into_current, rename_branch, sync_fetch, sync_pull, sync_push,
    PullStrategy,
};
use vantage_core::git::stash::{stash_drop, stash_pop, stash_push};
use vantage_core::git::status::query_repo_overview;
use vantage_core::persist::{PersistedState, StateBackup, StatePersistence};
use vantage_core::{repos, GitRunner, RepoEntry};

use crate::model::{FileChangeStatus, PreviewLine, Repository};
use crate::settings::UiSettings;

/// Key in `PersistedState.extra` holding Vantage's own grouping settings.
const EXTRA_KEY: &str = "vantage";
/// Key this app's grouping settings were written under before the rename.
///
/// State saved by the old build still carries groups and assignments there, so
/// the backend reads it as a fallback and migrates it on startup.
const LEGACY_EXTRA_KEY: &str = "branchdeck";
/// Workspace of a repository the user has not assigned to a group.
const UNGROUPED: &str = "Ungrouped";

/// Seconds since the Unix epoch, clamped to the range `i64` can hold.
pub(crate) fn now_seconds() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(elapsed) => elapsed.as_secs().min(i64::MAX as u64) as i64,
        Err(_) => 0,
    }
}

/// `just now` / `14m ago` / `3h ago` / `2d ago` / `1w ago` / `3mo ago` /
/// `1y ago`. Shared by the repository rows and the commit lists so both use
/// the same scale.
pub(crate) fn relative_time_label(now: i64, then: i64) -> String {
    let minutes = now.saturating_sub(then).max(0) / 60;
    if minutes < 1 {
        "just now".to_string()
    } else if minutes < 60 {
        format!("{minutes}m ago")
    } else if minutes < 60 * 24 {
        format!("{}h ago", minutes / 60)
    } else if minutes < 60 * 24 * 7 {
        format!("{}d ago", minutes / (60 * 24))
    } else if minutes < 60 * 24 * 30 {
        format!("{}w ago", minutes / (60 * 24 * 7))
    } else if minutes < 60 * 24 * 365 {
        format!("{}mo ago", minutes / (60 * 24 * 30))
    } else {
        format!("{}y ago", minutes / (60 * 24 * 365))
    }
}

pub struct Backend {
    /// Private engine runtime: every `block_on` in this module runs on it.
    runtime: Runtime,
    runner: GitRunner,
    /// Test-only persistence override. Production goes through the
    /// process-wide handle in [`vantage_core::repos`].
    persistence: Option<StatePersistence>,
    /// The startup load warning, kept until the UI can show it. Later loads
    /// only fill it when it is still empty, so a startup notice is not lost.
    state_warning: Mutex<Option<String>>,
}

impl Backend {
    pub fn new() -> Result<Self> {
        Self::with_persistence(None)
    }

    fn with_persistence(persistence: Option<StatePersistence>) -> Result<Self> {
        let runtime = Runtime::new().context("start the git worker runtime")?;
        let backend = Self {
            runtime,
            runner: GitRunner::new(),
            persistence,
            state_warning: Mutex::new(None),
        };

        let (_, warning) = backend.load_state();
        if let Some(warning) = warning {
            if let Ok(mut slot) = backend.state_warning.lock() {
                *slot = Some(warning);
            }
        }
        backend.migrate_legacy_groups();
        Ok(backend)
    }

    #[cfg(test)]
    pub(crate) fn with_state_path(path: PathBuf) -> Result<Self> {
        Self::with_persistence(Some(StatePersistence::with_path(path)))
    }

    /// Take the warning produced by a state load. The UI uses this once at
    /// startup; `None` means the state file was read cleanly.
    pub fn take_state_warning(&self) -> Option<String> {
        self.state_warning
            .lock()
            .ok()
            .and_then(|mut warning| warning.take())
    }

    /// Full snapshot: every persisted repo mapped to the UI model. Repositories
    /// whose git state cannot be read are returned in `errors` instead (with
    /// git's own message), so one broken work tree cannot blank the dashboard.
    pub fn snapshot(&self) -> (Vec<Repository>, Vec<String>) {
        self.snapshot_matching(|_| true)
    }

    /// Snapshot only the repositories whose work tree is one of `roots`.
    ///
    /// Focused views use this to re-read just the repositories they show
    /// instead of paying for a full workspace scan.
    pub fn snapshot_subset(&self, roots: &[PathBuf]) -> (Vec<Repository>, Vec<String>) {
        self.snapshot_matching(|path| roots.iter().any(|root| root.as_path() == path))
    }

    /// Map the persisted repos whose absolute path satisfies `include`, in
    /// sidebar order: pinned repositories first, then the persisted `order`.
    ///
    /// Every repository is independent, so a small worker pool maps several at
    /// once; each worker runs one git process at a time, staying well inside
    /// `GitRunner`'s semaphore. Results are reassembled in the persisted order
    /// and a failure still only drops its own row.
    fn snapshot_matching(&self, include: impl Fn(&Path) -> bool) -> (Vec<Repository>, Vec<String>) {
        const MAX_MAP_WORKERS: usize = 4;

        let state = self.state();
        let (_, assignments) = groups_from(&state);
        let mut entries: Vec<&RepoEntry> = state
            .repos
            .iter()
            .filter(|entry| include(Path::new(&entry.path)))
            .collect();
        entries.sort_by_key(|repo| (if repo.pinned { 0 } else { 1 }, repo.order));

        map_ordered_bounded(&entries, MAX_MAP_WORKERS, |entry| {
            self.map_entry(entry, group_of(&assignments, &entry.id))
        })
    }

    /// Add a folder as a repo; returns the mapped row.
    ///
    /// The entry is mapped before the workspace is saved: if its git state
    /// cannot be read the add fails without persisting, so a retry is not
    /// rejected as a duplicate of a repo the UI never showed.
    pub fn add_repo(&self, path: &str) -> Result<Repository> {
        let mut state = self.state();
        let entry = self
            .runtime
            .block_on(repos::add(&self.runner, &mut state, Path::new(path)))?;

        let (_, assignments) = groups_from(&state);
        let repository = self
            .map_entry(&entry, group_of(&assignments, &entry.id))
            .map_err(anyhow::Error::msg)?;
        // Persist only the new entry instead of the snapshot read above: a
        // queued settings write may have changed other fields in the meantime.
        let path = entry.path.clone();
        self.update_now(move |state| {
            if state.repos.iter().any(|repo| repo.path == path) {
                return Err(vantage_core::AppError::new(
                    "DuplicateRepo",
                    format!("'{}' is already in the workspace.", path),
                ));
            }
            state.repos.push(entry);
            Ok(())
        })?;
        Ok(repository)
    }

    /// `git fetch --prune` for one repo. The caller re-scans the workspace once
    /// this returns.
    pub fn fetch(&self, repo_id: &str) -> Result<()> {
        let path = self.known_path(repo_id)?;
        self.runtime.block_on(sync_fetch(&self.runner, &path))?;
        Ok(())
    }

    /// `git pull` for one repo, reconciling diverged history with `strategy`.
    /// The caller re-scans the workspace once this returns.
    pub fn pull(&self, repo_id: &str, strategy: PullStrategy) -> Result<()> {
        let path = self.known_path(repo_id)?;
        self.runtime
            .block_on(sync_pull(&self.runner, &path, None, strategy))?;
        Ok(())
    }

    /// `git push` for one repo.
    ///
    /// A branch that tracks nothing is published with `--set-upstream`, so the
    /// first push of a local branch needs no separate step. Which case applies
    /// is decided here rather than in the UI, so a stale row cannot push with
    /// the wrong intent.
    pub fn push(&self, repo_id: &str) -> Result<()> {
        let path = self.known_path(repo_id)?;
        let publish = !self.runtime.block_on(has_upstream(&self.runner, &path))?;
        self.runtime
            .block_on(sync_push(&self.runner, &path, publish, None))?;
        Ok(())
    }

    /// Fetch several repositories with a small bounded worker pool. Returns
    /// `(repo_id, error)` for the failures so the UI can report them without
    /// blocking a successful background run.
    pub fn fetch_all(&self, repo_ids: &[String]) -> Vec<(String, String)> {
        const MAX_PARALLEL: usize = 4;
        let mut failures = Vec::new();
        for chunk in repo_ids.chunks(MAX_PARALLEL) {
            std::thread::scope(|scope| {
                let handles: Vec<_> = chunk
                    .iter()
                    .map(|repo_id| {
                        scope.spawn(|| {
                            let result = self.fetch(repo_id);
                            (repo_id.clone(), result)
                        })
                    })
                    .collect();
                for handle in handles {
                    match handle.join() {
                        Ok((_repo_id, Ok(()))) => {}
                        Ok((repo_id, Err(error))) => {
                            failures.push((repo_id, error.to_string()));
                        }
                        Err(_) => failures
                            .push(("unknown".to_string(), "fetch worker panicked".to_string())),
                    }
                }
            });
        }
        failures
    }

    /// v2 preferences, importing only the legacy theme when no v2 object has
    /// been written yet.
    pub fn ui_settings(&self) -> UiSettings {
        UiSettings::from_state(&self.state())
    }

    /// Queue v2 preferences without disturbing groups, assignments or any
    /// other key in the shared `vantage` object.
    ///
    /// This is the hot path for selection and layout clicks, so it is coalesced
    /// by the background writer instead of rewriting the whole state file on
    /// every event. Call [`Backend::flush_state`] before quitting.
    pub fn set_ui_settings(&self, settings: &UiSettings) -> Result<()> {
        let settings = settings.clone();
        repos::update_state_debounced(move |state| {
            sets_ui_settings_in(state, &settings)
                .map_err(|error| vantage_core::AppError::other(error.to_string()))
        })
        .context("save Vantage settings")
    }

    /// Wait for queued preference writes to reach disk.
    pub fn flush_state(&self) -> Result<()> {
        repos::flush_state().context("flush the workspace state")
    }

    /// The oldest asynchronous persistence problem, if any. The UI polls this
    /// so a failed debounced write still becomes a visible toast.
    pub fn take_state_notice(&self) -> Option<String> {
        repos::take_state_notice()
    }

    /// Absolute path of the workspace state file, from the cached handle.
    pub fn state_path(&self) -> std::path::PathBuf {
        match &self.persistence {
            Some(persistence) => persistence.path().to_path_buf(),
            None => repos::state_path().unwrap_or_default(),
        }
    }

    /// Directory that holds the state file and its backups.
    pub fn state_data_dir(&self) -> PathBuf {
        self.state_path()
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default()
    }

    /// Recoverable state files beside the live `state.json`, newest first.
    pub fn state_backups(&self) -> Vec<StateBackup> {
        match &self.persistence {
            Some(persistence) => persistence.backups().unwrap_or_default(),
            None => repos::state_backups(),
        }
    }

    /// Check out `branch` in one repo. The caller re-reads the row with
    /// [`Backend::refresh`] once this returns.
    pub fn checkout(&self, repo_id: &str, branch: &str) -> Result<()> {
        let path = self.known_path(repo_id)?;
        self.runtime
            .block_on(checkout_branch(&self.runner, &path, branch))?;
        Ok(())
    }

    /// Local branches, freshly read from git for one repository.
    ///
    /// Branch prompts use this to confirm the branch they were opened on is
    /// still there before writing, and to surface git's own error when it is
    /// not.
    pub fn local_branches(&self, repo_id: &str) -> Result<Vec<vantage_core::state::BranchInfo>> {
        let path = self.known_path(repo_id)?;
        self.runtime
            .block_on(list_branches(&self.runner, &path))
            .map_err(Into::into)
    }

    /// Create `name` at `start_point` and check it out.
    pub fn create_branch(&self, repo_id: &str, name: &str, start_point: &str) -> Result<()> {
        let repo_path = self.known_path(repo_id)?;
        self.runtime.block_on(create_branch(
            &self.runner,
            &repo_path,
            name,
            Some(start_point),
            true,
        ))?;
        Ok(())
    }

    /// Rename the current branch.
    pub fn rename_branch(&self, repo_id: &str, from: &str, to: &str) -> Result<()> {
        let repo_path = self.known_path(repo_id)?;
        self.runtime
            .block_on(rename_branch(&self.runner, &repo_path, Some(from), to))?;
        Ok(())
    }

    /// Delete a branch. Unmerged work needs `force`, which the UI only asks for
    /// after git refused the ordinary `-d`; the outcome says whether that
    /// refusal happened so the caller can ask for the second confirmation.
    pub fn delete_branch(
        &self,
        repo_id: &str,
        name: &str,
        force: bool,
    ) -> Result<BranchDeleteOutcome> {
        let repo_path = self.known_path(repo_id)?;
        Ok(self
            .runtime
            .block_on(delete_branch(&self.runner, &repo_path, name, force))?)
    }

    /// Create a fresh repository under `parent` and register it.
    pub fn create_repo(&self, parent: &str, name: &str, with_gitignore: bool) -> Result<()> {
        let name = name.trim();
        if !vantage_core::git::clone::is_safe_folder_name(name) {
            return Err(anyhow!("Use a plain folder name, like my-project."));
        }
        let folder = Path::new(parent).join(name);
        if folder.exists() {
            return Err(anyhow!("{} already exists.", folder.display()));
        }
        std::fs::create_dir_all(&folder).with_context(|| format!("create {}", folder.display()))?;

        let branch = self.ui_settings().default_branch.trim().to_string();
        let branch = if branch.is_empty() {
            "main".to_string()
        } else {
            branch
        };
        self.runtime
            .block_on(self.runner.run_write(&folder, &["init", "-b", &branch], 30))
            .and_then(|out| {
                if out.success() {
                    Ok(())
                } else {
                    Err(vantage_core::AppError::other(out.stderr))
                }
            })?;

        if with_gitignore {
            let body = "# macOS\n.DS_Store\n\n# Editor\n.vscode/\n.idea/\n";
            std::fs::write(folder.join(".gitignore"), body).ok();
        }

        self.add_repo(&folder.to_string_lossy())?;
        Ok(())
    }

    /// Drop a repository from the workspace list; the folder stays on disk.
    pub fn forget(&self, repo_id: &str) -> Result<()> {
        let repo_id = repo_id.to_string();
        self.update_now(move |state| {
            repos::remove(state, &repo_id);
            Ok(())
        })
    }

    /// Clone `url` into `parent` (under `name` when given) and register it.
    ///
    /// The engine emits progress through a sink; the dashboard only needs the
    /// finished result, so the sink is a no-op here.
    pub fn clone_repo(&self, url: &str, parent: &str, name: Option<&str>) -> Result<()> {
        let name = name.map(str::to_string);
        let path = self.runtime.block_on(vantage_core::git::clone::clone_repo(
            &self.runner,
            &vantage_core::Sink::default(),
            "vantage-clone",
            url,
            parent,
            name.as_deref(),
            None,
        ))?;
        self.add_repo(&path)?;
        Ok(())
    }

    /// Add one repository-relative path to `.gitignore`.
    ///
    /// Only untracked files take effect; the caller warns for tracked ones.
    pub fn ignore_path(&self, repo_id: &str, path: &str) -> Result<()> {
        let repo_path = self.known_path(repo_id)?;
        let ignore_file = repo_path.join(".gitignore");
        let mut body = std::fs::read_to_string(&ignore_file).unwrap_or_default();
        // Anchor at the repository root so the rule cannot match deeper paths.
        let rule = format!("/{path}");
        if body.lines().any(|line| line.trim() == rule) {
            return Ok(());
        }
        if !body.is_empty() && !body.ends_with('\n') {
            body.push('\n');
        }
        body.push_str(&rule);
        body.push('\n');
        std::fs::write(&ignore_file, body)
            .with_context(|| format!("write {}", ignore_file.display()))?;
        Ok(())
    }

    /// Move the branch tip to `sha`, keeping the working tree untouched.
    pub fn reset_to(&self, repo_id: &str, sha: &str) -> Result<()> {
        let repo_path = self.known_path(repo_id)?;
        self.runtime
            .block_on(ops::reset_to(&self.runner, &repo_path, sha, false))?;
        Ok(())
    }

    /// Every stash the repository keeps, newest first.
    pub fn stashes(&self, repo_id: &str) -> Result<Vec<vantage_core::state::StashEntry>> {
        let repo_path = self.known_path(repo_id)?;
        Ok(details::stash_entries(
            &self.runtime,
            &self.runner,
            &repo_path,
        )?)
    }

    pub fn stash_drop(&self, repo_id: &str, index: u32) -> Result<()> {
        let repo_path = self.known_path(repo_id)?;
        self.runtime
            .block_on(stash_drop(&self.runner, &repo_path, index))?;
        Ok(())
    }

    /// Files changed between two branches, each with its patch lines.
    pub fn compare(
        &self,
        repo_id: &str,
        base: &str,
        head: &str,
    ) -> Result<Vec<crate::model::CommitFile>> {
        let repo_path = self.known_path(repo_id)?;
        Ok(details::compare_branches(
            &self.runtime,
            &self.runner,
            &repo_path,
            base,
            head,
        )?)
    }

    /// Merge `target` into the checked-out branch. Returns whether git stopped
    /// on conflicts, in which case the tree is left for the user to resolve.
    pub fn merge(&self, repo_id: &str, target: &str) -> Result<bool> {
        let repo_path = self.known_path(repo_id)?;
        let outcome =
            self.runtime
                .block_on(merge_into_current(&self.runner, &repo_path, target))?;
        Ok(outcome.conflicted)
    }

    /// Fetch, then merge `origin/<default>` into the current branch.
    pub fn update_from_default(&self, repo_id: &str) -> Result<bool> {
        let repo_path = self.known_path(repo_id)?;
        let default = self
            .runtime
            .block_on(repos::default_branch(&self.runner, &repo_path));
        self.runtime
            .block_on(sync_fetch(&self.runner, &repo_path))?;
        let outcome = self.runtime.block_on(merge_into_current(
            &self.runner,
            &repo_path,
            &format!("origin/{default}"),
        ))?;
        Ok(outcome.conflicted)
    }

    /// The browser page for the repository's `origin`, when it is a hosted one.
    pub fn origin_page(&self, repo_id: &str) -> Result<Option<String>> {
        let repo_path = self.known_path(repo_id)?;
        let url = self
            .runtime
            .block_on(vantage_core::git::refs::get_remote_url(
                &self.runner,
                &repo_path,
            ))?;
        let Some(url) = url else {
            return Ok(None);
        };
        let page = match vantage_core::git::github::parse_remote(&url) {
            Some(target) => format!("https://{}/{}/{}", target.host, target.owner, target.repo),
            // Anything that is not a hosted remote is shown as it is.
            None => url,
        };
        Ok(Some(page))
    }

    /// Create a commit that undoes `sha`, leaving history intact.
    pub fn revert(&self, repo_id: &str, sha: &str) -> Result<String> {
        let repo_path = self.known_path(repo_id)?;
        let reverted = self
            .runtime
            .block_on(ops::revert(&self.runner, &repo_path, sha))?;
        Ok(reverted.short)
    }

    /// Inspector data for one repository: branches, history, stashes, identity
    /// and remote capability. Loaded when a row is selected, never by the
    /// overview scan.
    pub fn details(
        &self,
        repo_id: &str,
        current_branch: &str,
        has_head: bool,
    ) -> Result<details::RepoDetails> {
        let repo_path = self.known_path(repo_id)?;
        details::repo_details(
            &self.runtime,
            &self.runner,
            &repo_path,
            current_branch,
            has_head,
        )
        .map_err(anyhow::Error::msg)
    }

    /// Files and patch lines for one commit.
    pub fn commit_files(&self, repo_id: &str, sha: &str) -> Result<Vec<crate::model::CommitFile>> {
        let repo_path = self.known_path(repo_id)?;
        Ok(details::commit_files(
            &self.runtime,
            &self.runner,
            &repo_path,
            sha,
        )?)
    }

    /// Patch lines for one working-tree file, read only when the UI expands it.
    ///
    /// Scans stop at counts and never read a repository's full diff; this
    /// reads the selected path (and its rename source) under a byte cap.
    pub fn file_preview(
        &self,
        repo_id: &str,
        path: &str,
        staged: bool,
        status: FileChangeStatus,
        orig_path: Option<&str>,
    ) -> Result<(Vec<PreviewLine>, bool)> {
        let repo_path = self.known_path(repo_id)?;
        details::file_preview(
            &self.runtime,
            &self.runner,
            &repo_path,
            path,
            staged,
            status,
            orig_path,
        )
    }

    /// Rewrite the tip commit: its message, plus any paths given (they are
    /// staged first, so the same ticked-file rule as a normal commit applies).
    ///
    /// `body` follows the engine's amend semantics: `None` keeps HEAD's body,
    /// `Some("")` clears it, and `Some(text)` replaces it.
    pub fn amend(
        &self,
        repo_id: &str,
        paths: &[String],
        summary: &str,
        body: Option<&str>,
    ) -> Result<String> {
        let repo_path = self.known_path(repo_id)?;
        let summary = summary.to_string();
        let body = body.map(str::to_string);
        let paths = paths.to_vec();
        let amended = self.runtime.block_on(async {
            ops::stage_paths(&self.runner, &repo_path, &paths).await?;
            ops::amend_commit(&self.runner, &repo_path, &paths, &summary, body.as_deref()).await
        })?;
        Ok(amended.short)
    }

    /// Throw away every changed path at once, with the same backup safety net.
    pub fn discard_paths(&self, repo_id: &str, paths: &[String]) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }
        let repo_path = self.known_path(repo_id)?;
        self.runtime
            .block_on(ops::discard(&self.runner, &repo_path, paths))?;
        Ok(())
    }

    /// Put the working tree aside under a message.
    pub fn stash_all(&self, repo_id: &str, message: Option<&str>) -> Result<()> {
        let repo_path = self.known_path(repo_id)?;
        let message = message.map(str::to_string);
        self.runtime.block_on(stash_push(
            &self.runner,
            &repo_path,
            message.as_deref(),
            true,
        ))?;
        Ok(())
    }

    /// Bring stash `index` back and remove it from the list.
    pub fn stash_pop(&self, repo_id: &str, index: u32) -> Result<()> {
        let repo_path = self.known_path(repo_id)?;
        self.runtime
            .block_on(stash_pop(&self.runner, &repo_path, index))?;
        Ok(())
    }

    /// Throw away the working-tree changes of one path, keeping a backup ref so
    /// a mistaken discard is recoverable from the repository itself.
    pub fn discard(&self, repo_id: &str, path: &str) -> Result<()> {
        let repo_path = self.known_path(repo_id)?;
        let paths = vec![path.to_string()];
        self.runtime
            .block_on(ops::discard(&self.runner, &repo_path, &paths))?;
        Ok(())
    }

    /// Undo the tip commit of a repository, keeping its changes staged.
    ///
    /// Refuses when the commit is already on a remote branch: undoing it there
    /// would rewrite published history. The engine also writes a backup ref
    /// before resetting, so the commit is never silently lost.
    pub fn undo_commit(&self, repo_id: &str) -> Result<()> {
        let repo_path = self.known_path(repo_id)?;
        let pushed =
            self.runtime
                .block_on(is_commit_on_remote(&self.runner, &repo_path, "HEAD"))?;
        if pushed {
            return Err(anyhow!(
                "This commit is already on the remote. Undoing it would rewrite \
                 published history — revert it instead."
            ));
        }
        self.runtime
            .block_on(ops::undo_last_commit(&self.runner, &repo_path))?;
        Ok(())
    }

    /// Stage or unstage every path in one repository, in a single git call.
    ///
    /// "Select all" goes through this instead of looping over
    /// [`Backend::set_staged`], so the whole list costs one process.
    pub fn set_all_staged(&self, repo_id: &str, staged: bool) -> Result<()> {
        let repo_path = self.known_path(repo_id)?;
        let result = self.runtime.block_on(async {
            if staged {
                ops::stage_all(&self.runner, &repo_path).await
            } else {
                ops::unstage_all(&self.runner, &repo_path).await
            }
        });
        result.with_context(|| {
            format!(
                "{} every path in `{}`",
                if staged { "stage" } else { "unstage" },
                repo_path.display()
            )
        })
    }

    /// The full SHA of the repository's checked-out HEAD, or `None` when the
    /// branch is unborn. Used to refuse a queued commit when HEAD moved.
    pub fn head_sha(&self, repo_id: &str) -> Result<Option<String>> {
        let repo_path = self.known_path(repo_id)?;
        self.runtime.block_on(async {
            let output = self
                .runner
                .run_read(&repo_path, &["rev-parse", "--verify", "HEAD"])
                .await?;
            if !output.success() {
                return Ok(None);
            }
            Ok(Some(output.stdout_string().trim().to_string()))
        })
    }

    /// Commit `paths` with `summary` as the message, using the repository's own
    /// git identity.
    ///
    /// Returns the short SHA so the UI can name the commit it just created. The
    /// engine does the staging, identity validation and conflict checks; this
    /// adapter only moves the call off the UI thread.
    pub fn commit(
        &self,
        repo_id: &str,
        paths: &[String],
        summary: &str,
        body: Option<&str>,
    ) -> Result<String> {
        let repo_path = self.known_path(repo_id)?;
        let summary = summary.to_string();
        let body = body.map(str::to_string);
        let paths = paths.to_vec();
        let committed = self.runtime.block_on(async {
            // Assert the index from the caller's list before committing. Each
            // tick stage runs as its own git process, so one can still be in
            // flight when the commit starts; without this the engine would see
            // that path as unselected and silently leave it behind.
            ops::stage_paths(&self.runner, &repo_path, &paths).await?;
            ops::commit(
                &self.runner,
                &repo_path,
                &paths,
                &summary,
                body.as_deref(),
                &[],
            )
            .await
        })?;
        Ok(committed.short)
    }

    /// Stage or unstage one file, addressed by its repository-relative path.
    pub fn set_staged(&self, repo_id: &str, path: &str, staged: bool) -> Result<()> {
        let repo_path = self.known_path(repo_id)?;
        let result = self.runtime.block_on(async {
            if staged {
                ops::stage_path(&self.runner, &repo_path, path).await
            } else {
                ops::unstage_path(&self.runner, &repo_path, path).await
            }
        });
        result.with_context(|| {
            format!(
                "{} `{path}` in `{}`",
                if staged { "stage" } else { "unstage" },
                repo_path.display()
            )
        })
    }

    /// Vantage groups live in `PersistedState.extra` under [`EXTRA_KEY`]:
    /// an ordered group list plus `repo id -> group name` assignments. A state
    /// file saved before the rename reads from [`LEGACY_EXTRA_KEY`] until the
    /// startup migration moves it.
    pub fn groups(&self) -> (Vec<String>, BTreeMap<String, String>) {
        groups_from(&self.state())
    }

    pub fn set_groups(
        &self,
        groups: &[String],
        assignments: &BTreeMap<String, String>,
    ) -> Result<()> {
        let groups = groups.to_vec();
        let assignments = assignments.clone();
        self.update_now(move |state| {
            sets_groups_in(state, &groups, &assignments);
            Ok(())
        })
    }

    /// Absolute path for a repo id.
    pub fn repo_path(&self, repo_id: &str) -> Option<PathBuf> {
        self.state()
            .repos
            .iter()
            .find(|repo| repo.id == repo_id)
            .map(|repo| PathBuf::from(&repo.path))
    }

    /// Ids of every repository currently in the workspace.
    pub fn repo_ids(&self) -> std::collections::HashSet<String> {
        self.state().repos.into_iter().map(|repo| repo.id).collect()
    }

    // ---------------- internals ----------------

    fn state(&self) -> PersistedState {
        let (state, warning) = self.load_state();
        if let Some(warning) = warning {
            if let Ok(mut slot) = self.state_warning.lock() {
                if slot.is_none() {
                    *slot = Some(warning);
                }
            }
        }
        state
    }

    /// Read the live state from the test override or the process-wide handle.
    fn load_state(&self) -> (PersistedState, Option<String>) {
        match &self.persistence {
            Some(persistence) => persistence.load(),
            None => repos::load_state(),
        }
    }

    /// Apply a critical mutation to the latest on-disk state and wait for it,
    /// so it cannot be lost under a queued settings write.
    fn update_now<F>(&self, update: F) -> Result<()>
    where
        F: FnOnce(&mut PersistedState) -> Result<(), vantage_core::AppError> + Send + 'static,
    {
        match &self.persistence {
            Some(persistence) => persistence
                .update_and_wait(update)
                .context("save the workspace state"),
            None => repos::update_state_now(update).context("save the workspace state"),
        }
    }

    /// Move grouping settings written under the pre-rename key to
    /// [`EXTRA_KEY`], once, at startup.
    ///
    /// A failure here must not keep the dashboard from opening, so the write
    /// error is reported and dropped; the in-memory fallback in
    /// [`groups_from`] still shows the legacy groups on this run.
    fn migrate_legacy_groups(&self) {
        let mut state = self.state();
        if !migrate_legacy_groups_in(&mut state) {
            return;
        }
        if let Err(error) = self.update_now(|state| {
            migrate_legacy_groups_in(state);
            Ok(())
        }) {
            eprintln!("failed to migrate legacy group settings: {error:#}");
        }
    }

    /// Path of a repo that must exist, for the commands that take an id but
    /// have nothing to return.
    fn known_path(&self, repo_id: &str) -> Result<PathBuf> {
        self.repo_path(repo_id)
            .ok_or_else(|| anyhow!("no repository with id `{repo_id}` in the workspace"))
    }

    /// Map one persisted entry, re-reading its git state. `Err` carries git's
    /// own message so the UI can name the repository that failed.
    ///
    /// The status is queried for the real checked-out HEAD (`None` for both
    /// target branch and marker): Vantage shows the working tree, not the
    /// tracked branch Vantage's panes may be pinned to.
    fn map_entry(&self, entry: &RepoEntry, group: String) -> Result<Repository, String> {
        let path = PathBuf::from(&entry.path);
        let snapshot = self
            .runtime
            .block_on(query_repo_overview(&self.runner, &entry.id, &path));
        if let Some(error) = snapshot.status.error.as_deref() {
            return Err(format!("{}: {error}", entry.name));
        }
        Ok(repo_map::map_repo(
            &self.runtime,
            &self.runner,
            entry,
            &snapshot.status,
            &snapshot.records,
            snapshot.head_sha.as_deref(),
            group,
        ))
    }
}

/// Map `items` with at most `max_workers` threads, keeping the input order.
///
/// Values and errors are collected separately, so one failed item cannot drop
/// or reorder the successful ones. The pool reads a shared index instead of
/// splitting the input into fixed chunks: a slow item delays only the worker
/// that picked it, not the rest of its chunk.
fn map_ordered_bounded<T, U, E, F>(items: &[T], max_workers: usize, map: F) -> (Vec<U>, Vec<E>)
where
    T: Sync,
    U: Send,
    E: Send,
    F: Fn(&T) -> Result<U, E> + Sync,
{
    if items.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let workers = max_workers.max(1).min(items.len());
    let next = AtomicUsize::new(0);
    let slots: Vec<Mutex<Option<Result<U, E>>>> = items.iter().map(|_| Mutex::new(None)).collect();

    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(item) = items.get(index) else {
                    break;
                };
                let result = map(item);
                *slots[index]
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(result);
            });
        }
    });

    let mut values = Vec::with_capacity(items.len());
    let mut errors = Vec::new();
    for slot in slots {
        match slot
            .into_inner()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
        {
            Some(Ok(value)) => values.push(value),
            Some(Err(error)) => errors.push(error),
            None => unreachable!("every worker index is within the item slice"),
        }
    }
    (values, errors)
}

fn group_of(assignments: &BTreeMap<String, String>, repo_id: &str) -> String {
    assignments
        .get(repo_id)
        .cloned()
        .unwrap_or_else(|| UNGROUPED.to_string())
}

/// The `{ "groups": [...], "assignments": { id: group } }` object, or empty
/// collections when Vantage has never written its settings.
///
/// Settings under [`EXTRA_KEY`] win; a state file from before the rename only
/// has [`LEGACY_EXTRA_KEY`]. Startup migrates that, but reading the fallback
/// keeps the groups intact if the migration's save fails.
fn groups_from(state: &PersistedState) -> (Vec<String>, BTreeMap<String, String>) {
    let mut groups = Vec::new();
    let mut assignments = BTreeMap::new();

    let Some(section) = state
        .extra
        .get(EXTRA_KEY)
        .or_else(|| state.extra.get(LEGACY_EXTRA_KEY))
        .and_then(|v| v.as_object())
    else {
        return (groups, assignments);
    };
    if let Some(list) = section.get("groups").and_then(|v| v.as_array()) {
        groups = list
            .iter()
            .filter_map(|value| value.as_str().map(str::to_string))
            .collect();
    }
    if let Some(map) = section.get("assignments").and_then(|v| v.as_object()) {
        assignments = map
            .iter()
            .filter_map(|(id, value)| value.as_str().map(|group| (id.clone(), group.to_string())))
            .collect();
    }
    (groups, assignments)
}

/// Store the group settings, leaving every other `extra` key untouched.
///
/// The legacy key is dropped so an old build (or the startup migration) cannot
/// resurrect stale groups over what the user just wrote.
fn sets_groups_in(
    state: &mut PersistedState,
    groups: &[String],
    assignments: &BTreeMap<String, String>,
) {
    let mut section = state
        .extra
        .get(EXTRA_KEY)
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    section.insert("groups".to_string(), serde_json::json!(groups));
    section.insert("assignments".to_string(), serde_json::json!(assignments));
    state
        .extra
        .insert(EXTRA_KEY.to_string(), serde_json::Value::Object(section));
    state.extra.remove(LEGACY_EXTRA_KEY);
}

/// Store the v2 preferences inside the shared `vantage` object, preserving
/// groups, assignments and any future keys.
fn sets_ui_settings_in(state: &mut PersistedState, settings: &UiSettings) -> Result<()> {
    let mut section = state
        .extra
        .get(EXTRA_KEY)
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    section.insert(
        "settings".to_string(),
        serde_json::to_value(settings).context("serialize Vantage settings")?,
    );
    state
        .extra
        .insert(EXTRA_KEY.to_string(), serde_json::Value::Object(section));
    Ok(())
}

/// Copy the pre-rename group settings to [`EXTRA_KEY`] unless a non-empty
/// section is already there, then drop the legacy key. Returns whether `state`
/// changed and therefore needs saving.
fn migrate_legacy_groups_in(state: &mut PersistedState) -> bool {
    let Some(legacy) = state.extra.get(LEGACY_EXTRA_KEY).cloned() else {
        return false;
    };

    let mut section = state
        .extra
        .get(EXTRA_KEY)
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    let new_section_value = serde_json::Value::Object(section.clone());
    if groups_section_is_empty(&new_section_value) {
        if let Some(legacy) = legacy.as_object() {
            for key in ["groups", "assignments"] {
                if let Some(value) = legacy.get(key) {
                    section.insert(key.to_string(), value.clone());
                }
            }
        }
    }
    state
        .extra
        .insert(EXTRA_KEY.to_string(), serde_json::Value::Object(section));
    state.extra.remove(LEGACY_EXTRA_KEY);
    true
}

/// True when a group section names no groups and assigns none. A section that
/// is not an object at all also counts as empty, so a malformed placeholder
/// does not block the migration.
fn groups_section_is_empty(section: &serde_json::Value) -> bool {
    let no_groups = section
        .get("groups")
        .and_then(|value| value.as_array())
        .is_none_or(|groups| groups.is_empty());
    let no_assignments = section
        .get("assignments")
        .and_then(|value| value.as_object())
        .is_none_or(|assignments| assignments.is_empty());
    no_groups && no_assignments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_round_trip_through_the_state_extra() {
        let mut state = PersistedState::default();
        let groups = vec!["AI Tools".to_string(), "Web Apps".to_string()];
        let mut assignments = BTreeMap::new();
        assignments.insert("repo-1".to_string(), "Web Apps".to_string());

        sets_groups_in(&mut state, &groups, &assignments);

        assert_eq!(groups_from(&state), (groups.clone(), assignments.clone()));
        assert_eq!(group_of(&assignments, "repo-1"), "Web Apps");
        assert_eq!(group_of(&assignments, "repo-2"), UNGROUPED);
    }

    #[test]
    fn groups_do_not_disturb_other_extra_keys() {
        let mut state = PersistedState::default();
        state
            .extra
            .insert("futureSetting".to_string(), serde_json::json!({ "a": 1 }));
        sets_groups_in(
            &mut state,
            &["Packages & CLI".to_string()],
            &BTreeMap::new(),
        );

        assert!(state.extra.contains_key("futureSetting"));
        assert_eq!(groups_from(&state).0, ["Packages & CLI".to_string()]);
    }

    #[test]
    fn state_without_vantage_settings_has_no_groups() {
        let (groups, assignments) = groups_from(&PersistedState::default());
        assert!(groups.is_empty());
        assert!(assignments.is_empty());
    }

    /// The shape the pre-rename build wrote, including the assignments the
    /// user's real `state.json` has.
    fn legacy_settings(state: &mut PersistedState) {
        state.extra.insert(
            LEGACY_EXTRA_KEY.to_string(),
            serde_json::json!({
                "groups": ["Ungrouped", "SE"],
                "assignments": { "repo-se": "SE" },
            }),
        );
    }

    #[test]
    fn legacy_group_settings_are_read_when_vantage_is_absent() {
        let mut state = PersistedState::default();
        legacy_settings(&mut state);

        let (groups, assignments) = groups_from(&state);
        assert_eq!(groups, ["Ungrouped".to_string(), "SE".to_string()]);
        assert_eq!(assignments.get("repo-se"), Some(&"SE".to_string()));
        assert_eq!(group_of(&assignments, "repo-other"), UNGROUPED);
    }

    #[test]
    fn vantage_group_settings_win_over_the_legacy_key() {
        let mut state = PersistedState::default();
        legacy_settings(&mut state);
        sets_groups_in(
            &mut state,
            &["New".to_string()],
            &BTreeMap::from([("repo-new".to_string(), "New".to_string())]),
        );

        let (groups, assignments) = groups_from(&state);
        assert_eq!(groups, ["New".to_string()]);
        assert_eq!(assignments.len(), 1);
        assert_eq!(assignments.get("repo-new"), Some(&"New".to_string()));
    }

    #[test]
    fn writing_groups_removes_the_legacy_key() {
        let mut state = PersistedState::default();
        legacy_settings(&mut state);
        state
            .extra
            .insert("futureSetting".to_string(), serde_json::json!({ "a": 1 }));

        sets_groups_in(
            &mut state,
            &["Packages & CLI".to_string()],
            &BTreeMap::new(),
        );

        assert!(state.extra.contains_key(EXTRA_KEY));
        assert!(!state.extra.contains_key(LEGACY_EXTRA_KEY));
        assert!(state.extra.contains_key("futureSetting"));
    }

    #[test]
    fn migration_copies_legacy_groups_and_drops_the_key() {
        let mut state = PersistedState::default();
        legacy_settings(&mut state);
        state
            .extra
            .insert("futureSetting".to_string(), serde_json::json!({ "a": 1 }));

        assert!(migrate_legacy_groups_in(&mut state));

        assert!(!state.extra.contains_key(LEGACY_EXTRA_KEY));
        assert!(state.extra.contains_key("futureSetting"));
        let (groups, assignments) = groups_from(&state);
        assert_eq!(groups, ["Ungrouped".to_string(), "SE".to_string()]);
        assert_eq!(assignments.get("repo-se"), Some(&"SE".to_string()));
    }

    #[test]
    fn migration_keeps_non_empty_vantage_settings() {
        let mut state = PersistedState::default();
        legacy_settings(&mut state);
        sets_groups_in(
            &mut state,
            &["Mine".to_string()],
            &BTreeMap::from([("repo-mine".to_string(), "Mine".to_string())]),
        );
        // `sets_groups_in` already drops the legacy key; put a stale copy back
        // to prove the migration does not overwrite live data with it.
        legacy_settings(&mut state);

        assert!(migrate_legacy_groups_in(&mut state));
        assert!(!state.extra.contains_key(LEGACY_EXTRA_KEY));
        let (groups, assignments) = groups_from(&state);
        assert_eq!(groups, ["Mine".to_string()]);
        assert_eq!(assignments.get("repo-mine"), Some(&"Mine".to_string()));
        assert_eq!(assignments.get("repo-se"), None);
    }

    #[test]
    fn migration_is_a_no_op_without_the_legacy_key() {
        let mut state = PersistedState::default();
        assert!(!migrate_legacy_groups_in(&mut state));
        assert!(state.extra.is_empty());
    }

    #[test]
    fn writing_groups_preserves_v2_settings() {
        let mut state = PersistedState::default();
        let settings = UiSettings {
            theme: crate::settings::ThemeChoice::Dark,
            ..UiSettings::default()
        };
        sets_ui_settings_in(&mut state, &settings).unwrap();

        sets_groups_in(
            &mut state,
            &["SE".to_string()],
            &BTreeMap::from([("repo-se".to_string(), "SE".to_string())]),
        );

        assert_eq!(groups_from(&state).0, ["SE".to_string()]);
        let loaded = UiSettings::from_state(&state);
        assert_eq!(loaded.theme, crate::settings::ThemeChoice::Dark);
    }

    #[test]
    fn writing_settings_preserves_groups() {
        let mut state = PersistedState::default();
        sets_groups_in(
            &mut state,
            &["Ungrouped".to_string(), "SE".to_string()],
            &BTreeMap::from([("repo-se".to_string(), "SE".to_string())]),
        );

        sets_ui_settings_in(
            &mut state,
            &UiSettings {
                auto_fetch_minutes: 15,
                ..UiSettings::default()
            },
        )
        .unwrap();

        let (groups, assignments) = groups_from(&state);
        assert_eq!(groups, ["Ungrouped".to_string(), "SE".to_string()]);
        assert_eq!(assignments.get("repo-se"), Some(&"SE".to_string()));
        assert_eq!(UiSettings::from_state(&state).auto_fetch_minutes, 15);
    }

    #[test]
    fn bounded_mapping_keeps_order_and_isolates_errors() {
        let items: Vec<usize> = (0..12).collect();
        let (values, errors) = map_ordered_bounded(&items, 4, |item| {
            // Later items finish first, so a chunked map would misorder them.
            std::thread::sleep(std::time::Duration::from_millis(
                ((6 - item % 7) * 2) as u64,
            ));
            if item % 5 == 0 {
                Err(format!("bad {item}"))
            } else {
                Ok(item * 2)
            }
        });

        assert_eq!(values, vec![2, 4, 6, 8, 12, 14, 16, 18, 22]);
        assert_eq!(errors, vec!["bad 0", "bad 5", "bad 10"]);
    }

    #[test]
    fn bounded_mapping_never_exceeds_its_worker_count() {
        use std::sync::atomic::AtomicUsize;
        use std::sync::atomic::Ordering::SeqCst;

        let items: Vec<usize> = (0..16).collect();
        let active = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let (values, errors): (Vec<usize>, Vec<()>) = map_ordered_bounded(&items, 3, |item| {
            let running = active.fetch_add(1, SeqCst) + 1;
            peak.fetch_max(running, SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(5));
            active.fetch_sub(1, SeqCst);
            Ok(*item)
        });

        assert_eq!(values, items);
        assert!(errors.is_empty());
        assert!(
            peak.load(SeqCst) <= 3,
            "mapping ran {} workers, not the requested 3",
            peak.load(SeqCst)
        );
    }

    #[test]
    fn bounded_mapping_shares_one_git_runtime() {
        let runtime = Runtime::new().unwrap();
        let items: Vec<u64> = (0..8).collect();
        let (values, errors): (Vec<u64>, Vec<String>) = map_ordered_bounded(&items, 4, |item| {
            runtime.block_on(async move {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                Ok(item * 2)
            })
        });

        assert_eq!(values, (0..8).map(|item| item * 2).collect::<Vec<_>>());
        assert!(errors.is_empty());
    }

    #[test]
    fn bounded_mapping_spawns_git_processes_on_the_shared_runtime() {
        let runtime = Runtime::new().unwrap();
        let runner = GitRunner::new();
        let temp = std::env::temp_dir();
        let items: Vec<u64> = (0..8).collect();
        let (values, errors): (Vec<u64>, Vec<String>) = map_ordered_bounded(&items, 4, |item| {
            runtime.block_on(async {
                let out = runner
                    .run_read(&temp, &["--version"])
                    .await
                    .map_err(|error| error.message)?;
                Ok(if out.success() { *item } else { u64::MAX })
            })
        });

        assert_eq!(values, items);
        assert!(errors.is_empty());
    }

    #[test]
    fn bounded_mapping_handles_an_empty_input() {
        let items: Vec<usize> = Vec::new();
        let (values, errors): (Vec<usize>, Vec<String>) =
            map_ordered_bounded(&items, 4, |item| Ok(*item));
        assert!(values.is_empty());
        assert!(errors.is_empty());
    }

    #[test]
    fn migration_adds_legacy_groups_without_dropping_v2_settings() {
        let mut state = PersistedState::default();
        sets_ui_settings_in(
            &mut state,
            &UiSettings {
                theme: crate::settings::ThemeChoice::Light,
                ..UiSettings::default()
            },
        )
        .unwrap();
        legacy_settings(&mut state);

        assert!(migrate_legacy_groups_in(&mut state));
        assert!(!state.extra.contains_key(LEGACY_EXTRA_KEY));
        let (groups, assignments) = groups_from(&state);
        assert_eq!(groups, ["Ungrouped".to_string(), "SE".to_string()]);
        assert_eq!(assignments.get("repo-se"), Some(&"SE".to_string()));
        assert_eq!(
            UiSettings::from_state(&state).theme,
            crate::settings::ThemeChoice::Light
        );
    }

    fn temp_state_dir(name: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "vantage_backend_state_{name}_{}_{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn startup_keeps_the_corrupt_state_warning_for_the_ui() {
        let dir = temp_state_dir("corrupt");
        let path = dir.join("state.json");
        std::fs::write(&path, "{ not json").unwrap();

        let backend = Backend::with_state_path(path).unwrap();
        let warning = backend
            .take_state_warning()
            .expect("backend must preserve the load warning");
        assert!(
            warning.contains("could not read its saved state"),
            "{warning}"
        );
        assert!(
            backend.take_state_warning().is_none(),
            "the startup warning is delivered once"
        );
        assert_eq!(backend.state_backups().len(), 1);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn set_groups_reports_a_failed_state_write() {
        let dir = temp_state_dir("failed_save");
        let blocker = dir.join("not-a-directory");
        std::fs::write(&blocker, "blocked").unwrap();
        let backend = Backend::with_state_path(blocker.join("state.json")).unwrap();

        let error = backend
            .set_groups(&["Work".to_string()], &BTreeMap::new())
            .expect_err("an unwritable state path must fail");
        assert!(
            error.to_string().contains("save the workspace state"),
            "{error:#}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
