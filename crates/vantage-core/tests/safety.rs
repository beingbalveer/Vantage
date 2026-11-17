//! Cross-cutting safety regression tests for invariants that hold on `main`.
//!
//! These tests deliberately use only `vantage-core`'s public API and the real
//! `git` binary against a disposable repository per test. The module-level
//! unit tests already cover several of these paths; this file proves the same
//! contract from outside the crate, where the UI and future surfaces consume
//! it.
//!
//! Every test removes its temporary directory on drop.

use std::fs;
use std::path::{Path, PathBuf};

use vantage_core::git::ops::{commit, discard, reset_to, stage_path};
use vantage_core::git::refs::{
    delete_branch, is_branch_merged, list_branches, BranchDeleteOutcome,
};
use vantage_core::git::runner::GitOutput;
use vantage_core::persist::StatePersistence;
use vantage_core::GitRunner;

const AUTHOR_NAME: &str = "Vantage Safety Test";
const AUTHOR_EMAIL: &str = "safety@vantage.invalid";

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "vantage-safety-{label}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct TempRepo {
    root: TempDir,
}

impl TempRepo {
    async fn new(runner: &GitRunner) -> Self {
        let root = TempDir::new("repo");
        let repo = Self { root };
        repo.git(runner, &["init", "-q", "-b", "main"]).await;
        repo.git(runner, &["config", "user.name", AUTHOR_NAME])
            .await;
        repo.git(runner, &["config", "user.email", AUTHOR_EMAIL])
            .await;
        repo.git(runner, &["config", "commit.gpgsign", "false"])
            .await;
        repo
    }

    fn path(&self) -> &Path {
        self.root.path()
    }

    fn write(&self, name: &str, contents: &str) {
        let path = self.path().join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create fixture parent");
        }
        fs::write(path, contents).expect("write fixture file");
    }

    async fn git(&self, runner: &GitRunner, args: &[&str]) -> String {
        let out = runner
            .run_write(self.path(), args, 30)
            .await
            .unwrap_or_else(|error| panic!("git {args:?} failed to start: {error}"));
        assert!(out.success(), "git {args:?} failed:\n{}", out.stderr.trim());
        out.stdout_string()
    }

    async fn read(&self, runner: &GitRunner, args: &[&str]) -> GitOutput {
        runner
            .run_read(self.path(), args)
            .await
            .unwrap_or_else(|error| panic!("git {args:?} failed to start: {error}"))
    }

    async fn commit_all(&self, runner: &GitRunner, message: &str) -> String {
        self.git(runner, &["add", "-A", "."]).await;
        self.git(runner, &["commit", "-q", "-m", message]).await;
        self.git(runner, &["rev-parse", "HEAD"])
            .await
            .trim()
            .to_string()
    }

    async fn head(&self, runner: &GitRunner) -> String {
        self.git(runner, &["rev-parse", "HEAD"])
            .await
            .trim()
            .to_string()
    }
}

/// Paths from a `-z`-terminated git output, with no quoting.
fn nul_paths(out: &GitOutput) -> Vec<String> {
    out.stdout
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
        .map(|part| String::from_utf8_lossy(part).into_owned())
        .collect()
}

fn sorted(mut paths: Vec<String>) -> Vec<String> {
    paths.sort();
    paths
}

