use crate::error::AppError;
use crate::git::patch::{synthesize, PatchTarget, SelectedLine, Synthesized};
use crate::git::runner::GitRunner;
use crate::state::{BackupInfo, CommitSummary};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupRecord {
    pub id: String,
    pub kind: String, // "reset" | "discard"
    pub head_sha: Option<String>,
    pub stash_sha: Option<String>,
    pub paths: Vec<String>,
    pub untracked_dir: Option<String>,
    pub created_at: String,
    pub description: String,
}

pub async fn get_real_git_dir(runner: &GitRunner, repo_path: &Path) -> Result<PathBuf, AppError> {
    let out = runner
        .run_read(repo_path, &["rev-parse", "--git-dir"])
        .await?;
    if out.success() {
        let p_str = out.stdout_string().trim().to_string();
        let p = PathBuf::from(&p_str);
        if p.is_absolute() {
            Ok(p)
        } else {
            Ok(repo_path.join(p))
        }
    } else {
        Ok(repo_path.join(".git"))
    }
}

async fn save_backup_record(
    runner: &GitRunner,
    repo_path: &Path,
    record: &BackupRecord,
) -> Result<(), AppError> {
    let git_dir = get_real_git_dir(runner, repo_path).await?;
    let dir = git_dir.join("vantage-backups");
    let _ = fs::create_dir_all(&dir);
    let file = dir.join(format!("{}.json", record.id));
    let json = serde_json::to_string_pretty(record)?;
    fs::write(&file, json)?;
    Ok(())
}

async fn load_backup_record(
    runner: &GitRunner,
    repo_path: &Path,
    id_or_ref: &str,
) -> Option<BackupRecord> {
    let git_dir = get_real_git_dir(runner, repo_path).await.ok()?;
    let id = id_or_ref
        .strip_prefix("refs/vantage/backup/")
        .or_else(|| id_or_ref.strip_prefix("refs/relay/backup/"))
        .unwrap_or(id_or_ref);
    let file = git_dir.join("vantage-backups").join(format!("{}.json", id));
    if file.exists() {
        if let Ok(content) = fs::read_to_string(&file) {
            return serde_json::from_str(&content).ok();
        }
    }
    let legacy_file = git_dir.join("relay-backups").join(format!("{}.json", id));
    if legacy_file.exists() {
        if let Ok(content) = fs::read_to_string(&legacy_file) {
            return serde_json::from_str(&content).ok();
        }
    }
    None
}

/// Extract the path from a porcelain v2 `1`/`2` record without splitting on
/// spaces inside the filename. `fields` is the number of space-separated
/// fields before the path (8 for `1`, 9 for `2`).
fn porcelain_v2_path(line: &str, fields: usize) -> Option<&str> {
    let bytes = line.as_bytes();
    let mut idx = 0;
    for _ in 0..fields {
        let rel = bytes[idx..].iter().position(|b| *b == b' ')?;
        idx += rel + 1;
    }
    if idx >= bytes.len() {
        return None;
    }
    Some(&line[idx..])
}

async fn latest_head_summary(
    runner: &GitRunner,
    repo_path: &Path,
) -> Result<CommitSummary, AppError> {
    let log_out = runner
        .run_read(
            repo_path,
            &[
                "log",
                "-1",
                "--format=%H%x1f%an%x1f%ae%x1f%aI%x1f%P%x1f%s%x1f%b%x1e",
                "--shortstat",
                "HEAD",
            ],
        )
        .await?;
    let commits = crate::git::log::parse_log_output(
        &log_out.stdout_string(),
        &std::collections::HashSet::new(),
    );
    commits
        .into_iter()
        .next()
        .ok_or_else(|| AppError::other("Failed to get commit summary after commit"))
}

/// Commit a tree that preserves the index state of selected staged paths while
/// still taking working-tree content for selected paths that are not staged.
///
/// This is required for line-level staging: `git commit --only -- <path>`
/// takes the full working-tree content of `<path>` and would silently undo the
/// partial index. The temporary index starts as a copy of the real index so
/// selected staged paths (including renames) are committed exactly as staged;
/// unselected staged paths are reset to HEAD in the temporary index so they are
/// never committed. Real index entries are left untouched, except for paths
/// whose working-tree content was committed, which are added so they become
/// clean.
#[allow(clippy::too_many_arguments)]
async fn commit_using_temp_index(
    runner: &GitRunner,
    repo_path: &Path,
    git_dir: &Path,
    paths: &[String],
    paths_to_commit: &[String],
    staged_paths: &std::collections::HashMap<String, Option<String>>,
    summary: &str,
    body: Option<&str>,
) -> Result<CommitSummary, AppError> {
    let temp_index = git_dir.join(format!(
        "vantage-index-{}",
        uuid::Uuid::new_v4().simple()
    ));
    let temp_index_str = temp_index.to_string_lossy().to_string();

    let real_index = git_dir.join("index");
    let init = if real_index.exists() {
        fs::copy(&real_index, &temp_index)
            .map(|_| ())
            .map_err(AppError::io)
    } else {
        Ok(())
    };
    if let Err(e) = init {
        let _ = fs::remove_file(&temp_index);
        return Err(e);
    }
    if !temp_index.exists() {
        let out = runner
            .run_env(
                Some(repo_path),
                &["read-tree", "HEAD"],
                20,
                true,
                &[("GIT_INDEX_FILE", &temp_index_str)],
            )
            .await?;
        if !out.success() {
            let _ = fs::remove_file(&temp_index);
            return Err(AppError::other(out.stderr));
        }
    }

    // Reset staged content that the user did not select.
    for (path, orig) in staged_paths {
        if paths.contains(path) {
            continue;
        }
        let mut args = vec!["reset", "-q", "HEAD", "--", path.as_str()];
        if let Some(orig) = orig {
            args.push(orig.as_str());
        }
        let out = runner
            .run_env(
                Some(repo_path),
                &args,
                20,
                true,
                &[
                    ("GIT_INDEX_FILE", &temp_index_str),
                    ("GIT_LITERAL_PATHSPECS", "1"),
                ],
            )
            .await?;
        if !out.success() {
            let _ = fs::remove_file(&temp_index);
            return Err(AppError::other(out.stderr));
        }
    }

    // Stage the working-tree content of selected paths that are not already
    // staged (including the old side of unstaged renames).
    let add_paths: Vec<&String> = paths_to_commit
        .iter()
        .filter(|p| !staged_paths.contains_key(*p))
        .collect();
    if !add_paths.is_empty() {
        let mut add_args = vec!["add", "-A", "--"];
        for p in &add_paths {
            add_args.push(p.as_str());
        }
        let out = runner
            .run_env(
                Some(repo_path),
                &add_args,
                30,
                true,
                &[
                    ("GIT_INDEX_FILE", &temp_index_str),
                    ("GIT_LITERAL_PATHSPECS", "1"),
                ],
            )
            .await?;
        if !out.success() {
            let _ = fs::remove_file(&temp_index);
            return Err(AppError::other(out.stderr));
        }
    }

    let mut commit_args = vec!["commit", "-m", summary];
    let body_str;
    if let Some(b) = body {
        if !b.trim().is_empty() {
            body_str = b.to_string();
            commit_args.push("-m");
            commit_args.push(&body_str);
        }
    }

    let out = runner
        .run_env(
            Some(repo_path),
            &commit_args,
            60,
            true,
            &[("GIT_INDEX_FILE", &temp_index_str)],
        )
        .await;
    let _ = fs::remove_file(&temp_index);
    let out = out?;
    if !out.success() {
        return Err(AppError::other(out.stderr));
    }

    // Make the real index agree with the new HEAD for paths that came from the
    // working tree, so they are clean while unstaged lines in partially staged
    // files remain visible. Reset (rather than add) so a post-commit hook that
    // touches the file cannot leak extra content into the index.
    if !add_paths.is_empty() {
        let mut reset_args = vec!["reset", "-q", "HEAD", "--"];
        for p in &add_paths {
            reset_args.push(p.as_str());
        }
        let _ = runner
            .run_write_literal(repo_path, &reset_args, 30)
            .await;
    }

    latest_head_summary(runner, repo_path).await
}

