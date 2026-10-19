use crate::error::AppError;
use crate::git::runner::GitRunner;
use crate::state::{CommitSummary, HistoryPage};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Machine-readable log format: record sep `\x1e`, field sep `\x1f`.
const LOG_FORMAT: &str = "%x1e%H%x1f%an%x1f%ae%x1f%aI%x1f%P%x1f%s%x1f%b";

/// Upper bound on the number of commits collected for a filtered search so a
/// broad query ("a", "e") cannot force us to walk an entire 100k-commit repo.
const SEARCH_RESULT_CAP: usize = 2000;

pub fn parse_log_output(output: &str, new_shas: &HashSet<String>) -> Vec<CommitSummary> {
    let mut commits = Vec::new();
    let records = output.split('\x1e');

    for record in records {
        let trimmed = record.trim();
        if trimmed.is_empty() {
            continue;
        }

        // The record contains the formatted commit fields separated by \x1f
        // followed by optional shortstat output
        let fields: Vec<&str> = trimmed.split('\x1f').collect();
        if fields.len() < 7 {
            continue;
        }

        let raw_sha = fields[0].trim();
        let sha = if let Some(last_token) = raw_sha.split_whitespace().last() {
            last_token.to_string()
        } else {
            raw_sha.to_string()
        };
        let author_name = fields[1].to_string();
        let author_email = fields[2].to_string();
        let date = fields[3].to_string();
        let parents_str = fields[4].trim();
        let parents: Vec<String> = if parents_str.is_empty() {
            Vec::new()
        } else {
            parents_str
                .split_whitespace()
                .map(|s| s.to_string())
                .collect()
        };
        let subject = fields[5].to_string();

        // Field 6 has body, which might also contain the trailing shortstat
        let rest = fields[6];
        let mut files_changed = 0;
        let mut additions = 0;
        let mut deletions = 0;

        // Extract shortstat from the end of rest if present
        let mut lines: Vec<&str> = rest.lines().collect();
        if let Some(last_line) = lines.last() {
            if last_line.contains("changed")
                || last_line.contains("insertion")
                || last_line.contains("deletion")
            {
                parse_shortstat(
                    last_line,
                    &mut files_changed,
                    &mut additions,
                    &mut deletions,
                );
                lines.pop();
            }
        }
        let body = lines.join("\n").trim().to_string();

        let short = if sha.len() >= 7 {
            sha[..7].to_string()
        } else {
            sha.clone()
        };

        let is_new = new_shas.contains(&sha);

        commits.push(CommitSummary {
            sha,
            short,
            author_name,
            author_email,
            date,
            subject,
            body,
            parents,
            files_changed,
            additions,
            deletions,
            flags: Vec::new(),
            is_new,
        });
    }

    commits
}

fn parse_shortstat(line: &str, files_changed: &mut u32, additions: &mut u32, deletions: &mut u32) {
    for part in line.split(',') {
        let p = part.trim();
        if p.contains("file") {
            if let Some(num_str) = p.split_whitespace().next() {
                *files_changed = num_str.parse().unwrap_or(0);
            }
        } else if p.contains("insertion") {
            if let Some(num_str) = p.split_whitespace().next() {
                *additions = num_str.parse().unwrap_or(0);
            }
        } else if p.contains("deletion") {
            if let Some(num_str) = p.split_whitespace().next() {
                *deletions = num_str.parse().unwrap_or(0);
            }
        }
    }
}

/// Resolve the set of commits in `marker..target_ref` so log rows can be marked
/// `is_new`. Best-effort: a missing/diverged marker yields an empty set.
async fn resolve_new_shas(
    runner: &GitRunner,
    repo_path: &Path,
    marker_sha: Option<&str>,
    target_ref: &str,
) -> HashSet<String> {
    let mut new_shas = HashSet::new();
    if let Some(m_sha) = marker_sha {
        let rev_range = format!("{}..{}", m_sha, target_ref);
        if let Ok(rev_out) = runner.run_read(repo_path, &["rev-list", &rev_range]).await {
            if rev_out.success() {
                for line in rev_out.stdout_string().lines() {
                    let s = line.trim();
                    if !s.is_empty() {
                        new_shas.insert(s.to_string());
                    }
                }
            }
        }
    }
    new_shas
}

/// Total commits reachable from `target_ref` (`git rev-list --count`).
async fn get_history_count(
    runner: &GitRunner,
    repo_path: &Path,
    target_ref: &str,
) -> Result<u32, AppError> {
    let out = runner
        .run_read(repo_path, &["rev-list", "--count", target_ref])
        .await?;
    if !out.success() {
        return Err(AppError::other(out.stderr));
    }
    Ok(out.stdout_string().trim().parse::<u32>().unwrap_or(0))
}

