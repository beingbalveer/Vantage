//! Mirrors `src/types/repo.ts` from the Vantage React app, field for field.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FileChangeStatus {
    Modified,
    Added,
    Deleted,
    Untracked,
    Conflicted,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RepoSyncState {
    Clean,
    Modified,
    Ahead,
    Behind,
    Conflict,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NavSection {
    Overview,
    Attention,
    Changes,
}

impl NavSection {
    /// `SECTION_LABELS` in App.tsx
    pub fn label(self) -> &'static str {
        match self {
            NavSection::Overview => "All Repositories",
            NavSection::Attention => "Needs Attention",
            NavSection::Changes => "Pending Changes",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ChangedFile {
    pub path: String,
    pub status: FileChangeStatus,
    pub additions: u32,
    pub deletions: u32,
    pub staged: bool,
    pub diff_preview: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct BranchInfo {
    pub name: String,
    pub is_current: bool,
    pub is_default: bool,
    pub ahead: i32,
    pub behind: i32,
    pub last_commit_msg: String,
    pub last_commit_time: String,
}

#[derive(Clone, Debug)]
pub struct CommitEntry {
    pub sha: String,
    pub message: String,
    pub author: String,
    pub relative_time: String,
}

#[derive(Clone, Debug)]
pub struct Repository {
    pub id: String,
    pub name: String,
    pub workspace: String,
    pub local_path: String,
    pub remote_repo: String,
    pub current_branch: String,
    pub sync_state: RepoSyncState,
    pub ahead_count: i32,
    pub behind_count: i32,
    pub last_activity_label: String,
    pub last_activity_minutes: i64,
    pub primary_language: String,
    pub uncommitted_files: Vec<ChangedFile>,
    pub branches: Vec<BranchInfo>,
    pub recent_commits: Vec<CommitEntry>,
}

impl Repository {
    /// `RepoSyncState` drives the Working Tree cell (see StatusText.tsx).
    pub fn needs_attention(&self) -> bool {
        matches!(
            self.sync_state,
            RepoSyncState::Conflict | RepoSyncState::Behind
        ) || !self.uncommitted_files.is_empty()
    }
}

/// Helpers used by the status cell and the pending-changes list.
pub fn file_status_code(status: FileChangeStatus) -> &'static str {
    match status {
        FileChangeStatus::Modified => "M",
        FileChangeStatus::Added => "A",
        FileChangeStatus::Deleted => "D",
        FileChangeStatus::Untracked => "?",
        FileChangeStatus::Conflicted => "!",
    }
}
