use crate::error::AppError;
use crate::git::runner::GitRunner;
use crate::git::stash::{stash_pop_locked, stash_push_locked};
use crate::state::{BranchInfo, CheckoutResult};
use std::path::Path;

pub async fn list_branches(
    runner: &GitRunner,
    repo_path: &Path,
) -> Result<Vec<BranchInfo>, AppError> {
    let out = runner
        .run_read(
            repo_path,
            &[
                "for-each-ref",
                "--format=%(refname:short)\x1f%(upstream:short)\x1f%(HEAD)",
                "refs/heads",
            ],
        )
        .await?;

    if !out.success() {
        return Err(AppError::other(out.stderr));
    }

    let mut branches = Vec::new();
    for line in out.stdout_string().lines() {
        let parts: Vec<&str> = line.split('\x1f').collect();
        if parts.is_empty() {
            continue;
        }

        let name = parts[0].trim().to_string();
        let upstream = if parts.len() > 1 && !parts[1].trim().is_empty() {
            Some(parts[1].trim().to_string())
        } else {
            None
        };
        let is_head = parts.len() > 2 && parts[2].trim() == "*";

        branches.push(BranchInfo {
            name,
            upstream,
            is_head,
        });
    }

    Ok(branches)
}

pub async fn checkout_branch(
    runner: &GitRunner,
    repo_path: &Path,
    branch: &str,
) -> Result<(), AppError> {
    checkout_target(runner, repo_path, branch, None, None, false)
        .await
        .map(|_| ())
}

/// Check out `target` (branch/tag/commit), optionally dealing with a dirty tree.
///
/// `dirty_action`:
/// - `"leave"` (default): plain checkout; git carries the changes across when it can
///   and we surface its real error when it would overwrite them.
/// - `"bring"`: stash first, check out, then re-apply the stash on the new branch.
/// - `"stash"`: park the changes (honouring `include_untracked`) and check out.
///
/// Every branch of this function either completes the checkout or returns git's own
/// error text, so the caller never dead-ends.
pub async fn checkout_target(
    runner: &GitRunner,
    repo_path: &Path,
    target: &str,
    dirty_action: Option<&str>,
    stash_message: Option<&str>,
    include_untracked: bool,
) -> Result<CheckoutResult, AppError> {
    let repo_lock = runner.get_repo_lock(repo_path).await;
    let _repo_guard = repo_lock.lock().await;

    match dirty_action.unwrap_or("leave") {
        "stash" => {
            let push =
                stash_push_locked(runner, repo_path, stash_message, include_untracked).await?;
            match checkout_locked(runner, repo_path, target).await {
                Ok(()) => Ok(CheckoutResult { stash: push.entry }),
                Err(err) => {
                    // Never strand the parked work: restore it on the current branch.
                    if push.stashed {
                        let _ = stash_pop_locked(runner, repo_path, 0).await;
                    }
                    Err(err)
                }
            }
        }
        "bring" => {
            let push =
                stash_push_locked(runner, repo_path, stash_message, include_untracked).await?;
            if !push.stashed {
                // Nothing to carry; a plain checkout is all that is needed.
                checkout_locked(runner, repo_path, target).await?;
                return Ok(CheckoutResult { stash: None });
            }
            match checkout_locked(runner, repo_path, target).await {
                Ok(()) => {
                    // Re-apply on the new branch; git's error is surfaced if it conflicts.
                    stash_pop_locked(runner, repo_path, 0).await?;
                    Ok(CheckoutResult { stash: None })
                }
                Err(err) => {
                    let _ = stash_pop_locked(runner, repo_path, 0).await;
                    Err(err)
                }
            }
        }
        _ => {
            checkout_locked(runner, repo_path, target).await?;
            Ok(CheckoutResult { stash: None })
        }
    }
}

async fn checkout_locked(
    runner: &GitRunner,
    repo_path: &Path,
    target: &str,
) -> Result<(), AppError> {
    let out = runner
        .run_write(repo_path, &["checkout", target], 20)
        .await?;
    if !out.success() {
        let stderr = out.stderr.clone();
        let message = stderr.trim().to_string();
        if message.contains("local changes") || message.contains("would be overwritten") {
            return Err(AppError::dirty(message).with_stderr(stderr));
        }
        return Err(AppError::other(message).with_stderr(stderr));
    }
    Ok(())
}

pub async fn sync_fetch(runner: &GitRunner, repo_path: &Path) -> Result<(), AppError> {
    let repo_lock = runner.get_repo_lock(repo_path).await;
    let _repo_guard = repo_lock.lock().await;

    let out = runner
        .run_write(repo_path, &["fetch", "--prune"], 120)
        .await?;
    if !out.success() {
        return Err(AppError::other(out.stderr));
    }
    Ok(())
}