pub async fn commit(
    runner: &GitRunner,
    repo_path: &Path,
    paths: &[String],
    summary: &str,
    body: Option<&str>,
) -> Result<CommitSummary, AppError> {
    if summary.trim().is_empty() {
        return Err(AppError::other("Commit summary cannot be empty"));
    }

    if paths.is_empty() {
        return Err(AppError::other("No files selected for commit"));
    }

    // Acquire per-repo mutex across the entire staging & commit operation
    let repo_lock = runner.get_repo_lock(repo_path).await;
    let _repo_guard = repo_lock.lock().await;

    // Check if HEAD exists
    let has_head = runner
        .run_read(repo_path, &["rev-parse", "--verify", "HEAD"])
        .await
        .map(|o| o.success())
        .unwrap_or(false);

    if has_head {
        // Find which selected paths are untracked and detect renames (R05)
        let status_out = runner
            .run_read(
                repo_path,
                &["status", "--porcelain=v2", "-z", "--untracked-files=all"],
            )
            .await?;

        let mut untracked_in_paths = Vec::new();
        let mut paths_to_commit: Vec<String> = paths.to_vec();
        // Staged paths (new path -> optional original path, for renames).
        let mut staged_paths: std::collections::HashMap<String, Option<String>> =
            std::collections::HashMap::new();

        let raw_parts: Vec<&[u8]> = status_out.stdout.split(|&b| b == 0).collect();
        let mut i = 0;
        while i < raw_parts.len() {
            let part = raw_parts[i];
            if part.is_empty() {
                i += 1;
                continue;
            }

            if part.starts_with(b"? ") {
                let un_path = String::from_utf8_lossy(&part[2..]).to_string();
                if paths.contains(&un_path) {
                    untracked_in_paths.push(un_path);
                }
                i += 1;
            } else if part.starts_with(b"2 ") {
                // Rename entry in porcelain v2: header contains <path>, next part contains <origPath>
                // Format: 2 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <X><score> <path>\0<origPath>\0
                let line = String::from_utf8_lossy(part);
                if let Some(path) = porcelain_v2_path(&line, 9) {
                    if i + 1 < raw_parts.len() {
                        let orig_path = String::from_utf8_lossy(raw_parts[i + 1]).to_string();
                        let xy = line.as_bytes().get(2).copied().unwrap_or(b'.');
                        if xy != b'.' {
                            staged_paths.insert(path.to_string(), Some(orig_path.clone()));
                        }
                        if paths.contains(&path.to_string())
                            && !paths_to_commit.contains(&orig_path)
                        {
                            paths_to_commit.push(orig_path);
                        }
                        i += 2;
                        continue;
                    }
                }
                i += 1;
            } else if part.starts_with(b"1 ") {
                let line = String::from_utf8_lossy(part);
                if let Some(path) = porcelain_v2_path(&line, 8) {
                    if line.as_bytes().get(2).copied().unwrap_or(b'.') != b'.' {
                        staged_paths.insert(path.to_string(), None);
                    }
                }
                i += 1;
            } else if part.starts_with(b"u ") {
                i += 1;
            } else {
                i += 1;
            }
        }

        // If any selected path has staged content, commit the index state for
        // that path. `git commit --only` would otherwise replace it with the
        // full working-tree content and silently include unstaged lines.
        let selected_staged: Vec<String> = paths
            .iter()
            .filter(|p| staged_paths.contains_key(*p))
            .cloned()
            .collect();
        if !selected_staged.is_empty() {
            let git_dir = get_real_git_dir(runner, repo_path).await?;
            return commit_using_temp_index(
                runner,
                repo_path,
                &git_dir,
                paths,
                &paths_to_commit,
                &staged_paths,
                summary,
                body,
            )
            .await;
        }

        // For untracked selected files, mark them intent-to-add literally so git commit --only recognises them
        if !untracked_in_paths.is_empty() {
            let mut add_args = vec!["add", "-N", "--"];
            for p in &untracked_in_paths {
                add_args.push(p);
            }
            let add_out = runner.run_write_literal(repo_path, &add_args, 20).await?;
            if !add_out.success() {
                return Err(AppError::other(add_out.stderr));
            }
        }

        // Commit using --only -- <paths_to_commit> with literal pathspecs
        let mut commit_args = vec!["commit", "-m", summary];
        let body_str;
        if let Some(b) = body {
            if !b.trim().is_empty() {
                body_str = b.to_string();
                commit_args.push("-m");
                commit_args.push(&body_str);
            }
        }
        commit_args.push("--only");
        commit_args.push("--");
        for p in &paths_to_commit {
            commit_args.push(p);
        }

        let commit_res = runner.run_write_literal(repo_path, &commit_args, 60).await;
        match commit_res {
            Ok(out) if out.success() => {}
            Ok(out) => {
                // Roll back intent-to-add if commit failed so index remains intact
                if !untracked_in_paths.is_empty() {
                    let mut reset_args = vec!["reset", "HEAD", "--"];
                    for p in &untracked_in_paths {
                        reset_args.push(p);
                    }
                    let _ = runner.run_write_literal(repo_path, &reset_args, 20).await;
                }
                return Err(AppError::other(out.stderr));
            }
            Err(e) => {
                if !untracked_in_paths.is_empty() {
                    let mut reset_args = vec!["reset", "HEAD", "--"];
                    for p in &untracked_in_paths {
                        reset_args.push(p);
                    }
                    let _ = runner.run_write_literal(repo_path, &reset_args, 20).await;
                }
                return Err(e);
            }
        }
    } else {
        // Initial / root commit (unborn branch) — safe temporary index transaction (R04)
        let git_dir = get_real_git_dir(runner, repo_path).await?;
        let temp_index_name = format!("index_temp_{}", uuid::Uuid::new_v4().simple());
        let temp_index_path = git_dir.join(&temp_index_name);
        let temp_index_str = temp_index_path.to_string_lossy().to_string();

        let mut add_args = vec!["add", "--"];
        for p in paths {
            add_args.push(p);
        }

        // Stage into the temporary index file only
        let add_out = runner
            .run_env(
                Some(repo_path),
                &add_args,
                20,
                true,
                &[
                    ("GIT_INDEX_FILE", &temp_index_str),
                    ("GIT_LITERAL_PATHSPECS", "1"),
                ],
            )
            .await?;

        if !add_out.success() {
            let _ = fs::remove_file(&temp_index_path);
            return Err(AppError::other(add_out.stderr));
        }

        // Write tree from the temporary index
        let write_tree_out = runner
            .run_env(
                Some(repo_path),
                &["write-tree"],
                20,
                true,
                &[("GIT_INDEX_FILE", &temp_index_str)],
            )
            .await?;

        let _ = fs::remove_file(&temp_index_path);

        if !write_tree_out.success() {
            return Err(AppError::other(write_tree_out.stderr));
        }

        let tree_sha = write_tree_out.stdout_string().trim().to_string();

        // Commit tree directly
        let mut commit_tree_args = vec!["commit-tree", &tree_sha, "-m", summary];
        let body_str;
        if let Some(b) = body {
            if !b.trim().is_empty() {
                body_str = b.to_string();
                commit_tree_args.push("-m");
                commit_tree_args.push(&body_str);
            }
        }

        let commit_tree_out = runner.run_write(repo_path, &commit_tree_args, 30).await?;

        if !commit_tree_out.success() {
            return Err(AppError::other(commit_tree_out.stderr));
        }

        let commit_sha = commit_tree_out.stdout_string().trim().to_string();

        // Point HEAD at the new commit
        let update_ref_out = runner
            .run_write(repo_path, &["update-ref", "HEAD", &commit_sha], 20)
            .await?;

        if !update_ref_out.success() {
            return Err(AppError::other(update_ref_out.stderr));
        }

        // Now mark the committed paths staged/clean in the primary index without touching unselected files
        let mut primary_add_args = vec!["add", "--"];
        for p in paths {
            primary_add_args.push(p);
        }
        let _ = runner
            .run_write_literal(repo_path, &primary_add_args, 20)
            .await;
    }

    // Return latest commit summary
    let log_out = runner
        .run_read(
            repo_path,
            &[
                "log",
                "-1",
                "--format=%H%x1f%an%x1f%ae%x1f%aI%x1f%P%x1f%s%x1f%b%x1e",
                "--shortstat",
                "HEAD",
            ],
        )
        .await?;

    let commits = crate::git::log::parse_log_output(
        &log_out.stdout_string(),
        &std::collections::HashSet::new(),
    );
    commits
        .into_iter()
        .next()
        .ok_or_else(|| AppError::other("Failed to get commit summary after commit"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartialAction {
    Stage,
    Unstage,
    Discard,
}

/// Partial operations only accept repository-relative paths. Reject absolute
/// paths and parent traversal so a malformed IPC call cannot touch files
/// outside the repository.
fn validate_relative_path(path: &str) -> Result<(), AppError> {
    let p = Path::new(path);
    if path.is_empty() || p.is_absolute() {
        return Err(AppError::other("Invalid file path for a partial operation"));
    }
    for component in p.components() {
        if matches!(component, std::path::Component::ParentDir) {
            return Err(AppError::other(
                "Invalid file path for a partial operation",
            ));
        }
    }
    Ok(())
}

async fn has_head(runner: &GitRunner, repo_path: &Path) -> bool {
    runner
        .run_read(repo_path, &["rev-parse", "--verify", "HEAD"])
        .await
        .map(|o| o.success())
        .unwrap_or(false)
}

async fn ensure_no_conflicts(runner: &GitRunner, repo_path: &Path) -> Result<(), AppError> {
    let out = runner
        .run_read(
            repo_path,
            &["status", "--porcelain=v2", "-z", "--untracked-files=all"],
        )
        .await?;
    if out
        .stdout
        .split(|b| *b == 0)
        .any(|part| part.starts_with(b"u "))
    {
        return Err(AppError::conflict(
            "Resolve the conflicted files before staging or discarding individual lines.",
        ));
    }
    Ok(())
}

async fn path_is_tracked(
    runner: &GitRunner,
    repo_path: &Path,
    path: &str,
) -> Result<bool, AppError> {
    let out = runner
        .run_read_literal(repo_path, &["ls-files", "--error-unmatch", "--", path])
        .await?;
    Ok(out.success())
}

/// Run the authoritative diff for the tree we are about to modify.
/// `staged` selects HEAD→index; otherwise index→worktree.
async fn partial_diff(
    runner: &GitRunner,
    repo_path: &Path,
    path: &str,
    staged: bool,
    ignore_whitespace: bool,
) -> Result<Vec<u8>, AppError> {
    let head = has_head(runner, repo_path).await;
    let mut args = vec!["diff"];
    if staged {
        args.push("--cached");
    }
    args.extend_from_slice(&["-U0", "-M", "--no-color", "--no-ext-diff"]);
    if ignore_whitespace {
        args.push("-w");
    }
    if staged && head {
        args.push("HEAD");
    }
    args.push("--");
    args.push(path);

    let out = runner.run_read_literal(repo_path, &args).await?;
    if !out.success() {
        return Err(AppError::other(out.stderr));
    }
    if !out.stdout.is_empty() || staged {
        return Ok(out.stdout);
    }

    // An untracked file has no index entry, so `git diff` is empty. Show it as
    // a new-file diff. Do not do this for tracked files that simply have no
    // unstaged changes (that would fake an entire-file addition).
    let full_path = repo_path.join(path);
    if full_path.exists() && !path_is_tracked(runner, repo_path, path).await? {
        let mut no_index_args = vec![
            "diff",
            "--no-index",
            "-U0",
            "--no-color",
            "--no-ext-diff",
        ];
        if ignore_whitespace {
            no_index_args.push("-w");
        }
        no_index_args.push("--");
        no_index_args.push("/dev/null");
        no_index_args.push(path);
        let no_index = runner.run_read(repo_path, &no_index_args).await?;
        // `git diff --no-index` exits with 1 when files differ; stdout is the
        // patch we want.
        if !no_index.stdout.is_empty() {
            return Ok(no_index.stdout);
        }
    }

    Ok(Vec::new())
}

async fn apply_patch_bytes(
    runner: &GitRunner,
    repo_path: &Path,
    patch: &[u8],
    reverse: bool,
    cached: bool,
) -> Result<(), AppError> {
    let git_dir = get_real_git_dir(runner, repo_path).await?;
    let patch_path = git_dir.join(format!(
        "vantage-patch-{}.patch",
        uuid::Uuid::new_v4().simple()
    ));
    fs::write(&patch_path, patch)?;
    let patch_arg = patch_path.to_string_lossy().to_string();

    let build_args = |check: bool| {
        let mut args = vec!["apply", "--unidiff-zero", "--whitespace=nowarn"];
        if cached {
            args.push("--cached");
        }
        if reverse {
            args.push("--reverse");
        }
        if check {
            args.push("--check");
        }
        args.push(&patch_arg);
        args
    };

    // Check first so we can give a precise, reassuring error. `git apply`
    // itself is all-or-nothing, so no partial application is possible.
    let check_args = build_args(true);
    let check = runner.run_write(repo_path, &check_args, 30).await;
    let check = match check {
        Ok(out) => out,
        Err(e) => {
            let _ = fs::remove_file(&patch_path);
            return Err(e);
        }
    };
    if !check.success() {
        let _ = fs::remove_file(&patch_path);
        return Err(AppError::other(format!(
            "Cannot apply the selected lines because the index or working tree changed since the diff was shown. Nothing was changed. {}",
            check.stderr.trim()
        )));
    }

    let apply_args = build_args(false);
    let applied = runner.run_write(repo_path, &apply_args, 30).await;
    let _ = fs::remove_file(&patch_path);
    let applied = applied?;
    if !applied.success() {
        return Err(AppError::other(format!(
            "Git could not apply the selected lines. Nothing was changed. {}",
            applied.stderr.trim()
        )));
    }
    Ok(())
}

/// Snapshot the file before a line-level discard. This mirrors the file-level
/// discard backup: tracked changes go into a stash referenced by
/// `refs/vantage/backup/*`, untracked (or root-commit) files are copied into
/// `.git/vantage-trash/<id>` with a manifest.
async fn create_line_discard_backup(
    runner: &GitRunner,
    repo_path: &Path,
    path: &str,
    line_count: usize,
) -> Result<(BackupRecord, String), AppError> {
    let backup_id = format!(
        "{}_{}",
        Utc::now().timestamp_millis(),
        uuid::Uuid::new_v4().simple()
    );
    let backup_ref = format!("refs/vantage/backup/{}", backup_id);

    let status_out = runner
        .run_read(
            repo_path,
            &["status", "--porcelain=v2", "-z", "--untracked-files=all"],
        )
        .await?;
    let is_untracked = status_out
        .stdout
        .split(|b| *b == 0)
        .any(|part| part.starts_with(b"? ") && String::from_utf8_lossy(&part[2..]) == path);

    let head_exists = has_head(runner, repo_path).await;
    let mut stash_sha = None;
    let mut untracked_dir = None;

    if !is_untracked && head_exists {
        let stash_out = runner
            .run_write(repo_path, &["stash", "create"], 30)
            .await?;
        let sha = stash_out.stdout_string().trim().to_string();
        if sha.is_empty() {
            return Err(AppError::other(
                "Could not create a backup before discarding lines. Nothing was changed.",
            ));
        }
        let update = runner
            .run_write(repo_path, &["update-ref", &backup_ref, &sha], 20)
            .await?;
        if !update.success() {
            return Err(AppError::other(
                "Could not record the discard backup. Nothing was changed.",
            ));
        }
        stash_sha = Some(sha);
    } else {
        let git_dir = get_real_git_dir(runner, repo_path).await?;
        let trash_dir = git_dir.join("vantage-trash").join(&backup_id);
        let dest = trash_dir.join("files").join(path);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(AppError::io)?;
        }
        let src = repo_path.join(path);
        fs::copy(&src, &dest).map_err(AppError::io)?;
        let manifest = serde_json::to_string_pretty(&vec![path.to_string()])?;
        fs::write(trash_dir.join("manifest.json"), manifest).map_err(AppError::io)?;
        untracked_dir = Some(backup_id.clone());
    }

    let record = BackupRecord {
        id: backup_id.clone(),
        kind: "discard".to_string(),
        head_sha: None,
        stash_sha,
        paths: vec![path.to_string()],
        untracked_dir,
        created_at: Utc::now().to_rfc3339(),
        description: format!(
            "Discarded {} line{} from {}",
            line_count,
            if line_count == 1 { "" } else { "s" },
            path
        ),
    };

    save_backup_record(runner, repo_path, &record).await?;

    Ok((record, backup_ref))
}

async fn remove_line_discard_backup(
    runner: &GitRunner,
    repo_path: &Path,
    record: &BackupRecord,
    backup_ref: &str,
) {
    let _ = runner
        .run_write(repo_path, &["update-ref", "-d", backup_ref], 10)
        .await;
    if let Ok(git_dir) = get_real_git_dir(runner, repo_path).await {
        let _ = fs::remove_file(
            git_dir
                .join("vantage-backups")
                .join(format!("{}.json", record.id)),
        );
        if let Some(trash_id) = &record.untracked_dir {
            let _ = fs::remove_dir_all(git_dir.join("vantage-trash").join(trash_id));
        }
    }
}

/// Stage, unstage, or discard selected lines in one file.
///
/// The entire operation runs under the repository write lock. The patch is
/// synthesized from a freshly generated diff, checked with `git apply
/// --check`, and then applied atomically. On any failure nothing is modified.
pub async fn partial_lines(
    runner: &GitRunner,
    repo_path: &Path,
    path: &str,
    selection: &[SelectedLine],
    action: PartialAction,
    ignore_whitespace: bool,
) -> Result<Option<BackupInfo>, AppError> {
    validate_relative_path(path)?;
    if selection.is_empty() {
        return Err(AppError::other("No lines selected"));
    }

    let repo_lock = runner.get_repo_lock(repo_path).await;
    let _repo_guard = repo_lock.lock().await;

    ensure_no_conflicts(runner, repo_path).await?;

    let staged_source = action == PartialAction::Unstage;
    let raw = partial_diff(runner, repo_path, path, staged_source, ignore_whitespace).await?;
    if raw.is_empty() {
        return Err(AppError::other(
            "No changes for this file in the selected view. Refresh the diff and try again.",
        ));
    }

    let target = match action {
        PartialAction::Stage => PatchTarget::Stage,
        PartialAction::Unstage => PatchTarget::Unstage,
        PartialAction::Discard => PatchTarget::Discard,
    };
    let synthesized = synthesize(&raw, selection, target)?;

    match synthesized {
        Synthesized::ResetIndex => {
            let head = has_head(runner, repo_path).await;
            let args = if head {
                vec!["reset", "-q", "HEAD", "--", path]
            } else {
                vec!["rm", "-q", "--cached", "--", path]
            };
            let out = runner.run_write_literal(repo_path, &args, 20).await?;
            if !out.success() {
                return Err(AppError::other(out.stderr));
            }
            return Ok(None);
        }
        Synthesized::AddFile(patch) => {
            // Creating a partial index entry cannot be done by reversing a
            // deletion patch, so this synthesized new-file patch is applied
            // forward.
            apply_patch_bytes(runner, repo_path, &patch, false, true).await?;
            Ok(None)
        }
        Synthesized::Patch(patch) => {
            if action != PartialAction::Discard {
                // Unstage applies the HEAD→index patch in reverse so exactly
                // the selected lines leave the index.
                let reverse = action == PartialAction::Unstage;
                apply_patch_bytes(runner, repo_path, &patch, reverse, true).await?;
                return Ok(None);
            }

            let line_count = selection
                .iter()
                .filter(|s| {
                    s.kind == crate::state::LineKind::Add
                        || s.kind == crate::state::LineKind::Del
                })
                .count();

            // Validate that the worktree patch applies before creating a
            // backup, then apply it. The backup is rolled back if Git refuses
            // the patch after the check (e.g. an external writer won the race).
            let git_dir = get_real_git_dir(runner, repo_path).await?;
            let patch_path = git_dir.join(format!(
                "vantage-patch-{}.patch",
                uuid::Uuid::new_v4().simple()
            ));
            fs::write(&patch_path, &patch)?;
            let patch_arg = patch_path.to_string_lossy().to_string();
            let check_args = vec![
                "apply",
                "--unidiff-zero",
                "--whitespace=nowarn",
                "--reverse",
                "--check",
                patch_arg.as_str(),
            ];
            let check = runner.run_write(repo_path, &check_args, 30).await;
            let _ = fs::remove_file(&patch_path);
            let check = check?;
            if !check.success() {
                return Err(AppError::other(format!(
                    "Cannot discard the selected lines because the file changed since the diff was shown. Nothing was changed. {}",
                    check.stderr.trim()
                )));
            }

            let (record, backup_ref) =
                create_line_discard_backup(runner, repo_path, path, line_count).await?;

            if let Err(e) = apply_patch_bytes(runner, repo_path, &patch, true, false).await {
                remove_line_discard_backup(runner, repo_path, &record, &backup_ref).await;
                return Err(e);
            }

            Ok(Some(BackupInfo {
                r#ref: backup_ref,
                created_at: record.created_at,
                kind: record.kind,
                description: record.description,
            }))
        }
    }
}

pub async fn discard(
    runner: &GitRunner,
    repo_path: &Path,
    paths: &[String],
) -> Result<BackupInfo, AppError> {
    if paths.is_empty() {
        return Err(AppError::other("No files selected for discard"));
    }

    let repo_lock = runner.get_repo_lock(repo_path).await;
    let _repo_guard = repo_lock.lock().await;

    let backup_id = format!(
        "{}_{}",
        Utc::now().timestamp_millis(),
        uuid::Uuid::new_v4().simple()
    );
    let backup_ref = format!("refs/vantage/backup/{}", backup_id);

    // 1. Separate tracked vs untracked using porcelain v2 with untracked-files=all (R22)
    let mut tracked = Vec::new();
    let mut untracked = Vec::new();

    let status_out = runner
        .run_read(
            repo_path,
            &["status", "--porcelain=v2", "-z", "--untracked-files=all"],
        )
        .await?;

    let parts: Vec<&[u8]> = status_out.stdout.split(|&b| b == 0).collect();
    for p in paths {
        let mut is_untracked = false;
        for part in &parts {
            if part.starts_with(b"? ") {
                let un_path = String::from_utf8_lossy(&part[2..]);
                if un_path == *p {
                    is_untracked = true;
                    break;
                }
            }
        }
        if is_untracked {
            untracked.push(p.clone());
        } else {
            tracked.push(p.clone());
        }
    }

    // 2. If there are tracked changes, create stash snapshot of tracked state
    let mut stash_sha_opt = None;
    if !tracked.is_empty() {
        let stash_out = runner
            .run_write(repo_path, &["stash", "create"], 30)
            .await?;
        let sha = stash_out.stdout_string().trim().to_string();
        if !sha.is_empty() {
            let update_res = runner
                .run_write(repo_path, &["update-ref", &backup_ref, &sha], 20)
                .await?;
            if !update_res.success() {
                return Err(AppError::other("Failed to record discard backup ref"));
            }
            stash_sha_opt = Some(sha);
        }

        // Discard tracked paths literally (R02)
        let mut checkout_args = vec!["checkout", "HEAD", "--"];
        for t in &tracked {
            checkout_args.push(t.as_str());
        }
        let out = runner
            .run_write_literal(repo_path, &checkout_args, 20)
            .await?;
        if !out.success() {
            return Err(AppError::other(out.stderr));
        }
    }

    // 3. Move untracked files to <real_git_dir>/relay-trash/<backup_id> (R09)
    let mut untracked_dir_opt = None;
    if !untracked.is_empty() {
        let git_dir = get_real_git_dir(runner, repo_path).await?;
        let trash_dir = git_dir.join("vantage-trash").join(&backup_id);
        let trash_files_dir = trash_dir.join("files");

        for u in &untracked {
            let src = repo_path.join(u);
            let dest = trash_files_dir.join(u);
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).map_err(AppError::io)?;
            }
            if fs::rename(&src, &dest).is_err() {
                // If rename fails (e.g. cross-device), fallback to copy and delete
                fs::copy(&src, &dest).map_err(AppError::io)?;
                let _ = fs::remove_file(&src);
            }
        }

        let manifest_file = trash_dir.join("manifest.json");
        let manifest_json = serde_json::to_string_pretty(&untracked)?;
        fs::write(&manifest_file, manifest_json).map_err(AppError::io)?;

        untracked_dir_opt = Some(backup_id.clone());

        // Even for untracked-only discard, record an empty tree or head ref so ref lookup works
        if stash_sha_opt.is_none() {
            let head_out = runner.run_read(repo_path, &["rev-parse", "HEAD"]).await;
            if let Ok(ho) = head_out {
                let h_sha = ho.stdout_string().trim().to_string();
                if !h_sha.is_empty() {
                    let _ = runner
                        .run_write(repo_path, &["update-ref", &backup_ref, &h_sha], 20)
                        .await;
                }
            }
        }
    }

    let record = BackupRecord {
        id: backup_id.clone(),
        kind: "discard".to_string(),
        head_sha: None,
        stash_sha: stash_sha_opt,
        paths: paths.to_vec(),
        untracked_dir: untracked_dir_opt,
        created_at: Utc::now().to_rfc3339(),
        description: format!("Discarded {} files", paths.len()),
    };

    save_backup_record(runner, repo_path, &record).await?;

    Ok(BackupInfo {
        r#ref: backup_ref,
        created_at: record.created_at,
        kind: "discard".to_string(),
        description: record.description,
    })
}

