use crate::error::AppError;
use crate::git::runner::GitRunner;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitIdentity {
    pub global_name: Option<String>,
    pub global_email: Option<String>,
    pub local_name: Option<String>,
    pub local_email: Option<String>,
    pub effective_name: Option<String>,
    pub effective_email: Option<String>,
    /// "repo", "global", "system", or "unset".
    pub name_source: String,
    pub email_source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityScope {
    Global,
    Repo,
}

impl IdentityScope {
    pub fn parse(value: &str) -> Result<Self, AppError> {
        match value {
            "global" => Ok(Self::Global),
            "repo" => Ok(Self::Repo),
            other => Err(AppError::new(
                "Validation",
                format!("Unknown identity scope '{}'.", other),
            )),
        }
    }
}

/// Where the effective value came from, given each config layer.
pub fn config_source(
    local: Option<&str>,
    global: Option<&str>,
    effective: Option<&str>,
) -> &'static str {
    if local.map(str::trim).is_some_and(|v| !v.is_empty()) {
        "repo"
    } else if global.map(str::trim).is_some_and(|v| !v.is_empty()) {
        "global"
    } else if effective.map(str::trim).is_some_and(|v| !v.is_empty()) {
        "system"
    } else {
        "unset"
    }
}

pub fn validate_name(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("Name cannot be empty.".to_string());
    }
    if trimmed.chars().count() > 200 {
        return Err("Name is too long.".to_string());
    }
    if trimmed.chars().any(|c| c.is_control()) {
        return Err("Name cannot contain control characters.".to_string());
    }
    Ok(trimmed.to_string())
}

pub fn validate_email(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("Email cannot be empty.".to_string());
    }
    if trimmed.len() > 320 || trimmed.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("Email cannot contain spaces or control characters.".to_string());
    }
    let mut parts = trimmed.split('@');
    let local = parts.next().unwrap_or("");
    let domain = parts.next().unwrap_or("");
    if local.is_empty() || domain.is_empty() || parts.next().is_some() {
        return Err("Enter a valid email address (name@example.com).".to_string());
    }
    if local.starts_with('.')
        || local.ends_with('.')
        || domain.starts_with('.')
        || domain.ends_with('.')
    {
        return Err("Enter a valid email address (name@example.com).".to_string());
    }
    if !domain.contains('.') || domain.split('.').any(|part| part.is_empty()) {
        return Err("Enter a valid email address (name@example.com).".to_string());
    }
    Ok(trimmed.to_string())
}

/// A missing effective identity must produce an actionable message instead of
/// whatever raw text `git commit` happens to print.
pub fn missing_identity_error(name: Option<&str>, email: Option<&str>) -> Option<AppError> {
    let missing_name = name.map(|v| v.trim().is_empty()).unwrap_or(true);
    let missing_email = email.map(|v| v.trim().is_empty()).unwrap_or(true);
    if !missing_name && !missing_email {
        return None;
    }
    let what = match (missing_name, missing_email) {
        (true, true) => "user.name and user.email are",
        (true, false) => "user.name is",
        _ => "user.email is",
    };
    Some(AppError::new(
        "IdentityMissing",
        format!(
            "Git author identity is not configured: {}. Set them in Settings → Git before committing.",
            what
        ),
    ))
}

async fn read_config(
    runner: &GitRunner,
    repo_path: Option<&Path>,
    scope: Option<&str>,
    key: &str,
) -> Option<String> {
    let mut args: Vec<&str> = vec!["config"];
    if let Some(scope) = scope {
        args.push(scope);
    }
    args.push("--get");
    args.push(key);
    let out = match repo_path {
        Some(path) => runner.run_read(path, &args).await.ok()?,
        None => runner.run(None, &args, 10, false).await.ok()?,
    };
    if out.success() {
        let value = out.stdout_string().trim().to_string();
        if !value.is_empty() {
            return Some(value);
        }
    }
    None
}