pub async fn get_log_page(
    runner: &GitRunner,
    repo_path: &Path,
    target_branch: Option<&str>,
    skip: u32,
    limit: u32,
    query: Option<&str>,
    marker_sha: Option<&str>,
) -> Result<Vec<CommitSummary>, AppError> {
    let mut args = vec!["log", LOG_FORMAT, "--shortstat"];

    let skip_str = skip.to_string();
    let limit_str = limit.to_string();
    args.push("-n");
    args.push(&limit_str);
    args.push("--skip");
    args.push(&skip_str);

    let grep_arg;
    if let Some(q) = query {
        if !q.trim().is_empty() {
            grep_arg = format!("--grep={}", q.trim());
            args.push(&grep_arg);
            args.push("-i");
        }
    }

    let target_ref = target_branch.unwrap_or("HEAD");
    args.push(target_ref);

    let out = runner.run_read(repo_path, &args).await?;
    if !out.success() {
        return Err(AppError::other(out.stderr));
    }

    let new_shas = resolve_new_shas(runner, repo_path, marker_sha, target_ref).await;
    Ok(parse_log_output(&out.stdout_string(), &new_shas))
}

/// One offset page of history plus the total available, used by the UI to page
/// with "Load more" without lying about a hard 200-commit cap.
///
/// When `query` is non-empty the page is served from a filtered search that
/// matches the commit subject/body, the author name/email, or a touched path.
pub async fn get_history_page(
    runner: &GitRunner,
    repo_path: &Path,
    target_branch: Option<&str>,
    skip: u32,
    limit: u32,
    query: Option<&str>,
    marker_sha: Option<&str>,
) -> Result<HistoryPage, AppError> {
    let target_ref = target_branch.unwrap_or("HEAD");

    if let Some(q) = query {
        let q = q.trim();
        if !q.is_empty() {
            return search_history(runner, repo_path, target_ref, q, skip, limit, marker_sha).await;
        }
    }

    let mut args = vec!["log", LOG_FORMAT, "--shortstat"];
    let limit_str = limit.to_string();
    let skip_str = skip.to_string();
    args.push("-n");
    args.push(&limit_str);
    args.push("--skip");
    args.push(&skip_str);
    args.push(target_ref);

    let out = runner.run_read(repo_path, &args).await?;
    if !out.success() {
        return Err(AppError::other(out.stderr));
    }

    let new_shas = resolve_new_shas(runner, repo_path, marker_sha, target_ref).await;
    let commits = parse_log_output(&out.stdout_string(), &new_shas);
    let total = get_history_count(runner, repo_path, target_ref)
        .await
        .unwrap_or(commits.len() as u32);

    Ok(HistoryPage {
        commits,
        total,
        capped: false,
    })
}

