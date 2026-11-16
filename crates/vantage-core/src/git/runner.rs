use crate::error::AppError;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::{Mutex, Semaphore};

pub struct GitOutput {
    pub stdout: Vec<u8>,
    pub stderr: String,
    pub status: std::process::ExitStatus,
}

/// Output of [`GitRunner::run_read_limited`].
pub struct LimitedGitOutput {
    pub output: GitOutput,
    /// True when stdout was cut at the caller's byte cap. The child is killed,
    /// so `output.status` is a failure even though the prefix is still useful.
    pub truncated: bool,
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
                    let _ = s.read_to_end(&mut stdout_buf).await;
                }
            };
            let read_err = async {
                if let Some(mut s) = stderr_pipe {
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

    /// Run a git command and invoke `on_line` for every stderr line as it is
    /// produced, while still collecting the full stdout/stderr for the caller.
    /// Used for long, progress-reporting commands such as `git clone --progress`.
    /// `on_line` receives the raw line (without the trailing newline/CR).
    pub async fn run_streaming_stderr<F>(
        &self,
        repo_path: Option<&Path>,
        args: &[&str],
        timeout_secs: u64,
        mut on_line: F,
    ) -> Result<GitOutput, AppError>
    where
        F: FnMut(&str) + Send,
    {
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|e| AppError::other(format!("Semaphore error: {}", e)))?;

        let mut cmd = Command::new("git");
        cmd.kill_on_drop(true);
        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        cmd.env("LC_ALL", "C")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_PAGER", "cat");
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
                    let _ = s.read_to_end(&mut stdout_buf).await;
                }
            };

            // git progress uses `\r` between updates and `\n` at the end, so we
            // split on either character rather than lines().
            let read_err = async {
                if let Some(mut s) = stderr_pipe {
                    let mut buf = [0u8; 4096];
                    let mut pending: Vec<u8> = Vec::new();
                    loop {
                        let n = match s.read(&mut buf).await {
                            Ok(0) => break,
                            Ok(n) => n,
                            Err(_) => break,
                        };
                        for &b in &buf[..n] {
                            if b == b'\n' || b == b'\r' {
                                if !pending.is_empty() {
                                    let line = String::from_utf8_lossy(&pending).to_string();
                                    stderr_buf.extend_from_slice(&pending);
                                    stderr_buf.push(b'\n');
                                    on_line(&line);
                                    pending.clear();
                                }
                            } else {
                                pending.push(b);
                            }
                        }
                    }
                    if !pending.is_empty() {
                        let line = String::from_utf8_lossy(&pending).to_string();
                        stderr_buf.extend_from_slice(&pending);
                        on_line(&line);
                    }
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
        Ok(GitOutput {
            stdout: stdout_bytes,
            stderr: String::from_utf8_lossy(&stderr_bytes).to_string(),
            status,
        })
    }

    pub async fn run_read(&self, repo_path: &Path, args: &[&str]) -> Result<GitOutput, AppError> {
        self.run(Some(repo_path), args, 20, false).await
    }

    /// Run a read command, keeping at most `max_stdout_bytes` of its output.
    ///
    /// Used for patches that are displayed a few lines at a time: a generated
    /// or vendored file can produce a diff far larger than the view will ever
    /// show. Once the cap is reached the child is killed and `truncated` is
    /// set, so callers can label the partial result instead of buffering it.
    pub async fn run_read_limited(
        &self,
        repo_path: &Path,
        args: &[&str],
        max_stdout_bytes: usize,
    ) -> Result<LimitedGitOutput, AppError> {
        self.run_limited_env(repo_path, args, max_stdout_bytes, &[])
            .await
    }

    /// [`run_read_limited`](Self::run_read_limited) with literal pathspecs, so
    /// a file name containing `*` or `[` is read as that exact path.
    pub async fn run_read_literal_limited(
        &self,
        repo_path: &Path,
        args: &[&str],
        max_stdout_bytes: usize,
    ) -> Result<LimitedGitOutput, AppError> {
        self.run_limited_env(
            repo_path,
            args,
            max_stdout_bytes,
            &[("GIT_LITERAL_PATHSPECS", "1")],
        )
        .await
    }

    async fn run_limited_env(
        &self,
        repo_path: &Path,
        args: &[&str],
        max_stdout_bytes: usize,
        extra_envs: &[(&str, &str)],
    ) -> Result<LimitedGitOutput, AppError> {
        const STDERR_CAP: usize = 64 * 1024;

        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|e| AppError::other(format!("Semaphore error: {}", e)))?;

        let mut cmd = Command::new("git");
        cmd.kill_on_drop(true);
        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        cmd.env("LC_ALL", "C")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_PAGER", "cat")
            .env("GIT_OPTIONAL_LOCKS", "0");
        for (key, value) in extra_envs {
            cmd.env(key, value);
        }
        cmd.arg("-c").arg("core.quotepath=off");
        cmd.arg("-c").arg("color.ui=never");
        cmd.arg("-C").arg(repo_path);
        cmd.args(args);

        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(AppError::git_missing());
            }
            Err(error) => return Err(AppError::io(error)),
        };

        let mut stdout_pipe = child.stdout.take();
        let mut stderr_pipe = child.stderr.take();
        let wait = async {
            let mut stdout = Vec::with_capacity(max_stdout_bytes.min(64 * 1024));
            let mut stderr = Vec::new();
            let mut truncated = false;
            let mut out_buf = [0u8; 8192];
            let mut err_buf = [0u8; 8192];
            let mut out_open = stdout_pipe.is_some();
            let mut err_open = stderr_pipe.is_some();

            while out_open || err_open {
                tokio::select! {
                    read = async {
                        stdout_pipe
                            .as_mut()
                            .expect("stdout is open")
                            .read(&mut out_buf)
                            .await
                    }, if out_open => {
                        match read {
                            Ok(0) => out_open = false,
                            Ok(read) => {
                                let room = max_stdout_bytes.saturating_sub(stdout.len());
                                if read > room {
                                    stdout.extend_from_slice(&out_buf[..room]);
                                    truncated = true;
                                    out_open = false;
                                    // The rest cannot be kept; stop git instead
                                    // of draining a patch nobody will show.
                                    let _ = child.start_kill();
                                } else {
                                    stdout.extend_from_slice(&out_buf[..read]);
                                }
                            }
                            Err(_) => out_open = false,
                        }
                    }
                    read = async {
                        stderr_pipe
                            .as_mut()
                            .expect("stderr is open")
                            .read(&mut err_buf)
                            .await
                    }, if err_open => {
                        match read {
                            Ok(0) => err_open = false,
                            Ok(read) => {
                                let room = STDERR_CAP.saturating_sub(stderr.len());
                                stderr.extend_from_slice(&err_buf[..read.min(room)]);
                            }
                            Err(_) => err_open = false,
                        }
                    }
                }
            }

            let status = child.wait().await.map_err(AppError::io)?;
            Ok::<_, AppError>((status, stdout, stderr, truncated))
        };

        let (status, stdout, stderr, truncated) =
            match tokio::time::timeout(Duration::from_secs(20), wait).await {
                Ok(result) => result?,
                Err(_) => {
                    return Err(AppError::timeout(format!(
                        "Git command timed out after 20s: git {}",
                        args.join(" ")
                    )));
                }
            };

        Ok(LimitedGitOutput {
            output: GitOutput {
                stdout,
                stderr: String::from_utf8_lossy(&stderr).to_string(),
                status,
            },
            truncated,
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn limited_read_keeps_short_output_intact() {
        let runner = GitRunner::new();
        let temp = std::env::temp_dir();
        let out = runner
            .run_read_limited(&temp, &["--version"], 4096)
            .await
            .expect("git --version");

        assert!(!out.truncated);
        assert!(out.output.success());
        assert!(out.output.stdout_string().contains("git version"));
    }

    #[tokio::test]
    async fn limited_read_stops_at_the_byte_cap() {
        let runner = GitRunner::new();
        let temp = std::env::temp_dir();
        let out = runner
            .run_read_limited(&temp, &["--version"], 4)
            .await
            .expect("git --version");

        assert!(out.truncated, "a 4-byte cap cannot fit git's version");
        assert!(out.output.stdout.len() <= 4);
    }
}