pub async fn sync_pull(
    runner: &GitRunner,
    repo_path: &Path,
    target_branch: Option<&str>,
) -> Result<(), AppError> {
    let repo_lock = runner.get_repo_lock(repo_path).await;
    let _repo_guard = repo_lock.lock().await;

    let head_out = runner
        .run_read(repo_path, &["rev-parse", "--abbrev-ref", "HEAD"])
        .await?;
    let current_branch = head_out.stdout_string().trim().to_string();

    if let Some(tb) = target_branch {
        if current_branch != tb {
            return Err(AppError::other(format!(
                "Cannot pull inactive branch '{}' (current checked-out branch is '{}'). Checkout branch first.",
                tb, current_branch
            )));
        }
    }

    let out = runner
        .run_write(repo_path, &["pull", "--ff-only"], 120)
        .await?;
    if !out.success() {
        return Err(AppError::other(out.stderr));
    }
    Ok(())
}

pub async fn sync_push(
    runner: &GitRunner,
    repo_path: &Path,
    publish: bool,
    target_branch: Option<&str>,
) -> Result<(), AppError> {
    let repo_lock = runner.get_repo_lock(repo_path).await;
    let _repo_guard = repo_lock.lock().await;

    let head_out = runner
        .run_read(repo_path, &["rev-parse", "--abbrev-ref", "HEAD"])
        .await?;
    let current_branch = head_out.stdout_string().trim().to_string();

    if let Some(tb) = target_branch {
        if current_branch != tb {
            return Err(AppError::other(format!(
                "Cannot push inactive branch '{}' (current checked-out branch is '{}'). Checkout branch first.",
                tb, current_branch
            )));
        }
    }

    let mut args = vec!["push"];
    if publish {
        args.push("-u");
        args.push("origin");
        args.push(&current_branch);
    }

    let out = runner.run_write(repo_path, &args, 120).await?;
    if !out.success() {
        return Err(AppError::other(out.stderr));
    }
    Ok(())
}

pub async fn get_remote_url(
    runner: &GitRunner,
    repo_path: &Path,
) -> Result<Option<String>, AppError> {
    let out = runner
        .run_read(repo_path, &["config", "--get", "remote.origin.url"])
        .await?;
    if out.success() {
        let url = out.stdout_string().trim().to_string();
        if !url.is_empty() {
            return Ok(Some(url));
        }
    }
    Ok(None)
}

pub async fn is_commit_on_remote(
    runner: &GitRunner,
    repo_path: &Path,
    sha: &str,
) -> Result<bool, AppError> {
    let out = runner
        .run_read(repo_path, &["branch", "-r", "--contains", sha])
        .await?;
    if out.success() {
        let text = out.stdout_string();
        return Ok(!text.trim().is_empty());
    }
    Ok(false)
}

// ---------------- Branch create / rename / delete ----------------

/// Validate a branch name using git's own refname rules
/// (`git check-ref-format --branch`). Returns the original git error text on
/// failure so the UI never has to guess why a name was rejected.
pub async fn validate_branch_name(
    runner: &GitRunner,
    repo_path: &Path,
    name: &str,
) -> Result<(), AppError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::new("Validation", "Branch name cannot be empty."));
    }

    let out = runner
        .run_read(repo_path, &["check-ref-format", "--branch", trimmed])
        .await?;

    if !out.success() {
        let detail = out.stderr.trim();
        let message = if detail.is_empty() {
            format!("'{}' is not a valid branch name.", trimmed)
        } else {
            detail.to_string()
        };
        return Err(AppError::new("Validation", message).with_stderr(out.stderr));
    }
    Ok(())
}

/// Resolve the currently checked-out branch, or `None` on a detached HEAD.
async fn current_branch_name(
    runner: &GitRunner,
    repo_path: &Path,
) -> Result<Option<String>, AppError> {
    let out = runner
        .run_read(repo_path, &["rev-parse", "--abbrev-ref", "HEAD"])
        .await?;
    let name = out.stdout_string().trim().to_string();
    if name.is_empty() || name == "HEAD" {
        Ok(None)
    } else {
        Ok(Some(name))
    }
}

/// True when the working tree has staged, unstaged or untracked changes.
async fn working_tree_dirty(runner: &GitRunner, repo_path: &Path) -> Result<bool, AppError> {
    let out = runner
        .run_read(
            repo_path,
            &["status", "--porcelain=v2", "-z", "--untracked-files=all"],
        )
        .await?;
    if !out.success() {
        return Err(AppError::other(out.stderr.trim().to_string()).with_stderr(out.stderr));
    }
    Ok(!out.stdout.is_empty())
}