/// Escape the glob metacharacters git pathspecs understand so a user query is
/// matched literally (`*`/`?`/`[`/`]`/`\` become plain characters).
fn escape_pathspec_literal(query: &str) -> String {
    let mut out = String::with_capacity(query.len());
    for c in query.chars() {
        if matches!(c, '*' | '?' | '[' | ']' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Regex-escape a literal query for POSIX **extended** regex (`--extended-regexp`).
///
/// `git log --grep`/`--author` accept a regex, not a literal, and the
/// `--fixed-strings` flag that used to make them literal only exists in git
/// 2.38+ (and is absent on Ubuntu 22.04's git 2.34 and older Xcode CLT git).
/// Escaping the metacharacters ourselves gives literal-substring semantics on
/// every git version with no version floor: `.`/`*`/`[`/`\` etc. are matched
/// character-for-character.
fn escape_ere_literal(query: &str) -> String {
    let mut out = String::with_capacity(query.len());
    for c in query.chars() {
        if matches!(
            c,
            '.' | '^' | '$' | '*' | '+' | '?' | '|' | '(' | ')' | '{' | '}' | '[' | ']' | '\\'
        ) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Merge per-source `(sha, commit timestamp)` pairs into a newest-first,
/// de-duplicated SHA list, truncated to `cap`.
fn order_search_shas(acc: HashMap<String, i64>, cap: usize) -> Vec<String> {
    let mut entries: Vec<(String, i64)> = acc.into_iter().collect();
    // Newest first; SHA breaks ties so paging is stable across requests.
    entries.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    entries.truncate(cap);
    entries.into_iter().map(|(sha, _)| sha).collect()
}

/// Run one filtered `git log --format=%H %ct` source and fold the results into
/// `acc`. Errors are swallowed only for a missing ref (returned as empty).
async fn collect_search_source(
    runner: &GitRunner,
    repo_path: &Path,
    target_ref: &str,
    opts: &[&str],
    pathspec: Option<&str>,
    acc: &mut HashMap<String, i64>,
) -> Result<(), AppError> {
    let mut args: Vec<&str> = vec!["log", "--format=%H %ct"];
    args.extend_from_slice(opts);
    args.push(target_ref);
    if let Some(p) = pathspec {
        args.push("--");
        args.push(p);
    }

    let out = runner.run_read(repo_path, &args).await?;
    if !out.success() {
        // `git log` exits 0 for "no matches"; a non-zero exit means the filter
        // or ref was rejected. Surface it rather than silently showing nothing.
        return Err(AppError::other(out.stderr));
    }

    for line in out.stdout_string().lines() {
        let mut parts = line.split_whitespace();
        if let (Some(sha), Some(ts)) = (parts.next(), parts.next()) {
            if let Ok(ct) = ts.parse::<i64>() {
                acc.entry(sha.to_string()).or_insert(ct);
            }
        }
    }
    Ok(())
}

/// Ordered, de-duplicated SHAs matching `query` across message, author and
/// touched path. Message/author are separate `--grep`/`--author` sources and
/// path is a pathspec source because git ANDs those predicate families.
async fn search_commit_shas(
    runner: &GitRunner,
    repo_path: &Path,
    target_ref: &str,
    query: &str,
    cap: usize,
) -> Result<Vec<String>, AppError> {
    let n = cap.to_string();
    let mut acc: HashMap<String, i64> = HashMap::new();
    // Literal matching without `--fixed-strings` (git < 2.38): escape the query
    // and opt into POSIX extended regex, which exists on every supported git.
    let escaped = escape_ere_literal(query);

    let grep_arg = format!("--grep={}", escaped);
    collect_search_source(
        runner,
        repo_path,
        target_ref,
        &["-i", "--extended-regexp", "-n", &n, &grep_arg],
        None,
        &mut acc,
    )
    .await?;

    let author_arg = format!("--author={}", escaped);
    collect_search_source(
        runner,
        repo_path,
        target_ref,
        &["-i", "--extended-regexp", "-n", &n, &author_arg],
        None,
        &mut acc,
    )
    .await?;

    let pathspec = format!(":(icase)*{}*", escape_pathspec_literal(query));
    collect_search_source(
        runner,
        repo_path,
        target_ref,
        &["-n", &n],
        Some(&pathspec),
        &mut acc,
    )
    .await?;

    Ok(order_search_shas(acc, cap))
}

/// Fetch full summaries for an explicit, ordered list of SHAs. `--no-walk=unsorted`
/// keeps the caller's order (which is the merged search order).
async fn summaries_for_shas(
    runner: &GitRunner,
    repo_path: &Path,
    shas: &[String],
    new_shas: &HashSet<String>,
) -> Result<Vec<CommitSummary>, AppError> {
    if shas.is_empty() {
        return Ok(Vec::new());
    }

    let mut args: Vec<&str> = vec!["log", "--no-walk=unsorted", LOG_FORMAT, "--shortstat"];
    for s in shas {
        args.push(s.as_str());
    }

    let out = runner.run_read(repo_path, &args).await?;
    if !out.success() {
        return Err(AppError::other(out.stderr));
    }
    Ok(parse_log_output(&out.stdout_string(), new_shas))
}

async fn search_history(
    runner: &GitRunner,
    repo_path: &Path,
    target_ref: &str,
    query: &str,
    skip: u32,
    limit: u32,
    marker_sha: Option<&str>,
) -> Result<HistoryPage, AppError> {
    let all_shas = search_commit_shas(runner, repo_path, target_ref, query, SEARCH_RESULT_CAP).await?;
    let capped = all_shas.len() >= SEARCH_RESULT_CAP;
    let total = all_shas.len() as u32;

    let page: Vec<String> = all_shas
        .into_iter()
        .skip(skip as usize)
        .take(limit as usize)
        .collect();

    let new_shas = resolve_new_shas(runner, repo_path, marker_sha, target_ref).await;
    let commits = summaries_for_shas(runner, repo_path, &page, &new_shas).await?;

    Ok(HistoryPage {
        commits,
        total,
        capped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_log_output_with_special_characters() {
        let input = "\x1e7f3a2c1000000000\x1fCodex\x1fagent@codex.ai\x1f2026-10-15T10:00:00Z\x1fa19be02\x1ffix: resolve \"quotes\" & <tags>\x1fThis is a body line.\nAnother line with special: \x1d and Unicode \u{1F916}\n\n 2 files changed, 10 insertions(+), 3 deletions(-)";
        let mut new_shas = HashSet::new();
        new_shas.insert("7f3a2c1000000000".to_string());

        let commits = parse_log_output(input, &new_shas);
        assert_eq!(commits.len(), 1);
        let c = &commits[0];
        assert_eq!(c.sha, "7f3a2c1000000000");
        assert_eq!(c.short, "7f3a2c1");
        assert_eq!(c.author_name, "Codex");
        assert_eq!(c.subject, "fix: resolve \"quotes\" & <tags>");
        assert!(c.body.contains("Unicode 🤖"));
        assert_eq!(c.files_changed, 2);
        assert_eq!(c.additions, 10);
        assert_eq!(c.deletions, 3);
        assert!(c.is_new);
    }

    #[test]
    fn test_parse_log_output_multiple_commits_with_shortstat() {
        let input = "\x1e1c8a9ce4172561e3cfc48279a70ba5f3569abd95\x1fAuthor A\x1fa@a.com\x1f2026-10-15T10:00:00Z\x1fparent1\x1fSubject 1\x1fBody 1\n\n 9 files changed, 61 insertions(+), 21 deletions(-)\n\x1e72d7b0b3703dfe1aa9de18c47af14a1e239a6d94\x1fAuthor B\x1fb@b.com\x1f2026-10-15T09:00:00Z\x1fparent2\x1fSubject 2\x1fBody 2\n\n 8 files changed, 450 insertions(+), 45 deletions(-)";
        let new_shas = HashSet::new();
        let commits = parse_log_output(input, &new_shas);
        assert_eq!(commits.len(), 2);

        assert_eq!(commits[0].sha, "1c8a9ce4172561e3cfc48279a70ba5f3569abd95");
        assert_eq!(commits[0].files_changed, 9);
        assert_eq!(commits[0].additions, 61);
        assert_eq!(commits[0].deletions, 21);
        assert_eq!(commits[0].body, "Body 1");

        assert_eq!(commits[1].sha, "72d7b0b3703dfe1aa9de18c47af14a1e239a6d94");
        assert_eq!(commits[1].files_changed, 8);
        assert_eq!(commits[1].additions, 450);
        assert_eq!(commits[1].deletions, 45);
        assert_eq!(commits[1].body, "Body 2");
    }

    #[test]
    fn test_escape_pathspec_literal_escapes_globs() {
        assert_eq!(escape_pathspec_literal("Pane.svelte"), "Pane.svelte");
        assert_eq!(escape_pathspec_literal("a*b"), "a\\*b");
        assert_eq!(escape_pathspec_literal("a?b"), "a\\?b");
        assert_eq!(escape_pathspec_literal("a[0]\\b"), "a\\[0\\]\\\\b");
    }

    #[test]
    fn test_escape_ere_literal_escapes_metacharacters() {
        // Ordinary text is left untouched.
        assert_eq!(escape_ere_literal("plain"), "plain");
        assert_eq!(escape_ere_literal("balveer@users.noreply.github.com"), "balveer@users\\.noreply\\.github\\.com");
        // Every POSIX ERE metacharacter is neutralized so it matches literally.
        assert_eq!(escape_ere_literal("v0.1.0"), "v0\\.1\\.0");
        assert_eq!(escape_ere_literal("a*b"), "a\\*b");
        assert_eq!(escape_ere_literal("a?b"), "a\\?b");
        assert_eq!(escape_ere_literal("a+b"), "a\\+b");
        assert_eq!(escape_ere_literal("a[b]"), "a\\[b\\]");
        assert_eq!(escape_ere_literal("(a|b)"), "\\(a\\|b\\)");
        assert_eq!(escape_ere_literal("a{b}"), "a\\{b\\}");
        assert_eq!(escape_ere_literal("a^b$"), "a\\^b\\$");
        assert_eq!(escape_ere_literal("a\\b"), "a\\\\b");
        assert_eq!(escape_ere_literal("fix: [wip] (v2.0)"), "fix: \\[wip\\] \\(v2\\.0\\)");
    }

    #[test]
    fn test_order_search_shas_newest_first_and_deduped() {
        let mut acc = HashMap::new();
        acc.insert("ccc".to_string(), 300);
        acc.insert("aaa".to_string(), 100);
        acc.insert("bbb".to_string(), 300);
        let ordered = order_search_shas(acc, 10);
        // Newest timestamp first; equal timestamps broken by SHA ascending.
        assert_eq!(ordered, vec!["bbb", "ccc", "aaa"]);
    }

    #[test]
    fn test_order_search_shas_truncates_to_cap() {
        let mut acc = HashMap::new();
        acc.insert("a".to_string(), 3);
        acc.insert("b".to_string(), 2);
        acc.insert("c".to_string(), 1);
        assert_eq!(order_search_shas(acc, 2), vec!["a", "b"]);
    }
}
