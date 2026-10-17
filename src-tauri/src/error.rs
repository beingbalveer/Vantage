use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: String,
    pub message: String,
    pub stderr: Option<String>,
}

impl AppError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            stderr: None,
        }
    }

    pub fn with_stderr(mut self, stderr: impl Into<String>) -> Self {
        self.stderr = Some(stderr.into());
        self
    }

    pub fn git_missing() -> Self {
        Self::new("GitMissing", "Git executable was not found on PATH.")
    }

    pub fn not_a_repo(path: &str) -> Self {
        Self::new(
            "NotARepo",
            format!("The path '{}' is not a Git repository.", path),
        )
    }

    pub fn path_missing(path: &str) -> Self {
        Self::new(
            "PathMissing",
            format!("The path '{}' does not exist.", path),
        )
    }

    pub fn timeout(msg: impl Into<String>) -> Self {
        Self::new("Timeout", msg)
    }

    pub fn auth_failed(stderr: Option<String>) -> Self {
        let mut err = Self::new(
            "AuthFailed",
            "Git needs credentials. Run `git fetch` in the terminal to sign in.",
        );
        err.stderr = stderr;
        err
    }

    pub fn dirty(msg: impl Into<String>) -> Self {
        Self::new("Dirty", msg)
    }

    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::new("Conflict", msg)
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::new("NotFound", msg)
    }

    pub fn too_large(msg: impl Into<String>) -> Self {
        Self::new("TooLarge", msg)
    }

    pub fn io(err: std::io::Error) -> Self {
        Self::new("Io", err.to_string())
    }

    pub fn other(msg: impl Into<String>) -> Self {
        Self::new("Other", msg)
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}]: {}", self.code, self.message)
    }
}

impl std::error::Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::io(e)
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::new("Serialization", e.to_string())
    }
}
