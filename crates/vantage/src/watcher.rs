//! Filesystem watcher: tracks every scanned working tree and reports which
//! repositories changed, so the UI can rescan only those.
//!
//! Both the working tree and the repository's Git metadata are watched. A
//! commit, fetch, push or checkout performed from a terminal changes HEAD or
//! `refs/` without touching a working-tree file, so watching only the checkout
//! would leave the dashboard stale while Vantage is focused.
//!
//! Events are collected on notify's own thread into a shared dirty set; the UI
//! drains that set on a timer, which keeps the render thread non-blocking.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use notify::event::{MetadataKind, ModifyKind};
use notify::{Event, EventKind, RecursiveMode, Watcher};

/// Directories that generate churn without changing what Git reports. Their
/// events are ignored, so a build cannot cause a rescan loop.
const IGNORED_DIRS: &[&str] = &[
    "target",
    "node_modules",
    "dist",
    "build",
    ".next",
    "coverage",
    ".venv",
    "vendor",
];

/// Version-control internals. Git metadata is matched separately before this
/// filter runs; everything else under a VCS directory is ignored.
const VCS_DIRS: &[&str] = &[".git", ".hg", ".svn", ".jj", ".bzr"];

/// Files in a Git directory whose changes can alter the dashboard: branch and
/// ref moves, index rewrites, packed refs and fetch results.
const GIT_METADATA_FILES: &[&str] = &["HEAD", "index", "packed-refs", "FETCH_HEAD"];

#[derive(Debug, Clone, PartialEq, Eq)]
struct RepoWatch {
    root: PathBuf,
    /// Real Git directory plus the common directory shared by linked
    /// worktrees. Both can contain metadata we need to observe.
    git_dirs: Vec<PathBuf>,
}

impl RepoWatch {
    fn new(root: &Path) -> Self {
        let mut git_dirs = Vec::new();
        if let Some(git_dir) = resolve_git_dir(root) {
            push_unique(&mut git_dirs, git_dir.clone());
            if let Some(common_dir) = resolve_common_dir(&git_dir) {
                push_unique(&mut git_dirs, common_dir);
            }
        }
        Self {
            root: root.to_path_buf(),
            git_dirs,
        }
    }

    /// Whether an event path belongs to this repository and is meaningful.
    fn matches(&self, path: &Path) -> bool {
        // Metadata is checked first: `.git/HEAD` is inside the working tree but
        // must not be dropped by the VCS-internals filter below.
        if self
            .git_dirs
            .iter()
            .any(|git_dir| is_git_metadata_path(git_dir, path))
        {
            return true;
        }
        path.starts_with(&self.root) && !is_ignored_working_path(&self.root, path)
    }
}

#[derive(Default)]
struct State {
    roots: Vec<RepoWatch>,
    /// Every path currently registered with notify, so a changed root set can
    /// be unwatched without leaking watches.
    watched: Vec<PathBuf>,
    dirty: HashSet<PathBuf>,
}

pub struct RepoWatcher {
    watcher: notify::RecommendedWatcher,
    state: Arc<Mutex<State>>,
    registration_errors: Vec<String>,
}

