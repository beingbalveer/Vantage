use crate::error::AppError;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;

const RING_BUFFER_CAPACITY: usize = 256 * 1024; // 256 KB

struct RingBuffer {
    buffer: VecDeque<u8>,
}

impl RingBuffer {
    fn new() -> Self {
        Self {
            buffer: VecDeque::with_capacity(RING_BUFFER_CAPACITY),
        }
    }

    fn push(&mut self, bytes: &[u8]) {
        for &b in bytes {
            if self.buffer.len() >= RING_BUFFER_CAPACITY {
                self.buffer.pop_front();
            }
            self.buffer.push_back(b);
        }
    }
}

impl std::fmt::Display for RingBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (first, second) = self.buffer.as_slices();
        let mut v = Vec::with_capacity(first.len() + second.len());
        v.extend_from_slice(first);
        v.extend_from_slice(second);
        write!(f, "{}", String::from_utf8_lossy(&v))
    }
}

pub struct TerminalSession {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn portable_pty::ChildKiller + Send + Sync>,
    ring_buffer: Arc<std::sync::Mutex<RingBuffer>>,
    pub repo_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TermOpenResult {
    pub replay: String,
}

pub struct TerminalManager {
    sessions: Arc<Mutex<HashMap<String, TerminalSession>>>,
}

impl Default for TerminalManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn default_shell() -> String {
        if cfg!(windows) {
            which::which("pwsh.exe")
                .or_else(|_| which::which("powershell.exe"))
                .or_else(|_| which::which("cmd.exe"))
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| "powershell.exe".to_string())
        } else {
            std::env::var("SHELL")
                .ok()
                .filter(|path| std::path::Path::new(path).is_file())
                .or_else(|| {
                    which::which("sh")
                        .ok()
                        .map(|p| p.to_string_lossy().to_string())
                })
                .unwrap_or_else(|| "/bin/sh".to_string())
        }
    }

    pub async fn open(
        &self,
        app_handle: AppHandle,
        repo_id: String,
        repo_path: PathBuf,
        shell_override: Option<String>,
        cols: u16,
        rows: u16,
    ) -> Result<TermOpenResult, AppError> {
        let mut sessions = self.sessions.lock().await;

        if let Some(session) = sessions.get_mut(&repo_id) {
            let _ = session.master.resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            });
            let replay = session.ring_buffer.lock().unwrap().to_string();
            return Ok(TermOpenResult { replay });
        }

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| AppError::other(format!("Failed to open PTY: {}", e)))?;

        let shell = shell_override.unwrap_or_else(Self::default_shell);
        let mut cmd = CommandBuilder::new(&shell);
        cmd.cwd(&repo_path);

        let mut child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| AppError::other(format!("Failed to spawn shell '{}': {}", shell, e)))?;

        let killer = child.clone_killer();

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| AppError::other(format!("Failed to clone PTY reader: {}", e)))?;

        let writer = pair
            .master
            .take_writer()
            .map_err(|e| AppError::other(format!("Failed to take PTY writer: {}", e)))?;

        let ring_buffer = Arc::new(std::sync::Mutex::new(RingBuffer::new()));
        let ring_clone = ring_buffer.clone();
        let r_id = repo_id.clone();
        let app = app_handle.clone();

        // Spawn background reader thread with streaming UTF-8 buffer (R51)
        std::thread::spawn(move || {
            let mut read_buf = [0u8; 4096];
            let mut remainder = Vec::new();

            loop {
                match reader.read(&mut read_buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let mut chunk = std::mem::take(&mut remainder);
                        chunk.extend_from_slice(&read_buf[..n]);
                        ring_clone.lock().unwrap().push(&read_buf[..n]);

                        match std::str::from_utf8(&chunk) {
                            Ok(valid_str) => {
                                let _ = app.emit(
                                    "term://output",
                                    serde_json::json!({
                                        "repoId": r_id,
                                        "data": valid_str
                                    }),
                                );
                            }
                            Err(e) => {
                                let valid_up_to = e.valid_up_to();
                                if valid_up_to > 0 {
                                    if let Ok(valid_str) =
                                        std::str::from_utf8(&chunk[..valid_up_to])
                                    {
                                        let _ = app.emit(
                                            "term://output",
                                            serde_json::json!({
                                                "repoId": r_id,
                                                "data": valid_str
                                            }),
                                        );
                                    }
                                }
                                if let Some(error_len) = e.error_len() {
                                    let lossy = String::from_utf8_lossy(
                                        &chunk[valid_up_to..valid_up_to + error_len],
                                    )
                                    .to_string();
                                    let _ = app.emit(
                                        "term://output",
                                        serde_json::json!({
                                            "repoId": r_id,
                                            "data": lossy
                                        }),
                                    );
                                    remainder = chunk[valid_up_to + error_len..].to_vec();
                                } else {
                                    remainder = chunk[valid_up_to..].to_vec();
                                }
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
            let _ = child.wait();
            let _ = app.emit(
                "term://exit",
                serde_json::json!({
                    "repoId": r_id
                }),
            );
        });

        sessions.insert(
            repo_id,
            TerminalSession {
                master: pair.master,
                writer,
                killer,
                ring_buffer: ring_buffer.clone(),
                repo_path,
            },
        );

        Ok(TermOpenResult {
            replay: String::new(),
        })
    }

    pub async fn input(&self, repo_id: &str, data: &str) -> Result<(), AppError> {
        let mut sessions = self.sessions.lock().await;
        if let Some(session) = sessions.get_mut(repo_id) {
            session
                .writer
                .write_all(data.as_bytes())
                .map_err(AppError::io)?;
            session.writer.flush().map_err(AppError::io)?;
        }
        Ok(())
    }

    pub async fn paste(&self, repo_id: &str, text: &str) -> Result<(), AppError> {
        // Bracketed paste: \x1b[200~ + text + \x1b[201~
        let bracketed = format!("\x1b[200~{}\x1b[201~", text);
        self.input(repo_id, &bracketed).await
    }

    pub async fn resize(&self, repo_id: &str, cols: u16, rows: u16) -> Result<(), AppError> {
        let mut sessions = self.sessions.lock().await;
        if let Some(session) = sessions.get_mut(repo_id) {
            let _ = session.master.resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            });
        }
        Ok(())
    }

    pub async fn kill(&self, repo_id: &str) -> Result<(), AppError> {
        let mut sessions = self.sessions.lock().await;
        if let Some(mut session) = sessions.remove(repo_id) {
            let _ = session.killer.kill();
        }
        Ok(())
    }
}
