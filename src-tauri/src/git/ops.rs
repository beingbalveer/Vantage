use crate::error::AppError;
use crate::git::identity::ensure_commit_identity;
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

    // Fail with an actionable message instead of git's raw "Author identity
    // unknown" error, and do it before staging anything.
    ensure_commit_identity(runner, repo_path).await?;

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
                let tokens: Vec<&str> = line.split(' ').collect();
                if tokens.len() >= 9 && i + 1 < raw_parts.len() {
                    let path = tokens[tokens.len() - 1].to_string();
                    let orig_path = String::from_utf8_lossy(raw_parts[i + 1]).to_string();
                    if paths.contains(&path) && !paths_to_commit.contains(&orig_path) {
                        paths_to_commit.push(orig_path);
                    }
                    i += 2;
                    continue;
                }
                i += 1;
            } else {
                i += 1;
            }
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
