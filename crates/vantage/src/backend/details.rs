//! Detail queries (branches, commits, working tree, identity, remote) for the
//! Vantage backend.
//!
//! The backend owns a tokio runtime and calls these synchronously from the UI
//! thread, so every async read runs on that runtime via `block_on`. All git
//! access goes through Vantage's read-only git layer (`vantage_core::git`)
//! instead of a second front end-specific implementation.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::model::{CommitIdentity, PreviewKind, PreviewLine};

use vantage_core::git::{diff, log, refs, status};
use vantage_core::GitRunner;

use crate::model::{BranchInfo, ChangedFile, CommitEntry, FileChangeStatus};

/// Commits listed per branch panel.
const COMMIT_LIMIT: u32 = 15;
/// Patch lines kept as a changed file's preview.
///
/// Generous enough to show whole hunks with their context, bounded so a
/// thousand-line rewrite cannot be put into the row model.
const PREVIEW_LINES: usize = 200;
const UNKNOWN_AUTHOR: &str = "unknown";

// ---------------- Branches ----------------

/// Every local branch, current first and then newest tip first.
pub fn branches(
    rt: &tokio::runtime::Runtime,
    runner: &GitRunner,
    path: &Path,
    current: &str,
    default: &str,
) -> Vec<BranchInfo> {
    let listed = match rt.block_on(refs::list_branches(runner, path)) {
        Ok(list) => list,
        Err(_) => return Vec::new(),
    };
    let tips = rt.block_on(branch_tips(runner, path));
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
                    .map(|seconds| relative_label(now, seconds))
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
    snapshots.into_iter().map(|(info, _)| info).collect()
}

struct BranchTip {
    /// Seconds since the epoch of the branch tip, `None` for an unborn branch.
    seconds: Option<i64>,
}

const BRANCH_TIP_FORMAT: &str = "%(refname:short)\x1f%(committerdate:unix)";

/// Ahead/behind and tip-commit metadata for every local branch in one call.
/// `state::BranchInfo` (what `refs::list_branches` returns) only carries the
/// ref name and its committer date, so the remaining fields come from here.
async fn branch_tips(runner: &GitRunner, repo_path: &Path) -> HashMap<String, BranchTip> {
    let format_arg = format!("--format={BRANCH_TIP_FORMAT}");
    let out = match runner
        .run_read(repo_path, &["for-each-ref", &format_arg, "refs/heads"])
        .await
    {
        Ok(out) if out.success() => out,
        _ => return HashMap::new(),
    };

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
    tips
}

// ---------------- Commits ----------------

/// The most recent commits of `branch` (the checked-out branch when empty).
pub fn commits(
    rt: &tokio::runtime::Runtime,
    runner: &GitRunner,
    path: &Path,
    branch: &str,
) -> Vec<CommitEntry> {
    let target = if branch.trim().is_empty() {
        "HEAD"
    } else {
        branch.trim()
    };
    let page = match rt.block_on(log::get_log_page(
        runner,
        path,
        Some(target),
        0,
        COMMIT_LIMIT,
        None,
        None,
    )) {
        Ok(page) => page,
        Err(_) => return Vec::new(),
    };
    let now = now_seconds();

    page.into_iter()
        .take(COMMIT_LIMIT as usize)
        .map(|commit| CommitEntry {
            sha: short_sha(&commit.sha, &commit.short),
            message: commit.subject,
            author: if commit.author_name.trim().is_empty() {
                commit.author_email.clone()
            } else {
                commit.author_name
            },
            author_email: commit.author_email,
            relative_time: relative_iso_label(now, &commit.date),
        })
        .collect()
}

fn short_sha(sha: &str, short: &str) -> String {
    if short.trim().is_empty() {
        sha.chars().take(7).collect()
    } else {
        short.to_string()
    }
}

// ---------------- Working tree ----------------

