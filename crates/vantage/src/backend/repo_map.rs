//! One persisted repository + its live git status, mapped onto the `Repository`
//! view model (`src/types/repo.ts` in the React app).
//!
//! Everything that needs git output the core engine does not already expose
//! (branches, commits, changed files, identity, remote) comes from
//! [`super::details`]; this module only decides *what the dashboard shows*.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::runtime::Runtime;
use vantage_core::git::status::StatusRecord;
use vantage_core::state::{RepoEntry, RepoStatus};
use vantage_core::GitRunner;

use crate::model::{RepoSyncState, Repository};

use super::details;

/// Shown instead of a branch name when HEAD is detached.
const DETACHED: &str = "detached";
/// Fallback for a status with no branch name; porcelain reports the branch for
/// an unborn HEAD, so this is only a last resort.
const FALLBACK_BRANCH: &str = "main";
/// Shown when the repository has no commit to date the row with.
const NO_COMMITS: &str = "no commits";
const SECONDS_PER_MINUTE: i64 = 60;
const MINUTES_PER_HOUR: i64 = 60;
const MINUTES_PER_DAY: i64 = 60 * 24;
const MINUTES_PER_WEEK: i64 = MINUTES_PER_DAY * 7;
const MINUTES_PER_MONTH: i64 = MINUTES_PER_DAY * 30;
const MINUTES_PER_YEAR: i64 = MINUTES_PER_DAY * 365;

