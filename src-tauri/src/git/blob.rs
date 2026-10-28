use crate::error::AppError;
use crate::git::binary::{worktree_path, WORKTREE_REV};
use crate::git::runner::GitRunner;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::io::AsyncReadExt;

/// 5 MB cap shared by revision blobs and working-tree fallback reads.
const BLOB_CAP_BYTES: u64 = 5_000_000;

/// Local-only config files worth showing even though they are ignored. A
/// dotenv file is almost always in `.gitignore`, so `git ls-tree` never
/// returns it; the reviewer still needs to read it from the tree and search.
const DOTENV_PATHSPECS: [&str; 3] = [".env", ".env.*", "*.env"];

fn is_dotenv_path(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name == ".env" || name.starts_with(".env.") || name.ends_with(".env")
}

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

    let mut files: Vec<String> = out
        .stdout
        .split(|&b| b == 0)
        .filter(|part| !part.is_empty())
        .map(|part| String::from_utf8_lossy(part).to_string())
        .collect();

    // Ignored dotenv files live only in the worktree, so they are invisible to
    // `ls-tree`. Append them (deduplicated) after the tracked list: the tree
    // sorts itself, and keeping tracked files first preserves the "open the
    // first file on first visit" behaviour.
    for path in list_worktree_dotenv_files(runner, repo_path).await {
        if !files.iter().any(|f| f == &path) {
            files.push(path);
        }
    }

    Ok(files)
}

/// Dotenv files present in the worktree, whether untracked or ignored.
///
/// `--directory` prunes wholly-ignored trees (`node_modules`, `target`, …) so
/// this does not walk a large repository just to find a handful of config
/// files. The directory entries git reports are filtered out here.
async fn list_worktree_dotenv_files(runner: &GitRunner, repo_path: &Path) -> Vec<String> {
    let mut files: Vec<String> = Vec::new();

    for ignored in [None, Some("--ignored")] {
        let mut args = vec![
            "ls-files",
            "-z",
            "--others",
            "--exclude-standard",
            "--directory",
        ];
        if let Some(flag) = ignored {
            args.push(flag);
        }
        args.push("--");
        args.extend(DOTENV_PATHSPECS);

        let Ok(out) = runner.run_read(repo_path, &args).await else {
            continue;
        };
        if !out.success() {
            continue;
        }

        for part in out.stdout.split(|&b| b == 0) {
            if part.is_empty() {
                continue;
            }
            let path = String::from_utf8_lossy(part).to_string();
            if path.ends_with('/') || !is_dotenv_path(&path) {
                continue;
            }
            if !files.iter().any(|f| f == &path) {
                files.push(path);
            }
        }
    }

    files
}