struct StatusEntry {
    path: String,
    status: FileChangeStatus,
    /// True when the path has index (staged) changes.
    staged: bool,
}

/// Unstaged, staged, untracked and unmerged paths of the working tree.
pub fn changed_files(
    rt: &tokio::runtime::Runtime,
    runner: &GitRunner,
    path: &Path,
) -> Vec<ChangedFile> {
    let (entries, conflicts) = rt.block_on(status_snapshot(runner, path));
    if entries.is_empty() && conflicts.is_empty() {
        return Vec::new();
    }
    let totals = rt.block_on(numstat_totals(runner, path));
    let (unstaged_previews, staged_previews) = rt.block_on(diff_preview_maps(runner, path));

    let mut seen: HashSet<String> = HashSet::new();
    let mut files: Vec<ChangedFile> = Vec::new();
    for entry in entries {
        if !seen.insert(entry.path.clone()) {
            continue;
        }
        let state = if conflicts.contains(&entry.path) {
            FileChangeStatus::Conflicted
        } else {
            entry.status
        };
        let (additions, deletions) = totals.get(&entry.path).copied().unwrap_or((0, 0));
        let (diff_preview, preview_truncated) = if matches!(state, FileChangeStatus::Untracked) {
            // Untracked paths have no patch in `git diff`: show the file.
            untracked_preview(path, &entry.path)
        } else {
            let previews = if entry.staged {
                &staged_previews
            } else {
                &unstaged_previews
            };
            let lines = previews.get(&entry.path).cloned().unwrap_or_default();
            let truncated = lines.len() == PREVIEW_LINES;
            (lines, truncated)
        };
        files.push(ChangedFile {
            path: entry.path,
            status: state,
            additions,
            deletions,
            staged: entry.staged,
            diff_preview,
            preview_truncated,
        });
    }

    // A conflicted path always belongs in the list, even if the porcelain parse
    // surfaced no ordinary record for it.
    for conflicted in conflicts {
        if !seen.insert(conflicted.clone()) {
            continue;
        }
        let (additions, deletions) = totals.get(&conflicted).copied().unwrap_or((0, 0));
        files.push(ChangedFile {
            path: conflicted,
            status: FileChangeStatus::Conflicted,
            additions,
            deletions,
            staged: false,
            diff_preview: Vec::new(),
            preview_truncated: false,
        });
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));
    files
}

async fn status_snapshot(
    runner: &GitRunner,
    repo_path: &Path,
) -> (Vec<StatusEntry>, HashSet<String>) {
    let out = match runner
        .run_read(
            repo_path,
            &["status", "--porcelain=v2", "-z", "--untracked-files=all"],
        )
        .await
    {
        Ok(out) if out.success() => out,
        _ => return (Vec::new(), HashSet::new()),
    };

    let conflicts: HashSet<String> = status::parse_porcelain_v2(&out.stdout)
        .conflicted_paths
        .into_iter()
        .collect();
    (parse_status_entries(&out.stdout), conflicts)
}

