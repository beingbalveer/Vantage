//! One persisted repository + its live git status, mapped onto the `Repository`
//! view model (`src/types/repo.ts` in the React app).
//!
//! Everything that needs git output the core engine does not already expose
//! (branches, commits, changed files, identity, remote) comes from
//! [`super::details`]; this module only decides *what the dashboard shows*.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::runtime::Runtime;
use vantage_core::state::{RepoEntry, RepoStatus};
use vantage_core::{repos, GitRunner};

use crate::model::{RepoSyncState, Repository};

use super::details;

/// Shown instead of a branch name when HEAD is detached.
const DETACHED: &str = "detached";
/// Shown when the repository has no commit to date the row with.
const NO_COMMITS: &str = "no commits";
const SECONDS_PER_MINUTE: i64 = 60;
const MINUTES_PER_HOUR: i64 = 60;
const MINUTES_PER_DAY: i64 = 60 * 24;
const MINUTES_PER_WEEK: i64 = MINUTES_PER_DAY * 7;
const MINUTES_PER_MONTH: i64 = MINUTES_PER_DAY * 30;
const MINUTES_PER_YEAR: i64 = MINUTES_PER_DAY * 365;

/// `group` is the workspace the user assigned the repo to, or `"Ungrouped"`.
pub fn map_repo(
    rt: &Runtime,
    runner: &GitRunner,
    entry: &RepoEntry,
    status: &RepoStatus,
    group: String,
) -> Repository {
    let path = Path::new(&entry.path);
    let default_branch = rt.block_on(repos::default_branch(runner, path));
    let current_branch = current_branch(status, &default_branch);
    let (last_activity_label, last_activity_minutes) = last_activity(status);

    let uncommitted_files = details::changed_files(rt, runner, path);
    let branches = details::branches(rt, runner, path, &current_branch, &default_branch);
    // "detached" is only a display label; `git log` must be asked for HEAD.
    let commit_branch = if current_branch == DETACHED {
        "HEAD"
    } else {
        current_branch.as_str()
    };
    let recent_commits = details::commits(rt, runner, path, commit_branch);
    let has_remote = details::has_remote(path);
    let local_only_commits =
        details::local_only_commits(rt, runner, path, status.upstream.as_deref(), has_remote);

    Repository {
        id: entry.id.clone(),
        name: entry.name.clone(),
        workspace: group,
        local_path: crate::path::contract(path),
        has_remote,
        stash_count: details::stash_count(rt, runner, path),
        current_branch,
        author: details::commit_identity(path),
        sync_state: sync_state(status),
        ahead_count: status.ahead as i32,
        behind_count: status.behind as i32,
        has_upstream: status.upstream.is_some(),
        last_activity_label,
        last_activity_minutes,
        uncommitted_files,
        branches,
        recent_commits,
        local_only_commits,
    }
}

/// Working-tree state, in the React app's precedence: a conflict hides
/// everything else, then being behind the upstream, then uncommitted changes,
/// then commits that exist only locally.
fn sync_state(status: &RepoStatus) -> RepoSyncState {
    if status.conflicted_count > 0 {
        RepoSyncState::Conflict
    } else if status.behind > 0 {
        RepoSyncState::Behind
    } else if status.dirty_count > 0 {
        RepoSyncState::Modified
    } else if status.ahead > 0 {
        RepoSyncState::Ahead
    } else {
        RepoSyncState::Clean
    }
}

/// The branch the working tree is on. A detached HEAD reports `"detached"`; an
/// unborn HEAD (fresh `git init`) reports the branch the first commit will use.
fn current_branch(status: &RepoStatus, default_branch: &str) -> String {
    match status.branch.as_deref() {
        Some(branch) => branch.to_string(),
        None if status.detached_sha.is_some() => DETACHED.to_string(),
        None => default_branch.to_string(),
    }
}

