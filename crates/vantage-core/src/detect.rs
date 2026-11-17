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
        programs: &[
            "subl",
            "/Applications/Sublime Text.app/Contents/SharedSupport/bin/subl",
        ],
        args: "{full}:{line}",
    },
    EditorSpec {
        id: "idea",
        label: "IntelliJ IDEA",
        programs: &[
            "idea",
            "/Applications/IntelliJ IDEA.app/Contents/MacOS/idea",
        ],
        args: "--line {line} {full}",
    },
    EditorSpec {
        id: "webstorm",
        label: "WebStorm",
        programs: &[
            "webstorm",
            "/Applications/WebStorm.app/Contents/MacOS/webstorm",
        ],
        args: "--line {line} {full}",
    },
    EditorSpec {
        id: "pycharm",
        label: "PyCharm",
        programs: &[
            "pycharm",
            "/Applications/PyCharm.app/Contents/MacOS/pycharm",
        ],
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
        programs: &[
            "rustrover",
            "/Applications/RustRover.app/Contents/MacOS/rustrover",
        ],
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

// ---- external actions ---------------------------------------------------
//
// Reveal, terminal and editor launches are built as an explicit program plus
// argv. Nothing is passed through a shell, so spaces and shell metacharacters
// in paths stay literal arguments.

/// The desktop conventions Vantage knows how to launch external programs
/// with. Selecting the convention is separate from running it so every branch
/// is testable on one host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesktopPlatform {
    MacOs,
    Linux,
    Windows,
    Unsupported,
}

impl DesktopPlatform {
    pub const fn current() -> Self {
        #[cfg(target_os = "macos")]
        {
            Self::MacOs
        }
        #[cfg(target_os = "linux")]
        {
            Self::Linux
        }
        #[cfg(target_os = "windows")]
        {
            Self::Windows
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            Self::Unsupported
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::MacOs => "macOS",
            Self::Linux => "Linux",
            Self::Windows => "Windows",
            Self::Unsupported => "this platform",
        }
    }
}

/// A resolved external command. `spawn` never goes through a shell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchCommand {
    program: String,
    args: Vec<String>,
    working_dir: Option<PathBuf>,
}

impl LaunchCommand {
    fn new(program: impl Into<String>, args: impl IntoIterator<Item = String>) -> Self {
        Self {
            program: program.into(),
            args: args.into_iter().collect(),
            working_dir: None,
        }
    }

    pub fn program(&self) -> &str {
        &self.program
    }

    pub fn args(&self) -> &[String] {
        &self.args
    }

    pub fn working_dir(&self) -> Option<&Path> {
        self.working_dir.as_deref()
    }

    /// Launch the command and hand the child to the OS. The caller decides
    /// whether a failure is worth a toast.
    pub fn spawn(&self) -> std::io::Result<std::process::Child> {
        let mut command = std::process::Command::new(&self.program);
        command.args(&self.args);
        if let Some(dir) = &self.working_dir {
            command.current_dir(dir);
        }
        command.spawn()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaunchError {
    EmptyCommand,
    UnterminatedQuote {
        input: String,
    },
    ProgramNotFound {
        source: &'static str,
        program: String,
    },
    NoEditor,
    NoTerminal,
    NoFileManager,
    Unsupported {
        action: &'static str,
        platform: &'static str,
    },
}

impl std::fmt::Display for LaunchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyCommand => write!(f, "the command is empty"),
            Self::UnterminatedQuote { input } => {
                write!(f, "the command has an unterminated quote: {input}")
            }
            Self::ProgramNotFound { source, program } => {
                write!(f, "`{program}` from {source} was not found on PATH")
            }
            Self::NoEditor => write!(
                f,
                "no editor found; set VISUAL or EDITOR, install `code`, or choose one in Settings"
            ),
            Self::NoTerminal => write!(
                f,
                "no terminal emulator found; set TERMINAL or install x-terminal-emulator, \
                 gnome-terminal, konsole, or xterm"
            ),
            Self::NoFileManager => {
                write!(
                    f,
                    "no file manager launcher found; install xdg-open or dbus-send"
                )
            }
            Self::Unsupported { action, platform } => {
                write!(f, "opening {action} is not supported on {platform}")
            }
        }
    }
}

