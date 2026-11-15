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

/// What one line of a patch does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PreviewKind {
    /// Unchanged line shown around a change.
    Context,
    Added,
    Removed,
    /// `@@ -a,b +c,d @@` boundary between hunks.
    Hunk,
}

/// One rendered line of a diff preview.
///
/// Line numbers are resolved while parsing so the view can print a gutter
/// instead of re-reading the patch: `None` means the line does not exist on
/// that side (an addition has no old line, a deletion no new line).
#[derive(Clone, Debug)]
pub struct PreviewLine {
    pub kind: PreviewKind,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct ChangedFile {
    pub path: String,
    pub status: FileChangeStatus,
    pub additions: u32,
    pub deletions: u32,
    pub staged: bool,
    pub diff_preview: Vec<PreviewLine>,
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

/// Who git will attribute the next commit to in a repository.
///
/// This is the *effective* `user.name` / `user.email`, so a repository that
/// overrides the global config reports its own identity — the one a commit made
/// from here will actually carry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitIdentity {
    pub name: String,
    pub email: String,
}

#[derive(Clone, Debug)]
pub struct Repository {
    pub id: String,
    pub name: String,
    pub workspace: String,
    pub local_path: String,
    pub remote_repo: String,
    /// `remote_repo` names a real origin rather than the "no remote" placeholder.
    pub has_remote: bool,
    pub current_branch: String,
    /// `None` when git has no usable `user.name` / `user.email` here, which
    /// also means a commit would fail.
    pub author: Option<CommitIdentity>,
    pub sync_state: RepoSyncState,
    pub ahead_count: i32,
    pub behind_count: i32,
    /// The checked-out branch tracks a remote branch, so `Push` needs no
    /// `--set-upstream` and `Pull` has something to pull from.
    pub has_upstream: bool,
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
