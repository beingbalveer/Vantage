use crate::error::AppError;
use crate::git::runner::GitRunner;
use crate::state::BranchInfo;
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
    let repo_lock = runner.get_repo_lock(repo_path).await;
    let _repo_guard = repo_lock.lock().await;

    let out = runner
        .run_write(repo_path, &["checkout", branch], 20)
        .await?;
    if !out.success() {
        if out.stderr.contains("local changes would be overwritten") {
            return Err(AppError::dirty(
                "Your local changes would be overwritten by checkout. Commit or discard your changes first.",
            ));
        }
        return Err(AppError::other(out.stderr));
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
