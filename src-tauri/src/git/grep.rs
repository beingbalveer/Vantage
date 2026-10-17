use crate::error::AppError;
use crate::git::runner::GitRunner;
use crate::state::{BlameInfo, DefHit};
use std::path::Path;

pub async fn blame_line(
    runner: &GitRunner,
    repo_path: &Path,
    rev: &str,
    path: &str,
    line: u32,
) -> Result<BlameInfo, AppError> {
    let line_arg = format!("-L{},{}", line, line);
    let out = runner
        .run_read(
            repo_path,
            &["blame", &line_arg, "--porcelain", rev, "--", path],
        )
        .await?;

    if !out.success() {
        return Err(AppError::other(out.stderr));
    }

    let mut sha = String::new();
    let mut author = String::new();
    let mut date = String::new();
    let mut subject = String::new();

    for l in out.stdout_string().lines() {
        if sha.is_empty() {
            if let Some(first_word) = l.split_whitespace().next() {
                sha = first_word.to_string();
            }
        } else if let Some(rest) = l.strip_prefix("author ") {
            author = rest.trim().to_string();
        } else if let Some(rest) = l.strip_prefix("author-time ") {
            if let Ok(ts) = rest.trim().parse::<i64>() {
                if let Some(dt) = chrono::DateTime::from_timestamp(ts, 0) {
                    date = dt.to_rfc3339();
                }
            }
        } else if let Some(rest) = l.strip_prefix("summary ") {
            subject = rest.trim().to_string();
        }
    }

    Ok(BlameInfo {
        sha,
        author,
        date,
        subject,
    })
}

pub async fn grep_definitions(
    runner: &GitRunner,
    repo_path: &Path,
    rev: &str,
    ident: &str,
    ext_hint: Option<&str>,
) -> Result<Vec<DefHit>, AppError> {
    if ident.is_empty() {
        return Ok(Vec::new());
    }

    // Patterns from Section 7 F10:
    let pattern = format!(
        r"((fn|function|def|class|struct|enum|trait|interface|type|const|let|var|func|impl|module)[[:space:]]+{}\b|{}[[:space:]]*[:=][[:space:]]*(async[[:space:]]*)?(function|\()|{}[[:space:]]*\(.*\)[[:space:]]*\{{)",
        ident, ident, ident
    );

    let mut hits = Vec::new();

    // 1. Try with extension hint first if provided
    if let Some(ext) = ext_hint {
        let glob = format!("*.{}", ext);
        hits = run_grep(runner, repo_path, rev, &pattern, Some(&glob)).await?;
    }

    // 2. If no hits, run repo-wide
    if hits.is_empty() {
        hits = run_grep(runner, repo_path, rev, &pattern, None).await?;
    }

    hits.truncate(8);
    Ok(hits)
}

async fn run_grep(
    runner: &GitRunner,
    repo_path: &Path,
    rev: &str,
    pattern: &str,
    glob: Option<&str>,
) -> Result<Vec<DefHit>, AppError> {
    let mut args = vec!["grep", "-n", "-I", "-E", pattern, rev];
    if let Some(g) = glob {
        args.push("--");
        args.push(g);
    }

    let out = runner.run_read(repo_path, &args).await?;
    if !out.success() {
        return Ok(Vec::new());
    }

    let mut results = Vec::new();
    for line in out.stdout_string().lines() {
        // Output format: <rev>:<path>:<line_no>:<line_text>
        let parts: Vec<&str> = line.splitn(4, ':').collect();
        if parts.len() >= 4 {
            let path = parts[1].to_string();
            let line_no = parts[2].parse::<u32>().unwrap_or(1);
            let line_text = parts[3].to_string();

            // Fetch 2 context lines if possible
            let context = crate::git::blob::get_blob_lines(
                runner,
                repo_path,
                rev,
                &path,
                line_no.saturating_sub(1),
                line_no + 2,
            )
            .await
            .unwrap_or_default();

            results.push(DefHit {
                path,
                line: line_no,
                line_text,
                context_lines: context,
            });

            if results.len() >= 8 {
                break;
            }
        }
    }

    Ok(results)
}
