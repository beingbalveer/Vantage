//! Detail queries (branches, commits, working tree, identity, remote) for the
//! Vantage backend.
//!
//! The backend owns a tokio runtime and calls these synchronously from the UI
//! thread, so every async read runs on that runtime via `block_on`. All git
//! access goes through Vantage's read-only git layer (`vantage_core::git`)
//! instead of a second front end-specific implementation.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::model::{CommitIdentity, PreviewKind, PreviewLine};

use vantage_core::git::runner::GitOutput;
use vantage_core::git::stash;
use vantage_core::git::status::StatusRecord;
use vantage_core::git::{diff, log, refs};
use vantage_core::{repos, AppError, GitRunner};

use super::{now_seconds, relative_time_label};
use crate::model::{BranchInfo, ChangedFile, CommitEntry, FileChangeStatus};

/// Commits listed per branch panel.
const COMMIT_LIMIT: u32 = 15;
/// Patch lines kept as a changed file's preview.
///
/// Generous enough to show whole hunks with their context, bounded so a
/// thousand-line rewrite cannot be put into the row model.
const PREVIEW_LINES: usize = 200;
/// Largest patch read for one file's preview. A scan never reads patches at
/// all; this only bounds the on-demand read when a file row is expanded.
const MAX_PATCH_BYTES: usize = 512 * 1024;
const UNKNOWN_AUTHOR: &str = "unknown";

/// Everything the inspector needs but an overview row does not.
///
/// The dashboard scans every repository; these reads (branch list, history,
/// stashes, identity, remote) are paid only for the selected repository.
#[derive(Clone, Debug)]
pub struct RepoDetails {
    pub has_remote: bool,
    pub stash_count: usize,
    pub author: Option<CommitIdentity>,
    pub branches: Vec<BranchInfo>,
    pub recent_commits: Vec<CommitEntry>,
    /// Commits on HEAD that no remote has; drives the local-only marker.
    pub local_only_commits: HashSet<String>,
}

/// Load the selected repository's inspector data in one background job.
pub fn repo_details(
    rt: &tokio::runtime::Runtime,
    runner: &GitRunner,
    path: &Path,
    current_branch: &str,
    has_head: bool,
) -> Result<RepoDetails, String> {
    let default_branch = rt.block_on(repos::default_branch(runner, path));
    let branches = branches(rt, runner, path, current_branch, &default_branch)?;
    // An unborn branch has no history to read; `git log` would fail here.
    let recent_commits = if has_head {
        // "detached" is a display label; `git log` must be asked for HEAD.
        let commit_target = if current_branch == "detached" {
            "HEAD"
        } else {
            current_branch
        };
        commits(rt, runner, path, commit_target)?
    } else {
        Vec::new()
    };
    let stash_count = stash_count(rt, runner, path);
    let author = commit_identity(path)?;
    let has_remote = has_remote(path)?;
    let local_only_commits = if has_remote {
        local_only_commits(rt, runner, path, None, has_remote)
    } else {
        HashSet::new()
    };

    Ok(RepoDetails {
        has_remote,
        stash_count,
        author,
        branches,
        recent_commits,
        local_only_commits,
    })
}

// ---------------- Branches ----------------

/// Every local branch, current first and then newest tip first.
pub fn branches(
    rt: &tokio::runtime::Runtime,
    runner: &GitRunner,
    path: &Path,
    current: &str,
    default: &str,
) -> Result<Vec<BranchInfo>, String> {
    let listed = rt
        .block_on(refs::list_branches(runner, path))
        .map_err(|error| error.message)?;
    let tips = rt.block_on(branch_tips(runner, path))?;
    let now = now_seconds();

    let mut seen: HashSet<String> = HashSet::new();
    let mut snapshots: Vec<(BranchInfo, i64)> = Vec::new();
    for branch in listed {
        let name = branch.name.trim().to_string();
        // `BranchInfo` models local branches: a remote-tracking ref would list a
        // branch twice and can never be checked out by that name.
        if name.is_empty()
            || name.starts_with("remotes/")
            || name.starts_with("origin/")
            || name.ends_with("/HEAD")
            || !seen.insert(name.clone())
        {
            continue;
        }

        let tip = tips.get(&name);
        let seconds = tip.and_then(|tip| tip.seconds).unwrap_or(0);
        snapshots.push((
            BranchInfo {
                is_current: branch.is_head || name == current,
                is_default: name == default,
                last_commit_time: tip
                    .and_then(|tip| tip.seconds)
                    .map(|seconds| relative_time_label(now, seconds))
                    .unwrap_or_default(),
                name,
            },
            seconds,
        ));
    }

    snapshots.sort_by(|a, b| {
        b.0.is_current
            .cmp(&a.0.is_current)
            .then_with(|| b.1.cmp(&a.1))
    });
    Ok(snapshots.into_iter().map(|(info, _)| info).collect())
}

struct BranchTip {
    /// Seconds since the epoch of the branch tip, `None` for an unborn branch.
    seconds: Option<i64>,
}

const BRANCH_TIP_FORMAT: &str = "%(refname:short)\x1f%(committerdate:unix)";