/// Per-path records of `git status --porcelain=v2 -z`.
///
/// `status::parse_porcelain_v2` only reports counts and conflicts, so the
/// records are split here using the same NUL-separated layout:
/// `1 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <path>`,
/// `2 <XY> ... <X><score> <path>\0<origPath>`,
/// `u <XY> <sub> <m1> <m2> <m3> <mW> <h1> <h2> <h3> <path>`, `? <path>`.
fn parse_status_entries(output: &[u8]) -> Vec<StatusEntry> {
    let mut entries = Vec::new();
    let parts: Vec<&[u8]> = output.split(|&byte| byte == 0).collect();
    let mut i = 0;

    while i < parts.len() {
        let record = parts[i];
        i += 1;
        let Some(kind) = record.first() else {
            continue;
        };

        match kind {
            // Header and ignored records carry no file to list.
            b'#' | b'!' => {}
            b'?' => {
                if let Some(path) = record.get(2..).filter(|path| !path.is_empty()) {
                    entries.push(StatusEntry {
                        path: String::from_utf8_lossy(path).to_string(),
                        status: FileChangeStatus::Untracked,
                        staged: false,
                    });
                }
            }
            b'1' => {
                let line = String::from_utf8_lossy(record);
                let fields: Vec<&str> = line.splitn(9, ' ').collect();
                if fields.len() == 9 {
                    entries.push(entry_from_xy(fields[1], fields[8]));
                }
            }
            b'2' => {
                let line = String::from_utf8_lossy(record);
                let fields: Vec<&str> = line.splitn(10, ' ').collect();
                if fields.len() == 10 {
                    entries.push(entry_from_xy(fields[1], fields[9]));
                }
                // The original path of a rename/copy is its own record.
                i += 1;
            }
            b'u' => {
                let line = String::from_utf8_lossy(record);
                let fields: Vec<&str> = line.splitn(11, ' ').collect();
                if fields.len() == 11 {
                    entries.push(StatusEntry {
                        path: fields[10].to_string(),
                        status: FileChangeStatus::Conflicted,
                        staged: false,
                    });
                }
            }
            _ => {}
        }
    }

    entries
}

/// `XY` is index status followed by worktree status; `.` means "unchanged".
fn entry_from_xy(xy: &str, path: &str) -> StatusEntry {
    let mut codes = xy.chars();
    let index = codes.next().unwrap_or('.');
    let worktree = codes.next().unwrap_or('.');
    let staged = index != '.' && index != '?';
    let code = if staged { index } else { worktree };
    StatusEntry {
        path: path.to_string(),
        status: status_from_code(code),
        staged,
    }
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

/// Unstaged and staged previews for every path, each map from one `git diff`
/// run: `(worktree, index)`. Fetching them per file would cost one process per
/// changed path.
async fn diff_preview_maps(
    runner: &GitRunner,
    repo_path: &Path,
) -> (
    HashMap<String, Vec<PreviewLine>>,
    HashMap<String, Vec<PreviewLine>>,
) {
    (
        diff_preview_map(runner, repo_path, false).await,
        diff_preview_map(runner, repo_path, true).await,
    )
}

async fn diff_preview_map(
    runner: &GitRunner,
    repo_path: &Path,
    staged: bool,
) -> HashMap<String, Vec<PreviewLine>> {
    let mut args = vec!["diff", "--no-color", "--no-ext-diff"];
    if staged {
        args.push("--cached");
    }

    match runner.run_read(repo_path, &args).await {
        Ok(out) if out.success() => parse_diff_previews(&out.stdout_string()),
        _ => HashMap::new(),
    }
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

// ---------------- Compare + ignored ----------------

/// Files between two branches, with the same preview shape a commit uses.
pub fn compare_branches(
    rt: &tokio::runtime::Runtime,
    runner: &GitRunner,
    path: &Path,
    base: &str,
    head: &str,
) -> Vec<crate::model::CommitFile> {
    let range = format!("{base}...{head}");
    let patch = rt.block_on(async {
        runner
            .run_read(path, &["diff", "--no-color", "--no-ext-diff", "-M", &range])
            .await
    });
    let patch = match patch {
        Ok(out) if out.success() => out.stdout_string(),
        _ => return Vec::new(),
    };

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
    files
}

// ---------------- Stashes ----------------

/// How many stashes the repository keeps; the menu labels use the count.
pub fn stash_count(rt: &tokio::runtime::Runtime, runner: &GitRunner, path: &Path) -> usize {
    rt.block_on(vantage_core::git::stash::list_stashes(runner, path))
        .map(|entries| entries.len())
        .unwrap_or(0)
}

// ---------------- One commit ----------------

/// The files a commit touched, each with its own patch lines and counts.
///
/// `git show` gives the patch with the same shape as a working-tree diff, so it
/// reuses the parser the Changes tab already relies on.
pub fn commit_files(
    rt: &tokio::runtime::Runtime,
    runner: &GitRunner,
    path: &Path,
    sha: &str,
) -> Vec<crate::model::CommitFile> {
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
    });
    let patch = match patch {
        Ok(out) if out.success() => out.stdout_string(),
        _ => return Vec::new(),
    };

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
    files
}

