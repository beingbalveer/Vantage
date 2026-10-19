use crate::error::AppError;
use crate::git::runner::GitRunner;
use crate::state::{StashEntry, StashPushResult};
use std::path::Path;

/// Machine-readable `git stash list` format: selector, sha, committer date, subject,
/// terminated with a record separator so messages cannot bleed between entries.
pub const STASH_LIST_FORMAT: &str = "%gd\x1f%H\x1f%cI\x1f%s\x1e";

/// Parse the output produced with [`STASH_LIST_FORMAT`]. Pure function, unit-tested.
pub fn parse_stash_list(output: &str) -> Vec<StashEntry> {
    let mut entries = Vec::new();
    for (position, record) in output.split('\x1e').enumerate() {
        let record = record.trim_matches(['\n', '\r']);
        if record.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = record.split('\x1f').collect();
        if parts.len() < 4 {
            continue;
        }
        let index = parse_stash_index(parts[0]).unwrap_or(position as u32);
        entries.push(StashEntry {
            index,
            sha: parts[1].trim().to_string(),
            date: parts[2].trim().to_string(),
            subject: parts[3].to_string(),
        });
    }
    entries
}

/// `stash@{0}` -> `Some(0)`; anything else -> `None`.
fn parse_stash_index(selector: &str) -> Option<u32> {
    let selector = selector.trim();
    let open = selector.rfind("@{")?;
    let rest = &selector[open + 2..];
    rest.trim_end_matches('}').trim().parse::<u32>().ok()
}

fn stash_selector(index: u32) -> String {
    format!("stash@{{{}}}", index)
}

/// Map git's stderr onto the closest typed error, always preserving the real text
/// so the UI can show exactly why git refused.
fn stash_failure(stderr: String) -> AppError {
    let message = stderr.trim().to_string();
    let friendly = message.to_string();
    if message.contains("would be overwritten")
        || message.contains("Your local changes")
        || message.contains("local changes")
        || message.contains("already exists, no checkout")
        || message.contains("could not restore")
    {
        AppError::conflict(friendly).with_stderr(stderr)
    } else {
        AppError::other(friendly).with_stderr(stderr)
    }
}

/// List stashes newest-first.
pub async fn list_stashes(
    runner: &GitRunner,
    repo_path: &Path,
) -> Result<Vec<StashEntry>, AppError> {
    let format_arg = format!("--format={}", STASH_LIST_FORMAT);
    let out = runner
        .run_read(repo_path, &["stash", "list", &format_arg])
        .await?;
    if !out.success() {
        return Err(AppError::other(out.stderr));
    }
    Ok(parse_stash_list(&out.stdout_string()))
}

async fn latest_stash(runner: &GitRunner, repo_path: &Path) -> Option<StashEntry> {
    list_stashes(runner, repo_path)
        .await
        .ok()
        .and_then(|mut entries| {
            if entries.is_empty() {
                None
            } else {
                Some(entries.remove(0))
            }
        })
}

/// Create a stash. Returns `stashed: false` (not an error) when there is nothing to save.
pub(crate) async fn stash_push_locked(
    runner: &GitRunner,
    repo_path: &Path,
    message: Option<&str>,
    include_untracked: bool,
) -> Result<StashPushResult, AppError> {
    let mut args: Vec<&str> = vec!["stash", "push"];
    if include_untracked {
        args.push("--include-untracked");
    }
    let trimmed_message = message.map(str::trim).filter(|m| !m.is_empty());
    if let Some(m) = trimmed_message {
        args.push("-m");
        args.push(m);
    }

    let out = runner.run_write(repo_path, &args, 30).await?;
    if !out.success() {
        return Err(stash_failure(out.stderr));
    }

    // git reports "No local changes to save" on stdout when there is nothing to stash.
    if out.stdout_string().contains("No local changes to save") {
        return Ok(StashPushResult {
            stashed: false,
            entry: None,
        });
    }

    Ok(StashPushResult {
        stashed: true,
        entry: latest_stash(runner, repo_path).await,
    })
}

pub async fn stash_push(
    runner: &GitRunner,
    repo_path: &Path,
    message: Option<&str>,
    include_untracked: bool,
) -> Result<StashPushResult, AppError> {
    let repo_lock = runner.get_repo_lock(repo_path).await;
    let _guard = repo_lock.lock().await;
    stash_push_locked(runner, repo_path, message, include_untracked).await
}