impl RepoWatcher {
    pub fn new() -> anyhow::Result<Self> {
        let state: Arc<Mutex<State>> = Arc::new(Mutex::new(State::default()));
        let sink = state.clone();

        let watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
            let Ok(event) = result else {
                return;
            };
            if !is_relevant_event(&event.kind) {
                return;
            }

            let Ok(mut state) = sink.lock() else {
                return;
            };
            for changed in &event.paths {
                // Clone the root so the immutable borrow of `state.roots` ends
                // before mutating the dirty set.
                let root = state
                    .roots
                    .iter()
                    .find(|repo| repo.matches(changed))
                    .map(|repo| repo.root.clone());
                if let Some(root) = root {
                    state.dirty.insert(root);
                }
            }
        })?;

        Ok(Self {
            watcher,
            state,
            registration_errors: Vec::new(),
        })
    }

    /// Replace the watched set with `roots` (absolute repository paths).
    ///
    /// Registration failures are retained for [`Self::registration_errors`].
    /// On Linux this is where an exhausted inotify watch limit becomes visible;
    /// callers should report the message instead of silently running without
    /// filesystem updates.
    pub fn set_roots(&mut self, roots: Vec<PathBuf>) {
        let mut seen = HashSet::new();
        let roots: Vec<PathBuf> = roots
            .into_iter()
            .filter(|root| seen.insert(root.clone()))
            .collect();
        let specs: Vec<RepoWatch> = roots.iter().map(|root| RepoWatch::new(root)).collect();

        let old_watched = {
            let Ok(mut state) = self.state.lock() else {
                self.registration_errors = vec!["file watcher state is poisoned".to_string()];
                return;
            };
            if state.roots == specs {
                return;
            }

            // Publish the new roots before touching notify. The callback can
            // run on its own thread; calling `watch`/`unwatch` while holding
            // the state lock could deadlock against an event delivery.
            let old_watched = std::mem::take(&mut state.watched);
            state.roots = specs.clone();
            state.dirty.clear();
            old_watched
        };

        for path in old_watched {
            // Removing an already-gone watch is harmless; registration errors
            // below are the ones worth surfacing.
            let _ = self.watcher.unwatch(&path);
        }

        let mut watched = Vec::new();
        let mut errors = Vec::new();
        for spec in &specs {
            if spec.root.is_dir() {
                Self::watch(
                    &mut self.watcher,
                    &spec.root,
                    RecursiveMode::Recursive,
                    &mut watched,
                    &mut errors,
                );
            }
            for git_dir in &spec.git_dirs {
                if git_dir.is_dir() {
                    // Watch the directory rather than the individual files:
                    // Git replaces HEAD and index via rename, which would leave
                    // a file watch pointing at the old inode. The callback
                    // filters events to `GIT_METADATA_FILES` and `refs/`.
                    Self::watch(
                        &mut self.watcher,
                        git_dir,
                        RecursiveMode::NonRecursive,
                        &mut watched,
                        &mut errors,
                    );
                }
                let refs = git_dir.join("refs");
                if refs.is_dir() {
                    Self::watch(
                        &mut self.watcher,
                        &refs,
                        RecursiveMode::Recursive,
                        &mut watched,
                        &mut errors,
                    );
                }
            }
        }

        if let Ok(mut state) = self.state.lock() {
            state.watched = watched;
        }
        self.registration_errors = errors;
    }

    fn watch(
        watcher: &mut notify::RecommendedWatcher,
        path: &Path,
        mode: RecursiveMode,
        watched: &mut Vec<PathBuf>,
        errors: &mut Vec<String>,
    ) {
        // Repositories that are views of the same checkout (or a linked
        // worktree sharing the main Git dir) must not register the same watch
        // twice: notify reports an error for duplicate registrations.
        if watched.iter().any(|registered| registered == path) {
            return;
        }
        match watcher.watch(path, mode) {
            Ok(()) => watched.push(path.to_path_buf()),
            Err(error) => errors.push(format!("{}: {error}", path.display())),
        }
    }

    /// Registration failures from the last successful `set_roots` call.
    pub fn registration_errors(&self) -> &[String] {
        &self.registration_errors
    }

    /// Roots that changed since the last call.
    pub fn take_dirty(&self) -> Vec<PathBuf> {
        let Ok(mut state) = self.state.lock() else {
            return Vec::new();
        };
        state.dirty.drain().collect()
    }
}

fn is_relevant_event(kind: &EventKind) -> bool {
    match kind {
        EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(_) => {
            // Reading a file can update its access time on some filesystems.
            // Acting on that would make our own `git status` rescans dirty the
            // repository again, so only content/metadata writes count.
            !matches!(
                kind,
                EventKind::Modify(ModifyKind::Metadata(MetadataKind::AccessTime))
            )
        }
        _ => false,
    }
}

