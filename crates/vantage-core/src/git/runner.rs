use crate::error::AppError;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::{watch, Mutex, Semaphore};

/// Cooperative cancellation for a long-running git process.
///
/// The UI holds a clone and calls [`CancelToken::cancel`] from its own thread;
/// the runner waits for it alongside the child process and kills the child on
/// cancellation. Cancellation is one-way: a token cannot be reset.
#[derive(Clone)]
pub struct CancelToken {
    sender: watch::Sender<bool>,
    receiver: watch::Receiver<bool>,
}

impl Default for CancelToken {
    fn default() -> Self {
        let (sender, receiver) = watch::channel(false);
        Self { sender, receiver }
    }
}

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ask the running operation to stop. Safe to call repeatedly.
    pub fn cancel(&self) {
        let _ = self.sender.send(true);
    }

    pub fn is_cancelled(&self) -> bool {
        *self.receiver.borrow()
    }

    /// Resolve once [`CancelToken::cancel`] has been called.
    pub async fn cancelled(&self) {
        let mut receiver = self.receiver.clone();
        if *receiver.borrow_and_update() {
            return;
        }
        while receiver.changed().await.is_ok() {
            if *receiver.borrow() {
                return;
            }
        }
    }
}

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
    program: PathBuf,
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
            program: PathBuf::from("git"),
            semaphore: Arc::new(Semaphore::new(8)),
            repo_locks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Test-only constructor that runs a stand-in executable. Lets the clone
    /// tests exercise progress and cancellation without touching the network or
    /// a real repository.
    #[cfg(test)]
    pub(crate) fn with_program(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            ..Self::new()
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

        let mut cmd = Command::new(&self.program);

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

    /// Run a git command and invoke `on_line` for every stderr line as it is
    /// produced, while still collecting the full stdout/stderr for the caller.
    /// Used for long, progress-reporting commands such as `git clone --progress`.
    /// `on_line` receives the raw line (without the trailing newline/CR).
    pub async fn run_streaming_stderr<F>(
        &self,
        repo_path: Option<&Path>,
        args: &[&str],
        timeout_secs: u64,
        on_line: F,
    ) -> Result<GitOutput, AppError>
    where
        F: FnMut(&str) + Send,
    {
        self.run_streaming_stderr_cancellable(repo_path, args, timeout_secs, None, on_line)
            .await
    }

    /// [`GitRunner::run_streaming_stderr`] with cooperative cancellation.
    ///
    /// When `cancel` fires the child is killed and reaped, and this returns an
    /// `AppError` with code `Cancelled` rather than the child's exit status.
    pub async fn run_streaming_stderr_cancellable<F>(
        &self,
        repo_path: Option<&Path>,
        args: &[&str],
        timeout_secs: u64,
        cancel: Option<&CancelToken>,
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

        let mut cmd = Command::new(&self.program);
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

        let run = async {
            let wait_fut = async {
                let mut stdout_buf = Vec::new();
                let mut stderr_buf = Vec::new();

                let read_out = async {
                    if let Some(mut s) = stdout_pipe {
                        use tokio::io::AsyncReadExt;
                        let _ = s.read_to_end(&mut stdout_buf).await;
                    }
                };

                // git progress uses `\r` between updates and `\n` at the end, so we
                // split on either character rather than lines().
                let read_err = async {
                    if let Some(mut s) = stderr_pipe {
                        use tokio::io::AsyncReadExt;
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

            // The wait future owns the mutable borrow of the child, so scope the
            // select to drop it before the caller kills or reaps the process.
            tokio::pin!(wait_fut);
            match cancel {
                Some(token) => tokio::select! {
                    finished = &mut wait_fut => Some(finished),
                    _ = token.cancelled() => None,
                },
                None => Some(wait_fut.await),
            }
        };

        let outcome = match tokio::time::timeout(Duration::from_secs(timeout_secs), run).await {
            Ok(outcome) => outcome,
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

        let Some((status_res, stdout_bytes, stderr_bytes)) = outcome else {
            let _ = child.start_kill();
            let _ = child.wait().await;
            return Err(AppError::cancelled());
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

/// Write an executable `git` stand-in for tests that must not touch real Git.
#[cfg(all(test, unix))]
pub(crate) fn fake_git_program(dir: &Path, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let path = dir.join("git");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write fake git");
    let mut permissions = std::fs::metadata(&path)
        .expect("stat fake git")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&path, permissions).expect("make fake git executable");
    path
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::time::Instant;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("vantage_runner_{}_{}", name, uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[tokio::test]
    async fn streams_each_stderr_line_as_it_arrives() {
        let dir = temp_dir("stream");
        let program = fake_git_program(
            &dir,
            "printf 'Receiving objects:  42%% (42/100)\\r' >&2\n\
             printf 'Resolving deltas: 100%%\\n' >&2\n\
             exit 0",
        );
        let runner = GitRunner::with_program(program);

        let mut lines = Vec::new();
        let output = runner
            .run_streaming_stderr(None, &["clone"], 5, |line| lines.push(line.to_string()))
            .await
            .expect("fake clone succeeds");

        assert_eq!(
            lines,
            vec![
                "Receiving objects:  42% (42/100)".to_string(),
                "Resolving deltas: 100%".to_string(),
            ]
        );
        assert!(output.status.success());
        assert!(output.stderr.contains("Receiving objects:  42% (42/100)"));
        assert!(output.stderr.contains("Resolving deltas: 100%"));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn cancellation_kills_the_child_and_reports_cancelled() {
        let dir = temp_dir("cancel");
        let program = fake_git_program(
            &dir,
            "printf 'Receiving objects:  10%% (1/10)\\n' >&2\nexec sleep 30",
        );
        let runner = GitRunner::with_program(program);
        let cancel = CancelToken::new();
        let callback_cancel = cancel.clone();
        let started = Instant::now();

        let result = runner
            .run_streaming_stderr_cancellable(None, &["clone"], 5, Some(&cancel), |line| {
                if line.contains("Receiving objects") {
                    callback_cancel.cancel();
                }
            })
            .await;

        let error = match result {
            Ok(_) => panic!("cancelling a running clone must fail the call"),
            Err(error) => error,
        };
        assert!(error.is_cancelled());
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "cancellation must not wait for the child to finish"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn cancel_token_wakes_waiters() {
        let token = CancelToken::new();
        let waiter = token.clone();
        let waiting = tokio::spawn(async move {
            waiter.cancelled().await;
            true
        });

        token.cancel();

        assert!(token.is_cancelled());
        assert!(waiting.await.expect("waiter task"));
    }
}
