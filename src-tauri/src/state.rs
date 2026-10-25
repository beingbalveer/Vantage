use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub theme: Theme,
    pub diff_mode: DiffMode,
    pub ignore_whitespace: bool,
    pub context_lines: u32,
    pub auto_fetch_minutes: u32,
    pub shell: Option<String>,
    pub external_editor_cmd: Option<String>,
    pub commit_message_cmd: Option<String>,
    pub terminal_font_size: u32,
    pub ui_scale: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            diff_mode: DiffMode::Unified,
            ignore_whitespace: false,
            context_lines: 3,
            auto_fetch_minutes: 5,
            shell: None,
            external_editor_cmd: None,
            commit_message_cmd: None,
            terminal_font_size: 13,
            ui_scale: 1.0,
        }
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
