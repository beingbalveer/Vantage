//! GitHub profile photos for the commit identity.
//!
//! Vantage has no HTTP client dependency. Like `git` and `open`, it drives a
//! binary that is already on the machine: `curl`. Downloads are cached next to
//! `state.json`, so a photo is fetched once and then read from disk — the commit
//! bar never waits on the network twice for the same person.
//!
//! The account is resolved from evidence, never guessed: a
//! `@users.noreply.github.com` address names its login outright, `github.user`
//! is an explicit choice, and a logged-in `gh` CLI is the account this machine
//! pushes as. Without one of those the caller keeps the initials tile rather
//! than showing somebody else's face.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Re-download a cached photo after this long, so a changed avatar appears
/// without asking GitHub on every launch.
const MAX_AGE: Duration = Duration::from_secs(60 * 60 * 24 * 7);
/// Give up on a slow network rather than leaving a download thread stuck.
const FETCH_TIMEOUT_SECONDS: &str = "8";

/// What GitHub actually served.
///
/// The avatar endpoint returns whichever format the account uploaded — PNG for
/// some accounts, JPEG for most — so the bytes decide, not the URL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AvatarFormat {
    Png,
    Jpeg,
    Gif,
    Webp,
}

/// One downloaded profile photo.
#[derive(Debug)]
pub struct Avatar {
    pub format: AvatarFormat,
    pub bytes: Vec<u8>,
}

/// Identify an image from its magic bytes, so a JPEG avatar is not mistaken for
/// a failed download.
pub fn format_of(bytes: &[u8]) -> Option<AvatarFormat> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some(AvatarFormat::Png)
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(AvatarFormat::Jpeg)
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some(AvatarFormat::Gif)
    } else if bytes.len() > 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some(AvatarFormat::Webp)
    } else {
        None
    }
}

/// The GitHub login behind a commit identity, if the machine can prove one.
pub fn resolve_login(email: &str, repo_path: &Path) -> Option<String> {
    if let Some(login) = login_from_noreply_email(email) {
        return Some(login);
    }
    if let Some(login) = github_user_config(repo_path) {
        return Some(login);
    }
    gh_cli_login()
}

/// `12345+octocat@users.noreply.github.com` → `octocat`.
///
/// GitHub's private-email addresses embed the account, which makes this the
/// only exact email → login mapping available offline.
pub fn login_from_noreply_email(email: &str) -> Option<String> {
    let (local, domain) = email.trim().split_once('@')?;
    if !domain.eq_ignore_ascii_case("users.noreply.github.com") {
        return None;
    }
    // Both `login@…` and `<id>+login@…` forms are in use.
    let login = local.rsplit_once('+').map_or(local, |(_, login)| login);
    sanitize_login(login)
}

/// The `user:` entry under `github.com:` in `gh`'s `hosts.yml`.
pub fn login_from_gh_config(text: &str) -> Option<String> {
    let mut in_github = false;
    for line in text.lines() {
        let without_newline = line.trim_end();
        if !without_newline.starts_with(|c: char| c.is_whitespace()) {
            in_github = without_newline
                .trim_end_matches(':')
                .eq_ignore_ascii_case("github.com");
            continue;
        }
        if !in_github {
            continue;
        }
        if let Some(value) = without_newline.trim_start().strip_prefix("user:") {
            if let Some(login) = sanitize_login(value) {
                return Some(login);
            }
        }
    }
    None
}

/// The photo for `login`: from the cache when it is fresh, otherwise fetched.
pub fn avatar(login: &str) -> Option<Avatar> {
    let login = sanitize_login(login)?;
    let dir = cache_dir()?;
    let path = dir.join(format!("{login}.img"));

    if let Some(avatar) = read_avatar(&path) {
        if fresh(&path) {
            return Some(avatar);
        }
    }
    // A stale or missing file: try the network, then fall back to whatever is
    // already on disk (an expired photo beats no photo).
    download(&login, &path);
    read_avatar(&path).or_else(|| {
        // Cache files written before format detection existed were named .png
        // even when the bytes were a JPEG.
        read_avatar(&dir.join(format!("{login}.png")))
    })
}

/// Resolve the login for `email` and return its photo, if any step works.
pub fn load(email: &str, repo_path: &Path) -> Option<Avatar> {
    let login = resolve_login(email, repo_path)?;
    avatar(&login)
}

/// `…/vantage/avatars`, beside the workspace `state.json`.
pub fn cache_dir() -> Option<PathBuf> {
    let state_path = vantage_core::repos::state_path().ok()?;
    let dir = cache_dir_for(&state_path)?;
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// The avatar cache directory for a given workspace state file.
fn cache_dir_for(state_path: &Path) -> Option<PathBuf> {
    Some(state_path.parent()?.join("avatars"))
}

/// A GitHub login is alphanumeric with hyphens, up to 39 characters.
///
/// Validating before use keeps a hostile `user.email` from walking out of the
/// cache directory or building a different URL than intended.
fn sanitize_login(login: &str) -> Option<String> {
    let trimmed = login.trim();
    let length_ok = !trimmed.is_empty() && trimmed.len() <= 39;
    let characters_ok = trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-');
    let edges_ok = !trimmed.starts_with('-') && !trimmed.ends_with('-');
    (length_ok && characters_ok && edges_ok).then(|| trimmed.to_string())
}

fn read_avatar(path: &Path) -> Option<Avatar> {
    let bytes = std::fs::read(path).ok()?;
    let format = format_of(&bytes)?;
    Some(Avatar { format, bytes })
}

fn fresh(path: &Path) -> bool {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|age| age < MAX_AGE)
}