pub async fn read_identity(
    runner: &GitRunner,
    repo_path: Option<&Path>,
) -> Result<GitIdentity, AppError> {
    let global_name = read_config(runner, None, Some("--global"), "user.name").await;
    let global_email = read_config(runner, None, Some("--global"), "user.email").await;

    let (local_name, local_email, effective_name, effective_email) = match repo_path {
        Some(path) => (
            read_config(runner, Some(path), Some("--local"), "user.name").await,
            read_config(runner, Some(path), Some("--local"), "user.email").await,
            read_config(runner, Some(path), None, "user.name").await,
            read_config(runner, Some(path), None, "user.email").await,
        ),
        None => (None, None, global_name.clone(), global_email.clone()),
    };

    let name_source = config_source(
        local_name.as_deref(),
        global_name.as_deref(),
        effective_name.as_deref(),
    );
    let email_source = config_source(
        local_email.as_deref(),
        global_email.as_deref(),
        effective_email.as_deref(),
    );

    Ok(GitIdentity {
        global_name,
        global_email,
        local_name,
        local_email,
        effective_name,
        effective_email,
        name_source: name_source.to_string(),
        email_source: email_source.to_string(),
    })
}

async fn set_config(
    runner: &GitRunner,
    repo_path: Option<&Path>,
    scope: &str,
    key: &str,
    value: Option<&str>,
) -> Result<(), AppError> {
    let mut args: Vec<&str> = vec!["config", scope];
    match value {
        Some(value) => {
            args.push(key);
            args.push(value);
        }
        None => {
            args.push("--unset-all");
            args.push(key);
        }
    }

    let out = match repo_path {
        Some(path) => runner.run_write(path, &args, 20).await?,
        None => runner.run(None, &args, 20, true).await?,
    };

    // Unsetting a key that is not present exits 5; that is already the desired
    // state, so it is not an error.
    if !out.success() && value.is_some() {
        return Err(AppError::other(out.stderr));
    }
    Ok(())
}

pub async fn write_identity(
    runner: &GitRunner,
    repo_path: Option<&Path>,
    scope: IdentityScope,
    name: Option<&str>,
    email: Option<&str>,
) -> Result<(), AppError> {
    if scope == IdentityScope::Repo && repo_path.is_none() {
        return Err(AppError::new(
            "Validation",
            "A repository is required to set repository-specific identity.",
        ));
    }

    let name = name
        .map(validate_name)
        .transpose()
        .map_err(|e| AppError::new("Validation", e))?;
    let email = email
        .map(validate_email)
        .transpose()
        .map_err(|e| AppError::new("Validation", e))?;

    let config_scope = match scope {
        IdentityScope::Global => "--global",
        IdentityScope::Repo => "--local",
    };

    set_config(
        runner,
        repo_path,
        config_scope,
        "user.name",
        name.as_deref(),
    )
    .await?;
    set_config(
        runner,
        repo_path,
        config_scope,
        "user.email",
        email.as_deref(),
    )
    .await?;
    Ok(())
}

pub async fn ensure_commit_identity(runner: &GitRunner, repo_path: &Path) -> Result<(), AppError> {
    let name = read_config(runner, Some(repo_path), None, "user.name").await;
    let email = read_config(runner, Some(repo_path), None, "user.email").await;
    if let Some(err) = missing_identity_error(name.as_deref(), email.as_deref()) {
        return Err(err);
    }
    Ok(())
}

/// A one-off author for a single commit.
///
/// The dialog lets the user pick the repository identity, the global identity,
/// or type a custom name/email. "Repository" needs no override because Git
/// already resolves that itself; the other two are applied with `-c` for this
/// invocation only, so no config is ever rewritten.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitAuthor {
    pub name: String,
    pub email: String,
}

impl CommitAuthor {
    pub fn validated(&self) -> Result<Self, AppError> {
        Ok(Self {
            name: validate_name(&self.name).map_err(|e| AppError::new("Validation", e))?,
            email: validate_email(&self.email).map_err(|e| AppError::new("Validation", e))?,
        })
    }

