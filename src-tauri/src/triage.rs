use crate::state::{FileChange, FileStatus, FlagKind, Severity, TriageFlag};
use regex::Regex;

pub fn evaluate_commit_flags(
    files_changed: u32,
    additions: u32,
    deletions: u32,
    changes: &[FileChange],
    added_lines_sample: Option<&str>,
) -> Vec<TriageFlag> {
    let mut flags = Vec::new();

    // 1. Large
    if files_changed > 15 || (additions + deletions) > 500 {
        flags.push(TriageFlag {
            kind: FlagKind::Large,
            severity: Severity::Caution,
            detail: format!("{} files, {} lines", files_changed, additions + deletions),
        });
    }

    // 2. DeletionHeavy
    if deletions >= 50 && deletions > 2 * additions {
        flags.push(TriageFlag {
            kind: FlagKind::DeletionHeavy,
            severity: Severity::Caution,
            detail: format!("-{} vs +{}", deletions, additions),
        });
    }

    // 3. Dependencies
    let dep_files = [
        "package.json",
        "package-lock.json",
        "yarn.lock",
        "pnpm-lock.yaml",
        "bun.lockb",
        "Cargo.toml",
        "Cargo.lock",
        "go.mod",
        "go.sum",
        "pyproject.toml",
        "poetry.lock",
        "composer.json",
        "composer.lock",
        "pom.xml",
    ];
    let mut matched_deps = Vec::new();
    for c in changes {
        let filename = c.path.split('/').next_back().unwrap_or(&c.path);
        let is_dep = dep_files.contains(&filename)
            || filename.starts_with("requirements") && filename.ends_with(".txt")
            || filename.starts_with("Pipfile")
            || filename.starts_with("Gemfile")
            || filename.starts_with("build.gradle");

        if is_dep && !matched_deps.contains(&filename.to_string()) {
            matched_deps.push(filename.to_string());
        }
    }
    if !matched_deps.is_empty() {
        flags.push(TriageFlag {
            kind: FlagKind::Dependencies,
            severity: Severity::Info,
            detail: matched_deps.join(", "),
        });
    }

    // 4. Config
    let mut matched_configs = Vec::new();
    for c in changes {
        let p = &c.path;
        let filename = p.split('/').next_back().unwrap_or(p);
        let is_config = p.starts_with(".github/workflows/")
            || filename.starts_with("Dockerfile")
            || filename.starts_with("docker-compose")
            || filename.starts_with(".env")
            || p.contains("/migrations/")
            || p.starts_with("migrations/")
            || p.ends_with(".tf")
            || p.ends_with(".tfvars")
            || p == ".gitlab-ci.yml"
            || p.starts_with("k8s/")
            || p.starts_with("helm/")
            || (filename.starts_with("nginx") && filename.ends_with(".conf"));

        if is_config && !matched_configs.contains(&filename.to_string()) {
            matched_configs.push(filename.to_string());
        }
    }
    if !matched_configs.is_empty() {
        flags.push(TriageFlag {
            kind: FlagKind::Config,
            severity: Severity::Caution,
            detail: matched_configs.join(", "),
        });
    }

    // 5. TestsRemoved
    let mut deleted_test_count = 0;
    let mut heavy_test_deletions = false;
    for c in changes {
        let lower = c.path.to_lowercase();
        let is_test =
            lower.contains("test") || lower.contains("spec") || lower.contains("__tests__");
        if is_test {
            if c.status == FileStatus::Deleted {
                deleted_test_count += 1;
            } else if c.deletions > c.additions && c.deletions >= 10 {
                heavy_test_deletions = true;
            }
        }
    }

    let mut skip_pattern_matched = false;
    if let Some(text) = added_lines_sample {
        let re = Regex::new(r"(\b(it|test|describe)\.(skip|only)\b|\bx(it|describe)\(|@Ignore|#\[ignore\]|pytest\.mark\.skip|t\.Skip\()").unwrap();
        if re.is_match(text) {
            skip_pattern_matched = true;
        }
    }

    if deleted_test_count > 0 || heavy_test_deletions || skip_pattern_matched {
        let detail = if deleted_test_count > 0 {
            format!("{} test files deleted", deleted_test_count)
        } else if heavy_test_deletions {
            "Substantial test lines removed".to_string()
        } else {
            "Test skip/ignore patterns added".to_string()
        };
        flags.push(TriageFlag {
            kind: FlagKind::TestsRemoved,
            severity: Severity::Warn,
            detail,
        });
    }

    // 6. Secrets
    let mut secret_detected = false;
    let mut secret_detail = String::new();

    for c in changes {
        let filename = c.path.split('/').next_back().unwrap_or(&c.path);
        if filename == ".env" && c.status == FileStatus::Added {
            secret_detected = true;
            secret_detail = format!(".env file added: {}", c.path);
            break;
        }
    }

    if !secret_detected {
        if let Some(text) = added_lines_sample {
            let key_pattern = Regex::new(r"-----BEGIN [A-Z ]*PRIVATE KEY-----|AKIA[0-9A-Z]{16}|ghp_[A-Za-z0-9]{36}|sk-[A-Za-z0-9]{32,}|(api[_-]?key|secret|token|passwd|password)\s*[:=]\s*['\x22][^'\x22]{8,}['\x22]").unwrap();
            if let Some(m) = key_pattern.find(text) {
                secret_detected = true;
                secret_detail = format!(
                    "Possible secret: {}",
                    &text[m.start()..m.end().min(m.start() + 30)]
                );
            }
        }
    }

    if secret_detected {
        flags.push(TriageFlag {
            kind: FlagKind::Secrets,
            severity: Severity::Warn,
            detail: secret_detail,
        });
    }

    // 7. Binary
    let binary_count = changes.iter().filter(|c| c.binary).count();
    if binary_count > 0 {
        flags.push(TriageFlag {
            kind: FlagKind::Binary,
            severity: Severity::Info,
            detail: format!(
                "{} binary file{}",
                binary_count,
                if binary_count > 1 { "s" } else { "" }
            ),
        });
    }

    flags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_large_flag() {
        let flags = evaluate_commit_flags(20, 300, 300, &[], None);
        assert!(flags.iter().any(|f| f.kind == FlagKind::Large));
    }

    #[test]
    fn test_deletion_heavy_flag() {
        let flags = evaluate_commit_flags(2, 10, 100, &[], None);
        assert!(flags.iter().any(|f| f.kind == FlagKind::DeletionHeavy));
    }

    #[test]
    fn test_dependencies_flag() {
        let changes = vec![FileChange {
            path: "Cargo.toml".to_string(),
            old_path: None,
            status: FileStatus::Modified,
            additions: 1,
            deletions: 0,
            binary: false,
            old_mode: None,
            new_mode: None,
        }];
        let flags = evaluate_commit_flags(1, 1, 0, &changes, None);
        assert!(flags.iter().any(|f| f.kind == FlagKind::Dependencies));
    }

    #[test]
    fn test_secrets_flag() {
        let sample = "const apiKey = \"ghp_123456789012345678901234567890123456\";";
        let flags = evaluate_commit_flags(1, 1, 0, &[], Some(sample));
        assert!(flags.iter().any(|f| f.kind == FlagKind::Secrets));
    }
}
