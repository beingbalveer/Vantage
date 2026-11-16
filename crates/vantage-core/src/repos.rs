//! Repository inventory management: validating a work tree, building the
//! persisted [`RepoEntry`] for it, and the mutations the workspace performs on
//! the repo list (add / remove / reorder / relocate / markers).
//!
//! The functions here are UI-agnostic: git is reached through a [`GitRunner`]
//! and persistence through [`StatePersistence`]. Disk writes are left to the
//! caller (`save_state`) so a UI can batch a mutation with its own bookkeeping
//! (watchers, terminals) before committing once.

use crate::error::AppError;
use crate::git::runner::GitRunner;
use crate::persist::{PersistedState, StatePersistence};
use crate::state::{Marker, RepoEntry};
use chrono::Utc;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use uuid::Uuid;

/// Fallback branch name when the repository has no `origin/HEAD` and no
/// `init.defaultBranch` setting.
const DEFAULT_BRANCH: &str = "main";

// ---------------- State persistence ----------------

/// One process-wide persistence handle. `StatePersistence` owns the debounced
/// writer thread, so creating one per call would leak a thread per save.
static PERSISTENCE: OnceLock<Result<StatePersistence, AppError>> = OnceLock::new();

fn persistence() -> Result<&'static StatePersistence, AppError> {
    PERSISTENCE
        .get_or_init(StatePersistence::new)
        .as_ref()
        .map_err(Clone::clone)
}

/// Load the persisted workspace state. Unreadable or corrupt state loads as
/// defaults (the corrupt file is backed up by `StatePersistence::load`); the
/// returned warning is only useful to a UI that shows startup notices.
pub fn load_state() -> (PersistedState, Option<String>) {
    match persistence() {
        Ok(persistence) => persistence.load(),
        Err(error) => (
            PersistedState::default(),
            Some(format!("Failed to open the state file: {error}")),
        ),
    }
}

/// Write the workspace state immediately (used after critical mutations).
pub fn save_state(state: &PersistedState) -> Result<(), AppError> {
    persistence()?.save(state)
}

// ---------------- Path validation / entry construction ----------------

/// Resolve `path` to the root of the git work tree that contains it.
///
/// The path must exist, be a directory, and sit inside a work tree: a bare
/// repository or a stray directory is rejected with [`AppError::not_a_repo`],
/// matching `git rev-parse --show-toplevel`.
pub async fn validate_repo_path(runner: &GitRunner, path: &Path) -> Result<PathBuf, AppError> {
    let display = path.to_string_lossy();
    if !path.exists() {
        return Err(AppError::path_missing(&display));
    }

    let canon = dunce::canonicalize(path).map_err(AppError::io)?;
    if !canon.is_dir() {
        return Err(AppError::not_a_repo(&display));
    }

    let out = runner
        .run_read(&canon, &["rev-parse", "--show-toplevel"])
        .await?;
    if !out.success() {
        return Err(AppError::not_a_repo(&display).with_stderr(out.stderr));
    }

    let toplevel = out.stdout_string().trim().to_string();
    // `--show-toplevel` reports the real path, but resolve symlinks anyway so
    // entries compare equal to paths picked through a symlinked folder.
    Ok(dunce::canonicalize(Path::new(&toplevel)).unwrap_or_else(|_| PathBuf::from(toplevel)))
}

/// Build a pinned-false, unmarked entry for the work tree containing `path`.
///
/// `branch` is the checked-out branch; it is `None` when HEAD is detached
/// (`rev-parse --abbrev-ref HEAD` prints `HEAD`) or unborn (a fresh `git init`
/// has no HEAD yet), leaving branch resolution to the caller's UI.
pub async fn new_repo_entry(
    runner: &GitRunner,
    path: &Path,
    order: u32,
) -> Result<RepoEntry, AppError> {
    let canon = validate_repo_path(runner, path).await?;
    let branch = head_branch(runner, &canon).await;
    let name = folder_name(&canon);
    Ok(build_entry(canon, name, branch, order))
}