impl std::error::Error for LaunchError {}

/// Split a user-supplied command into a program and its arguments.
///
/// This is deliberately a tokenizer, not a shell: quotes group words,
/// backslash escapes the next quote/whitespace/backslash, and nothing else is
/// expanded or interpreted.
pub fn parse_command_line(input: &str) -> Result<LaunchCommand, LaunchError> {
    let mut tokens: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\'' => {
                started = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(inner) => current.push(inner),
                        None => {
                            return Err(LaunchError::UnterminatedQuote {
                                input: input.to_string(),
                            })
                        }
                    }
                }
            }
            '"' => {
                started = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some(escaped @ ('"' | '\\')) => current.push(escaped),
                            Some(other) => {
                                current.push('\\');
                                current.push(other);
                            }
                            None => {
                                return Err(LaunchError::UnterminatedQuote {
                                    input: input.to_string(),
                                })
                            }
                        },
                        Some(inner) => current.push(inner),
                        None => {
                            return Err(LaunchError::UnterminatedQuote {
                                input: input.to_string(),
                            })
                        }
                    }
                }
            }
            '\\' => {
                started = true;
                match chars.next() {
                    Some(escaped @ (' ' | '\t' | '\'' | '"' | '\\')) => current.push(escaped),
                    Some(other) => {
                        current.push('\\');
                        current.push(other);
                    }
                    None => current.push('\\'),
                }
            }
            ch if ch.is_whitespace() => {
                if started {
                    tokens.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            other => {
                started = true;
                current.push(other);
            }
        }
    }

    if started {
        tokens.push(current);
    }

    let mut tokens = tokens.into_iter();
    let program = tokens.next().ok_or(LaunchError::EmptyCommand)?;
    if program.is_empty() {
        return Err(LaunchError::EmptyCommand);
    }
    Ok(LaunchCommand {
        program,
        args: tokens.collect(),
        working_dir: None,
    })
}

fn env_nonempty(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn path_arg(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Check a Settings editor command without launching anything. An empty value
/// means automatic detection and is always valid.
pub fn validate_editor_command(spec: &str) -> Result<(), LaunchError> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Ok(());
    }
    let command = parse_command_line(spec)?;
    if resolve_editor_program_with(&command.program, &find_program).is_none() {
        return Err(LaunchError::ProgramNotFound {
            source: "Settings",
            program: command.program,
        });
    }
    Ok(())
}

/// Reveal `path` in the platform's file manager.
pub fn reveal_in_file_manager(path: &Path) -> Result<LaunchCommand, LaunchError> {
    reveal_with(DesktopPlatform::current(), path, &find_program)
}

pub fn reveal_with(
    platform: DesktopPlatform,
    path: &Path,
    resolve: &dyn Fn(&str) -> Option<String>,
) -> Result<LaunchCommand, LaunchError> {
    match platform {
        DesktopPlatform::MacOs => {
            let open = resolve("open").ok_or(LaunchError::NoFileManager)?;
            Ok(LaunchCommand::new(open, ["-R".to_string(), path_arg(path)]))
        }
        DesktopPlatform::Linux => linux_reveal(path, resolve),
        DesktopPlatform::Windows => {
            let explorer = resolve("explorer").unwrap_or_else(|| "explorer".to_string());
            // Explorer wants `/select,<path>` as one argument for a file, and
            // the directory itself for a folder.
            let args = if path.is_dir() {
                vec![path_arg(path)]
            } else {
                vec![format!("/select,{}", path_arg(path))]
            };
            Ok(LaunchCommand::new(explorer, args))
        }
        DesktopPlatform::Unsupported => Err(LaunchError::Unsupported {
            action: "the file manager",
            platform: platform.label(),
        }),
    }
}

