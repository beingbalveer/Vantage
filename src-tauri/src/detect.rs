use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// A launchable application option shown in Settings. `command` is a template
/// accepted by `open_external_editor` (editors) or a shell executable path
/// (shells).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedApp {
    pub id: String,
    pub label: String,
    pub command: String,
}

pub struct EditorSpec {
    pub id: &'static str,
    pub label: &'static str,
    /// Programs to look for, in order. Absolute paths are allowed.
    pub programs: &'static [&'static str],
    /// Arguments appended after the resolved program.
    pub args: &'static str,
}

const EDITOR_SPECS: &[EditorSpec] = &[
    EditorSpec {
        id: "vscode",
        label: "VS Code",
        programs: &[
            "code",
            "/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code",
            "~/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code",
        ],
        args: "-g {full}:{line}",
    },
    EditorSpec {
        id: "cursor",
        label: "Cursor",
        programs: &[
            "cursor",
            "/Applications/Cursor.app/Contents/Resources/app/bin/cursor",
            "~/Applications/Cursor.app/Contents/Resources/app/bin/cursor",
        ],
        args: "-g {full}:{line}",
    },
    EditorSpec {
        id: "windsurf",
        label: "Windsurf",
        programs: &[
            "windsurf",
            "/Applications/Windsurf.app/Contents/Resources/app/bin/windsurf",
        ],
        args: "-g {full}:{line}",
    },
    EditorSpec {
        id: "zed",
        label: "Zed",
        programs: &["zed", "/Applications/Zed.app/Contents/MacOS/zed"],
        args: "{full}:{line}",
    },
    EditorSpec {
        id: "sublime",
        label: "Sublime Text",
        programs: &["subl", "/Applications/Sublime Text.app/Contents/SharedSupport/bin/subl"],
        args: "{full}:{line}",
    },
    EditorSpec {
        id: "idea",
        label: "IntelliJ IDEA",
        programs: &["idea", "/Applications/IntelliJ IDEA.app/Contents/MacOS/idea"],
        args: "--line {line} {full}",
    },
    EditorSpec {
        id: "webstorm",
        label: "WebStorm",
        programs: &["webstorm", "/Applications/WebStorm.app/Contents/MacOS/webstorm"],
        args: "--line {line} {full}",
    },
    EditorSpec {
        id: "pycharm",
        label: "PyCharm",
        programs: &["pycharm", "/Applications/PyCharm.app/Contents/MacOS/pycharm"],
        args: "--line {line} {full}",
    },
    EditorSpec {
        id: "goland",
        label: "GoLand",
        programs: &["goland", "/Applications/GoLand.app/Contents/MacOS/goland"],
        args: "--line {line} {full}",
    },
    EditorSpec {
        id: "rustrover",
        label: "RustRover",
        programs: &["rustrover", "/Applications/RustRover.app/Contents/MacOS/rustrover"],
        args: "--line {line} {full}",
    },
];

/// The always-available fallback: hand the file to the OS.
pub fn system_editor_command() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "open {full}"
    }
    #[cfg(target_os = "windows")]
    {
        "cmd /c start \"\" {full}"
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        "xdg-open {full}"
    }
}

fn expand_home(program: &str) -> PathBuf {
    if let Some(rest) = program.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(program)
}

/// Resolve a program name (PATH lookup) or an absolute path.
pub fn find_program(program: &str) -> Option<String> {
    let expanded = expand_home(program);
    if program.contains('/') || program.contains('\\') || expanded.is_absolute() {
        if expanded.is_file() {
            return Some(expanded.to_string_lossy().to_string());
        }
        return None;
    }
    which::which(program)
        .ok()
        .map(|p| p.to_string_lossy().to_string())
}

fn quote_program(path: &str) -> String {
    if path.contains(char::is_whitespace) && !path.starts_with('"') {
        format!("\"{}\"", path.replace('"', "\\\""))
    } else {
        path.to_string()
    }
}

/// Detect editors using the supplied resolver. Exposed for tests so detection
/// parsing can be verified without touching the host machine.
pub fn detect_editors_with<F>(resolve: F) -> Vec<DetectedApp>
where
    F: Fn(&str) -> Option<String>,
{
    let mut editors = Vec::new();
    for spec in EDITOR_SPECS {
        if let Some(path) = spec.programs.iter().find_map(|p| resolve(p)) {
            editors.push(DetectedApp {
                id: spec.id.to_string(),
                label: spec.label.to_string(),
                command: format!("{} {}", quote_program(&path), spec.args),
            });
        }
    }
    editors.push(DetectedApp {
        id: "system".to_string(),
        label: "System default app".to_string(),
        command: system_editor_command().to_string(),
    });
    editors
}