/// Ahead/behind and tip-commit metadata for every local branch in one call.
/// `state::BranchInfo` (what `refs::list_branches` returns) only carries the
/// ref name and its committer date, so the remaining fields come from here.
async fn branch_tips(
    runner: &GitRunner,
    repo_path: &Path,
) -> Result<HashMap<String, BranchTip>, String> {
    let format_arg = format!("--format={BRANCH_TIP_FORMAT}");
    let out = runner
        .run_read(repo_path, &["for-each-ref", &format_arg, "refs/heads"])
        .await
        .map_err(|error| error.message)?;
    if !out.success() {
        return Err(out.stderr);
    }

    let mut tips = HashMap::new();
    for line in out.stdout_string().lines() {
        let fields: Vec<&str> = line.split('\x1f').collect();
        let (Some(name), Some(seconds)) = (fields.first(), fields.last()) else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        tips.insert(
            name.to_string(),
            BranchTip {
                seconds: seconds.trim().parse::<i64>().ok(),
            },
        );
    }
    Ok(tips)
}

// ---------------- Commits ----------------

/// The most recent commits of `branch` (the checked-out branch when empty).
pub fn commits(
    rt: &tokio::runtime::Runtime,
    runner: &GitRunner,
    path: &Path,
    branch: &str,
) -> Result<Vec<CommitEntry>, String> {
    let target = if branch.trim().is_empty() {
        "HEAD"
    } else {
        branch.trim()
    };
    let page = rt
        .block_on(log::get_log_page(
            runner,
            path,
            Some(target),
            0,
            COMMIT_LIMIT,
            None,
            None,
        ))
        .map_err(|error| error.message)?;
    let now = now_seconds();

    Ok(page
        .into_iter()
        .take(COMMIT_LIMIT as usize)
        .map(|commit| commit_entry(commit, now))
        .collect())
}

/// Map the engine's commit summary to the row/composer model, keeping the body
/// so amend can restore the full original message and the full SHA so history
/// can be matched against the upstream range.
fn commit_entry(commit: vantage_core::state::CommitSummary, now: i64) -> CommitEntry {
    CommitEntry {
        sha: short_sha(&commit.sha, &commit.short),
        full_sha: commit.sha,
        message: commit.subject,
        body: commit.body,
        author: if commit.author_name.trim().is_empty() {
            commit.author_email.clone()
        } else {
            commit.author_name
        },
        author_email: commit.author_email,
        relative_time: relative_iso_label(now, &commit.date),
    }
}

/// Full SHAs of commits reachable from `HEAD` that the upstream does not have.
///
/// When the checked-out branch tracks nothing, every remote is used instead, so
/// a commit pushed to any remote is not called local. A repository without a
/// remote has no push target and gets an empty set. A failure (unborn `HEAD`,
/// deleted upstream ref) is treated as "nothing known" rather than guessing
/// from indices, so the UI never labels a pushed commit as local.
pub fn local_only_commits(
    rt: &tokio::runtime::Runtime,
    runner: &GitRunner,
    path: &Path,
    upstream: Option<&str>,
    has_remote: bool,
) -> HashSet<String> {
    if !has_remote {
        return HashSet::new();
    }

    let mut args = vec!["rev-list", "HEAD", "--not"];
    match upstream.map(str::trim).filter(|name| !name.is_empty()) {
        Some(upstream) => args.push(upstream),
        None => args.push("--remotes"),
    }

    match rt.block_on(runner.run_read(path, &args)) {
        Ok(out) if out.success() => out
            .stdout_string()
            .lines()
            .map(str::trim)
            .filter(|sha| !sha.is_empty())
            .map(str::to_string)
            .collect(),
        _ => HashSet::new(),
    }
}

fn short_sha(sha: &str, short: &str) -> String {
    if short.trim().is_empty() {
        sha.chars().take(7).collect()
    } else {
        short.to_string()
    }
}

// ---------------- Working tree ----------------

/// Unstaged, staged, untracked and unmerged paths of the working tree.
///
/// `records` come from the same `git status` call that produced the row's
/// branch and counts, so the scan does not query the working tree a second
/// time. Patch previews are deliberately absent: a scan only needs counts, and
/// [`file_preview`] reads one file's diff when the user expands it.
pub fn changed_files(
    rt: &tokio::runtime::Runtime,
    runner: &GitRunner,
    path: &Path,
    records: &[StatusRecord],
) -> Vec<ChangedFile> {
    if records.is_empty() {
        return Vec::new();
    }
    // An untracked-only tree has no `git diff` to numstat; skip both calls.
    let totals = if records.iter().all(|record| record.index == '?') {
        HashMap::new()
    } else {
        rt.block_on(numstat_totals(runner, path))
    };
    changed_files_from_records(records, &totals)
}

/// Map parsed status records onto the row model, adding numstat totals.
fn changed_files_from_records(
    records: &[StatusRecord],
    totals: &HashMap<String, (u32, u32)>,
) -> Vec<ChangedFile> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut files: Vec<ChangedFile> = Vec::new();
    for record in records {
        if !seen.insert(record.path.clone()) {
            continue;
        }
        let (status, staged) = changed_file_state(record);
        let (additions, deletions) = totals.get(&record.path).copied().unwrap_or((0, 0));
        files.push(ChangedFile {
            path: record.path.clone(),
            status,
            additions,
            deletions,
            staged,
            orig_path: record.orig_path.clone(),
            diff_preview: Vec::new(),
            preview_truncated: false,
        });
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));
    files
}

/// `(status badge, staged)` for one porcelain record.
fn changed_file_state(record: &StatusRecord) -> (FileChangeStatus, bool) {
    if record.unmerged {
        return (FileChangeStatus::Conflicted, false);
    }
    let staged = record.index != '.' && record.index != '?';
    let code = if staged {
        record.index
    } else {
        record.worktree
    };
    (status_from_code(code), staged)
}