fn linux_reveal(
    path: &Path,
    resolve: &dyn Fn(&str) -> Option<String>,
) -> Result<LaunchCommand, LaunchError> {
    // A folder is exactly what xdg-open is for. A file needs FileManager1's
    // ShowItems so the surrounding folder opens with the file selected.
    if path.is_dir() {
        if let Some(xdg_open) = resolve("xdg-open") {
            return Ok(LaunchCommand::new(xdg_open, [path_arg(path)]));
        }
        if let Some(dbus_send) = resolve("dbus-send") {
            return Ok(dbus_reveal_command(dbus_send, path));
        }
    } else {
        if let Some(dbus_send) = resolve("dbus-send") {
            return Ok(dbus_reveal_command(dbus_send, path));
        }
        if let Some(xdg_open) = resolve("xdg-open") {
            let folder = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or(path);
            return Ok(LaunchCommand::new(xdg_open, [path_arg(folder)]));
        }
    }
    Err(LaunchError::NoFileManager)
}

fn dbus_reveal_command(program: String, path: &Path) -> LaunchCommand {
    LaunchCommand::new(
        program,
        [
            "--session".to_string(),
            "--dest=org.freedesktop.FileManager1".to_string(),
            "--type=method_call".to_string(),
            "/org/freedesktop/FileManager1".to_string(),
            "org.freedesktop.FileManager1.ShowItems".to_string(),
            format!("array:string:{}", file_uri(path)),
            "string:".to_string(),
        ],
    )
}

/// Percent-encode a path as a `file://` URI for FileManager1.
fn file_uri(path: &Path) -> String {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };
    let mut uri = String::from("file://");
    for &byte in absolute.to_string_lossy().as_bytes() {
        match byte {
            b'/' => uri.push('/'),
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                uri.push(char::from(byte));
            }
            _ => uri.push_str(&format!("%{byte:02X}")),
        }
    }
    uri
}

/// Open a terminal at `path`, reusing the user's terminal when one is named.
pub fn open_terminal_at(path: &Path) -> Result<LaunchCommand, LaunchError> {
    let terminal = env_nonempty("TERMINAL");
    let use_iterm = std::env::var("TERM_PROGRAM")
        .is_ok_and(|value| value.eq_ignore_ascii_case("iTerm.app"))
        || std::env::var_os("ITERM_SESSION_ID").is_some();
    terminal_with(
        DesktopPlatform::current(),
        path,
        terminal.as_deref(),
        use_iterm,
        &find_program,
    )
}

pub fn terminal_with(
    platform: DesktopPlatform,
    path: &Path,
    terminal: Option<&str>,
    use_iterm: bool,
    resolve: &dyn Fn(&str) -> Option<String>,
) -> Result<LaunchCommand, LaunchError> {
    match platform {
        DesktopPlatform::MacOs => {
            let open = resolve("open").ok_or(LaunchError::NoTerminal)?;
            let app = if use_iterm { "iTerm" } else { "Terminal" };
            Ok(LaunchCommand::new(
                open,
                ["-a".to_string(), app.to_string(), path_arg(path)],
            ))
        }
        DesktopPlatform::Linux => {
            let mut command = match terminal.filter(|value| !value.trim().is_empty()) {
                Some(spec) => {
                    let mut command = parse_command_line(spec)?;
                    command.program =
                        resolve(&command.program).ok_or_else(|| LaunchError::ProgramNotFound {
                            source: "TERMINAL",
                            program: command.program.clone(),
                        })?;
                    command
                }
                None => {
                    let program = ["x-terminal-emulator", "gnome-terminal", "konsole", "xterm"]
                        .iter()
                        .find_map(|name| resolve(name))
                        .ok_or(LaunchError::NoTerminal)?;
                    LaunchCommand::new(program, Vec::new())
                }
            };
            // Terminals disagree on a working-directory flag; the inherited
            // process directory is the one option every xterm-compatible
            // terminal understands.
            command.working_dir = Some(path.to_path_buf());
            Ok(command)
        }
        DesktopPlatform::Windows => {
            let cmd = resolve("cmd").unwrap_or_else(|| "cmd".to_string());
            let mut command = LaunchCommand::new(
                cmd,
                [
                    "/C".to_string(),
                    "start".to_string(),
                    String::new(),
                    "cmd.exe".to_string(),
                ],
            );
            command.working_dir = Some(path.to_path_buf());
            Ok(command)
        }
        DesktopPlatform::Unsupported => Err(LaunchError::Unsupported {
            action: "a terminal",
            platform: platform.label(),
        }),
    }
}