#[tokio::test]
async fn literal_pathspec_staging_and_discard_only_touch_the_named_path() {
    let runner = GitRunner::new();
    let repo = TempRepo::new(&runner).await;

    // `*` and `?` are ordinary characters in a Unix filename. Git must treat
    // them literally when Vantage hands the path over.
    repo.write("other.txt", "other v1\n");
    repo.write("*.txt", "star v1\n");
    repo.write("what?.txt", "question v1\n");
    repo.commit_all(&runner, "baseline").await;

    repo.write("other.txt", "other v2\n");
    repo.write("*.txt", "star v2\n");
    repo.write("what?.txt", "question v2\n");

    stage_path(&runner, repo.path(), "*.txt")
        .await
        .expect("staging the literal '*' filename must succeed");

    let staged = sorted(nul_paths(
        &repo
            .read(&runner, &["diff", "--cached", "--name-only", "-z"])
            .await,
    ));
    assert_eq!(
        staged,
        vec!["*.txt"],
        "staging `*.txt` must not stage other paths"
    );

    let unstaged = sorted(nul_paths(
        &repo.read(&runner, &["diff", "--name-only", "-z"]).await,
    ));
    assert_eq!(
        unstaged,
        vec!["other.txt", "what?.txt"],
        "the other edited files must stay unstaged"
    );

    // Discarding the literal star path restores only that file. The stash
    // backup `discard` records is intentionally not reused here.
    discard(&runner, repo.path(), &["*.txt".to_string()])
        .await
        .expect("discarding the literal '*' filename must succeed");
    assert_eq!(
        fs::read_to_string(repo.path().join("*.txt")).unwrap(),
        "star v1\n"
    );
    assert_eq!(
        fs::read_to_string(repo.path().join("other.txt")).unwrap(),
        "other v2\n",
        "discarding `*.txt` must not touch other.txt"
    );
    assert_eq!(
        fs::read_to_string(repo.path().join("what?.txt")).unwrap(),
        "question v2\n",
        "discarding `*.txt` must not touch what?.txt"
    );

    // Repeat with the `?` wildcard character, which is also literal here.
    stage_path(&runner, repo.path(), "what?.txt")
        .await
        .expect("staging the literal '?' filename must succeed");

    let staged = sorted(nul_paths(
        &repo
            .read(&runner, &["diff", "--cached", "--name-only", "-z"])
            .await,
    ));
    assert_eq!(staged, vec!["what?.txt"]);

    let unstaged = sorted(nul_paths(
        &repo.read(&runner, &["diff", "--name-only", "-z"]).await,
    ));
    assert_eq!(
        unstaged,
        vec!["other.txt"],
        "the unselected file must remain unstaged"
    );

    discard(&runner, repo.path(), &["what?.txt".to_string()])
        .await
        .expect("discarding the literal '?' filename must succeed");
    assert_eq!(
        fs::read_to_string(repo.path().join("what?.txt")).unwrap(),
        "question v1\n"
    );
    assert_eq!(
        fs::read_to_string(repo.path().join("other.txt")).unwrap(),
        "other v2\n",
        "only the named path may change"
    );
}

#[tokio::test]
async fn selective_root_commit_excludes_an_unselected_staged_file() {
    let runner = GitRunner::new();
    let repo = TempRepo::new(&runner).await;

    // A file already in the real index but not selected for this commit.
    repo.write("secret.txt", "secret credentials\n");
    repo.git(&runner, &["add", "secret.txt"]).await;

    // The file the caller selects. It does not have to be staged first; the
    // commit path stages exactly this path in its temporary index.
    repo.write("chosen.txt", "public content\n");

    let committed = commit(
        &runner,
        repo.path(),
        &["chosen.txt".to_string()],
        "root commit with chosen file only",
        None,
        &[],
    )
    .await
    .expect("selective root commit must succeed");
    assert_eq!(committed.subject, "root commit with chosen file only");

    let head_paths = sorted(nul_paths(
        &repo
            .read(&runner, &["ls-tree", "-r", "--name-only", "-z", "HEAD"])
            .await,
    ));
    assert_eq!(
        head_paths,
        vec!["chosen.txt"],
        "the root commit must contain only the selected path"
    );

    let staged_paths = sorted(nul_paths(
        &repo
            .read(&runner, &["diff", "--cached", "--name-only", "-z"])
            .await,
    ));
    assert_eq!(
        staged_paths,
        vec!["secret.txt"],
        "a pre-staged file that was not selected must stay staged"
    );
}

