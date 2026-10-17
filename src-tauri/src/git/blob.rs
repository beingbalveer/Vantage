use crate::error::AppError;
use crate::git::runner::GitRunner;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlobTextResult {
    pub text: String,
    pub too_large: bool,
    pub binary: bool,
}

pub async fn get_tree_list(
    runner: &GitRunner,
    repo_path: &Path,
    rev: &str,
) -> Result<Vec<String>, AppError> {
    let out = runner
        .run_read(repo_path, &["ls-tree", "-r", "--name-only", "-z", rev])
        .await?;

    if !out.success() {
        return Err(AppError::other(out.stderr));
    }

    let files: Vec<String> = out
        .stdout
        .split(|&b| b == 0)
        .filter(|part| !part.is_empty())
        .map(|part| String::from_utf8_lossy(part).to_string())
        .collect();

    Ok(files)
}

pub async fn get_blob_text(
    runner: &GitRunner,
    repo_path: &Path,
    rev: &str,
    path: &str,
) -> Result<BlobTextResult, AppError> {
    let rev_path = format!("{}:{}", rev, path);

    // 1. Check size with cat-file -s
    let size_out = runner
        .run_read(repo_path, &["cat-file", "-s", &rev_path])
        .await?;
    if !size_out.success() {
        return Err(AppError::not_found(format!(
            "Blob not found for {}",
            rev_path
        )));
    }

    let size = size_out.stdout_string().trim().parse::<u64>().unwrap_or(0);

    // 5 MB cap
    if size > 5_000_000 {
        return Ok(BlobTextResult {
            text: String::new(),
            too_large: true,
            binary: false,
        });
    }

    // 2. Fetch content with git show <rev>:<path>
    let show_out = runner.run_read(repo_path, &["show", &rev_path]).await?;

    if !show_out.success() {
        return Err(AppError::other(show_out.stderr));
    }

    // Check if binary (contains null bytes)
    let is_binary = show_out.stdout.iter().take(8000).any(|&b| b == 0);
    if is_binary {
        return Ok(BlobTextResult {
            text: String::new(),
            too_large: false,
            binary: true,
        });
    }

    let text = String::from_utf8_lossy(&show_out.stdout).to_string();
    Ok(BlobTextResult {
        text,
        too_large: false,
        binary: false,
    })
}

pub async fn get_blob_lines(
    runner: &GitRunner,
    repo_path: &Path,
    rev: &str,
    path: &str,
    start_line: u32,
    end_line: u32,
) -> Result<Vec<String>, AppError> {
    let res = get_blob_text(runner, repo_path, rev, path).await?;
    if res.binary || res.too_large {
        return Ok(Vec::new());
    }

    let lines: Vec<&str> = res.text.lines().collect();
    let start_idx = (start_line.saturating_sub(1)) as usize;
    let end_idx = (end_line as usize).min(lines.len());

    if start_idx >= lines.len() || start_idx >= end_idx {
        return Ok(Vec::new());
    }

    Ok(lines[start_idx..end_idx]
        .iter()
        .map(|s| s.to_string())
        .collect())
}
