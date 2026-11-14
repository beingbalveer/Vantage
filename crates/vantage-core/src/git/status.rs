use crate::git::runner::GitRunner;
use crate::state::{MarkerState, OperationKind, RepoStatus};
use std::path::Path;

#[derive(Debug, Default, Clone)]
pub struct ParsedStatusV2 {
    pub branch: Option<String>,
    pub detached_sha: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub dirty_count: u32,
    pub conflicted_paths: Vec<String>,
}

/// Extract the paths from `git status --porcelain=v2 -z` unmerged (`u`) records.
///
/// Record shape (space separated, path last and may contain spaces):
/// `u <XY> <sub> <m1> <m2> <m3> <mW> <h1> <h2> <h3> <path>`
pub fn parse_unmerged_paths(output: &[u8]) -> Vec<String> {
    let mut paths = Vec::new();
    for part in output.split(|&b| b == 0) {
        if !part.starts_with(b"u ") {
            continue;
        }
        let line = String::from_utf8_lossy(part);
        if let Some(path) = line.splitn(11, ' ').nth(10) {
            if !path.is_empty() {
                paths.push(path.to_string());
            }
        }
    }
    paths
}

/// Extract deduplicated paths from `git ls-files -u -z` records.
///
/// Record shape: `<mode> <object> <stage>\t<path>\0` (up to three stages per path).
pub fn parse_unmerged_ls_files(output: &[u8]) -> Vec<String> {
    let mut paths: Vec<String> = Vec::new();
    for rec in output.split(|&b| b == 0) {
        if rec.is_empty() {
            continue;
        }
        if let Some(tab) = rec.iter().position(|&b| b == b'\t') {
            let path = String::from_utf8_lossy(&rec[tab + 1..]).to_string();
            if !path.is_empty() && !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    paths
}

/// Detect an in-progress repository operation from the resolved git directory.
///
/// Uses the same marker files git itself creates:
/// `.git/rebase-merge` / `.git/rebase-apply`, `MERGE_HEAD`, `CHERRY_PICK_HEAD`,
/// `REVERT_HEAD`. Rebase is checked first because a rebase can also carry a
/// `CHERRY_PICK_HEAD` while replaying a commit.
pub fn detect_operation(git_dir: &Path) -> Option<OperationKind> {
    if git_dir.join("rebase-merge").is_dir() || git_dir.join("rebase-apply").is_dir() {
        return Some(OperationKind::Rebase);
    }
    if git_dir.join("MERGE_HEAD").is_file() {
        return Some(OperationKind::Merge);
    }
    if git_dir.join("CHERRY_PICK_HEAD").is_file() {
        return Some(OperationKind::CherryPick);
    }
    if git_dir.join("REVERT_HEAD").is_file() {
        return Some(OperationKind::Revert);
    }
    None
}

/// Branch being rebased, read from `.git/rebase-merge/head-name`.
///
/// A rebase detaches HEAD, so porcelain reports `(detached)`. The head-name
/// file is what lets us still map the working-tree operation to the branch the
/// user is viewing.
pub fn rebase_head_branch(git_dir: &Path) -> Option<String> {
    for dir in ["rebase-merge", "rebase-apply"] {
        let file = git_dir.join(dir).join("head-name");
        if let Ok(raw) = std::fs::read_to_string(&file) {
            let name = raw.trim();
            if !name.is_empty() {
                return Some(name.strip_prefix("refs/heads/").unwrap_or(name).to_string());
            }
        }
    }
    None
}

pub fn parse_porcelain_v2(output: &[u8]) -> ParsedStatusV2 {
    let mut result = ParsedStatusV2::default();
    let parts: Vec<&[u8]> = output.split(|&b| b == 0).collect();

    let mut i = 0;
    while i < parts.len() {
        let part = parts[i];
        if part.is_empty() {
            i += 1;
            continue;
        }

        let line = String::from_utf8_lossy(part);
        if let Some(stripped) = line.strip_prefix("# branch.head ") {
            let head = stripped.trim();
            if head == "(detached)" {
                result.branch = None;
            } else {
                result.branch = Some(head.to_string());
            }
        } else if let Some(stripped) = line.strip_prefix("# branch.oid ") {
            let oid = stripped.trim();
            if oid != "(initial)" {
                result.detached_sha = Some(oid.to_string());
            }
        } else if let Some(stripped) = line.strip_prefix("# branch.upstream ") {
            let upstream = stripped.trim();
            if !upstream.is_empty() {
                result.upstream = Some(upstream.to_string());
            }
        } else if let Some(stripped) = line.strip_prefix("# branch.ab ") {
            let ab = stripped.trim();
            let mut ahead = 0;
            let mut behind = 0;
            for token in ab.split_whitespace() {
                if let Some(val) = token.strip_prefix('+') {
                    ahead = val.parse().unwrap_or(0);
                } else if let Some(val) = token.strip_prefix('-') {
                    behind = val.parse().unwrap_or(0);
                }
            }
            result.ahead = ahead;
            result.behind = behind;
        } else if line.starts_with("1 ") || line.starts_with("? ") {
            result.dirty_count += 1;
        } else if line.starts_with("u ") {
            result.dirty_count += 1;
            if let Some(path) = line.splitn(11, ' ').nth(10) {
                if !path.is_empty() {
                    result.conflicted_paths.push(path.to_string());
                }
            }
        } else if line.starts_with("2 ") {
            result.dirty_count += 1;
            // Rename entries in -z format have an extra field for origPath
            i += 1;
        }

        i += 1;
    }

    // If not detached, detached_sha should only be set if branch is None
    if result.branch.is_some() {
        result.detached_sha = None;
    }

    result
}

pub async fn query_repo_status(
    runner: &GitRunner,
    repo_id: &str,
    repo_path: &Path,
    target_branch: Option<&str>,
    marker_sha: Option<&str>,
) -> RepoStatus {
    if !repo_path.exists() {
        return RepoStatus {
            repo_id: repo_id.to_string(),
            branch: target_branch.map(|s| s.to_string()),
            detached_sha: None,
            upstream: None,
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
            is_checked_out: target_branch.is_none(),
            error: Some("Repository path does not exist".to_string()),
        };
    }

    let status_res = runner
        .run_read(
            repo_path,
            &[
                "status",
                "--porcelain=v2",
                "--branch",
                "-z",
                "--untracked-files=all",
            ],
        )
        .await;

    let parsed = match status_res {
        Ok(out) => {
            if !out.success() {
                return RepoStatus {
                    repo_id: repo_id.to_string(),
                    branch: target_branch.map(|s| s.to_string()),
                    detached_sha: None,
                    upstream: None,
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
                    is_checked_out: target_branch.is_none(),
                    error: Some(out.stderr),
                };
            }
            parse_porcelain_v2(&out.stdout)
        }
        Err(e) => {
            return RepoStatus {
                repo_id: repo_id.to_string(),
                branch: target_branch.map(|s| s.to_string()),
                detached_sha: None,
                upstream: None,
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
                is_checked_out: target_branch.is_none(),
                error: Some(e.message),
            };
        }
    };

    let target_ref = target_branch.unwrap_or("HEAD");

    // Calculate marker state and new_commit_count
    let mut marker_state = MarkerState::None;
    let mut new_commit_count = 0;

    if let Some(m_sha) = marker_sha {
        // Check if marker exists
        let exists_check = runner
            .run_read(
                repo_path,
                &["cat-file", "-e", &format!("{}^{{commit}}", m_sha)],
            )
            .await;

        if exists_check.is_err() || !exists_check.as_ref().unwrap().success() {
            marker_state = MarkerState::Missing;
        } else {
            // Check if marker is ancestor of target_ref
            let ancestor_check = runner
                .run_read(
                    repo_path,
                    &["merge-base", "--is-ancestor", m_sha, target_ref],
                )
                .await;

            if let Ok(out) = ancestor_check {
                if out.success() {
                    marker_state = MarkerState::Ok;
                    // Count new commits: git rev-list --count {M}..{target_ref}
                    let rev_range = format!("{}..{}", m_sha, target_ref);
                    let count_res = runner
                        .run_read(repo_path, &["rev-list", "--count", &rev_range])
                        .await;
                    if let Ok(count_out) = count_res {
                        new_commit_count =
                            count_out.stdout_string().trim().parse::<u32>().unwrap_or(0);
                    }
                } else {
                    marker_state = MarkerState::Diverged;
                }
            } else {
                marker_state = MarkerState::Diverged;
            }
        }
    }

    // Determine last_activity_at
    let last_activity_at = get_last_activity(runner, repo_path, target_branch).await;
    let last_fetched_at = get_last_fetch_at(runner, repo_path).await;

    let git_dir = crate::git::ops::get_real_git_dir(runner, repo_path)
        .await
        .ok();
    let operation = git_dir.as_deref().and_then(detect_operation);

    let is_checked_out = match target_branch {
        None => true,
        Some(tb) => {
            parsed.branch.as_deref() == Some(tb)
                // A rebase detaches HEAD; map it back to the branch being
                // rebased so the conflict banner and file list still work.
                || (parsed.branch.is_none()
                    && git_dir
                        .as_deref()
                        .and_then(rebase_head_branch)
                        .as_deref()
                        == Some(tb))
        }
    };

    // In-progress operation + unmerged paths belong to the working tree, so they
    // are only meaningful for the checked-out branch view.
    let (operation, conflicted_paths) = if is_checked_out {
        (operation, parsed.conflicted_paths.clone())
    } else {
        (None, Vec::new())
    };
    let conflicted_count = conflicted_paths.len() as u32;

    let mut ahead = if is_checked_out { parsed.ahead } else { 0 };
    if is_checked_out && ahead == 0 && parsed.upstream.is_none() {
        let has_remote = runner
            .run_read(repo_path, &["remote"])
            .await
            .map(|out| out.success() && !out.stdout_string().trim().is_empty())
            .unwrap_or(false);
        if has_remote {
            let rev_res = runner
                .run_read(
                    repo_path,
                    &["rev-list", target_ref, "--not", "--remotes", "--count"],
                )
                .await;
            if let Ok(out) = rev_res {
                if out.success() {
                    ahead = out.stdout_string().trim().parse::<u32>().unwrap_or(0);
                }
            }
        }
    }

    RepoStatus {
        repo_id: repo_id.to_string(),
        branch: target_branch.map(|s| s.to_string()).or(parsed.branch),
        detached_sha: if target_branch.is_none() {
            parsed.detached_sha
        } else {
            None
        },
        upstream: if is_checked_out {
            parsed.upstream
        } else {
            None
        },
        ahead,
        behind: if is_checked_out { parsed.behind } else { 0 },
        dirty_count: if is_checked_out {
            parsed.dirty_count
        } else {
            0
        },
        operation,
        conflicted_count,
        conflicted_paths,
        new_commit_count,
        marker_state,
        last_activity_at,
        last_fetched_at,
        is_checked_out,
        error: None,
    }
}

/// Timestamp of the last fetch/pull, from `FETCH_HEAD`'s mtime. This also
/// covers fetches run by the terminal or an agent, not just this app.
async fn get_last_fetch_at(runner: &GitRunner, repo_path: &Path) -> Option<String> {
    let out = runner
        .run_read(repo_path, &["rev-parse", "--git-path", "FETCH_HEAD"])
        .await
        .ok()?;
    if !out.success() {
        return None;
    }
    let rel = out.stdout_string().trim().to_string();
    if rel.is_empty() {
        return None;
    }
    let rel_path = Path::new(&rel);
    let full = if rel_path.is_absolute() {
        rel_path.to_path_buf()
    } else {
        repo_path.join(rel_path)
    };
    let modified = tokio::fs::metadata(&full).await.ok()?.modified().ok()?;
    let when: chrono::DateTime<chrono::Utc> = modified.into();
    Some(when.to_rfc3339())
}

async fn get_last_activity(
    runner: &GitRunner,
    repo_path: &Path,
    target_branch: Option<&str>,
) -> Option<String> {
    let target_ref = target_branch.unwrap_or("HEAD");
    // Check log commit date for target ref
    let out = runner
        .run_read(repo_path, &["log", "-1", "--format=%cI", target_ref])
        .await
        .ok()?;

    if out.success() {
        let str = out.stdout_string().trim().to_string();
        if !str.is_empty() {
            return Some(str);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_porcelain_v2_clean_branch() {
        let input = b"# branch.oid 0123456789abcdef\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +2 -1\0";
        let parsed = parse_porcelain_v2(input);
        assert_eq!(parsed.branch, Some("main".to_string()));
        assert_eq!(parsed.detached_sha, None);
        assert_eq!(parsed.upstream, Some("origin/main".to_string()));
        assert_eq!(parsed.ahead, 2);
        assert_eq!(parsed.behind, 1);
        assert_eq!(parsed.dirty_count, 0);
    }

    #[test]
    fn test_parse_porcelain_v2_detached_head() {
        let input = b"# branch.oid 7f3a2c1000000000\0# branch.head (detached)\0# branch.ab +0 -0\0";
        let parsed = parse_porcelain_v2(input);
        assert_eq!(parsed.branch, None);
        assert_eq!(parsed.detached_sha, Some("7f3a2c1000000000".to_string()));
        assert_eq!(parsed.upstream, None);
        assert_eq!(parsed.ahead, 0);
        assert_eq!(parsed.behind, 0);
    }

    #[test]
    fn test_parse_porcelain_v2_dirty_files_and_renames() {
        let input = b"# branch.oid abc\x00# branch.head feat\x001 .M N... 100644 100644 100644 abc abc src/main.rs\x002 R. N... 100644 100644 100644 abc abc R100 new.rs\x00old.rs\x00? untracked.txt\x00";
        let parsed = parse_porcelain_v2(input);
        assert_eq!(parsed.branch, Some("feat".to_string()));
        assert_eq!(parsed.dirty_count, 3);
    }

    #[test]
    fn test_rebase_head_branch_detaches_to_viewed_branch() {
        let base = std::env::temp_dir().join(format!("vantage_rebase_{}", uuid::Uuid::new_v4()));
        let git_dir = base.join("gitdir");
        std::fs::create_dir_all(git_dir.join("rebase-merge")).unwrap();

        std::fs::write(
            git_dir.join("rebase-merge").join("head-name"),
            "refs/heads/feat/conflicts\n",
        )
        .unwrap();
        assert_eq!(
            rebase_head_branch(&git_dir),
            Some("feat/conflicts".to_string())
        );

        // Plain names are returned as-is, and rebase-apply is a fallback.
        std::fs::remove_dir_all(git_dir.join("rebase-merge")).unwrap();
        std::fs::create_dir_all(git_dir.join("rebase-apply")).unwrap();
        std::fs::write(git_dir.join("rebase-apply").join("head-name"), "main\n").unwrap();
        assert_eq!(rebase_head_branch(&git_dir), Some("main".to_string()));
        assert_eq!(rebase_head_branch(&base.join("missing")), None);

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_parse_porcelain_v2_unmerged_entries() {
        // Real `git status --porcelain=v2 -z` output while a merge conflicts.
        let input = b"# branch.oid abc\0# branch.head main\0u UU N... 100644 100644 100644 100644 aaa bbb ccc f.txt\0u AA N... 100644 100644 100644 100644 aaa bbb ccc dir with space/other file.txt\0";
        let parsed = parse_porcelain_v2(input);
        assert_eq!(parsed.branch, Some("main".to_string()));
        // Each unmerged entry still counts as a dirty file.
        assert_eq!(parsed.dirty_count, 2);
        assert_eq!(
            parsed.conflicted_paths,
            vec![
                "f.txt".to_string(),
                "dir with space/other file.txt".to_string()
            ]
        );
    }

    #[test]
    fn test_parse_unmerged_paths_ignores_other_records() {
        let input = b"1 .M N... 100644 100644 100644 abc abc src/main.rs\0u UU N... 100644 100644 100644 100644 aaa bbb ccc conflicted.rs\0? untracked.txt\0";
        assert_eq!(
            parse_unmerged_paths(input),
            vec!["conflicted.rs".to_string()]
        );
    }

    #[test]
    fn test_parse_unmerged_ls_files_dedupes_stages() {
        let input = b"100644 aaa 1\tf.txt\x00100644 bbb 2\tf.txt\x00100644 ccc 3\tf.txt\x00100644 ddd 2\tsub/g.txt\x00";
        assert_eq!(
            parse_unmerged_ls_files(input),
            vec!["f.txt".to_string(), "sub/g.txt".to_string()]
        );
    }

    #[test]
    fn test_detect_operation_precedence_and_none() {
        let base = std::env::temp_dir().join(format!("vantage_status_{}", uuid::Uuid::new_v4()));
        let git_dir = base.join("gitdir");
        std::fs::create_dir_all(&git_dir).unwrap();

        // Nothing in progress.
        assert_eq!(detect_operation(&git_dir), None);

        // Merge.
        std::fs::write(git_dir.join("MERGE_HEAD"), "abc").unwrap();
        assert_eq!(detect_operation(&git_dir), Some(OperationKind::Merge));
        std::fs::remove_file(git_dir.join("MERGE_HEAD")).unwrap();

        // Cherry-pick.
        std::fs::write(git_dir.join("CHERRY_PICK_HEAD"), "abc").unwrap();
        assert_eq!(detect_operation(&git_dir), Some(OperationKind::CherryPick));
        std::fs::remove_file(git_dir.join("CHERRY_PICK_HEAD")).unwrap();

        // Revert.
        std::fs::write(git_dir.join("REVERT_HEAD"), "abc").unwrap();
        assert_eq!(detect_operation(&git_dir), Some(OperationKind::Revert));
        std::fs::remove_file(git_dir.join("REVERT_HEAD")).unwrap();

        // Rebase wins over a lingering CHERRY_PICK_HEAD.
        std::fs::create_dir_all(git_dir.join("rebase-merge")).unwrap();
        std::fs::write(git_dir.join("CHERRY_PICK_HEAD"), "abc").unwrap();
        assert_eq!(detect_operation(&git_dir), Some(OperationKind::Rebase));

        let _ = std::fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn last_fetch_at_reads_fetch_head_mtime() {
        let runner = GitRunner::new();
        let repo =
            std::env::temp_dir().join(format!("vantage_fetch_head_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&repo).unwrap();
        runner.run_write(&repo, &["init"], 10).await.unwrap();

        assert_eq!(get_last_fetch_at(&runner, &repo).await, None);

        std::fs::write(
            repo.join(".git/FETCH_HEAD"),
            "abc123\t\tbranch 'main' of https://example.com/repo\n",
        )
        .unwrap();
        let stamp = get_last_fetch_at(&runner, &repo)
            .await
            .expect("FETCH_HEAD should produce a timestamp");
        assert!(chrono::DateTime::parse_from_rfc3339(&stamp).is_ok());

        let _ = std::fs::remove_dir_all(&repo);
    }
}
