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
    /// The primary destinations, in sidebar order.
    pub const ALL: [Self; 3] = [Self::Overview, Self::Attention, Self::Changes];

    /// `SECTION_LABELS` in App.tsx
    pub fn label(self) -> &'static str {
        match self {
            NavSection::Overview => "All Repositories",
            NavSection::Attention => "Needs Attention",
            NavSection::Changes => "Pending Changes",
        }
    }

    /// Stable fragment for element ids.
    pub fn key(self) -> &'static str {
        match self {
            NavSection::Overview => "overview",
            NavSection::Attention => "attention",
            NavSection::Changes => "changes",
        }
    }

    /// The Lucide glyph shown beside the destination in the sidebar rail.
    pub fn icon(self) -> &'static str {
        match self {
            NavSection::Overview => "folder-git-2",
            NavSection::Attention => "alert-triangle",
            NavSection::Changes => "file-diff",
        }
    }
}

/// How many repositories a destination lists.
///
/// Derived from the current rows on every render, so a rescan that replaces
/// `repos` updates the sidebar without a second counter to keep in sync.
pub fn section_repo_count(section: NavSection, repos: &[Repository]) -> usize {
    match section {
        NavSection::Overview => repos.len(),
        NavSection::Attention => repos.iter().filter(|repo| repo.needs_attention()).count(),
        NavSection::Changes => repos
            .iter()
            .filter(|repo| !repo.uncommitted_files.is_empty())
            .count(),
    }
}

/// One sidebar destination with its live count and whether it is selected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NavItem {
    pub section: NavSection,
    pub count: usize,
    pub active: bool,
}

/// The three primary destinations for the current repository list.
pub fn nav_items(repos: &[Repository], active: NavSection) -> [NavItem; 3] {
    NavSection::ALL.map(|section| NavItem {
        section,
        count: section_repo_count(section, repos),
        active: section == active,
    })
}

/// One file changed by a commit, with its patch lines ready to render.
#[derive(Clone, Debug)]
pub struct CommitFile {
    pub path: String,
    pub additions: u32,
    pub deletions: u32,
    pub lines: Vec<PreviewLine>,
    pub truncated: bool,
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
    /// The preview stopped at the line cap; the view says so.
    pub preview_truncated: bool,
}

#[derive(Clone, Debug)]
pub struct BranchInfo {
    pub name: String,
    pub is_current: bool,
    pub is_default: bool,
    /// How long ago the branch last moved, for the row's timestamp.
    pub last_commit_time: String,
}

#[derive(Clone, Debug)]
pub struct CommitEntry {
    pub sha: String,
    pub message: String,
    pub author: String,
    /// The author's email, used to look up their GitHub photo for the row icon.
    pub author_email: String,
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
    /// Whether an `origin` exists, which gates fetch/pull/push.
    pub has_remote: bool,
    /// Number of stashes kept in the repository.
    pub stash_count: usize,
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
    pub uncommitted_files: Vec<ChangedFile>,
    pub branches: Vec<BranchInfo>,
    pub recent_commits: Vec<CommitEntry>,
}