/// Map the fields an overview row shows.
///
/// `group` is the workspace the user assigned the repo to, or `"Ungrouped"`.
/// `records` are the paths from the same status query as `status`. Branch
/// lists, history, stashes, identity and remote state belong to
/// [`super::details::repo_details`] and stay empty until the inspector asks for
/// them, so scanning a workspace never pays for every row's details.
pub fn map_repo(
    rt: &Runtime,
    runner: &GitRunner,
    entry: &RepoEntry,
    status: &RepoStatus,
    records: &[StatusRecord],
    head_sha: Option<&str>,
    group: String,
) -> Repository {
    let path = Path::new(&entry.path);
    let current_branch = current_branch(status);
    let (last_activity_label, last_activity_minutes) = last_activity(status);
    let uncommitted_files = details::changed_files(rt, runner, path, records);

    Repository {
        id: entry.id.clone(),
        name: entry.name.clone(),
        workspace: group,
        local_path: crate::path::contract(path),
        head_sha: head_sha.map(str::to_string),
        has_remote: false,
        stash_count: 0,
        current_branch,
        author: None,
        sync_state: sync_state(status),
        ahead_count: status.ahead as i32,
        behind_count: status.behind as i32,
        has_upstream: status.upstream.is_some(),
        is_detached: status.detached_sha.is_some(),
        last_activity_label,
        last_activity_minutes,
        uncommitted_files,
        branches: Vec::new(),
        recent_commits: Vec::new(),
        local_only_commits: std::collections::HashSet::new(),
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
/// unborn HEAD (fresh `git init`) reports the branch the first commit will use,
/// which porcelain includes in its branch header.
fn current_branch(status: &RepoStatus) -> String {
    match status.branch.as_deref() {
        Some(branch) => branch.to_string(),
        None if status.detached_sha.is_some() => DETACHED.to_string(),
        None => FALLBACK_BRANCH.to_string(),
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
    use crate::backend::details::repo_details;
    use vantage_core::git::status::query_repo_overview;
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
    fn current_branch_falls_back_to_detached_then_a_plain_name() {
        assert_eq!(current_branch(&status()), "main");

        let detached = RepoStatus {
            branch: None,
            detached_sha: Some("abc123".to_string()),
            ..status()
        };
        assert_eq!(current_branch(&detached), DETACHED);

        // `git status --branch` names the branch even on an unborn HEAD; this
        // fallback only covers a malformed/older status.
        let unborn = RepoStatus {
            branch: None,
            detached_sha: None,
            ..status()
        };
        assert_eq!(current_branch(&unborn), FALLBACK_BRANCH);
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

    /// The overview row carries status data only; details are a separate read.
    /// A clean work tree with an upstream costs exactly two Git processes:
    /// `status` and the last-activity `log`.
    #[test]
    fn overview_scan_reads_only_row_data() {
        if !git_available() {
            eprintln!("skipping: git is not installed");
            return;
        }
        let dir = std::env::temp_dir().join(format!("vantage_repo_map_{}", std::process::id()));
        let remote =
            std::env::temp_dir().join(format!("vantage_repo_map_origin_{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_dir_all(&remote).ok();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::create_dir_all(&remote).unwrap();
        git(&dir, &["init", "-q", "-b", "main"]);
        git(&dir, &["config", "user.email", "qa@example.com"]);
        git(&dir, &["config", "user.name", "QA"]);
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

        // No upstream yet: `status` + `log` + the `git remote` probe used for
        // the unpushed-commit fallback. Still no branch list, history, stash,
        // identity or remote-detail reads.
        let before = runner.commands_run();
        let snapshot = rt.block_on(query_repo_overview(&runner, &entry.id, &dir));
        assert!(
            snapshot.status.error.is_none(),
            "{:?}",
            snapshot.status.error
        );
        let clean = map_repo(
            &rt,
            &runner,
            &entry,
            &snapshot.status,
            &snapshot.records,
            snapshot.head_sha.as_deref(),
            "Ungrouped".to_string(),
        );
        assert_eq!(
            runner.commands_run() - before,
            3,
            "overview paid for more than status, activity and the remote probe"
        );
        assert_eq!(clean.id, "repo-fixture");
        assert_eq!(clean.name, "fixture");
        assert_eq!(clean.workspace, "Ungrouped");
        assert_eq!(clean.current_branch, "main");
        assert_eq!(clean.sync_state, RepoSyncState::Clean);
        assert_eq!(clean.ahead_count, 0);
        assert_eq!(clean.behind_count, 0);
        assert_eq!(clean.last_activity_minutes, 0);
        assert_ne!(clean.last_activity_label, NO_COMMITS);
        assert!(clean.uncommitted_files.is_empty());
        assert!(
            !clean.has_remote,
            "a work tree with no origin must not offer fetch/pull/push"
        );
        assert!(!clean.has_upstream);
        assert!(!clean.is_detached);
        assert!(
            clean.local_only_commits.is_empty(),
            "a repository with no remote has nothing to push"
        );
        assert!(!clean.local_path.is_empty());
        assert!(clean.head_sha.is_some());
        assert!(
            clean.branches.is_empty()
                && clean.recent_commits.is_empty()
                && clean.author.is_none()
                && clean.stash_count == 0
                && !clean.has_remote,
            "the overview must not read inspector details"
        );

        // Selecting the row is what pays for branches, history, stashes,
        // identity and remote capability.
        let before = runner.commands_run();
        let details = repo_details(
            &rt,
            &runner,
            &dir,
            &clean.current_branch,
            clean.head_sha.is_some(),
        )
        .unwrap();
        assert!(
            runner.commands_run() - before >= 5,
            "detail load should read the branch list, history and stashes"
        );
        assert!(!details.has_remote, "this fixture has no origin yet");
        assert_eq!(
            details.author,
            Some(crate::model::CommitIdentity {
                name: "QA".to_string(),
                email: "qa@example.com".to_string(),
            }),
            "the bar names the identity the fixture configured"
        );
        assert!(details
            .branches
            .iter()
            .any(|b| b.is_current && b.name == "main"));
        assert!(details
            .branches
            .iter()
            .any(|branch| !branch.last_commit_time.is_empty()));
        assert!(details
            .recent_commits
            .iter()
            .any(|commit| commit.message.contains("initial commit")));
        assert_eq!(details.stash_count, 0);

        // An uncommitted edit still flips the overview row and lists the file.
        std::fs::write(dir.join("lib.ts"), "export const one = 2;\n").unwrap();
        let snapshot = rt.block_on(query_repo_overview(&runner, &entry.id, &dir));
        let dirty = map_repo(
            &rt,
            &runner,
            &entry,
            &snapshot.status,
            &snapshot.records,
            snapshot.head_sha.as_deref(),
            "AI Tools".to_string(),
        );
        assert_eq!(dirty.sync_state, RepoSyncState::Modified);
        assert_eq!(dirty.workspace, "AI Tools");
        assert_eq!(dirty.uncommitted_files.len(), 1);
        assert_eq!(dirty.uncommitted_files[0].path, "lib.ts");
        assert!(dirty.branches.is_empty() && dirty.recent_commits.is_empty());

        // Adding an origin without pushing leaves the branch untracked; the
        // detail load is what knows fetch/pull/push can be offered.
        git(&remote, &["init", "-q", "--bare"]);
        git(&dir, &["remote", "add", "origin", remote.to_str().unwrap()]);
        git(&dir, &["commit", "-q", "-am", "second commit"]);
        let snapshot = rt.block_on(query_repo_overview(&runner, &entry.id, &dir));
        let with_remote = map_repo(
            &rt,
            &runner,
            &entry,
            &snapshot.status,
            &snapshot.records,
            snapshot.head_sha.as_deref(),
            "Ungrouped".to_string(),
        );
        assert!(!with_remote.has_remote, "overview stays cheap");
        assert!(
            !with_remote.has_upstream,
            "a branch that was never pushed tracks nothing"
        );
        let details = repo_details(
            &rt,
            &runner,
            &dir,
            &with_remote.current_branch,
            with_remote.head_sha.is_some(),
        )
        .unwrap();
        assert!(details.has_remote);
        assert_eq!(
            details.local_only_commits.len(),
            2,
            "with no upstream, commits absent from every remote are local"
        );

        git(&dir, &["push", "-q", "-u", "origin", "main"]);
        let before = runner.commands_run();
        let snapshot = rt.block_on(query_repo_overview(&runner, &entry.id, &dir));
        let published = map_repo(
            &rt,
            &runner,
            &entry,
            &snapshot.status,
            &snapshot.records,
            snapshot.head_sha.as_deref(),
            "Ungrouped".to_string(),
        );
        assert_eq!(
            runner.commands_run() - before,
            2,
            "a clean repo with an upstream costs one status and one activity log"
        );
        assert!(published.has_upstream, "push -u sets the upstream");
        assert_eq!(published.ahead_count, 0);

        // A new local commit is the only one in the range, even though older
        // rows are on the remote.
        std::fs::write(dir.join("lib.ts"), "export const one = 3;\n").unwrap();
        git(&dir, &["commit", "-q", "-am", "third commit"]);
        let details = repo_details(
            &rt,
            &runner,
            &dir,
            &published.current_branch,
            published.head_sha.is_some(),
        )
        .unwrap();
        let tip = details
            .recent_commits
            .first()
            .expect("the new tip is listed");
        assert_eq!(
            details.local_only_commits,
            [tip.full_sha.clone()].into_iter().collect(),
            "only the commit the upstream does not have is local"
        );

        // A detached HEAD has no branch to publish, so the row must carry the
        // guard the Push button and backend both read.
        git(&dir, &["checkout", "-q", "--detach", "HEAD"]);
        let snapshot = rt.block_on(query_repo_overview(&runner, &entry.id, &dir));
        let detached = map_repo(
            &rt,
            &runner,
            &entry,
            &snapshot.status,
            &snapshot.records,
            snapshot.head_sha.as_deref(),
            "Ungrouped".to_string(),
        );
        assert!(detached.is_detached);
        assert_eq!(detached.current_branch, DETACHED);
        assert_eq!(
            detached.push_blocked_reason(),
            Some("HEAD is detached. Check out a branch before pushing.")
        );

        std::fs::remove_dir_all(&remote).ok();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_unborn_repository_loads_details_without_history_errors() {
        if !git_available() {
            eprintln!("skipping: git is not installed");
            return;
        }
        let dir = std::env::temp_dir().join(format!("vantage_repo_unborn_{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q", "-b", "trunk"]);
        git(&dir, &["config", "user.email", "qa@example.com"]);
        git(&dir, &["config", "user.name", "QA"]);

        let rt = Runtime::new().unwrap();
        let runner = GitRunner::new();
        let entry = RepoEntry {
            id: "repo-unborn".to_string(),
            path: dir.to_string_lossy().to_string(),
            name: "unborn".to_string(),
            branch: None,
            pinned: false,
            order: 1,
            marker: None,
            notes: Vec::new(),
        };
        let snapshot = rt.block_on(query_repo_overview(&runner, &entry.id, &dir));
        let row = map_repo(
            &rt,
            &runner,
            &entry,
            &snapshot.status,
            &snapshot.records,
            snapshot.head_sha.as_deref(),
            "Ungrouped".to_string(),
        );
        assert_eq!(row.current_branch, "trunk");
        assert!(row.head_sha.is_none());

        let details = repo_details(
            &rt,
            &runner,
            &dir,
            &row.current_branch,
            row.head_sha.is_some(),
        )
        .expect("an unborn repository still has details");
        assert!(details.recent_commits.is_empty());
        assert!(details.branches.is_empty());
        assert_eq!(
            details.author,
            Some(crate::model::CommitIdentity {
                name: "QA".to_string(),
                email: "qa@example.com".to_string(),
            })
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