/// Age of the newest commit, as the label and the whole minutes the table sorts
/// on. `last_activity_at` is `git log -1 --format=%cI` (RFC3339 with offset).
fn last_activity(status: &RepoStatus) -> (String, i64) {
    match status
        .last_activity_at
        .as_deref()
        .and_then(parse_rfc3339_seconds)
    {
        Some(then) => relative_activity(now_seconds(), then),
        None => (NO_COMMITS.to_string(), 0),
    }
}

/// `just now` / `14m ago` / `3h ago` / `2d ago` / `1w ago` / `2mo ago`, matching
/// the React app's time strings, plus the age in whole minutes.
fn relative_activity(now: i64, then: i64) -> (String, i64) {
    let minutes = now.saturating_sub(then).max(0) / SECONDS_PER_MINUTE;
    let label = if minutes < 1 {
        "just now".to_string()
    } else if minutes < MINUTES_PER_HOUR {
        format!("{minutes}m ago")
    } else if minutes < MINUTES_PER_DAY {
        format!("{}h ago", minutes / MINUTES_PER_HOUR)
    } else if minutes < MINUTES_PER_WEEK {
        format!("{}d ago", minutes / MINUTES_PER_DAY)
    } else if minutes < MINUTES_PER_MONTH {
        format!("{}w ago", minutes / MINUTES_PER_WEEK)
    } else if minutes < MINUTES_PER_YEAR {
        format!("{}mo ago", minutes / MINUTES_PER_MONTH)
    } else {
        format!("{}y ago", minutes / MINUTES_PER_YEAR)
    };
    (label, minutes)
}

fn now_seconds() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(elapsed) => elapsed.as_secs().min(i64::MAX as u64) as i64,
        Err(_) => 0,
    }
}

/// Epoch seconds for `YYYY-MM-DDTHH:MM:SS` plus a `Z`/`±HH:MM` offset.
///
/// Hand-rolled so the UI crate needs no date dependency; anything that is not a
/// timestamp git would print returns `None` (the row falls back to "no commits"
/// instead of showing a wildly wrong age).
fn parse_rfc3339_seconds(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    if bytes.len() < 19
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[13] != b':'
        || bytes[16] != b':'
    {
        return None;
    }
    if !matches!(bytes[10], b'T' | b't' | b' ') {
        return None;
    }

    let year: i64 = text.get(0..4)?.parse().ok()?;
    let month: u32 = text.get(5..7)?.parse().ok()?;
    let day: u32 = text.get(8..10)?.parse().ok()?;
    let hour: i64 = text.get(11..13)?.parse().ok()?;
    let minute: i64 = text.get(14..16)?.parse().ok()?;
    let second: i64 = text.get(17..19)?.parse().ok()?;
    if hour > 23 || minute > 59 || second > 60 {
        return None;
    }

    let offset = parse_offset(text.get(19..)?)?;
    let days = days_from_civil(year, month, day)?;
    Some(days * 86_400 + hour * 3_600 + minute * 60 + second - offset)
}