#[tokio::test]
async fn reset_dirty_tree_refusal_leaves_head_and_worktree_untouched() {
    let runner = GitRunner::new();
    let repo = TempRepo::new(&runner).await;

    repo.write("tracked.txt", "v1\n");
    let first = repo.commit_all(&runner, "first").await;
    repo.write("tracked.txt", "v2\n");
    let second = repo.commit_all(&runner, "second").await;

    // Both a tracked edit and an untracked file make the tree dirty. A reset
    // that promises not to discard changes must refuse before touching either.
    repo.write("tracked.txt", "uncommitted edit\n");
    repo.write("untracked.txt", "new file\n");

    let error = reset_to(&runner, repo.path(), &first, false)
        .await
        .expect_err("a dirty tree must be refused");
    assert_eq!(error.code, "Dirty");

    assert_eq!(
        repo.head(&runner).await,
        second,
        "a refused reset must not move HEAD"
    );
    assert_eq!(
        fs::read_to_string(repo.path().join("tracked.txt")).unwrap(),
        "uncommitted edit\n",
        "a refused reset must keep the tracked edit"
    );
    assert_eq!(
        fs::read_to_string(repo.path().join("untracked.txt")).unwrap(),
        "new file\n",
        "a refused reset must keep the untracked file"
    );

    let backups = repo
        .read(
            &runner,
            &["for-each-ref", "--format=%(refname)", "refs/vantage/backup"],
        )
        .await;
    assert!(
        backups.stdout_string().trim().is_empty(),
        "a refused reset must not create a backup ref: {:?}",
        backups.stdout_string()
    );
}

#[tokio::test]
async fn branch_delete_refuses_unmerged_work_and_succeeds_once_merged() {
    let runner = GitRunner::new();
    let repo = TempRepo::new(&runner).await;

    repo.write("base.txt", "base\n");
    repo.commit_all(&runner, "base").await;

    repo.git(&runner, &["checkout", "-q", "-b", "feature"])
        .await;
    repo.write("feature.txt", "feature work\n");
    repo.commit_all(&runner, "feature work").await;
    repo.git(&runner, &["checkout", "-q", "main"]).await;

    assert!(
        !is_branch_merged(&runner, repo.path(), "feature")
            .await
            .expect("check merged state"),
        "feature has a commit that main does not contain"
    );

    let outcome = delete_branch(&runner, repo.path(), "feature", false)
        .await
        .expect("check the unmerged delete outcome");
    assert_eq!(
        outcome,
        BranchDeleteOutcome::NotMerged,
        "deleting unmerged work without force must be refused"
    );
    let names: Vec<String> = list_branches(&runner, repo.path())
        .await
        .expect("list branches")
        .into_iter()
        .map(|branch| branch.name)
        .collect();
    assert!(
        names.iter().any(|name| name == "feature"),
        "a refused delete must leave the branch in place: {names:?}"
    );

    repo.git(
        &runner,
        &["merge", "--no-ff", "-m", "merge feature", "feature"],
    )
    .await;
    assert!(
        is_branch_merged(&runner, repo.path(), "feature")
            .await
            .expect("check merged state"),
        "feature is an ancestor of main after the merge"
    );

    delete_branch(&runner, repo.path(), "feature", false)
        .await
        .expect("deleting a merged branch must succeed without force");
    let names: Vec<String> = list_branches(&runner, repo.path())
        .await
        .expect("list branches")
        .into_iter()
        .map(|branch| branch.name)
        .collect();
    assert!(
        !names.iter().any(|name| name == "feature"),
        "the merged branch should be gone: {names:?}"
    );
}

#[test]
fn corrupt_state_load_warns_and_preserves_a_backup_file() {
    let dir = TempDir::new("state");
    let state_path = dir.path().join("state.json");
    let corrupt = r#"{ "repos": [ this is not valid json"#;
    fs::write(&state_path, corrupt).unwrap();

    let persistence = StatePersistence::with_path(state_path.clone());
    let (state, warning) = persistence.load();

    let warning = warning.expect("corrupt state must produce a visible warning");
    assert!(
        warning.contains("corrupt"),
        "the warning should name the corruption: {warning}"
    );
    assert!(
        !state_path.exists(),
        "the corrupt file must be moved out of the way"
    );
    assert!(
        state.repos.is_empty(),
        "loading corrupt state must fall back to an empty workspace"
    );

    let backups: Vec<PathBuf> = fs::read_dir(dir.path())
        .expect("read state dir")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("state.corrupt-"))
        })
        .collect();
    assert_eq!(
        backups.len(),
        1,
        "exactly one corrupt backup should be kept: {backups:?}"
    );
    assert_eq!(
        fs::read_to_string(&backups[0]).unwrap(),
        corrupt,
        "the backup must preserve the original corrupt bytes"
    );
    assert!(
        warning.contains(
            backups[0]
                .file_name()
                .and_then(|name| name.to_str())
                .expect("backup filename")
        ),
        "the warning should point at the backup: {warning}"
    );

    drop(persistence);
}