/// Open `path` in the preferred editor, then `VISUAL`, `EDITOR`, a common
/// editor on `PATH`, and finally the platform's default app.
pub fn open_editor_at(path: &Path, preferred: Option<&str>) -> Result<LaunchCommand, LaunchError> {
    let visual = env_nonempty("VISUAL");
    let editor = env_nonempty("EDITOR");
    editor_with(
        DesktopPlatform::current(),
        path,
        preferred.filter(|value| !value.trim().is_empty()),
        visual.as_deref(),
        editor.as_deref(),
        &find_program,
    )
}

pub fn editor_with(
    platform: DesktopPlatform,
    path: &Path,
    preferred: Option<&str>,
    visual: Option<&str>,
    editor: Option<&str>,
    resolve: &dyn Fn(&str) -> Option<String>,
) -> Result<LaunchCommand, LaunchError> {
    for (source, spec) in [
        ("Settings", preferred),
        ("VISUAL", visual),
        ("EDITOR", editor),
    ] {
        if let Some(spec) = spec.filter(|value| !value.trim().is_empty()) {
            return editor_from_spec(spec, source, path, resolve);
        }
    }

    if let Some(program) = common_editor_program(resolve) {
        return Ok(LaunchCommand::new(program, [path_arg(path)]));
    }

    match platform {
        DesktopPlatform::MacOs => {
            let open = resolve("open").ok_or(LaunchError::NoEditor)?;
            Ok(LaunchCommand::new(open, [path_arg(path)]))
        }
        DesktopPlatform::Linux => {
            let xdg_open = resolve("xdg-open").ok_or(LaunchError::NoEditor)?;
            Ok(LaunchCommand::new(xdg_open, [path_arg(path)]))
        }
        DesktopPlatform::Windows => {
            let cmd = resolve("cmd").unwrap_or_else(|| "cmd".to_string());
            Ok(LaunchCommand::new(
                cmd,
                [
                    "/C".to_string(),
                    "start".to_string(),
                    String::new(),
                    path_arg(path),
                ],
            ))
        }
        DesktopPlatform::Unsupported => Err(LaunchError::Unsupported {
            action: "an editor",
            platform: platform.label(),
        }),
    }
}

fn editor_from_spec(
    spec: &str,
    source: &'static str,
    path: &Path,
    resolve: &dyn Fn(&str) -> Option<String>,
) -> Result<LaunchCommand, LaunchError> {
    let mut command = parse_command_line(spec)?;
    command.program = resolve_editor_program_with(&command.program, resolve).ok_or_else(|| {
        LaunchError::ProgramNotFound {
            source,
            program: command.program.clone(),
        }
    })?;
    command.args.push(path_arg(path));
    Ok(command)
}

/// Resolve an editor command, including the known macOS application bundle
/// paths. A Finder-launched app has a minimal `PATH`, so `code --wait` must
/// still find `/Applications/Visual Studio Code.app/.../code` when the user
/// names the command rather than the absolute binary.
fn resolve_editor_program_with(
    program: &str,
    resolve: &dyn Fn(&str) -> Option<String>,
) -> Option<String> {
    resolve(program).or_else(|| {
        EDITOR_SPECS
            .iter()
            .find(|spec| spec.programs.contains(&program))
            .and_then(|spec| {
                spec.programs
                    .iter()
                    .find_map(|candidate| resolve(candidate))
            })
    })
}