/// Fetch `https://github.com/<login>.png` into `dest` via `curl`.
///
/// The download lands in a temporary file first, so an interrupted fetch can
/// never leave a truncated image in the cache.
fn download(login: &str, dest: &Path) -> Option<()> {
    let temp = dest.with_extension("part");
    let url = format!("https://github.com/{login}.png?size=64");
    let status = std::process::Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--location",
            "--fail",
            "--max-time",
            FETCH_TIMEOUT_SECONDS,
            "--output",
        ])
        .arg(&temp)
        .arg(&url)
        .env("GIT_TERMINAL_PROMPT", "0")
        .status()
        .ok()?;

    if !status.success() {
        std::fs::remove_file(&temp).ok();
        return None;
    }
    if read_avatar(&temp).is_none() {
        std::fs::remove_file(&temp).ok();
        return None;
    }
    std::fs::rename(&temp, dest).ok()?;
    Some(())
}

fn github_user_config(repo_path: &Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["config", "--get", "github.user"])
        .env("LC_ALL", "C")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    sanitize_login(&String::from_utf8_lossy(&output.stdout))
}

/// Try every place `gh` may keep `hosts.yml`: an explicit `GH_CONFIG_DIR`, the
/// XDG location used on Linux and by some macOS setups, and the platform config
/// dir (`~/Library/Application Support` on macOS).
fn gh_cli_login() -> Option<String> {
    for path in gh_config_paths() {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if let Some(login) = login_from_gh_config(&text) {
            return Some(login);
        }
    }
    None
}

fn gh_config_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(dir) = std::env::var("GH_CONFIG_DIR") {
        paths.push(Path::new(&dir).join("hosts.yml"));
    }
    if let Some(home) = dirs::home_dir() {
        paths.push(home.join(".config").join("gh").join("hosts.yml"));
    }
    if let Some(config) = dirs::config_dir() {
        paths.push(config.join("gh").join("hosts.yml"));
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avatar_cache_lives_beside_the_workspace_state_file() {
        assert_eq!(
            cache_dir_for(Path::new("/tmp/vantage/state.json")),
            Some(PathBuf::from("/tmp/vantage/avatars"))
        );
    }

    #[test]
    fn noreply_email_names_the_login() {
        assert_eq!(
            login_from_noreply_email("12345+octocat@users.noreply.github.com").as_deref(),
            Some("octocat")
        );
        assert_eq!(
            login_from_noreply_email("octocat@users.noreply.github.com").as_deref(),
            Some("octocat")
        );
        assert_eq!(
            login_from_noreply_email("OCTOCAT@Users.NoReply.GitHub.com").as_deref(),
            Some("OCTOCAT")
        );
        assert_eq!(login_from_noreply_email("octocat@example.com"), None);
        assert_eq!(login_from_noreply_email("not-an-email"), None);
    }

    #[test]
    fn gh_config_finds_the_logged_in_account() {
        let hosts =
            "github.com:\n    user: octocat\n    oauth_token: xxx\n    git_protocol: https\n";
        assert_eq!(login_from_gh_config(hosts).as_deref(), Some("octocat"));

        // The `user:` line of another host must not be picked up.
        let other = "ghe.example.com:\n    user: wrong\n    oauth_token: yyy\n";
        assert_eq!(login_from_gh_config(other), None);

        // Enterprise hosts before github.com do not shadow it.
        let mixed = "ghe.example.com:\n    user: wrong\ngithub.com:\n    user: octocat\n";
        assert_eq!(login_from_gh_config(mixed).as_deref(), Some("octocat"));
        assert_eq!(login_from_gh_config(""), None);
    }

    #[test]
    fn format_comes_from_the_bytes_because_github_serves_both_png_and_jpeg() {
        // A real PNG header.
        assert_eq!(
            format_of(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00]),
            Some(AvatarFormat::Png)
        );
        // A real JPEG (JFIF) header — this is what github.com/<login>.png
        // returns for most accounts.
        assert_eq!(
            format_of(&[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F']),
            Some(AvatarFormat::Jpeg)
        );
        assert_eq!(format_of(b"GIF89a\x01\x00"), Some(AvatarFormat::Gif));
        assert_eq!(
            format_of(b"RIFF\x24\x00\x00\x00WEBPVP8 "),
            Some(AvatarFormat::Webp)
        );
        // An HTML error page or a truncated download is not an avatar.
        assert_eq!(format_of(b"<html>404</html>"), None);
        assert_eq!(format_of(b""), None);
    }

    #[test]
    fn logins_that_could_escape_the_cache_are_rejected() {
        assert_eq!(sanitize_login("../../etc/passwd"), None);
        assert_eq!(sanitize_login("octo cat"), None);
        assert_eq!(sanitize_login("-octocat"), None);
        assert_eq!(sanitize_login(&"a".repeat(40)), None);
        assert!(sanitize_login("octo-cat").is_some());
    }
}
