use crate::error::AppError;
use crate::git::runner::GitRunner;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

/// The trailer key git uses for co-authors.
pub const TRAILER_KEY: &str = "Co-authored-by:";

/// A contributor surfaced by the co-author picker (recent authors from `git log`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Contributor {
    pub name: String,
    pub email: String,
}

/// A plausible email address for a `Co-authored-by:` trailer.
///
/// Deliberately strict: no whitespace, no angle brackets, no control characters,
/// and exactly one `@` separating two non-empty halves. This is what stops a
/// crafted entry from smuggling extra headers into the commit message.
fn is_plausible_email(email: &str) -> bool {
    if email.is_empty()
        || email
            .chars()
            .any(|c| c.is_whitespace() || c == '<' || c == '>' || c.is_control())
    {
        return false;
    }
    match email.rsplit_once('@') {
        Some((local, domain)) => {
            !local.is_empty() && !domain.is_empty() && !local.contains('@') && !domain.contains('@')
        }
        None => false,
    }
}

/// Validate and normalize a single `Name <email>` co-author entry.
///
/// Returns the canonical `Name <email>` string, or a clear error. Newlines and
/// control characters are refused outright so the value can never escape the
/// trailer block or be interpreted as an extra flag/header.
pub fn validate_co_author(raw: &str) -> Result<String, AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::other("Co-author cannot be empty"));
    }
    if trimmed
        .chars()
        .any(|c| c == '\n' || c == '\r' || c.is_control())
    {
        return Err(AppError::other(format!(
            "Invalid co-author \"{}\": newlines and control characters are not allowed",
            trimmed.escape_debug()
        )));
    }

    // Require the entry to end with `>` and contain a matching `<`.
    let close = trimmed.rfind('>');
    let open = trimmed.rfind('<');
    let (open, close) = match (open, close) {
        (Some(o), Some(c)) if c > o && c == trimmed.len() - 1 => (o, c),
        _ => {
            return Err(AppError::other(format!(
                "Invalid co-author \"{}\": expected the form \"Name <email>\"",
                trimmed
            )))
        }
    };

    let name = trimmed[..open].trim();
    let email = trimmed[open + 1..close].trim();

    if name.is_empty() {
        return Err(AppError::other(format!(
            "Invalid co-author \"{}\": expected the form \"Name <email>\"",
            trimmed
        )));
    }
    if name.contains('<') || name.contains('>') || name.contains('@') {
        return Err(AppError::other(format!(
            "Invalid co-author \"{}\": the name must not contain '<', '>' or '@'",
            trimmed
        )));
    }
    if !is_plausible_email(email) {
        return Err(AppError::other(format!(
            "Invalid co-author \"{}\": \"{}\" is not a valid email address",
            trimmed, email
        )));
    }

    Ok(format!("{} <{}>", name, email))
}

/// Extract the lowercased email from a normalized `Name <email>` entry.
fn email_of_entry(entry: &str) -> Option<String> {
    let open = entry.rfind('<')?;
    let close = entry.rfind('>')?;
    if close <= open {
        return None;
    }
    Some(entry[open + 1..close].trim().to_ascii_lowercase())
}

/// Extract the lowercased email from an existing `Co-authored-by:` trailer line.
fn trailer_email(line: &str) -> Option<String> {
    let rest = line.trim();
    if rest.len() < TRAILER_KEY.len()
        || !rest[..TRAILER_KEY.len()].eq_ignore_ascii_case(TRAILER_KEY)
    {
        return None;
    }
    let value = rest[TRAILER_KEY.len()..].trim();
    let open = value.rfind('<')?;
    let close = value.rfind('>')?;
    if close <= open {
        return None;
    }
    Some(value[open + 1..close].trim().to_ascii_lowercase())
}

/// Normalized co-author entries that still need to be appended to `message`.
///
/// Idempotent with respect to `message`: any entry whose email already appears
/// in a `Co-authored-by:` trailer (case-insensitively) is skipped, as are
/// duplicates within `co_authors` itself. Invalid entries are rejected.
pub fn pending_co_authors(message: &str, co_authors: &[String]) -> Result<Vec<String>, AppError> {
    let mut seen: HashSet<String> = message.lines().filter_map(trailer_email).collect();
    let mut pending = Vec::new();
    for raw in co_authors {
        let normalized = validate_co_author(raw)?;
        if let Some(email) = email_of_entry(&normalized) {
            if seen.insert(email) {
                pending.push(normalized);
            }
        }
    }
    Ok(pending)
}

/// Build the trailer paragraph (one `Co-authored-by:` line per entry) to append
/// to a commit message, or `None` when there is nothing left to add.
///
/// The caller appends this as a separate paragraph (blank line before it), which
/// is exactly what `git commit -m <summary> -m <block>` produces.
pub fn co_author_trailer_block(
    message: &str,
    co_authors: &[String],
) -> Result<Option<String>, AppError> {
    let pending = pending_co_authors(message, co_authors)?;
    if pending.is_empty() {
        Ok(None)
    } else {
        Ok(Some(
            pending
                .iter()
                .map(|entry| format!("{} {}", TRAILER_KEY, entry))
                .collect::<Vec<_>>()
                .join("\n"),
        ))
    }
}