/// Whether `path` is one of the Git metadata paths Vantage refreshes on.
fn is_git_metadata_path(git_dir: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(git_dir) else {
        return false;
    };
    let mut components = relative.components();
    let Some(Component::Normal(first)) = components.next() else {
        return false;
    };
    let first = first.to_string_lossy();
    if first == "refs" {
        return true;
    }
    components.next().is_none() && GIT_METADATA_FILES.contains(&first.as_ref())
}

/// Whether a working-tree path is noise we never want to rescan for.
fn is_ignored_working_path(root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    relative.components().any(|component| match component {
        Component::Normal(name) => {
            let name = name.to_string_lossy();
            IGNORED_DIRS.contains(&name.as_ref()) || VCS_DIRS.contains(&name.as_ref())
        }
        _ => false,
    })
}

/// Resolve the real Git directory for `root`.
///
/// Handles both the normal `.git` directory and the `.git` file used by linked
/// worktrees and submodules (`gitdir: /path/to/.git/worktrees/name`).
fn resolve_git_dir(root: &Path) -> Option<PathBuf> {
    let dot_git = root.join(".git");
    if dot_git.is_dir() {
        return Some(normalize_path(&dot_git));
    }
    if !dot_git.is_file() {
        return None;
    }

    let contents = std::fs::read_to_string(&dot_git).ok()?;
    let target = contents.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim()
            .eq_ignore_ascii_case("gitdir")
            .then(|| value.trim())
    })?;
    if target.is_empty() {
        return None;
    }

    let target = Path::new(target);
    let resolved = if target.is_absolute() {
        target.to_path_buf()
    } else {
        root.join(target)
    };
    Some(normalize_path(&resolved))
}

/// Linked worktrees keep HEAD/index in their private Git dir, but refs and
/// packed-refs in the main repository's common dir (`commondir`).
fn resolve_common_dir(git_dir: &Path) -> Option<PathBuf> {
    let contents = std::fs::read_to_string(git_dir.join("commondir")).ok()?;
    let value = contents.trim();
    if value.is_empty() {
        return None;
    }
    let path = Path::new(value);
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        git_dir.join(path)
    };
    Some(normalize_path(&resolved))
}

