//! Read-only GitHub account and permission detection.
//!
//! Vantage deliberately does not store OAuth tokens or talk to the GitHub API
//! itself. When the GitHub CLI (`gh`) is installed and signed in, we can show
//! the same accounts the terminal uses, plus the viewer's permission on the
//! current repository. Everything here is best-effort: a missing `gh`, a
//! non-GitHub remote, or an offline machine must never block a commit.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubAccount {
    pub host: String,
    pub login: String,
    /// `success` when the stored token still works, otherwise the raw state.
    pub state: String,
    pub active: bool,
    pub token_source: Option<String>,
    pub scopes: Vec<String>,
    pub git_protocol: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubStatus {
    pub gh_installed: bool,
    pub gh_version: Option<String>,
    pub remote_url: Option<String>,
    pub host: Option<String>,
    pub owner: Option<String>,
    pub repo: Option<String>,
    pub is_github: bool,
    pub authenticated: bool,
    pub accounts: Vec<GitHubAccount>,
    pub active_account: Option<String>,
    /// `None` means "could not determine" — not "no access".
    pub can_push: Option<bool>,
    pub permission: Option<String>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteTarget {
    pub host: String,
    pub owner: String,
    pub repo: String,
}

/// Parse the common Git remote URL shapes into host/owner/repo.
///
/// Supported:
/// - `git@github.com:owner/repo.git`
/// - `ssh://git@github.com/owner/repo.git`
/// - `https://github.com/owner/repo.git`
/// - `github.com:owner/repo`
pub fn parse_remote(url: &str) -> Option<RemoteTarget> {
    let url = url.trim();
    if url.is_empty() {
        return None;
    }

    let (host, path) = if let Some(rest) = url.strip_prefix("git@") {
        let (host, path) = rest.split_once(':')?;
        (host.to_string(), path.to_string())
    } else if let Some((_, rest)) = url.split_once("://") {
        let (authority, path) = rest.split_once('/')?;
        let host = authority.rsplit('@').next().unwrap_or(authority);
        let host = host.split(':').next().unwrap_or(host);
        (host.to_string(), path.to_string())
    } else if let Some((host, path)) = url.split_once(':') {
        (host.to_string(), path.to_string())
    } else {
        return None;
    };

    let host = host.trim().trim_end_matches('.').to_lowercase();
    if host.is_empty() {
        return None;
    }

    let path = path.trim().trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    if parts.len() < 2 {
        return None;
    }

    Some(RemoteTarget {
        host,
        owner: parts[parts.len() - 2].to_string(),
        repo: parts[parts.len() - 1].to_string(),
    })
}

struct GhOutput {
    success: bool,
    stdout: String,
    stderr: String,
}

/// Finder-launched apps inherit a minimal PATH, so `which("gh")` alone misses
/// Homebrew, MacPorts, and per-user installs. Probe the usual locations too.
fn gh_program() -> Option<PathBuf> {
    if let Ok(path) = which::which("gh") {
        return Some(path);
    }

    let mut candidates: Vec<PathBuf> = vec![
        PathBuf::from("/opt/homebrew/bin/gh"),
        PathBuf::from("/usr/local/bin/gh"),
        PathBuf::from("/opt/local/bin/gh"),
        PathBuf::from("/usr/bin/gh"),
        PathBuf::from("/snap/bin/gh"),
        PathBuf::from("/nix/var/nix/profiles/default/bin/gh"),
    ];
    if let Ok(home) = std::env::var("HOME") {
        let home = PathBuf::from(home);
        candidates.push(home.join(".local/bin/gh"));
        candidates.push(home.join("bin/gh"));
        candidates.push(home.join(".nix-profile/bin/gh"));
    }

    candidates.into_iter().find(|path| path.is_file())
}

async fn run_gh(args: &[&str], timeout_secs: u64) -> Option<GhOutput> {
    let program = gh_program()?;
    let mut cmd = Command::new(program);
    cmd.kill_on_drop(true);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.env("GH_PROMPT_DISABLED", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("NO_COLOR", "1");
    cmd.args(args);

    let child = cmd.spawn().ok()?;
    let wait = child.wait_with_output();
    match tokio::time::timeout(Duration::from_secs(timeout_secs), wait).await {
        Ok(Ok(out)) => Some(GhOutput {
            success: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).to_string(),
            stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        }),
        _ => None,
    }
}

fn parse_accounts(raw: &str) -> Vec<GitHubAccount> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return Vec::new();
    };
    let Some(hosts) = value.get("hosts").and_then(|h| h.as_object()) else {
        return Vec::new();
    };

    let mut accounts = Vec::new();
    for (host, entries) in hosts {
        // Newer gh versions return an array per host; older ones returned a
        // single object. Accept both instead of guessing from the version.
        let list: Vec<&serde_json::Value> = match entries {
            serde_json::Value::Array(items) => items.iter().collect(),
            serde_json::Value::Object(_) => vec![entries],
            _ => continue,
        };

        for entry in list {
            let Some(login) = entry.get("login").and_then(|v| v.as_str()) else {
                continue;
            };
            let scopes = entry
                .get("scopes")
                .and_then(|v| v.as_str())
                .map(|s| {
                    s.split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();

            accounts.push(GitHubAccount {
                host: entry
                    .get("host")
                    .and_then(|v| v.as_str())
                    .unwrap_or(host)
                    .to_string(),
                login: login.to_string(),
                state: entry
                    .get("state")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                active: entry
                    .get("active")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                token_source: entry
                    .get("tokenSource")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                scopes,
                git_protocol: entry
                    .get("gitProtocol")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
            });
        }
    }

    // Stable, useful ordering: the active account first, then by host/login.
    accounts.sort_by(|a, b| {
        b.active
            .cmp(&a.active)
            .then_with(|| a.host.cmp(&b.host))
            .then_with(|| a.login.cmp(&b.login))
    });
    accounts
}

fn trim_detail(text: &str) -> Option<String> {
    let line = text.lines().map(str::trim).find(|line| !line.is_empty())?;
    let mut detail = line.to_string();
    if detail.chars().count() > 240 {
        detail = detail.chars().take(240).collect::<String>() + "…";
    }
    Some(detail)
}

async fn check_permission(
    host: &str,
    owner: &str,
    repo: &str,
) -> (Option<bool>, Option<String>, Option<String>) {
    let path = format!("repos/{}/{}", owner, repo);
    let mut args: Vec<&str> = vec!["api"];
    if host != "github.com" {
        args.push("--hostname");
        args.push(host);
    }
    args.push(&path);

    let Some(out) = run_gh(&args, 15).await else {
        return (
            None,
            None,
            Some("Could not reach the GitHub API (gh timed out).".to_string()),
        );
    };

    if !out.success {
        let detail = trim_detail(&out.stderr);
        // A 404 on a repository endpoint means the signed-in account cannot see
        // it, which also means it cannot push. Anything else (offline, DNS,
        // rate limit) leaves the answer unknown rather than wrongly negative.
        if out.stderr.contains("HTTP 404") || out.stderr.contains("Not Found") {
            return (
                Some(false),
                None,
                detail.or_else(|| Some("Repository not found for this account.".to_string())),
            );
        }
        return (None, None, detail);
    }

    let Ok(value) = serde_json::from_str::<serde_json::Value>(&out.stdout) else {
        return (
            None,
            None,
            Some("Could not read the GitHub API response.".to_string()),
        );
    };

    let can_push = value
        .get("permissions")
        .and_then(|p| p.get("push"))
        .and_then(|v| v.as_bool());

    let permission = value
        .get("viewerPermission")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .or_else(|| {
            value
                .get("permissions")
                .and_then(|p| p.as_object())
                .map(|p| {
                    let enabled: Vec<&str> = p
                        .iter()
                        .filter(|(_, v)| v.as_bool().unwrap_or(false))
                        .map(|(k, _)| k.as_str())
                        .collect();
                    if enabled.contains(&"admin") {
                        "ADMIN".to_string()
                    } else if enabled.contains(&"maintain") {
                        "MAINTAIN".to_string()
                    } else if enabled.contains(&"push") {
                        "WRITE".to_string()
                    } else if enabled.contains(&"triage") {
                        "TRIAGE".to_string()
                    } else {
                        "READ".to_string()
                    }
                })
        });

    (can_push, permission, None)
}

/// Inspect the current repository's remote and the GitHub CLI's accounts.
pub async fn inspect(remote_url: Option<String>) -> GitHubStatus {
    let mut status = GitHubStatus {
        gh_installed: false,
        gh_version: None,
        remote_url: remote_url.clone(),
        host: None,
        owner: None,
        repo: None,
        is_github: false,
        authenticated: false,
        accounts: Vec::new(),
        active_account: None,
        can_push: None,
        permission: None,
        detail: None,
    };

    let target = remote_url.as_deref().and_then(parse_remote);
    if let Some(target) = &target {
        status.host = Some(target.host.clone());
        status.owner = Some(target.owner.clone());
        status.repo = Some(target.repo.clone());
    }

    if gh_program().is_none() {
        status.detail = Some(
            "GitHub CLI (gh) is not installed. Commits still use your system Git identity and credentials."
                .to_string(),
        );
        return status;
    }
    status.gh_installed = true;

    if let Some(out) = run_gh(&["--version"], 5).await {
        status.gh_version = out
            .stdout
            .lines()
            .next()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string);
    }

    let Some(auth) = run_gh(&["auth", "status", "--json", "hosts"], 10).await else {
        status.detail = Some("Could not run `gh auth status`.".to_string());
        return status;
    };

    // `gh auth status` exits non-zero when at least one account is unhealthy,
    // but still prints the JSON we need. Parse stdout either way.
    status.accounts = parse_accounts(&auth.stdout);
    status.authenticated = status
        .accounts
        .iter()
        .any(|a| a.state == "success" && a.active);

    let target_host = target.as_ref().map(|t| t.host.as_str());
    let active_for_target = target_host.and_then(|host| {
        status
            .accounts
            .iter()
            .find(|a| a.host == host && a.state == "success" && a.active)
    });
    status.active_account = active_for_target
        .or_else(|| status.accounts.iter().find(|a| a.active))
        .map(|a| a.login.clone());

    status.is_github = target_host.is_some_and(|host| {
        host == "github.com"
            || host.contains("github")
            || status.accounts.iter().any(|a| a.host == host)
    });

    if let (Some(target), true) = (&target, status.is_github) {
        let signed_in = status
            .accounts
            .iter()
            .any(|a| a.host == target.host && a.state == "success");
        if signed_in {
            let (can_push, permission, detail) =
                check_permission(&target.host, &target.owner, &target.repo).await;
            status.can_push = can_push;
            status.permission = permission;
            status.detail = detail;
        } else {
            status.detail = Some(format!("No signed-in gh account for {}.", target.host));
        }
    } else if target.is_none() {
        status.detail = Some("This repository has no origin remote.".to_string());
    } else if !status.is_github {
        status.detail = Some("The origin remote is not hosted on GitHub.".to_string());
    }

    status
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_remote_shapes() {
        let ssh = parse_remote("git@github.com:octo/hello.git").unwrap();
        assert_eq!(ssh.host, "github.com");
        assert_eq!(ssh.owner, "octo");
        assert_eq!(ssh.repo, "hello");

        let https = parse_remote("https://github.com/octo/hello.git").unwrap();
        assert_eq!(https.repo, "hello");

        let ssh_url = parse_remote("ssh://git@github.com:22/octo/hello").unwrap();
        assert_eq!(ssh_url.host, "github.com");
        assert_eq!(ssh_url.owner, "octo");

        let enterprise = parse_remote("git@ghe.example.com:team/repo.git").unwrap();
        assert_eq!(enterprise.host, "ghe.example.com");
        assert_eq!(enterprise.owner, "team");
    }

    #[test]
    fn rejects_non_remote_strings() {
        assert!(parse_remote("").is_none());
        assert!(parse_remote("not a url").is_none());
        assert!(parse_remote("https://github.com/only-owner").is_none());
    }

    #[test]
    fn parses_accounts_from_gh_json() {
        let raw = r#"{
            "hosts": {
                "github.com": [
                    {
                        "state": "success",
                        "active": true,
                        "host": "github.com",
                        "login": "octocat",
                        "tokenSource": "keyring",
                        "scopes": "repo, workflow",
                        "gitProtocol": "https"
                    }
                ]
            }
        }"#;
        let accounts = parse_accounts(raw);
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].login, "octocat");
        assert!(accounts[0].active);
        assert_eq!(accounts[0].scopes, vec!["repo", "workflow"]);
    }
}