/// Fetch recent contributors from a bounded `git log` window.
///
/// Output is parsed from a machine format (`%an%x1f%ae`, one commit per line),
/// deduped case-insensitively by email, newest first, and capped at `limit`.
/// A repo with no commits yields an empty list rather than an error.
pub async fn recent_contributors(
    runner: &GitRunner,
    repo_path: &Path,
    limit: u32,
) -> Result<Vec<Contributor>, AppError> {
    let n = limit.clamp(1, 1000).to_string();
    let out = runner
        .run_read(repo_path, &["log", "-n", &n, "--format=%an%x1f%ae"])
        .await?;

    if !out.success() {
        return Ok(Vec::new());
    }

    let mut seen: HashSet<String> = HashSet::new();
    let mut contributors = Vec::new();
    for line in out.stdout_string().lines() {
        if line.trim().is_empty() {
            continue;
        }
        let mut parts = line.splitn(2, '\x1f');
        let name = parts.next().unwrap_or("").trim().to_string();
        let email = parts.next().unwrap_or("").trim().to_string();
        if name.is_empty() || !is_plausible_email(&email) {
            continue;
        }
        if seen.insert(email.to_ascii_lowercase()) {
            contributors.push(Contributor { name, email });
        }
    }
    Ok(contributors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_accepts_and_normalizes() {
        assert_eq!(
            validate_co_author("Jane Doe <jane@example.com>").unwrap(),
            "Jane Doe <jane@example.com>"
        );
        // Surrounding whitespace is trimmed.
        assert_eq!(
            validate_co_author("  Jane Doe <jane@example.com>  ").unwrap(),
            "Jane Doe <jane@example.com>"
        );
        // Single-word names are fine.
        assert_eq!(
            validate_co_author("jane <jane@example.com>").unwrap(),
            "jane <jane@example.com>"
        );
    }

    #[test]
    fn validate_rejects_malformed_entries() {
        assert!(validate_co_author("").is_err());
        assert!(validate_co_author("   ").is_err());
        assert!(validate_co_author("Jane Doe").is_err());
        assert!(validate_co_author("Jane Doe <jane@example.com").is_err());
        assert!(validate_co_author("<jane@example.com>").is_err());
        assert!(validate_co_author("Jane Doe <jane example.com>").is_err());
        assert!(validate_co_author("Jane Doe <jane@example>").is_ok());
        assert!(validate_co_author("Jane Doe <jane@localhost>").is_ok());
        assert!(validate_co_author("Jane Doe <@example.com>").is_err());
        assert!(validate_co_author("Jane Doe <jane@>").is_err());
    }

    #[test]
    fn validate_rejects_injection_attempts() {
        // Newline would let the caller smuggle an extra header/trailer.
        assert!(
            validate_co_author("Jane <jane@example.com>\nSigned-off-by: Evil <e@e.com>").is_err()
        );
        // Trailing text after the closing angle bracket (looks like a flag).
        assert!(validate_co_author("Jane <jane@example.com> --author=evil").is_err());
        assert!(validate_co_author("Jane <jane@example.com> -m extra").is_err());
        // Carriage returns and other control characters.
        assert!(validate_co_author("Jane\r<jane@example.com>").is_err());
        assert!(validate_co_author("Ja\u{7}ne <jane@example.com>").is_err());
    }

    #[test]
    fn pending_appends_when_absent() {
        let co = vec![
            "Jane Doe <jane@example.com>".to_string(),
            "Sam <sam@example.com>".to_string(),
        ];
        let block = co_author_trailer_block("add feature", &co).unwrap();
        assert_eq!(
            block.unwrap(),
            "Co-authored-by: Jane Doe <jane@example.com>\nCo-authored-by: Sam <sam@example.com>"
        );
    }

    #[test]
    fn pending_is_idempotent_when_trailer_already_present() {
        let co = vec!["Jane Doe <jane@example.com>".to_string()];
        let message = "add feature\n\nCo-authored-by: Jane Doe <jane@example.com>";
        assert_eq!(co_author_trailer_block(message, &co).unwrap(), None);
        // Case-insensitive on the email.
        let message_ci = "add feature\n\nCo-Authored-By: Jane Doe <JANE@example.com>";
        assert_eq!(co_author_trailer_block(message_ci, &co).unwrap(), None);
    }

    #[test]
    fn pending_only_returns_new_entries() {
        let co = vec![
            "Jane Doe <jane@example.com>".to_string(),
            "Sam <sam@example.com>".to_string(),
        ];
        let message = "add feature\n\nCo-authored-by: Jane Doe <jane@example.com>";
        let block = co_author_trailer_block(message, &co).unwrap().unwrap();
        assert_eq!(block, "Co-authored-by: Sam <sam@example.com>");
    }

    #[test]
    fn pending_dedups_within_input() {
        let co = vec![
            "Jane Doe <jane@example.com>".to_string(),
            "Jane Doe <JANE@example.com>".to_string(),
        ];
        let block = co_author_trailer_block("add feature", &co)
            .unwrap()
            .unwrap();
        assert_eq!(block, "Co-authored-by: Jane Doe <jane@example.com>");
    }

    #[test]
    fn pending_rejects_invalid_entry() {
        let co = vec!["not a valid entry".to_string()];
        assert!(co_author_trailer_block("add feature", &co).is_err());
    }

    #[test]
    fn pending_empty_input_is_noop() {
        assert_eq!(co_author_trailer_block("add feature", &[]).unwrap(), None);
    }
}