async fn revert_locked(
    runner: &GitRunner,
    repo_path: &Path,
    sha: &str,
) -> Result<CommitSummary, AppError> {
    // Check if working tree is clean
    let status_out = runner
        .run_read(
            repo_path,
            &["status", "--porcelain=v2", "-z", "--untracked-files=all"],
        )
        .await?;
    if !status_out.stdout.is_empty() {
        return Err(AppError::dirty(
            "Working tree is dirty. Commit or discard your changes first before reverting.",
        ));
    }

    // Check if merge commit (has multiple parents)
    let parent_out = runner
        .run_read(repo_path, &["rev-parse", &format!("{}^@", sha)])
        .await?;
    let parents = parent_out.stdout_string();
    let is_merge = parents.lines().count() > 1;

    let mut args = vec!["revert", "--no-edit"];
    if is_merge {
        args.push("-m");
        args.push("1");
    }
    args.push(sha);

    let out = runner.run_write(repo_path, &args, 30).await?;
    if !out.success() {
        let _ = runner
            .run_write(repo_path, &["revert", "--abort"], 10)
            .await;
        return Err(AppError::conflict(format!(
            "Revert failed with conflicts (revert was aborted): {}",
            out.stderr
        )));
    }

    // Return latest commit summary
    let log_out = runner
        .run_read(
            repo_path,
            &[
                "log",
                "-1",
                "--format=%H%x1f%an%x1f%ae%x1f%aI%x1f%P%x1f%s%x1f%b%x1e",
                "--shortstat",
                "HEAD",
            ],
        )
        .await?;

    let commits = crate::git::log::parse_log_output(
        &log_out.stdout_string(),
        &std::collections::HashSet::new(),
    );
    commits
        .into_iter()
        .next()
        .ok_or_else(|| AppError::other("Failed to get commit summary after revert"))
}

