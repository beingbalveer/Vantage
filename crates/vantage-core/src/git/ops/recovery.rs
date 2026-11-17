//! Backup listing, restore and pruning.

use std::fs;
use std::path::Path;

use chrono::Utc;

use super::{get_real_git_dir, load_backup_record, BackupRecord};
use crate::error::AppError;
use crate::git::runner::GitRunner;
use crate::state::BackupInfo;

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
                } else if r_ref.contains("amend") {
                    "amend"
                } else if r_ref.contains("undo") {
                    "undo"
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
        } else if record.kind == "amend" || record.kind == "undo" {
            // Put HEAD back on the pre-amend / pre-undo commit. The rewritten
            // (amend) or removed (undo) commit's changes are contained in this
            // commit, so a hard reset restores the exact prior state.
            if let Some(ref head) = record.head_sha {
                let out = runner
                    .run_write(repo_path, &["reset", "--hard", head], 30)
                    .await?;
                if !out.success() {
                    return Err(AppError::other(out.stderr));
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
    } else if backup_ref.contains("reset")
        || backup_ref.contains("amend")
        || backup_ref.contains("undo")
    {
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