impl Repository {
    /// Whether the commit at `index` in the newest-first list has not reached
    /// any remote.
    ///
    /// `ahead_count` is git's own count of commits the upstream — or, on a
    /// branch that tracks nothing, any remote — does not have, so the newest N
    /// rows are exactly the local ones. A repository with no remote has nothing
    /// to push to, so it gets no badges.
    pub fn commit_is_local_only(&self, index: usize) -> bool {
        self.has_remote && (index as i32) < self.ahead_count
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(ahead: i32, has_remote: bool) -> Repository {
        Repository {
            id: "repo-1".to_string(),
            name: "fixture".to_string(),
            workspace: "Ungrouped".to_string(),
            local_path: "/tmp/fixture".to_string(),
            has_remote,
            stash_count: 0,
            current_branch: "main".to_string(),
            author: None,
            sync_state: RepoSyncState::Ahead,
            ahead_count: ahead,
            behind_count: 0,
            has_upstream: true,
            last_activity_label: "just now".to_string(),
            last_activity_minutes: 0,
            uncommitted_files: Vec::new(),
            branches: Vec::new(),
            recent_commits: Vec::new(),
        }
    }

    #[test]
    fn the_newest_unpushed_commits_are_marked_local() {
        let repo = repo(2, true);
        assert!(repo.commit_is_local_only(0), "the tip is never pushed");
        assert!(repo.commit_is_local_only(1));
        assert!(
            !repo.commit_is_local_only(2),
            "everything older is on the remote"
        );
    }

    #[test]
    fn a_fully_pushed_branch_has_no_local_commits() {
        let repo = repo(0, true);
        assert!(!repo.commit_is_local_only(0));
    }

    #[test]
    fn a_repository_without_a_remote_has_nothing_to_push() {
        let repo = repo(3, false);
        assert!(!repo.commit_is_local_only(0));
        assert!(!repo.commit_is_local_only(2));
    }

    fn changed_file(path: &str) -> ChangedFile {
        ChangedFile {
            path: path.to_string(),
            status: FileChangeStatus::Modified,
            additions: 1,
            deletions: 0,
            staged: false,
            diff_preview: Vec::new(),
            preview_truncated: false,
        }
    }

    fn repo_with_status(id: &str, state: RepoSyncState) -> Repository {
        let mut repo = repo(0, true);
        repo.id = id.to_string();
        repo.sync_state = state;
        repo
    }

    #[test]
    fn nav_counts_match_the_current_repository_list() {
        let clean = repo_with_status("clean", RepoSyncState::Clean);
        let mut dirty = repo_with_status("dirty", RepoSyncState::Modified);
        dirty.uncommitted_files = vec![changed_file("src/app.rs")];
        let mut behind = repo_with_status("behind", RepoSyncState::Behind);
        behind.behind_count = 2;
        let conflict = repo_with_status("conflict", RepoSyncState::Conflict);
        let mut ahead = repo_with_status("ahead", RepoSyncState::Ahead);
        ahead.ahead_count = 2;
        let repos = vec![clean, dirty, behind, conflict, ahead];

        assert_eq!(section_repo_count(NavSection::Overview, &repos), 5);
        assert_eq!(
            section_repo_count(NavSection::Attention, &repos),
            3,
            "conflict, behind and uncommitted rows need attention; unpushed work does not"
        );
        assert_eq!(
            section_repo_count(NavSection::Changes, &repos),
            1,
            "only uncommitted files count as pending changes"
        );
    }

    #[test]
    fn only_the_selected_destination_has_the_active_state() {
        let repos = vec![repo_with_status("clean", RepoSyncState::Clean)];
        let items = nav_items(&repos, NavSection::Changes);

        assert_eq!(
            items.map(|item| item.active),
            [false, false, true],
            "one active destination, in sidebar order"
        );
        assert_eq!(items.map(|item| item.section), NavSection::ALL);
    }

    #[test]
    fn nav_counts_follow_a_rescan() {
        let repos = vec![repo_with_status("clean", RepoSyncState::Clean)];
        assert_eq!(section_repo_count(NavSection::Overview, &repos), 1);
        assert_eq!(section_repo_count(NavSection::Attention, &repos), 0);
        assert_eq!(section_repo_count(NavSection::Changes, &repos), 0);

        // The engine scan replaces the rows wholesale; counts are derived from
        // this new list rather than stored.
        let mut rescanned = repo_with_status("dirty", RepoSyncState::Modified);
        rescanned.uncommitted_files = vec![changed_file("README.md")];
        let repos = vec![rescanned];

        assert_eq!(section_repo_count(NavSection::Overview, &repos), 1);
        assert_eq!(section_repo_count(NavSection::Attention, &repos), 1);
        assert_eq!(section_repo_count(NavSection::Changes, &repos), 1);
    }
}