pub async fn revert(
    runner: &GitRunner,
    repo_path: &Path,
    sha: &str,
) -> Result<CommitSummary, AppError> {
    let repo_lock = runner.get_repo_lock(repo_path).await;
    let _repo_guard = repo_lock.lock().await;
    revert_locked(runner, repo_path, sha).await
}

pub async fn revert_range(
    runner: &GitRunner,
    repo_path: &Path,
    from: &str,
    to: &str,
) -> Result<Vec<CommitSummary>, AppError> {
    let repo_lock = runner.get_repo_lock(repo_path).await;
    let _repo_guard = repo_lock.lock().await;

    let rev_out = runner
        .run_read(repo_path, &["rev-list", &format!("{}..{}", from, to)])
        .await?;
    if !rev_out.success() {
        return Err(AppError::other(rev_out.stderr));
    }

    let shas: Vec<String> = rev_out
        .stdout_string()
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let mut reverted = Vec::new();
    for sha in &shas {
        let c = revert_locked(runner, repo_path, sha).await?;
        reverted.push(c);
    }

    Ok(reverted)
}

pub async fn check_pushed(
    runner: &GitRunner,
    repo_path: &Path,
    sha: &str,
) -> Result<bool, AppError> {
    let out = runner
        .run_read(repo_path, &["branch", "-r", "--contains", sha])
        .await?;
    if !out.success() {
        return Ok(false);
    }
    let stdout = out.stdout_string();
    let is_pushed = stdout.lines().any(|l| !l.trim().is_empty());
    Ok(is_pushed)
}

pub async fn reset_to(
    runner: &GitRunner,
    repo_path: &Path,
    target_sha: &str,
    also_discard_dirty: bool,
) -> Result<BackupInfo, AppError> {
    let repo_lock = runner.get_repo_lock(repo_path).await;
    let _repo_guard = repo_lock.lock().await;

    // Check dirty status under the lock (R06)
    let status_out = runner
        .run_read(
            repo_path,
            &["status", "--porcelain=v2", "-z", "--untracked-files=all"],
        )
        .await?;
    let is_dirty = !status_out.stdout.is_empty();

    if is_dirty && !also_discard_dirty {
        return Err(AppError::dirty(
            "Working tree has uncommitted changes. Commit or discard your changes first before resetting.",
        ));
    }

    let backup_id = format!(
        "{}_{}",
        Utc::now().timestamp_millis(),
        uuid::Uuid::new_v4().simple()
    );
    let backup_ref = format!("refs/vantage/backup/{}-reset", backup_id);

    // 1. Save backup ref of HEAD
    let head_out = runner.run_read(repo_path, &["rev-parse", "HEAD"]).await?;
    if !head_out.success() {
        return Err(AppError::other("Failed to resolve HEAD before reset"));
    }
    let head_sha = head_out.stdout_string().trim().to_string();

    let update_head_res = runner
        .run_write(repo_path, &["update-ref", &backup_ref, &head_sha], 20)
        .await?;
    if !update_head_res.success() {
        return Err(AppError::other("Failed to create backup ref of HEAD"));
    }

    // 2. If dirty changes should also be backed up
    let mut stash_sha_opt = None;
    if is_dirty && also_discard_dirty {
        let stash_out = runner
            .run_write(repo_path, &["stash", "create"], 30)
            .await?;
        if !stash_out.success() {
            return Err(AppError::other(
                "Failed to create stash backup of dirty state",
            ));
        }
        let stash_sha = stash_out.stdout_string().trim().to_string();
        if !stash_sha.is_empty() {
            let dirty_ref = format!("{}-dirty", backup_ref);
            let update_dirty_res = runner
                .run_write(repo_path, &["update-ref", &dirty_ref, &stash_sha], 20)
                .await?;
            if !update_dirty_res.success() {
                return Err(AppError::other("Failed to save dirty backup ref"));
            }
            stash_sha_opt = Some(stash_sha);
        }
    }

    // 3. Reset hard to target_sha
    let out = runner
        .run_write(repo_path, &["reset", "--hard", target_sha], 30)
        .await?;

    if !out.success() {
        return Err(AppError::other(out.stderr));
    }

    let record = BackupRecord {
        id: format!("{}-reset", backup_id),
        kind: "reset".to_string(),
        head_sha: Some(head_sha),
        stash_sha: stash_sha_opt,
        paths: Vec::new(),
        untracked_dir: None,
        created_at: Utc::now().to_rfc3339(),
        description: format!("Reset to {}", target_sha),
    };

    save_backup_record(runner, repo_path, &record).await?;

    Ok(BackupInfo {
        r#ref: backup_ref,
        created_at: record.created_at,
        kind: "reset".to_string(),
        description: record.description,
    })
}

