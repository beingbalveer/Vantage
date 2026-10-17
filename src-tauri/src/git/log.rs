use crate::error::AppError;
use crate::git::runner::GitRunner;
use crate::state::CommitSummary;
use std::collections::HashSet;
use std::path::Path;

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

pub async fn get_log_page(
    runner: &GitRunner,
    repo_path: &Path,
    target_branch: Option<&str>,
    skip: u32,
    limit: u32,
    query: Option<&str>,
    marker_sha: Option<&str>,
) -> Result<Vec<CommitSummary>, AppError> {
    let mut args = vec![
        "log",
        "--format=%x1e%H%x1f%an%x1f%ae%x1f%aI%x1f%P%x1f%s%x1f%b",
        "--shortstat",
    ];

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

    // Determine new commits if marker is present
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

    Ok(parse_log_output(&out.stdout_string(), &new_shas))
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
}