async fn head_branch(runner: &GitRunner, path: &Path) -> Option<String> {
    let out = runner
        .run_read(path, &["rev-parse", "--abbrev-ref", "HEAD"])
        .await
        .ok()?;
    if !out.success() {
        return None;
    }
    let name = out.stdout_string().trim().to_string();
    if name.is_empty() || name == "HEAD" {
        None
    } else {
        Some(name)
    }
}

fn folder_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "repo".to_string())
}

fn build_entry(path: PathBuf, name: String, branch: Option<String>, order: u32) -> RepoEntry {
    RepoEntry {
        id: Uuid::new_v4().to_string(),
        path: path.to_string_lossy().to_string(),
        name,
        branch,
        pinned: false,
        order,
        marker: None,
        notes: Vec::new(),
    }
}

// ---------------- Repo list mutations ----------------
//
// These operate on an in-memory `PersistedState` and never touch disk: the UI
// shell decides when to `save_state`, so it can keep watchers/terminals in sync
// with the same change.

/// Append the work tree containing `path` to the workspace.
///
/// A repository is identified by its canonical path, so adding the same tree
/// twice (or once through a subdirectory or symlink) is a `DuplicateRepo`
/// error rather than a second workspace entry.
pub async fn add(
    runner: &GitRunner,
    existing: &mut PersistedState,
    path: &Path,
) -> Result<RepoEntry, AppError> {
    let canon = validate_repo_path(runner, path).await?;
    let canon_string = canon.to_string_lossy().to_string();
    if existing.repos.iter().any(|repo| repo.path == canon_string) {
        return Err(AppError::new(
            "DuplicateRepo",
            format!("'{}' is already in the workspace.", canon.display()),
        ));
    }

    let branch = head_branch(runner, &canon).await;
    let order = existing
        .repos
        .iter()
        .map(|repo| repo.order)
        .max()
        .unwrap_or(0)
        + 1;
    let name = folder_name(&canon);
    let entry = build_entry(canon, name, branch, order);
    existing.repos.push(entry.clone());
    Ok(entry)
}

/// Add every path that still resolves to a work tree.
///
/// One bad path does not sink the batch: each path is added independently and
/// failures come back with the path that caused them, so a UI can add the rest
/// and say which ones it skipped. Duplicate detection is the same as [`add`],
/// so a path already in `existing` is a failure rather than a second entry.
pub async fn add_all(
    runner: &GitRunner,
    existing: &mut PersistedState,
    paths: &[PathBuf],
) -> (Vec<RepoEntry>, Vec<(PathBuf, AppError)>) {
    let mut added = Vec::with_capacity(paths.len());
    let mut failures = Vec::new();
    for path in paths {
        match add(runner, existing, path).await {
            Ok(entry) => added.push(entry),
            Err(error) => failures.push((path.clone(), error)),
        }
    }
    (added, failures)
}

/// Drop the entry with `id`. Returns whether an entry was removed.
pub fn remove(existing: &mut PersistedState, id: &str) -> bool {
    let before = existing.repos.len();
    existing.repos.retain(|repo| repo.id != id);
    existing.repos.len() != before
}

/// Apply a new display order. `ordered_ids` is the full desired sequence; ids
/// that are not in the workspace are ignored. Pinned repositories keep sorting
/// ahead of unpinned ones.
pub fn reorder(existing: &mut PersistedState, ordered_ids: &[String]) {
    for (position, id) in ordered_ids.iter().enumerate() {
        if let Some(entry) = existing.repos.iter_mut().find(|repo| &repo.id == id) {
            entry.order = position as u32;
        }
    }
    sort_repos(&mut existing.repos);
}

/// Point the entry with `id` at a moved work tree. The path is canonicalized so
/// it still compares equal to the other entries' paths.
pub fn relocate(
    existing: &mut PersistedState,
    id: &str,
    path: &Path,
) -> Result<RepoEntry, AppError> {
    if !path.exists() {
        return Err(AppError::path_missing(&path.to_string_lossy()));
    }
    let canon = dunce::canonicalize(path).map_err(AppError::io)?;
    let entry = existing
        .repos
        .iter_mut()
        .find(|repo| repo.id == id)
        .ok_or_else(|| AppError::not_found("Repository not found"))?;
    entry.path = canon.to_string_lossy().to_string();
    Ok(entry.clone())
}