/// Offset in seconds east of UTC from `"Z"`, `".500Z"`, `"+02:00"`, `"+0200"`
/// or `"+02"`. A timestamp without an offset is rejected: git's `%cI` always
/// writes one, and a zoneless value would silently read as local time.
fn parse_offset(rest: &str) -> Option<i64> {
    let rest = rest.trim();
    if rest.eq_ignore_ascii_case("z") {
        return Some(0);
    }
    if rest.is_empty() {
        return None;
    }

    // Fractional seconds, if the writer emitted them.
    let rest = match rest.strip_prefix('.') {
        Some(fraction) => {
            let digits = fraction
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(fraction.len());
            if digits == 0 {
                return None;
            }
            &fraction[digits..]
        }
        None => rest,
    };
    if rest.eq_ignore_ascii_case("z") {
        return Some(0);
    }
    if rest.is_empty() {
        return None;
    }

    let (sign, digits) = match rest.as_bytes().first()? {
        b'+' => (1, rest.get(1..)?),
        b'-' => (-1, rest.get(1..)?),
        _ => return None,
    };
    let (hours, minutes) = match digits.len() {
        2 => (digits, "00"),
        4 => (digits.get(0..2)?, digits.get(2..4)?),
        5 if digits.as_bytes()[2] == b':' => (digits.get(0..2)?, digits.get(3..5)?),
        _ => return None,
    };
    let hours: i64 = hours.parse().ok()?;
    let minutes: i64 = minutes.parse().ok()?;
    if hours > 23 || minutes > 59 {
        return None;
    }
    Some(sign * (hours * 3_600 + minutes * 60))
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's
/// `days_from_civil`).
fn days_from_civil(year: i64, month: u32, day: u32) -> Option<i64> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_index = (month as i64 + 9) % 12;
    let day_of_year = (153 * month_index + 2) / 5 + day as i64 - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some(era * 146_097 + day_of_era - 719_468)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vantage_core::git::status::query_repo_status;
    use vantage_core::state::MarkerState;

    fn status() -> RepoStatus {
        RepoStatus {
            repo_id: "repo-1".to_string(),
            branch: Some("main".to_string()),
            detached_sha: None,
            upstream: Some("origin/main".to_string()),
            ahead: 0,
            behind: 0,
            dirty_count: 0,
            operation: None,
            conflicted_count: 0,
            conflicted_paths: Vec::new(),
            new_commit_count: 0,
            marker_state: MarkerState::None,
            last_activity_at: None,
            last_fetched_at: None,
            is_checked_out: true,
            error: None,
        }
    }

    #[test]
    fn sync_state_uses_conflict_behind_modified_ahead_clean_order() {
        assert_eq!(sync_state(&status()), RepoSyncState::Clean);

        let ahead = RepoStatus {
            ahead: 2,
            ..status()
        };
        assert_eq!(sync_state(&ahead), RepoSyncState::Ahead);

        let dirty = RepoStatus {
            dirty_count: 1,
            ..ahead
        };
        assert_eq!(sync_state(&dirty), RepoSyncState::Modified);

        let behind = RepoStatus { behind: 3, ..dirty };
        assert_eq!(sync_state(&behind), RepoSyncState::Behind);

        let conflicted = RepoStatus {
            conflicted_count: 1,
            conflicted_paths: vec!["f.txt".to_string()],
            ..behind
        };
        assert_eq!(sync_state(&conflicted), RepoSyncState::Conflict);
    }

    #[test]
    fn current_branch_falls_back_to_detached_then_default() {
        assert_eq!(current_branch(&status(), "main"), "main");

        let detached = RepoStatus {
            branch: None,
            detached_sha: Some("abc123".to_string()),
            ..status()
        };
        assert_eq!(current_branch(&detached, "main"), DETACHED);

        // Unborn HEAD: `git init` has neither a branch nor a commit.
        let unborn = RepoStatus {
            branch: None,
            detached_sha: None,
            ..status()
        };
        assert_eq!(current_branch(&unborn, "trunk"), "trunk");
    }

    #[test]
    fn relative_activity_matches_the_react_labels() {
        assert_eq!(relative_activity(1_000, 1_000), ("just now".to_string(), 0));
        assert_eq!(
            relative_activity(1_000, 1_000 - 14 * 60),
            ("14m ago".to_string(), 14)
        );
        assert_eq!(
            relative_activity(0, -(3 * 60 + 5) * 60),
            ("3h ago".to_string(), 185)
        );
        assert_eq!(
            relative_activity(0, -2 * 24 * 3600),
            ("2d ago".to_string(), 2 * 24 * 60)
        );
        assert_eq!(
            relative_activity(0, -8 * 24 * 3600),
            ("1w ago".to_string(), 8 * 24 * 60)
        );
        assert_eq!(
            relative_activity(0, -45 * 24 * 3600),
            ("1mo ago".to_string(), 45 * 24 * 60)
        );
        // A clock that is ahead of the commit date must not read as negative.
        assert_eq!(relative_activity(0, 600), ("just now".to_string(), 0));
    }

    #[test]
    fn last_activity_reads_the_iso_timestamp() {
        let dated = RepoStatus {
            last_activity_at: Some("2024-02-29T12:00:00Z".to_string()),
            ..status()
        };
        let (label, minutes) = last_activity(&dated);
        assert!(label.ends_with("ago") || label == "just now", "{label}");
        assert!(minutes > 0);
    }

    #[test]
    fn rfc3339_parser_applies_offsets_and_fractions() {
        assert_eq!(parse_rfc3339_seconds("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_rfc3339_seconds("1970-01-01T02:00:00+02:00"), Some(0));
        assert_eq!(parse_rfc3339_seconds("1969-12-31T19:00:00-05:00"), Some(0));
        assert_eq!(parse_rfc3339_seconds("1970-01-01T02:00:00+0200"), Some(0));
        assert_eq!(parse_rfc3339_seconds("1970-01-01T00:00:00.500Z"), Some(0));
        assert_eq!(
            parse_rfc3339_seconds("2024-02-29T12:00:00Z"),
            Some(1_709_208_000)
        );
        assert_eq!(
            parse_rfc3339_seconds("2024-05-01T08:30:00+05:30"),
            Some(1_714_532_400)
        );
    }

    #[test]
    fn rfc3339_parser_rejects_non_timestamps() {
        assert_eq!(parse_rfc3339_seconds(""), None);
        assert_eq!(parse_rfc3339_seconds("not a date"), None);
        assert_eq!(parse_rfc3339_seconds("2024-13-01T00:00:00Z"), None);
        assert_eq!(parse_rfc3339_seconds("2024-02-01T00:00:00"), None);
        assert_eq!(parse_rfc3339_seconds("2024-02-01T00:00:00+25:00"), None);
    }

    fn git_available() -> bool {
        std::process::Command::new("git")
            .arg("--version")
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false)
    }

    fn git(dir: &Path, args: &[&str]) {
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
    }

    /// End-to-end against a real work tree: the core engine reports the state
    /// and `map_repo` turns it into the row the dashboard renders.
    #[test]
    fn map_repo_reads_a_real_work_tree() {
        if !git_available() {
            eprintln!("skipping: git is not installed");
            return;
        }
        let dir = std::env::temp_dir().join(format!("vantage_repo_map_{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q", "-b", "main"]);
        git(&dir, &["config", "user.email", "qa@example.com"]);
        git(&dir, &["config", "user.name", "QA"]);
        // Pin the default-branch probe to the branch we created, whatever the
        // machine's global `init.defaultBranch` says.
        git(&dir, &["config", "init.defaultBranch", "main"]);
        std::fs::write(dir.join("lib.ts"), "export const one = 1;\n").unwrap();
        git(&dir, &["add", "."]);
        git(&dir, &["commit", "-q", "-m", "initial commit"]);

        let rt = Runtime::new().unwrap();
        let runner = GitRunner::new();
        let entry = RepoEntry {
            id: "repo-fixture".to_string(),
            path: dir.to_string_lossy().to_string(),
            name: "fixture".to_string(),
            branch: None,
            pinned: false,
            order: 1,
            marker: None,
            notes: Vec::new(),
        };

        let status = rt.block_on(query_repo_status(&runner, &entry.id, &dir, None, None));
        assert!(status.error.is_none(), "{:?}", status.error);
        let clean = map_repo(&rt, &runner, &entry, &status, "Ungrouped".to_string());
        assert_eq!(clean.id, "repo-fixture");
        assert_eq!(clean.name, "fixture");
        assert_eq!(clean.workspace, "Ungrouped");
        assert_eq!(clean.current_branch, "main");
        assert_eq!(clean.sync_state, RepoSyncState::Clean);
        assert_eq!(clean.ahead_count, 0);
        assert_eq!(clean.behind_count, 0);
        assert_eq!(clean.last_activity_minutes, 0);
        assert_ne!(clean.last_activity_label, NO_COMMITS);
        assert!(clean
            .branches
            .iter()
            .any(|b| b.is_current && b.name == "main"));
        assert!(clean
            .recent_commits
            .iter()
            .any(|commit| commit.message.contains("initial commit")));
        assert!(clean.uncommitted_files.is_empty());
        assert!(
            !clean.has_remote,
            "a work tree with no origin must not offer fetch/pull/push"
        );
        assert!(!clean.has_upstream);
        assert!(
            clean.local_only_commits.is_empty(),
            "a repository with no remote has nothing to push"
        );
        assert!(!clean.local_path.is_empty());
        assert!(
            clean
                .branches
                .iter()
                .any(|branch| !branch.last_commit_time.is_empty()),
            "branch rows show when the branch last moved"
        );
        assert_eq!(
            clean.author,
            Some(crate::model::CommitIdentity {
                name: "QA".to_string(),
                email: "qa@example.com".to_string(),
            }),
            "the bar names the identity the fixture configured"
        );

        // An uncommitted edit flips the row to Modified and lists the file.
        std::fs::write(dir.join("lib.ts"), "export const one = 2;\n").unwrap();
        let status = rt.block_on(query_repo_status(&runner, &entry.id, &dir, None, None));
        let dirty = map_repo(&rt, &runner, &entry, &status, "AI Tools".to_string());
        assert_eq!(dirty.sync_state, RepoSyncState::Modified);
        assert_eq!(dirty.workspace, "AI Tools");
        assert_eq!(dirty.uncommitted_files.len(), 1);
        assert_eq!(dirty.uncommitted_files[0].path, "lib.ts");

        // Adding an origin turns the remote capability on while the branch
        // still tracks nothing, which is what makes `Push` publish it.
        let remote =
            std::env::temp_dir().join(format!("vantage_repo_map_origin_{}", std::process::id()));
        std::fs::remove_dir_all(&remote).ok();
        std::fs::create_dir_all(&remote).unwrap();
        git(&remote, &["init", "-q", "--bare"]);
        git(&dir, &["remote", "add", "origin", remote.to_str().unwrap()]);
        git(&dir, &["commit", "-q", "-am", "second commit"]);

        let status = rt.block_on(query_repo_status(&runner, &entry.id, &dir, None, None));
        let with_remote = map_repo(&rt, &runner, &entry, &status, "Ungrouped".to_string());
        assert!(with_remote.has_remote);
        assert!(
            !with_remote.has_upstream,
            "a branch that was never pushed tracks nothing"
        );
        assert_eq!(
            with_remote.local_only_commits.len(),
            2,
            "with no upstream, commits absent from every remote are local"
        );
        assert_eq!(
            with_remote.local_only_prefix_len(),
            with_remote.recent_commits.len(),
            "both visible commits are local"
        );

        git(&dir, &["push", "-q", "-u", "origin", "main"]);
        let status = rt.block_on(query_repo_status(&runner, &entry.id, &dir, None, None));
        let published = map_repo(&rt, &runner, &entry, &status, "Ungrouped".to_string());
        assert!(published.has_remote);
        assert!(published.has_upstream, "push -u sets the upstream");
        assert!(
            published.local_only_commits.is_empty(),
            "everything is on the upstream after a push"
        );
        assert_eq!(published.local_only_prefix_len(), 0);

        // A new local commit is the only one in the range, even though older
        // rows are on the remote.
        std::fs::write(dir.join("lib.ts"), "export const one = 3;\n").unwrap();
        git(&dir, &["commit", "-q", "-am", "third commit"]);
        let status = rt.block_on(query_repo_status(&runner, &entry.id, &dir, None, None));
        let ahead = map_repo(&rt, &runner, &entry, &status, "Ungrouped".to_string());
        let tip = ahead.recent_commits.first().expect("the new tip is listed");
        assert_eq!(
            ahead.local_only_commits,
            [tip.full_sha.clone()].into_iter().collect(),
            "only the commit the upstream does not have is local"
        );
        assert!(
            ahead.commit_is_local_only(tip),
            "the newest row carries the marker"
        );
        assert_eq!(ahead.local_only_prefix_len(), 1);
        assert!(
            ahead
                .recent_commits
                .iter()
                .skip(1)
                .all(|commit| !ahead.commit_is_local_only(commit)),
            "older, pushed commits stay unmarked"
        );

        std::fs::remove_dir_all(&remote).ok();
        std::fs::remove_dir_all(&dir).ok();
    }
}