/// Remove `.`/`..` lexically so watched paths and notify's event paths compare
/// consistently (notably a worktree's `commondir` of `../..`).
fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn push_unique(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.contains(&path) {
        paths.push(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::time::{Duration, Instant};

    #[test]
    fn ignored_working_paths_cover_build_and_vcs_directories() {
        let root = Path::new("/repo");
        for ignored in [
            "target/debug/vantage",
            "node_modules/pkg/index.js",
            "dist/assets/app.js",
            "build/out.o",
            ".next/cache/webpack",
            "coverage/lcov.info",
            ".venv/bin/python",
            "vendor/crate/src/lib.rs",
            ".git/objects/ab/cdef",
            ".git/index.lock",
            ".git/logs/HEAD",
            ".hg/store/data",
            ".svn/pristine",
        ] {
            assert!(
                is_ignored_working_path(root, &root.join(ignored)),
                "{ignored} should be ignored"
            );
        }

        for kept in [
            "src/main.rs",
            "README.md",
            ".gitignore",
            ".github/workflows/ci.yml",
            "targets/notes.md",
            "build.rs",
            "src/vendor.rs",
        ] {
            assert!(
                !is_ignored_working_path(root, &root.join(kept)),
                "{kept} should not be ignored"
            );
        }
    }

    #[test]
    fn git_metadata_matching_is_limited_to_state_files() {
        let git_dir = Path::new("/repo/.git");
        for path in [
            "/repo/.git/HEAD",
            "/repo/.git/index",
            "/repo/.git/packed-refs",
            "/repo/.git/FETCH_HEAD",
            "/repo/.git/refs",
            "/repo/.git/refs/heads/main",
            "/repo/.git/refs/remotes/origin/main",
        ] {
            assert!(
                is_git_metadata_path(git_dir, Path::new(path)),
                "{path} is metadata"
            );
        }

        for path in [
            "/repo/.git/config",
            "/repo/.git/index.lock",
            "/repo/.git/logs/HEAD",
            "/repo/.git/objects/ab/cdef",
            "/repo/.git/description",
            "/repo/src/main.rs",
        ] {
            assert!(
                !is_git_metadata_path(git_dir, Path::new(path)),
                "{path} is not metadata"
            );
        }
    }

    #[test]
    fn repository_matching_prefers_metadata_over_ignored_directories() {
        let temp = tempfile::tempdir().expect("temp dir");
        let root = temp.path().join("repo");
        std::fs::create_dir_all(root.join(".git/refs/heads")).expect("git dirs");

        let watch = RepoWatch::new(&root);
        assert_eq!(watch.git_dirs, vec![root.join(".git")]);

        assert!(watch.matches(&root.join("src/main.rs")));
        assert!(watch.matches(&root.join(".git/HEAD")));
        assert!(watch.matches(&root.join(".git/refs/heads/main")));
        assert!(!watch.matches(&root.join(".git/objects/ab/cdef")));
        assert!(!watch.matches(&root.join("target/debug/app")));
        assert!(!watch.matches(&root.join("node_modules/pkg/index.js")));
    }

    #[test]
    fn worktree_git_file_resolves_private_and_common_directories() {
        let temp = tempfile::tempdir().expect("temp dir");
        let main_git = temp.path().join("main/.git");
        let worktree_git = main_git.join("worktrees/feature");
        std::fs::create_dir_all(&worktree_git).expect("worktree git dir");
        std::fs::write(worktree_git.join("HEAD"), "ref: refs/heads/feature\n").expect("HEAD");
        std::fs::write(worktree_git.join("index"), b"index").expect("index");
        std::fs::write(worktree_git.join("commondir"), "../..\n").expect("commondir");

        let root = temp.path().join("checkout");
        std::fs::create_dir_all(&root).expect("worktree root");
        std::fs::write(
            root.join(".git"),
            format!("gitdir: {}\n", worktree_git.display()),
        )
        .expect(".git file");

        let watch = RepoWatch::new(&root);
        assert_eq!(watch.git_dirs, vec![worktree_git.clone(), main_git.clone()]);

        // Private worktree state...
        assert!(watch.matches(&worktree_git.join("HEAD")));
        assert!(watch.matches(&worktree_git.join("index")));
        // ...and shared refs/packed-refs from the common repository.
        assert!(watch.matches(&main_git.join("refs/heads/main")));
        assert!(watch.matches(&main_git.join("packed-refs")));
        assert!(!watch.matches(&main_git.join("objects/ab/cdef")));
    }

    #[test]
    fn external_commit_marks_repository_dirty() {
        let temp = tempfile::tempdir().expect("temp dir");
        let root = temp.path().join("repo");
        std::fs::create_dir(&root).expect("repo root");
        run_git(&root, &["init", "-q"]);
        run_git(&root, &["config", "user.email", "watcher@example.invalid"]);
        run_git(&root, &["config", "user.name", "Watcher Test"]);
        run_git(&root, &["commit", "-q", "--allow-empty", "-m", "initial"]);

        // Match the path form notify reports (on macOS temp dirs live behind
        // `/private/var`, while `temp_dir()` returns `/var`).
        let root = std::fs::canonicalize(&root).expect("canonical repo root");

        let mut watcher = RepoWatcher::new().expect("create watcher");
        watcher.set_roots(vec![root.clone()]);
        let errors = watcher.registration_errors();
        assert!(errors.is_empty(), "registration failed: {errors:?}");

        // A separate Git process, as an agent or terminal would run it. The
        // empty commit changes only Git metadata, never a working-tree file.
        run_git(&root, &["commit", "-q", "--allow-empty", "-m", "external"]);

        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if watcher.take_dirty().iter().any(|dirty| dirty == &root) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "watcher did not report the external commit"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn run_git(root: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
