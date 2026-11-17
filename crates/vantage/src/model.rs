//! Mirrors `src/types/repo.ts` from the Vantage React app, field for field.

use std::collections::HashSet;

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
    /// Short object name shown in the row.
    pub sha: String,
    /// Full object name, used to match the row against the commits a remote
    /// actually has. Short SHAs are ambiguous and not safe for that lookup.
    pub full_sha: String,
    pub message: String,
    /// The rest of the commit message, used to pre-fill an amend.
    pub body: String,
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
    /// HEAD is not on a branch (`git checkout <sha>`), so there is no branch
    /// name to push or publish.
    pub is_detached: bool,
    pub last_activity_label: String,
    pub last_activity_minutes: i64,
    pub uncommitted_files: Vec<ChangedFile>,
    pub branches: Vec<BranchInfo>,
    pub recent_commits: Vec<CommitEntry>,
    /// Full SHAs of commits reachable from HEAD that the upstream does not
    /// have — or, when the branch tracks nothing, that no remote has.
    ///
    /// This is the actual range (`@{upstream}..HEAD` / `HEAD --not --remotes`),
    /// not the newest N rows: on a diverged branch an upstream commit can sit
    /// between two local ones in the log. A repository with no remote has
    /// nothing to push to, so the set is empty.
    pub local_only_commits: HashSet<String>,
}

impl Repository {
    /// Whether this commit has not reached the upstream (or, on a branch that
    /// tracks nothing, any remote). A repository with no remote has nothing to
    /// push to, so it gets no badges.
    pub fn commit_is_local_only(&self, commit: &CommitEntry) -> bool {
        self.has_remote && self.local_only_commits.contains(&commit.full_sha)
    }

    /// Number of local-only commits at the top of the newest-first list.
    ///
    /// The divider belongs after this run. It is deliberately not the size of
    /// [`Self::local_only_commits`]: history can interleave a pushed commit
    /// between two local ones, and positioning by the total count would label
    /// that pushed commit as local. The per-row badge remains the source of
    /// truth for every commit.
    pub fn local_only_prefix_len(&self) -> usize {
        self.recent_commits
            .iter()
            .take_while(|commit| self.commit_is_local_only(commit))
            .count()
    }

    /// Why `Push`/`Publish` cannot run, beyond the busy and remote checks.
    ///
    /// A detached HEAD has no branch to send, and `git push -u origin HEAD`
    /// would create or target a remote branch literally named `HEAD` instead
    /// of publishing the user's work.
    pub fn push_blocked_reason(&self) -> Option<&'static str> {
        self.is_detached
            .then_some("HEAD is detached. Check out a branch before pushing.")
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

    fn commit(sha: &str) -> CommitEntry {
        CommitEntry {
            sha: sha.chars().take(7).collect(),
            full_sha: sha.to_string(),
            message: format!("subject of {sha}"),
            body: format!("body of {sha}"),
            author: "QA".to_string(),
            author_email: "qa@example.com".to_string(),
            relative_time: "just now".to_string(),
        }
    }

    fn repo(has_remote: bool, has_upstream: bool) -> Repository {
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
            ahead_count: 0,
            behind_count: 0,
            has_upstream,
            is_detached: false,
            last_activity_label: "just now".to_string(),
            last_activity_minutes: 0,
            uncommitted_files: Vec::new(),
            branches: Vec::new(),
            recent_commits: Vec::new(),
            local_only_commits: HashSet::new(),
        }
    }

    #[test]
    fn ahead_only_commits_are_marked_by_sha() {
        let mut repo = repo(true, true);
        repo.recent_commits = vec![
            commit("cccccccccccccccccccccccccccccccccccccccc"),
            commit("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
            commit("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        ];
        repo.local_only_commits = repo.recent_commits[..2]
            .iter()
            .map(|commit| commit.full_sha.clone())
            .collect();

        assert!(repo.commit_is_local_only(&repo.recent_commits[0]));
        assert!(repo.commit_is_local_only(&repo.recent_commits[1]));
        assert!(
            !repo.commit_is_local_only(&repo.recent_commits[2]),
            "everything older is on the upstream"
        );
        assert_eq!(
            repo.local_only_prefix_len(),
            2,
            "the divider follows the leading run"
        );
    }

    #[test]
    fn diverged_history_marks_only_the_commits_absent_from_upstream() {
        // Local:   aaaaaaa - bbbbbbb - ddddddd (HEAD, merge)
        // Upstream: aaaaaaa - ccccccc
        // `ddddddd` and `bbbbbbb` are local; `ccccccc` is already upstream but
        // sorts between them in the newest-first log.
        let mut repo = repo(true, true);
        repo.recent_commits = vec![
            commit("dddddddddddddddddddddddddddddddddddddddd"),
            commit("cccccccccccccccccccccccccccccccccccccccc"),
            commit("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
            commit("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        ];
        repo.local_only_commits = [
            "dddddddddddddddddddddddddddddddddddddddd",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        ]
        .iter()
        .map(|sha| sha.to_string())
        .collect();

        assert!(repo.commit_is_local_only(&repo.recent_commits[0]));
        assert!(
            !repo.commit_is_local_only(&repo.recent_commits[1]),
            "an upstream commit between two local ones must not be marked"
        );
        assert!(repo.commit_is_local_only(&repo.recent_commits[2]));
        assert!(!repo.commit_is_local_only(&repo.recent_commits[3]));
        assert_eq!(
            repo.local_only_prefix_len(),
            1,
            "the divider stops at the first pushed commit; badges mark the rest"
        );
    }

    #[test]
    fn a_fully_pushed_branch_has_no_local_commits() {
        let mut repo = repo(true, true);
        repo.recent_commits = vec![commit("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")];
        assert!(!repo.commit_is_local_only(&repo.recent_commits[0]));
        assert_eq!(repo.local_only_prefix_len(), 0);
    }

    #[test]
    fn a_branch_without_an_upstream_uses_the_all_remotes_set() {
        let mut repo = repo(true, false);
        repo.recent_commits = vec![
            commit("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
            commit("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        ];
        // `HEAD --not --remotes`: neither commit is on any remote yet, so both
        // are local even though the branch tracks nothing.
        repo.local_only_commits = repo
            .recent_commits
            .iter()
            .map(|commit| commit.full_sha.clone())
            .collect();

        assert!(repo.commit_is_local_only(&repo.recent_commits[0]));
        assert!(repo.commit_is_local_only(&repo.recent_commits[1]));
        assert_eq!(repo.local_only_prefix_len(), 2);
    }

    #[test]
    fn a_repository_without_a_remote_has_nothing_to_push() {
        let mut repo = repo(false, false);
        repo.recent_commits = vec![
            commit("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
            commit("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        ];
        // Even a stale/hand-built set must not produce badges without a push
        // target.
        repo.local_only_commits = repo
            .recent_commits
            .iter()
            .map(|commit| commit.full_sha.clone())
            .collect();

        assert!(!repo.commit_is_local_only(&repo.recent_commits[0]));
        assert!(!repo.commit_is_local_only(&repo.recent_commits[1]));
        assert_eq!(repo.local_only_prefix_len(), 0);
    }

    #[test]
    fn a_detached_head_blocks_push_with_an_actionable_reason() {
        let mut repo = repo(true, true);
        repo.is_detached = true;
        assert_eq!(
            repo.push_blocked_reason(),
            Some("HEAD is detached. Check out a branch before pushing.")
        );
    }

    #[test]
    fn an_attached_branch_has_no_push_blocker() {
        let repo = repo(true, true);
        assert_eq!(repo.push_blocked_reason(), None);
    }
}
