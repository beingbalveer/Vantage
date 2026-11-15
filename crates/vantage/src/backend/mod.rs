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

use anyhow::{anyhow, Context, Result};
use tokio::runtime::Runtime;
use vantage_core::git::ops;
use vantage_core::git::refs::{
    checkout_branch, create_branch, delete_branch, has_upstream, is_commit_on_remote,
    rename_branch, sync_fetch, sync_pull, sync_push, PullStrategy,
};
use vantage_core::git::status::query_repo_status;
use vantage_core::persist::PersistedState;
use vantage_core::{repos, GitRunner, RepoEntry};

use crate::model::Repository;
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

pub struct Backend {
    /// Private engine runtime: every `block_on` in this module runs on it.
    runtime: Runtime,
    runner: GitRunner,
}

impl Backend {
    pub fn new() -> Result<Self> {
        let runtime = Runtime::new().context("start the git worker runtime")?;
        let backend = Self {
            runtime,
            runner: GitRunner::new(),
        };
        backend.migrate_legacy_groups();
        Ok(backend)
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
    fn snapshot_matching(&self, include: impl Fn(&Path) -> bool) -> (Vec<Repository>, Vec<String>) {
        let state = self.state();
        let (_, assignments) = groups_from(&state);
        let mut entries: Vec<&RepoEntry> = state
            .repos
            .iter()
            .filter(|entry| include(Path::new(&entry.path)))
            .collect();
        entries.sort_by_key(|repo| (if repo.pinned { 0 } else { 1 }, repo.order));

        let mut repositories = Vec::with_capacity(entries.len());
        let mut errors = Vec::new();
        for entry in entries {
            let group = group_of(&assignments, &entry.id);
            match self.map_entry(entry, group) {
                Ok(repository) => repositories.push(repository),
                Err(error) => errors.push(error),
            }
        }
        (repositories, errors)
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
        self.save(&state)?;
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

    /// Persist v2 preferences without disturbing groups, assignments or any
    /// other key in the shared `vantage` object.
    pub fn set_ui_settings(&self, settings: &UiSettings) -> Result<()> {
        let mut state = self.state();
        sets_ui_settings_in(&mut state, settings)?;
        self.save(&state)
    }

    /// Absolute path of the workspace state file.
    pub fn state_path(&self) -> std::path::PathBuf {
        vantage_core::persist::StatePersistence::new()
            .map(|persistence| persistence.path().to_path_buf())
            .unwrap_or_default()
    }

    /// Check out `branch` in one repo. The caller re-reads the row with
    /// [`Backend::refresh`] once this returns.
    pub fn checkout(&self, repo_id: &str, branch: &str) -> Result<()> {
        let path = self.known_path(repo_id)?;
        self.runtime
            .block_on(checkout_branch(&self.runner, &path, branch))?;
        Ok(())
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
    /// after git refuses.
    pub fn delete_branch(&self, repo_id: &str, name: &str, force: bool) -> Result<()> {
        let repo_path = self.known_path(repo_id)?;
        self.runtime
            .block_on(delete_branch(&self.runner, &repo_path, name, force))?;
        Ok(())
    }

    /// Rewrite the tip commit: its message, plus any paths given (they are
    /// staged first, so the same ticked-file rule as a normal commit applies).
    pub fn amend(&self, repo_id: &str, paths: &[String], summary: &str) -> Result<String> {
        let repo_path = self.known_path(repo_id)?;
        let summary = summary.to_string();
        let paths = paths.to_vec();
        let amended = self.runtime.block_on(async {
            ops::stage_paths(&self.runner, &repo_path, &paths).await?;
            ops::amend_commit(&self.runner, &repo_path, &paths, &summary, None).await
        })?;
        Ok(amended.short)
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

    /// Commit `paths` with `summary` as the message, using the repository's own
    /// git identity.
    ///
    /// Returns the short SHA so the UI can name the commit it just created. The
    /// engine does the staging, identity validation and conflict checks; this
    /// adapter only moves the call off the UI thread.
    pub fn commit(&self, repo_id: &str, paths: &[String], summary: &str) -> Result<String> {
        let repo_path = self.known_path(repo_id)?;
        let summary = summary.to_string();
        let paths = paths.to_vec();
        let committed = self.runtime.block_on(async {
            // Assert the index from the caller's list before committing. Each
            // tick stage runs as its own git process, so one can still be in
            // flight when the commit starts; without this the engine would see
            // that path as unselected and silently leave it behind.
            ops::stage_paths(&self.runner, &repo_path, &paths).await?;
            ops::commit(&self.runner, &repo_path, &paths, &summary, None, &[]).await
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
        let mut state = self.state();
        sets_groups_in(&mut state, groups, assignments);
        self.save(&state)
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
        repos::load_state().0
    }

    fn save(&self, state: &PersistedState) -> Result<()> {
        repos::save_state(state).context("save the workspace state")
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
        if let Err(error) = self.save(&state) {
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
        let status = self.runtime.block_on(query_repo_status(
            &self.runner,
            &entry.id,
            &path,
            None,
            None,
        ));
        if let Some(error) = status.error.as_deref() {
            return Err(format!("{}: {error}", entry.name));
        }
        Ok(repo_map::map_repo(
            &self.runtime,
            &self.runner,
            entry,
            &status,
            group,
        ))
    }
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
}