/// Set the reviewed marker for `id` to `sha`.
pub fn set_marker(existing: &mut PersistedState, id: &str, sha: &str) -> Result<(), AppError> {
    let entry = existing
        .repos
        .iter_mut()
        .find(|repo| repo.id == id)
        .ok_or_else(|| AppError::not_found("Repository not found"))?;
    entry.marker = Some(Marker {
        sha: sha.to_string(),
        set_at: Utc::now().to_rfc3339(),
        branch: None,
    });
    Ok(())
}

/// Sort workspace entries the way the sidebar renders them: pinned first, then
/// ascending `order`. `reorder` calls this after applying a drag sequence.
fn sort_repos(repos: &mut [RepoEntry]) {
    repos.sort_by_key(|repo| (if repo.pinned { 0 } else { 1 }, repo.order));
}

// ---------------- Scan ----------------

/// Find git work trees directly under `folder` (the folder itself included, so
/// picking a repository returns it). Only one level is read; nested work trees
/// are found by scanning their parent. Hidden directories, `node_modules` and
/// `target` are skipped because they never hold a repository a user wants to
/// open and scanning them is slow.
pub fn scan_folder(folder: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    if folder.join(".git").exists() {
        found.push(folder.to_path_buf());
    }

    let Ok(entries) = fs::read_dir(folder) else {
        return found;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let path = entry.path();
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        if name.starts_with('.') || name == "node_modules" || name == "target" {
            continue;
        }
        if path.join(".git").exists() {
            found.push(path);
        }
    }

    found.sort();
    found
}

/// Repositories directly under `folder` that are new to the workspace.
///
/// The paths are canonical, sorted, and free of duplicates, so two scan hits
/// that resolve to one work tree (or a hit that is already in `existing`)
/// appear once at most. The raw scan count is kept separately: a UI can then
/// tell "this folder has no repositories" from "every repository in it is
/// already added".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FolderScan {
    /// How many repositories [`scan_folder`] found, before filtering.
    pub scanned: usize,
    /// Canonical paths of repositories not yet in the workspace.
    pub repositories: Vec<PathBuf>,
}

/// Scan `folder` for repositories and filter out what the workspace knows.
///
/// `existing` can hold paths that were written before canonicalization was
/// enforced; they are canonicalized here too, so a repository is matched by
/// its real location rather than its spelling.
pub fn scan_new_repositories(folder: &Path, existing: &PersistedState) -> FolderScan {
    let found = scan_folder(folder);
    let scanned = found.len();
    let known: std::collections::HashSet<PathBuf> = existing
        .repos
        .iter()
        .map(|repo| {
            let path = Path::new(&repo.path);
            dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
        })
        .collect();

    let mut seen = std::collections::HashSet::new();
    let mut repositories = Vec::new();
    for path in found {
        let canon = dunce::canonicalize(&path).unwrap_or(path);
        if known.contains(&canon) || !seen.insert(canon.clone()) {
            continue;
        }
        repositories.push(canon);
    }

    FolderScan {
        scanned,
        repositories,
    }
}

// ---------------- Stack detection ----------------

/// Extension groups used for the primary-language count, in tie-break order:
/// application languages come before documentation formats so a repo full of
/// READMEs still reports the language it is written in.
const LANGUAGES: [(&str, &[&str]); 8] = [
    ("TypeScript", &["ts", "tsx"]),
    ("JavaScript", &["js", "jsx"]),
    ("Python", &["py"]),
    ("Rust", &["rs"]),
    ("Go", &["go"]),
    ("Swift", &["swift"]),
    ("Markdown", &["md"]),
    ("JSON", &["json"]),
];

const SKIP_DIRS: [&str; 9] = [
    "node_modules",
    ".git",
    "dist",
    "build",
    "target",
    "out",
    ".next",
    "vendor",
    "coverage",
];