fn status_from_code(code: char) -> FileChangeStatus {
    match code {
        'A' => FileChangeStatus::Added,
        'D' => FileChangeStatus::Deleted,
        'U' => FileChangeStatus::Conflicted,
        '?' => FileChangeStatus::Untracked,
        // M, plus R/C/T which `ChangedFile` has no slot for.
        _ => FileChangeStatus::Modified,
    }
}

/// Added/deleted line counts per path, summing the staged (HEAD→index) and
/// worktree (index→worktree) numstats. Untracked paths are absent from both.
async fn numstat_totals(runner: &GitRunner, repo_path: &Path) -> HashMap<String, (u32, u32)> {
    let mut totals: HashMap<String, (u32, u32)> = HashMap::new();
    for args in [
        vec!["diff", "--numstat", "-M", "-z"],
        vec!["diff", "--numstat", "-M", "-z", "--cached"],
    ] {
        let out = match runner.run_read(repo_path, &args).await {
            Ok(out) if out.success() => out,
            _ => continue,
        };
        for change in diff::parse_numstat_z(&out.stdout) {
            let total = totals.entry(change.path).or_insert((0, 0));
            total.0 = total.0.saturating_add(change.additions);
            total.1 = total.1.saturating_add(change.deletions);
        }
    }
    totals
}

/// One preview per file in a `git diff` patch, keyed by the path after the
/// change. A block starts at `diff --git ` (or `diff --cc ` for unmerged
/// paths); its `+++` header names the new path (`b/...`), except for deletions
/// where it is `/dev/null` and the `---` header names the removed path
/// (`a/...`). Blocks without file headers, such as binary or mode-only
/// changes, contribute no entry.
fn parse_diff_previews(patch: &str) -> HashMap<String, Vec<PreviewLine>> {
    let mut previews = HashMap::new();
    let mut block: Vec<&str> = Vec::new();

    for line in patch.lines() {
        let starts_block = line.starts_with("diff --git ") || line.starts_with("diff --cc ");
        if starts_block {
            insert_block_preview(&mut previews, &block);
            block.clear();
        }
        if !block.is_empty() || starts_block {
            block.push(line);
        }
    }
    insert_block_preview(&mut previews, &block);
    previews
}

fn insert_block_preview(previews: &mut HashMap<String, Vec<PreviewLine>>, block: &[&str]) {
    let Some(path) = block_path(block) else {
        return;
    };
    let (lines, _truncated) = preview_lines(&block.join("\n"));
    previews.insert(path, lines);
}

/// Path a patch block changes, from its `+++`/`---` headers before the first
/// hunk. `None` for blocks that only change metadata.
fn block_path(block: &[&str]) -> Option<String> {
    let mut new_path = None;
    let mut old_path = None;
    for line in block {
        if line.starts_with("@@") {
            break;
        }
        if let Some(path) = line.strip_prefix("+++ ") {
            new_path = Some(path.trim());
        } else if let Some(path) = line.strip_prefix("--- ") {
            old_path = Some(path.trim());
        }
    }

    let (path, prefix) = match new_path {
        Some(path) if path != "/dev/null" => (unquote_path(path.trim()), "b/"),
        _ => (unquote_path(old_path?.trim()), "a/"),
    };
    let path = path.strip_prefix(prefix).unwrap_or(path.as_str());
    if path.is_empty() || path == "/dev/null" {
        return None;
    }
    Some(path.to_string())
}

/// Undo git's C-style quoting for paths with control characters or quotes.
/// `GitRunner` disables `core.quotePath`, so only these escapes can appear.
fn unquote_path(path: &str) -> String {
    if !(path.len() >= 2 && path.starts_with('"') && path.ends_with('"')) {
        return path.to_string();
    }
    let mut unquoted = String::new();
    let mut chars = path[1..path.len() - 1].chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            unquoted.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => unquoted.push('\n'),
            Some('t') => unquoted.push('\t'),
            Some('r') => unquoted.push('\r'),
            Some('"') => unquoted.push('"'),
            Some('\\') => unquoted.push('\\'),
            Some(other) => {
                unquoted.push('\\');
                unquoted.push(other);
            }
            None => break,
        }
    }
    unquoted
}

/// Turn one file's patch into the lines the row renders.
///
/// File metadata (`diff --git`, `index`, `---`, `+++`, mode and rename lines) is
/// dropped: it says nothing about the change and used to fill the whole preview.
/// Hunk headers and context lines are kept, and both line numbers are tracked so
/// the view can draw a real gutter.
fn preview_lines(patch: &str) -> (Vec<PreviewLine>, bool) {
    let mut preview: Vec<PreviewLine> = Vec::new();
    // (next old line, next new line) from the current `@@` header.
    let mut old_number: Option<u32> = None;
    let mut new_number: Option<u32> = None;

    let mut truncated = false;
    for line in patch.lines() {
        if preview.len() == PREVIEW_LINES {
            truncated = true;
            break;
        }

        if line.starts_with("@@") {
            let (old_start, new_start) = hunk_starts(line);
            old_number = old_start;
            new_number = new_start;
            preview.push(PreviewLine {
                kind: PreviewKind::Hunk,
                old_line: None,
                new_line: None,
                text: line.to_string(),
            });
            continue;
        }

        // Anything before the first hunk is file metadata.
        let inside_hunk = old_number.is_some() || new_number.is_some();
        if !inside_hunk || is_patch_metadata(line) {
            continue;
        }

        let (kind, old_line, new_line, text) = match line.chars().next() {
            Some('+') => {
                let text = line[1..].to_string();
                let line_no = new_number;
                new_number = new_number.map(|n| n + 1);
                (PreviewKind::Added, None, line_no, text)
            }
            Some('-') => {
                let text = line[1..].to_string();
                let line_no = old_number;
                old_number = old_number.map(|n| n + 1);
                (PreviewKind::Removed, line_no, None, text)
            }
            Some(' ') => {
                let text = line[1..].to_string();
                let (old_line, new_line) = (old_number, new_number);
                old_number = old_number.map(|n| n + 1);
                new_number = new_number.map(|n| n + 1);
                (PreviewKind::Context, old_line, new_line, text)
            }
            // `\ No newline at end of file` and anything else git adds.
            _ => continue,
        };

        preview.push(PreviewLine {
            kind,
            old_line,
            new_line,
            text,
        });
    }

    (preview, truncated)
}