    /// `git -c user.name=… -c user.email=…` for this command only.
    pub fn config_args(&self) -> Vec<String> {
        vec![
            "-c".to_string(),
            format!("user.name={}", self.name),
            "-c".to_string(),
            format!("user.email={}", self.email),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_names() {
        assert_eq!(validate_name("  Ada Lovelace  ").unwrap(), "Ada Lovelace");
        assert!(validate_name("").is_err());
        assert!(validate_name("   ").is_err());
        assert!(validate_name("a\nb").is_err());
    }

    #[test]
    fn validates_emails() {
        for ok in ["ada@example.com", "a.b+tag@sub.example.co.uk"] {
            assert!(validate_email(ok).is_ok(), "{ok} should be valid");
        }
        for bad in [
            "",
            "  ",
            "ada",
            "ada@",
            "@example.com",
            "ada@example",
            "a b@example.com",
            "a@@example.com",
            "ada@.com",
            "ada@example..com",
        ] {
            assert!(validate_email(bad).is_err(), "{bad} should be invalid");
        }
    }

    #[test]
    fn resolves_config_source() {
        assert_eq!(
            config_source(Some("Repo"), Some("Global"), Some("Repo")),
            "repo"
        );
        assert_eq!(
            config_source(None, Some("Global"), Some("Global")),
            "global"
        );
        assert_eq!(config_source(None, None, Some("System")), "system");
        assert_eq!(config_source(None, None, None), "unset");
        assert_eq!(config_source(Some("  "), None, None), "unset");
    }

    #[test]
    fn missing_identity_message_lists_missing_keys() {
        let both = missing_identity_error(None, None).unwrap();
        assert_eq!(both.code, "IdentityMissing");
        assert!(both.message.contains("user.name and user.email"));
        assert!(missing_identity_error(Some("Ada"), None)
            .unwrap()
            .message
            .contains("user.email"));
        assert!(missing_identity_error(None, Some("ada@example.com"))
            .unwrap()
            .message
            .contains("user.name"));
        assert!(missing_identity_error(Some("Ada"), Some("ada@example.com")).is_none());
    }

    #[tokio::test]
    async fn repo_local_identity_round_trips_and_can_be_cleared() {
        let runner = GitRunner::new();
        let path =
            std::env::temp_dir().join(format!("vantage_identity_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).unwrap();
        runner.run_write(&path, &["init"], 10).await.unwrap();

        write_identity(
            &runner,
            Some(&path),
            IdentityScope::Repo,
            Some("Temp Author"),
            Some("temp@example.com"),
        )
        .await
        .unwrap();

        let read = read_identity(&runner, Some(&path)).await.unwrap();
        assert_eq!(read.local_name.as_deref(), Some("Temp Author"));
        assert_eq!(read.local_email.as_deref(), Some("temp@example.com"));
        assert_eq!(read.effective_name.as_deref(), Some("Temp Author"));
        assert_eq!(read.name_source, "repo");

        write_identity(&runner, Some(&path), IdentityScope::Repo, None, None)
            .await
            .unwrap();
        let cleared = read_identity(&runner, Some(&path)).await.unwrap();
        assert_eq!(cleared.local_name, None);
        assert_eq!(cleared.local_email, None);

        let _ = std::fs::remove_dir_all(&path);
    }

    #[tokio::test]
    async fn rejects_invalid_identity_before_writing() {
        let runner = GitRunner::new();
        let path =
            std::env::temp_dir().join(format!("vantage_identity_invalid_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).unwrap();
        runner.run_write(&path, &["init"], 10).await.unwrap();

        let err = write_identity(
            &runner,
            Some(&path),
            IdentityScope::Repo,
            Some("Ada"),
            Some("not-an-email"),
        )
        .await
        .unwrap_err();
        assert_eq!(err.code, "Validation");

        let _ = std::fs::remove_dir_all(&path);
    }
}
