use crate::error::AppError;
use crate::git::runner::GitRunner;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};

/// Event streamed to the UI while a clone is running so the dialog can show the
/// latest progress line. Payload is `{ cloneId, line }`.
pub const CLONE_PROGRESS_EVENT: &str = "repo://clone-progress";

/// Bounded timeout for `git clone`. Cloning a large repository can take a while,
/// but a hanging interactive prompt or dead network must fail instead of blocking.
const CLONE_TIMEOUT_SECS: u64 = 300;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneProgress {
    pub clone_id: String,
    pub line: String,
}

/// Current state of the proposed destination folder. Pure data so the validation
/// rules can be unit-tested without touching the filesystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DestinationState {
    pub exists: bool,
    pub is_dir: bool,
    pub is_empty: bool,
    /// True when the destination lives inside an already-registered work tree.
    pub inside_repo: bool,
}

/// A folder name that is safe to create on disk on every platform.
pub fn is_safe_folder_name(name: &str) -> bool {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        return false;
    }
    // Never create a repo folder that shadows git's own metadata directory.
    if trimmed.eq_ignore_ascii_case(".git") {
        return false;
    }
    if trimmed
        .chars()
        .any(|c| c == '/' || c == '\\' || c == '\0' || c.is_control())
    {
        return false;
    }
    // Characters that are invalid in Windows path segments.
    if trimmed
        .chars()
        .any(|c| matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*'))
    {
        return false;
    }
    // Windows rejects trailing dots/spaces in a path segment.
    let last = trimmed.chars().last().unwrap_or(' ');
    if last == '.' || last == ' ' {
        return false;
    }
    true
}

/// Derive a sensible local folder name from a remote URL (`https`, `ssh`,
/// scp-style `git@host:owner/repo`) or a local path.
pub fn suggest_folder_name(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Drop any query string or fragment (e.g. `?ref=main`).
    let without_query = trimmed.split(['?', '#']).next().unwrap_or(trimmed);

    // Drop trailing path separators.
    let without_trailing = without_query.trim_end_matches(['/', '\\']);
    if without_trailing.is_empty() {
        return None;
    }

    // Take the last path segment (handles both `/` and `\`), then the part after
    // the final `:` so scp-style `host:owner/repo.git` also works.
    let mut segment = without_trailing
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(without_trailing);
    if let Some(idx) = segment.rfind(':') {
        segment = &segment[idx + 1..];
    }
    segment = segment.trim();

    // Strip a trailing `.git` (case-insensitive).
    let stem = if segment.to_ascii_lowercase().ends_with(".git") && segment.len() > 4 {
        &segment[..segment.len() - 4]
    } else {
        segment
    };
    let name = stem.trim_end_matches(['/', '\\']).trim();

    if is_safe_folder_name(name) {
        Some(name.to_string())
    } else {
        None
    }
}

/// Normalize a raw stderr chunk from `git clone --progress` into a single
/// displayable line, or `None` when there is nothing worth showing.
pub fn progress_message(line: &str) -> Option<String> {
    let mut cleaned = String::with_capacity(line.len());
    for ch in line.chars() {
        if ch == '\r' || ch == '\0' {
            continue;
        }
        cleaned.push(ch);
    }
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        return None;
    }

    // Collapse runs of spaces (git pads progress columns).
    let mut out = String::with_capacity(cleaned.len());
    let mut prev_space = false;
    for ch in cleaned.chars() {
        if ch == ' ' {
            if !prev_space {
                out.push(' ');
            }
            prev_space = true;
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    Some(out)
}

/// Refuse pathological destinations: existing non-empty folders, files where a
/// folder is expected, and nesting a new repo inside an existing one.
pub fn validate_destination(state: &DestinationState) -> Result<(), AppError> {
    if state.inside_repo {
        return Err(AppError::new(
            "Validation",
            "That destination is inside an existing Git repository. Choose a folder outside any repo.",
        ));
    }
    if state.exists {
        if !state.is_dir {
            return Err(AppError::new(
                "Validation",
                "A file already exists at that destination. Choose a different folder name or location.",
            ));
        }
        if !state.is_empty {
            return Err(AppError::new(
                "Validation",
                "The destination folder already exists and is not empty. Choose a different folder name or location.",
            ));
        }
    }
    Ok(())
}

/// Map git's stderr into a friendly, actionable error.
pub fn classify_clone_error(stderr: &str, url: &str) -> AppError {
    let lower = stderr.to_lowercase();

    if lower.contains("authentication failed")
        || lower.contains("permission denied (publickey)")
        || lower.contains("could not read username")
        || lower.contains("could not read password")
        || lower.contains("terminal prompts disabled")
        || lower.contains("host key verification failed")
    {
        return AppError::new(
            "AuthFailed",
            "Git needs credentials for this repository. Configure a credential helper or an SSH key, then try again.",
        )
        .with_stderr(stderr);
    }

    if lower.contains("already exists and is not an empty directory") {
        return AppError::new(
            "Validation",
            "The destination folder already exists and is not empty. Choose a different folder name or location.",
        )
        .with_stderr(stderr);
    }

    if lower.contains("could not resolve host")
        || lower.contains("failed to connect")
        || lower.contains("network is unreachable")
        || lower.contains("connection timed out")
        || lower.contains("operation timed out")
        || lower.contains("connection refused")
    {
        return AppError::new(
            "Other",
            "Couldn't reach the remote host. Check your network connection and the repository URL.",
        )
        .with_stderr(stderr);
    }

    if lower.contains("repository not found")
        || lower.contains("does not appear to be a git repository")
        || lower.contains("repository does not exist")
        || lower.contains("not found")
    {
        return AppError::new(
            "NotFound",
            format!("Repository '{}' was not found. Check the URL.", url),
        )
        .with_stderr(stderr);
    }

    let first_line = stderr
        .lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty())
        .unwrap_or("Clone failed");
    AppError::other(format!("Clone failed: {}", first_line)).with_stderr(stderr)
}