/// The stack label shown for a repository: the JS/TS framework when
/// `package.json` declares one, otherwise the most common source extension,
/// otherwise `"Other"`.
pub fn stack_of(path: &Path) -> String {
    if let Some(framework) = js_framework(path) {
        return framework.to_string();
    }

    let mut counts = [0usize; LANGUAGES.len()];
    collect_extensions(path, 0, &mut counts);

    let mut best: Option<&'static str> = None;
    let mut best_count = 0;
    for (slot, (label, _)) in LANGUAGES.iter().enumerate() {
        if counts[slot] > best_count {
            best = Some(label);
            best_count = counts[slot];
        }
    }
    best.unwrap_or("Other").to_string()
}

/// `Next.js` / `React` when declared in any dependency section of
/// `package.json`. Parsed as JSON rather than substring-matched so a "next" in
/// a script name is not mistaken for the framework.
fn js_framework(root: &Path) -> Option<&'static str> {
    let bytes = fs::read(root.join("package.json")).ok()?;
    let manifest: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let declares = |name: &str| {
        ["dependencies", "devDependencies", "peerDependencies"]
            .iter()
            .any(|section| {
                manifest
                    .get(*section)
                    .and_then(|deps| deps.get(name))
                    .is_some()
            })
    };
    if declares("next") {
        Some("Next.js")
    } else if declares("react") {
        Some("React")
    } else {
        None
    }
}

/// Count source extensions up to two directory levels below `root`.
fn collect_extensions(dir: &Path, depth: usize, counts: &mut [usize; LANGUAGES.len()]) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if path.is_dir() {
            if depth < 2 && !name.starts_with('.') && !SKIP_DIRS.contains(&name.as_str()) {
                collect_extensions(&path, depth + 1, counts);
            }
            continue;
        }
        let Some(extension) = path
            .extension()
            .map(|extension| extension.to_string_lossy().to_lowercase())
        else {
            continue;
        };
        for (slot, (_, extensions)) in LANGUAGES.iter().enumerate() {
            if extensions.contains(&extension.as_str()) {
                counts[slot] += 1;
                break;
            }
        }
    }
}

// ---------------- Default branch ----------------