// ---------------- Commit identity ----------------

/// The identity git will use for the next commit in `path`.
///
/// Like [`remote`], this is a single cheap config read done synchronously: the
/// dashboard needs it for the commit bar of the selected repository. `git config
/// --get-regexp` already resolves precedence, so a repository-local `user.name`
/// beats the global one.
pub fn commit_identity(path: &Path) -> Option<CommitIdentity> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["config", "--get-regexp", "^user\\.(name|email)$"])
        .env("LC_ALL", "C")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
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

    Some(CommitIdentity {
        name: name?,
        email: email?,
    })
}

// ---------------- Remote ----------------

/// Whether the repository has an `origin` to fetch, pull and push against.
///
/// The URL itself is not part of the dashboard — the action bar names the branch
/// and nothing else — so this is the only thing the UI needs from the remote.
/// Like the other cheap config reads here it asks git directly, so worktrees and
/// `include`d configs resolve correctly.
pub fn has_remote(path: &Path) -> bool {
    origin_url(path).is_some()
}

fn origin_url(repo_path: &Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["config", "--get", "remote.origin.url"])
        .env("LC_ALL", "C")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }
    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!url.is_empty()).then_some(url)
}

// ---------------- Relative time ----------------

fn now_seconds() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(elapsed) => elapsed.as_secs().min(i64::MAX as u64) as i64,
        Err(_) => 0,
    }
}

/// `just now` / `14m ago` / `3h ago` / `2d ago` / `1w ago` / `3mo ago`.
fn relative_label(now: i64, then: i64) -> String {
    let minutes = now.saturating_sub(then).max(0) / 60;
    if minutes < 1 {
        "just now".to_string()
    } else if minutes < 60 {
        format!("{minutes}m ago")
    } else if minutes < 60 * 24 {
        format!("{}h ago", minutes / 60)
    } else if minutes < 60 * 24 * 7 {
        format!("{}d ago", minutes / (60 * 24))
    } else if minutes < 60 * 24 * 30 {
        format!("{}w ago", minutes / (60 * 24 * 7))
    } else {
        format!("{}mo ago", minutes / (60 * 24 * 30))
    }
}

/// Same labels for git's `%aI` timestamps (`2024-05-06T07:08:09+02:00`).
fn relative_iso_label(now: i64, stamp: &str) -> String {
    match chrono::DateTime::parse_from_rfc3339(stamp) {
        Ok(moment) => relative_label(now, moment.timestamp()),
        Err(_) => UNKNOWN_AUTHOR.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_labels_follow_the_dashboard_scale() {
        let now = 1_700_000_000;
        assert_eq!(relative_label(now, now), "just now");
        assert_eq!(relative_label(now, now - 30), "just now");
        assert_eq!(relative_label(now, now - 14 * 60), "14m ago");
        assert_eq!(relative_label(now, now - 3 * 3600), "3h ago");
        assert_eq!(relative_label(now, now - 2 * 86_400), "2d ago");
        assert_eq!(relative_label(now, now - 9 * 86_400), "1w ago");
        assert_eq!(relative_label(now, now - 95 * 86_400), "3mo ago");
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

        let entries = parse_status_entries(output);
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
        assert_eq!(find("conflicted.rs").status, FileChangeStatus::Conflicted);
        let untracked = find("untracked file.rs");
        assert_eq!(untracked.status, FileChangeStatus::Untracked);
        assert!(!untracked.staged);

        assert!(
            entries.iter().all(|entry| entry.path != "old.rs"),
            "a rename's original path must not become its own row"
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
}
