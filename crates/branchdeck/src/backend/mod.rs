//! Adapter from Vantage's async core engine to BranchDeck's view model.
//!
//! `vantage-core` drives git through `tokio::process`, while BranchDeck renders
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

use anyhow::{anyhow, bail, Context, Result};
use tokio::runtime::Runtime;
use vantage_core::git::refs::sync_fetch;
use vantage_core::git::status::query_repo_status;
use vantage_core::persist::PersistedState;
use vantage_core::{repos, GitRunner, RepoEntry};

use crate::model::Repository;

/// Key in `PersistedState.extra` holding BranchDeck's own grouping settings.
const EXTRA_KEY: &str = "branchdeck";
/// Workspace of a repository the user has not assigned to a group.
const UNGROUPED: &str = "Ungrouped";
const GITHUB_DESKTOP: &str = "GitHub Desktop";

pub struct Backend {
    /// Private engine runtime: every `block_on` in this module runs on it.
    runtime: Runtime,
    runner: GitRunner,
}

impl Backend {
    pub fn new() -> Result<Self> {
        let runtime = Runtime::new().context("start the git worker runtime")?;
        Ok(Self {
            runtime,
            runner: GitRunner::new(),
        })
    }

    /// Full snapshot: every persisted repo mapped to the UI model. Repositories
    /// whose git state cannot be read are returned in `errors` instead (with
    /// git's own message), so one broken work tree cannot blank the dashboard.
    pub fn snapshot(&self) -> (Vec<Repository>, Vec<String>) {
        let state = self.state();
        let (_, assignments) = groups_from(&state);
        let mut entries = state.repos.clone();
        // Sidebar order: pinned repositories first, then the persisted `order`.
        entries.sort_by_key(|repo| (if repo.pinned { 0 } else { 1 }, repo.order));

        let mut repositories = Vec::with_capacity(entries.len());
        let mut errors = Vec::new();
        for entry in &entries {
            let group = group_of(&assignments, &entry.id);
            match self.map_entry(entry, group) {
                Ok(repository) => repositories.push(repository),
                Err(error) => errors.push(error),
            }
        }
        (repositories, errors)
    }

    /// Re-read one repo (after a watcher hit or an action).
    #[allow(dead_code)]
    pub fn refresh(&self, repo_id: &str) -> Option<Repository> {
        let state = self.state();
        let (_, assignments) = groups_from(&state);
        let entry = state.repos.iter().find(|repo| repo.id == repo_id)?;
        self.map_entry(entry, group_of(&assignments, repo_id)).ok()
    }

    /// Add a folder as a repo; returns the mapped row.
    pub fn add_repo(&self, path: &str) -> Result<Repository> {
        let mut state = self.state();
        let entry = self
            .runtime
            .block_on(repos::add(&self.runner, &mut state, Path::new(path)))?;
        self.save(&state)?;

        let (_, assignments) = groups_from(&state);
        self.map_entry(&entry, group_of(&assignments, &entry.id))
            .map_err(anyhow::Error::msg)
    }

    #[allow(dead_code)]
    pub fn remove_repo(&self, repo_id: &str) -> Result<()> {
        let mut state = self.state();
        if !repos::remove(&mut state, repo_id) {
            bail!("no repository with id `{repo_id}` in the workspace");
        }
        self.save(&state)
    }

    /// `git fetch --prune` for one repo. The caller re-reads the row with
    /// [`Backend::refresh`] once this returns.
    pub fn fetch(&self, repo_id: &str) -> Result<()> {
        let path = self.known_path(repo_id)?;
        self.runtime.block_on(sync_fetch(&self.runner, &path))?;
        Ok(())
    }

    /// `open -a "GitHub Desktop" <path>`.
    pub fn open_in_github_desktop(&self, repo_id: &str) -> Result<()> {
        let path = self.known_path(repo_id)?;
        launch_github_desktop(&path)
    }

    /// BranchDeck groups live in `PersistedState.extra` under "branchdeck":
    /// an ordered group list plus `repo id -> group name` assignments.
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

    // ---------------- internals ----------------

    fn state(&self) -> PersistedState {
        repos::load_state().0
    }

    fn save(&self, state: &PersistedState) -> Result<()> {
        repos::save_state(state).context("save the workspace state")
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
    /// target branch and marker): BranchDeck shows the working tree, not the
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

#[cfg(target_os = "macos")]
fn launch_github_desktop(path: &Path) -> Result<()> {
    let status = std::process::Command::new("open")
        .arg("-a")
        .arg(GITHUB_DESKTOP)
        .arg(path)
        .status()
        .context("launch GitHub Desktop")?;
    if !status.success() {
        bail!(
            "`open -a \"{GITHUB_DESKTOP}\" {}` failed: {status}",
            path.display()
        );
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn launch_github_desktop(path: &Path) -> Result<()> {
    let _ = path;
    bail!("GitHub Desktop can only be opened on macOS")
}

fn group_of(assignments: &BTreeMap<String, String>, repo_id: &str) -> String {
    assignments
        .get(repo_id)
        .cloned()
        .unwrap_or_else(|| UNGROUPED.to_string())
}

/// The `{ "groups": [...], "assignments": { id: group } }` object, or empty
/// collections when BranchDeck has never written its settings.
fn groups_from(state: &PersistedState) -> (Vec<String>, BTreeMap<String, String>) {
    let mut groups = Vec::new();
    let mut assignments = BTreeMap::new();

    let Some(section) = state.extra.get(EXTRA_KEY).and_then(|v| v.as_object()) else {
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
fn sets_groups_in(
    state: &mut PersistedState,
    groups: &[String],
    assignments: &BTreeMap<String, String>,
) {
    let section = serde_json::json!({ "groups": groups, "assignments": assignments });
    state.extra.insert(EXTRA_KEY.to_string(), section);
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
    fn state_without_branchdeck_settings_has_no_groups() {
        let (groups, assignments) = groups_from(&PersistedState::default());
        assert!(groups.is_empty());
        assert!(assignments.is_empty());
    }
}