/// The repository's default branch: `origin/HEAD` once a clone has fetched,
/// else the configured `init.defaultBranch`, else `main`. Never fails — the
/// fallback exists so a UI can label an unborn repository before its first
/// commit.
pub async fn default_branch(runner: &GitRunner, path: &Path) -> String {
    if let Ok(out) = runner
        .run_read(path, &["symbolic-ref", "refs/remotes/origin/HEAD"])
        .await
    {
        if out.success() {
            let target = out.stdout_string().trim().to_string();
            if let Some(name) = target.strip_prefix("refs/remotes/origin/") {
                if !name.is_empty() && name != "HEAD" {
                    return name.to_string();
                }
            }
        }
    }

    if let Ok(out) = runner
        .run_read(path, &["config", "--get", "init.defaultBranch"])
        .await
    {
        if out.success() {
            let name = out.stdout_string().trim().to_string();
            if !name.is_empty() {
                return name;
            }
        }
    }

    DEFAULT_BRANCH.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("vantage_repos_{}_{}", name, Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn git_available() -> bool {
        Command::new("git")
            .arg("--version")
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false)
    }

    fn git_init(dir: &Path) {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["init", "-q"])
            .output()
            .expect("run git init");
        assert!(
            out.status.success(),
            "git init failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn entry(id: &str, order: u32, pinned: bool) -> RepoEntry {
        RepoEntry {
            id: id.to_string(),
            path: format!("/tmp/{id}"),
            name: id.to_string(),
            branch: Some("main".to_string()),
            pinned,
            order,
            marker: None,
            notes: Vec::new(),
        }
    }

    fn state_with(entries: Vec<RepoEntry>) -> PersistedState {
        PersistedState {
            repos: entries,
            ..PersistedState::default()
        }
    }

    #[tokio::test]
    async fn validate_repo_path_resolves_work_tree_root_and_rejects_non_repos() {
        if !git_available() {
            eprintln!("skipping: git is not installed");
            return;
        }
        let root = temp_dir("validate");
        let repo = root.join("repo");
        fs::create_dir_all(&repo).unwrap();
        git_init(&repo);
        let expected = dunce::canonicalize(&repo).unwrap();

        let runner = GitRunner::new();
        assert_eq!(validate_repo_path(&runner, &repo).await.unwrap(), expected);

        // A subdirectory of the work tree resolves to the root, which is what
        // makes duplicate detection path-independent.
        let nested = repo.join("src");
        fs::create_dir_all(&nested).unwrap();
        assert_eq!(
            validate_repo_path(&runner, &nested).await.unwrap(),
            expected
        );

        let plain = root.join("plain");
        fs::create_dir_all(&plain).unwrap();
        let error = validate_repo_path(&runner, &plain).await.unwrap_err();
        assert_eq!(error.code, "NotARepo");

        let error = validate_repo_path(&runner, &root.join("missing"))
            .await
            .unwrap_err();
        assert_eq!(error.code, "PathMissing");

        fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn add_rejects_duplicate_canonical_path() {
        if !git_available() {
            eprintln!("skipping: git is not installed");
            return;
        }
        let root = temp_dir("add");
        let repo = root.join("repo");
        fs::create_dir_all(&repo).unwrap();
        git_init(&repo);

        let runner = GitRunner::new();
        let mut state = PersistedState::default();

        let added = add(&runner, &mut state, &repo).await.unwrap();
        assert_eq!(added.name, "repo");
        assert_eq!(added.order, 1);
        assert!(!added.pinned);
        assert!(added.marker.is_none());
        // Freshly initialized: HEAD is unborn, so there is no branch yet.
        assert_eq!(added.branch, None);
        assert_eq!(state.repos.len(), 1);

        let duplicate = add(&runner, &mut state, &repo).await.unwrap_err();
        assert_eq!(duplicate.code, "DuplicateRepo");
        assert_eq!(state.repos.len(), 1);

        // Adding through a subdirectory resolves to the same work tree.
        let nested = repo.join("src");
        fs::create_dir_all(&nested).unwrap();
        let duplicate = add(&runner, &mut state, &nested).await.unwrap_err();
        assert_eq!(duplicate.code, "DuplicateRepo");

        // A second repository gets the next order value.
        let other = root.join("other");
        fs::create_dir_all(&other).unwrap();
        git_init(&other);
        let second = add(&runner, &mut state, &other).await.unwrap();
        assert_eq!(second.order, 2);

        fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn default_branch_prefers_origin_head_and_never_returns_empty() {
        if !git_available() {
            eprintln!("skipping: git is not installed");
            return;
        }
        let root = temp_dir("default_branch");
        let runner = GitRunner::new();

        let fetched = root.join("fetched");
        fs::create_dir_all(&fetched).unwrap();
        git_init(&fetched);
        // A fetched clone points refs/remotes/origin/HEAD at the remote default.
        let out = Command::new("git")
            .arg("-C")
            .arg(&fetched)
            .args([
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/trunk",
            ])
            .output()
            .expect("run git symbolic-ref");
        assert!(
            out.status.success(),
            "git symbolic-ref failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(default_branch(&runner, &fetched).await, "trunk");

        let plain = root.join("plain");
        fs::create_dir_all(&plain).unwrap();
        git_init(&plain);
        let fallback = default_branch(&runner, &plain).await;
        assert!(!fallback.is_empty());
        assert_ne!(fallback, "HEAD");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn reorder_numbers_entries_and_keeps_pinned_first() {
        let mut state = state_with(vec![
            entry("a", 1, false),
            entry("b", 2, false),
            entry("c", 3, false),
        ]);

        reorder(
            &mut state,
            &["c".to_string(), "a".to_string(), "b".to_string()],
        );
        let ids: Vec<&str> = state.repos.iter().map(|repo| repo.id.as_str()).collect();
        assert_eq!(ids, ["c", "a", "b"]);
        assert_eq!(state.repos[0].order, 0);
        assert_eq!(state.repos[2].order, 2);

        // Pinning is independent of the drag order and sorts ahead.
        state.repos[0].pinned = true; // "c"
        state.repos[2].pinned = true; // "b"
        sort_repos(&mut state.repos);
        let ids: Vec<&str> = state.repos.iter().map(|repo| repo.id.as_str()).collect();
        assert_eq!(ids, ["c", "b", "a"]);

        // Unknown ids are ignored instead of adding phantom entries.
        reorder(&mut state, &["missing".to_string()]);
        assert_eq!(state.repos.len(), 3);
    }

    #[test]
    fn remove_reports_whether_the_entry_existed() {
        let mut state = state_with(vec![entry("a", 1, false), entry("b", 2, false)]);
        assert!(remove(&mut state, "a"));
        assert!(!remove(&mut state, "a"));
        assert_eq!(state.repos.len(), 1);
        assert_eq!(state.repos[0].id, "b");
    }

    #[test]
    fn relocate_requires_existing_path_and_unknown_id_fails() {
        let root = temp_dir("relocate");
        let mut state = state_with(vec![entry("a", 1, false)]);

        let error = relocate(&mut state, "a", &root.join("missing")).unwrap_err();
        assert_eq!(error.code, "PathMissing");

        let moved = root.join("moved");
        fs::create_dir_all(&moved).unwrap();
        let relocated = relocate(&mut state, "a", &moved).unwrap();
        assert_eq!(
            relocated.path,
            dunce::canonicalize(&moved).unwrap().to_string_lossy()
        );

        let error = relocate(&mut state, "nope", &moved).unwrap_err();
        assert_eq!(error.code, "NotFound");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn set_marker_stamps_the_reviewed_sha() {
        let mut state = state_with(vec![entry("a", 1, false)]);
        set_marker(&mut state, "a", "abc123").unwrap();
        let marker = state.repos[0].marker.clone().unwrap();
        assert_eq!(marker.sha, "abc123");
        assert!(marker.branch.is_none());
        assert!(!marker.set_at.is_empty());

        let error = set_marker(&mut state, "nope", "abc123").unwrap_err();
        assert_eq!(error.code, "NotFound");
    }

    #[test]
    fn stack_of_detects_frameworks_languages_and_falls_back_to_other() {
        let root = temp_dir("stack");

        let empty = root.join("empty");
        fs::create_dir_all(&empty).unwrap();
        assert_eq!(stack_of(&empty), "Other");

        let docs = root.join("docs");
        fs::create_dir_all(&docs).unwrap();
        fs::write(docs.join("README.md"), "# title").unwrap();
        fs::write(docs.join("CONTRIBUTING.md"), "").unwrap();
        assert_eq!(stack_of(&docs), "Markdown");

        let next_app = root.join("next_app");
        fs::create_dir_all(&next_app).unwrap();
        fs::write(
            next_app.join("package.json"),
            r#"{"dependencies":{"next":"14.0.0"}}"#,
        )
        .unwrap();
        fs::write(next_app.join("index.ts"), "").unwrap();
        assert_eq!(stack_of(&next_app), "Next.js");

        let react_app = root.join("react_app");
        fs::create_dir_all(&react_app).unwrap();
        fs::write(
            react_app.join("package.json"),
            r#"{"devDependencies":{"react":"18.0.0"}}"#,
        )
        .unwrap();
        assert_eq!(stack_of(&react_app), "React");

        let ts_app = root.join("ts_app");
        fs::create_dir_all(&ts_app).unwrap();
        fs::write(ts_app.join("package.json"), "{}").unwrap();
        fs::write(ts_app.join("main.ts"), "").unwrap();
        assert_eq!(stack_of(&ts_app), "TypeScript");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn scan_folder_reports_direct_children_with_a_git_entry() {
        let root = temp_dir("scan");
        let workspace = root.join("workspace");
        fs::create_dir_all(workspace.join("alpha").join(".git")).unwrap();
        fs::create_dir_all(workspace.join("beta")).unwrap();
        fs::create_dir_all(workspace.join("node_modules").join(".git")).unwrap();

        let found = scan_folder(&workspace);
        assert_eq!(found, vec![workspace.join("alpha")]);

        // A folder that is itself a work tree is included in its own scan.
        assert_eq!(
            scan_folder(&workspace.join("alpha")),
            vec![workspace.join("alpha")]
        );

        // A missing folder scans to nothing instead of panicking.
        assert!(scan_folder(&root.join("missing")).is_empty());

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn scan_new_repositories_filters_known_work_trees_and_keeps_the_raw_count() {
        let root = temp_dir("scan_filter");
        let workspace = root.join("workspace");
        fs::create_dir_all(workspace.join("alpha").join(".git")).unwrap();
        fs::create_dir_all(workspace.join("beta").join(".git")).unwrap();
        fs::create_dir_all(workspace.join("plain")).unwrap();
        fs::create_dir_all(workspace.join(".hidden").join(".git")).unwrap();
        fs::create_dir_all(workspace.join("node_modules").join(".git")).unwrap();

        let beta = dunce::canonicalize(workspace.join("beta")).unwrap();
        let alpha = dunce::canonicalize(workspace.join("alpha")).unwrap();
        let mut state = state_with(vec![entry("known", 1, false)]);
        state.repos[0].path = alpha.to_string_lossy().to_string();

        let scan = scan_new_repositories(&workspace, &state);
        assert_eq!(scan.scanned, 2, "hidden and node_modules repos are skipped");
        assert_eq!(scan.repositories, vec![beta]);

        // The folder itself is a candidate when it is a work tree.
        let self_scan = scan_new_repositories(&alpha, &state);
        assert_eq!(self_scan.scanned, 1);
        assert!(self_scan.repositories.is_empty());

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn scan_new_repositories_reports_all_known_but_no_matches_separately() {
        let root = temp_dir("scan_known");
        let workspace = root.join("workspace");
        fs::create_dir_all(workspace.join("alpha").join(".git")).unwrap();

        let alpha = dunce::canonicalize(workspace.join("alpha")).unwrap();
        let mut state = state_with(vec![entry("known", 1, false)]);
        state.repos[0].path = alpha.to_string_lossy().to_string();

        let all_known = scan_new_repositories(&workspace, &state);
        assert_eq!(all_known.scanned, 1);
        assert!(all_known.repositories.is_empty());

        fs::create_dir_all(root.join("empty")).unwrap();
        let empty = scan_new_repositories(&root.join("empty"), &state);
        assert_eq!(empty.scanned, 0);
        assert!(empty.repositories.is_empty());

        fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn add_all_adds_valid_paths_and_reports_the_rest() {
        if !git_available() {
            eprintln!("skipping: git is not installed");
            return;
        }
        let root = temp_dir("add_all");
        let one = root.join("one");
        let two = root.join("two");
        let plain = root.join("plain");
        for repo in [&one, &two] {
            fs::create_dir_all(repo).unwrap();
            git_init(repo);
        }
        fs::create_dir_all(&plain).unwrap();

        let runner = GitRunner::new();
        let mut state = PersistedState::default();
        add(&runner, &mut state, &one).await.unwrap();

        let paths = vec![
            one.clone(),
            two.clone(),
            plain.clone(),
            root.join("missing"),
        ];
        let (added, failures) = add_all(&runner, &mut state, &paths).await;

        assert_eq!(added.len(), 1, "only the new work tree is added");
        assert_eq!(added[0].name, "two");
        assert_eq!(added[0].order, 2);
        assert_eq!(failures.len(), 3, "duplicate, non-repo and missing path");
        assert_eq!(failures[0].1.code, "DuplicateRepo");
        assert_eq!(failures[1].1.code, "NotARepo");
        assert_eq!(failures[2].1.code, "PathMissing");

        let paths: Vec<&str> = state.repos.iter().map(|repo| repo.name.as_str()).collect();
        assert_eq!(paths, ["one", "two"], "no duplicate entry was persisted");

        fs::remove_dir_all(&root).ok();
    }
}
