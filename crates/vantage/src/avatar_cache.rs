//! GitHub profile photos for the commit identity.
//!
//! Vantage has no HTTP client dependency. Like `git` and `open`, it drives a
//! binary that is already on the machine: `curl`. Downloads are cached next to
//! `state.json`, so a photo is fetched once and then read from disk — the commit
//! bar never waits on the network twice for the same person.
//!
//! The account is resolved from evidence, never guessed. History rows only get
//! a photo when the commit itself proves a login: a `@users.noreply.github.com`
//! address names the account, and an explicit `github.user` is a deliberate
//! repository setting. The commit bar may additionally fall back to the logged-in
//! `gh` CLI, because that is the account this machine commits as — but that
//! fallback must never be applied to somebody else's commit.
//!
//! Downloads are capped at [`MAX_CONCURRENT_DOWNLOADS`], and history rows are
//! served from the cache only, so selecting a repository cannot start one
//! network request per author.

use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::{Duration, SystemTime};

/// Re-download a cached photo after this long, so a changed avatar appears
/// without asking GitHub on every launch.
const MAX_AGE: Duration = Duration::from_secs(60 * 60 * 24 * 7);
/// Give up on a slow network rather than leaving a download thread stuck.
const FETCH_TIMEOUT_SECONDS: &str = "8";
/// Never run more than this many `curl` processes at once.
const MAX_CONCURRENT_DOWNLOADS: usize = 2;

/// The avatars directory is resolved once: finding it constructs the state
/// persistence handle (and its writer thread), and a history pass asks for it
/// once per author.
static CACHE_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Slots for in-flight downloads, shared by every caller in the process.
static DOWNLOAD_GATE: OnceLock<DownloadGate> = OnceLock::new();

struct DownloadGate {
    active: Mutex<usize>,
    ready: Condvar,
}

