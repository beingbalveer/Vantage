use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoEntry {
    pub id: String,
    pub path: String,
    pub name: String,
    #[serde(default)]
    pub branch: Option<String>,
    pub pinned: bool,
    pub order: u32,
    pub marker: Option<Marker>,
    pub notes: Vec<ReviewNote>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Marker {
    pub sha: String,
    pub set_at: String, // ISO-8601
    pub branch: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MarkerState {
    Ok,
    Missing,
    Diverged,
    None,
}

/// A repository operation that is currently in progress and must be finished or
/// abandoned before normal committing resumes (ALPH-156).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OperationKind {
    Merge,
    Rebase,
    CherryPick,
    Revert,
}

impl OperationKind {
    /// Human-facing name used in banners, confirmation dialogs and errors.
    pub fn label(&self) -> &'static str {
        match self {
            OperationKind::Merge => "merge",
            OperationKind::Rebase => "rebase",
            OperationKind::CherryPick => "cherry-pick",
            OperationKind::Revert => "revert",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoStatus {
    pub repo_id: String,
    pub branch: Option<String>,
    pub detached_sha: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub dirty_count: u32,
    /// In-progress merge/rebase/cherry-pick/revert, if any.
    #[serde(default)]
    pub operation: Option<OperationKind>,
    /// Number of paths with unmerged (conflicted) index entries.
    #[serde(default)]
    pub conflicted_count: u32,
    /// Paths with unmerged (conflicted) index entries.
    #[serde(default)]
    pub conflicted_paths: Vec<String>,
    pub new_commit_count: u32,
    pub marker_state: MarkerState,
    pub last_activity_at: Option<String>,
    #[serde(default = "default_true")]
    pub is_checked_out: bool,
    pub error: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitSummary {
    pub sha: String,
    pub short: String,
    pub author_name: String,
    pub author_email: String,
    pub date: String,
    pub subject: String,
    pub body: String,
    pub parents: Vec<String>,
    pub files_changed: u32,
    pub additions: u32,
    pub deletions: u32,
    pub flags: Vec<TriageFlag>,
    pub is_new: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPage {
    pub commits: Vec<CommitSummary>,
    /// Total commits reachable from the target ref, or the number of matching
    /// commits when a search filter is active. Saturates at the search cap.
    pub total: u32,
    /// True when a filtered search stopped at the result cap (total is a floor).
    pub capped: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    Untracked,
    Conflicted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    pub path: String,
    pub old_path: Option<String>,
    pub status: FileStatus,
    pub additions: u32,
    pub deletions: u32,
    pub binary: bool,
    pub old_mode: Option<String>,
    pub new_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffFile {
    pub change: FileChange,
    pub hunks: Vec<Hunk>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LineKind {
    Context,
    Add,
    Del,
    NoNewline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    pub kind: LineKind,
    pub old_no: Option<u32>,
    pub new_no: Option<u32>,
    pub text: String,
    pub word_ranges: Option<Vec<(u32, u32)>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DiffScope {
    Commit { sha: String },
    Range { from: String, to: String },
    Working,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FlagKind {
    Large,
    DeletionHeavy,
    Dependencies,
    Config,
    TestsRemoved,
    Secrets,
    BigFile,
    Binary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Info,
    Caution,
    Warn,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriageFlag {
    pub kind: FlagKind,
    pub severity: Severity,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NoteSide {
    New,
    Old,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewNote {
    pub id: String,
    pub file: String,
    pub line_start: u32,
    pub line_end: u32,
    pub side: NoteSide,
    pub commit_sha: Option<String>,
    pub text: String,
    pub created_at: String,
    pub snippet: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DiffMode {
    Unified,
    Split,
}

fn default_branch_name() -> String {
    "main".to_string()
}

fn default_terminal_font_size() -> u32 {
    13
}

fn default_tab_size() -> u32 {
    4
}

fn default_ui_scale() -> f32 {
    1.0
}

fn default_true_bool() -> bool {
    true
}

/// Settings persisted in `state.json`.
///
/// Every field has a serde default so that settings files written by older
/// versions (or partial/hand-edited files) still load instead of being
/// treated as corrupt. `Default` values are also the values used by the
/// `#[serde(default)]` container attribute.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub theme: Theme,
    pub diff_mode: Option<String>,
    pub ignore_whitespace: bool,
    pub context_lines: u32,
    pub auto_fetch_minutes: u32,
    pub shell: Option<String>,
    pub external_editor_cmd: Option<String>,
    pub commit_message_cmd: Option<String>,
    pub terminal_font_size: u32,
    pub ui_scale: f32,
    /// Branch name used when opening a repository that has no commits yet.
    pub default_branch: String,
    /// Indentation width used by the diff and code viewer.
    pub tab_size: u32,
    /// Ask before hiding a terminal whose shell is still running.
    pub confirm_terminal_close: bool,
    /// Ask before restarting a terminal whose shell is still running.
    pub confirm_terminal_restart: bool,
    /// Unknown keys from newer versions, preserved across saves.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            diff_mode: None,
            ignore_whitespace: false,
            context_lines: 3,
            auto_fetch_minutes: 5,
            shell: None,
            external_editor_cmd: None,
            commit_message_cmd: None,
            terminal_font_size: default_terminal_font_size(),
            ui_scale: default_ui_scale(),
            default_branch: default_branch_name(),
            tab_size: default_tab_size(),
            confirm_terminal_close: default_true_bool(),
            confirm_terminal_restart: default_true_bool(),
            extra: HashMap::new(),
        }
    }
}

/// Clamp persisted values into the ranges the UI accepts. This runs after
/// loading, so a hand-edited or out-of-range value can never break layout or
/// launch an absurd font size.
impl Settings {
    pub fn sanitized(mut self) -> Self {
        if !self.ui_scale.is_finite() {
            self.ui_scale = 1.0;
        }
        self.ui_scale = self.ui_scale.clamp(0.75, 1.5);
        self.context_lines = self.context_lines.min(20);
        self.auto_fetch_minutes = self.auto_fetch_minutes.min(60);
        self.terminal_font_size = if self.terminal_font_size == 0 {
            default_terminal_font_size()
        } else {
            self.terminal_font_size.clamp(10, 20)
        };
        self.tab_size = if self.tab_size == 0 {
            default_tab_size()
        } else {
            self.tab_size.clamp(1, 8)
        };
        self.shell = normalize_optional(self.shell);
        self.external_editor_cmd = normalize_optional(self.external_editor_cmd);
        self.commit_message_cmd = normalize_optional(self.commit_message_cmd);
        self.default_branch = self.default_branch.trim().to_string();
        if validate_branch_name(&self.default_branch).is_err() {
            self.default_branch = default_branch_name();
        }
        self
    }
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Validate a branch name using the rules from `git check-ref-format` that
/// matter for a single branch component. Kept dependency-free so it can be
/// unit tested and reused by the settings UI.
pub fn validate_branch_name(name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Branch name cannot be empty.".to_string());
    }
    if name == "HEAD" || name == "@" {
        return Err("Branch name cannot be HEAD or @.".to_string());
    }
    if name.starts_with('-') || name.starts_with('/') || name.ends_with('/') || name.ends_with('.')
    {
        return Err("Branch name cannot start with '-' or '/' or end with '/' or '.'.".to_string());
    }
    if name.contains("..") || name.contains("@{") || name.contains("//") {
        return Err("Branch name contains an invalid sequence.".to_string());
    }
    if name
        .chars()
        .any(|c| c.is_control() || " ~^:?*[\\".contains(c))
    {
        return Err("Branch name contains a character Git does not allow.".to_string());
    }
    if name
        .split('/')
        .any(|part| part.is_empty() || part.starts_with('.') || part.ends_with(".lock"))
    {
        return Err("Each branch path component must be a valid name.".to_string());
    }
    Ok(())
}

/// The initial branch target for a repo view. An explicitly requested branch
/// always wins. Otherwise, a repository with no commits (unborn HEAD) uses the
/// configured default branch name; a normal repository tracks its checked-out
/// HEAD exactly as before.
pub fn resolve_initial_branch(
    requested: Option<&str>,
    has_head: bool,
    default_branch: &str,
) -> Option<String> {
    if let Some(branch) = requested.map(str::trim).filter(|b| !b.is_empty()) {
        return Some(branch.to_string());
    }
    if has_head {
        None
    } else {
        Some(default_branch.trim().to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaneLayout {
    pub repo_id: Option<String>,
    pub tab: String, // "history" or "files"
    pub widths: Option<Vec<f32>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowBounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    pub pane_count: u32,
    pub panes: Vec<PaneLayout>,
    pub sidebar_width: f32,
    pub sidebar_collapsed: bool,
    pub terminal_height: f32,
    pub terminal_open: bool,
    pub window_bounds: Option<WindowBounds>,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            pane_count: 1,
            panes: vec![PaneLayout {
                repo_id: None,
                tab: "history".to_string(),
                widths: None,
            }],
            sidebar_width: 232.0,
            sidebar_collapsed: false,
            terminal_height: 240.0,
            terminal_open: false,
            window_bounds: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchInfo {
    pub name: String,
    pub upstream: Option<String>,
    pub is_head: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub r#ref: String,
    pub created_at: String,
    pub kind: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StashEntry {
    pub index: u32,
    pub sha: String,
    pub subject: String,
    pub date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StashPushResult {
    pub stashed: bool,
    pub entry: Option<StashEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckoutResult {
    pub stash: Option<StashEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DefHit {
    pub path: String,
    pub line: u32,
    pub line_text: String,
    pub context_lines: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlameInfo {
    pub sha: String,
    pub author: String,
    pub date: String,
    pub subject: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvInfo {
    pub git_version: String,
    pub shell: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_include_the_new_honest_settings() {
        let s = Settings::default();
        assert_eq!(s.default_branch, "main");
        assert_eq!(s.tab_size, 4);
        assert_eq!(s.terminal_font_size, 13);
        assert!(s.confirm_terminal_close);
        assert!(s.confirm_terminal_restart);
    }

    #[test]
    fn old_settings_json_without_new_fields_loads() {
        let json = r#"{
            "theme": "dark",
            "ignoreWhitespace": true,
            "contextLines": 5,
            "autoFetchMinutes": 10,
            "shell": "/bin/zsh",
            "externalEditorCmd": null,
            "commitMessageCmd": null,
            "terminalFontSize": 15,
            "uiScale": 1.1
        }"#;
        let s: Settings = serde_json::from_str(json).expect("old settings must load");
        assert_eq!(s.theme, Theme::Dark);
        assert_eq!(s.context_lines, 5);
        assert_eq!(s.terminal_font_size, 15);
        assert_eq!(s.default_branch, "main");
        assert_eq!(s.tab_size, 4);
        assert!(s.confirm_terminal_close);
    }

    #[test]
    fn partial_settings_json_fills_only_missing_fields() {
        let json = r#"{ "theme": "light" }"#;
        let s: Settings = serde_json::from_str(json).expect("partial settings must load");
        assert_eq!(s.theme, Theme::Light);
        assert_eq!(s.auto_fetch_minutes, 5);
        assert_eq!(s.terminal_font_size, 13);
    }

    #[test]
    fn unknown_settings_fields_are_preserved() {
        let json = r#"{ "theme": "light", "futureSetting": { "a": 1 } }"#;
        let s: Settings = serde_json::from_str(json).expect("unknown fields must not fail");
        assert_eq!(s.extra.get("futureSetting").unwrap()["a"], 1);
        let out = serde_json::to_string(&s).unwrap();
        assert!(out.contains("futureSetting"));
    }

    #[test]
    fn sanitize_clamps_out_of_range_values() {
        let s = Settings {
            context_lines: 999,
            auto_fetch_minutes: 999,
            terminal_font_size: 0,
            tab_size: 99,
            ui_scale: f32::NAN,
            default_branch: "  ".to_string(),
            shell: Some("  ".to_string()),
            ..Settings::default()
        }
        .sanitized();

        assert_eq!(s.context_lines, 20);
        assert_eq!(s.auto_fetch_minutes, 60);
        assert_eq!(s.terminal_font_size, 13);
        assert_eq!(s.tab_size, 8);
        assert_eq!(s.ui_scale, 1.0);
        assert_eq!(s.default_branch, "main");
        assert_eq!(s.shell, None);
    }

    #[test]
    fn sanitize_keeps_valid_values() {
        let s = Settings {
            terminal_font_size: 10,
            tab_size: 1,
            ui_scale: 1.5,
            default_branch: "develop".to_string(),
            ..Settings::default()
        }
        .sanitized();
        assert_eq!(s.terminal_font_size, 10);
        assert_eq!(s.tab_size, 1);
        assert_eq!(s.ui_scale, 1.5);
        assert_eq!(s.default_branch, "develop");
    }

    #[test]
    fn branch_name_validation() {
        for ok in ["main", "feature/ALPH-162", "release-1.0", "a_b"] {
            assert!(validate_branch_name(ok).is_ok(), "{ok} should be valid");
        }
        for bad in [
            "", " ", "HEAD", "@", "-x", "/x", "x/", "x.", "a..b", "a b", "a~b", "a:b", "a?", "a*",
            "a[b", "a//b", ".hidden", "x.lock",
        ] {
            assert!(
                validate_branch_name(bad).is_err(),
                "{bad} should be invalid"
            );
        }
    }

    #[test]
    fn initial_branch_uses_default_only_for_unborn_repos() {
        assert_eq!(
            resolve_initial_branch(Some("feature"), true, "main"),
            Some("feature".to_string())
        );
        assert_eq!(resolve_initial_branch(None, true, "main"), None);
        assert_eq!(
            resolve_initial_branch(None, false, "trunk"),
            Some("trunk".to_string())
        );
        assert_eq!(
            resolve_initial_branch(Some("  "), false, "trunk"),
            Some("trunk".to_string())
        );
    }
}