pub fn detect_editors() -> Vec<DetectedApp> {
    detect_editors_with(find_program)
}

/// Parse `/etc/shells` content: ignore comments/blank lines, keep absolute
/// paths, and de-duplicate while preserving order.
pub fn parse_shells(content: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut shells = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if !Path::new(line).is_absolute() {
            continue;
        }
        if seen.insert(line.to_string()) {
            shells.push(line.to_string());
        }
    }
    shells
}

fn shell_label(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
        .to_string()
}

fn push_shell(shells: &mut Vec<DetectedApp>, seen: &mut HashSet<String>, path: &str) {
    if path.trim().is_empty() {
        return;
    }
    let key = std::fs::canonicalize(path)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| path.to_string());
    if !seen.insert(key.clone()) {
        return;
    }
    shells.push(DetectedApp {
        id: key.clone(),
        label: shell_label(path),
        command: key,
    });
}

/// Detect real shells on this machine. `default_shell` is always present as a
/// fallback so the picker can never be empty.
pub fn detect_shells(default_shell: &str) -> Vec<DetectedApp> {
    let mut shells = Vec::new();
    let mut seen = HashSet::new();

    #[cfg(unix)]
    {
        if let Ok(content) = std::fs::read_to_string("/etc/shells") {
            for path in parse_shells(&content) {
                if Path::new(&path).exists() {
                    push_shell(&mut shells, &mut seen, &path);
                }
            }
        }

        if let Ok(env_shell) = std::env::var("SHELL") {
            if Path::new(&env_shell).exists() {
                push_shell(&mut shells, &mut seen, &env_shell);
            }
        }

        for name in ["zsh", "bash", "fish", "nu", "sh"] {
            if let Ok(path) = which::which(name) {
                push_shell(
                    &mut shells,
                    &mut seen,
                    &path.to_string_lossy().to_string(),
                );
            }
        }
    }

    #[cfg(windows)]
    {
        for name in ["pwsh.exe", "powershell.exe", "cmd.exe"] {
            if let Ok(path) = which::which(name) {
                push_shell(
                    &mut shells,
                    &mut seen,
                    &path.to_string_lossy().to_string(),
                );
            }
        }
    }

    push_shell(&mut shells, &mut seen, default_shell);
    shells
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_shells_ignores_comments_blanks_and_duplicates() {
        let parsed = parse_shells(
            "# /etc/shells\n\n/bin/sh\n/usr/bin/bash\n  /usr/bin/bash  \nrelative/path\n/bin/zsh\n",
        );
        assert_eq!(parsed, vec!["/bin/sh", "/usr/bin/bash", "/bin/zsh"]);
    }

    #[test]
    fn editor_detection_only_returns_resolvable_programs() {
        let editors = detect_editors_with(|program| match program {
            "code" => Some("code".to_string()),
            "/Applications/Cursor.app/Contents/Resources/app/bin/cursor" => {
                Some("/Applications/Cursor.app/Contents/Resources/app/bin/cursor".to_string())
            }
            _ => None,
        });

        let ids: Vec<&str> = editors.iter().map(|e| e.id.as_str()).collect();
        assert!(ids.contains(&"vscode"));
        assert!(ids.contains(&"cursor"));
        assert!(!ids.contains(&"zed"));
        assert_eq!(ids.last().copied(), Some("system"));

        let cursor = editors.iter().find(|e| e.id == "cursor").unwrap();
        assert_eq!(
            cursor.command,
            "/Applications/Cursor.app/Contents/Resources/app/bin/cursor -g {full}:{line}"
        );
    }

    #[test]
    fn editor_paths_with_spaces_are_quoted() {
        let editors = detect_editors_with(|program| match program {
            "code" => Some(
                "/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code".to_string(),
            ),
            _ => None,
        });
        let vscode = editors.iter().find(|e| e.id == "vscode").unwrap();
        assert_eq!(
            vscode.command,
            "\"/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code\" -g {full}:{line}"
        );
    }

    #[test]
    fn system_fallback_is_always_offered() {
        let editors = detect_editors_with(|_| None);
        assert_eq!(editors.len(), 1);
        assert_eq!(editors[0].id, "system");
        assert!(editors[0].command.contains("{full}"));
    }

    #[test]
    fn shell_detection_always_includes_default() {
        let shells = detect_shells("/definitely/not/a/real/shell");
        assert!(shells.iter().any(|s| s.command == "/definitely/not/a/real/shell"));
    }
}