pub async fn list_backups(
    runner: &GitRunner,
    repo_path: &Path,
) -> Result<Vec<BackupInfo>, AppError> {
    let out = runner
        .run_read(
            repo_path,
            &[
                "for-each-ref",
                "--format=%(refname)\x1f%(creatordate:iso-strict)",
                "refs/vantage/backup",
                "refs/relay/backup",
            ],
        )
        .await?;

    let mut backups = Vec::new();
    for line in out.stdout_string().lines() {
        let parts: Vec<&str> = line.split('\x1f').collect();
        if parts.len() >= 2 {
            let r_ref = parts[0].trim().to_string();
            let created_at = parts[1].trim().to_string();

            // Try to read metadata if available
            if let Some(record) = load_backup_record(runner, repo_path, &r_ref).await {
                backups.push(BackupInfo {
                    r#ref: r_ref,
                    created_at: record.created_at,
                    kind: record.kind,
                    description: record.description,
                });
            } else {
                let kind = if r_ref.contains("reset") {
                    "reset"
                } else if r_ref.contains("discard") {
                    "discard"
                } else {
                    "backup"
                };

                backups.push(BackupInfo {
                    r#ref: r_ref,
                    created_at,
                    kind: kind.to_string(),
                    description: format!("Backup: {}", kind),
                });
            }
        }
    }

    // Also include any untracked-only discard backups that may only exist as records
    if let Ok(git_dir) = get_real_git_dir(runner, repo_path).await {
        for dir_name in &["vantage-backups", "relay-backups"] {
            let backups_dir = git_dir.join(dir_name);
            if let Ok(entries) = fs::read_dir(backups_dir) {
                for entry in entries.flatten() {
                    if let Ok(content) = fs::read_to_string(entry.path()) {
                        if let Ok(record) = serde_json::from_str::<BackupRecord>(&content) {
                            let expected_ref = format!("refs/vantage/backup/{}", record.id);
                            let legacy_ref = format!("refs/relay/backup/{}", record.id);
                            if !backups
                                .iter()
                                .any(|b| b.r#ref == expected_ref || b.r#ref == legacy_ref)
                            {
                                backups.push(BackupInfo {
                                    r#ref: expected_ref,
                                    created_at: record.created_at,
                                    kind: record.kind,
                                    description: record.description,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    backups.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(backups)
}

pub async fn restore_backup(
    runner: &GitRunner,
    repo_path: &Path,
    backup_ref: &str,
) -> Result<(), AppError> {
    let repo_lock = runner.get_repo_lock(repo_path).await;
    let _repo_guard = repo_lock.lock().await;

    let maybe_record = load_backup_record(runner, repo_path, backup_ref).await;

    if let Some(record) = maybe_record {
        if record.kind == "reset" {
            // Restore HEAD commit (R07)
            if let Some(ref head) = record.head_sha {
                let out = runner
                    .run_write(repo_path, &["reset", "--hard", head], 30)
                    .await?;
                if !out.success() {
                    return Err(AppError::other(out.stderr));
                }
            }

            // Restore dirty state using stash apply --index so no stash merge commit is added to history!
            if let Some(ref stash) = record.stash_sha {
                let out = runner
                    .run_write(repo_path, &["stash", "apply", "--index", stash], 30)
                    .await?;
                if !out.success() {
                    // Try fallback without --index if index merge conflicted
                    let _ = runner
                        .run_write(repo_path, &["stash", "apply", stash], 30)
                        .await;
                }
            }
        } else if record.kind == "discard" {
            // Restore ONLY the specific tracked files that were discarded (R08)
            if let Some(ref stash) = record.stash_sha {
                if !record.paths.is_empty() {
                    let mut checkout_args = vec!["checkout", stash.as_str(), "--"];
                    for p in &record.paths {
                        checkout_args.push(p.as_str());
                    }
                    let out = runner
                        .run_write_literal(repo_path, &checkout_args, 30)
                        .await?;
                    if !out.success() {
                        return Err(AppError::other(out.stderr));
                    }
                }
            }

            // Restore untracked files from trash (R09)
            if let Some(ref trash_id) = record.untracked_dir {
                let git_dir = get_real_git_dir(runner, repo_path).await?;
                let mut trash_dir = git_dir.join("vantage-trash").join(trash_id);
                if !trash_dir.exists() {
                    let legacy_trash = git_dir.join("relay-trash").join(trash_id);
                    if legacy_trash.exists() {
                        trash_dir = legacy_trash;
                    }
                }
                let manifest_file = trash_dir.join("manifest.json");
                if manifest_file.exists() {
                    if let Ok(manifest_content) = fs::read_to_string(&manifest_file) {
                        if let Ok(untracked_paths) =
                            serde_json::from_str::<Vec<String>>(&manifest_content)
                        {
                            let trash_files_dir = trash_dir.join("files");
                            for u in untracked_paths {
                                let src = trash_files_dir.join(&u);
                                let dest = repo_path.join(&u);
                                if src.exists() {
                                    if let Some(parent) = dest.parent() {
                                        let _ = fs::create_dir_all(parent);
                                    }
                                    let _ = fs::rename(&src, &dest).or_else(|_| {
                                        fs::copy(&src, &dest).and_then(|_| fs::remove_file(&src))
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        return Ok(());
    }

    // Fallback for raw refs without metadata records
    if backup_ref.ends_with("-dirty") {
        let out = runner
            .run_write(repo_path, &["stash", "apply", "--index", backup_ref], 30)
            .await?;
        if !out.success() {
            return Err(AppError::other(out.stderr));
        }
    } else if backup_ref.contains("reset") {
        let out = runner
            .run_write(repo_path, &["reset", "--hard", backup_ref], 30)
            .await?;
        if !out.success() {
            return Err(AppError::other(out.stderr));
        }
    } else if backup_ref.contains("discard") {
        let out = runner
            .run_write(repo_path, &["stash", "apply", backup_ref], 30)
            .await?;
        if !out.success() {
            return Err(AppError::other(out.stderr));
        }
    }
    Ok(())
}

pub async fn prune_old_backups(runner: &GitRunner, repo_path: &Path) {
    let thirty_days_ago = Utc::now().timestamp() - (30 * 24 * 3600);
    if let Ok(backups) = list_backups(runner, repo_path).await {
        for b in backups {
            if let Some(record) = load_backup_record(runner, repo_path, &b.r#ref).await {
                if let Ok(created) = chrono::DateTime::parse_from_rfc3339(&record.created_at) {
                    if created.timestamp() < thirty_days_ago {
                        let _ = runner
                            .run_write(repo_path, &["update-ref", "-d", &b.r#ref], 10)
                            .await;
                    }
                }
            } else if let Some(ts_str) = b
                .r#ref
                .strip_prefix("refs/vantage/backup/")
                .or_else(|| b.r#ref.strip_prefix("refs/relay/backup/"))
            {
                if let Some(ts_part) = ts_str.split(['_', '-']).next() {
                    if let Ok(ts) = ts_part.parse::<i64>() {
                        let ts_sec = if ts > 1_000_000_000_000 {
                            ts / 1000
                        } else {
                            ts
                        };
                        if ts_sec < thirty_days_ago {
                            let _ = runner
                                .run_write(repo_path, &["update-ref", "-d", &b.r#ref], 10)
                                .await;
                        }
                    }
                }
            }
        }
    }
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
            let path = std::env::temp_dir().join(format!("vantage_test_{}", uuid::Uuid::new_v4()));
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
    }

    impl Drop for TempTestRepo {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[tokio::test]
    async fn test_selective_root_commit_preserves_pre_staged_files() {
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;

        // No HEAD exists yet (root commit)
        // 1. Pre-stage secret.txt into the index
        let secret_file = repo.path.join("secret.txt");
        fs::write(&secret_file, "secret credentials").unwrap();
        runner
            .run_write(&repo.path, &["add", "secret.txt"], 10)
            .await
            .unwrap();

        // 2. Create chosen.txt
        let chosen_file = repo.path.join("chosen.txt");
        fs::write(&chosen_file, "public content").unwrap();

        // 3. Commit ONLY chosen.txt
        let commit_res = commit(
            &runner,
            &repo.path,
            &["chosen.txt".to_string()],
            "root commit with chosen file only",
            None,
        )
        .await
        .expect("selective root commit must succeed");

        assert_eq!(commit_res.subject, "root commit with chosen file only");

        // 4. Verify that secret.txt was NOT committed into HEAD
        let show_out = runner
            .run_read(&repo.path, &["show", "--name-only", "--format=", "HEAD"])
            .await
            .unwrap();
        let show_str = show_out.stdout_string();
        assert!(
            show_str.contains("chosen.txt"),
            "HEAD must contain chosen.txt"
        );
        assert!(
            !show_str.contains("secret.txt"),
            "HEAD must NOT contain secret.txt"
        );

        // 5. Verify secret.txt is STILL staged in index
        let status_out = runner
            .run_read(&repo.path, &["status", "--porcelain"])
            .await
            .unwrap();
        let status_str = status_out.stdout_string();
        assert!(
            status_str.contains("A  secret.txt"),
            "secret.txt must still be staged in index"
        );
    }

    #[tokio::test]
    async fn test_discard_literal_wildcard_filenames() {
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;

        // Create initial commit
        let other_file = repo.path.join("other.txt");
        fs::write(&other_file, "original other\n").unwrap();
        let wildcard_file = repo.path.join("*.txt");
        fs::write(&wildcard_file, "original wildcard\n").unwrap();

        runner
            .run_write(&repo.path, &["add", "other.txt", "*.txt"], 10)
            .await
            .unwrap();
        runner
            .run_write(&repo.path, &["commit", "-m", "init"], 10)
            .await
            .unwrap();

        // Dirty both files
        fs::write(&other_file, "modified other\n").unwrap();
        fs::write(&wildcard_file, "modified wildcard\n").unwrap();

        // Discard ONLY the file named literally "*.txt"
        let backup = discard(&runner, &repo.path, &["*.txt".to_string()])
            .await
            .expect("literal discard must succeed");

        assert_eq!(backup.kind, "discard");

        // Verify: other.txt is STILL modified, while *.txt was discarded back to original
        let other_content = fs::read_to_string(&other_file).unwrap();
        assert_eq!(
            other_content, "modified other\n",
            "other.txt must NOT be discarded"
        );

        let wildcard_content = fs::read_to_string(&wildcard_file).unwrap();
        assert_eq!(
            wildcard_content, "original wildcard\n",
            "*.txt must be discarded"
        );
    }

    #[tokio::test]
    async fn test_partial_discard_undo_preserves_unselected_dirty_files() {
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;

        let a_file = repo.path.join("a.txt");
        let b_file = repo.path.join("b.txt");
        fs::write(&a_file, "initial a\n").unwrap();
        fs::write(&b_file, "initial b\n").unwrap();
        runner
            .run_write(&repo.path, &["add", "a.txt", "b.txt"], 10)
            .await
            .unwrap();
        runner
            .run_write(&repo.path, &["commit", "-m", "init"], 10)
            .await
            .unwrap();

        // Edit both a and b
        fs::write(&a_file, "modified a\n").unwrap();
        fs::write(&b_file, "modified b\n").unwrap();

        // Discard ONLY a.txt
        let backup = discard(&runner, &repo.path, &["a.txt".to_string()])
            .await
            .unwrap();

        assert_eq!(fs::read_to_string(&a_file).unwrap(), "initial a\n");
        assert_eq!(fs::read_to_string(&b_file).unwrap(), "modified b\n");

        // Undo discard of a.txt
        restore_backup(&runner, &repo.path, &backup.r#ref)
            .await
            .expect("Undo of partial discard must succeed without overwriting b.txt");

        assert_eq!(fs::read_to_string(&a_file).unwrap(), "modified a\n");
        assert_eq!(fs::read_to_string(&b_file).unwrap(), "modified b\n");
    }

    #[tokio::test]
    async fn test_untracked_discard_and_undo() {
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;

        let init_file = repo.path.join("init.txt");
        fs::write(&init_file, "init").unwrap();
        runner
            .run_write(&repo.path, &["add", "init.txt"], 10)
            .await
            .unwrap();
        runner
            .run_write(&repo.path, &["commit", "-m", "init"], 10)
            .await
            .unwrap();

        // Create untracked file inside subdirectory
        let sub_file = repo.path.join("sub").join("untracked.txt");
        fs::create_dir_all(sub_file.parent().unwrap()).unwrap();
        fs::write(&sub_file, "untracked content\n").unwrap();

        let backup = discard(&runner, &repo.path, &["sub/untracked.txt".to_string()])
            .await
            .expect("discarding untracked file must succeed");

        assert!(
            !sub_file.exists(),
            "sub/untracked.txt must be removed from working tree"
        );

        // Restore backup
        restore_backup(&runner, &repo.path, &backup.r#ref)
            .await
            .expect("Restoring untracked backup must succeed");

        assert!(sub_file.exists(), "sub/untracked.txt must be restored");
        assert_eq!(
            fs::read_to_string(&sub_file).unwrap(),
            "untracked content\n"
        );
    }

    #[tokio::test]
    async fn test_reset_dirty_undo_restores_head_and_dirty_state() {
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;

        let f = repo.path.join("file.txt");
        fs::write(&f, "v1\n").unwrap();
        runner
            .run_write(&repo.path, &["add", "file.txt"], 10)
            .await
            .unwrap();
        runner
            .run_write(&repo.path, &["commit", "-m", "c1"], 10)
            .await
            .unwrap();
        let c1_sha = runner
            .run_read(&repo.path, &["rev-parse", "HEAD"])
            .await
            .unwrap()
            .stdout_string()
            .trim()
            .to_string();

        fs::write(&f, "v2\n").unwrap();
        runner
            .run_write(&repo.path, &["commit", "-am", "c2"], 10)
            .await
            .unwrap();

        // Create dirty work on top of c2
        fs::write(&f, "v2 dirty work\n").unwrap();

        // Reset to c1 with also_discard_dirty = true
        let backup = reset_to(&runner, &repo.path, &c1_sha, true).await.unwrap();
        assert_eq!(fs::read_to_string(&f).unwrap(), "v1\n");

        // Restore backup
        restore_backup(&runner, &repo.path, &backup.r#ref)
            .await
            .unwrap();

        // Check HEAD is restored to c2
        let restored_head = runner
            .run_read(&repo.path, &["rev-parse", "HEAD"])
            .await
            .unwrap()
            .stdout_string()
            .trim()
            .to_string();
        let c2_sha = runner
            .run_read(&repo.path, &["log", "-1", "--format=%H", "HEAD"])
            .await
            .unwrap()
            .stdout_string()
            .trim()
            .to_string();
        assert_eq!(restored_head, c2_sha);

        // Check dirty state is restored
        assert_eq!(fs::read_to_string(&f).unwrap(), "v2 dirty work\n");
    }

    #[tokio::test]
    async fn test_selective_commit_preserves_pre_staged_files() {
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;

        let init_file = repo.path.join("init.txt");
        fs::write(&init_file, "initial").unwrap();
        runner
            .run_write(&repo.path, &["add", "init.txt"], 10)
            .await
            .unwrap();
        runner
            .run_write(&repo.path, &["commit", "-m", "init"], 10)
            .await
            .unwrap();

        let other_staged = repo.path.join("other_staged.txt");
        fs::write(&other_staged, "pre-staged content").unwrap();
        runner
            .run_write(&repo.path, &["add", "other_staged.txt"], 10)
            .await
            .unwrap();

        let chosen_file = repo.path.join("chosen_file.txt");
        fs::write(&chosen_file, "selected content").unwrap();

        let commit_res = commit(
            &runner,
            &repo.path,
            &["chosen_file.txt".to_string()],
            "commit selected file only",
            None,
        )
        .await
        .expect("selective commit should succeed");

        assert_eq!(commit_res.subject, "commit selected file only");

        let status_out = runner
            .run_read(&repo.path, &["status", "--porcelain"])
            .await
            .unwrap();
        let status_str = status_out.stdout_string();
        assert!(status_str.contains("A  other_staged.txt"));
        assert!(!status_str.contains("chosen_file.txt"));
    }

    fn line_sel(
        kind: crate::state::LineKind,
        old_no: Option<u32>,
        new_no: Option<u32>,
        text: &str,
    ) -> SelectedLine {
        SelectedLine {
            kind,
            old_no,
            new_no,
            text: text.to_string(),
        }
    }

    #[tokio::test]
    async fn test_partial_stage_only_selected_lines() {
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;
        let f = repo.path.join("f.txt");
        fs::write(&f, "a\nb\nc\nd\ne\n").unwrap();
        runner
            .run_write(&repo.path, &["add", "f.txt"], 10)
            .await
            .unwrap();
        runner
            .run_write(&repo.path, &["commit", "-m", "init"], 10)
            .await
            .unwrap();

        fs::write(&f, "a\nB\nc\nd\nE\n").unwrap();

        partial_lines(
            &runner,
            &repo.path,
            "f.txt",
            &[
                line_sel(crate::state::LineKind::Del, Some(2), None, "b"),
                line_sel(crate::state::LineKind::Add, None, Some(2), "B"),
            ],
            PartialAction::Stage,
            false,
        )
        .await
        .expect("partial stage should succeed");

        let cached = runner
            .run_read(&repo.path, &["diff", "--cached", "--", "f.txt"])
            .await
            .unwrap()
            .stdout_string();
        assert!(cached.contains("+B"));
        assert!(!cached.contains("+E"));

        let unstaged = runner
            .run_read(&repo.path, &["diff", "--", "f.txt"])
            .await
            .unwrap()
            .stdout_string();
        assert!(unstaged.contains("+E"));
        assert!(!unstaged.contains("+B"));
        assert_eq!(fs::read_to_string(&f).unwrap(), "a\nB\nc\nd\nE\n");
    }

    #[tokio::test]
    async fn test_commit_after_partial_stage_ignores_unstaged_lines() {
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;
        let f = repo.path.join("f.txt");
        fs::write(&f, "a\nb\nc\nd\ne\n").unwrap();
        runner
            .run_write(&repo.path, &["add", "f.txt"], 10)
            .await
            .unwrap();
        runner
            .run_write(&repo.path, &["commit", "-m", "init"], 10)
            .await
            .unwrap();

        fs::write(&f, "a\nB\nc\nd\nE\n").unwrap();
        partial_lines(
            &runner,
            &repo.path,
            "f.txt",
            &[
                line_sel(crate::state::LineKind::Del, Some(2), None, "b"),
                line_sel(crate::state::LineKind::Add, None, Some(2), "B"),
            ],
            PartialAction::Stage,
            false,
        )
        .await
        .unwrap();

        let summary = commit(
            &runner,
            &repo.path,
            &["f.txt".to_string()],
            "partial commit",
            None,
        )
        .await
        .expect("commit after partial stage should succeed");
        assert_eq!(summary.subject, "partial commit");

        let head_file = runner
            .run_read(&repo.path, &["show", "HEAD:f.txt"])
            .await
            .unwrap()
            .stdout;
        assert_eq!(head_file, b"a\nB\nc\nd\ne\n");
        assert_eq!(fs::read_to_string(&f).unwrap(), "a\nB\nc\nd\nE\n");

        let status = runner
            .run_read(&repo.path, &["status", "--porcelain"])
            .await
            .unwrap()
            .stdout_string();
        assert!(status.contains(" M f.txt"), "status was: {}", status);
    }

    #[tokio::test]
    async fn test_partial_unstage_only_selected_lines() {
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;
        let f = repo.path.join("f.txt");
        fs::write(&f, "a\nb\nc\nd\ne\n").unwrap();
        runner
            .run_write(&repo.path, &["add", "f.txt"], 10)
            .await
            .unwrap();
        runner
            .run_write(&repo.path, &["commit", "-m", "init"], 10)
            .await
            .unwrap();

        fs::write(&f, "a\nB\nc\nd\nE\n").unwrap();
        runner
            .run_write(&repo.path, &["add", "f.txt"], 10)
            .await
            .unwrap();

        partial_lines(
            &runner,
            &repo.path,
            "f.txt",
            &[
                line_sel(crate::state::LineKind::Del, Some(2), None, "b"),
                line_sel(crate::state::LineKind::Add, None, Some(2), "B"),
            ],
            PartialAction::Unstage,
            false,
        )
        .await
        .expect("partial unstage should succeed");

        let cached = runner
            .run_read(&repo.path, &["diff", "--cached", "--", "f.txt"])
            .await
            .unwrap()
            .stdout_string();
        assert!(cached.contains("+E"));
        assert!(!cached.contains("+B"));

        let unstaged = runner
            .run_read(&repo.path, &["diff", "--", "f.txt"])
            .await
            .unwrap()
            .stdout_string();
        assert!(unstaged.contains("+B"));
        assert!(!unstaged.contains("+E"));
    }

    #[tokio::test]
    async fn test_partial_discard_preserves_unselected_and_undo_restores() {
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;
        let f = repo.path.join("f.txt");
        fs::write(&f, "a\nb\nc\nd\ne\n").unwrap();
        runner
            .run_write(&repo.path, &["add", "f.txt"], 10)
            .await
            .unwrap();
        runner
            .run_write(&repo.path, &["commit", "-m", "init"], 10)
            .await
            .unwrap();

        fs::write(&f, "a\nB\nc\nd\nE\n").unwrap();

        let backup = partial_lines(
            &runner,
            &repo.path,
            "f.txt",
            &[
                line_sel(crate::state::LineKind::Del, Some(2), None, "b"),
                line_sel(crate::state::LineKind::Add, None, Some(2), "B"),
            ],
            PartialAction::Discard,
            false,
        )
        .await
        .expect("partial discard should succeed")
        .expect("partial discard must return a backup");

        // Only the selected replacement was discarded.
        assert_eq!(fs::read_to_string(&f).unwrap(), "a\nb\nc\nd\nE\n");

        restore_backup(&runner, &repo.path, &backup.r#ref)
            .await
            .expect("undo should restore the file snapshot");
        assert_eq!(fs::read_to_string(&f).unwrap(), "a\nB\nc\nd\nE\n");
    }

    #[tokio::test]
    async fn test_partial_stage_preserves_crlf() {
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;
        let f = repo.path.join("f.txt");
        fs::write(&f, b"a\r\nb\r\nc\r\n").unwrap();
        runner
            .run_write(&repo.path, &["add", "f.txt"], 10)
            .await
            .unwrap();
        runner
            .run_write(&repo.path, &["commit", "-m", "init"], 10)
            .await
            .unwrap();

        fs::write(&f, b"a\r\nB\r\nc\r\n").unwrap();

        partial_lines(
            &runner,
            &repo.path,
            "f.txt",
            &[
                line_sel(crate::state::LineKind::Del, Some(2), None, "b"),
                line_sel(crate::state::LineKind::Add, None, Some(2), "B"),
            ],
            PartialAction::Stage,
            false,
        )
        .await
        .expect("CRLF partial stage should succeed");

        let index_content = runner
            .run_read(&repo.path, &["show", ":f.txt"])
            .await
            .unwrap()
            .stdout;
        assert_eq!(index_content, b"a\r\nB\r\nc\r\n");
        assert_eq!(fs::read(&f).unwrap(), b"a\r\nB\r\nc\r\n");
    }

    #[tokio::test]
    async fn test_partial_stage_untracked_subset() {
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;
        let base = repo.path.join("base.txt");
        fs::write(&base, "base\n").unwrap();
        runner
            .run_write(&repo.path, &["add", "base.txt"], 10)
            .await
            .unwrap();
        runner
            .run_write(&repo.path, &["commit", "-m", "init"], 10)
            .await
            .unwrap();

        let f = repo.path.join("new.txt");
        fs::write(&f, "one\ntwo\nthree\n").unwrap();

        partial_lines(
            &runner,
            &repo.path,
            "new.txt",
            &[
                line_sel(crate::state::LineKind::Add, None, Some(1), "one"),
                line_sel(crate::state::LineKind::Add, None, Some(2), "two"),
            ],
            PartialAction::Stage,
            false,
        )
        .await
        .expect("partial untracked stage should succeed");

        let index_content = runner
            .run_read(&repo.path, &["show", ":new.txt"])
            .await
            .unwrap()
            .stdout;
        assert_eq!(index_content, b"one\ntwo\n");
        assert_eq!(fs::read_to_string(&f).unwrap(), "one\ntwo\nthree\n");
    }

    #[tokio::test]
    async fn test_partial_commit_excludes_unselected_staged_file() {
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;
        let f = repo.path.join("f.txt");
        fs::write(&f, "a\nb\nc\nd\ne\n").unwrap();
        runner
            .run_write(&repo.path, &["add", "f.txt"], 10)
            .await
            .unwrap();
        runner
            .run_write(&repo.path, &["commit", "-m", "init"], 10)
            .await
            .unwrap();

        let other = repo.path.join("other.txt");
        fs::write(&other, "staged elsewhere\n").unwrap();
        runner
            .run_write(&repo.path, &["add", "other.txt"], 10)
            .await
            .unwrap();

        fs::write(&f, "a\nB\nc\nd\nE\n").unwrap();
        partial_lines(
            &runner,
            &repo.path,
            "f.txt",
            &[
                line_sel(crate::state::LineKind::Del, Some(2), None, "b"),
                line_sel(crate::state::LineKind::Add, None, Some(2), "B"),
            ],
            PartialAction::Stage,
            false,
        )
        .await
        .unwrap();

        commit(
            &runner,
            &repo.path,
            &["f.txt".to_string()],
            "partial commit only",
            None,
        )
        .await
        .expect("partial commit should succeed");

        let committed_files = runner
            .run_read(&repo.path, &["show", "--name-only", "--format=", "HEAD"])
            .await
            .unwrap()
            .stdout_string();
        assert!(committed_files.contains("f.txt"));
        assert!(!committed_files.contains("other.txt"));

        let head_file = runner
            .run_read(&repo.path, &["show", "HEAD:f.txt"])
            .await
            .unwrap()
            .stdout;
        assert_eq!(head_file, b"a\nB\nc\nd\ne\n");

        let status = runner
            .run_read(&repo.path, &["status", "--porcelain"])
            .await
            .unwrap()
            .stdout_string();
        assert!(status.contains("A  other.txt"), "status was: {}", status);
        assert!(status.contains(" M f.txt"), "status was: {}", status);
    }

    #[tokio::test]
    async fn test_partial_unstage_new_file_subset() {
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;
        let base = repo.path.join("base.txt");
        fs::write(&base, "base\n").unwrap();
        runner
            .run_write(&repo.path, &["add", "base.txt"], 10)
            .await
            .unwrap();
        runner
            .run_write(&repo.path, &["commit", "-m", "init"], 10)
            .await
            .unwrap();

        let f = repo.path.join("new.txt");
        fs::write(&f, "one\ntwo\nthree\n").unwrap();
        runner
            .run_write(&repo.path, &["add", "new.txt"], 10)
            .await
            .unwrap();

        partial_lines(
            &runner,
            &repo.path,
            "new.txt",
            &[line_sel(crate::state::LineKind::Add, None, Some(2), "two")],
            PartialAction::Unstage,
            false,
        )
        .await
        .expect("partial new-file unstage should succeed");

        let index_content = runner
            .run_read(&repo.path, &["show", ":new.txt"])
            .await
            .unwrap()
            .stdout;
        assert_eq!(index_content, b"one\nthree\n");
        assert_eq!(fs::read_to_string(&f).unwrap(), "one\ntwo\nthree\n");
    }

    #[tokio::test]
    async fn test_partial_stage_whole_deleted_file() {
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;
        let f = repo.path.join("gone.txt");
        fs::write(&f, "a\nb\nc\n").unwrap();
        runner
            .run_write(&repo.path, &["add", "gone.txt"], 10)
            .await
            .unwrap();
        runner
            .run_write(&repo.path, &["commit", "-m", "init"], 10)
            .await
            .unwrap();
        fs::remove_file(&f).unwrap();

        partial_lines(
            &runner,
            &repo.path,
            "gone.txt",
            &[
                line_sel(crate::state::LineKind::Del, Some(1), None, "a"),
                line_sel(crate::state::LineKind::Del, Some(2), None, "b"),
                line_sel(crate::state::LineKind::Del, Some(3), None, "c"),
            ],
            PartialAction::Stage,
            false,
        )
        .await
        .expect("staging a whole deletion should succeed");

        let status = runner
            .run_read(&repo.path, &["status", "--porcelain"])
            .await
            .unwrap()
            .stdout_string();
        assert!(status.contains("D  gone.txt"));
    }

    #[tokio::test]
    async fn test_partial_discard_untracked_subset_and_undo() {
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;
        let base = repo.path.join("base.txt");
        fs::write(&base, "base\n").unwrap();
        runner
            .run_write(&repo.path, &["add", "base.txt"], 10)
            .await
            .unwrap();
        runner
            .run_write(&repo.path, &["commit", "-m", "init"], 10)
            .await
            .unwrap();

        let f = repo.path.join("new.txt");
        fs::write(&f, "one\ntwo\nthree\n").unwrap();

        let backup = partial_lines(
            &runner,
            &repo.path,
            "new.txt",
            &[line_sel(crate::state::LineKind::Add, None, Some(2), "two")],
            PartialAction::Discard,
            false,
        )
        .await
        .expect("partial untracked discard should succeed")
        .expect("backup expected");

        assert_eq!(fs::read_to_string(&f).unwrap(), "one\nthree\n");
        assert!(backup.description.contains("new.txt"));

        restore_backup(&runner, &repo.path, &backup.r#ref)
            .await
            .expect("undo should restore the untracked file");
        assert_eq!(fs::read_to_string(&f).unwrap(), "one\ntwo\nthree\n");
    }

    #[tokio::test]
    async fn test_partial_discard_whole_untracked_file_and_undo() {
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;
        let base = repo.path.join("base.txt");
        fs::write(&base, "base\n").unwrap();
        runner
            .run_write(&repo.path, &["add", "base.txt"], 10)
            .await
            .unwrap();
        runner
            .run_write(&repo.path, &["commit", "-m", "init"], 10)
            .await
            .unwrap();

        let f = repo.path.join("new.txt");
        fs::write(&f, "one\ntwo\n").unwrap();

        let backup = partial_lines(
            &runner,
            &repo.path,
            "new.txt",
            &[
                line_sel(crate::state::LineKind::Add, None, Some(1), "one"),
                line_sel(crate::state::LineKind::Add, None, Some(2), "two"),
            ],
            PartialAction::Discard,
            false,
        )
        .await
        .expect("whole untracked discard should succeed")
        .expect("backup expected");

        assert!(!f.exists(), "discarding every line should remove the file");
        restore_backup(&runner, &repo.path, &backup.r#ref)
            .await
            .expect("undo should restore the untracked file");
        assert_eq!(fs::read_to_string(&f).unwrap(), "one\ntwo\n");
    }

    #[tokio::test]
    async fn test_partial_stage_part_of_deleted_file() {
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;
        let f = repo.path.join("gone.txt");
        fs::write(&f, "a\nb\nc\n").unwrap();
        runner
            .run_write(&repo.path, &["add", "gone.txt"], 10)
            .await
            .unwrap();
        runner
            .run_write(&repo.path, &["commit", "-m", "init"], 10)
            .await
            .unwrap();
        fs::remove_file(&f).unwrap();

        partial_lines(
            &runner,
            &repo.path,
            "gone.txt",
            &[line_sel(crate::state::LineKind::Del, Some(2), None, "b")],
            PartialAction::Stage,
            false,
        )
        .await
        .expect("partial deletion stage should succeed");

        let index_content = runner
            .run_read(&repo.path, &["show", ":gone.txt"])
            .await
            .unwrap()
            .stdout;
        assert_eq!(index_content, b"a\nc\n");
    }

    #[tokio::test]
    async fn test_partial_unstage_part_of_deleted_file() {
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;
        let f = repo.path.join("gone.txt");
        fs::write(&f, "a\nb\nc\n").unwrap();
        runner
            .run_write(&repo.path, &["add", "gone.txt"], 10)
            .await
            .unwrap();
        runner
            .run_write(&repo.path, &["commit", "-m", "init"], 10)
            .await
            .unwrap();
        runner
            .run_write(&repo.path, &["rm", "gone.txt"], 10)
            .await
            .unwrap();

        partial_lines(
            &runner,
            &repo.path,
            "gone.txt",
            &[line_sel(crate::state::LineKind::Del, Some(2), None, "b")],
            PartialAction::Unstage,
            false,
        )
        .await
        .expect("partial deletion unstage should succeed");

        let index_content = runner
            .run_read(&repo.path, &["show", ":gone.txt"])
            .await
            .unwrap()
            .stdout;
        assert_eq!(index_content, b"b\n");
    }

    #[tokio::test]
    async fn test_partial_stage_no_newline() {
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;
        let f = repo.path.join("f.txt");
        fs::write(&f, b"a\nb").unwrap();
        runner
            .run_write(&repo.path, &["add", "f.txt"], 10)
            .await
            .unwrap();
        runner
            .run_write(&repo.path, &["commit", "-m", "init"], 10)
            .await
            .unwrap();

        fs::write(&f, b"a\nB").unwrap();

        partial_lines(
            &runner,
            &repo.path,
            "f.txt",
            &[
                line_sel(crate::state::LineKind::Del, Some(2), None, "b"),
                line_sel(crate::state::LineKind::Add, None, Some(2), "B"),
            ],
            PartialAction::Stage,
            false,
        )
        .await
        .expect("whole no-newline hunk should stage");

        let index_content = runner
            .run_read(&repo.path, &["show", ":f.txt"])
            .await
            .unwrap()
            .stdout;
        assert_eq!(index_content, b"a\nB");
    }

    #[tokio::test]
    async fn test_partial_stage_refuses_binary_without_changes() {
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;
        let f = repo.path.join("image.bin");
        fs::write(&f, [0u8, 1, 2, 3, 4, 5, 0, 7]).unwrap();
        runner
            .run_write(&repo.path, &["add", "image.bin"], 10)
            .await
            .unwrap();
        runner
            .run_write(&repo.path, &["commit", "-m", "init"], 10)
            .await
            .unwrap();
        fs::write(&f, [0u8, 9, 9, 9, 9, 9, 0, 7]).unwrap();

        let before = runner
            .run_read(&repo.path, &["status", "--porcelain"])
            .await
            .unwrap()
            .stdout_string();

        let err = partial_lines(
            &runner,
            &repo.path,
            "image.bin",
            &[line_sel(crate::state::LineKind::Add, None, Some(1), "x")],
            PartialAction::Stage,
            false,
        )
        .await
        .unwrap_err();
        assert!(err.message.contains("Binary"));

        let after = runner
            .run_read(&repo.path, &["status", "--porcelain"])
            .await
            .unwrap()
            .stdout_string();
        assert_eq!(before, after);
    }

    #[tokio::test]
    async fn test_partial_stage_refuses_conflicted_index() {
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;
        let f = repo.path.join("f.txt");
        fs::write(&f, "base\n").unwrap();
        runner
            .run_write(&repo.path, &["add", "f.txt"], 10)
            .await
            .unwrap();
        runner
            .run_write(&repo.path, &["commit", "-m", "base"], 10)
            .await
            .unwrap();

        runner
            .run_write(&repo.path, &["checkout", "-b", "side"], 10)
            .await
            .unwrap();
        fs::write(&f, "side\n").unwrap();
        runner
            .run_write(&repo.path, &["commit", "-am", "side"], 10)
            .await
            .unwrap();
        runner
            .run_write(&repo.path, &["checkout", "-"], 10)
            .await
            .unwrap();
        fs::write(&f, "main\n").unwrap();
        runner
            .run_write(&repo.path, &["commit", "-am", "main"], 10)
            .await
            .unwrap();
        let _ = runner
            .run_write(&repo.path, &["merge", "side"], 10)
            .await;

        let err = partial_lines(
            &runner,
            &repo.path,
            "f.txt",
            &[
                line_sel(crate::state::LineKind::Del, Some(1), None, "main"),
                line_sel(crate::state::LineKind::Add, None, Some(1), "merged"),
            ],
            PartialAction::Stage,
            false,
        )
        .await
        .unwrap_err();
        assert!(err.message.contains("conflicted"));
    }

    #[tokio::test]
    async fn test_check_pushed() {
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;

        let f = repo.path.join("test.txt");
        fs::write(&f, "content").unwrap();
        runner
            .run_write(&repo.path, &["add", "test.txt"], 10)
            .await
            .unwrap();
        runner
            .run_write(&repo.path, &["commit", "-m", "msg"], 10)
            .await
            .unwrap();

        let head_sha = runner
            .run_read(&repo.path, &["rev-parse", "HEAD"])
            .await
            .unwrap()
            .stdout_string()
            .trim()
            .to_string();

        let is_pushed = check_pushed(&runner, &repo.path, &head_sha).await.unwrap();
        assert!(!is_pushed);
    }

    #[tokio::test]
    async fn test_revert_requires_clean_tree_and_reverts_commit() {
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;

        let f = repo.path.join("file.txt");
        fs::write(&f, "v1\n").unwrap();
        runner
            .run_write(&repo.path, &["add", "file.txt"], 10)
            .await
            .unwrap();
        runner
            .run_write(&repo.path, &["commit", "-m", "c1"], 10)
            .await
            .unwrap();

        fs::write(&f, "v2\n").unwrap();
        runner
            .run_write(&repo.path, &["commit", "-am", "c2"], 10)
            .await
            .unwrap();

        let c2_sha = runner
            .run_read(&repo.path, &["rev-parse", "HEAD"])
            .await
            .unwrap()
            .stdout_string()
            .trim()
            .to_string();

        let dirty_file = repo.path.join("dirty.txt");
        fs::write(&dirty_file, "dirty").unwrap();
        let dirty_err = revert(&runner, &repo.path, &c2_sha).await;
        assert!(dirty_err.is_err(), "Revert must fail on dirty tree");

        fs::remove_file(&dirty_file).unwrap();

        let rev_summary = revert(&runner, &repo.path, &c2_sha)
            .await
            .expect("Revert should succeed on clean tree");
        assert!(rev_summary.subject.contains("Revert"));

        let content = fs::read_to_string(&f).unwrap();
        assert_eq!(content, "v1\n");
    }

    #[tokio::test]
    async fn test_prune_old_backups() {
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;

        let f = repo.path.join("file.txt");
        fs::write(&f, "v1\n").unwrap();
        runner
            .run_write(&repo.path, &["add", "file.txt"], 10)
            .await
            .unwrap();
        runner
            .run_write(&repo.path, &["commit", "-m", "init"], 10)
            .await
            .unwrap();

        let old_ts = Utc::now().timestamp() - (40 * 24 * 3600);
        let recent_ts = Utc::now().timestamp() - (2 * 24 * 3600);

        let old_ref = format!("refs/vantage/backup/{}-reset", old_ts);
        let recent_ref = format!("refs/vantage/backup/{}-reset", recent_ts);

        runner
            .run_write(&repo.path, &["update-ref", &old_ref, "HEAD"], 10)
            .await
            .unwrap();
        runner
            .run_write(&repo.path, &["update-ref", &recent_ref, "HEAD"], 10)
            .await
            .unwrap();

        prune_old_backups(&runner, &repo.path).await;

        let backups = list_backups(&runner, &repo.path).await.unwrap();
        assert_eq!(backups.len(), 1);
        assert_eq!(backups[0].r#ref, recent_ref);
    }
}