/// Create a branch at `start_point` (defaults to HEAD). When `checkout` is set
/// the branch is checked out immediately, but only if the working tree is
/// clean — a dirty tree is refused with an actionable message rather than
/// silently leaving the repo in a half-switched state.
pub async fn create_branch(
    runner: &GitRunner,
    repo_path: &Path,
    name: &str,
    start_point: Option<&str>,
    checkout: bool,
) -> Result<(), AppError> {
    let branch = name.trim();
    validate_branch_name(runner, repo_path, branch).await?;
    let start = start_point.map(|s| s.trim()).filter(|s| !s.is_empty());

    let repo_lock = runner.get_repo_lock(repo_path).await;
    let _repo_guard = repo_lock.lock().await;

    if checkout && working_tree_dirty(runner, repo_path).await? {
        return Err(AppError::dirty(format!(
            "Working tree has uncommitted changes. Commit or discard them before creating and checking out '{}'.",
            branch
        )));
    }

    let mut args = vec!["branch", branch];
    if let Some(s) = start {
        args.push(s);
    }

    let out = runner.run_write(repo_path, &args, 20).await?;
    if !out.success() {
        return Err(AppError::other(out.stderr.trim().to_string()).with_stderr(out.stderr));
    }

    if checkout {
        let out = runner.run_write(repo_path, &["checkout", branch], 20).await?;
        if !out.success() {
            return Err(AppError::other(out.stderr.trim().to_string()).with_stderr(out.stderr));
        }
    }

    Ok(())
}

/// Rename `from` (defaults to the current branch) to `to` via `git branch -m`.
pub async fn rename_branch(
    runner: &GitRunner,
    repo_path: &Path,
    from: Option<&str>,
    to: &str,
) -> Result<(), AppError> {
    let new_name = to.trim();
    validate_branch_name(runner, repo_path, new_name).await?;

    let old = from.map(|s| s.trim()).filter(|s| !s.is_empty());

    let repo_lock = runner.get_repo_lock(repo_path).await;
    let _repo_guard = repo_lock.lock().await;

    let mut args = vec!["branch", "-m"];
    if let Some(o) = old {
        args.push(o);
    }
    args.push(new_name);

    let out = runner.run_write(repo_path, &args, 20).await?;
    if !out.success() {
        return Err(AppError::other(out.stderr.trim().to_string()).with_stderr(out.stderr));
    }
    Ok(())
}

/// Delete a branch. `force` maps to `-D` (loses unmerged commits) and is the
/// caller's responsibility to gate behind an explicit confirmation. Deleting
/// the currently checked-out branch is always refused.
pub async fn delete_branch(
    runner: &GitRunner,
    repo_path: &Path,
    name: &str,
    force: bool,
) -> Result<(), AppError> {
    let branch = name.trim();
    if branch.is_empty() {
        return Err(AppError::new("Validation", "Branch name cannot be empty."));
    }

    let repo_lock = runner.get_repo_lock(repo_path).await;
    let _repo_guard = repo_lock.lock().await;

    if let Some(current) = current_branch_name(runner, repo_path).await? {
        if current == branch {
            return Err(AppError::new(
                "Validation",
                format!(
                    "Cannot delete '{}' because it is the currently checked-out branch. Switch to another branch first.",
                    branch
                ),
            ));
        }
    }

    let flag = if force { "-D" } else { "-d" };
    let out = runner.run_write(repo_path, &["branch", flag, branch], 20).await?;
    if !out.success() {
        return Err(AppError::other(out.stderr.trim().to_string()).with_stderr(out.stderr));
    }
    Ok(())
}

/// Whether `name` is already merged into HEAD or into its own upstream, which
/// is the precondition `git branch -d` uses. Lets the delete dialog explain
/// the consequence *before* the user commits to a destructive action.
pub async fn is_branch_merged(
    runner: &GitRunner,
    repo_path: &Path,
    name: &str,
) -> Result<bool, AppError> {
    let branch = name.trim();
    if branch.is_empty() {
        return Err(AppError::new("Validation", "Branch name cannot be empty."));
    }

    let head_check = runner
        .run_read(repo_path, &["merge-base", "--is-ancestor", branch, "HEAD"])
        .await?;
    if head_check.success() {
        return Ok(true);
    }

    let upstream_spec = format!("{}@{{upstream}}", branch);
    let upstream_out = runner
        .run_read(
            repo_path,
            &["rev-parse", "--abbrev-ref", "--symbolic-full-name", &upstream_spec],
        )
        .await;
    if let Ok(out) = upstream_out {
        if out.success() {
            let upstream = out.stdout_string().trim().to_string();
            if !upstream.is_empty() {
                let merged = runner
                    .run_read(repo_path, &["merge-base", "--is-ancestor", branch, &upstream])
                    .await?;
                if merged.success() {
                    return Ok(true);
                }
            }
        }
    }

    Ok(false)
}