/// The `(old, new)` first line of a `@@ -a,b +c,d @@` header.
fn hunk_starts(header: &str) -> (Option<u32>, Option<u32>) {
    let mut old = None;
    let mut new = None;
    for token in header.split_whitespace() {
        if let Some(rest) = token.strip_prefix('-') {
            old = parse_start(rest);
        } else if let Some(rest) = token.strip_prefix('+') {
            new = parse_start(rest);
        }
        if old.is_some() && new.is_some() {
            break;
        }
    }
    (old, new)
}

fn parse_start(range: &str) -> Option<u32> {
    let start = range.split(',').next()?;
    start.parse().ok()
}

/// File-level patch lines that carry no change content.
fn is_patch_metadata(line: &str) -> bool {
    const PREFIXES: [&str; 12] = [
        "diff --git ",
        "index ",
        "--- ",
        "+++ ",
        "new file mode ",
        "deleted file mode ",
        "old mode ",
        "new mode ",
        "similarity index ",
        "rename from ",
        "rename to ",
        "Binary files ",
    ];
    PREFIXES.iter().any(|prefix| line.starts_with(prefix))
}

/// Preview for a path git has never tracked: the file's own first lines, shown
/// as additions, so a new file can be read here instead of in an editor.
fn untracked_preview(repo_path: &Path, relative: &str) -> (Vec<PreviewLine>, bool) {
    /// Anything bigger is not a review-friendly preview.
    const MAX_BYTES: u64 = 64 * 1024;

    let full = repo_path.join(relative);
    let Ok(meta) = std::fs::metadata(&full) else {
        return (Vec::new(), false);
    };
    if meta.len() > MAX_BYTES {
        return (Vec::new(), true);
    }
    let Ok(bytes) = std::fs::read(&full) else {
        return (Vec::new(), false);
    };
    // A binary file has no readable lines; the row keeps its "no preview" text.
    let Ok(text) = String::from_utf8(bytes) else {
        return (Vec::new(), false);
    };

    let mut preview = Vec::new();
    let mut truncated = false;
    for (index, line) in text.lines().enumerate() {
        if preview.len() == PREVIEW_LINES {
            truncated = true;
            break;
        }
        preview.push(PreviewLine {
            kind: PreviewKind::Added,
            old_line: None,
            new_line: Some(index as u32 + 1),
            text: line.to_string(),
        });
    }
    (preview, truncated)
}

/// Patch lines for one changed file, read when the user expands its row.
///
/// A scan only needs counts, so it never reads a repository's full diff. This
/// asks git for just the selected path (plus its rename source, when there is
/// one) and stops at [`MAX_PATCH_BYTES`], so a huge generated file cannot put
/// an unbounded patch in memory. Untracked files have no diff and are read
/// from disk instead.
pub fn file_preview(
    rt: &tokio::runtime::Runtime,
    runner: &GitRunner,
    repo_path: &Path,
    relative: &str,
    staged: bool,
    status: FileChangeStatus,
    orig_path: Option<&str>,
) -> anyhow::Result<(Vec<PreviewLine>, bool)> {
    if matches!(status, FileChangeStatus::Untracked) {
        return Ok(untracked_preview(repo_path, relative));
    }

    let mut args = vec!["diff", "--no-color", "--no-ext-diff", "-M"];
    if staged {
        args.push("--cached");
    }
    args.push("--");
    args.push(relative);
    if let Some(orig_path) = orig_path {
        args.push(orig_path);
    }

    let limited = rt
        .block_on(runner.run_read_literal_limited(repo_path, &args, MAX_PATCH_BYTES))
        .map_err(|error| anyhow::anyhow!(error.message))?;
    if !limited.output.success() && !limited.truncated {
        let stderr = limited.output.stderr.trim();
        return Err(anyhow::anyhow!(if stderr.is_empty() {
            format!("git could not produce a patch for {relative}")
        } else {
            stderr.to_string()
        }));
    }

    let (lines, line_truncated) = preview_lines(&limited.output.stdout_string());
    Ok((lines, line_truncated || limited.truncated))
}

// ---------------- Compare + ignored ----------------

/// A read that ran but exited unsuccessfully, with Git's own explanation.
///
/// The runner returns `Ok` for any command git actually started, including a
/// non-zero exit, so callers must turn a failed status into a visible error
/// instead of the empty output looking like "nothing changed".
fn failed_read(action: &str, output: &GitOutput) -> AppError {
    let stderr = output.stderr.trim();
    let message = if stderr.is_empty() {
        format!("git {action} exited with {}.", output.status)
    } else {
        stderr.to_string()
    };
    AppError::other(message).with_stderr(output.stderr.clone())
}