/// The first editor from the known GUI editor list that is installed. The
/// order matches Settings' editor suggestions.
fn common_editor_program(resolve: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    EDITOR_SPECS
        .iter()
        .flat_map(|spec| spec.programs.iter())
        .find_map(|program| resolve(program))
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
                push_shell(&mut shells, &mut seen, path.to_string_lossy().as_ref());
            }
        }
    }

    #[cfg(windows)]
    {
        for name in ["pwsh.exe", "powershell.exe", "cmd.exe"] {
            if let Ok(path) = which::which(name) {
                push_shell(&mut shells, &mut seen, &path.to_string_lossy().to_string());
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
        assert!(shells
            .iter()
            .any(|s| s.command == "/definitely/not/a/real/shell"));
    }

    fn resolver(
        available: &'static [(&'static str, &'static str)],
    ) -> impl Fn(&str) -> Option<String> {
        move |program| {
            available
                .iter()
                .find(|(name, _)| *name == program)
                .map(|(_, path)| (*path).to_string())
        }
    }

    #[test]
    fn parses_editor_commands_with_arguments() {
        let command = parse_command_line("code --wait").unwrap();
        assert_eq!(command.program, "code");
        assert_eq!(command.args, vec!["--wait".to_string()]);
        assert!(command.working_dir.is_none());

        let command = parse_command_line("  'code'  '--new-window'  ").unwrap();
        assert_eq!(command.program, "code");
        assert_eq!(command.args, vec!["--new-window".to_string()]);

        let command = parse_command_line(
            r#""/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code" --wait"#,
        )
        .unwrap();
        assert_eq!(
            command.program,
            "/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code"
        );
        assert_eq!(command.args, vec!["--wait".to_string()]);

        let command = parse_command_line(r"code /tmp/My\ Repo").unwrap();
        assert_eq!(command.args, vec!["/tmp/My Repo".to_string()]);

        // Backslashes in an unquoted Windows path are literal unless they
        // escape whitespace, a quote or another backslash.
        let command = parse_command_line(r"C:\Tools\code.exe --wait").unwrap();
        assert_eq!(command.program, r"C:\Tools\code.exe");
        assert_eq!(command.args, vec!["--wait".to_string()]);
    }

    #[test]
    fn rejects_empty_and_unterminated_editor_commands() {
        assert_eq!(
            parse_command_line("   ").unwrap_err(),
            LaunchError::EmptyCommand
        );
        assert_eq!(
            parse_command_line(r#""code --wait"#).unwrap_err(),
            LaunchError::UnterminatedQuote {
                input: r#""code --wait"#.to_string()
            }
        );
        assert_eq!(
            parse_command_line("'code --wait").unwrap_err(),
            LaunchError::UnterminatedQuote {
                input: "'code --wait".to_string()
            }
        );
        assert!(validate_editor_command("").is_ok());
        assert!(validate_editor_command("   ").is_ok());
        assert!(validate_editor_command(r#""code"#).is_err());
    }

    #[test]
    fn reveal_commands_follow_the_platform_and_keep_paths_whole() {
        let path = Path::new("/tmp/vantage repo");
        let resolve = resolver(&[
            ("open", "/usr/bin/open"),
            ("xdg-open", "/usr/bin/xdg-open"),
            ("dbus-send", "/usr/bin/dbus-send"),
            ("explorer", "C:\\Windows\\explorer.exe"),
        ]);

        let mac = reveal_with(DesktopPlatform::MacOs, path, &resolve).unwrap();
        assert_eq!(mac.program, "/usr/bin/open");
        assert_eq!(
            mac.args,
            vec!["-R".to_string(), "/tmp/vantage repo".to_string()]
        );

        let folder = std::env::temp_dir();
        let linux = reveal_with(DesktopPlatform::Linux, &folder, &resolve).unwrap();
        assert_eq!(linux.program, "/usr/bin/xdg-open");
        assert_eq!(linux.args, vec![folder.to_string_lossy().to_string()]);

        let file = Path::new("/tmp/vantage repo/file.txt");
        let linux_file = reveal_with(DesktopPlatform::Linux, file, &resolve).unwrap();
        assert_eq!(linux_file.program, "/usr/bin/dbus-send");
        assert!(linux_file
            .args
            .iter()
            .any(|arg| arg == "array:string:file:///tmp/vantage%20repo/file.txt"));

        let windows = reveal_with(DesktopPlatform::Windows, file, &resolve).unwrap();
        assert_eq!(windows.program, "C:\\Windows\\explorer.exe");
        assert_eq!(
            windows.args,
            vec!["/select,/tmp/vantage repo/file.txt".to_string()]
        );

        // Without dbus-send, revealing a file opens its folder instead.
        let only_xdg = resolver(&[("xdg-open", "/usr/bin/xdg-open")]);
        let fallback = reveal_with(DesktopPlatform::Linux, file, &only_xdg).unwrap();
        assert_eq!(fallback.program, "/usr/bin/xdg-open");
        assert_eq!(fallback.args, vec!["/tmp/vantage repo".to_string()]);

        let unsupported = reveal_with(DesktopPlatform::Unsupported, path, &resolve).unwrap_err();
        assert!(unsupported.to_string().contains("not supported"));
    }

    #[test]
    fn terminal_commands_follow_the_platform_and_keep_paths_whole() {
        let path = Path::new("/tmp/vantage repo");
        let resolve = resolver(&[
            ("open", "/usr/bin/open"),
            ("x-terminal-emulator", "/usr/bin/x-terminal-emulator"),
            ("gnome-terminal", "/usr/bin/gnome-terminal"),
            ("konsole", "/usr/bin/konsole"),
        ]);

        let mac = terminal_with(DesktopPlatform::MacOs, path, None, false, &resolve).unwrap();
        assert_eq!(mac.program, "/usr/bin/open");
        assert_eq!(
            mac.args,
            vec![
                "-a".to_string(),
                "Terminal".to_string(),
                "/tmp/vantage repo".to_string()
            ]
        );

        let iterm = terminal_with(DesktopPlatform::MacOs, path, None, true, &resolve).unwrap();
        assert_eq!(iterm.args[1], "iTerm");

        let linux = terminal_with(DesktopPlatform::Linux, path, None, false, &resolve).unwrap();
        assert_eq!(linux.program, "/usr/bin/x-terminal-emulator");
        assert!(linux.args.is_empty());
        assert_eq!(linux.working_dir(), Some(path));

        let custom = terminal_with(
            DesktopPlatform::Linux,
            path,
            Some("gnome-terminal --tab"),
            false,
            &resolve,
        )
        .unwrap();
        assert_eq!(custom.program, "/usr/bin/gnome-terminal");
        assert_eq!(custom.args, vec!["--tab".to_string()]);
        assert_eq!(custom.working_dir(), Some(path));

        let only_konsole =
            resolver(&[("konsole", "/usr/bin/konsole"), ("xterm", "/usr/bin/xterm")]);
        let fallback =
            terminal_with(DesktopPlatform::Linux, path, None, false, &only_konsole).unwrap();
        assert_eq!(fallback.program, "/usr/bin/konsole");

        let missing = terminal_with(
            DesktopPlatform::Linux,
            path,
            Some("definitely-not-a-terminal"),
            false,
            &resolve,
        )
        .unwrap_err();
        assert_eq!(
            missing,
            LaunchError::ProgramNotFound {
                source: "TERMINAL",
                program: "definitely-not-a-terminal".to_string()
            }
        );

        assert_eq!(
            terminal_with(DesktopPlatform::Linux, path, None, false, &resolver(&[])).unwrap_err(),
            LaunchError::NoTerminal
        );

        let windows =
            terminal_with(DesktopPlatform::Windows, path, None, false, &resolver(&[])).unwrap();
        assert_eq!(windows.program, "cmd");
        assert_eq!(windows.working_dir(), Some(path));
    }

    #[test]
    fn editor_commands_parse_arguments_and_keep_paths_whole() {
        let path = Path::new("/tmp/vantage repo");
        let resolve = resolver(&[
            ("code", "/usr/local/bin/code"),
            (
                "/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code",
                "/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code",
            ),
        ]);

        let command = editor_with(
            DesktopPlatform::MacOs,
            path,
            None,
            Some("code --wait"),
            None,
            &resolve,
        )
        .unwrap();
        assert_eq!(command.program, "/usr/local/bin/code");
        assert_eq!(
            command.args,
            vec!["--wait".to_string(), "/tmp/vantage repo".to_string()]
        );

        let quoted = editor_with(
            DesktopPlatform::MacOs,
            path,
            None,
            Some(
                "\"/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code\" \
                 --new-window",
            ),
            None,
            &resolve,
        )
        .unwrap();
        assert_eq!(
            quoted.program,
            "/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code"
        );
        assert_eq!(
            quoted.args,
            vec!["--new-window".to_string(), "/tmp/vantage repo".to_string()]
        );

        // A Finder-launched app may not have `code` on PATH, but the known
        // application bundle path still resolves the command.
        let bundle_only = resolver(&[(
            "/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code",
            "/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code",
        )]);
        let bundle = editor_with(
            DesktopPlatform::MacOs,
            path,
            None,
            Some("code --wait"),
            None,
            &bundle_only,
        )
        .unwrap();
        assert_eq!(
            bundle.program,
            "/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code"
        );
        assert_eq!(
            bundle.args,
            vec!["--wait".to_string(), "/tmp/vantage repo".to_string()]
        );
    }

    #[test]
    fn editor_precedence_prefers_settings_then_visual_then_editor_then_common() {
        let path = Path::new("/tmp/vantage repo");
        let resolve = resolver(&[
            ("code", "/bin/code"),
            ("cursor", "/bin/cursor"),
            ("nano", "/bin/nano"),
        ]);

        let settings = editor_with(
            DesktopPlatform::MacOs,
            path,
            Some("cursor"),
            Some("code --wait"),
            Some("nano"),
            &resolve,
        )
        .unwrap();
        assert_eq!(settings.program, "/bin/cursor");

        let visual = editor_with(
            DesktopPlatform::MacOs,
            path,
            None,
            Some("code --wait"),
            Some("nano"),
            &resolve,
        )
        .unwrap();
        assert_eq!(visual.program, "/bin/code");
        assert_eq!(visual.args[0], "--wait");

        let editor = editor_with(
            DesktopPlatform::MacOs,
            path,
            None,
            None,
            Some("nano"),
            &resolve,
        )
        .unwrap();
        assert_eq!(editor.program, "/bin/nano");

        let common = editor_with(DesktopPlatform::MacOs, path, None, None, None, &resolve).unwrap();
        assert_eq!(common.program, "/bin/code");
    }

    #[test]
    fn editor_without_any_candidate_uses_the_platform_default() {
        let path = Path::new("/tmp/vantage repo");
        let resolve = resolver(&[("open", "/usr/bin/open"), ("xdg-open", "/usr/bin/xdg-open")]);

        let mac = editor_with(DesktopPlatform::MacOs, path, None, None, None, &resolve).unwrap();
        assert_eq!(mac.program, "/usr/bin/open");
        assert_eq!(mac.args, vec!["/tmp/vantage repo".to_string()]);

        let linux = editor_with(DesktopPlatform::Linux, path, None, None, None, &resolve).unwrap();
        assert_eq!(linux.program, "/usr/bin/xdg-open");
        assert_eq!(linux.args, vec!["/tmp/vantage repo".to_string()]);

        let unsupported = editor_with(
            DesktopPlatform::Unsupported,
            path,
            None,
            None,
            None,
            &resolve,
        )
        .unwrap_err();
        assert!(unsupported.to_string().contains("not supported"));
    }

    #[test]
    fn invalid_editor_commands_report_actionable_errors() {
        let path = Path::new("/tmp/vantage repo");
        let resolve = resolver(&[("code", "/bin/code")]);

        let missing = editor_with(
            DesktopPlatform::MacOs,
            path,
            Some("definitely-not-an-editor --wait"),
            None,
            None,
            &resolve,
        )
        .unwrap_err();
        assert_eq!(
            missing,
            LaunchError::ProgramNotFound {
                source: "Settings",
                program: "definitely-not-an-editor".to_string()
            }
        );
        assert!(missing.to_string().contains("not found on PATH"));

        let unterminated = editor_with(
            DesktopPlatform::MacOs,
            path,
            None,
            Some("\"code"),
            None,
            &resolve,
        )
        .unwrap_err();
        assert!(matches!(
            unterminated,
            LaunchError::UnterminatedQuote { .. }
        ));
        assert!(unterminated.to_string().contains("unterminated quote"));
    }
}