pub(crate) async fn stash_pop_locked(
    runner: &GitRunner,
    repo_path: &Path,
    index: u32,
) -> Result<(), AppError> {
    let selector = stash_selector(index);
    let out = runner
        .run_write(repo_path, &["stash", "pop", &selector], 30)
        .await?;
    if !out.success() {
        return Err(stash_failure(out.stderr));
    }
    Ok(())
}

pub async fn stash_pop(
    runner: &GitRunner,
    repo_path: &Path,
    index: u32,
) -> Result<(), AppError> {
    let repo_lock = runner.get_repo_lock(repo_path).await;
    let _guard = repo_lock.lock().await;
    stash_pop_locked(runner, repo_path, index).await
}

pub(crate) async fn stash_apply_locked(
    runner: &GitRunner,
    repo_path: &Path,
    index: u32,
) -> Result<(), AppError> {
    let selector = stash_selector(index);
    let out = runner
        .run_write(repo_path, &["stash", "apply", &selector], 30)
        .await?;
    if !out.success() {
        return Err(stash_failure(out.stderr));
    }
    Ok(())
}

pub async fn stash_apply(
    runner: &GitRunner,
    repo_path: &Path,
    index: u32,
) -> Result<(), AppError> {
    let repo_lock = runner.get_repo_lock(repo_path).await;
    let _guard = repo_lock.lock().await;
    stash_apply_locked(runner, repo_path, index).await
}

pub(crate) async fn stash_drop_locked(
    runner: &GitRunner,
    repo_path: &Path,
    index: u32,
) -> Result<(), AppError> {
    let selector = stash_selector(index);
    let out = runner
        .run_write(repo_path, &["stash", "drop", &selector], 30)
        .await?;
    if !out.success() {
        return Err(stash_failure(out.stderr));
    }
    Ok(())
}

