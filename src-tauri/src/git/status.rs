use crate::git::runner::GitRunner;
use crate::state::{MarkerState, RepoStatus};
use std::path::Path;

#[derive(Debug, Default, Clone)]
pub struct ParsedStatusV2 {
    pub branch: Option<String>,
    pub detached_sha: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub dirty_count: u32,
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
        } else if line.starts_with("1 ") || line.starts_with("u ") || line.starts_with("? ") {
            result.dirty_count += 1;
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
            new_commit_count: 0,
            marker_state: MarkerState::None,
            last_activity_at: None,
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
                    new_commit_count: 0,
                    marker_state: MarkerState::None,
                    last_activity_at: None,
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
                new_commit_count: 0,
                marker_state: MarkerState::None,
                last_activity_at: None,
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

    let is_checked_out = match target_branch {
        None => true,
        Some(tb) => parsed.branch.as_deref() == Some(tb),
    };

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
        new_commit_count,
        marker_state,
        last_activity_at,
        is_checked_out,
        error: None,
    }
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
        let input = b"# branch.oid abc\0# branch.head feat\01 .M N... 100644 100644 100644 abc abc src/main.rs\02 R. N... 100644 100644 100644 abc abc R100 new.rs\0old.rs\0? untracked.txt\0";
        let parsed = parse_porcelain_v2(input);
        assert_eq!(parsed.branch, Some("feat".to_string()));
        assert_eq!(parsed.dirty_count, 3);
    }
}
