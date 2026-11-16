use crate::error::AppError;
use crate::git::runner::GitRunner;
use crate::git::stash::{stash_pop_locked, stash_push_locked};
use crate::state::{BranchInfo, CheckoutResult};
use std::path::Path;

use serde::{Deserialize, Serialize};

/// A git tag. `target` is the peeled commit/object SHA the tag ultimately
/// points at (for annotated tags this is `%(*objectname)`, not the tag
/// object itself), so callers can match it directly against a commit row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagInfo {
    pub name: String,
    pub target: String,
    pub annotated: bool,
    /// Annotated tag message subject line; `None` for lightweight tags.
    pub message: Option<String>,
    /// Tag creation date (annotated) or target commit date (lightweight).
    pub date: Option<String>,
}

/// Machine-parseable `for-each-ref` format. Fields are separated by the ASCII
/// unit separator (0x1f). `%(*objectname)` is empty for lightweight tags.
const TAG_FORMAT: &str = "%(refname:short)\x1f%(objecttype)\x1f%(objectname)\x1f%(*objectname)\x1f%(creatordate:iso-strict)\x1f%(contents:subject)";

pub async fn list_branches(
    runner: &GitRunner,
    repo_path: &Path,
) -> Result<Vec<BranchInfo>, AppError> {
    let out = runner
        .run_read(
            repo_path,
            &[
                "for-each-ref",
                "--format=%(refname:short)\x1f%(upstream:short)\x1f%(HEAD)\x1f%(committerdate:iso-strict)",
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
        let updated_at = parts
            .get(3)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        branches.push(BranchInfo {
            name,
            upstream,
            is_head,
            updated_at,
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

/// What a merge left behind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeOutcome {
    /// True when git stopped on conflicts; the working tree is left for the
    /// user, exactly as the command line would.
    pub conflicted: bool,
}

/// Merge `target` (a branch or remote ref) into the checked-out branch.
pub async fn merge_into_current(
    runner: &GitRunner,
    repo_path: &Path,
    target: &str,
) -> Result<MergeOutcome, AppError> {
    let repo_lock = runner.get_repo_lock(repo_path).await;
    let _repo_guard = repo_lock.lock().await;

    let out = runner
        .run_write(repo_path, &["merge", "--no-edit", target], 120)
        .await?;
    if out.success() {
        return Ok(MergeOutcome { conflicted: false });
    }
    let stderr = out.stderr.clone();
    if stderr.contains("CONFLICT") || stderr.contains("Automatic merge failed") {
        return Ok(MergeOutcome { conflicted: true });
    }
    Err(AppError::other(stderr))
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

/// How a pull reconciles a branch that has diverged from its upstream.
///
/// `--ff-only` is the default because it can never rewrite or merge local
/// commits behind the user's back, which matters when another agent may be
/// committing in the same work tree.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PullStrategy {
    /// Refuse the pull when the branches diverged.
    #[default]
    FastForward,
    /// `--no-rebase`: merge the upstream branch into the local one.
    Merge,
    /// `--rebase`: replay local commits on top of the upstream branch.
    Rebase,
}

impl PullStrategy {
    /// The `git pull` argument that selects this strategy.
    pub fn flag(self) -> &'static str {
        match self {
            Self::FastForward => "--ff-only",
            Self::Merge => "--no-rebase",
            Self::Rebase => "--rebase",
        }
    }

    /// The complete `git pull` invocation for this strategy.
    ///
    /// A merge pull can create a merge commit, and git would open an editor
    /// for its message when `GIT_MERGE_AUTOEDIT` is set (or when a terminal is
    /// attached). `--no-edit` keeps the background job from blocking on that
    /// editor; fast-forward and rebase pulls are unchanged.
    pub fn pull_args(self) -> &'static [&'static str] {
        match self {
            Self::FastForward => &["pull", "--ff-only"],
            Self::Merge => &["pull", "--no-rebase", "--no-edit"],
            Self::Rebase => &["pull", "--rebase"],
        }
    }
}

pub async fn sync_pull(
    runner: &GitRunner,
    repo_path: &Path,
    target_branch: Option<&str>,
    strategy: PullStrategy,
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
        .run_write(repo_path, strategy.pull_args(), 120)
        .await?;
    if !out.success() {
        let stderr = out.stderr.clone();
        let message = stderr.trim();
        // git's wording for the two cases the user can act on. Anything else
        // (auth, network, missing upstream) is passed through untouched.
        if strategy == PullStrategy::FastForward
            && (message.contains("Not possible to fast-forward")
                || message.contains("diverging branches")
                || message.contains("would be overwritten"))
        {
            return Err(AppError::other(
                "This branch has local commits the remote does not, so it cannot be \
                 fast-forwarded. Pick a pull strategy in Settings → Git, or merge/rebase \
                 outside Vantage.",
            )
            .with_stderr(stderr));
        }
        if message.contains("CONFLICT") || message.contains("Automatic merge failed") {
            return Err(AppError::other(
                "The pull stopped on a conflict. Resolve the conflicted files, then abort or \
                 finish the operation before pulling again.",
            )
            .with_stderr(stderr));
        }
        return Err(AppError::other(message.to_string()).with_stderr(stderr));
    }
    Ok(())
}

/// Whether the checked-out branch tracks a remote branch.
///
/// A branch with no upstream has to be published with `--set-upstream` the
/// first time it is pushed.
pub async fn has_upstream(runner: &GitRunner, repo_path: &Path) -> Result<bool, AppError> {
    let out = runner
        .run_read(
            repo_path,
            &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
        )
        .await?;
    Ok(out.success() && !out.stdout_string().trim().is_empty())
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

    // Detached HEAD has no branch to publish. Never run `push -u origin HEAD`:
    // it would either fail or create a remote branch literally named HEAD.
    if current_branch.is_empty() || current_branch == "HEAD" {
        return Err(AppError::new(
            "DetachedHead",
            "HEAD is detached, so there is no branch to push. Check out a branch first.",
        ));
    }

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
        let out = runner
            .run_write(repo_path, &["checkout", branch], 20)
            .await?;
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

/// What a branch delete did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BranchDeleteOutcome {
    Deleted,
    /// `git branch -d` refused because the branch has commits that are not
    /// merged into HEAD or its upstream. The branch is untouched; callers must
    /// ask for an explicit second confirmation before retrying with `force`.
    NotMerged,
}

/// Whether git refused `branch -d` because the branch is not fully merged.
fn branch_delete_refused_as_unmerged(stderr: &str) -> bool {
    stderr.to_ascii_lowercase().contains("not fully merged")
}

/// Delete a branch. `force` maps to `-D` (loses unmerged commits) and is the
/// caller's responsibility to gate behind an explicit confirmation. Deleting
/// the currently checked-out branch is always refused.
///
/// A normal call first runs `git branch -d`. When git refuses because the
/// branch has unmerged commits this returns [`BranchDeleteOutcome::NotMerged`]
/// instead of an error, so the UI can explain the consequence and require a
/// second confirmation before trying again with `force`.
pub async fn delete_branch(
    runner: &GitRunner,
    repo_path: &Path,
    name: &str,
    force: bool,
) -> Result<BranchDeleteOutcome, AppError> {
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
    let out = runner
        .run_write(repo_path, &["branch", flag, branch], 20)
        .await?;
    if out.success() {
        return Ok(BranchDeleteOutcome::Deleted);
    }

    // Git's wording is stable but localised builds can vary it, so confirm
    // with the same merge precondition `-d` uses before offering `-D`.
    if !force
        && branch_delete_refused_as_unmerged(&out.stderr)
        && !is_branch_merged(runner, repo_path, branch).await?
    {
        return Ok(BranchDeleteOutcome::NotMerged);
    }

    Err(AppError::other(out.stderr.trim().to_string()).with_stderr(out.stderr))
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
            &[
                "rev-parse",
                "--abbrev-ref",
                "--symbolic-full-name",
                &upstream_spec,
            ],
        )
        .await;
    if let Ok(out) = upstream_out {
        if out.success() {
            let upstream = out.stdout_string().trim().to_string();
            if !upstream.is_empty() {
                let merged = runner
                    .run_read(
                        repo_path,
                        &["merge-base", "--is-ancestor", branch, &upstream],
                    )
                    .await?;
                if merged.success() {
                    return Ok(true);
                }
            }
        }
    }

    Ok(false)
}

// ---------------- Tags (ALPH-157) ----------------

fn parse_tag_line(line: &str) -> Option<TagInfo> {
    if line.trim().is_empty() {
        return None;
    }
    let parts: Vec<&str> = line.split('\x1f').collect();
    if parts.len() < 4 {
        return None;
    }

    let name = parts[0].trim().to_string();
    if name.is_empty() {
        return None;
    }

    let object_type = parts[1].trim();
    let object_name = parts[2].trim();
    let peeled = parts.get(3).map(|s| s.trim()).unwrap_or("");
    let annotated = object_type == "tag";

    // Annotated tags have a distinct tag object; list the commit/tree it peels to.
    let target = if annotated && !peeled.is_empty() {
        peeled.to_string()
    } else {
        object_name.to_string()
    };

    let date = parts
        .get(4)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    // `%(contents:subject)` falls back to the target commit subject for
    // lightweight tags, which is misleading — only surface it for annotated tags.
    let message = if annotated {
        parts
            .get(5)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    } else {
        None
    };

    Some(TagInfo {
        name,
        target,
        annotated,
        message,
        date,
    })
}

/// List all local tags with their peeled target SHA and annotation metadata.
pub async fn list_tags(runner: &GitRunner, repo_path: &Path) -> Result<Vec<TagInfo>, AppError> {
    let format_arg = format!("--format={}", TAG_FORMAT);
    let out = runner
        .run_read(repo_path, &["for-each-ref", &format_arg, "refs/tags"])
        .await?;

    if !out.success() {
        return Err(AppError::other(out.stderr));
    }

    Ok(out
        .stdout_string()
        .lines()
        .filter_map(parse_tag_line)
        .collect())
}

/// Validate a user-supplied tag name.
///
/// Structural guards catch what `git check-ref-format` cannot (a leading `-`
/// would otherwise be swallowed as an option), then the name is validated as
/// the full ref `refs/tags/<name>` using git's own rules.
pub async fn validate_tag_name(runner: &GitRunner, name: &str) -> Result<(), AppError> {
    if name.trim().is_empty() {
        return Err(AppError::new("Validation", "Tag name cannot be empty."));
    }
    if name != name.trim() {
        return Err(AppError::new(
            "Validation",
            "Tag name cannot start or end with whitespace.",
        ));
    }
    if name.starts_with('-') {
        return Err(AppError::new(
            "Validation",
            "Tag name cannot start with '-'.",
        ));
    }
    if name.contains(char::is_whitespace) {
        return Err(AppError::new(
            "Validation",
            "Tag name cannot contain whitespace.",
        ));
    }
    if name.chars().any(|c| c.is_control()) {
        return Err(AppError::new(
            "Validation",
            "Tag name cannot contain control characters.",
        ));
    }

    let full_ref = format!("refs/tags/{}", name);
    let out = runner
        .run(None, &["check-ref-format", &full_ref], 5, false)
        .await?;
    if !out.success() {
        return Err(AppError::new(
            "Validation",
            format!("'{}' is not a valid git tag name.", name),
        ));
    }
    Ok(())
}

async fn tag_exists(runner: &GitRunner, repo_path: &Path, name: &str) -> Result<bool, AppError> {
    let full_ref = format!("refs/tags/{}", name);
    let out = runner
        .run_read(repo_path, &["show-ref", "--verify", "--quiet", &full_ref])
        .await?;
    Ok(out.success())
}

/// Create a lightweight or annotated tag at `target` (a SHA, ref, or branch).
///
/// Annotated tags always pass an explicit `-m` message so no editor is opened;
/// an empty message is rejected. A pre-existing tag name is a clear error.
pub async fn create_tag(
    runner: &GitRunner,
    repo_path: &Path,
    name: &str,
    target: Option<&str>,
    annotated: bool,
    message: Option<&str>,
) -> Result<(), AppError> {
    validate_tag_name(runner, name).await?;

    let repo_lock = runner.get_repo_lock(repo_path).await;
    let _repo_guard = repo_lock.lock().await;

    if tag_exists(runner, repo_path, name).await? {
        return Err(AppError::new(
            "Validation",
            format!("A tag named '{}' already exists.", name),
        ));
    }

    let target = target
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("HEAD");

    let message_owned;
    let args: Vec<&str> = if annotated {
        let msg = message.map(|m| m.trim()).unwrap_or("");
        if msg.is_empty() {
            return Err(AppError::new(
                "Validation",
                "Annotated tags require a message.",
            ));
        }
        message_owned = msg.to_string();
        vec!["tag", "-a", "-m", &message_owned, name, target]
    } else {
        vec!["tag", name, target]
    };

    let out = runner.run_write(repo_path, &args, 20).await?;
    if !out.success() {
        // Race: the tag may have been created between our check and the call.
        if out.stderr.contains("already exists") {
            return Err(AppError::new(
                "Validation",
                format!("A tag named '{}' already exists.", name),
            ));
        }
        return Err(AppError::other(out.stderr));
    }
    Ok(())
}

/// Push a single existing tag to `origin`. Uses a fully-qualified refspec so a
/// branch of the same name can never be pushed by accident.
pub async fn push_tag(runner: &GitRunner, repo_path: &Path, name: &str) -> Result<(), AppError> {
    validate_tag_name(runner, name).await?;

    let repo_lock = runner.get_repo_lock(repo_path).await;
    let _repo_guard = repo_lock.lock().await;

    let refspec = format!("refs/tags/{}:refs/tags/{}", name, name);
    let out = runner
        .run_write(repo_path, &["push", "origin", &refspec], 120)
        .await?;
    if !out.success() {
        return Err(AppError::other(out.stderr));
    }
    Ok(())
}

/// Explicitly push every local tag to `origin` (`git push origin --tags`).
/// Kept separate from branch pushes on purpose (see `sync_push`).
pub async fn push_all_tags(runner: &GitRunner, repo_path: &Path) -> Result<(), AppError> {
    let repo_lock = runner.get_repo_lock(repo_path).await;
    let _repo_guard = repo_lock.lock().await;

    let out = runner
        .run_write(repo_path, &["push", "origin", "--tags"], 120)
        .await?;
    if !out.success() {
        return Err(AppError::other(out.stderr));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn pull_strategy_maps_to_git_flags() {
        assert_eq!(PullStrategy::default(), PullStrategy::FastForward);
        assert_eq!(PullStrategy::FastForward.flag(), "--ff-only");
        assert_eq!(PullStrategy::Merge.flag(), "--no-rebase");
        assert_eq!(PullStrategy::Rebase.flag(), "--rebase");
    }

    #[test]
    fn merge_pull_never_waits_for_an_editor() {
        assert_eq!(
            PullStrategy::Merge.pull_args(),
            ["pull", "--no-rebase", "--no-edit"],
            "a merge pull must not open an editor for its merge commit"
        );
        // The strategies that cannot create a merge commit keep their exact
        // previous invocation.
        assert_eq!(PullStrategy::FastForward.pull_args(), ["pull", "--ff-only"]);
        assert_eq!(PullStrategy::Rebase.pull_args(), ["pull", "--rebase"]);
    }

    #[test]
    fn unmerged_branch_refusal_is_recognised() {
        assert!(branch_delete_refused_as_unmerged(
            "error: the branch 'feature' is not fully merged"
        ));
        assert!(branch_delete_refused_as_unmerged(
            "error: The branch 'feature' is not fully merged."
        ));
        assert!(!branch_delete_refused_as_unmerged(
            "error: branch 'feature' not found"
        ));
    }

    #[test]
    fn pull_strategy_persists_as_camel_case() {
        // Settings store this verbatim; a rename here would silently reset
        // every saved preference to the default.
        assert_eq!(
            serde_json::to_string(&PullStrategy::FastForward).unwrap(),
            "\"fastForward\""
        );
        assert_eq!(
            serde_json::from_str::<PullStrategy>("\"rebase\"").unwrap(),
            PullStrategy::Rebase
        );
    }

    struct TempTestRepo {
        path: std::path::PathBuf,
    }

    impl TempTestRepo {
        async fn new(runner: &GitRunner) -> Self {
            let path =
                std::env::temp_dir().join(format!("vantage_tag_test_{}", uuid::Uuid::new_v4()));
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

        async fn commit(
            &self,
            runner: &GitRunner,
            file: &str,
            contents: &str,
            msg: &str,
        ) -> String {
            fs::write(self.path.join(file), contents).unwrap();
            runner
                .run_write(&self.path, &["add", file], 10)
                .await
                .unwrap();
            runner
                .run_write(&self.path, &["commit", "-m", msg], 10)
                .await
                .unwrap();
            runner
                .run_read(&self.path, &["rev-parse", "HEAD"])
                .await
                .unwrap()
                .stdout_string()
                .trim()
                .to_string()
        }
    }

    impl Drop for TempTestRepo {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn parse_tag_line_lightweight_uses_object_sha() {
        let line = "v1.0\x1fcommit\x1fabc123\x1f\x1f2020-01-01T00:00:00+00:00\x1ffirst commit";
        let tag = parse_tag_line(line).expect("line should parse");
        assert_eq!(tag.name, "v1.0");
        assert!(!tag.annotated);
        assert_eq!(tag.target, "abc123");
        assert_eq!(tag.message, None, "lightweight tags carry no message");
        assert_eq!(tag.date.as_deref(), Some("2020-01-01T00:00:00+00:00"));
    }

    #[test]
    fn parse_tag_line_annotated_peels_target_and_keeps_message() {
        let line = "v2.0\x1ftag\x1ftagobjectsha\x1fcommitobjectsha\x1f2020-01-02T00:00:00+00:00\x1fRelease 2.0";
        let tag = parse_tag_line(line).expect("line should parse");
        assert_eq!(tag.name, "v2.0");
        assert!(tag.annotated);
        assert_eq!(
            tag.target, "commitobjectsha",
            "annotated tags must report the peeled commit, not the tag object"
        );
        assert_eq!(tag.message.as_deref(), Some("Release 2.0"));
    }

    #[test]
    fn parse_tag_line_rejects_malformed_input() {
        assert!(parse_tag_line("").is_none());
        assert!(parse_tag_line("v1.0\x1fcommit").is_none());
        assert!(parse_tag_line("\x1fcommit\x1fsha\x1f\x1f\x1f").is_none());
    }

    #[tokio::test]
    async fn validate_tag_name_accepts_valid_and_rejects_invalid() {
        let runner = GitRunner::new();

        assert!(validate_tag_name(&runner, "v1.0").await.is_ok());
        assert!(validate_tag_name(&runner, "release/v1.0").await.is_ok());

        assert!(validate_tag_name(&runner, "").await.is_err());
        assert!(validate_tag_name(&runner, "   ").await.is_err());
        assert!(validate_tag_name(&runner, "-dash").await.is_err());
        assert!(validate_tag_name(&runner, "has space").await.is_err());
        assert!(validate_tag_name(&runner, " v1.0").await.is_err());
        assert!(validate_tag_name(&runner, "bad..name").await.is_err());
        assert!(validate_tag_name(&runner, "ends.lock").await.is_err());
    }

    #[tokio::test]
    async fn create_and_list_lightweight_and_annotated_tags() {
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;
        let head = repo
            .commit(&runner, "a.txt", "hello\n", "first commit")
            .await;

        create_tag(&runner, &repo.path, "v1.0", Some(&head), false, None)
            .await
            .expect("lightweight tag should be created");
        create_tag(
            &runner,
            &repo.path,
            "v2.0",
            Some(&head),
            true,
            Some("Release 2.0"),
        )
        .await
        .expect("annotated tag should be created");

        let tags = list_tags(&runner, &repo.path).await.unwrap();
        assert_eq!(tags.len(), 2);

        let light = tags.iter().find(|t| t.name == "v1.0").unwrap();
        assert!(!light.annotated);
        assert_eq!(light.target, head);
        assert_eq!(light.message, None);

        let annotated = tags.iter().find(|t| t.name == "v2.0").unwrap();
        assert!(annotated.annotated);
        assert_eq!(
            annotated.target, head,
            "annotated tag must resolve to the commit SHA"
        );
        assert_eq!(annotated.message.as_deref(), Some("Release 2.0"));
    }

    #[tokio::test]
    async fn create_tag_rejects_duplicate_and_missing_annotated_message() {
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;
        let head = repo
            .commit(&runner, "a.txt", "hello\n", "first commit")
            .await;

        create_tag(&runner, &repo.path, "v1.0", Some(&head), false, None)
            .await
            .unwrap();

        let duplicate = create_tag(&runner, &repo.path, "v1.0", Some(&head), false, None).await;
        assert!(duplicate.is_err());
        assert_eq!(duplicate.unwrap_err().code, "Validation");

        let no_message =
            create_tag(&runner, &repo.path, "v3.0", Some(&head), true, Some("   ")).await;
        assert!(
            no_message.is_err(),
            "annotated tag without message must fail"
        );

        // The failed annotated attempt must not leave a partial tag behind.
        let tags = list_tags(&runner, &repo.path).await.unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "v1.0");
    }

    #[tokio::test]
    async fn create_tag_targets_commit_not_reachable_from_head() {
        // Tagging an older commit (not the branch tip) must be allowed and the
        // listed target must be that older commit, not HEAD.
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;
        let first = repo.commit(&runner, "a.txt", "one\n", "first").await;
        repo.commit(&runner, "a.txt", "two\n", "second").await;

        create_tag(&runner, &repo.path, "v0.1", Some(&first), false, None)
            .await
            .unwrap();

        let tags = list_tags(&runner, &repo.path).await.unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].target, first);
    }

    #[tokio::test]
    async fn checkout_branch_switches_head_and_reports_unknown_branches() {
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;
        let default = runner
            .run_read(&repo.path, &["symbolic-ref", "--short", "HEAD"])
            .await
            .unwrap()
            .stdout_string()
            .trim()
            .to_string();
        repo.commit(&runner, "a.txt", "one\n", "first").await;
        runner
            .run_write(&repo.path, &["checkout", "-b", "feature"], 10)
            .await
            .unwrap();
        repo.commit(&runner, "b.txt", "two\n", "second").await;
        runner
            .run_write(&repo.path, &["checkout", &default], 10)
            .await
            .unwrap();

        checkout_branch(&runner, &repo.path, "feature")
            .await
            .unwrap();
        let head = runner
            .run_read(&repo.path, &["symbolic-ref", "--short", "HEAD"])
            .await
            .unwrap()
            .stdout_string();
        assert_eq!(head.trim(), "feature");
        assert!(repo.path.join("b.txt").exists());

        let err = checkout_branch(&runner, &repo.path, "no-such-branch")
            .await
            .unwrap_err();
        assert!(
            err.message.contains("no-such-branch"),
            "git's own error should surface: {}",
            err.message
        );
    }

    #[tokio::test]
    async fn branch_list_includes_the_tip_commit_date() {
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;
        repo.commit(&runner, "a.txt", "one\n", "first").await;
        runner
            .run_write(&repo.path, &["branch", "feature"], 10)
            .await
            .unwrap();

        let branches = list_branches(&runner, &repo.path).await.unwrap();
        let head = branches.iter().find(|b| b.is_head).expect("HEAD branch");
        let feature = branches
            .iter()
            .find(|b| b.name == "feature")
            .expect("feature branch");

        let updated = feature.updated_at.as_deref().expect("branch date");
        assert_eq!(head.updated_at.as_deref(), Some(updated));
        assert!(
            chrono::DateTime::parse_from_rfc3339(updated).is_ok(),
            "branch date must be ISO-8601, got {updated}"
        );
    }

    #[tokio::test]
    async fn delete_branch_removes_a_merged_branch_without_force() {
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;
        repo.commit(&runner, "a.txt", "one\n", "first").await;
        runner
            .run_write(&repo.path, &["branch", "feature"], 10)
            .await
            .unwrap();

        assert!(is_branch_merged(&runner, &repo.path, "feature")
            .await
            .unwrap());
        let outcome = delete_branch(&runner, &repo.path, "feature", false)
            .await
            .unwrap();
        assert_eq!(outcome, BranchDeleteOutcome::Deleted);
        assert!(
            !list_branches(&runner, &repo.path)
                .await
                .unwrap()
                .iter()
                .any(|branch| branch.name == "feature"),
            "the merged branch should be gone"
        );
    }

    #[tokio::test]
    async fn delete_branch_offers_force_only_for_unmerged_work() {
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;
        let default = runner
            .run_read(&repo.path, &["symbolic-ref", "--short", "HEAD"])
            .await
            .unwrap()
            .stdout_string()
            .trim()
            .to_string();
        repo.commit(&runner, "a.txt", "one\n", "first").await;

        runner
            .run_write(&repo.path, &["checkout", "-b", "feature"], 10)
            .await
            .unwrap();
        repo.commit(&runner, "b.txt", "two\n", "unmerged work")
            .await;
        runner
            .run_write(&repo.path, &["checkout", &default], 10)
            .await
            .unwrap();

        assert!(
            !is_branch_merged(&runner, &repo.path, "feature")
                .await
                .unwrap(),
            "feature carries a commit outside the default branch"
        );

        // The first delete is the normal `-d`: git refuses, the branch is kept
        // and the caller is told why so it can ask again explicitly.
        let outcome = delete_branch(&runner, &repo.path, "feature", false)
            .await
            .unwrap();
        assert_eq!(outcome, BranchDeleteOutcome::NotMerged);
        assert!(
            list_branches(&runner, &repo.path)
                .await
                .unwrap()
                .iter()
                .any(|branch| branch.name == "feature"),
            "a refused delete must not remove the branch"
        );

        // Only the confirmed retry uses `-D`.
        let outcome = delete_branch(&runner, &repo.path, "feature", true)
            .await
            .unwrap();
        assert_eq!(outcome, BranchDeleteOutcome::Deleted);
        assert!(!list_branches(&runner, &repo.path)
            .await
            .unwrap()
            .iter()
            .any(|branch| branch.name == "feature"));
    }

    #[tokio::test]
    async fn sync_push_refuses_a_detached_head() {
        let runner = GitRunner::new();
        let repo = TempTestRepo::new(&runner).await;
        repo.commit(&runner, "a.txt", "one\n", "first").await;
        runner
            .run_write(&repo.path, &["checkout", "--detach", "HEAD"], 10)
            .await
            .unwrap();

        let error = sync_push(&runner, &repo.path, true, None)
            .await
            .expect_err("a detached HEAD has no branch to publish");
        assert_eq!(error.code, "DetachedHead");
        assert!(
            error.message.contains("Check out a branch first"),
            "the error must tell the user how to recover: {}",
            error.message
        );
    }
}