/// Files between two branches, with the same preview shape a commit uses.
///
/// An unknown branch or invalid range is returned as an error carrying Git's
/// message, never as an empty file list.
pub fn compare_branches(
    rt: &tokio::runtime::Runtime,
    runner: &GitRunner,
    path: &Path,
    base: &str,
    head: &str,
) -> Result<Vec<crate::model::CommitFile>, AppError> {
    let range = format!("{base}...{head}");
    let patch = rt.block_on(async {
        runner
            .run_read(path, &["diff", "--no-color", "--no-ext-diff", "-M", &range])
            .await
    })?;
    if !patch.success() {
        return Err(failed_read("diff", &patch));
    }
    let patch = patch.stdout_string();

    let counts = rt.block_on(async {
        runner
            .run_read(path, &["diff", "--numstat", "-M", "-z", &range])
            .await
    });
    let mut by_path: HashMap<String, (u32, u32)> = HashMap::new();
    if let Ok(out) = counts {
        if out.success() {
            for change in vantage_core::git::diff::parse_numstat_z(&out.stdout) {
                by_path.insert(change.path, (change.additions, change.deletions));
            }
        }
    }

    let mut files: Vec<crate::model::CommitFile> = parse_diff_previews(&patch)
        .into_iter()
        .map(|(path, lines)| {
            let (additions, deletions) = by_path.get(&path).copied().unwrap_or((
                lines
                    .iter()
                    .filter(|l| l.kind == PreviewKind::Added)
                    .count() as u32,
                lines
                    .iter()
                    .filter(|l| l.kind == PreviewKind::Removed)
                    .count() as u32,
            ));
            let truncated = lines.len() == PREVIEW_LINES;
            crate::model::CommitFile {
                path,
                additions,
                deletions,
                lines,
                truncated,
            }
        })
        .collect();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

// ---------------- Stashes ----------------

/// The stash list shown by the stash dialog.
///
/// A failed read stays an error so the dialog can show Git's message and offer
/// a retry; an empty repository stash list is a successful empty vector.
pub fn stash_entries(
    rt: &tokio::runtime::Runtime,
    runner: &GitRunner,
    path: &Path,
) -> Result<Vec<vantage_core::state::StashEntry>, AppError> {
    rt.block_on(stash::list_stashes(runner, path))
}

/// How many stashes the repository keeps; the menu labels use the count.
pub fn stash_count(rt: &tokio::runtime::Runtime, runner: &GitRunner, path: &Path) -> usize {
    rt.block_on(stash::list_stashes(runner, path))
        .map(|entries| entries.len())
        .unwrap_or(0)
}

// ---------------- One commit ----------------

/// The files a commit touched, each with its own patch lines and counts.
///
/// `git show` gives the patch with the same shape as a working-tree diff, so it
/// reuses the parser the Changes tab already relies on. A missing commit or
/// other Git failure is an error carrying Git's message.
pub fn commit_files(
    rt: &tokio::runtime::Runtime,
    runner: &GitRunner,
    path: &Path,
    sha: &str,
) -> Result<Vec<crate::model::CommitFile>, AppError> {
    let patch = rt.block_on(async {
        runner
            .run_read(
                path,
                &[
                    "show",
                    "--no-color",
                    "--no-ext-diff",
                    "-M",
                    "--format=",
                    sha,
                ],
            )
            .await
    })?;
    if !patch.success() {
        return Err(failed_read("show", &patch));
    }
    let patch = patch.stdout_string();

    let counts = rt.block_on(async {
        runner
            .run_read(path, &["show", "--numstat", "-M", "--format=", "-z", sha])
            .await
    });
    let mut by_path: HashMap<String, (u32, u32)> = HashMap::new();
    if let Ok(out) = counts {
        if out.success() {
            for change in vantage_core::git::diff::parse_numstat_z(&out.stdout) {
                by_path.insert(change.path, (change.additions, change.deletions));
            }
        }
    }

    let mut files: Vec<crate::model::CommitFile> = parse_diff_previews(&patch)
        .into_iter()
        .map(|(path, lines)| {
            let (additions, deletions) = by_path.get(&path).copied().unwrap_or((
                lines
                    .iter()
                    .filter(|l| l.kind == PreviewKind::Added)
                    .count() as u32,
                lines
                    .iter()
                    .filter(|l| l.kind == PreviewKind::Removed)
                    .count() as u32,
            ));
            let truncated = lines.len() == PREVIEW_LINES;
            crate::model::CommitFile {
                path,
                additions,
                deletions,
                lines,
                truncated,
            }
        })
        .collect();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

// ---------------- Commit identity ----------------

/// The identity git will use for the next commit in `path`.
///
/// Like [`remote`], this is a single cheap config read done synchronously: the
/// dashboard needs it for the commit bar of the selected repository. `git config
/// --get-regexp` already resolves precedence, so a repository-local `user.name`
/// beats the global one.
pub fn commit_identity(path: &Path) -> Result<Option<CommitIdentity>, String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["config", "--get-regexp", "^user\\.(name|email)$"])
        .env("LC_ALL", "C")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|error| format!("read the Git identity: {error}"))?;
    if !output.status.success() {
        return Ok(None);
    }

    let mut name = None;
    let mut email = None;
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Some((key, value)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        match key {
            "user.name" => name = Some(value.to_string()),
            "user.email" => email = Some(value.to_string()),
            _ => {}
        }
    }

    Ok(match (name, email) {
        (Some(name), Some(email)) => Some(CommitIdentity { name, email }),
        _ => None,
    })
}

// ---------------- Remote ----------------