pub async fn stash_drop(
    runner: &GitRunner,
    repo_path: &Path,
    index: u32,
) -> Result<(), AppError> {
    let repo_lock = runner.get_repo_lock(repo_path).await;
    let _guard = repo_lock.lock().await;
    stash_drop_locked(runner, repo_path, index).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct TempTestRepo {
        path: std::path::PathBuf,
    }

    impl TempTestRepo {
        async fn new(runner: &GitRunner) -> Self {
            let path = std::env::temp_dir().join(format!("vantage_stash_{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();

            runner.run_write(&path, &["init"], 10).await.unwrap();
            runner
                .run_write(&path, &["config", "user.name", "Vantage Test"], 10)
                .await
                .unwrap();
            runner
                .run_write(&path, &["config", "user.email", "vantage@test.local"], 10)
                .await
                .unwrap();
            runner
                .run_write(&path, &["config", "commit.gpgsign", "false"], 10)
                .await
                .unwrap();

            Self { path }
        }

        async fn commit_all(&self, runner: &GitRunner, message: &str) {
            runner
                .run_write(&self.path, &["add", "--", "."], 10)
                .await
                .unwrap();
            runner
                .run_write(&self.path, &["commit", "-m", message], 10)
                .await
                .unwrap();
        }
    }

    impl Drop for TempTestRepo {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn test_parse_stash_list() {
        let raw = "stash@{0}\x1fabc123\x1f2026-10-19T10:00:00+00:00\x1fWIP on main: 7f3a2c1 add auth\x1e\nstash@{1}\x1fdef456\x1f2026-10-18T09:00:00+00:00\x1fOn feature: my message\x1e\n";
        let entries = parse_stash_list(raw);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].index, 0);
        assert_eq!(entries[0].sha, "abc123");
        assert_eq!(entries[0].date, "2026-10-19T10:00:00+00:00");
        assert_eq!(entries[0].subject, "WIP on main: 7f3a2c1 add auth");
        assert_eq!(entries[1].index, 1);
        assert_eq!(entries[1].subject, "On feature: my message");
    }

    #[test]
    fn test_parse_stash_list_empty_and_malformed() {
        assert!(parse_stash_list("").is_empty());
        assert!(parse_stash_list("garbage-without-separators").is_empty());
        // A record missing fields is skipped rather than panicking.
        let partial = "stash@{4}\x1fonly-two\x1e";
        assert!(parse_stash_list(partial).is_empty());
    }

    #[test]
    fn test_parse_stash_index_fallbacks() {
        assert_eq!(parse_stash_index("stash@{0}"), Some(0));
        assert_eq!(parse_stash_index("stash@{12}"), Some(12));
        assert_eq!(parse_stash_index("nonsense"), None);
    }

    #[tokio::test]
    async fn test_stash_push_list_and_pop() {
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;
        let file = repo.path.join("file.txt");
        fs::write(&file, "v1\n").unwrap();
        repo.commit_all(&runner, "init").await;

        fs::write(&file, "v2\n").unwrap();

        let push = stash_push(&runner, &repo.path, Some("parking spot"), false)
            .await
            .expect("stash push should succeed");
        assert!(push.stashed, "there were changes to stash");
        let entry = push.entry.expect("push returns the created stash");
        assert_eq!(entry.index, 0);
        assert!(!entry.sha.is_empty());
        assert!(
            entry.subject.contains("parking spot"),
            "stash subject should carry the message, got: {}",
            entry.subject
        );

        // Working tree is clean again.
        assert_eq!(fs::read_to_string(&file).unwrap(), "v1\n");

        let list = list_stashes(&runner, &repo.path).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].index, 0);

        stash_pop(&runner, &repo.path, 0)
            .await
            .expect("pop should restore changes");
        assert_eq!(fs::read_to_string(&file).unwrap(), "v2\n");
        assert!(list_stashes(&runner, &repo.path).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_stash_push_with_nothing_to_stash_is_not_an_error() {
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;
        let file = repo.path.join("file.txt");
        fs::write(&file, "v1\n").unwrap();
        repo.commit_all(&runner, "init").await;

        let push = stash_push(&runner, &repo.path, None, false)
            .await
            .expect("nothing to stash must not be an error");
        assert!(!push.stashed);
        assert!(push.entry.is_none());
    }

    #[tokio::test]
    async fn test_stash_push_untracked_is_explicit() {
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;
        let file = repo.path.join("file.txt");
        fs::write(&file, "v1\n").unwrap();
        repo.commit_all(&runner, "init").await;

        let untracked = repo.path.join("untracked.txt");
        fs::write(&untracked, "new file\n").unwrap();

        // Without --include-untracked there is nothing (tracked) to stash.
        let without = stash_push(&runner, &repo.path, None, false).await.unwrap();
        assert!(!without.stashed);
        assert!(untracked.exists());

        // With the explicit choice, the untracked file is parked.
        let with = stash_push(&runner, &repo.path, Some("with untracked"), true)
            .await
            .unwrap();
        assert!(with.stashed);
        assert!(!untracked.exists());
    }

    #[tokio::test]
    async fn test_stash_apply_keeps_stash_and_drop_removes() {
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;
        let file = repo.path.join("file.txt");
        fs::write(&file, "v1\n").unwrap();
        repo.commit_all(&runner, "init").await;

        fs::write(&file, "v2\n").unwrap();
        stash_push(&runner, &repo.path, None, false).await.unwrap();

        stash_apply(&runner, &repo.path, 0)
            .await
            .expect("apply should restore changes");
        assert_eq!(fs::read_to_string(&file).unwrap(), "v2\n");
        // apply keeps the stash until explicitly dropped.
        assert_eq!(list_stashes(&runner, &repo.path).await.unwrap().len(), 1);

        // Discard the applied changes, then drop the stash.
        runner
            .run_write(&repo.path, &["checkout", "--", "file.txt"], 10)
            .await
            .unwrap();
        stash_drop(&runner, &repo.path, 0).await.unwrap();
        assert!(list_stashes(&runner, &repo.path).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_stash_pop_refuses_when_tree_would_be_overwritten() {
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;
        let file = repo.path.join("file.txt");
        fs::write(&file, "v1\n").unwrap();
        repo.commit_all(&runner, "init").await;

        // Park a change to file.txt.
        fs::write(&file, "stashed value\n").unwrap();
        stash_push(&runner, &repo.path, None, false).await.unwrap();

        // Make a conflicting local change to the same file.
        fs::write(&file, "conflicting local change\n").unwrap();

        let err = stash_pop(&runner, &repo.path, 0)
            .await
            .expect_err("pop must refuse when it would overwrite local changes");
        // The real git text is surfaced, not a generic failure.
        let text = format!("{}", err);
        assert!(
            text.contains("local changes") || text.contains("overwritten"),
            "expected git's real error text, got: {}",
            text
        );
        // The stash is retained so the work is not lost.
        assert_eq!(list_stashes(&runner, &repo.path).await.unwrap().len(), 1);
    }
}