/// A held download slot. Dropping it wakes the next waiter.
struct DownloadSlot(&'static DownloadGate);

impl DownloadSlot {
    fn acquire() -> Self {
        let gate = DOWNLOAD_GATE.get_or_init(|| DownloadGate {
            active: Mutex::new(0),
            ready: Condvar::new(),
        });
        let mut active = gate
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while *active >= MAX_CONCURRENT_DOWNLOADS {
            active = gate
                .ready
                .wait(active)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        *active += 1;
        Self(gate)
    }
}

impl Drop for DownloadSlot {
    fn drop(&mut self) {
        let mut active = self
            .0
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *active = active.saturating_sub(1);
        self.0.ready.notify_one();
    }
}

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

/// The login an email proves without consulting the machine's own account.
///
/// A `users.noreply.github.com` address embeds its login, and `github.user` is
/// an explicit repository setting. Nothing else proves that an arbitrary commit
/// author is a particular GitHub account.
pub fn proven_login(email: &str, explicit_github_user: Option<&str>) -> Option<String> {
    login_from_noreply_email(email).or_else(|| explicit_github_user.and_then(sanitize_login))
}

/// The login for the identity this machine commits as.
///
/// The commit bar describes the local user, so a logged-in `gh` account is
/// valid evidence here. History deliberately omits it: the same fallback on a
/// commit row would put the owner's face beside somebody else's commit.
pub fn resolve_current_login(email: &str, repo_path: &Path) -> Option<String> {
    current_login_from(
        email,
        github_user_config(repo_path).as_deref(),
        gh_cli_login().as_deref(),
    )
}

fn current_login_from(
    email: &str,
    explicit_github_user: Option<&str>,
    machine_login: Option<&str>,
) -> Option<String> {
    proven_login(email, explicit_github_user).or_else(|| machine_login.and_then(sanitize_login))
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
    read_cached(&dir, &login)
}

/// The photo already on disk for a history identity.
///
/// History never downloads: a cache miss just keeps the initials tile, so
/// selecting a repository cannot fan out into one network request per author.
pub fn cached(email: &str, explicit_github_user: Option<&str>) -> Option<Avatar> {
    let login = proven_login(email, explicit_github_user)?;
    read_cached(&cache_dir()?, &login)
}

/// The explicit `github.user` configured for a repository, if any. Read once
/// per history pass rather than once per author.
pub fn explicit_github_user(repo_path: &Path) -> Option<String> {
    github_user_config(repo_path)
}

/// Resolve the current commit identity and return its photo, fetching it if
/// the cache is missing or stale.
pub fn load_current(email: &str, repo_path: &Path) -> Option<Avatar> {
    let login = resolve_current_login(email, repo_path)?;
    avatar(&login)
}

/// `…/vantage/avatars`, beside the workspace `state.json`.
pub fn cache_dir() -> Option<PathBuf> {
    CACHE_DIR
        .get_or_init(|| {
            let state_path = vantage_core::repos::state_path().ok()?;
            let dir = cache_dir_for(&state_path)?;
            std::fs::create_dir_all(&dir).ok()?;
            Some(dir)
        })
        .clone()
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

fn read_cached(dir: &Path, login: &str) -> Option<Avatar> {
    read_avatar(&dir.join(format!("{login}.img"))).or_else(|| {
        // Cache files written before format detection existed were named .png
        // even when the bytes were a JPEG.
        read_avatar(&dir.join(format!("{login}.png")))
    })
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
    let _slot = DownloadSlot::acquire();
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
        assert_eq!(login_from_noreply_email("@users.noreply.github.com"), None);
        assert_eq!(
            login_from_noreply_email("12345+@users.noreply.github.com"),
            None
        );
        assert_eq!(
            login_from_noreply_email("octocat@users.noreply.github.com.evil"),
            None
        );
    }

    #[test]
    fn a_proven_login_comes_only_from_the_email_or_explicit_config() {
        // A noreply address is the strongest evidence and wins over config.
        assert_eq!(
            proven_login(
                "12345+octocat@users.noreply.github.com",
                Some("someone-else")
            )
            .as_deref(),
            Some("octocat")
        );
        // `github.user` is explicit consent, so it may name the account.
        assert_eq!(
            proven_login("octocat@example.com", Some("octocat")).as_deref(),
            Some("octocat")
        );
        // Without either, an arbitrary email proves nothing.
        assert_eq!(proven_login("octocat@example.com", None), None);
        // A hostile config value cannot escape the cache directory.
        assert_eq!(
            proven_login("octocat@example.com", Some("../../etc/passwd")),
            None
        );
    }

    #[test]
    fn history_identity_never_falls_back_to_the_machines_gh_account() {
        // The commit bar describes the local user, so it may use `gh`.
        assert_eq!(
            current_login_from("author@example.com", None, Some("octocat")).as_deref(),
            Some("octocat")
        );
        // A history row with the same evidence must not: the `gh` account is
        // whoever owns this machine, not necessarily the commit's author.
        assert_eq!(proven_login("author@example.com", None), None);
        assert!(cached("author@example.com", None).is_none());
    }

    #[test]
    fn explicit_github_user_config_proves_a_login() {
        let dir = temp_dir("explicit_user");
        let Ok(init) = std::process::Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(["init", "-q"])
            .status()
        else {
            eprintln!("skipping: git is not installed");
            std::fs::remove_dir_all(&dir).ok();
            return;
        };
        if !init.success() {
            eprintln!("skipping: git init failed");
            std::fs::remove_dir_all(&dir).ok();
            return;
        }
        let configured = std::process::Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(["config", "github.user", "octocat"])
            .status()
            .expect("run git config");
        assert!(configured.success());

        assert_eq!(explicit_github_user(&dir).as_deref(), Some("octocat"));
        assert_eq!(
            proven_login("someone@example.com", explicit_github_user(&dir).as_deref()).as_deref(),
            Some("octocat"),
            "an explicit github.user is deliberate proof, not a guess"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn download_slots_cap_concurrent_fetches() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        static ACTIVE: AtomicUsize = AtomicUsize::new(0);
        static PEAK: AtomicUsize = AtomicUsize::new(0);

        let handles: Vec<_> = (0..6)
            .map(|_| {
                std::thread::spawn(|| {
                    let _slot = DownloadSlot::acquire();
                    let active = ACTIVE.fetch_add(1, Ordering::SeqCst) + 1;
                    PEAK.fetch_max(active, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(10));
                    ACTIVE.fetch_sub(1, Ordering::SeqCst);
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        assert!(
            PEAK.load(Ordering::SeqCst) <= MAX_CONCURRENT_DOWNLOADS,
            "no more than {MAX_CONCURRENT_DOWNLOADS} downloads may run at once"
        );
    }

    fn temp_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or_default();
        let dir = std::env::temp_dir().join(format!(
            "vantage_avatar_{name}_{}_{unique}",
            std::process::id()
        ));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
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