async fn is_inside_git_repo(runner: &GitRunner, dir: &Path) -> bool {
    matches!(
        runner.run_read(dir, &["rev-parse", "--show-toplevel"]).await,
        Ok(out) if out.success()
    )
}

/// Clone `url` into `<dest_parent>/<name>`, streaming progress, then return the
/// final resolved directory (the git toplevel, canonicalized).
pub async fn clone_repo(
    runner: &GitRunner,
    app_handle: &AppHandle,
    clone_id: &str,
    url: &str,
    dest_parent: &str,
    name: Option<&str>,
    branch: Option<&str>,
) -> Result<String, AppError> {
    let url = url.trim();
    if url.is_empty() {
        return Err(AppError::new(
            "Validation",
            "Enter a repository URL or local path.",
        ));
    }

    // The destination parent folder is chosen through the folder picker.
    let parent = PathBuf::from(dest_parent);
    if !parent.exists() {
        return Err(AppError::path_missing(dest_parent));
    }
    if !parent.is_dir() {
        return Err(AppError::new(
            "Validation",
            "The destination must be a folder.",
        ));
    }
    let parent = dunce::canonicalize(&parent).map_err(AppError::io)?;

    // Resolve the folder name: explicit override, else derived from the URL.
    let folder_name = match name.map(|s| s.trim()).filter(|s| !s.is_empty()) {
        Some(n) => {
            if !is_safe_folder_name(n) {
                return Err(AppError::new(
                    "Validation",
                    "Enter a valid folder name (it can't contain path separators).",
                ));
            }
            n.to_string()
        }
        None => suggest_folder_name(url).ok_or_else(|| {
            AppError::new(
                "Validation",
                "Couldn't determine a folder name from that URL. Enter a name for the local folder.",
            )
        })?,
    };

    let dest = parent.join(&folder_name);

    let state = DestinationState {
        exists: dest.exists(),
        is_dir: dest.is_dir(),
        is_empty: if dest.is_dir() {
            std::fs::read_dir(&dest)
                .map(|mut it| it.next().is_none())
                .unwrap_or(false)
        } else {
            true
        },
        inside_repo: is_inside_git_repo(runner, &parent).await,
    };
    validate_destination(&state)?;

    let dest_str = dest.to_string_lossy().to_string();

    let mut args: Vec<String> = vec!["clone".into(), "--progress".into()];
    if let Some(b) = branch.map(|s| s.trim()).filter(|s| !s.is_empty()) {
        args.push("--branch".into());
        args.push(b.to_string());
    }
    args.push("--".into());
    args.push(url.to_string());
    args.push(dest_str);
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

    let emitter = app_handle.clone();
    let clone_id_owned = clone_id.to_string();
    let out = runner
        .run_streaming_stderr(None, &arg_refs, CLONE_TIMEOUT_SECS, move |line| {
            if let Some(message) = progress_message(line) {
                let _ = emitter.emit(
                    CLONE_PROGRESS_EVENT,
                    CloneProgress {
                        clone_id: clone_id_owned.clone(),
                        line: message,
                    },
                );
            }
        })
        .await?;

    if !out.success() {
        return Err(classify_clone_error(&out.stderr, url));
    }

    // git may have created the folder; resolve what actually landed on disk.
    let final_path = if dest.exists() {
        dunce::canonicalize(&dest).unwrap_or(dest)
    } else {
        dest
    };

    let check = runner
        .run_read(&final_path, &["rev-parse", "--show-toplevel"])
        .await?;
    if !check.success() {
        return Err(AppError::not_a_repo(&final_path.to_string_lossy()));
    }
    let top = check.stdout_string().trim().to_string();
    Ok(if top.is_empty() {
        final_path.to_string_lossy().to_string()
    } else {
        top
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggests_name_from_https_url() {
        assert_eq!(
            suggest_folder_name("https://github.com/owner/repo.git"),
            Some("repo".to_string())
        );
        assert_eq!(
            suggest_folder_name("https://github.com/owner/repo"),
            Some("repo".to_string())
        );
        assert_eq!(
            suggest_folder_name("https://github.com/owner/repo/"),
            Some("repo".to_string())
        );
    }

    #[test]
    fn suggests_name_from_scp_and_ssh_urls() {
        assert_eq!(
            suggest_folder_name("git@github.com:owner/repo.git"),
            Some("repo".to_string())
        );
        assert_eq!(
            suggest_folder_name("git@github.com:repo.git"),
            Some("repo".to_string())
        );
        assert_eq!(
            suggest_folder_name("ssh://git@host:2222/owner/repo.git"),
            Some("repo".to_string())
        );
    }

    #[test]
    fn suggests_name_from_local_paths() {
        assert_eq!(
            suggest_folder_name("/Users/x/projects/my-app"),
            Some("my-app".to_string())
        );
        assert_eq!(
            suggest_folder_name("C:\\code\\repo"),
            Some("repo".to_string())
        );
        assert_eq!(suggest_folder_name("~/foo/bar/"), Some("bar".to_string()));
    }

    #[test]
    fn ignores_query_and_fragment() {
        assert_eq!(
            suggest_folder_name("https://github.com/owner/repo.git?ref=main"),
            Some("repo".to_string())
        );
        assert_eq!(
            suggest_folder_name("https://github.com/owner/repo#readme"),
            Some("repo".to_string())
        );
    }

    #[test]
    fn rejects_empty_or_unsafe_input() {
        assert_eq!(suggest_folder_name(""), None);
        assert_eq!(suggest_folder_name("   "), None);
        assert_eq!(suggest_folder_name("/"), None);
        assert_eq!(suggest_folder_name("https://github.com/owner/.git"), None);
    }

    #[test]
    fn detects_unsafe_folder_names() {
        assert!(is_safe_folder_name("my-app"));
        assert!(is_safe_folder_name("repo_2"));
        assert!(!is_safe_folder_name(""));
        assert!(!is_safe_folder_name(".."));
        assert!(!is_safe_folder_name("a/b"));
        assert!(!is_safe_folder_name("a\\b"));
        assert!(!is_safe_folder_name("bad:name"));
        assert!(!is_safe_folder_name("trailing."));
        assert!(!is_safe_folder_name(".git"));
    }

    #[test]
    fn normalizes_progress_lines() {
        assert_eq!(
            progress_message("Receiving objects:  45% (45/100)\r"),
            Some("Receiving objects: 45% (45/100)".to_string())
        );
        assert_eq!(progress_message("\r\n"), None);
        assert_eq!(progress_message("   "), None);
        assert_eq!(
            progress_message("Cloning into 'repo'..."),
            Some("Cloning into 'repo'...".to_string())
        );
    }

    #[test]
    fn validates_destination_rules() {
        let ok_new = DestinationState {
            exists: false,
            is_dir: false,
            is_empty: true,
            inside_repo: false,
        };
        assert!(validate_destination(&ok_new).is_ok());

        let ok_empty_dir = DestinationState {
            exists: true,
            is_dir: true,
            is_empty: true,
            inside_repo: false,
        };
        assert!(validate_destination(&ok_empty_dir).is_ok());

        let non_empty = DestinationState {
            exists: true,
            is_dir: true,
            is_empty: false,
            inside_repo: false,
        };
        assert_eq!(
            validate_destination(&non_empty).unwrap_err().code,
            "Validation"
        );

        let file_exists = DestinationState {
            exists: true,
            is_dir: false,
            is_empty: true,
            inside_repo: false,
        };
        assert_eq!(
            validate_destination(&file_exists).unwrap_err().code,
            "Validation"
        );

        let nested = DestinationState {
            exists: false,
            is_dir: false,
            is_empty: true,
            inside_repo: true,
        };
        assert_eq!(validate_destination(&nested).unwrap_err().code, "Validation");
    }

    #[test]
    fn classifies_clone_errors() {
        assert_eq!(
            classify_clone_error("fatal: Authentication failed for 'https://x'", "u").code,
            "AuthFailed"
        );
        assert_eq!(
            classify_clone_error("Permission denied (publickey).", "u").code,
            "AuthFailed"
        );
        assert_eq!(
            classify_clone_error("fatal: repository 'https://x/y' not found", "u").code,
            "NotFound"
        );
        assert_eq!(
            classify_clone_error("fatal: unable to access: Could not resolve host: github.com", "u")
                .code,
            "Other"
        );
        assert_eq!(
            classify_clone_error("fatal: destination path 'x' already exists and is not an empty directory.", "u").code,
            "Validation"
        );
        assert_eq!(classify_clone_error("fatal: something odd", "u").code, "Other");
    }
}
