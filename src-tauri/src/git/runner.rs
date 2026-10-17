use crate::error::AppError;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::{Mutex, Semaphore};

pub struct GitOutput {
    pub stdout: Vec<u8>,
    pub stderr: String,
    pub status: std::process::ExitStatus,
}

impl GitOutput {
    pub fn stdout_string(&self) -> String {
        String::from_utf8_lossy(&self.stdout).to_string()
    }

    pub fn success(&self) -> bool {
        self.status.success()
    }
}

pub struct GitRunner {
    semaphore: Arc<Semaphore>,
    repo_locks: Arc<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>>,
}

impl Default for GitRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl GitRunner {
    pub fn new() -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(8)),
            repo_locks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn get_repo_lock(&self, repo_path: &Path) -> Arc<Mutex<()>> {
        let canon = dunce::canonicalize(repo_path).unwrap_or_else(|_| repo_path.to_path_buf());
        let mut locks = self.repo_locks.lock().await;
        locks
            .entry(canon)
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    pub async fn run(
        &self,
        repo_path: Option<&Path>,
        args: &[&str],
        timeout_secs: u64,
        is_write: bool,
    ) -> Result<GitOutput, AppError> {
        self.run_env(repo_path, args, timeout_secs, is_write, &[])
            .await
    }

    pub async fn run_env(
        &self,
        repo_path: Option<&Path>,
        args: &[&str],
        timeout_secs: u64,
        is_write: bool,
        extra_envs: &[(&str, &str)],
    ) -> Result<GitOutput, AppError> {
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|e| AppError::other(format!("Semaphore error: {}", e)))?;

        let mut cmd = Command::new("git");

        // Standard flags and environment
        cmd.kill_on_drop(true);
        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        cmd.env("LC_ALL", "C")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_PAGER", "cat");

        if !is_write {
            cmd.env("GIT_OPTIONAL_LOCKS", "0");
        }

        for (k, v) in extra_envs {
            cmd.env(k, v);
        }

        cmd.arg("-c").arg("core.quotepath=off");
        cmd.arg("-c").arg("color.ui=never");

        if let Some(path) = repo_path {
            cmd.arg("-C").arg(path);
        }

        cmd.args(args);

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    return Err(AppError::git_missing());
                } else {
                    return Err(AppError::io(e));
                }
            }
        };

        let stdout_pipe = child.stdout.take();
        let stderr_pipe = child.stderr.take();

        let wait_fut = async {
            let mut stdout_buf = Vec::new();
            let mut stderr_buf = Vec::new();
            let read_out = async {
                if let Some(mut s) = stdout_pipe {
                    use tokio::io::AsyncReadExt;
                    let _ = s.read_to_end(&mut stdout_buf).await;
                }
            };
            let read_err = async {
                if let Some(mut s) = stderr_pipe {
                    use tokio::io::AsyncReadExt;
                    let _ = s.read_to_end(&mut stderr_buf).await;
                }
            };
            let (status, _, _) = tokio::join!(child.wait(), read_out, read_err);
            (status, stdout_buf, stderr_buf)
        };

        let (status_res, stdout_bytes, stderr_bytes) =
            match tokio::time::timeout(Duration::from_secs(timeout_secs), wait_fut).await {
                Ok(res) => res,
                Err(_) => {
                    let _ = child.start_kill();
                    let _ = child.wait().await;
                    return Err(AppError::timeout(format!(
                        "Git command timed out after {}s: git {}",
                        timeout_secs,
                        args.join(" ")
                    )));
                }
            };

        let status = status_res.map_err(AppError::io)?;
        let stderr = String::from_utf8_lossy(&stderr_bytes).to_string();

        if !status.success() {
            // Check for auth failure
            if stderr.contains("Authentication failed")
                || stderr.contains("Permission denied (publickey)")
                || stderr.contains("could not read Username")
                || stderr.contains("fatal: could not read Password")
            {
                return Err(AppError::auth_failed(Some(stderr)));
            }

            // Check for index.lock
            if stderr.contains("index.lock") {
                return Err(AppError::dirty(
                    "Another Git process is running. Try again in a moment.",
                )
                .with_stderr(stderr));
            }
        }

        Ok(GitOutput {
            stdout: stdout_bytes,
            stderr,
            status,
        })
    }

    pub async fn run_read(&self, repo_path: &Path, args: &[&str]) -> Result<GitOutput, AppError> {
        self.run(Some(repo_path), args, 20, false).await
    }

    pub async fn run_write(
        &self,
        repo_path: &Path,
        args: &[&str],
        timeout_secs: u64,
    ) -> Result<GitOutput, AppError> {
        self.run(Some(repo_path), args, timeout_secs, true).await
    }

    pub async fn run_write_literal(
        &self,
        repo_path: &Path,
        args: &[&str],
        timeout_secs: u64,
    ) -> Result<GitOutput, AppError> {
        self.run_env(
            Some(repo_path),
            args,
            timeout_secs,
            true,
            &[("GIT_LITERAL_PATHSPECS", "1")],
        )
        .await
    }

    pub async fn run_read_literal(
        &self,
        repo_path: &Path,
        args: &[&str],
    ) -> Result<GitOutput, AppError> {
        self.run_env(
            Some(repo_path),
            args,
            20,
            false,
            &[("GIT_LITERAL_PATHSPECS", "1")],
        )
        .await
    }
}