pub async fn get_blob_text(
    runner: &GitRunner,
    repo_path: &Path,
    rev: &str,
    path: &str,
) -> Result<BlobTextResult, AppError> {
    if rev == WORKTREE_REV {
        return read_worktree_text(repo_path, path).await;
    }

    let rev_path = format!("{}:{}", rev, path);

    // 1. Check size with cat-file -s
    let size_out = runner
        .run_read(repo_path, &["cat-file", "-s", &rev_path])
        .await?;
    if !size_out.success() {
        // Ignored files have no blob at any revision; fall back to the
        // working-tree copy so the viewer can still open them.
        return read_worktree_text(repo_path, path).await;
    }

    let size = size_out.stdout_string().trim().parse::<u64>().unwrap_or(0);

    // 5 MB cap
    if size > BLOB_CAP_BYTES {
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

/// Read a bounded text file from the working tree. Only paths that resolve
/// inside the repository are allowed; symlinks are not followed.
async fn read_worktree_text(repo_path: &Path, path: &str) -> Result<BlobTextResult, AppError> {
    let missing = || AppError::not_found(format!("File not found: {}", path));

    let full = worktree_path(repo_path, path).ok_or_else(missing)?;
    let meta = tokio::fs::symlink_metadata(&full)
        .await
        .map_err(|_| missing())?;
    if !meta.file_type().is_file() {
        return Err(missing());
    }
    if meta.len() > BLOB_CAP_BYTES {
        return Ok(BlobTextResult {
            text: String::new(),
            too_large: true,
            binary: false,
        });
    }

    let file = tokio::fs::File::open(&full)
        .await
        .map_err(|e| AppError::other(e.to_string()))?;
    let mut bytes = Vec::new();
    file.take(BLOB_CAP_BYTES + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|e| AppError::other(e.to_string()))?;

    // The file may have grown since the size probe.
    if bytes.len() as u64 > BLOB_CAP_BYTES {
        return Ok(BlobTextResult {
            text: String::new(),
            too_large: true,
            binary: false,
        });
    }

    if bytes.iter().take(8000).any(|&b| b == 0) {
        return Ok(BlobTextResult {
            text: String::new(),
            too_large: false,
            binary: true,
        });
    }

    Ok(BlobTextResult {
        text: String::from_utf8_lossy(&bytes).to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct TempRepo {
        path: PathBuf,
    }

    impl TempRepo {
        async fn new(runner: &GitRunner) -> Self {
            let path =
                std::env::temp_dir().join(format!("vantage_blob_test_{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&path).unwrap();
            for args in [
                vec!["init"],
                vec!["config", "user.name", "Vantage Test"],
                vec!["config", "user.email", "vantage@test.local"],
                vec!["config", "commit.gpgsign", "false"],
            ] {
                runner.run_write(&path, &args, 10).await.unwrap();
            }
            Self { path }
        }
    }

    impl Drop for TempRepo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    async fn commit_all(runner: &GitRunner, repo: &TempRepo) {
        runner
            .run_write(&repo.path, &["add", "-A"], 10)
            .await
            .unwrap();
        runner
            .run_write(&repo.path, &["commit", "-m", "snapshot"], 10)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn tree_list_includes_ignored_dotenv_files() {
        let runner = GitRunner::new();
        let repo = TempRepo::new(&runner).await;

        std::fs::write(
            repo.path.join(".gitignore"),
            "*.env\n.env.*\nnode_modules/\n",
        )
        .unwrap();
        std::fs::write(repo.path.join("README.md"), "# repo\n").unwrap();
        std::fs::write(repo.path.join(".env"), "SECRET=1\n").unwrap();
        std::fs::write(repo.path.join(".env.local"), "LOCAL=1\n").unwrap();
        std::fs::create_dir_all(repo.path.join("apps/web")).unwrap();
        std::fs::write(repo.path.join("apps/web/.env"), "WEB=1\n").unwrap();
        std::fs::create_dir_all(repo.path.join("node_modules")).unwrap();
        std::fs::write(repo.path.join("node_modules/.env"), "DEP=1\n").unwrap();
        commit_all(&runner, &repo).await;

        let files = get_tree_list(&runner, &repo.path, "HEAD").await.unwrap();

        assert!(files.contains(&"README.md".to_string()));
        assert!(files.contains(&".env".to_string()));
        assert!(files.contains(&".env.local".to_string()));
        assert!(files.contains(&"apps/web/.env".to_string()));
        // Ignored dependency trees stay out of the list.
        assert!(!files.iter().any(|f| f.starts_with("node_modules/")));
    }

    #[tokio::test]
    async fn blob_text_reads_ignored_dotenv_from_worktree() {
        let runner = GitRunner::new();
        let repo = TempRepo::new(&runner).await;

        std::fs::write(repo.path.join(".gitignore"), ".env\n").unwrap();
        std::fs::write(repo.path.join("README.md"), "# repo\n").unwrap();
        commit_all(&runner, &repo).await;

        std::fs::write(repo.path.join(".env"), "SECRET=1\n").unwrap();

        let res = get_blob_text(&runner, &repo.path, "HEAD", ".env")
            .await
            .unwrap();
        assert_eq!(res.text, "SECRET=1\n");
        assert!(!res.binary && !res.too_large);

        // The same read works through the explicit worktree revision.
        let worktree = get_blob_text(&runner, &repo.path, WORKTREE_REV, ".env")
            .await
            .unwrap();
        assert_eq!(worktree.text, "SECRET=1\n");
    }

    #[tokio::test]
    async fn blob_text_still_errors_for_missing_files() {
        let runner = GitRunner::new();
        let repo = TempRepo::new(&runner).await;

        std::fs::write(repo.path.join("README.md"), "# repo\n").unwrap();
        commit_all(&runner, &repo).await;

        let err = get_blob_text(&runner, &repo.path, "HEAD", "nope.txt").await;
        assert!(err.is_err());
    }
}