/// Whether the repository has an `origin` to fetch, pull and push against.
///
/// The URL itself is not part of the dashboard — the action bar names the branch
/// and nothing else — so this is the only thing the UI needs from the remote.
/// Like the other cheap config reads here it asks git directly, so worktrees and
/// `include`d configs resolve correctly.
pub fn has_remote(path: &Path) -> Result<bool, String> {
    Ok(origin_url(path)?.is_some())
}

fn origin_url(repo_path: &Path) -> Result<Option<String>, String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["config", "--get", "remote.origin.url"])
        .env("LC_ALL", "C")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .map_err(|error| format!("read the remote: {error}"))?;

    if !output.status.success() {
        return Ok(None);
    }
    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok((!url.is_empty()).then_some(url))
}

// ---------------- Relative time ----------------

/// Labels for git's `%aI` timestamps (`2024-05-06T07:08:09+02:00`).
fn relative_iso_label(now: i64, stamp: &str) -> String {
    match chrono::DateTime::parse_from_rfc3339(stamp) {
        Ok(moment) => relative_time_label(now, moment.timestamp()),
        Err(_) => UNKNOWN_AUTHOR.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_entries_keep_the_body_for_amend() {
        let entry = commit_entry(
            vantage_core::state::CommitSummary {
                sha: "0123456789abcdef".to_string(),
                short: "0123456".to_string(),
                author_name: "QA".to_string(),
                author_email: "qa@example.com".to_string(),
                date: "2026-11-16T10:00:00Z".to_string(),
                subject: "subject line".to_string(),
                body: "First body line.\n\nSecond paragraph.".to_string(),
                parents: Vec::new(),
                files_changed: 1,
                additions: 2,
                deletions: 0,
                is_new: false,
            },
            0,
        );

        assert_eq!(entry.message, "subject line");
        assert_eq!(entry.body, "First body line.\n\nSecond paragraph.");
    }

    #[test]
    fn relative_labels_follow_the_dashboard_scale() {
        let now = 1_700_000_000;
        assert_eq!(relative_time_label(now, now), "just now");
        assert_eq!(relative_time_label(now, now - 30), "just now");
        assert_eq!(relative_time_label(now, now - 14 * 60), "14m ago");
        assert_eq!(relative_time_label(now, now - 3 * 3600), "3h ago");
        assert_eq!(relative_time_label(now, now - 2 * 86_400), "2d ago");
        assert_eq!(relative_time_label(now, now - 9 * 86_400), "1w ago");
        assert_eq!(relative_time_label(now, now - 95 * 86_400), "3mo ago");
        assert_eq!(relative_time_label(now, now - 400 * 86_400), "1y ago");
    }

    #[test]
    fn relative_labels_parse_iso_timestamps() {
        let now = 1_700_000_000;
        let stamp = chrono::DateTime::from_timestamp(now - 120, 0)
            .unwrap()
            .to_rfc3339();
        assert_eq!(relative_iso_label(now, &stamp), "2m ago");
        assert_eq!(relative_iso_label(now, ""), UNKNOWN_AUTHOR);
    }

    #[test]
    fn status_records_map_to_changed_file_states() {
        let output = b"# branch.head main\0\
1 M. N... 100644 100644 100644 aaaaaaa bbbbbbb staged.rs\0\
1 .M N... 100644 100644 100644 aaaaaaa bbbbbbb worktree.rs\0\
1 A. N... 000000 100644 100644 0000000 bbbbbbb added.rs\0\
1 .D N... 100644 100644 000000 aaaaaaa 0000000 deleted.rs\0\
2 R. N... 100644 100644 100644 aaaaaaa bbbbbbb R100 renamed.rs\0old.rs\0\
u UU N... 100644 100644 100644 100644 aaaaaaa bbbbbbb ccccccc conflicted.rs\0\
? untracked file.rs\0";

        let parsed = vantage_core::git::status::parse_porcelain_v2(output);
        let entries = changed_files_from_records(&parsed.records, &HashMap::new());
        let find = |path: &str| {
            entries
                .iter()
                .find(|entry| entry.path == path)
                .unwrap_or_else(|| panic!("missing {path}"))
        };

        let staged = find("staged.rs");
        assert_eq!(staged.status, FileChangeStatus::Modified);
        assert!(staged.staged);

        let worktree = find("worktree.rs");
        assert_eq!(worktree.status, FileChangeStatus::Modified);
        assert!(!worktree.staged);

        assert_eq!(find("added.rs").status, FileChangeStatus::Added);
        assert_eq!(find("deleted.rs").status, FileChangeStatus::Deleted);
        assert_eq!(find("renamed.rs").status, FileChangeStatus::Modified);
        assert_eq!(find("renamed.rs").orig_path.as_deref(), Some("old.rs"));
        assert_eq!(find("conflicted.rs").status, FileChangeStatus::Conflicted);
        let untracked = find("untracked file.rs");
        assert_eq!(untracked.status, FileChangeStatus::Untracked);
        assert!(!untracked.staged);

        assert!(
            entries.iter().all(|entry| entry.path != "old.rs"),
            "a rename's original path must not become its own row"
        );
        assert!(
            entries.iter().all(|entry| entry.diff_preview.is_empty()),
            "a scan must not read patch previews"
        );
    }

    #[test]
    fn preview_keeps_hunks_context_and_line_numbers() {
        // Built line by line: a `\` continuation would eat the leading space
        // that marks a context line.
        let patch = [
            "diff --git a/a.rs b/a.rs",
            "index 111..222 100644",
            "--- a/a.rs",
            "+++ b/a.rs",
            "@@ -10,3 +10,4 @@ fn main() {",
            " context before",
            "-old line",
            "+new line",
            "+added line",
            " context after",
            "\\ No newline at end of file",
        ]
        .join("\n");

        let (preview, truncated) = preview_lines(&patch);
        assert!(!truncated, "six lines fit well under the cap");
        assert_eq!(
            preview
                .iter()
                .map(|line| (line.kind, line.old_line, line.new_line, line.text.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (
                    PreviewKind::Hunk,
                    None,
                    None,
                    "@@ -10,3 +10,4 @@ fn main() {"
                ),
                (PreviewKind::Context, Some(10), Some(10), "context before"),
                (PreviewKind::Removed, Some(11), None, "old line"),
                (PreviewKind::Added, None, Some(11), "new line"),
                (PreviewKind::Added, None, Some(12), "added line"),
                (PreviewKind::Context, Some(12), Some(13), "context after"),
            ],
            "file metadata is dropped, context is kept and the gutter numbers follow the hunk"
        );
    }

    #[test]
    fn preview_is_bounded_for_huge_files() {
        let mut patch = String::from("--- a/a.rs\n+++ b/a.rs\n@@ -1,1 +1,400 @@\n");
        for index in 0..400 {
            patch.push_str(&format!("+line {index}\n"));
        }
        let (preview, truncated) = preview_lines(&patch);
        assert_eq!(preview.len(), PREVIEW_LINES);
        assert!(truncated, "the view needs to know it stopped early");
    }

    #[test]
    fn untracked_files_preview_their_own_lines_as_additions() {
        let dir = std::env::temp_dir().join(format!("vantage_untracked_{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("new.rs"), "fn one() {}\nfn two() {}\n").unwrap();

        let (preview, truncated) = untracked_preview(&dir, "new.rs");
        assert!(!truncated);
        assert_eq!(
            preview
                .iter()
                .map(|line| (line.kind, line.new_line, line.text.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (PreviewKind::Added, Some(1), "fn one() {}"),
                (PreviewKind::Added, Some(2), "fn two() {}"),
            ]
        );

        // A file that is not UTF-8 text has no readable preview.
        std::fs::write(dir.join("blob.bin"), [0xff, 0xfe, 0x00]).unwrap();
        assert!(untracked_preview(&dir, "blob.bin").0.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    fn git_available() -> bool {
        std::process::Command::new("git")
            .arg("--version")
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false)
    }

    fn git(dir: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .expect("run git");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    #[test]
    fn a_scan_keeps_counts_and_reads_the_patch_only_on_demand() {
        if !git_available() {
            eprintln!("skipping: git is not installed");
            return;
        }
        let dir = std::env::temp_dir().join(format!(
            "vantage_preview_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q", "-b", "main"]);
        git(&dir, &["config", "user.email", "qa@example.com"]);
        git(&dir, &["config", "user.name", "QA"]);
        std::fs::write(dir.join("app.rs"), "fn one() {}\n").unwrap();
        git(&dir, &["add", "."]);
        git(&dir, &["commit", "-q", "-m", "initial"]);
        std::fs::write(dir.join("app.rs"), "fn one() {}\nfn two() {}\n").unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let runner = GitRunner::new();
        let snapshot = rt.block_on(vantage_core::git::status::query_repo_status_snapshot(
            &runner,
            "repo-fixture",
            &dir,
            None,
            None,
        ));
        let files = changed_files(&rt, &runner, &dir, &snapshot.records);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "app.rs");
        assert_eq!(files[0].additions, 1);
        assert!(
            files[0].diff_preview.is_empty(),
            "the overview scan must not read the patch"
        );

        let (lines, truncated) = file_preview(
            &rt,
            &runner,
            &dir,
            &files[0].path,
            files[0].staged,
            files[0].status,
            files[0].orig_path.as_deref(),
        )
        .expect("read the file's patch when expanded");
        assert!(!truncated);
        assert!(
            lines
                .iter()
                .any(|line| line.kind == PreviewKind::Added && line.text == "fn two() {}"),
            "the expanded row shows the change it counted: {lines:?}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn diff_previews_split_blocks_by_path_and_include_deletions() {
        let patch = [
            "diff --git a/src/a.rs b/src/a.rs",
            "index 1111111..2222222 100644",
            "--- a/src/a.rs",
            "+++ b/src/a.rs",
            "@@ -1,2 +1,2 @@",
            "-old line",
            "+new line",
            " context",
            "diff --git a/gone.rs b/gone.rs",
            "deleted file mode 100644",
            "index 3333333..0000000",
            "--- a/gone.rs",
            "+++ /dev/null",
            "@@ -1 +0,0 @@",
            "-removed line",
            "diff --git a/src/b.rs b/src/b.rs",
            "index 4444444..5555555 100644",
            "--- a/src/b.rs",
            "+++ b/src/b.rs",
            "@@ -1 +1,2 @@",
            " keep",
            "+added line",
        ]
        .join("\n");

        let previews = parse_diff_previews(&patch);
        assert_eq!(previews.len(), 3);

        let modified = previews.get("src/a.rs").expect("modified file");
        assert!(modified
            .iter()
            .any(|line| line.kind == PreviewKind::Removed && line.text == "old line"));
        assert!(modified
            .iter()
            .any(|line| line.kind == PreviewKind::Added && line.text == "new line"));
        assert!(modified
            .iter()
            .any(|line| line.kind == PreviewKind::Context && line.text == "context"));
        assert!(
            modified.iter().all(|line| !line.text.starts_with("--- a/")),
            "file headers must not survive into the preview"
        );

        let deleted = previews.get("gone.rs").expect("deleted file");
        assert!(deleted
            .iter()
            .any(|line| line.kind == PreviewKind::Removed && line.text == "removed line"));
        assert!(!previews.contains_key("/dev/null"));

        let second = previews.get("src/b.rs").expect("second file in the patch");
        assert!(second
            .iter()
            .any(|line| line.kind == PreviewKind::Added && line.text == "added line"));
        assert!(second
            .iter()
            .any(|line| line.kind == PreviewKind::Context && line.text == "keep"));
    }

    fn git_at(dir: &Path, args: &[&str], date: Option<&str>) -> String {
        let mut command = std::process::Command::new("git");
        command.arg("-C").arg(dir).args(args);
        if let Some(date) = date {
            command.env("GIT_AUTHOR_DATE", date);
            command.env("GIT_COMMITTER_DATE", date);
        }
        let out = command.output().expect("run git");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn init_repo(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        git(dir, &["init", "-q", "-b", "main"]);
        git(dir, &["config", "user.name", "QA"]);
        git(dir, &["config", "user.email", "qa@example.com"]);
        git(dir, &["config", "commit.gpgsign", "false"]);
    }

    fn commit(dir: &Path, file: &str, contents: &str, message: &str, date: &str) -> String {
        std::fs::write(dir.join(file), contents).unwrap();
        git(dir, &["add", file]);
        git_at(dir, &["commit", "-q", "-m", message], Some(date));
        git(dir, &["rev-parse", "HEAD"])
    }

    /// Build a diverged repository and assert the local-only set comes from
    /// the upstream range, not from the first N rows of the newest-first log.
    #[test]
    fn local_only_commits_use_the_upstream_range_not_list_indices() {
        if !git_available() {
            eprintln!("skipping: git is not installed");
            return;
        }

        let root = std::env::temp_dir().join(format!("vantage_local_only_{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let origin = root.join("origin.git");
        let local = root.join("local");
        let peer = root.join("peer");

        std::fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "-q", "--bare"]);

        init_repo(&local);
        let base = commit(&local, "base.ts", "base\n", "base", "2024-01-01T00:00:00Z");
        git(
            &local,
            &["remote", "add", "origin", origin.to_str().unwrap()],
        );
        git(&local, &["push", "-q", "-u", "origin", "main"]);

        // Two local commits on top of the published base: the ahead-only case.
        let first = commit(
            &local,
            "local.ts",
            "local one\n",
            "local one",
            "2024-01-02T00:00:00Z",
        );
        let second = commit(
            &local,
            "local.ts",
            "local two\n",
            "local two",
            "2024-01-03T00:00:00Z",
        );

        let rt = tokio::runtime::Runtime::new().unwrap();
        let runner = GitRunner::new();
        let ahead = local_only_commits(&rt, &runner, &local, Some("origin/main"), true);
        assert_eq!(
            ahead,
            [first.clone(), second.clone()].into_iter().collect(),
            "ahead-only commits are exactly the upstream range"
        );

        // A peer pushes an upstream commit that local then merges. The merge
        // commit and the two local commits are absent from origin/main, and the
        // upstream commit sorts between them in the newest-first log.
        std::process::Command::new("git")
            .args(["clone", "-q", origin.to_str().unwrap()])
            .arg(&peer)
            .output()
            .expect("clone peer");
        git(&peer, &["config", "user.name", "Peer"]);
        git(&peer, &["config", "user.email", "peer@example.com"]);
        git(&peer, &["config", "commit.gpgsign", "false"]);
        let upstream = commit(
            &peer,
            "upstream.ts",
            "upstream\n",
            "upstream commit",
            "2024-01-04T00:00:00Z",
        );
        git(&peer, &["push", "-q", "origin", "main"]);

        git(&local, &["fetch", "-q", "origin"]);
        git(&local, &["merge", "-q", "--no-edit", "origin/main"]);
        let merge = git(&local, &["rev-parse", "HEAD"]);

        let diverged = local_only_commits(&rt, &runner, &local, Some("origin/main"), true);
        assert_eq!(
            diverged,
            [first.clone(), second.clone(), merge.clone()]
                .into_iter()
                .collect(),
            "only commits absent from the upstream are local"
        );
        assert!(
            !diverged.contains(&upstream) && !diverged.contains(&base),
            "the upstream commit and the shared base are pushed"
        );

        // The same repository with no tracked upstream falls back to every
        // remote; the range is unchanged because origin is the only remote.
        assert_eq!(
            local_only_commits(&rt, &runner, &local, None, true),
            diverged,
            "an untracked branch compares against all remotes"
        );
        assert!(
            local_only_commits(&rt, &runner, &local, None, false).is_empty(),
            "a repository without a remote has nothing to push"
        );

        // The log itself is non-contiguous: the upstream commit sits between
        // the merge and the older local commits.
        let commits = commits(&rt, &runner, &local, "main").unwrap();
        let position = |sha: &str| {
            commits
                .iter()
                .position(|commit| commit.full_sha == sha)
                .unwrap_or_else(|| panic!("missing {sha}"))
        };
        assert!(position(&merge) < position(&upstream));
        assert!(position(&upstream) < position(&second));
        assert!(position(&upstream) < position(&first));

        // An unborn repository has no HEAD to diff and no local-only commits.
        let unborn = root.join("unborn");
        init_repo(&unborn);
        assert!(local_only_commits(&rt, &runner, &unborn, None, true).is_empty());

        std::fs::remove_dir_all(&root).ok();
    }
}
