//! Application state and the root view. Layout metrics follow the React app's
//! Tailwind classes: 256px sidebar, 56px top bar, 1440px content cap,
//! 576px inspector, 448px modal.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use gpui::{
    deferred, div, prelude::*, px, rgba, ClipboardItem, Context, FocusHandle, IntoElement,
    KeyDownEvent, MouseButton, PathPromptOptions, Window, WindowAppearance,
};

use crate::backend::details::RepoDetails;
use crate::backend::{Backend, BranchDeleteOutcome};
use crate::model::{
    BranchInfo, ChangedFile, FileChangeStatus, NavSection, PreviewLine, RepoSyncState, Repository,
};
use crate::settings::{
    DefaultView, Diagnostics, SettingsSection, SortOrder, ThemeChoice, UiSettings,
};
use crate::theme::Theme;
use crate::ui::{apply_field_key, FieldKey, FieldState, Txt};
use vantage_core::persist::StateBackup;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum InspectorTab {
    Changes,
    Commits,
    Branches,
}

/// The git network operation a repository is currently running.
///
/// One per repository at a time, so the row and the detail pane can show which
/// action is in flight instead of guessing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RepoOp {
    Fetch,
    Discard,
    Amend,
    Pull,
    Push,
    Commit,
    Undo,
    Branch,
    Stash,
    Revert,
    Merge,
}

impl RepoOp {
    /// Button label while this operation runs: "Fetching…", "Pulling…",
    /// "Pushing…", "Committing…".
    pub fn in_progress_label(self) -> &'static str {
        match self {
            Self::Fetch => "Fetching…",
            Self::Discard => "Discarding…",
            Self::Amend => "Amending…",
            Self::Branch => "Updating branch…",
            Self::Stash => "Stashing…",
            Self::Revert => "Reverting…",
            Self::Merge => "Merging…",
            Self::Pull => "Pulling…",
            Self::Push => "Pushing…",
            Self::Commit => "Committing…",
            Self::Undo => "Undoing…",
        }
    }
}

/// Side effect to run on the UI thread once an operation succeeds, with the
/// value the operation produced (a commit's short SHA, or `()` for the rest).
type SuccessHook<T> = Box<dyn FnOnce(&mut VantageApp, T)>;

/// The git call a branch prompt runs once the user submits a name.
type BranchAction = Box<dyn FnOnce(&Backend, &str) -> anyhow::Result<()> + Send>;

/// What a finished repository operation says, and what it cleans up.
///
/// Grouping these keeps [`VantageApp::run_repo_op`] down to the arguments that
/// differ per call site.
pub struct RepoOpReport<T = ()> {
    ok_message: String,
    fail_message: String,
    on_success: Option<SuccessHook<T>>,
}

impl<T> RepoOpReport<T> {
    pub fn new(ok_message: String, fail_message: String) -> Self {
        Self {
            ok_message,
            fail_message,
            on_success: None,
        }
    }

    /// Run `hook` on the UI thread when the operation returns successfully.
    pub fn on_success(mut self, hook: impl FnOnce(&mut VantageApp, T) + 'static) -> Self {
        self.on_success = Some(Box::new(hook));
        self
    }
}

/// How long a plain success toast stays up.
const TOAST_INFO_DURATION: Duration = Duration::from_millis(2600);
/// A short, single-line failure still clears itself, just not as quickly.
const TOAST_ERROR_DURATION: Duration = Duration::from_millis(6000);
/// A toast never grows wider than this, even on a large window.
const TOAST_MAX_WIDTH: f32 = 460.0;
/// Space between the toast and the window edge (also used for the fit test).
const TOAST_EDGE_MARGIN: f32 = 20.0;
/// Characters and lines shown in the compact toast before "Details" appears.
const TOAST_PREVIEW_CHARS: usize = 240;
const TOAST_PREVIEW_LINES: usize = 3;

/// What a toast is reporting. Errors stay longer, carry a copy action, and can
/// open a scrollable details dialog when Git's output is long.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Error,
}

/// A message shown at the bottom-right of the window.
#[derive(Clone, Debug)]
pub struct Toast {
    pub message: String,
    pub kind: ToastKind,
}

impl Toast {
    pub fn new(message: impl Into<String>, kind: ToastKind) -> Self {
        Self {
            message: message.into(),
            kind,
        }
    }

    pub fn info(message: impl Into<String>) -> Self {
        Self::new(message, ToastKind::Info)
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::new(message, ToastKind::Error)
    }

    /// True when the full message needs more room than the toast should take.
    pub fn is_detailed(&self) -> bool {
        toast_needs_details(&self.message)
    }

    /// The compact form rendered in the toast; the full text stays in `message`.
    pub fn preview(&self) -> String {
        toast_preview(&self.message)
    }

    /// Copy always uses the full message, never the shortened preview.
    pub fn copy_text(&self) -> &str {
        &self.message
    }

    /// `None` means the toast stays until the user dismisses it.
    ///
    /// Multi-line and long failures are sticky: Git's stderr is the useful part
    /// of a failed operation and must not disappear after a couple of seconds.
    pub fn auto_hide_after(&self) -> Option<Duration> {
        if self.is_detailed() || self.message.trim_end().contains('\n') {
            return None;
        }
        match self.kind {
            ToastKind::Info => Some(TOAST_INFO_DURATION),
            ToastKind::Error => Some(TOAST_ERROR_DURATION),
        }
    }
}

/// The commit composer for one repository: the message, the optional body and
/// the amend mode with the tip it is allowed to rewrite.
#[derive(Clone, Default)]
pub struct ComposerState {
    pub message: FieldState,
    pub body: FieldState,
    /// The next commit rewrites the tip instead of creating a new one.
    pub amending: bool,
    /// The tip captured when amend mode started; cleared with the mode.
    pub base_tip: Option<String>,
}

impl ComposerState {
    fn clear(&mut self) {
        self.message.clear();
        self.body.clear();
        self.cancel_amend();
    }

    /// Amend mode belongs to the repository it started in and never moves.
    fn cancel_amend(&mut self) {
        self.amending = false;
        self.base_tip = None;
    }

    /// The repository and tip the next commit may write.
    fn commit_target(&self, repo: &Repository) -> CommitTarget {
        let row_tip = repo.recent_commits.first().map(|commit| commit.sha.clone());
        CommitTarget {
            repo_id: repo.id.clone(),
            expected_head: if self.amending {
                self.base_tip.clone().or(row_tip)
            } else {
                row_tip
            },
        }
    }
}

/// The active composer plus one unfinished draft per repository the user
/// typed into. The composer belongs to the selected repository, so a message
/// written for one repository can never be submitted for another.
#[derive(Default)]
pub struct ComposerStore {
    active_repo: Option<String>,
    active: ComposerState,
    drafts: std::collections::HashMap<String, ComposerState>,
}

impl ComposerStore {
    pub fn active(&self) -> &ComposerState {
        &self.active
    }

    pub fn active_mut(&mut self) -> &mut ComposerState {
        &mut self.active
    }

    /// Make `repo_id`'s composer active, saving the previous one as a draft.
    /// Amend mode is deliberately dropped instead of travelling with the
    /// selection.
    pub fn switch_to(&mut self, repo_id: &str) {
        if self.active_repo.as_deref() == Some(repo_id) {
            return;
        }
        self.leave();
        let mut composer = self.drafts.remove(repo_id).unwrap_or_default();
        composer.cancel_amend();
        self.active = composer;
        self.active_repo = Some(repo_id.to_string());
    }

    /// Put the active composer back into its draft slot and clear the pane.
    pub fn leave(&mut self) {
        let Some(repo_id) = self.active_repo.take() else {
            self.active = ComposerState::default();
            return;
        };
        let mut composer = std::mem::take(&mut self.active);
        composer.cancel_amend();
        if composer.message.value.is_empty() && composer.body.value.is_empty() {
            self.drafts.remove(&repo_id);
        } else {
            self.drafts.insert(repo_id, composer);
        }
    }

    /// Spend only one repository's draft after its commit or amend succeeds.
    pub fn clear_for(&mut self, repo_id: &str) {
        self.drafts.remove(repo_id);
        if self.active_repo.as_deref() == Some(repo_id) {
            self.active.clear();
        }
    }

    /// Forget a repository that was removed from the workspace.
    pub fn forget(&mut self, repo_id: &str) {
        self.drafts.remove(repo_id);
        if self.active_repo.as_deref() == Some(repo_id) {
            self.active_repo = None;
            self.active = ComposerState::default();
        }
    }
}

/// The repository and tip a commit or amend was started against. The job
/// compares both with the world when it runs, so a queued write cannot land on
/// a repository or HEAD the user did not choose.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitTarget {
    pub repo_id: String,
    pub expected_head: Option<String>,
}

impl CommitTarget {
    /// `Some(message)` when the job must not write: it was queued for another
    /// repository, or HEAD moved before the operation ran.
    fn refusal(&self, repo_id: &str, current_head: Option<&str>) -> Option<String> {
        if self.repo_id != repo_id {
            return Some(
                "The commit was started for a different repository. Refresh and try again."
                    .to_string(),
            );
        }
        if !same_tip(self.expected_head.as_deref(), current_head) {
            return Some(
                "The repository moved to a different commit while the commit was starting. \
                 Refresh and try again."
                    .to_string(),
            );
        }
        None
    }
}

/// Compare the tip captured from a row with git's current HEAD. Rows carry an
/// abbreviated SHA, so a prefix match in either direction counts.
fn same_tip(expected: Option<&str>, current: Option<&str>) -> bool {
    match (expected, current) {
        (None, None) => true,
        (Some(expected), Some(current)) => {
            !expected.is_empty()
                && !current.is_empty()
                && (current.starts_with(expected) || expected.starts_with(current))
        }
        _ => false,
    }
}

/// A detailed toast shows a bounded preview; its copy action and dialog keep
/// the complete text.
fn toast_needs_details(message: &str) -> bool {
    let message = message.trim_end();
    message.chars().count() > TOAST_PREVIEW_CHARS || message.lines().count() > TOAST_PREVIEW_LINES
}

/// Cap a message at [`TOAST_PREVIEW_LINES`] and [`TOAST_PREVIEW_CHARS`],
/// marking the cut with an ellipsis. The full string is never modified.
fn toast_preview(message: &str) -> String {
    let message = message.trim_end();
    if !toast_needs_details(message) {
        return message.to_string();
    }

    let mut preview = message
        .lines()
        .take(TOAST_PREVIEW_LINES)
        .collect::<Vec<_>>()
        .join("\n");

    // Keep one character of the budget for the ellipsis, so the preview itself
    // also stays inside `TOAST_PREVIEW_CHARS`.
    let mut chars = preview.chars();
    let head: String = chars.by_ref().take(TOAST_PREVIEW_CHARS - 1).collect();
    if chars.next().is_some() {
        preview = format!("{head}…");
    } else {
        preview.push_str("\n…");
    }
    preview
}

/// Width available to the toast, leaving the same margin on both sides so it
/// still fits at the 1040 px minimum window size.
fn toast_max_width(window_width: f32) -> f32 {
    (window_width - TOAST_EDGE_MARGIN * 2.0).clamp(0.0, TOAST_MAX_WIDTH)
}

/// A compact text action inside the toast, inheriting its text size and colour.
fn toast_action(theme: &Theme, label: &'static str) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .justify_center()
        .px(px(8.0))
        .py(px(3.0))
        .rounded(px(5.0))
        .cursor_pointer()
        .whitespace_nowrap()
        .flex_shrink_0()
        .fg(theme.toast_text)
        .hover(move |s| s.bg(crate::theme::alpha(theme.toast_text, 0.12)))
        .child(label)
}

/// The commit this session most recently created, so the detail pane can offer
/// an Undo for a commit that has not been pushed (as GitHub Desktop does).
#[derive(Clone, Debug)]
pub struct LastCommit {
    pub repo_id: String,
    pub short: String,
    pub summary: String,
    /// The commit that was the branch tip before this one. The row list only
    /// catches up after the rescan, so this is what lets the Undo strip appear
    /// immediately instead of a beat later.
    pub previous_tip: Option<String>,
}

/// Keymap context for the repository list. Plain arrows and Enter only act
/// while the list itself owns the keyboard, never while a text field does.
pub const REPO_LIST_KEY_CONTEXT: &str = "RepoList";
/// Every editable field carries this context, so a future global binding can
/// exclude text entry without knowing which field is focused.
pub const TEXT_FIELD_KEY_CONTEXT: &str = "TextField";
/// An open dialog. Escape and Tab are handled at this level, and because the
/// context is on the dialog card it also matches when a field inside it is
/// focused.
pub const MODAL_KEY_CONTEXT: &str = "Modal";

/// A patch preview loaded when the user expands one changed file.
#[derive(Clone, Debug)]
pub struct FilePreview {
    pub staged: bool,
    pub status: FileChangeStatus,
    pub additions: u32,
    pub deletions: u32,
    pub lines: Vec<PreviewLine>,
    pub truncated: bool,
    pub error: Option<String>,
}

impl FilePreview {
    /// Whether this preview still describes `file`. A rescan that changes the
    /// counts or staged state invalidates it, so the row can never show an old
    /// patch beside new numbers.
    fn matches(&self, file: &ChangedFile) -> bool {
        self.staged == file.staged
            && self.status == file.status
            && self.additions == file.additions
            && self.deletions == file.deletions
    }
}

/// Scan requests that arrived while another scan was running.
///
/// Only one entry is kept: dirty roots are unioned and a full-workspace request
/// supersedes them, so a build that keeps touching files cannot grow a queue
/// of full scans.
#[derive(Clone, Debug, Default)]
struct PendingScans {
    full: bool,
    roots: std::collections::BTreeSet<PathBuf>,
    /// True only when every queued request asked for a silent scan.
    silent: bool,
    queued: bool,
}

/// What the inspector knows about a repository's lazily loaded details.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailsState {
    Loading,
    Error,
    Ready,
}

/// The status signals that make cached inspector details stale.
#[derive(Clone, Debug, PartialEq, Eq)]
struct DetailsKey {
    head_sha: Option<String>,
    branch: String,
}

impl DetailsKey {
    fn of(repo: &Repository) -> Self {
        Self {
            head_sha: repo.head_sha.clone(),
            branch: repo.current_branch.clone(),
        }
    }

    fn matches(&self, repo: &Repository) -> bool {
        self.head_sha == repo.head_sha && self.branch == repo.current_branch
    }
}

#[derive(Clone, Debug)]
struct CachedRepoDetails {
    key: DetailsKey,
    details: RepoDetails,
}

#[derive(Clone, Debug)]
struct DetailsError {
    key: DetailsKey,
    message: String,
}

/// Per-repository cache for inspector details.
///
/// One request per repository runs at a time. A cache hit is reused until HEAD
/// or the checked-out branch changes, or an operation explicitly invalidates
/// it. Each invalidation bumps a per-repository version so a result that was
/// already in flight cannot fill the cache with pre-operation data.
#[derive(Default)]
struct DetailsStore {
    cache: std::collections::HashMap<String, CachedRepoDetails>,
    loading: std::collections::HashSet<String>,
    /// Details known to be stale after an operation, waiting for the overview
    /// scan that supplies the new HEAD before the next read starts.
    pending: std::collections::HashSet<String>,
    errors: std::collections::HashMap<String, DetailsError>,
    versions: std::collections::HashMap<String, u64>,
}

impl DetailsStore {
    fn cached(&self, repo: &Repository) -> Option<&RepoDetails> {
        self.cache
            .get(&repo.id)
            .filter(|entry| entry.key.matches(repo))
            .map(|entry| &entry.details)
    }

    fn is_loading(&self, repo_id: &str) -> bool {
        self.loading.contains(repo_id) || self.pending.contains(repo_id)
    }

    fn error(&self, repo_id: &str) -> Option<&str> {
        self.errors.get(repo_id).map(|error| error.message.as_str())
    }

    fn needs_load(&self, repo: &Repository) -> bool {
        !self.loading.contains(&repo.id)
            && !self.pending.contains(&repo.id)
            && !self
                .cache
                .get(&repo.id)
                .is_some_and(|entry| entry.key.matches(repo))
            && !self
                .errors
                .get(&repo.id)
                .is_some_and(|error| error.key.matches(repo))
    }

    fn begin(&mut self, repo: &Repository) -> Option<(DetailsKey, u64)> {
        if !self.needs_load(repo) {
            return None;
        }
        let key = DetailsKey::of(repo);
        let version = self.versions.get(&repo.id).copied().unwrap_or(0);
        self.pending.remove(&repo.id);
        self.loading.insert(repo.id.clone());
        self.errors.remove(&repo.id);
        Some((key, version))
    }

    fn finish(
        &mut self,
        repo_id: &str,
        key: DetailsKey,
        version: u64,
        result: Result<RepoDetails, String>,
    ) {
        self.loading.remove(repo_id);
        if self.versions.get(repo_id).copied().unwrap_or(0) != version {
            return;
        }
        self.pending.remove(repo_id);
        match result {
            Ok(details) => {
                self.errors.remove(repo_id);
                self.cache
                    .insert(repo_id.to_string(), CachedRepoDetails { key, details });
            }
            Err(message) => {
                self.errors
                    .insert(repo_id.to_string(), DetailsError { key, message });
            }
        }
    }

    fn invalidate(&mut self, repo_id: &str) {
        *self.versions.entry(repo_id.to_string()).or_insert(0) += 1;
        self.cache.remove(repo_id);
        self.errors.remove(repo_id);
        self.pending.insert(repo_id.to_string());
    }

    /// The overview now reports this repository's new HEAD, so a pending
    /// detail read can start.
    fn observed(&mut self, repo_ids: &[String]) {
        for repo_id in repo_ids {
            self.pending.remove(repo_id);
        }
    }

    /// Retry a failed read now, without waiting for another overview scan.
    fn retry(&mut self, repo_id: &str) {
        *self.versions.entry(repo_id.to_string()).or_insert(0) += 1;
        self.cache.remove(repo_id);
        self.errors.remove(repo_id);
        self.pending.remove(repo_id);
    }
}

impl PendingScans {
    fn push(&mut self, roots: Option<Vec<PathBuf>>, silent: bool) {
        if !self.queued {
            self.silent = true;
            self.queued = true;
        }
        self.silent &= silent;
        match roots {
            None => {
                self.full = true;
                self.roots.clear();
            }
            Some(roots) if !self.full => self.roots.extend(roots),
            Some(_) => {}
        }
    }

    fn take(&mut self) -> Option<(Option<Vec<PathBuf>>, bool)> {
        if !self.queued {
            return None;
        }
        let roots = if self.full {
            None
        } else {
            Some(self.roots.iter().cloned().collect())
        };
        let silent = self.silent;
        *self = Self::default();
        Some((roots, silent))
    }
}

/// One repository found by the Add Repository dialog's folder scan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScannedRepo {
    pub path: String,
    pub name: String,
    pub selected: bool,
}

/// Preview of a scanned folder: the repositories that can be added, which of
/// them are ticked, and the raw scan count so the dialog can distinguish an
/// empty folder from one whose repositories are all already in the workspace.
#[derive(Clone, Debug)]
pub struct ScanPreview {
    pub folder: String,
    pub scanned: usize,
    pub candidates: Vec<ScannedRepo>,
}

impl ScanPreview {
    /// Build a preview from the paths a scan returned; every candidate starts
    /// ticked, so the common "add everything" case is one click.
    pub fn new(folder: &std::path::Path, scanned: usize, repositories: Vec<PathBuf>) -> Self {
        let candidates = repositories
            .into_iter()
            .map(|path| {
                let name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "repository".to_string());
                ScannedRepo {
                    path: path.to_string_lossy().into_owned(),
                    name,
                    selected: true,
                }
            })
            .collect();
        Self {
            folder: folder.to_string_lossy().into_owned(),
            scanned,
            candidates,
        }
    }

    pub fn selected_count(&self) -> usize {
        self.candidates.iter().filter(|repo| repo.selected).count()
    }

    pub fn all_selected(&self) -> bool {
        !self.candidates.is_empty() && self.candidates.iter().all(|repo| repo.selected)
    }

    pub fn selected_paths(&self) -> Vec<String> {
        self.candidates
            .iter()
            .filter(|repo| repo.selected)
            .map(|repo| repo.path.clone())
            .collect()
    }

    pub fn toggle(&mut self, path: &str) {
        if let Some(repo) = self.candidates.iter_mut().find(|repo| repo.path == path) {
            repo.selected = !repo.selected;
        }
    }

    pub fn set_all(&mut self, selected: bool) {
        for repo in &mut self.candidates {
            repo.selected = selected;
        }
    }

    /// Keep only the paths that still need adding. Used after a partial batch
    /// add so a retry cannot submit a repository that is already in the
    /// workspace.
    pub fn retain_paths(&mut self, paths: &HashSet<String>) {
        self.candidates.retain(|repo| paths.contains(&repo.path));
        self.scanned = self.candidates.len();
    }
}

/// Which text field a keystroke belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldId {
    NewGroup,
    ModalPath,
    ModalCustomGroup,
    SettingsBranch,
    SettingsEditor,
    CommitMessage,
    CommitBody,
    BranchName,
    CloneUrl,
    CloneParent,
    NewRepoName,
    NewRepoParent,
}

impl FieldId {
    /// Fields that live inside a dialog. Their widgets only exist while that
    /// dialog is open, so they are tab stops only for that lifetime.
    fn is_modal(self) -> bool {
        matches!(
            self,
            Self::ModalPath
                | Self::ModalCustomGroup
                | Self::SettingsBranch
                | Self::BranchName
                | Self::CloneUrl
                | Self::CloneParent
                | Self::NewRepoName
                | Self::NewRepoParent
        )
    }
}

/// A focus destination the app can restore after a dialog closes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FocusTarget {
    RepoList,
    Field(FieldId),
    Modal,
}

pub struct VantageApp {
    pub theme: Theme,
    pub settings: UiSettings,
    /// The settings last handed to the writer. A click that leaves preferences
    /// unchanged (opening the panel that is already open, say) must not queue a
    /// disk write at all.
    last_saved_settings: UiSettings,
    pub is_settings_open: bool,
    pub settings_section: SettingsSection,
    pub settings_reset_confirm: bool,
    pub diagnostics: Option<Diagnostics>,
    pub last_auto_fetch: Option<Instant>,
    pub did_initial_fetch: bool,
    pub repos: Vec<Repository>,
    pub groups: Vec<String>,
    pub active_section: NavSection,
    /// "All" or a group name.
    pub active_group: String,
    /// Filter chosen from the summary strip above the repository list.
    pub summary_filter: Option<SummaryFilter>,
    pub selected_repo_id: Option<String>,
    pub is_add_modal_open: bool,
    pub is_refreshing: bool,
    pub toast: Option<Toast>,
    toast_seq: u64,
    /// Load warning from startup (corrupt/unreadable state), kept after the
    /// banner is dismissed so Settings → Data can still show it.
    pub state_warning: Option<String>,
    /// The recovery banner was dismissed; the warning remains in Settings.
    pub state_warning_dismissed: bool,
    /// Recoverable files beside the live state file, refreshed when Settings
    /// opens.
    pub state_backups: Vec<StateBackup>,
    pub state_data_dir: PathBuf,
    toast_details_open: bool,
    toast_copied: bool,
    pub collapsed_groups: HashSet<String>,
    /// Left sidebar shrunk to its 56px icon rail.
    pub sidebar_collapsed: bool,
    /// `(pointer x, starting fraction)` while the right splitter is dragged.
    pub right_panel_drag_start: Option<(f32, f32)>,
    /// Throttle activation-triggered background rescans.
    last_focus_refresh: Option<Instant>,

    // Sidebar: inline "new group" input
    pub is_adding_group: bool,
    pub group_field: FieldState,
    group_field_focus: FocusHandle,

    // Inspector
    pub inspector_tab: InspectorTab,
    pub expanded_file: Option<String>,
    pub group_menu_open: bool,
    pub copied_path: bool,
    /// Commit message, body and amend mode for the selected repository.
    /// Inactive repositories keep their unfinished drafts here too.
    composers: ComposerStore,
    commit_focus: FocusHandle,

    // Add-repository modal
    pub modal_path: FieldState,
    pub modal_group_choice: String,
    pub modal_custom_group: FieldState,
    pub modal_error: Option<String>,
    pub modal_group_menu_open: bool,
    pub modal_is_adding: bool,
    /// Folder-scan preview; `Some` switches the modal from one path to a
    /// checkbox list of repositories.
    pub modal_scan: Option<ScanPreview>,
    pub modal_is_scanning: bool,
    /// Discards a scan/picker result that arrives after the modal moved on.
    modal_scan_seq: u64,

    // Settings modal
    pub settings_branch_field: FieldState,
    settings_branch_focus: FocusHandle,
    pub settings_editor_field: FieldState,
    settings_editor_focus: FocusHandle,

    pub backend: Option<std::sync::Arc<Backend>>,
    pub assignments: std::collections::BTreeMap<String, String>,
    pub is_scanning: bool,
    /// Repositories with a fetch/pull/push still running, and which one. Rows
    /// and the detail pane turn the running action into a spinner from this.
    pub busy_repos: std::collections::HashMap<String, RepoOp>,
    /// Right-click menu, at the pointer.
    pub context_menu: Option<ContextMenu>,
    commit_body_focus: FocusHandle,
    /// A branch action waiting for its name or confirmation.
    pub branch_prompt: Option<BranchPrompt>,
    /// A destructive action waiting for its confirm.
    pub confirm: Option<ConfirmPrompt>,
    /// Stash list dialog: repository and its entries.
    pub stash_modal: Option<StashModal>,
    /// Clone dialog: url, destination and whether git is running.
    pub is_clone_modal_open: bool,
    /// Create-repository dialog.
    pub is_new_repo_modal_open: bool,
    pub new_repo_name: FieldState,
    pub new_repo_parent: FieldState,
    pub new_repo_gitignore: bool,
    pub is_creating_repo: bool,
    new_repo_name_focus: FocusHandle,
    new_repo_parent_focus: FocusHandle,
    pub clone_url: FieldState,
    pub clone_parent: FieldState,
    pub is_cloning: bool,
    /// Latest `git clone --progress` line, shown under the clone form.
    pub clone_progress: Option<String>,
    /// Why the last clone attempt did not finish, shown until it is retried.
    pub clone_failure: Option<CloneFailure>,
    clone_cancel: Option<vantage_core::CancelToken>,
    clone_url_focus: FocusHandle,
    clone_parent_focus: FocusHandle,
    /// The commit opened in the Commits tab, if any.
    pub commit_view: Option<CommitView>,
    /// The pane borrows width from the list while a commit diff is open.
    pub commit_wide: bool,
    /// `(pointer x, starting width)` while the sidebar divider is dragged.
    pub sidebar_drag_start: Option<(f32, f32)>,
    /// Name typed for the pending create/rename.
    pub branch_field: FieldState,
    branch_focus: FocusHandle,
    /// The commit created from this session, while it is still the branch tip.
    pub last_commit: Option<LastCommit>,
    /// Paths the user ticked in the detail pane, per repository, until a commit
    /// spends them. Kept separately from the scan's `staged` flag because a
    /// background rescan can arrive between a tick and the commit and would
    /// otherwise drop that file from the commit.
    selection: std::collections::HashMap<String, std::collections::HashMap<String, bool>>,
    /// GitHub profile photos keyed by commit email. `Some(None)` means the
    /// lookup finished without a photo, so we stop retrying it.
    avatars: std::collections::HashMap<String, Option<std::sync::Arc<gpui::Image>>>,
    avatars_pending: HashSet<String>,
    /// Set by a text field's press handler, read by the window-level press
    /// handler: a press that no field claimed clears focus.
    pub text_click_claimed: bool,
    pub scan_errors: Vec<String>,
    watcher: Option<crate::watcher::RepoWatcher>,
    scan_seq: u64,
    /// Monotonic identity for commit, comparison and stash loads. Every load
    /// carries the value it was started with, so a result that arrives after a
    /// newer load replaced it is discarded instead of populating the wrong
    /// view.
    load_seq: u64,
    /// A scan is running; new requests are coalesced into [`Self::pending_scans`].
    scan_in_flight: bool,
    pending_scans: PendingScans,
    details: DetailsStore,
    /// Patch previews loaded on demand, keyed by `(repository id, file path)`.
    file_previews: std::collections::HashMap<(String, String), FilePreview>,
    file_previews_pending: std::collections::HashSet<(String, String)>,
    modal_path_focus: FocusHandle,
    modal_custom_group_focus: FocusHandle,
    /// Owns the plain arrow/Enter bindings while the repository list is the
    /// active keyboard context. Clicking a row focuses it.
    repo_list_focus: FocusHandle,
    /// The dialog card itself: focused for dialogs without a field, and the
    /// ancestor key context that makes Escape/Tab work anywhere inside.
    pub(crate) modal_focus: FocusHandle,
    /// Where focus should return after the open dialog closes.
    last_main_focus: Option<FocusTarget>,
    /// One-shot focus request, used when the inline New Group row appears.
    pending_focus: Option<FocusTarget>,
    /// Set by dialog close paths; consumed by the next render.
    restore_focus: bool,
    /// Flushes queued preference writes before the process exits.
    _quit_subscription: gpui::Subscription,
}

impl VantageApp {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let backend = Backend::new().map(std::sync::Arc::new).ok();
        Self::build(cx, backend, None)
    }

    /// Build the app with a warning already obtained by the caller.
    ///
    /// `main` must read preferences before opening the window, which loads the
    /// state file before [`Backend`] exists; passing that first load's warning
    /// here keeps a corrupt-state notice from being lost.
    pub fn new_with_startup_warning(cx: &mut Context<Self>, state_warning: Option<String>) -> Self {
        let backend = Backend::new().map(std::sync::Arc::new).ok();
        Self::build(cx, backend, state_warning)
    }

    /// Build the app around an already-created backend. Tests use this to
    /// point persistence at a temporary state file.
    fn build(
        cx: &mut Context<Self>,
        backend: Option<std::sync::Arc<Backend>>,
        startup_warning: Option<String>,
    ) -> Self {
        let state_warning = startup_warning.or_else(|| {
            backend
                .as_ref()
                .and_then(|backend| backend.take_state_warning())
        });
        let state_backups = backend
            .as_ref()
            .map(|backend| backend.state_backups())
            .unwrap_or_default();
        let state_data_dir = backend
            .as_ref()
            .map(|backend| backend.state_data_dir())
            .unwrap_or_default();
        let (groups, assignments) = match backend.as_ref() {
            Some(backend) => backend.groups(),
            None => (Vec::new(), Default::default()),
        };
        let settings = backend
            .as_ref()
            .map(|backend| backend.ui_settings())
            .unwrap_or_default();
        let quit_subscription = cx.on_app_quit(|app, _cx| {
            app.flush_state();
            async {}
        });

        Self {
            theme: if settings.theme == ThemeChoice::Dark {
                Theme::dark()
            } else {
                Theme::light()
            },
            settings: settings.clone(),
            last_saved_settings: settings.clone(),
            is_settings_open: false,
            settings_section: SettingsSection::General,
            settings_reset_confirm: false,
            diagnostics: None,
            last_auto_fetch: None,
            did_initial_fetch: false,
            // Repositories come exclusively from the engine scan; a fresh
            // install starts empty so the viewport can render its empty state.
            repos: Vec::new(),
            groups,
            active_section: settings.default_view.nav_section(),
            active_group: "All".to_string(),
            summary_filter: None,
            selected_repo_id: None,
            is_add_modal_open: false,
            is_refreshing: false,
            toast: None,
            toast_seq: 0,
            state_warning,
            state_warning_dismissed: false,
            state_backups,
            state_data_dir,
            toast_details_open: false,
            toast_copied: false,
            collapsed_groups: HashSet::new(),
            sidebar_collapsed: settings.sidebar_collapsed,
            right_panel_drag_start: None,
            last_focus_refresh: Some(Instant::now()),

            is_adding_group: false,
            group_field: FieldState::new(),
            group_field_focus: cx.focus_handle(),

            inspector_tab: InspectorTab::Changes,
            expanded_file: None,
            group_menu_open: false,
            copied_path: false,
            composers: ComposerStore::default(),
            commit_focus: cx.focus_handle(),

            modal_path: FieldState::new(),
            modal_group_choice: "Ungrouped".to_string(),
            modal_custom_group: FieldState::new(),
            modal_error: None,
            modal_group_menu_open: false,
            modal_is_adding: false,
            modal_scan: None,
            modal_is_scanning: false,
            modal_scan_seq: 0,

            settings_branch_field: FieldState::new(),
            settings_branch_focus: cx.focus_handle(),
            settings_editor_field: FieldState::new(),
            settings_editor_focus: cx.focus_handle(),

            backend,
            assignments,
            is_scanning: false,
            busy_repos: std::collections::HashMap::new(),
            last_commit: None,
            context_menu: None,
            commit_body_focus: cx.focus_handle(),
            branch_prompt: None,
            confirm: None,
            stash_modal: None,
            is_clone_modal_open: false,
            is_new_repo_modal_open: false,
            new_repo_name: FieldState::new(),
            new_repo_parent: FieldState::new(),
            new_repo_gitignore: true,
            is_creating_repo: false,
            new_repo_name_focus: cx.focus_handle(),
            new_repo_parent_focus: cx.focus_handle(),
            clone_url: FieldState::new(),
            clone_parent: FieldState::new(),
            is_cloning: false,
            clone_progress: None,
            clone_failure: None,
            clone_cancel: None,
            clone_url_focus: cx.focus_handle(),
            clone_parent_focus: cx.focus_handle(),
            commit_view: None,
            commit_wide: false,
            sidebar_drag_start: None,
            branch_field: FieldState::new(),
            branch_focus: cx.focus_handle(),
            selection: std::collections::HashMap::new(),
            avatars: std::collections::HashMap::new(),
            avatars_pending: HashSet::new(),
            text_click_claimed: false,
            scan_errors: Vec::new(),
            watcher: None,
            scan_seq: 0,
            load_seq: 0,
            scan_in_flight: false,
            pending_scans: PendingScans::default(),
            details: DetailsStore::default(),
            file_previews: std::collections::HashMap::new(),
            file_previews_pending: std::collections::HashSet::new(),
            modal_path_focus: cx.focus_handle(),
            modal_custom_group_focus: cx.focus_handle(),
            repo_list_focus: cx.focus_handle(),
            modal_focus: cx.focus_handle(),
            last_main_focus: None,
            pending_focus: None,
            restore_focus: false,
            _quit_subscription: quit_subscription,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_backend(cx: &mut Context<Self>, backend: Backend) -> Self {
        Self::build(cx, Some(std::sync::Arc::new(backend)), None)
    }

    #[cfg(test)]
    pub(crate) fn with_backend_and_warning(
        cx: &mut Context<Self>,
        backend: Backend,
        warning: String,
    ) -> Self {
        Self::build(cx, Some(std::sync::Arc::new(backend)), Some(warning))
    }

    // ---- scanning & persistence -----------------------------------------

    /// Re-read every repo through Vantage's git engine (background thread).
    pub fn start_scan(&mut self, cx: &mut Context<Self>) {
        self.begin_scan(None, false, cx);
    }

    /// Re-read every repo without surfacing the scanning state.
    pub fn start_scan_silent(&mut self, cx: &mut Context<Self>) {
        self.begin_scan(None, true, cx);
    }

    /// Refresh whenever the app becomes active. This is deliberately silent:
    /// the current rows remain visible until the fresh snapshot arrives.
    pub fn refresh_on_focus(&mut self, cx: &mut Context<Self>) {
        // The running scan already reads the workspace; queueing a second full
        // scan behind it would only repeat that work.
        if self.scan_in_flight {
            self.last_focus_refresh = Some(Instant::now());
            return;
        }
        let now = Instant::now();
        if self
            .last_focus_refresh
            .is_some_and(|last| now.duration_since(last) < Duration::from_secs(2))
        {
            return;
        }
        self.last_focus_refresh = Some(now);
        self.start_scan_silent(cx);
    }

    /// Re-read only the working trees the watcher reported as changed, leaving
    /// every other row untouched.
    pub fn start_scan_dirty(&mut self, roots: Vec<PathBuf>, cx: &mut Context<Self>) {
        self.begin_scan(Some(roots), false, cx);
    }

    /// Queue a scan, or remember it while another one is already running.
    ///
    /// Watchers can report dirty roots many times a second during a build.
    /// Coalescing them keeps at most one follow-up scan pending, and a
    /// full-workspace request supersedes the collected roots instead of being
    /// queued behind them.
    fn begin_scan(&mut self, roots: Option<Vec<PathBuf>>, silent: bool, cx: &mut Context<Self>) {
        if self.scan_in_flight {
            self.pending_scans.push(roots, silent);
            if !silent {
                self.is_scanning = true;
                cx.notify();
            }
            return;
        }
        self.launch_scan(roots, silent, cx);
    }

    fn launch_scan(&mut self, roots: Option<Vec<PathBuf>>, silent: bool, cx: &mut Context<Self>) {
        let Some(backend) = self.backend.clone() else {
            self.scan_errors = vec!["vantage-core engine unavailable".to_string()];
            cx.notify();
            return;
        };

        self.scan_in_flight = true;
        self.scan_seq = self.scan_seq.wrapping_add(1);
        let seq = self.scan_seq;
        if !silent {
            self.is_scanning = true;
            cx.notify();
        }

        let dirty = roots.is_some();
        cx.spawn(async move |this, cx| {
            let (repositories, errors) = cx
                .background_executor()
                .spawn(async move {
                    match roots {
                        Some(roots) => backend.snapshot_subset(&roots),
                        None => backend.snapshot(),
                    }
                })
                .await;

            this.update(cx, |app, cx| {
                // A newer scan owns the state; leave it in flight for that one.
                if app.scan_seq != seq {
                    return;
                }
                app.scan_in_flight = false;
                app.is_scanning = false;
                app.scan_errors = errors;
                let updated_ids: Vec<String> =
                    repositories.iter().map(|repo| repo.id.clone()).collect();

                if dirty {
                    for repository in repositories {
                        match app.repos.iter().position(|repo| repo.id == repository.id) {
                            Some(index) => app.repos[index] = repository,
                            None => app.repos.push(repository),
                        }
                    }
                } else {
                    app.repos = repositories;

                    // Keep the configured order, then append groups only found
                    // on disk.
                    app.bind_watcher(cx);
                    app.maybe_fetch_on_launch(cx);
                }

                for repo in &app.repos {
                    if !app.groups.contains(&repo.workspace) {
                        app.groups.push(repo.workspace.clone());
                    }
                }

                if !dirty {
                    if app.active_group != "All" && !app.groups.contains(&app.active_group) {
                        app.active_group = "All".to_string();
                    }
                    if let Some(id) = app.selected_repo_id.clone() {
                        if !app.repos.iter().any(|repo| repo.id == id) {
                            app.composers.leave();
                            app.selected_repo_id = None;
                        }
                    }
                }

                app.prune_selection();
                app.details.observed(&updated_ids);
                app.refresh_selected_details(cx);
                app.invalidate_file_previews_for(&updated_ids);
                app.refresh_expanded_preview(cx);

                if let Some((roots, silent)) = app.pending_scans.take() {
                    app.launch_scan(roots, silent, cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Watch every scanned working tree; a debounced poll rescans on change.
    fn bind_watcher(&mut self, cx: &mut Context<Self>) {
        let roots: Vec<std::path::PathBuf> = self
            .repos
            .iter()
            .filter_map(|repo| self.repo_path_opt(repo))
            .filter(|path| path.is_dir())
            .collect();

        if self.watcher.is_none() {
            match crate::watcher::RepoWatcher::new() {
                Ok(watcher) => {
                    self.watcher = Some(watcher);
                    self.poll_working_trees(cx);
                }
                Err(error) => {
                    self.show_toast(
                        format!(
                            "File watching is unavailable, so external Git changes may \
                             not refresh: {error}"
                        ),
                        cx,
                    );
                    return;
                }
            }
        }

        let errors = match self.watcher.as_mut() {
            Some(watcher) => {
                watcher.set_roots(roots);
                watcher.registration_errors().to_vec()
            }
            None => Vec::new(),
        };
        if let Some(first) = errors.first() {
            let more = errors.len().saturating_sub(1);
            let suffix = if more > 0 {
                format!(" (+{more} more)")
            } else {
                String::new()
            };
            self.show_toast(
                format!(
                    "Could not watch every repository, so external Git changes may not \
                     refresh: {first}{suffix}"
                ),
                cx,
            );
        }
    }

    fn poll_working_trees(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(700))
                .await;

            let dirty = this
                .update(cx, |app, _| {
                    app.watcher
                        .as_ref()
                        .map(|watcher| watcher.take_dirty())
                        .unwrap_or_default()
                })
                .unwrap_or_default();

            if dirty.is_empty() {
                continue;
            }
            let _ = this.update(cx, |app, cx| app.start_scan_dirty(dirty, cx));
        })
        .detach();
    }

    /// Persist groups and assignments into Vantage's own state file.
    ///
    /// Only rows backed by the engine are merged in: repositories that failed to
    /// scan must not overwrite the assignments already on disk, and rows that
    /// are not on screen keep their stored group.
    ///
    /// Returns whether the write landed. Failures are shown as a toast and the
    /// return value lets a caller skip its own success message.
    pub fn persist(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(backend) = self.backend.clone() else {
            self.show_toast(
                "Workspace state is unavailable; the change was not saved",
                cx,
            );
            return false;
        };
        let known = backend.repo_ids();
        sync_assignments(&self.repos, &known, &mut self.assignments);
        let result = backend.set_groups(&self.groups, &self.assignments);
        if let Some(warning) = backend.take_state_warning() {
            self.state_warning = Some(warning);
            self.state_warning_dismissed = false;
        }
        match result {
            Ok(()) => true,
            Err(error) => {
                self.show_toast(format!("Workspace changes were not saved: {error}"), cx);
                false
            }
        }
    }

    fn repo_path_opt(&self, repo: &Repository) -> Option<std::path::PathBuf> {
        if let Some(backend) = &self.backend {
            if let Some(path) = backend.repo_path(&repo.id) {
                return Some(path);
            }
        }
        let expanded = expand_home(&repo.local_path);
        expanded.is_absolute().then_some(expanded)
    }

    // ---- toasts ----------------------------------------------------------

    /// Show a short-lived informational message. Successes and hints keep the
    /// original 2.6 s lifetime.
    pub fn show_toast(&mut self, message: impl Into<String>, cx: &mut Context<Self>) {
        self.set_toast(Toast::info(message), cx);
    }

    /// Show a failure. Single-line errors stay for 6 s; long or multi-line Git
    /// output stays until dismissed, with copy and details actions.
    pub fn show_error_toast(&mut self, message: impl Into<String>, cx: &mut Context<Self>) {
        self.set_toast(Toast::error(message), cx);
    }

    fn set_toast(&mut self, toast: Toast, cx: &mut Context<Self>) {
        let auto_hide = toast.auto_hide_after();
        self.toast = Some(toast);
        self.toast_details_open = false;
        self.toast_copied = false;
        self.toast_seq = self.toast_seq.wrapping_add(1);
        let seq = self.toast_seq;
        cx.notify();

        let Some(duration) = auto_hide else {
            return;
        };
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(duration).await;
            this.update(cx, |this, cx| {
                // A newer toast owns the slot now; its timer must not be
                // cancelled by this older one.
                if this.toast_seq == seq {
                    this.toast = None;
                    this.toast_details_open = false;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    pub fn dismiss_toast(&mut self, cx: &mut Context<Self>) {
        self.toast_seq = self.toast_seq.wrapping_add(1);
        self.toast = None;
        self.toast_details_open = false;
        self.toast_copied = false;
        cx.notify();
    }

    /// Copy the complete toast text. Long errors show a preview in the toast,
    /// so this must use the stored message rather than the clipped rendering.
    pub fn copy_toast(&mut self, cx: &mut Context<Self>) {
        let Some(message) = self
            .toast
            .as_ref()
            .map(|toast| toast.copy_text().to_string())
        else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(message));
        self.toast_copied = true;
        cx.notify();
    }

    pub fn open_toast_details(&mut self, cx: &mut Context<Self>) {
        let Some(toast) = self.toast.as_ref() else {
            return;
        };
        if toast.is_detailed() {
            self.toast_details_open = true;
            cx.notify();
        }
    }

    pub fn close_toast_details(&mut self, cx: &mut Context<Self>) {
        self.toast_details_open = false;
        cx.notify();
    }

    // ---- derived data ----------------------------------------------------

    /// `groupedRepos` in App.tsx
    pub fn grouped_repos(&self) -> Vec<&Repository> {
        if self.active_group == "All" {
            self.repos.iter().collect()
        } else {
            self.repos
                .iter()
                .filter(|r| r.workspace == self.active_group)
                .collect()
        }
    }

    /// Visible repositories, newest activity first.
    pub fn filtered_repos(&self) -> Vec<&Repository> {
        let mut list: Vec<&Repository> = self.grouped_repos();

        if self.active_section == NavSection::Attention {
            list.retain(|r| r.needs_attention());
        }

        if let Some(filter) = self.summary_filter {
            list.retain(|repo| match filter {
                SummaryFilter::All => true,
                SummaryFilter::Uncommitted => !repo.uncommitted_files.is_empty(),
                SummaryFilter::Unpushed => repo.ahead_count > 0,
                SummaryFilter::Clean => {
                    repo.uncommitted_files.is_empty()
                        && repo.ahead_count <= 0
                        && repo.behind_count <= 0
                        && repo.sync_state != RepoSyncState::Conflict
                }
            });
        }

        match self.settings.sort {
            SortOrder::Recent => list.sort_by_key(|repo| repo.last_activity_minutes),
            SortOrder::Name => list.sort_by(|a, b| a.name.cmp(&b.name)),
            SortOrder::Attention => list.sort_by(|a, b| {
                attention_rank(a)
                    .cmp(&attention_rank(b))
                    .then(a.last_activity_minutes.cmp(&b.last_activity_minutes))
                    .then(a.name.cmp(&b.name))
            }),
            SortOrder::Changes => list.sort_by(|a, b| {
                let count = |repo: &Repository| {
                    let files = repo.uncommitted_files.len();
                    let churn: u32 = repo
                        .uncommitted_files
                        .iter()
                        .map(|file| file.additions.saturating_add(file.deletions))
                        .sum();
                    (files, churn)
                };
                count(b).cmp(&count(a))
            }),
        }
        list
    }

    pub fn group_count(&self, group: &str) -> usize {
        if group == "All" {
            self.repos.len()
        } else {
            self.repos.iter().filter(|r| r.workspace == group).count()
        }
    }

    pub fn selected_repo(&self) -> Option<&Repository> {
        self.selected_repo_id
            .as_ref()
            .and_then(|id| self.repos.iter().find(|r| &r.id == id))
    }

    /// The composer for the selected repository.
    pub fn composer(&self) -> &ComposerState {
        self.composers.active()
    }

    /// Whether the selected repository's inspector details are loading,
    /// failed, or ready.
    pub fn details_state(&self, repo_id: &str) -> DetailsState {
        if self.details.is_loading(repo_id) {
            DetailsState::Loading
        } else if self.details.error(repo_id).is_some() {
            DetailsState::Error
        } else {
            DetailsState::Ready
        }
    }

    /// The message from the last failed detail load for this repository.
    pub fn details_error(&self, repo_id: &str) -> Option<&str> {
        self.details.error(repo_id)
    }

    /// Retry a failed detail load for the selected repository.
    pub fn retry_details(&mut self, cx: &mut Context<Self>) {
        let Some(repo) = self.selected_repo().cloned() else {
            return;
        };
        self.details.retry(&repo.id);
        self.ensure_details_for(&repo, cx);
        cx.notify();
    }

    fn apply_details_for(&mut self, repo_id: &str, details: &RepoDetails) {
        if let Some(repo) = self.repos.iter_mut().find(|repo| repo.id == repo_id) {
            repo.has_remote = details.has_remote;
            repo.stash_count = details.stash_count;
            repo.author = details.author.clone();
            repo.branches = details.branches.clone();
            repo.recent_commits = details.recent_commits.clone();
            repo.local_only_commits = details.local_only_commits.clone();
        }
    }

    fn clear_details_for(&mut self, repo_id: &str) {
        if let Some(repo) = self.repos.iter_mut().find(|repo| repo.id == repo_id) {
            repo.has_remote = false;
            repo.stash_count = 0;
            repo.author = None;
            repo.branches.clear();
            repo.recent_commits.clear();
        }
    }

    /// Load the inspector details for `repo` once per HEAD/branch, or apply
    /// the cached copy. A request already running for this repository is not
    /// duplicated; its completion re-checks the row and starts a new load only
    /// when HEAD or the branch moved while it ran.
    fn ensure_details_for(&mut self, repo: &Repository, cx: &mut Context<Self>) {
        if let Some(details) = self.details.cached(repo).cloned() {
            self.apply_details_for(&repo.id, &details);
            if let Some(current) = self.selected_repo().cloned() {
                self.warm_avatars_for(&current, cx);
            }
            return;
        }

        let Some((key, version)) = self.details.begin(repo) else {
            return;
        };
        let Some(backend) = self.backend.clone() else {
            self.details.finish(
                &repo.id,
                key,
                version,
                Err("Git engine unavailable".to_string()),
            );
            return;
        };

        let repo_id = repo.id.clone();
        let branch = repo.current_branch.clone();
        let has_head = repo.head_sha.is_some();
        self.clear_details_for(&repo_id);
        cx.spawn(async move |this, cx| {
            let load_id = repo_id.clone();
            let result = cx
                .background_executor()
                .spawn(async move { backend.details(&load_id, &branch, has_head) })
                .await
                .map_err(|error| error.to_string());

            this.update(cx, |app, cx| {
                app.details.finish(&repo_id, key, version, result);
                if let Some(repo) = app.selected_repo().cloned() {
                    if repo.id == repo_id {
                        if let Some(details) = app.details.cached(&repo).cloned() {
                            app.apply_details_for(&repo.id, &details);
                            app.warm_avatars_for(&repo, cx);
                        } else {
                            // HEAD or the branch changed while the read ran;
                            // load again for the row now on screen.
                            app.ensure_details_for(&repo, cx);
                        }
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn refresh_selected_details(&mut self, cx: &mut Context<Self>) {
        let Some(repo) = self.selected_repo().cloned() else {
            return;
        };
        self.ensure_details_for(&repo, cx);
    }

    /// Drop the cached details so the next scan/selection reads them again.
    fn invalidate_details(&mut self, repo_id: &str) {
        self.details.invalidate(repo_id);
        self.clear_details_for(repo_id);
    }

    fn warm_avatars_for(&mut self, repo: &Repository, cx: &mut Context<Self>) {
        self.ensure_avatars(repo, cx);
    }

    /// The loaded preview for `file`, when it still matches the row's numbers.
    pub fn file_preview(&self, repo_id: &str, file: &ChangedFile) -> Option<&FilePreview> {
        self.file_previews
            .get(&(repo_id.to_string(), file.path.clone()))
            .filter(|preview| preview.matches(file))
    }

    /// Whether a preview request for this row is still running.
    pub fn file_preview_pending(&self, repo_id: &str, path: &str) -> bool {
        self.file_previews_pending
            .contains(&(repo_id.to_string(), path.to_string()))
    }

    /// Read one file's patch because its row was just expanded.
    ///
    /// The scan deliberately carries no patches; this is the only place a
    /// changed-file diff is read. Requests are keyed by repository and path so
    /// a second click while the first read runs does not start a duplicate.
    pub(crate) fn request_file_preview(
        &mut self,
        repo_id: String,
        file: &ChangedFile,
        cx: &mut Context<Self>,
    ) {
        if crate::views::inspector::is_image_path(&file.path) {
            return;
        }

        let key = (repo_id.clone(), file.path.clone());
        if self.file_previews_pending.contains(&key)
            || self
                .file_previews
                .get(&key)
                .is_some_and(|preview| preview.error.is_none() && preview.matches(file))
        {
            return;
        }

        let Some(backend) = self.backend.clone() else {
            return;
        };
        let path = file.path.clone();
        let staged = file.staged;
        let status = file.status;
        let additions = file.additions;
        let deletions = file.deletions;
        let orig_path = file.orig_path.clone();

        self.file_previews_pending.insert(key.clone());
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    backend.file_preview(&repo_id, &path, staged, status, orig_path.as_deref())
                })
                .await;

            this.update(cx, |app, cx| {
                app.file_previews_pending.remove(&key);
                let (lines, truncated, error) = match result {
                    Ok((lines, truncated)) => (lines, truncated, None),
                    Err(error) => (Vec::new(), false, Some(error.to_string())),
                };
                let failed = error.is_some();
                app.file_previews.insert(
                    key,
                    FilePreview {
                        staged,
                        status,
                        additions,
                        deletions,
                        lines,
                        truncated,
                        error,
                    },
                );
                // A scan may have replaced the row while this read ran; reload
                // only when the preview no longer describes the expanded file.
                // A failed read waits for the next scan or a fresh expand, so
                // one bad patch cannot spin retries.
                if !failed {
                    app.refresh_expanded_preview(cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Drop previews for rows the scan just re-read. The expanded row reloads
    /// through [`Self::refresh_expanded_preview`]; every other row will load
    /// again if the user opens it.
    fn invalidate_file_previews_for(&mut self, repo_ids: &[String]) {
        if repo_ids.is_empty() {
            return;
        }
        let updated: HashSet<&str> = repo_ids.iter().map(String::as_str).collect();
        self.file_previews
            .retain(|(repo_id, _), _| !updated.contains(repo_id.as_str()));
        // Requests already running are kept: removing their key would start a
        // duplicate read, and their completion re-checks the new row anyway.
    }

    /// Reload the expanded row's preview after a scan replaced its file list.
    fn refresh_expanded_preview(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.expanded_file.clone() else {
            return;
        };
        let Some(repo) = self.selected_repo() else {
            return;
        };
        let Some(file) = repo
            .uncommitted_files
            .iter()
            .find(|file| file.path == path)
            .cloned()
        else {
            return;
        };
        let repo_id = repo.id.clone();
        self.request_file_preview(repo_id, &file, cx);
    }

    // ---- actions ---------------------------------------------------------

    pub fn select_section(&mut self, section: NavSection, cx: &mut Context<Self>) {
        self.active_section = section;
        cx.notify();
    }

    pub fn select_group(&mut self, group: impl Into<String>, cx: &mut Context<Self>) {
        self.active_group = group.into();
        cx.notify();
    }

    pub fn open_add_modal(&mut self, cx: &mut Context<Self>) {
        self.is_add_modal_open = true;
        self.modal_group_choice = default_group_for_new_repo(&self.active_group);
        self.modal_path.clear();
        self.modal_custom_group.clear();
        self.modal_error = None;
        self.modal_group_menu_open = false;
        self.modal_is_adding = false;
        self.reset_modal_scan();
        cx.notify();
    }

    pub fn close_add_modal(&mut self, cx: &mut Context<Self>) {
        if self.modal_is_adding {
            return;
        }
        self.is_add_modal_open = false;
        self.modal_group_menu_open = false;
        self.restore_focus = true;
        self.reset_modal_scan();
        cx.notify();
    }

    /// Drop a folder preview and invalidate any scan or picker still running.
    fn reset_modal_scan(&mut self) {
        self.modal_scan = None;
        self.modal_is_scanning = false;
        self.modal_scan_seq = self.modal_scan_seq.wrapping_add(1);
    }

    pub fn select_repo(&mut self, id: String, cx: &mut Context<Self>) {
        if self.selected_repo_id.as_deref() != Some(id.as_str()) {
            self.expanded_file = None;
            self.context_menu = None;
            self.group_menu_open = false;
            self.copied_path = false;
        }
        // Load this repository's own draft before the pane renders. Amend mode
        // is dropped for the repository we are leaving.
        self.composers.switch_to(&id);
        self.selected_repo_id = Some(id);
        if !self.settings.right_panel_open {
            self.settings.right_panel_open = true;
            self.save_settings(cx);
        }
        if let Some(repo) = self.selected_repo().cloned() {
            self.ensure_details_for(&repo, cx);
            self.ensure_avatars(&repo, cx);
        }
        cx.notify();
    }

    /// Paths a commit from this pane would include, in list order.
    pub fn ticked_paths(&self, repo: &Repository) -> Vec<String> {
        ticked_paths(self.selection.get(&repo.id), repo)
    }

    /// How many files a commit would include, for the button and the header.
    pub fn ticked_count(&self, repo: &Repository) -> usize {
        self.ticked_paths(repo).len()
    }

    fn set_selected(&mut self, repo_id: &str, path: &str, staged: bool) {
        self.selection
            .entry(repo_id.to_string())
            .or_default()
            .insert(path.to_string(), staged);
    }

    /// Forget overrides the last scan has caught up with, so the map only ever
    /// holds choices git has not applied yet.
    fn prune_selection(&mut self) {
        for repo in &self.repos {
            let Some(overrides) = self.selection.get_mut(&repo.id) else {
                continue;
            };
            overrides.retain(|path, staged| {
                repo.uncommitted_files
                    .iter()
                    .find(|file| &file.path == path)
                    .is_some_and(|file| file.staged != *staged)
            });
        }
        self.selection.retain(|_, overrides| !overrides.is_empty());
    }

    /// The fetched GitHub photo for a commit email, when one is available.
    pub fn avatar_for(&self, email: &str) -> Option<std::sync::Arc<gpui::Image>> {
        self.avatars.get(email).cloned().flatten()
    }

    /// Load the commit-bar face and any history faces already on disk.
    ///
    /// The commit bar may resolve the machine's `gh` account because it names
    /// the identity a commit from here will use. History rows must not: their
    /// faces come from the cache and only when the commit email proves a login,
    /// so one repository selection cannot start a download per author.
    fn ensure_avatars(&mut self, repo: &Repository, cx: &mut Context<Self>) {
        self.ensure_commit_bar_avatar(repo, cx);
        self.ensure_history_avatars(repo, cx);
    }

    /// Fetch the photo for the identity this repository will commit as.
    fn ensure_commit_bar_avatar(&mut self, repo: &Repository, cx: &mut Context<Self>) {
        let Some(author) = repo.author.clone() else {
            return;
        };
        let Some(email) = self.claim_avatar_lookup(author.email) else {
            return;
        };
        let path = expand_home(&repo.local_path);
        let load_email = email.clone();
        cx.spawn(async move |this, cx| {
            let avatar = cx
                .background_executor()
                .spawn(async move { crate::avatar_cache::load_current(&load_email, &path) })
                .await;
            this.update(cx, |app, cx| {
                app.avatars_pending.remove(&email);
                app.avatars.insert(email, Self::avatar_image(avatar));
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Read photos already cached for this repository's commit authors.
    ///
    /// This never downloads. An unproven email keeps the initials tile, and a
    /// cache miss is not retried: the fallback `gh` account belongs to this
    /// machine, not necessarily to the person who wrote the commit.
    fn ensure_history_avatars(&mut self, repo: &Repository, cx: &mut Context<Self>) {
        let mut emails = Vec::new();
        for email in repo
            .recent_commits
            .iter()
            .map(|commit| commit.author_email.clone())
        {
            if let Some(email) = self.claim_avatar_lookup(email) {
                emails.push(email);
            }
        }
        if emails.is_empty() {
            return;
        }

        let path = expand_home(&repo.local_path);
        cx.spawn(async move |this, cx| {
            let avatars = cx
                .background_executor()
                .spawn(async move {
                    // `github.user` is repository-wide, so read it once rather
                    // than once per author.
                    let explicit = crate::avatar_cache::explicit_github_user(&path);
                    emails
                        .into_iter()
                        .map(|email| {
                            let avatar = crate::avatar_cache::cached(&email, explicit.as_deref());
                            (email, avatar)
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            this.update(cx, |app, cx| {
                for (email, avatar) in avatars {
                    app.avatars_pending.remove(&email);
                    app.avatars.insert(email, Self::avatar_image(avatar));
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Mark an email as being looked up, unless it has already been handled.
    fn claim_avatar_lookup(&mut self, email: String) -> Option<String> {
        if email.trim().is_empty()
            || self.avatars.contains_key(&email)
            || self.avatars_pending.contains(&email)
        {
            return None;
        }
        self.avatars_pending.insert(email.clone());
        Some(email)
    }

    fn avatar_image(
        avatar: Option<crate::avatar_cache::Avatar>,
    ) -> Option<std::sync::Arc<gpui::Image>> {
        avatar.map(|avatar| {
            // GitHub serves PNG or JPEG depending on the uploaded avatar, so
            // the detected format is what we decode with.
            let format = match avatar.format {
                crate::avatar_cache::AvatarFormat::Png => gpui::ImageFormat::Png,
                crate::avatar_cache::AvatarFormat::Jpeg => gpui::ImageFormat::Jpeg,
                crate::avatar_cache::AvatarFormat::Gif => gpui::ImageFormat::Gif,
                crate::avatar_cache::AvatarFormat::Webp => gpui::ImageFormat::Webp,
            };
            std::sync::Arc::new(gpui::Image::from_bytes(format, avatar.bytes))
        })
    }

    pub fn toggle_right_panel(&mut self, cx: &mut Context<Self>) {
        self.settings.right_panel_open = !self.settings.right_panel_open;
        self.save_settings(cx);
        cx.notify();
    }

    pub fn close_right_panel(&mut self, cx: &mut Context<Self>) {
        self.settings.right_panel_open = false;
        self.group_menu_open = false;
        self.save_settings(cx);
        cx.notify();
    }

    pub fn close_inspector(&mut self, cx: &mut Context<Self>) {
        self.settings.right_panel_open = false;
        self.composers.leave();
        self.selected_repo_id = None;
        self.group_menu_open = false;
        self.expanded_file = None;
        self.save_settings(cx);
        cx.notify();
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.is_refreshing {
            return;
        }
        self.is_refreshing = true;
        self.start_scan(cx);
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(550))
                .await;
            this.update(cx, |this, cx| {
                this.is_refreshing = false;
                this.show_toast("Repository status refreshed", cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn toggle_theme(&mut self, cx: &mut Context<Self>) {
        self.settings.theme = if self.theme.is_dark {
            ThemeChoice::Light
        } else {
            ThemeChoice::Dark
        };
        self.theme = if self.settings.theme == ThemeChoice::Dark {
            Theme::dark()
        } else {
            Theme::light()
        };
        self.save_settings(cx);
        cx.notify();
    }

    pub fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar_collapsed = !self.sidebar_collapsed;
        self.settings.sidebar_collapsed = self.sidebar_collapsed;
        self.save_settings(cx);
        cx.notify();
    }

    pub fn open_settings(&mut self, cx: &mut Context<Self>) {
        self.is_add_modal_open = false;
        self.modal_group_menu_open = false;
        self.is_settings_open = true;
        self.settings_section = SettingsSection::General;
        self.settings_reset_confirm = false;
        self.settings_branch_field
            .set(self.settings.default_branch.clone());
        self.settings_editor_field
            .set(self.settings.editor_command.clone());
        self.diagnostics = Some(Diagnostics::collect(&self.state_path()));
        self.refresh_state_backups();
        cx.notify();
    }

    /// Refresh the recoverable-backup list from the state directory.
    pub fn refresh_state_backups(&mut self) {
        let Some(backend) = &self.backend else {
            self.state_backups.clear();
            return;
        };
        self.state_backups = backend.state_backups();
        self.state_data_dir = backend.state_data_dir();
    }

    /// Open Settings on the Data section, where backups are listed.
    pub fn open_data_settings(&mut self, cx: &mut Context<Self>) {
        self.open_settings(cx);
        self.settings_section = SettingsSection::Data;
        cx.notify();
    }

    pub fn close_settings(&mut self, cx: &mut Context<Self>) {
        self.is_settings_open = false;
        self.settings_reset_confirm = false;
        self.restore_focus = true;
        cx.notify();
    }

    pub fn state_path(&self) -> std::path::PathBuf {
        self.backend
            .as_ref()
            .map(|backend| backend.state_path())
            .unwrap_or_default()
    }

    pub fn set_theme_choice(
        &mut self,
        choice: ThemeChoice,
        appearance: WindowAppearance,
        cx: &mut Context<Self>,
    ) {
        self.settings.theme = choice;
        self.apply_theme(appearance);
        self.save_settings(cx);
        cx.notify();
    }

    pub fn apply_theme(&mut self, appearance: WindowAppearance) {
        self.theme = if self.settings.theme.is_dark(appearance) {
            Theme::dark()
        } else {
            Theme::light()
        };
    }

    pub fn on_appearance_changed(&mut self, window: &Window, cx: &mut Context<Self>) {
        self.apply_theme(window.appearance());
        cx.notify();
    }

    pub fn set_default_view(&mut self, view: DefaultView, cx: &mut Context<Self>) {
        self.settings.default_view = view;
        self.save_settings(cx);
        cx.notify();
    }

    pub fn set_sort(&mut self, sort: SortOrder, cx: &mut Context<Self>) {
        self.settings.sort = sort;
        self.save_settings(cx);
        cx.notify();
    }

    pub fn set_auto_fetch_minutes(&mut self, minutes: u32, cx: &mut Context<Self>) {
        self.settings.auto_fetch_minutes = minutes;
        self.last_auto_fetch = None;
        self.save_settings(cx);
        cx.notify();
    }

    pub fn set_fetch_on_launch(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.settings.fetch_on_launch = enabled;
        self.save_settings(cx);
        cx.notify();
    }

    pub fn set_pull_strategy(
        &mut self,
        strategy: vantage_core::git::refs::PullStrategy,
        cx: &mut Context<Self>,
    ) {
        self.settings.pull_strategy = strategy;
        self.save_settings(cx);
        cx.notify();
    }

    pub fn set_remember_window(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.settings.remember_window = enabled;
        if !enabled {
            self.settings.window_bounds = None;
        }
        self.save_settings(cx);
        cx.notify();
    }

    pub fn commit_settings_branch(&mut self, cx: &mut Context<Self>) {
        let branch = self.settings_branch_field.value.trim().to_string();
        if let Err(error) = vantage_core::state::validate_branch_name(&branch) {
            self.show_error_toast(error, cx);
            return;
        }
        self.settings.default_branch = branch;
        if self.save_settings(cx) {
            self.show_toast("Default branch saved", cx);
        }
    }

    pub fn commit_settings_editor(&mut self, cx: &mut Context<Self>) {
        let command = self.settings_editor_field.value.trim().to_string();
        if let Err(error) = vantage_core::detect::validate_editor_command(&command) {
            self.show_toast(format!("Could not save the editor: {error}"), cx);
            return;
        }
        self.settings.editor_command = command;
        self.save_settings(cx);
        self.show_toast("Editor saved", cx);
    }

    pub fn reset_settings(&mut self, appearance: WindowAppearance, cx: &mut Context<Self>) {
        self.settings = UiSettings::default();
        self.sidebar_collapsed = false;
        self.settings_branch_field
            .set(self.settings.default_branch.clone());
        self.settings_editor_field
            .set(self.settings.editor_command.clone());
        self.settings_reset_confirm = false;
        self.apply_theme(appearance);
        if self.save_settings(cx) {
            self.show_toast("Preferences reset", cx);
        }
    }

    pub fn save_settings(&mut self, cx: &mut Context<Self>) -> bool {
        if self.settings == self.last_saved_settings {
            return true;
        }
        let Some(backend) = self.backend.clone() else {
            return false;
        };
        match backend.set_ui_settings(&self.settings) {
            Ok(()) => {
                self.last_saved_settings = self.settings.clone();
                true
            }
            Err(error) => {
                self.show_toast(format!("Could not save settings: {error}"), cx);
                false
            }
        }
    }

    /// Commit queued preference writes. Called on quit; a failure here has no
    /// window left to toast in, so it goes to stderr as a last resort.
    pub fn flush_state(&mut self) {
        let Some(backend) = self.backend.clone() else {
            return;
        };
        if let Err(error) = backend.flush_state() {
            eprintln!("failed to save state while quitting: {error}");
        }
    }

    /// Poll the writer for problems that happen after a debounced save
    /// returned. Without this a read-only state directory would look like a
    /// successful toggle until the next launch.
    pub fn watch_state_notices(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(900))
                .await;

            let notice = this.update(cx, |app, _| {
                app.backend
                    .as_ref()
                    .and_then(|backend| backend.take_state_notice())
            });
            match notice {
                Ok(Some(message)) => {
                    if this
                        .update(cx, |app, cx| app.show_toast(message, cx))
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(None) => {}
                Err(_) => break,
            }
        })
        .detach();
    }

    pub fn save_window_bounds(
        &mut self,
        bounds: gpui::Bounds<gpui::Pixels>,
        is_fullscreen: bool,
        cx: &mut Context<Self>,
    ) {
        if !self.settings.remember_window || is_fullscreen {
            return;
        }
        self.settings.window_bounds = Some(vantage_core::state::WindowBounds {
            x: f32::from(bounds.origin.x).round() as i32,
            y: f32::from(bounds.origin.y).round() as i32,
            width: f32::from(bounds.size.width).round().max(0.0) as u32,
            height: f32::from(bounds.size.height).round().max(0.0) as u32,
        });
        self.save_settings(cx);
    }

    pub fn copy_diagnostics(&mut self, cx: &mut Context<Self>) {
        let Some(diagnostics) = self.diagnostics.clone() else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(diagnostics.text()));
        self.show_toast("Diagnostics copied", cx);
    }

    /// Open the folder that holds `state.json` and its backups.
    pub fn open_state_data_folder(&mut self, cx: &mut Context<Self>) {
        let path = self.state_data_dir.clone();
        if path.as_os_str().is_empty() {
            self.show_toast("The data folder is unavailable", cx);
            return;
        }
        match spawn_external(vantage_core::detect::reveal_in_file_manager(&path)) {
            Ok(()) => cx.notify(),
            Err(error) => {
                self.show_error_toast(format!("Could not open the data folder: {error}"), cx)
            }
        }
    }

    /// Reveal one state backup in the platform file manager.
    pub fn reveal_state_backup(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        match spawn_external(vantage_core::detect::reveal_in_file_manager(&path)) {
            Ok(()) => cx.notify(),
            Err(error) => {
                self.show_error_toast(format!("Could not reveal the backup: {error}"), cx)
            }
        }
    }

    pub fn open_url(&mut self, url: &str, cx: &mut Context<Self>) {
        #[cfg(target_os = "macos")]
        let result = std::process::Command::new("open").arg(url).spawn();
        #[cfg(target_os = "windows")]
        let result = std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn();
        #[cfg(all(unix, not(target_os = "macos")))]
        let result = std::process::Command::new("xdg-open").arg(url).spawn();

        if let Err(error) = result {
            self.show_error_toast(format!("Could not open link: {error}"), cx);
        }
    }

    pub fn fetch_all_quiet(&mut self, cx: &mut Context<Self>) {
        let Some(backend) = self.backend.clone() else {
            return;
        };
        let ids: Vec<String> = self.repos.iter().map(|repo| repo.id.clone()).collect();
        if ids.is_empty() {
            return;
        }
        cx.spawn(async move |this, cx| {
            let failures = cx
                .background_executor()
                .spawn(async move { backend.fetch_all(&ids) })
                .await;
            this.update(cx, |app, cx| {
                if !failures.is_empty() {
                    let failures: Vec<String> = failures
                        .into_iter()
                        .map(|(id, error)| {
                            let name = app
                                .repos
                                .iter()
                                .find(|repo| repo.id == id)
                                .map(|repo| repo.name.clone())
                                .unwrap_or(id);
                            format!("{name}: {error}")
                        })
                        .collect();
                    app.show_error_toast(
                        format!("Background fetch failed — {}", failures.join("; ")),
                        cx,
                    );
                }
                app.start_scan(cx);
            })
            .ok();
        })
        .detach();
    }

    pub fn start_auto_fetch(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_secs(60))
                .await;

            let due = this
                .update(cx, |app, _| {
                    let minutes = app.settings.auto_fetch_minutes;
                    if minutes == 0 {
                        return false;
                    }
                    let now = Instant::now();
                    let due = app
                        .last_auto_fetch
                        .map(|last| {
                            now.duration_since(last) >= Duration::from_secs(minutes as u64 * 60)
                        })
                        .unwrap_or(true);
                    if due {
                        app.last_auto_fetch = Some(now);
                    }
                    due
                })
                .unwrap_or(false);

            if due {
                let _ = this.update(cx, |app, cx| app.fetch_all_quiet(cx));
            }
        })
        .detach();
    }

    fn maybe_fetch_on_launch(&mut self, cx: &mut Context<Self>) {
        if self.did_initial_fetch {
            return;
        }
        self.did_initial_fetch = true;
        if self.settings.fetch_on_launch {
            self.fetch_all_quiet(cx);
        }
    }

    #[cfg(not(target_os = "macos"))]
    pub fn show_about(&mut self, cx: &mut Context<Self>) {
        self.show_toast(
            "Vantage — every repository, every branch, one dashboard",
            cx,
        );
    }

    /// Fetch the repository currently open in the inspector.
    pub fn fetch_selected(&mut self, cx: &mut Context<Self>) {
        match self.selected_repo().cloned() {
            Some(repo) => self.fetch_repo(&repo, cx),
            None => self.show_toast("Select a repository first", cx),
        }
    }

    /// Pull the repository currently open in the inspector.
    pub fn pull_selected(&mut self, cx: &mut Context<Self>) {
        match self.selected_repo().cloned() {
            Some(repo) => self.pull_repo(&repo, cx),
            None => self.show_toast("Select a repository first", cx),
        }
    }

    /// Push the repository currently open in the inspector.
    pub fn push_selected(&mut self, cx: &mut Context<Self>) {
        match self.selected_repo().cloned() {
            Some(repo) => self.push_repo(&repo, cx),
            None => self.show_toast("Select a repository first", cx),
        }
    }

    pub fn toggle_group_collapsed(&mut self, key: String, cx: &mut Context<Self>) {
        if !self.collapsed_groups.remove(&key) {
            self.collapsed_groups.insert(key);
        }
        cx.notify();
    }

    pub fn create_group(&mut self, name: String, cx: &mut Context<Self>) {
        let trimmed = name.trim().to_string();
        if trimmed.is_empty() {
            return;
        }
        if self
            .groups
            .iter()
            .any(|g| g.to_lowercase() == trimmed.to_lowercase())
        {
            self.show_toast(format!("Group \"{trimmed}\" already exists"), cx);
            return;
        }
        self.groups.push(trimmed.clone());
        self.active_group = trimmed.clone();
        if self.persist(cx) {
            self.show_toast(format!("Created group \"{trimmed}\""), cx);
        }
    }

    pub fn delete_group(&mut self, name: String, cx: &mut Context<Self>) {
        self.groups.retain(|g| g != &name);
        if self.repos.iter().any(|r| r.workspace == name)
            && !self.groups.iter().any(|g| g == "Ungrouped")
        {
            self.groups.push("Ungrouped".to_string());
        }
        for repo in &mut self.repos {
            if repo.workspace == name {
                repo.workspace = "Ungrouped".to_string();
            }
        }
        if self.active_group == name {
            self.active_group = "All".to_string();
        }
        if self.persist(cx) {
            self.show_toast(format!("Removed group \"{name}\""), cx);
        }
    }

    pub fn move_repo_group(&mut self, repo_id: &str, group: String, cx: &mut Context<Self>) {
        add_group_if_missing(&mut self.groups, &group);
        let mut moved = None;
        for repo in &mut self.repos {
            if repo.id == repo_id {
                repo.workspace = group.clone();
                moved = Some(repo.name.clone());
            }
        }
        if let Some(name) = moved {
            if self.persist(cx) {
                self.show_toast(format!("Moved {name} to {group}"), cx);
            }
        }
    }

    pub fn toggle_file_stage(&mut self, repo_id: &str, path: &str, cx: &mut Context<Self>) {
        let Some(backend) = self.backend.clone() else {
            return;
        };
        let Some(index) = self.repos.iter().position(|repo| repo.id == repo_id) else {
            return;
        };
        let Some(target) = self.repos[index]
            .uncommitted_files
            .iter()
            .find(|file| file.path == path)
            .map(|file| !file.staged)
        else {
            return;
        };
        let root = expand_home(&self.repos[index].local_path);

        // Apply the tick before asking git, so a checkbox never waits on a
        // process. The rescan below confirms (or corrects) it.
        if let Some(file) = self.repos[index]
            .uncommitted_files
            .iter_mut()
            .find(|file| file.path == path)
        {
            file.staged = target;
        }
        cx.notify();

        self.set_selected(repo_id, path, target);

        let id = repo_id.to_string();
        let file_path = path.to_string();
        let toast_path = file_path.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { backend.set_staged(&id, &file_path, target) })
                .await;
            this.update(cx, |app, cx| {
                // A failed write is the only case that needs a re-read: the tick
                // is otherwise the user's answer and survives later scans.
                if let Err(error) = result {
                    app.show_error_toast(format!("Could not update {toast_path}: {error}"), cx);
                    app.start_scan_dirty(vec![root], cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Stage or unstage every changed file of one repository.
    ///
    /// The checkbox is "all staged"; clicking a partially staged list stages
    /// the rest, so one control both fills and clears the index.
    pub fn toggle_all_staged(&mut self, repo_id: &str, cx: &mut Context<Self>) {
        let Some(backend) = self.backend.clone() else {
            return;
        };
        let Some(index) = self.repos.iter().position(|repo| repo.id == repo_id) else {
            return;
        };
        let total = self.repos[index].uncommitted_files.len();
        if total == 0 {
            return;
        }
        let target = !self.repos[index]
            .uncommitted_files
            .iter()
            .all(|file| file.staged);
        let root = expand_home(&self.repos[index].local_path);

        let paths: Vec<String> = self.repos[index]
            .uncommitted_files
            .iter()
            .map(|file| file.path.clone())
            .collect();
        for file in self.repos[index].uncommitted_files.iter_mut() {
            file.staged = target;
        }
        for path in &paths {
            self.set_selected(repo_id, path, target);
        }
        cx.notify();

        let id = repo_id.to_string();
        let verb = if target { "stage" } else { "unstage" };
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { backend.set_all_staged(&id, target) })
                .await;
            this.update(cx, |app, cx| {
                if let Err(error) = result {
                    app.show_error_toast(format!("Could not {verb} {total} files: {error}"), cx);
                    app.start_scan_dirty(vec![root], cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn switch_branch(&mut self, repo_id: &str, branch_name: &str, cx: &mut Context<Self>) {
        let Some(backend) = self.backend.clone() else {
            return;
        };
        let name = self
            .repos
            .iter()
            .find(|repo| repo.id == repo_id)
            .map(|repo| repo.name.clone())
            .unwrap_or_else(|| repo_id.to_string());

        let root = self
            .repos
            .iter()
            .find(|repo| repo.id == repo_id)
            .map(|repo| expand_home(&repo.local_path));

        let id = repo_id.to_string();
        let branch = branch_name.to_string();
        let checkout_branch = branch.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { backend.checkout(&id, &checkout_branch) })
                .await;
            this.update(cx, |app, cx| {
                match result {
                    Ok(()) => app.show_toast(format!("Checked out {branch}"), cx),
                    Err(error) => {
                        app.show_error_toast(format!("Checkout failed for {name}: {error}"), cx)
                    }
                }
                match root {
                    Some(root) => app.start_scan_dirty(vec![root], cx),
                    None => app.start_scan(cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Phase 2: actually fetch, then rescan the tree.
    pub fn fetch_repo(&mut self, repo: &Repository, cx: &mut Context<Self>) {
        let name = repo.name.clone();
        self.run_repo_op(
            repo,
            cx,
            RepoOp::Fetch,
            RepoOpReport::new(
                format!("Fetched origin for {name}"),
                format!("Fetch failed for {name}"),
            ),
            |backend, repo_id| backend.fetch(repo_id),
        );
    }

    /// `git pull` the branch this repository has checked out, reconciled with
    /// the strategy chosen in Settings → Git.
    pub fn pull_repo(&mut self, repo: &Repository, cx: &mut Context<Self>) {
        let name = repo.name.clone();
        let strategy = self.settings.pull_strategy;
        self.run_repo_op(
            repo,
            cx,
            RepoOp::Pull,
            RepoOpReport::new(
                format!("Pulled {} into {name}", repo.current_branch),
                format!("Pull failed for {name}"),
            ),
            move |backend, repo_id| backend.pull(repo_id, strategy),
        );
    }

    /// `git push` the branch this repository has checked out. The backend
    /// publishes the branch when it tracks nothing yet.
    pub fn push_repo(&mut self, repo: &Repository, cx: &mut Context<Self>) {
        if let Some(reason) = repo.push_blocked_reason() {
            self.show_toast(reason, cx);
            return;
        }
        let name = repo.name.clone();
        let branch = repo.current_branch.clone();
        let publishing = !repo.has_upstream;
        let ok_message = if publishing {
            format!("Published {branch} to origin from {name}")
        } else {
            format!("Pushed {branch} to origin from {name}")
        };
        self.run_repo_op(
            repo,
            cx,
            RepoOp::Push,
            RepoOpReport::new(ok_message, format!("Push failed for {name}")),
            |backend, repo_id| backend.push(repo_id),
        );
    }

    /// Commit the staged files of the focused repository with the message in
    /// the composer.
    ///
    /// Only staged paths are committed, so ticking a file stays the explicit
    /// decision it is in any other git client.
    pub fn commit_selected(&mut self, cx: &mut Context<Self>) {
        // The composer only exists on the Changes tab of an open pane; committing
        // from anywhere else would be an invisible action.
        if !self.settings.right_panel_open || self.inspector_tab != InspectorTab::Changes {
            self.show_toast("Open a repository's Changes tab to commit", cx);
            return;
        }
        let Some(repo) = self.selected_repo().cloned() else {
            self.show_toast("Select a repository first", cx);
            return;
        };
        // Read this repository's own composer. The message/body captured here
        // are what the job submits, even if the selection moves on meanwhile.
        let composer = self.composer().clone();
        let message = composer.message.value.trim().to_string();
        let body = composer.body.value.trim().to_string();
        if message.is_empty() {
            self.show_toast("Write a commit message first", cx);
            return;
        }
        let paths = self.ticked_paths(&repo);
        // An amend may rewrite only the message, so a clean tree is valid
        // there. A normal commit still needs at least one selected path.
        if paths.is_empty() && commit_needs_selected_paths(composer.amending) {
            self.show_toast("Stage the files you want to commit", cx);
            return;
        }
        let target = composer.commit_target(&repo);

        if composer.amending {
            let name = repo.name.clone();
            let record_repo = repo.id.clone();
            let record_summary = message.clone();
            let previous_tip = repo.recent_commits.first().map(|commit| commit.sha.clone());
            let paths_for_op = paths.clone();
            let target_for_op = target;
            let body_for_op = body.clone();
            self.run_repo_op(
                &repo,
                cx,
                RepoOp::Amend,
                RepoOpReport::new(
                    format!("Amended the last commit in {name}"),
                    format!("Could not amend the last commit in {name}"),
                )
                .on_success(move |app, short: String| {
                    // Spend only the draft this operation belongs to; another
                    // repository's composer stays untouched.
                    app.composers.clear_for(&record_repo);
                    app.selection.remove(&record_repo);
                    app.last_commit = Some(LastCommit {
                        repo_id: record_repo,
                        short,
                        summary: record_summary,
                        previous_tip,
                    });
                }),
                move |backend, id| {
                    let head = backend.head_sha(id)?;
                    if let Some(refusal) = target_for_op.refusal(id, head.as_deref()) {
                        return Err(anyhow::anyhow!(refusal));
                    }
                    backend.amend(id, &paths_for_op, &message, Some(body_for_op.as_str()))
                },
            );
            return;
        }

        let count = paths.len();
        let name = repo.name.clone();
        let ok_message = if count == 1 {
            format!("Committed 1 file to {name}")
        } else {
            format!("Committed {count} files to {name}")
        };
        let record_repo = repo.id.clone();
        let record_summary = message.clone();
        let previous_tip = repo.recent_commits.first().map(|commit| commit.sha.clone());
        let target_for_op = target;
        self.run_repo_op(
            &repo,
            cx,
            RepoOp::Commit,
            RepoOpReport::new(ok_message, format!("Commit failed for {name}"))
                // The message is only spent once git accepted it, so a failed
                // commit leaves the composer intact for a retry.
                .on_success(move |app, short: String| {
                    app.composers.clear_for(&record_repo);
                    // The commit spent these ticks; the rescan reports the real
                    // index state from here on.
                    app.selection.remove(&record_repo);
                    app.last_commit = Some(LastCommit {
                        repo_id: record_repo,
                        short,
                        summary: record_summary,
                        previous_tip,
                    });
                }),
            move |backend, repo_id| {
                // Refuse a job whose repository or HEAD moved after the button
                // was pressed rather than writing to a different target.
                let head = backend.head_sha(repo_id)?;
                if let Some(refusal) = target_for_op.refusal(repo_id, head.as_deref()) {
                    return Err(anyhow::anyhow!(refusal));
                }
                let body = (!body.is_empty()).then_some(body.as_str());
                backend.commit(repo_id, &paths, &message, body)
            },
        );
    }

    /// Destinations for the sidebar, with counts from the current rows.
    pub fn nav_items(&self) -> [crate::model::NavItem; 3] {
        crate::model::nav_items(&self.repos, self.active_section)
    }

    /// Counts for the summary strip above the repository list.
    pub fn repo_summary(&self) -> RepoSummary {
        let mut summary = RepoSummary {
            total: self.repos.len(),
            ..RepoSummary::default()
        };
        for repo in &self.repos {
            if !repo.uncommitted_files.is_empty() {
                summary.uncommitted += 1;
            }
            if repo.ahead_count > 0 {
                summary.unpushed += 1;
            }
            if repo_is_clean(repo) {
                summary.clean += 1;
            }
        }
        summary
    }

    /// Show only the slice a summary item stands for; clicking it again (or a
    /// different one) changes or clears the filter.
    pub fn toggle_summary_filter(&mut self, filter: SummaryFilter, cx: &mut Context<Self>) {
        self.summary_filter = match self.summary_filter {
            Some(current) if current == filter => None,
            _ if filter == SummaryFilter::All => None,
            _ => Some(filter),
        };
        cx.notify();
    }

    /// Clear every list filter at once.
    pub fn clear_summary_filter(&mut self, cx: &mut Context<Self>) {
        self.summary_filter = None;
        cx.notify();
    }

    /// Open the context menu for a changed file at the pointer.
    pub fn open_file_menu(
        &mut self,
        repo_id: &str,
        path: &str,
        x: f32,
        y: f32,
        cx: &mut Context<Self>,
    ) {
        self.context_menu = Some(ContextMenu::File {
            repo_id: repo_id.to_string(),
            path: path.to_string(),
            x,
            y,
        });
        cx.notify();
    }

    /// Open the context menu for a commit at the pointer.
    #[allow(clippy::too_many_arguments)]
    pub fn open_commit_menu(
        &mut self,
        repo_id: &str,
        short: &str,
        subject: &str,
        body: &str,
        is_tip: bool,
        x: f32,
        y: f32,
        cx: &mut Context<Self>,
    ) {
        self.context_menu = Some(ContextMenu::Commit {
            repo_id: repo_id.to_string(),
            short: short.to_string(),
            subject: subject.to_string(),
            body: body.to_string(),
            is_tip,
            x,
            y,
        });
        cx.notify();
    }

    pub fn close_file_menu(&mut self, cx: &mut Context<Self>) {
        self.context_menu = None;
        cx.notify();
    }

    /// Run the action a context-menu row carries.
    ///
    /// Every action names itself, so a menu row is never matched by its English
    /// label. There is deliberately no catch-all arm: a new [`ContextAction`]
    /// fails to compile until it is handled here.
    fn run_context_action(&mut self, action: &ContextAction, cx: &mut Context<Self>) {
        match action {
            ContextAction::IgnoreFile { repo_id, path } => {
                self.ignore_file(repo_id, path, cx);
            }
            ContextAction::DiscardFile { repo_id, path } => {
                self.discard_file(repo_id, path, cx);
            }
            ContextAction::StashAll { repo_id } => self.stash_repo(repo_id, cx),
            ContextAction::RestoreStash { repo_id } => self.pop_stash(repo_id, cx),
            ContextAction::DiscardRepo { repo_id } => self.ask_discard_repo(repo_id, cx),
            ContextAction::UpdateFromDefault { repo_id } => {
                self.ask_update_from_default(repo_id, cx);
            }
            ContextAction::ManageStashes { repo_id } => self.open_stash_modal(repo_id, cx),
            ContextAction::ViewOnGitHub { repo_id } => self.open_repo_page(repo_id, cx),
            ContextAction::RemoveFromList { repo_id } => self.ask_forget_repo(repo_id, cx),
            ContextAction::RevealInFinder { repo_id } => self.reveal_repo(repo_id, cx),
            ContextAction::OpenInTerminal { repo_id } => self.open_repo_terminal(repo_id, cx),
            ContextAction::OpenInEditor { repo_id } => self.open_repo_editor(repo_id, cx),
            ContextAction::CheckoutBranch { repo_id, branch } => {
                if self.selected_repo_id.as_deref() == Some(repo_id.as_str()) {
                    self.close_file_menu(cx);
                    self.switch_branch(repo_id, branch, cx);
                } else {
                    self.close_file_menu(cx);
                }
            }
            ContextAction::MergeBranch { repo_id, branch } => {
                self.ask_merge_branch(repo_id, branch, cx);
            }
            ContextAction::CompareWithCurrent { repo_id, branch } => {
                self.compare_with_branch(repo_id, branch, cx);
            }
            ContextAction::NewBranchFrom { repo_id, base } => {
                // The action belongs to the repository the menu came from, not
                // whatever is selected now.
                if self.selected_repo_id.as_deref() == Some(repo_id.as_str()) {
                    self.start_branch_prompt(
                        repo_id,
                        BranchPromptKind::Create { base: base.clone() },
                        cx,
                    );
                } else {
                    self.close_file_menu(cx);
                }
            }
            ContextAction::RenameCurrentBranch { repo_id, from } => {
                if self.selected_repo_id.as_deref() == Some(repo_id.as_str()) {
                    self.start_branch_prompt(
                        repo_id,
                        BranchPromptKind::Rename { from: from.clone() },
                        cx,
                    );
                } else {
                    self.close_file_menu(cx);
                }
            }
            ContextAction::DeleteBranch { repo_id, name } => {
                if self.selected_repo_id.as_deref() == Some(repo_id.as_str()) {
                    self.start_branch_prompt(
                        repo_id,
                        BranchPromptKind::Delete { name: name.clone() },
                        cx,
                    );
                } else {
                    self.close_file_menu(cx);
                }
            }
            ContextAction::AmendCommit {
                repo_id,
                subject,
                body,
            } => {
                // Only the repo the menu came from can be amended, even if the
                // selection moved on.
                if self.selected_repo_id.as_deref() == Some(repo_id.as_str()) {
                    self.begin_amend(subject, body, cx);
                } else {
                    self.close_file_menu(cx);
                }
            }
            ContextAction::RevertCommit { repo_id, sha } => {
                self.revert_commit(repo_id, sha, cx);
            }
            ContextAction::ResetBranchHere { repo_id, sha } => {
                self.ask_reset_commit(repo_id, sha, cx);
            }
        }
    }

    /// Sidebar width in points while it is expanded.
    pub fn sidebar_width(&self) -> f32 {
        if self.sidebar_collapsed {
            56.0
        } else {
            self.settings.sidebar_width
        }
    }

    /// Pane width to render with: a commit diff borrows space from the list.
    pub fn panel_fraction(&self) -> f32 {
        if self.commit_wide && self.commit_view.is_some() {
            0.8
        } else {
            self.settings.right_panel_fraction
        }
    }

    /// Start a new asynchronous load and return the identity its result must
    /// carry back. The sequence makes two loads for the same target distinct,
    /// so an older response cannot satisfy a newer request.
    fn begin_load(&mut self, repo_id: String, kind: LoadKind) -> LoadRequest {
        self.load_seq = self.load_seq.wrapping_add(1);
        LoadRequest {
            generation: self.load_seq,
            repo_id,
            kind,
        }
    }

    /// Pane geometry for the current window, including the compact-mode
    /// thresholds the list and inspector headers use.
    pub fn dashboard_layout(&self, window: &Window) -> crate::layout::DashboardLayout {
        crate::layout::dashboard_layout(
            f32::from(window.viewport_size().width),
            self.sidebar_width(),
            self.sidebar_collapsed,
            self.settings.right_panel_open,
            self.panel_fraction(),
        )
    }

    /// Open one commit: the header shows immediately, files stream in from git.
    pub fn open_commit_view(
        &mut self,
        repo: &Repository,
        commit: &crate::model::CommitEntry,
        cx: &mut Context<Self>,
    ) {
        self.context_menu = None;
        let request = self.begin_load(
            repo.id.clone(),
            LoadKind::Commit {
                sha: commit.sha.clone(),
            },
        );
        self.commit_view = Some(CommitView::commit(request.clone(), repo, commit));

        let Some(backend) = self.backend.clone() else {
            apply_commit_result(
                self.commit_view.as_mut(),
                &request,
                Err("vantage-core engine unavailable".to_string()),
            );
            cx.notify();
            return;
        };
        let repo_id = request.repo_id.clone();
        let sha = commit.sha.clone();
        cx.spawn(async move |this, cx| {
            let files = cx
                .background_executor()
                .spawn(async move { backend.commit_files(&repo_id, &sha) })
                .await;
            let files = files.map_err(|error| format!("{error:#}"));
            this.update(cx, |app, cx| {
                if apply_commit_result(app.commit_view.as_mut(), &request, files) {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Close the commit or comparison view. Any load still running for it is
    /// invalidated with the view, so a late result cannot reopen it.
    pub fn close_commit_view(&mut self, cx: &mut Context<Self>) {
        self.commit_view = None;
        self.commit_wide = false;
        cx.notify();
    }

    pub fn toggle_commit_wide(&mut self, cx: &mut Context<Self>) {
        self.commit_wide = !self.commit_wide;
        cx.notify();
    }

    pub fn select_commit_file(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(view) = self.commit_view.as_mut() {
            view.selected = index;
        }
        cx.notify();
    }

    /// Open the actions for a repository row.
    #[allow(clippy::too_many_arguments)]
    pub fn open_repo_menu(
        &mut self,
        repo_id: &str,
        name: &str,
        uncommitted: usize,
        stashes: usize,
        with_changes: bool,
        x: f32,
        y: f32,
        cx: &mut Context<Self>,
    ) {
        self.context_menu = Some(ContextMenu::Repo {
            repo_id: repo_id.to_string(),
            name: name.to_string(),
            uncommitted,
            stashes,
            with_changes,
            x,
            y,
        });
        cx.notify();
    }

    /// Put every uncommitted change aside.
    pub fn stash_repo(&mut self, repo_id: &str, cx: &mut Context<Self>) {
        self.context_menu = None;
        let Some(repo) = self.repos.iter().find(|repo| repo.id == repo_id).cloned() else {
            return;
        };
        self.run_repo_op(
            &repo,
            cx,
            RepoOp::Stash,
            RepoOpReport::new(
                "Stashed the working tree".to_string(),
                "Could not stash".to_string(),
            ),
            move |backend, id| backend.stash_all(id, Some("Vantage: stashed changes")),
        );
    }

    /// Bring back the newest stash.
    pub fn pop_stash(&mut self, repo_id: &str, cx: &mut Context<Self>) {
        self.context_menu = None;
        let Some(repo) = self.repos.iter().find(|repo| repo.id == repo_id).cloned() else {
            return;
        };
        self.run_repo_op(
            &repo,
            cx,
            RepoOp::Stash,
            RepoOpReport::new(
                "Restored the newest stash".to_string(),
                "Could not restore the stash".to_string(),
            ),
            move |backend, id| backend.stash_pop(id, 0),
        );
    }

    /// Ask before throwing away every change in a repository.
    pub fn ask_discard_repo(&mut self, repo_id: &str, cx: &mut Context<Self>) {
        self.context_menu = None;
        let Some(repo) = self.repos.iter().find(|repo| repo.id == repo_id).cloned() else {
            return;
        };
        let paths: Vec<String> = repo
            .uncommitted_files
            .iter()
            .map(|file| file.path.clone())
            .collect();
        if paths.is_empty() {
            return;
        }
        self.confirm = Some(ConfirmPrompt {
            title: format!("Discard {} changes in {}?", paths.len(), repo.name),
            body: "Every uncommitted edit in this repository is thrown away. A backup of the \
                   change is kept in the repository's refs, but the files go back to the last \
                   commit."
                .to_string(),
            label: "Discard all",
            action: ConfirmAction::DiscardRepository {
                repo_id: repo_id.to_string(),
                paths,
            },
        });
        cx.notify();
    }

    pub fn cancel_confirm(&mut self, cx: &mut Context<Self>) {
        self.confirm = None;
        self.restore_focus = true;
        cx.notify();
    }

    /// Run whichever destructive action was confirmed.
    pub fn submit_confirm(&mut self, cx: &mut Context<Self>) {
        let Some(prompt) = self.confirm.take() else {
            return;
        };
        self.restore_focus = true;
        match prompt.action {
            ConfirmAction::MergeBranch { repo_id, target } => {
                let Some(repo) = self.repos.iter().find(|repo| repo.id == repo_id).cloned() else {
                    return;
                };
                let target_for_op = target.clone();
                self.run_repo_op(
                    &repo,
                    cx,
                    RepoOp::Merge,
                    RepoOpReport::new(
                        format!("Merged {target}"),
                        format!("Could not merge {target}"),
                    ),
                    move |backend, id| backend.merge(id, &target_for_op).map(|_| ()),
                );
            }
            ConfirmAction::UpdateFromDefault { repo_id } => {
                let Some(repo) = self.repos.iter().find(|repo| repo.id == repo_id).cloned() else {
                    return;
                };
                self.run_repo_op(
                    &repo,
                    cx,
                    RepoOp::Merge,
                    RepoOpReport::new(
                        "Updated from the default branch".to_string(),
                        "Could not update from the default branch".to_string(),
                    ),
                    move |backend, id| backend.update_from_default(id).map(|_| ()),
                );
            }
            ConfirmAction::ResetToCommit { repo_id, sha } => {
                let Some(repo) = self.repos.iter().find(|repo| repo.id == repo_id).cloned() else {
                    return;
                };
                self.run_repo_op(
                    &repo,
                    cx,
                    RepoOp::Merge,
                    RepoOpReport::new(
                        format!("Reset to {sha}"),
                        format!("Could not reset to {sha}"),
                    ),
                    move |backend, id| backend.reset_to(id, &sha),
                );
            }
            ConfirmAction::ForgetRepository { repo_id } => {
                let Some(backend) = self.backend.clone() else {
                    return;
                };
                match backend.forget(&repo_id) {
                    Ok(()) => {
                        self.repos.retain(|repo| repo.id != repo_id);
                        self.composers.forget(&repo_id);
                        if self.selected_repo_id.as_deref() == Some(repo_id.as_str()) {
                            self.selected_repo_id = None;
                            self.settings.right_panel_open = false;
                        }
                        let _ = &backend;
                        self.show_toast("Removed from the list", cx);
                        self.start_scan(cx);
                    }
                    Err(error) => {
                        self.show_error_toast(format!("Could not remove it: {error}"), cx)
                    }
                }
            }
            ConfirmAction::DiscardRepository { repo_id, paths } => {
                let Some(repo) = self.repos.iter().find(|repo| repo.id == repo_id).cloned() else {
                    return;
                };
                self.selection.remove(&repo_id);
                self.run_repo_op(
                    &repo,
                    cx,
                    RepoOp::Discard,
                    RepoOpReport::new(
                        format!("Discarded {} changes", paths.len()),
                        "Could not discard the changes".to_string(),
                    ),
                    move |backend, id| backend.discard_paths(id, &paths),
                );
            }
        }
        cx.notify();
    }

    /// Remove a repository from the list, keeping its folder.
    pub fn ask_forget_repo(&mut self, repo_id: &str, cx: &mut Context<Self>) {
        self.context_menu = None;
        let Some(repo) = self.repos.iter().find(|repo| repo.id == repo_id).cloned() else {
            return;
        };
        self.confirm = Some(ConfirmPrompt {
            title: format!("Remove {} from the list?", repo.name),
            body: "The folder and its Git history stay exactly as they are; Vantage just \
                   stops watching it. Add it again any time."
                .to_string(),
            label: "Remove",
            action: ConfirmAction::ForgetRepository {
                repo_id: repo_id.to_string(),
            },
        });
        cx.notify();
    }

    /// Open the stash list for a repository. The dialog opens immediately in a
    /// loading state, so closing it before git answers cannot bring it back.
    pub fn open_stash_modal(&mut self, repo_id: &str, cx: &mut Context<Self>) {
        self.context_menu = None;
        let request = self.begin_load(repo_id.to_string(), LoadKind::Stashes);
        self.stash_modal = Some(StashModal::loading(request.clone(), repo_id));
        let Some(backend) = self.backend.clone() else {
            apply_stash_result(
                self.stash_modal.as_mut(),
                &request,
                Err("vantage-core engine unavailable".to_string()),
            );
            cx.notify();
            return;
        };
        let load_id = request.repo_id.clone();
        cx.spawn(async move |this, cx| {
            let entries = cx
                .background_executor()
                .spawn(async move { backend.stashes(&load_id) })
                .await;
            let entries = entries.map_err(|error| format!("{error:#}"));
            this.update(cx, |app, cx| {
                if apply_stash_result(app.stash_modal.as_mut(), &request, entries) {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub fn close_stash_modal(&mut self, cx: &mut Context<Self>) {
        self.stash_modal = None;
        self.restore_focus = true;
        cx.notify();
    }

    /// Apply a stash and drop it, then rescan.
    pub fn pop_stash_entry(&mut self, index: u32, cx: &mut Context<Self>) {
        let Some(modal) = self.stash_modal.clone() else {
            return;
        };
        self.stash_modal = None;
        self.restore_focus = true;
        let Some(repo) = self
            .repos
            .iter()
            .find(|repo| repo.id == modal.repo_id)
            .cloned()
        else {
            return;
        };
        self.run_repo_op(
            &repo,
            cx,
            RepoOp::Stash,
            RepoOpReport::new(
                format!("Restored stash@{index}"),
                format!("Could not restore stash@{index}"),
            ),
            move |backend, id| backend.stash_pop(id, index),
        );
    }

    /// Throw a stash away without applying it.
    pub fn drop_stash_entry(&mut self, index: u32, cx: &mut Context<Self>) {
        let Some(modal) = self.stash_modal.clone() else {
            return;
        };
        self.stash_modal = None;
        self.restore_focus = true;
        let Some(repo) = self
            .repos
            .iter()
            .find(|repo| repo.id == modal.repo_id)
            .cloned()
        else {
            return;
        };
        self.run_repo_op(
            &repo,
            cx,
            RepoOp::Stash,
            RepoOpReport::new(
                format!("Dropped stash@{index}"),
                format!("Could not drop stash@{index}"),
            ),
            move |backend, id| backend.stash_drop(id, index),
        );
    }

    /// Ignore one file by adding its path to `.gitignore`.
    ///
    /// Git only honours ignore rules for untracked paths, so a tracked file gets
    /// a straight answer instead of a rule that would do nothing.
    pub fn ignore_file(&mut self, repo_id: &str, path: &str, cx: &mut Context<Self>) {
        self.context_menu = None;
        let Some(repo) = self.repos.iter().find(|repo| repo.id == repo_id).cloned() else {
            return;
        };
        let status = repo
            .uncommitted_files
            .iter()
            .find(|file| file.path == path)
            .map(|file| file.status);
        if !matches!(status, Some(crate::model::FileChangeStatus::Untracked)) {
            self.show_toast(
                "Ignoring only affects untracked files — this one is tracked",
                cx,
            );
            return;
        }
        let Some(backend) = self.backend.clone() else {
            return;
        };
        let id = repo_id.to_string();
        let file_path = path.to_string();
        let toast_path = file_path.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { backend.ignore_path(&id, &file_path) })
                .await;
            this.update(cx, |app, cx| {
                match result {
                    Ok(()) => app.show_toast(format!("Ignored {toast_path}"), cx),
                    Err(error) => {
                        app.show_error_toast(format!("Could not write .gitignore: {error}"), cx)
                    }
                }
                app.start_scan(cx);
            })
            .ok();
        })
        .detach();
    }

    /// Ask before moving the branch tip back to a commit.
    pub fn ask_reset_commit(&mut self, repo_id: &str, sha: &str, cx: &mut Context<Self>) {
        self.context_menu = None;
        let Some(repo) = self.repos.iter().find(|repo| repo.id == repo_id).cloned() else {
            return;
        };
        if let Some(refusal) = reset_refusal(&repo) {
            self.show_toast(refusal, cx);
            return;
        }
        self.confirm = Some(ConfirmPrompt {
            title: format!("Reset {} to {sha}?", repo.current_branch),
            body: reset_confirmation_body(sha),
            label: "Reset",
            action: ConfirmAction::ResetToCommit {
                repo_id: repo_id.to_string(),
                sha: sha.to_string(),
            },
        });
        cx.notify();
    }

    /// Open the diff between the current branch and another one.
    pub fn compare_with_branch(&mut self, repo_id: &str, other: &str, cx: &mut Context<Self>) {
        self.context_menu = None;
        let Some(repo) = self.repos.iter().find(|repo| repo.id == repo_id).cloned() else {
            return;
        };
        let base = repo.current_branch.clone();
        let request = self.begin_load(
            repo.id.clone(),
            LoadKind::Compare {
                base: base.clone(),
                other: other.to_string(),
            },
        );
        self.inspector_tab = InspectorTab::Commits;
        self.commit_view = Some(CommitView::comparison(request.clone(), &repo, &base, other));

        let Some(backend) = self.backend.clone() else {
            apply_commit_result(
                self.commit_view.as_mut(),
                &request,
                Err("vantage-core engine unavailable".to_string()),
            );
            cx.notify();
            return;
        };
        let id = repo.id.clone();
        let other = other.to_string();
        cx.spawn(async move |this, cx| {
            let files = cx
                .background_executor()
                .spawn(async move { backend.compare(&id, &base, &other) })
                .await;
            let files = files.map_err(|error| format!("{error:#}"));
            this.update(cx, |app, cx| {
                if apply_commit_result(app.commit_view.as_mut(), &request, files) {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Ask before merging another branch into the current one.
    pub fn ask_merge_branch(&mut self, repo_id: &str, target: &str, cx: &mut Context<Self>) {
        self.context_menu = None;
        let Some(repo) = self.repos.iter().find(|repo| repo.id == repo_id).cloned() else {
            return;
        };
        self.confirm = Some(ConfirmPrompt {
            title: format!("Merge {target} into {}?", repo.current_branch),
            body: "Git will merge and commit automatically. If the branches conflict, the merge \
                   stops and the files are left for you to resolve."
                .to_string(),
            label: "Merge",
            action: ConfirmAction::MergeBranch {
                repo_id: repo_id.to_string(),
                target: target.to_string(),
            },
        });
        cx.notify();
    }

    /// Ask before pulling the default branch into the current one.
    pub fn ask_update_from_default(&mut self, repo_id: &str, cx: &mut Context<Self>) {
        self.context_menu = None;
        let Some(repo) = self.repos.iter().find(|repo| repo.id == repo_id).cloned() else {
            return;
        };
        self.confirm = Some(ConfirmPrompt {
            title: format!("Update {} from the default branch?", repo.current_branch),
            body: "Vantage fetches, then merges the remote default branch into the branch you \
                   are on."
                .to_string(),
            label: "Update",
            action: ConfirmAction::UpdateFromDefault {
                repo_id: repo_id.to_string(),
            },
        });
        cx.notify();
    }

    /// Open the clone dialog, with a sensible parent folder already filled in.
    pub fn open_clone_modal(&mut self, cx: &mut Context<Self>) {
        // Reopening while a clone runs must not clear the URL or detach the
        // dialog from the process it is showing progress for.
        if self.is_cloning {
            self.is_clone_modal_open = true;
            cx.notify();
            return;
        }
        self.context_menu = None;
        self.clone_url.clear();
        self.clone_progress = None;
        self.clone_failure = None;
        self.clone_cancel = None;
        if self.clone_parent.value.trim().is_empty() {
            let parent = dirs::home_dir()
                .map(|home| home.join("Projects"))
                .filter(|path| path.is_dir())
                .or_else(dirs::home_dir)
                .map(|path| crate::path::contract(&path))
                .unwrap_or_default();
            self.clone_parent.set(parent);
        }
        self.is_clone_modal_open = true;
        cx.notify();
    }

    pub fn close_clone_modal(&mut self, cx: &mut Context<Self>) {
        // The backdrop and Cancel both mean "stop" while git is running; the
        // dialog stays open until the child has actually been reaped.
        if self.is_cloning {
            self.cancel_clone(cx);
            return;
        }
        self.is_clone_modal_open = false;
        self.is_cloning = false;
        self.restore_focus = true;
        self.clone_progress = None;
        self.clone_failure = None;
        self.clone_cancel = None;
        cx.notify();
    }

    /// Stop the running clone. The modal stays open and the backend reports
    /// `Cancelled` once the child process is gone.
    pub fn cancel_clone(&mut self, cx: &mut Context<Self>) {
        if !self.is_cloning {
            return;
        }
        if let Some(cancel) = &self.clone_cancel {
            cancel.cancel();
            self.clone_progress = Some("Cancelling…".to_string());
            cx.notify();
        }
    }

    pub fn open_new_repo_modal(&mut self, cx: &mut Context<Self>) {
        self.context_menu = None;
        self.new_repo_name.clear();
        if self.new_repo_parent.value.trim().is_empty() {
            let parent = dirs::home_dir()
                .map(|home| home.join("Projects"))
                .filter(|path| path.is_dir())
                .or_else(dirs::home_dir)
                .map(|path| crate::path::contract(&path))
                .unwrap_or_default();
            self.new_repo_parent.set(parent);
        }
        self.is_new_repo_modal_open = true;
        cx.notify();
    }

    pub fn close_new_repo_modal(&mut self, cx: &mut Context<Self>) {
        self.is_new_repo_modal_open = false;
        self.is_creating_repo = false;
        self.restore_focus = true;
        cx.notify();
    }

    /// Run `git init` in a new folder and add it to the list.
    pub fn submit_new_repo(&mut self, cx: &mut Context<Self>) {
        if self.is_creating_repo {
            return;
        }
        let name = self.new_repo_name.value.trim().to_string();
        let parent = self.new_repo_parent.value.trim().to_string();
        if name.is_empty() {
            self.show_toast("Name the repository first", cx);
            return;
        }
        if parent.is_empty() {
            self.show_toast("Choose where to create it", cx);
            return;
        }
        let Some(backend) = self.backend.clone() else {
            return;
        };
        let parent_path = expand_home(&parent);
        let with_gitignore = self.new_repo_gitignore;
        self.is_creating_repo = true;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    backend.create_repo(&parent_path.to_string_lossy(), &name, with_gitignore)
                })
                .await;
            this.update(cx, |app, cx| {
                app.is_creating_repo = false;
                match result {
                    Ok(()) => {
                        app.is_new_repo_modal_open = false;
                        app.new_repo_name.clear();
                        app.restore_focus = true;
                        app.show_toast("Repository created", cx);
                        app.start_scan(cx);
                    }
                    Err(error) => app.show_error_toast(format!("Could not create it: {error}"), cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Clone the URL into the chosen folder, then rescan the workspace.
    pub fn submit_clone(&mut self, cx: &mut Context<Self>) {
        if self.is_cloning {
            return;
        }
        let url = self.clone_url.value.trim().to_string();
        let parent = self.clone_parent.value.trim().to_string();
        if url.is_empty() {
            self.show_toast("Paste a repository URL first", cx);
            return;
        }
        if parent.is_empty() {
            self.show_toast("Choose the folder to clone into", cx);
            return;
        }
        let Some(backend) = self.backend.clone() else {
            return;
        };
        let parent_path = expand_home(&parent);
        let cancel = vantage_core::CancelToken::new();
        self.is_cloning = true;
        self.clone_progress = None;
        self.clone_failure = None;
        self.clone_cancel = Some(cancel.clone());
        cx.notify();

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<CloneMessage>();
        let progress_tx = tx.clone();
        let clone_backend = backend.clone();
        let clone_cancel = cancel.clone();
        let parent = parent_path.to_string_lossy().to_string();
        cx.background_executor()
            .spawn(async move {
                let result =
                    clone_backend.clone_repo(&url, &parent, None, &clone_cancel, move |line| {
                        let _ = progress_tx.send(CloneMessage::Progress(line));
                    });
                let _ = tx.send(CloneMessage::Done(result));
            })
            .detach();

        cx.spawn(async move |this, cx| {
            let mut finished = false;
            while let Some(message) = rx.recv().await {
                match message {
                    CloneMessage::Progress(line) => {
                        this.update(cx, |app, cx| {
                            if app.is_cloning {
                                app.clone_progress = Some(line);
                                cx.notify();
                            }
                        })
                        .ok();
                    }
                    CloneMessage::Done(result) => {
                        this.update(cx, |app, cx| app.finish_clone(result, cx)).ok();
                        finished = true;
                        break;
                    }
                }
            }
            if !finished {
                // The background task panicked or the channel closed without a
                // result; leave the modal usable instead of stuck on "Cloning…".
                this.update(cx, |app, cx| {
                    app.finish_clone(
                        Err(anyhow::anyhow!("Clone worker stopped unexpectedly")),
                        cx,
                    )
                })
                .ok();
            }
        })
        .detach();
    }

    fn finish_clone(&mut self, result: anyhow::Result<()>, cx: &mut Context<Self>) {
        self.is_cloning = false;
        self.clone_cancel = None;
        match result {
            Ok(()) => {
                self.is_clone_modal_open = false;
                self.clone_url.clear();
                self.clone_progress = None;
                self.clone_failure = None;
                self.show_toast("Cloned and added to the list", cx);
                self.start_scan(cx);
            }
            Err(error) => {
                self.clone_progress = None;
                self.clone_failure = Some(clone_failure(&error));
                cx.notify();
            }
        }
    }

    /// Create a commit that undoes `sha`.
    pub fn revert_commit(&mut self, repo_id: &str, sha: &str, cx: &mut Context<Self>) {
        self.context_menu = None;
        let Some(repo) = self.repos.iter().find(|repo| repo.id == repo_id).cloned() else {
            return;
        };
        let sha = sha.to_string();
        self.run_repo_op(
            &repo,
            cx,
            RepoOp::Revert,
            RepoOpReport::new(format!("Reverted {sha}"), format!("Could not revert {sha}")),
            move |backend, id| backend.revert(id, &sha).map(|_| ()),
        );
    }

    /// Open the repository's page in the browser.
    pub fn open_repo_page(&mut self, repo_id: &str, cx: &mut Context<Self>) {
        self.context_menu = None;
        let Some(backend) = self.backend.clone() else {
            return;
        };
        let id = repo_id.to_string();
        cx.spawn(async move |this, cx| {
            let page = cx
                .background_executor()
                .spawn(async move { backend.origin_page(&id) })
                .await;
            this.update(cx, |app, cx| match page {
                Ok(Some(url)) => app.open_url(&url, cx),
                Ok(None) => app.show_toast("This repository has no origin", cx),
                Err(error) => {
                    app.show_error_toast(format!("Could not read the remote: {error}"), cx)
                }
            })
            .ok();
        })
        .detach();
    }

    /// Show the repository in the platform's file manager.
    pub fn reveal_repo(&mut self, repo_id: &str, cx: &mut Context<Self>) {
        self.context_menu = None;
        let Some(repo) = self.repos.iter().find(|repo| repo.id == repo_id).cloned() else {
            return;
        };
        let path = expand_home(&repo.local_path);
        match spawn_external(vantage_core::detect::reveal_in_file_manager(&path)) {
            Ok(()) => cx.notify(),
            Err(error) => {
                self.show_error_toast(format!("Could not reveal the folder: {error}"), cx)
            }
        }
    }

    /// Open the repository in the platform's terminal.
    pub fn open_repo_terminal(&mut self, repo_id: &str, cx: &mut Context<Self>) {
        self.context_menu = None;
        let Some(repo) = self.repos.iter().find(|repo| repo.id == repo_id).cloned() else {
            return;
        };
        let path = expand_home(&repo.local_path);
        match spawn_external(vantage_core::detect::open_terminal_at(&path)) {
            Ok(()) => cx.notify(),
            Err(error) => self.show_error_toast(format!("Could not open a terminal: {error}"), cx),
        }
    }

    /// Open the repository in the editor the user configured, or the one the
    /// environment names.
    pub fn open_repo_editor(&mut self, repo_id: &str, cx: &mut Context<Self>) {
        self.context_menu = None;
        let Some(repo) = self.repos.iter().find(|repo| repo.id == repo_id).cloned() else {
            return;
        };
        let path = expand_home(&repo.local_path);
        let preference = self.settings.editor_command.clone();
        match spawn_external(vantage_core::detect::open_editor_at(
            &path,
            Some(&preference),
        )) {
            Ok(()) => cx.notify(),
            Err(error) => self.show_error_toast(format!("Could not open the editor: {error}"), cx),
        }
    }

    /// Move the highlighted repository up or down, without opening the pane.
    pub fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        let list = self.filtered_repos();
        if list.is_empty() {
            return;
        }
        let current = self
            .selected_repo_id
            .as_deref()
            .and_then(|id| list.iter().position(|repo| repo.id == id));
        let next = match (current, delta) {
            (Some(index), 0) => index,
            (Some(index), d) => (index as isize + d).clamp(0, list.len() as isize - 1) as usize,
            (None, d) if d < 0 => 0,
            (None, _) => 0,
        };
        let id = list[next].id.clone();
        drop(list);
        self.composers.switch_to(&id);
        if self.selected_repo_id.as_deref() != Some(id.as_str()) {
            self.expanded_file = None;
        }
        self.selected_repo_id = Some(id);
        if let Some(repo) = self.selected_repo().cloned() {
            self.ensure_details_for(&repo, cx);
        }
        cx.notify();
    }

    /// Open the highlighted repository's detail pane.
    pub fn open_selected(&mut self, cx: &mut Context<Self>) {
        if let Some(repo) = self.selected_repo().cloned() {
            self.settings.right_panel_open = true;
            self.save_settings(cx);
            self.ensure_details_for(&repo, cx);
            self.ensure_avatars(&repo, cx);
            cx.notify();
        }
    }

    /// Open the branch actions menu.
    #[allow(clippy::too_many_arguments)]
    pub fn open_branch_menu(
        &mut self,
        repo_id: &str,
        name: &str,
        is_current: bool,
        is_default: bool,
        x: f32,
        y: f32,
        cx: &mut Context<Self>,
    ) {
        self.context_menu = Some(ContextMenu::Branch {
            repo_id: repo_id.to_string(),
            name: name.to_string(),
            is_current,
            is_default,
            x,
            y,
        });
        cx.notify();
    }

    /// Start a branch action for `repo_id`, pre-filling its name field where it
    /// makes sense.
    ///
    /// The repository is captured here, not read from the selection at submit
    /// time, so a prompt can only ever write to the repository that opened it.
    pub fn start_branch_prompt(
        &mut self,
        repo_id: &str,
        kind: BranchPromptKind,
        cx: &mut Context<Self>,
    ) {
        self.context_menu = None;
        match &kind {
            BranchPromptKind::Create { .. }
            | BranchPromptKind::Delete { .. }
            | BranchPromptKind::ForceDelete { .. } => self.branch_field.clear(),
            BranchPromptKind::Rename { from } => self.branch_field.set(from.clone()),
        }
        self.branch_prompt = Some(BranchPrompt {
            repo_id: repo_id.to_string(),
            kind,
        });
        cx.notify();
    }

    /// Run a row action against the repository that rendered it.
    pub fn perform_branch_action(&mut self, action: BranchRowAction, cx: &mut Context<Self>) {
        self.context_menu = None;
        let repo_id = action.repo_id().to_string();
        if !self.repos.iter().any(|repo| repo.id == repo_id) {
            self.show_toast("That repository is no longer in the list", cx);
            return;
        }
        if let Some(prompt) = action.prompt() {
            self.start_branch_prompt(&repo_id, prompt, cx);
            return;
        }
        match action {
            BranchRowAction::Checkout { branch, .. } => self.switch_branch(&repo_id, &branch, cx),
            BranchRowAction::Merge { branch, .. } => self.ask_merge_branch(&repo_id, &branch, cx),
            BranchRowAction::Compare { branch, .. } => {
                self.compare_with_branch(&repo_id, &branch, cx)
            }
            // Prompt-backed variants returned above.
            BranchRowAction::Create { .. }
            | BranchRowAction::Rename { .. }
            | BranchRowAction::Delete { .. } => {}
        }
    }

    pub fn cancel_branch_prompt(&mut self, cx: &mut Context<Self>) {
        self.branch_prompt = None;
        self.branch_field.clear();
        self.restore_focus = true;
        cx.notify();
    }

    /// Enter in the name field (or the row's Save button) runs the action
    /// against the repository the prompt captured when it opened.
    pub fn submit_branch_prompt(&mut self, cx: &mut Context<Self>) {
        let Some(prompt) = self.branch_prompt.clone() else {
            return;
        };
        let Some(repo) = self
            .repos
            .iter()
            .find(|repo| repo.id == prompt.repo_id)
            .cloned()
        else {
            // The prompt outlived its repository. Refuse rather than acting on
            // whichever repository happens to be selected now.
            self.branch_prompt = None;
            self.branch_field.clear();
            self.show_toast(
                "The repository for this branch action is no longer in the workspace. Nothing was changed.",
                cx,
            );
            return;
        };
        let typed = self.branch_field.value.trim().to_string();

        if let BranchPromptKind::Delete { name } = &prompt.kind {
            self.run_branch_delete(&repo, name.clone(), false, cx);
            return;
        }
        if let BranchPromptKind::ForceDelete { name } = &prompt.kind {
            self.run_branch_delete(&repo, name.clone(), true, cx);
            return;
        }

        let (ok, fail, op): (String, String, BranchAction) = match &prompt.kind {
            BranchPromptKind::Create { base } => {
                if typed.is_empty() {
                    self.show_toast("Type a branch name first", cx);
                    return;
                }
                let base = base.clone();
                let check = prompt.kind.clone();
                (
                    format!("Created {typed} from {base}"),
                    format!("Could not create {typed}"),
                    Box::new(move |backend, id| {
                        validate_branch_prompt(&check, &backend.local_branches(id)?)
                            .map_err(anyhow::Error::msg)?;
                        backend.create_branch(id, &typed, &base)
                    }),
                )
            }
            BranchPromptKind::Rename { from } => {
                if typed.is_empty() {
                    self.show_toast("Type a branch name first", cx);
                    return;
                }
                let from = from.clone();
                let check = prompt.kind.clone();
                (
                    format!("Renamed {from} to {typed}"),
                    format!("Could not rename {from}"),
                    Box::new(move |backend, id| {
                        validate_branch_prompt(&check, &backend.local_branches(id)?)
                            .map_err(anyhow::Error::msg)?;
                        backend.rename_branch(id, &from, &typed)
                    }),
                )
            }
            BranchPromptKind::Delete { .. } | BranchPromptKind::ForceDelete { .. } => return,
        };

        self.branch_prompt = None;
        self.branch_field.clear();
        self.restore_focus = true;
        self.run_repo_op(&repo, cx, RepoOp::Branch, RepoOpReport::new(ok, fail), op);
    }

    /// Delete a branch, trying the normal `-d` first. When git refuses because
    /// the branch has unmerged work the dialog stays open as an explicit
    /// force-delete confirmation; nothing is force-deleted without that click.
    fn run_branch_delete(
        &mut self,
        repo: &Repository,
        name: String,
        force: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(backend) = self.backend.clone() else {
            return;
        };
        if self
            .busy_repos
            .insert(repo.id.clone(), RepoOp::Branch)
            .is_some()
        {
            return;
        }
        cx.notify();

        let id = repo.id.clone();
        let label = name.clone();
        cx.spawn(async move |this, cx| {
            let task_id = id.clone();
            let result = cx
                .background_executor()
                .spawn(async move { backend.delete_branch(&task_id, &name, force) })
                .await;
            this.update(cx, |app, cx| {
                app.busy_repos.remove(&id);
                match result {
                    Ok(BranchDeleteOutcome::Deleted) => {
                        app.branch_prompt = None;
                        app.branch_field.clear();
                        app.show_toast(format!("Deleted {label}"), cx);
                    }
                    Ok(BranchDeleteOutcome::NotMerged) => {
                        app.branch_prompt = Some(BranchPrompt {
                            repo_id: id.clone(),
                            kind: BranchPromptKind::ForceDelete {
                                name: label.clone(),
                            },
                        });
                        app.branch_field.clear();
                    }
                    Err(error) => {
                        app.branch_prompt = None;
                        app.branch_field.clear();
                        app.show_toast(format!("Could not delete {label}: {error}"), cx);
                    }
                }
                app.start_scan(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The stash list, with restore and drop per entry.
    fn render_stash_modal(&self, window: &mut Window, cx: &mut Context<Self>) -> gpui::Div {
        let theme = self.theme;
        let Some(modal) = self.stash_modal.clone() else {
            return div();
        };
        if !self.modal_focus.contains_focused(window, cx) {
            window.focus(&self.modal_focus);
        }

        let mut list = div().flex().flex_col().gap(px(2.0)).w_full();
        if modal.loading {
            list = list.child(
                div()
                    .py(px(10.0))
                    .xs()
                    .fg(theme.label_muted)
                    .child("Loading stashes…"),
            );
        } else if let Some(error) = &modal.error {
            list = list.child(
                div()
                    .py(px(10.0))
                    .xs()
                    .fg(theme.status_red)
                    .child(format!("Could not load stashes: {error}")),
            );
        } else if modal.entries.is_empty() {
            list = list.child(
                div()
                    .py(px(10.0))
                    .xs()
                    .fg(theme.label_muted)
                    .child("No stashes in this repository."),
            );
        }
        for entry in &modal.entries {
            let index = entry.index;
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(10.0))
                    .w_full()
                    .px(px(8.0))
                    .py(px(6.0))
                    .rounded(px(6.0))
                    .hover(move |s| s.bg(theme.row_hover_bg))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w(px(0.0))
                            .child(
                                div()
                                    .xs()
                                    .medium()
                                    .fg(theme.text_app)
                                    .truncate()
                                    .child(entry.subject.clone()),
                            )
                            .child(
                                div()
                                    .mono_xs()
                                    .fg(theme.text_meta)
                                    .child(format!("stash@{{{index}}} \u{00b7} {}", entry.date)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .flex_shrink_0()
                            .child(
                                div()
                                    .id(gpui::SharedString::from(format!("stash-pop-{index}")))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .h(px(crate::ui::HIT_TARGET_MIN))
                                    .px(px(8.0))
                                    .rounded(px(4.0))
                                    .cursor_pointer()
                                    .xs()
                                    .medium()
                                    .fg(theme.accent_text)
                                    .hover(move |s| s.bg(theme.nav_hover_bg))
                                    .child("Restore")
                                    .on_click(cx.listener(move |this, _e, _w, cx| {
                                        this.pop_stash_entry(index, cx);
                                    })),
                            )
                            .child(
                                div()
                                    .id(gpui::SharedString::from(format!("stash-drop-{index}")))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .h(px(crate::ui::HIT_TARGET_MIN))
                                    .px(px(8.0))
                                    .rounded(px(4.0))
                                    .cursor_pointer()
                                    .xs()
                                    .fg(theme.text_meta)
                                    .hover(move |s| s.fg(theme.status_red).underline())
                                    .child("Drop")
                                    .on_click(cx.listener(move |this, _e, _w, cx| {
                                        this.drop_stash_entry(index, cx);
                                    })),
                            ),
                    ),
            );
        }

        let card = div()
            .w(px(460.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .p(px(20.0))
            .bg(theme.panel)
            .border_1()
            .border_color(theme.divider)
            .rounded(px(12.0))
            .shadow_lg()
            .track_focus(&self.modal_focus)
            .key_context(MODAL_KEY_CONTEXT)
            .on_key_down(cx.listener(Self::handle_modal_key))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_this, _e, _w, cx| cx.stop_propagation()),
            )
            .child(div().sm().medium().fg(theme.text_app).child("Stashes"))
            .child(list)
            .child(
                div().flex().items_center().justify_end().child(
                    crate::ui::secondary_button(&theme, "Close", None)
                        .id("stash-close")
                        .on_click(cx.listener(|this, _e, _w, cx| this.close_stash_modal(cx))),
                ),
            );

        div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme.overlay)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _e, _w, cx| this.close_stash_modal(cx)),
            )
            .child(card)
    }

    /// Dialog for starting a repository: name, folder, and a starter ignore file.
    fn render_new_repo_modal(&self, window: &mut Window, cx: &mut Context<Self>) -> gpui::Div {
        let theme = self.theme;
        if !self.modal_focus.contains_focused(window, cx) {
            window.focus(&self.new_repo_name_focus);
        }

        let label = if self.is_creating_repo {
            "Creating…"
        } else {
            "Create repository"
        };
        let mut confirm = crate::ui::accent_button(&theme, label)
            .h(px(crate::ui::HIT_TARGET_PRIMARY))
            .id("new-repo-confirm")
            .on_click(cx.listener(|this, _e, _w, cx| this.submit_new_repo(cx)));
        if self.is_creating_repo {
            confirm = confirm.opacity(0.6);
        }

        let ignore = self.new_repo_gitignore;
        let toggle = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                crate::ui::checkbox(&theme, ignore)
                    .id("new-repo-gitignore")
                    .debug_selector(|| "new-repo-gitignore-target".to_string())
                    .on_click(cx.listener(|this, _e, _w, cx| {
                        this.new_repo_gitignore = !this.new_repo_gitignore;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .xs()
                    .fg(theme.text_body)
                    .child("Add a starter .gitignore"),
            );

        let card = div()
            .w(px(460.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .p(px(20.0))
            .bg(theme.panel)
            .border_1()
            .border_color(theme.divider)
            .rounded(px(12.0))
            .shadow_lg()
            .track_focus(&self.modal_focus)
            .key_context(MODAL_KEY_CONTEXT)
            .on_key_down(cx.listener(Self::handle_modal_key))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_this, _e, _w, cx| cx.stop_propagation()),
            )
            .child(
                div()
                    .sm()
                    .medium()
                    .fg(theme.text_app)
                    .child("New repository"),
            )
            .child(div().xs().fg(theme.label_muted).child(
                "A folder is created inside the destination, initialised as a Git repository and \
                 added to the list.",
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .child(crate::ui::field_label(&theme, "Repository name"))
                    .child(self.text_field(
                        FieldId::NewRepoName,
                        "Repository name",
                        crate::ui::FieldVariant::Form,
                        window,
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .child(crate::ui::field_label(&theme, "Destination folder"))
                    .child(self.text_field(
                        FieldId::NewRepoParent,
                        "Destination folder",
                        crate::ui::FieldVariant::Form,
                        window,
                        cx,
                    )),
            )
            .child(toggle)
            .child(crate::ui::keyboard_hint(
                &theme,
                "Enter creates · Esc cancels",
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        crate::ui::secondary_button(&theme, "Cancel", None)
                            .h(px(crate::ui::HIT_TARGET_PRIMARY))
                            .id("new-repo-cancel")
                            .on_click(
                                cx.listener(|this, _e, _w, cx| this.close_new_repo_modal(cx)),
                            ),
                    )
                    .child(confirm),
            );

        div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme.overlay)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _e, _w, cx| this.close_new_repo_modal(cx)),
            )
            .child(card)
    }

    /// Dialog for cloning a repository: where from, and into which folder.
    fn render_clone_modal(&self, window: &mut Window, cx: &mut Context<Self>) -> gpui::Div {
        let theme = self.theme;
        let cancelling = self
            .clone_cancel
            .as_ref()
            .is_some_and(vantage_core::CancelToken::is_cancelled);
        if !self.modal_focus.contains_focused(window, cx) {
            window.focus(&self.clone_url_focus);
        }

        let label = if cancelling {
            "Cancelling…"
        } else if self.is_cloning {
            "Cloning…"
        } else {
            "Clone"
        };
        let mut confirm = crate::ui::accent_button(&theme, label)
            .h(px(crate::ui::HIT_TARGET_PRIMARY))
            .id("clone-confirm")
            .on_click(cx.listener(|this, _e, _w, cx| this.submit_clone(cx)));
        if self.is_cloning {
            confirm = confirm.opacity(0.6);
        }

        let progress = self.clone_progress.as_deref().unwrap_or("Starting clone…");
        let cancel_label = if cancelling {
            "Cancelling…"
        } else if self.is_cloning {
            "Cancel clone"
        } else {
            "Cancel"
        };
        let cancel = crate::ui::secondary_button(&theme, cancel_label, None)
            .id("clone-cancel")
            .debug_selector(|| "clone-cancel".to_string())
            .when(cancelling, |button| button.opacity(0.6))
            .on_click(cx.listener(|this, _e, _w, cx| {
                if this.is_cloning {
                    this.cancel_clone(cx);
                } else {
                    this.close_clone_modal(cx);
                }
            }));

        let card = div()
            .w(px(460.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .p(px(20.0))
            .bg(theme.panel)
            .border_1()
            .border_color(theme.divider)
            .rounded(px(12.0))
            .shadow_lg()
            .debug_selector(|| "clone-card".to_string())
            .track_focus(&self.modal_focus)
            .key_context(MODAL_KEY_CONTEXT)
            .on_key_down(cx.listener(Self::handle_modal_key))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_this, _e, _w, cx| cx.stop_propagation()),
            )
            .child(
                div()
                    .sm()
                    .medium()
                    .fg(theme.text_app)
                    .child("Clone a repository"),
            )
            .child(div().xs().fg(theme.label_muted).child(
                "A URL, or a GitHub shorthand like owner/repo. The folder is created inside the \
                 destination.",
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .child(crate::ui::field_label(&theme, "Repository URL"))
                    .child(self.text_field(
                        FieldId::CloneUrl,
                        "https://github.com/owner/repo.git",
                        crate::ui::FieldVariant::Form,
                        window,
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .child(crate::ui::field_label(&theme, "Destination folder"))
                    .child(self.text_field(
                        FieldId::CloneParent,
                        "Destination folder",
                        crate::ui::FieldVariant::Form,
                        window,
                        cx,
                    )),
            )
            .child(crate::ui::keyboard_hint(
                &theme,
                "Enter clones · Esc cancels",
            ))
            .when_some(self.clone_failure.as_ref(), |card, failure| {
                let (background, border, text) = match failure {
                    CloneFailure::Cancelled => (theme.subtle_bg, theme.divider, theme.text_body),
                    CloneFailure::Failed(_) => {
                        (theme.error_bg, theme.error_border, theme.error_text)
                    }
                };
                card.child(
                    div()
                        .debug_selector(|| "clone-error".to_string())
                        .px(px(12.0))
                        .py(px(8.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(border)
                        .bg(background)
                        .xs()
                        .fg(text)
                        .child(failure.message().to_string()),
                )
            })
            .when(self.is_cloning, |card| {
                card.child(
                    div()
                        .debug_selector(|| "clone-progress".to_string())
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .px(px(12.0))
                        .py(px(8.0))
                        .rounded(px(8.0))
                        .bg(theme.subtle_bg)
                        .child(crate::ui::spinner(
                            "clone",
                            "refresh-cw",
                            crate::ui::IconSize::Xs,
                            theme.accent,
                        ))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .xs()
                                .fg(theme.text_body)
                                .truncate()
                                .child(progress.to_string()),
                        ),
                )
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(8.0))
                    .child(cancel)
                    .child(confirm),
            );

        div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme.overlay)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _e, _w, cx| this.close_clone_modal(cx)),
            )
            .child(card)
    }

    /// Small dialog for a destructive action: what will happen, and one way out.
    fn render_confirm_modal(&self, window: &mut Window, cx: &mut Context<Self>) -> gpui::Div {
        let theme = self.theme;
        let Some(prompt) = self.confirm.clone() else {
            return div();
        };
        if !self.modal_focus.contains_focused(window, cx) {
            window.focus(&self.modal_focus);
        }

        let card = div()
            .w(px(420.0))
            .flex()
            .flex_col()
            .gap(px(14.0))
            .p(px(20.0))
            .bg(theme.panel)
            .border_1()
            .border_color(theme.divider)
            .rounded(px(12.0))
            .shadow_lg()
            .track_focus(&self.modal_focus)
            .key_context(MODAL_KEY_CONTEXT)
            .on_key_down(cx.listener(Self::handle_modal_key))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_this, _e, _w, cx| cx.stop_propagation()),
            )
            .child(div().sm().medium().fg(theme.text_app).child(prompt.title))
            .child(div().xs().fg(theme.text_body).child(prompt.body))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        crate::ui::secondary_button(&theme, "Cancel", None)
                            .h(px(crate::ui::HIT_TARGET_PRIMARY))
                            .id("confirm-cancel")
                            .on_click(cx.listener(|this, _e, _w, cx| this.cancel_confirm(cx))),
                    )
                    .child(
                        crate::ui::destructive_button(&theme, prompt.label)
                            .h(px(crate::ui::HIT_TARGET_PRIMARY))
                            .id("confirm-ok")
                            .on_click(cx.listener(|this, _e, _w, cx| this.submit_confirm(cx))),
                    ),
            );

        div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme.overlay)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _e, _w, cx| this.cancel_confirm(cx)),
            )
            .child(card)
    }

    /// Small dialog for a branch action: a name to type, or a delete to confirm.
    fn render_branch_modal(&self, window: &mut Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme;
        let Some(prompt) = self.branch_prompt.clone() else {
            return div().into_any_element();
        };

        // The dialog owns the keyboard while it is open. A delete prompt has no
        // field, so the card itself takes focus and Escape still works.
        if !self.modal_focus.contains_focused(window, cx) {
            if matches!(
                self.branch_prompt.as_ref().map(|prompt| &prompt.kind),
                Some(BranchPromptKind::Create { .. } | BranchPromptKind::Rename { .. })
            ) {
                window.focus(&self.branch_focus);
            } else {
                window.focus(&self.modal_focus);
            }
        }

        let (title, action_label, destructive) = match &prompt.kind {
            BranchPromptKind::Create { base } => {
                (format!("New branch from {base}"), "Create branch", false)
            }
            BranchPromptKind::Rename { from } => (format!("Rename {from}"), "Rename", false),
            BranchPromptKind::Delete { name } => (format!("Delete {name}?"), "Delete", true),
            BranchPromptKind::ForceDelete { name } => {
                (format!("Force delete {name}?"), "Force delete", true)
            }
        };
        let repo_label = self
            .repos
            .iter()
            .find(|repo| repo.id == prompt.repo_id)
            .map(|repo| repo.name.clone())
            .unwrap_or_else(|| prompt.repo_id.clone());

        let mut body = div().flex().flex_col().gap(px(12.0)).w_full();
        body = body.child(
            div()
                .xs()
                .fg(theme.label_muted)
                .child(format!("Repository: {repo_label}")),
        );
        match &prompt.kind {
            BranchPromptKind::Delete { name } => {
                body = body.child(div().xs().fg(theme.text_body).child(format!(
                    "{name} is removed only when every commit is merged into the current \
                     branch or its upstream. If Git refuses because it has unmerged work, \
                     you can confirm a force delete next."
                )))
            }
            BranchPromptKind::ForceDelete { name } => {
                body = body.child(div().xs().fg(theme.text_body).child(format!(
                    "{name} has commits that are not merged into the current branch or its \
                     upstream, so Git refused the normal delete. Force-deleting removes the \
                     branch pointer, and commits reachable only from it may be lost. This \
                     cannot be undone."
                )))
            }
            _ => {
                body = body
                    .child(crate::ui::field_label(&theme, "Branch name"))
                    .child(self.text_field(
                        FieldId::BranchName,
                        "Branch name",
                        crate::ui::FieldVariant::Form,
                        window,
                        cx,
                    ))
                    .child(crate::ui::keyboard_hint(
                        &theme,
                        "Enter confirms · Esc cancels",
                    ))
            }
        }

        let confirm_button = if destructive {
            crate::ui::destructive_button(&theme, action_label)
        } else {
            crate::ui::accent_button(&theme, action_label)
        };
        let confirm = confirm_button
            .h(px(crate::ui::HIT_TARGET_PRIMARY))
            .id("branch-modal-confirm")
            .on_click(cx.listener(|this, _e, _w, cx| this.submit_branch_prompt(cx)));

        let card = div()
            .w(px(420.0))
            .flex()
            .flex_col()
            .gap(px(14.0))
            .p(px(20.0))
            .bg(theme.panel)
            .border_1()
            .border_color(theme.divider)
            .rounded(px(12.0))
            .shadow_lg()
            .track_focus(&self.modal_focus)
            .key_context(MODAL_KEY_CONTEXT)
            .on_key_down(cx.listener(Self::handle_modal_key))
            // A click inside the dialog is not a click on the backdrop.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_this, _e, _w, cx| cx.stop_propagation()),
            )
            .child(div().sm().medium().fg(theme.text_app).child(title))
            .child(body)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        crate::ui::secondary_button(&theme, "Cancel", None)
                            .h(px(crate::ui::HIT_TARGET_PRIMARY))
                            .id("branch-modal-cancel")
                            .on_click(
                                cx.listener(|this, _e, _w, cx| this.cancel_branch_prompt(cx)),
                            ),
                    )
                    .child(confirm),
            );

        // One absolute layer covers the window: dim it, centre the card in it,
        // and dismiss on any press that the card itself did not take.
        div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme.overlay)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _e, _w, cx| this.cancel_branch_prompt(cx)),
            )
            .child(card)
            .into_any_element()
    }

    /// Rewrite the tip commit instead of making a new one: both message fields
    /// are pre-filled from the commit so an untouched amend keeps its body.
    pub fn begin_amend(&mut self, subject: &str, body: &str, cx: &mut Context<Self>) {
        self.context_menu = None;
        // Remember the tip this amend started from, so a job queued before a
        // newer commit can be refused instead of rewriting the wrong commit.
        let base_tip = self
            .selected_repo()
            .and_then(|repo| repo.recent_commits.first().map(|commit| commit.sha.clone()));
        // The files being amended live in Changes, so go there: the mode is
        // about what the commit will contain, not just its message.
        self.inspector_tab = InspectorTab::Changes;
        let composer = self.composers.active_mut();
        composer.amending = true;
        composer.base_tip = base_tip;
        composer.message.set(subject.to_string());
        composer.body.set(body.to_string());
        cx.notify();
    }

    pub fn cancel_amend(&mut self, cx: &mut Context<Self>) {
        let composer = self.composers.active_mut();
        composer.amending = false;
        composer.base_tip = None;
        composer.message.clear();
        composer.body.clear();
        cx.notify();
    }

    /// Throw away the working-tree changes of one file.
    ///
    /// The engine snapshots the change into a backup ref first, so even this
    /// destructive action can be recovered inside the repository.
    pub fn discard_file(&mut self, repo_id: &str, path: &str, cx: &mut Context<Self>) {
        self.context_menu = None;
        let Some(repo) = self.repos.iter().find(|repo| repo.id == repo_id).cloned() else {
            return;
        };
        let file_path = path.to_string();
        self.run_repo_op(
            &repo,
            cx,
            RepoOp::Discard,
            RepoOpReport::new(
                format!("Discarded changes in {file_path}"),
                format!("Could not discard {file_path}"),
            ),
            move |backend, id| backend.discard(id, &file_path),
        );
    }

    /// The commit a detail pane may offer to undo: still the branch tip of
    /// this repository, and made from this session.
    pub fn undo_candidate(&self, repo: &Repository) -> Option<UndoOffer> {
        undo_offer(self.last_commit.as_ref(), repo)
    }

    /// Soft-reset the last commit so its changes are staged again.
    ///
    /// The engine refuses when the commit is already on the remote, and writes a
    /// backup ref first, so an undo can never silently lose work.
    pub fn undo_last_commit(&mut self, repo: &Repository, cx: &mut Context<Self>) {
        let Some(tip) = repo.recent_commits.first() else {
            return;
        };
        if self.undo_candidate(repo).is_none() {
            return;
        }
        let short = tip.sha.clone();
        self.run_repo_op(
            repo,
            cx,
            RepoOp::Undo,
            RepoOpReport::new(
                format!("Undid {short} — its changes are back in Changes"),
                "Could not undo the commit".to_string(),
            )
            .on_success(|app, ()| app.last_commit = None),
            move |backend, repo_id| backend.undo_commit(repo_id),
        );
    }

    /// Run one git operation off the UI thread, report which action is in
    /// flight for this repository, and re-scan when it ends.
    fn run_repo_op<T: Send + 'static>(
        &mut self,
        repo: &Repository,
        cx: &mut Context<Self>,
        op_kind: RepoOp,
        report: RepoOpReport<T>,
        op: impl FnOnce(&Backend, &str) -> anyhow::Result<T> + Send + 'static,
    ) {
        let Some(backend) = self.backend.clone() else {
            return;
        };
        // One operation per repository at a time: `git` already serialises on
        // the repo lock, so queueing a second one only delays the UI. Check
        // before recording the new kind: a rejected operation must leave the
        // running one alone, because its kind is what the spinner names.
        if self.busy_repos.contains_key(&repo.id) {
            return;
        }
        self.busy_repos.insert(repo.id.clone(), op_kind);
        cx.notify();

        let id = repo.id.clone();
        cx.spawn(async move |this, cx| {
            let task_id = id.clone();
            let result = cx
                .background_executor()
                .spawn(async move { op(&backend, &task_id) })
                .await;
            this.update(cx, |app, cx| {
                app.busy_repos.remove(&id);
                match result {
                    Ok(value) => {
                        if matches!(
                            op_kind,
                            RepoOp::Amend
                                | RepoOp::Branch
                                | RepoOp::Commit
                                | RepoOp::Merge
                                | RepoOp::Pull
                                | RepoOp::Revert
                                | RepoOp::Stash
                                | RepoOp::Undo
                        ) {
                            app.invalidate_details(&id);
                        }
                        if let Some(on_success) = report.on_success {
                            on_success(app, value);
                        }
                        app.show_toast(report.ok_message, cx);
                    }
                    Err(error) => {
                        app.show_error_toast(format!("{}: {error}", report.fail_message), cx)
                    }
                }
                app.start_scan(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Which git operation, if any, is still running for this repository.
    pub fn repo_op(&self, repo_id: &str) -> Option<RepoOp> {
        self.busy_repos.get(repo_id).copied()
    }

    pub fn copy_local_path(&mut self, path: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(path.clone()));
        self.copied_path = true;
        self.show_toast(format!("Copied path {path}"), cx);
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(1800))
                .await;
            this.update(cx, |this, cx| {
                this.copied_path = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn reset_filters(&mut self, cx: &mut Context<Self>) {
        self.active_group = "All".to_string();
        self.active_section = NavSection::Overview;
        cx.notify();
    }

    // ---- add repository --------------------------------------------------

    pub fn submit_add_modal(&mut self, cx: &mut Context<Self>) {
        if self.modal_is_adding {
            return;
        }

        let raw_path = self.modal_path.value.trim();
        if raw_path.is_empty() {
            self.modal_error = Some("Choose the repository folder to add.".to_string());
            cx.notify();
            return;
        }

        let resolved_group = match self.resolve_modal_group() {
            Ok(group) => group,
            Err(message) => {
                self.modal_error = Some(message);
                cx.notify();
                return;
            }
        };

        let Some(backend) = self.backend.clone() else {
            self.modal_error = Some("The Git engine is unavailable.".to_string());
            cx.notify();
            return;
        };

        let path = expand_home(raw_path).to_string_lossy().to_string();
        self.modal_is_adding = true;
        self.modal_error = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { backend.add_repo(&path) })
                .await;

            this.update(cx, |app, cx| {
                app.modal_is_adding = false;
                match result {
                    Ok(mut repository) => {
                        let resolved_group = app.ensure_group(&resolved_group);
                        repository.workspace = resolved_group.clone();
                        app.repos.retain(|repo| repo.id != repository.id);
                        app.repos.insert(0, repository.clone());
                        app.composers.switch_to(&repository.id);
                        app.selected_repo_id = Some(repository.id.clone());
                        app.settings.right_panel_open = true;
                        app.ensure_details_for(&repository, cx);

                        app.modal_path.clear();
                        app.modal_custom_group.clear();
                        app.modal_error = None;
                        app.is_add_modal_open = false;
                        app.modal_group_menu_open = false;
                        app.restore_focus = true;

                        let state_saved = app.persist(cx);
                        let settings_saved = app.save_settings(cx);
                        if state_saved && settings_saved {
                            app.show_toast(
                                format!("Added {} to {}", repository.name, resolved_group),
                                cx,
                            );
                        }
                        app.start_scan(cx);
                    }
                    Err(error) => {
                        app.modal_error = Some(error.to_string());
                        cx.notify();
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    /// The group named by the modal, creating it only once the add succeeds.
    fn resolve_modal_group(&self) -> Result<String, String> {
        if self.modal_group_choice == "__new__" {
            let custom = self.modal_custom_group.value.trim();
            if custom.is_empty() {
                return Err("Enter a name for the new group.".to_string());
            }
            Ok(custom.to_string())
        } else {
            Ok(self.modal_group_choice.clone())
        }
    }

    /// Match a group name case-insensitively, adding it to the sidebar list the
    /// first time it is used. Returns the name as it is stored.
    fn ensure_group(&mut self, name: &str) -> String {
        if let Some(existing) = self
            .groups
            .iter()
            .find(|group| group.to_lowercase() == name.to_lowercase())
            .cloned()
        {
            existing
        } else {
            self.groups.push(name.to_string());
            name.to_string()
        }
    }

    /// Open the Add Repository dialog and immediately ask for a folder to scan.
    /// Used by the File menu entry; the dialog's own button calls
    /// [`VantageApp::pick_scan_folder`] directly.
    pub fn open_scan_folder(&mut self, cx: &mut Context<Self>) {
        if !self.is_add_modal_open {
            self.open_add_modal(cx);
        }
        self.pick_scan_folder(cx);
    }

    /// Native folder picker for the folder scan (the modal's "Scan Folder…"
    /// button). The scan itself runs off the UI thread; the result lands in
    /// `modal_scan` as a checkbox preview.
    pub fn pick_scan_folder(&mut self, cx: &mut Context<Self>) {
        if self.modal_is_adding || self.modal_is_scanning {
            return;
        }
        if self.backend.is_none() {
            self.modal_error = Some("The Git engine is unavailable.".to_string());
            cx.notify();
            return;
        }

        self.modal_scan = None;
        self.modal_error = None;
        self.modal_group_menu_open = false;
        self.modal_is_scanning = true;
        self.modal_scan_seq = self.modal_scan_seq.wrapping_add(1);
        let seq = self.modal_scan_seq;
        cx.notify();

        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Scan a folder for Git repositories".into()),
        });

        cx.spawn(async move |this, cx| {
            let picked = match receiver.await {
                Ok(Ok(paths)) => paths.and_then(|paths| paths.into_iter().next()),
                Ok(Err(error)) => {
                    this.update(cx, |app, cx| {
                        if app.modal_scan_seq == seq {
                            app.modal_is_scanning = false;
                            app.modal_error =
                                Some(format!("Could not open the folder picker: {error}"));
                            cx.notify();
                        }
                    })
                    .ok();
                    return;
                }
                Err(error) => {
                    this.update(cx, |app, cx| {
                        if app.modal_scan_seq == seq {
                            app.modal_is_scanning = false;
                            app.modal_error =
                                Some(format!("Folder picker closed unexpectedly: {error}"));
                            cx.notify();
                        }
                    })
                    .ok();
                    return;
                }
            };

            let Some(path) = picked else {
                this.update(cx, |app, cx| {
                    if app.modal_scan_seq == seq {
                        app.modal_is_scanning = false;
                        cx.notify();
                    }
                })
                .ok();
                return;
            };

            let folder = path.to_string_lossy().to_string();
            let backend = this.update(cx, |app, _| app.backend.clone()).ok().flatten();
            let Some(backend) = backend else {
                this.update(cx, |app, cx| {
                    if app.modal_scan_seq == seq {
                        app.modal_is_scanning = false;
                        app.modal_error = Some("The Git engine is unavailable.".to_string());
                        cx.notify();
                    }
                })
                .ok();
                return;
            };

            let folder_for_field = folder.clone();
            this.update(cx, |app, cx| {
                if app.modal_scan_seq == seq {
                    app.modal_path.set(folder_for_field);
                    cx.notify();
                }
            })
            .ok();

            let scan = cx
                .background_executor()
                .spawn(async move {
                    if !path.is_dir() {
                        return Err(format!("'{}' is no longer a folder.", path.display()));
                    }
                    Ok(backend.scan_repositories(&path))
                })
                .await;

            this.update(cx, |app, cx| {
                if app.modal_scan_seq != seq || !app.is_add_modal_open {
                    return;
                }
                app.modal_is_scanning = false;
                match scan {
                    Ok(scan) => {
                        app.modal_scan = Some(ScanPreview::new(
                            std::path::Path::new(&folder),
                            scan.scanned,
                            scan.repositories,
                        ));
                        app.modal_error = None;
                    }
                    Err(error) => {
                        app.modal_error = Some(error);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Add every ticked repository from the scan preview in one batch.
    pub fn submit_scan_add(&mut self, cx: &mut Context<Self>) {
        if self.modal_is_adding || self.modal_is_scanning {
            return;
        }
        let Some(scan) = self.modal_scan.clone() else {
            return;
        };
        let paths = scan.selected_paths();
        if paths.is_empty() {
            self.modal_error = Some("Select at least one repository to add.".to_string());
            cx.notify();
            return;
        }

        let resolved_group = match self.resolve_modal_group() {
            Ok(group) => group,
            Err(message) => {
                self.modal_error = Some(message);
                cx.notify();
                return;
            }
        };
        let Some(backend) = self.backend.clone() else {
            self.modal_error = Some("The Git engine is unavailable.".to_string());
            cx.notify();
            return;
        };

        self.modal_is_adding = true;
        self.modal_error = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { backend.add_repos(&paths) })
                .await;

            this.update(cx, |app, cx| {
                app.modal_is_adding = false;
                match result {
                    Ok(outcome) => app.finish_scan_add(
                        outcome.repositories,
                        outcome.failures,
                        &resolved_group,
                        cx,
                    ),
                    Err(error) => {
                        app.modal_error = Some(error.to_string());
                        cx.notify();
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    /// Apply a finished batch add: successes join the workspace like a single
    /// add, failures keep the dialog open with only their rows left to retry.
    fn finish_scan_add(
        &mut self,
        repositories: Vec<Repository>,
        failures: Vec<(String, String)>,
        group: &str,
        cx: &mut Context<Self>,
    ) {
        let added = repositories.len();
        let mut resolved_group = group.to_string();

        if added > 0 {
            resolved_group = self.ensure_group(group);
            let first = repositories.first().map(|repository| repository.id.clone());
            for mut repository in repositories.into_iter().rev() {
                repository.workspace = resolved_group.clone();
                self.repos.retain(|repo| repo.id != repository.id);
                self.repos.insert(0, repository);
            }
            self.selected_repo_id = first;
            self.settings.right_panel_open = true;
            self.persist(cx);
            self.save_settings(cx);
            self.start_scan(cx);
        }

        if failures.is_empty() {
            self.reset_modal_scan();
            self.modal_path.clear();
            self.modal_custom_group.clear();
            self.modal_error = None;
            self.is_add_modal_open = false;
            self.modal_group_menu_open = false;
            let message = if added == 1 {
                format!("Added 1 repository to {resolved_group}")
            } else {
                format!("Added {added} repositories to {resolved_group}")
            };
            self.show_toast(message, cx);
            cx.notify();
            return;
        }

        // Keep only the rows git could not add, all ticked, so a retry does
        // not resubmit repositories that already made it into the workspace.
        let failed_paths: HashSet<String> = failures.iter().map(|(path, _)| path.clone()).collect();
        if let Some(scan) = self.modal_scan.as_mut() {
            scan.retain_paths(&failed_paths);
            scan.set_all(true);
        }
        self.modal_error = Some(scan_failure_message(added, &failures));
        cx.notify();
    }

    /// Native folder picker for the repository path (the modal's "Browse" button).
    pub fn browse_for_repo(&mut self, cx: &mut Context<Self>) {
        if self.modal_is_adding {
            return;
        }
        self.modal_group_menu_open = false;
        cx.notify();
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose a Git repository".into()),
        });

        cx.spawn(async move |this, cx| {
            let picked = match receiver.await {
                Ok(Ok(paths)) => paths.and_then(|paths| paths.into_iter().next()),
                Ok(Err(error)) => {
                    this.update(cx, |this, cx| {
                        this.modal_error =
                            Some(format!("Could not open the folder picker: {error}"));
                        cx.notify();
                    })
                    .ok();
                    return;
                }
                Err(error) => {
                    this.update(cx, |this, cx| {
                        this.modal_error =
                            Some(format!("Folder picker closed unexpectedly: {error}"));
                        cx.notify();
                    })
                    .ok();
                    return;
                }
            };

            let Some(path) = picked else {
                return;
            };
            this.update(cx, |this, cx| {
                this.modal_path.set(path.to_string_lossy());
                this.modal_error = None;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Folder name of the selected path when it resolves to a work tree. Used
    /// for the small "ready to add" line under the path field.
    pub fn modal_detected_repo_name(&self) -> Option<String> {
        let raw = self.modal_path.value.trim();
        if raw.is_empty() {
            return None;
        }
        let path = expand_home(raw);
        if !path.is_dir() {
            return None;
        }
        path.ancestors()
            .find(|ancestor| ancestor.join(".git").exists())?
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
    }

    pub fn render_toast(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let toast = self.toast.as_ref()?;
        let theme = self.theme;
        let is_error = toast.kind == ToastKind::Error;
        let detailed = toast.is_detailed();
        let sticky = toast.auto_hide_after().is_none();
        let preview = toast.preview();
        let max_width = toast_max_width(f32::from(window.bounds().size.width));

        let mut card = div()
            .absolute()
            .bottom(px(TOAST_EDGE_MARGIN))
            .right(px(TOAST_EDGE_MARGIN))
            .max_w(px(max_width))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .px(px(14.0))
            .py(px(10.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(if is_error {
                theme.error_border
            } else {
                theme.toast_border
            })
            .bg(theme.toast_bg)
            .fg(theme.toast_text)
            .shadow_lg()
            // Wrap long lines instead of letting them run off the window.
            .whitespace_normal()
            .xs()
            .medium()
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(8.0))
                    .when(is_error, |row| {
                        row.child(crate::ui::icon(
                            "alert-circle",
                            crate::ui::IconSize::Xs,
                            theme.error_border,
                        ))
                    })
                    .child(div().flex_1().min_w(px(0.0)).child(preview)),
            );

        // Success toasts stay visually quiet; errors and toasts that will not
        // clear themselves get the actions needed to read or reuse the text.
        if is_error || sticky {
            card = card.child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(4.0))
                    .child(
                        toast_action(&theme, if self.toast_copied { "Copied" } else { "Copy" })
                            .id("toast-copy")
                            .on_click(cx.listener(|this, _e, _w, cx| this.copy_toast(cx))),
                    )
                    .when(detailed, |row| {
                        row.child(
                            toast_action(&theme, "Details")
                                .id("toast-details")
                                .on_click(
                                    cx.listener(|this, _e, _w, cx| this.open_toast_details(cx)),
                                ),
                        )
                    })
                    .child(
                        toast_action(&theme, "Dismiss")
                            .id("toast-dismiss")
                            .on_click(cx.listener(|this, _e, _w, cx| this.dismiss_toast(cx))),
                    ),
            );
        }

        Some(card.into_any_element())
    }

    /// The full text of a long toast, in a scrollable dialog. The toast itself
    /// only shows a bounded preview so a 4 KB Git error cannot cover the app.
    pub fn render_toast_details(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        if !self.toast_details_open {
            return None;
        }
        let toast = self.toast.as_ref()?;
        let theme = self.theme;
        let title = if toast.kind == ToastKind::Error {
            "Error details"
        } else {
            "Message details"
        };
        let text = toast.copy_text().to_string();

        let card = div()
            .w(px(560.0))
            .max_w(gpui::relative(0.92))
            .max_h(gpui::relative(0.86))
            .flex()
            .flex_col()
            .gap(px(14.0))
            .p(px(20.0))
            .bg(theme.panel)
            .border_1()
            .border_color(theme.divider)
            .rounded(px(12.0))
            .shadow_lg()
            // A press inside the dialog is not a press on the backdrop.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_this, _e, _w, cx| cx.stop_propagation()),
            )
            .child(div().sm().medium().fg(theme.text_app).child(title))
            .child(
                div()
                    .id("toast-details-text")
                    .flex_1()
                    .min_h(px(0.0))
                    .max_h(px(360.0))
                    .overflow_y_scroll()
                    .p(px(10.0))
                    .rounded(px(6.0))
                    .bg(theme.field_bg)
                    .border_1()
                    .border_color(theme.field_border)
                    .mono_xs()
                    .whitespace_normal()
                    .fg(theme.text_body)
                    .child(text),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        crate::ui::secondary_button(
                            &theme,
                            if self.toast_copied { "Copied" } else { "Copy" },
                            Some("copy"),
                        )
                        .id("toast-details-copy")
                        .on_click(cx.listener(|this, _e, _w, cx| this.copy_toast(cx))),
                    )
                    .child(
                        crate::ui::primary_button(&theme, "Close", None)
                            .id("toast-details-close")
                            .on_click(cx.listener(|this, _e, _w, cx| this.close_toast_details(cx))),
                    ),
            );

        Some(
            div()
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(theme.overlay)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _e, _w, cx| this.close_toast_details(cx)),
                )
                .child(card)
                .into_any_element(),
        )
    }

    /// A persistent startup notice for a state file that had to be recovered.
    /// Unlike a toast it stays until dismissed, so the backup location cannot
    /// disappear before the user reads it.
    pub fn render_state_recovery(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        if self.state_warning_dismissed {
            return None;
        }
        let warning = self.state_warning.clone()?;
        let theme = self.theme;
        Some(
            div()
                .debug_selector(|| "state-recovery-banner".to_string())
                .flex()
                .items_start()
                .gap(px(12.0))
                .px(px(18.0))
                .py(px(10.0))
                .flex_shrink_0()
                .border_b_1()
                .border_color(theme.error_border)
                .bg(theme.error_bg)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .flex_1()
                        .min_w(px(0.0))
                        .child(
                            div()
                                .display_xs()
                                .semibold()
                                .fg(theme.error_text)
                                .child("Saved state needed recovery"),
                        )
                        .child(div().xs().fg(theme.text_body).child(warning)),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .flex_shrink_0()
                        .child(
                            crate::ui::secondary_button(
                                &theme,
                                "Open data folder",
                                Some("folder-git-2"),
                            )
                            .id("state-recovery-open-folder")
                            .on_click(
                                cx.listener(|this, _e, _w, cx| this.open_state_data_folder(cx)),
                            ),
                        )
                        .child(
                            crate::ui::secondary_button(&theme, "Review backups", None)
                                .id("state-recovery-review")
                                .on_click(cx.listener(|this, _e, _w, cx| {
                                    this.state_warning_dismissed = true;
                                    this.open_data_settings(cx);
                                })),
                        )
                        .child(
                            crate::ui::icon_button(
                                &theme,
                                "x",
                                "state-recovery-dismiss",
                                "Dismiss state warning",
                            )
                            .on_click(cx.listener(
                                |this, _e, _w, cx| {
                                    this.state_warning_dismissed = true;
                                    cx.notify();
                                },
                            )),
                        ),
                )
                .into_any_element(),
        )
    }

    // ---- text fields -----------------------------------------------------

    /// Whether any dialog currently owns the screen.
    pub fn has_modal_open(&self) -> bool {
        self.is_settings_open
            || self.is_add_modal_open
            || self.is_clone_modal_open
            || self.is_new_repo_modal_open
            || self.stash_modal.is_some()
            || self.confirm.is_some()
            || self.branch_prompt.is_some()
    }

    /// The repository rows are on screen and can take list navigation.
    fn repo_list_visible(&self) -> bool {
        !self.is_refreshing
            && !self.repos.is_empty()
            && matches!(
                self.active_section,
                NavSection::Overview | NavSection::Attention
            )
    }

    /// The commit bar (and therefore its fields) is rendered for this state.
    fn commit_bar_visible(&self) -> bool {
        self.settings.right_panel_open
            && self.inspector_tab == InspectorTab::Changes
            && self
                .selected_repo()
                .is_some_and(|repo| !repo.uncommitted_files.is_empty())
    }

    fn focus_target_for(&self, handle: &FocusHandle) -> Option<FocusTarget> {
        if handle == &self.repo_list_focus {
            return Some(FocusTarget::RepoList);
        }
        if handle == &self.modal_focus {
            return Some(FocusTarget::Modal);
        }
        for id in [
            FieldId::NewGroup,
            FieldId::ModalPath,
            FieldId::ModalCustomGroup,
            FieldId::SettingsBranch,
            FieldId::CommitMessage,
            FieldId::CommitBody,
            FieldId::BranchName,
            FieldId::CloneUrl,
            FieldId::CloneParent,
            FieldId::NewRepoName,
            FieldId::NewRepoParent,
        ] {
            if handle == &self.field_focus(id) {
                return Some(FocusTarget::Field(id));
            }
        }
        None
    }

    fn focus_handle_for(&self, target: FocusTarget) -> FocusHandle {
        match target {
            FocusTarget::RepoList => self.repo_list_focus.clone(),
            FocusTarget::Modal => self.modal_focus.clone(),
            FocusTarget::Field(id) => self.field_focus(id),
        }
    }

    /// Whether a target is part of the tree this frame can render.
    fn focus_target_visible(&self, target: FocusTarget) -> bool {
        match target {
            FocusTarget::RepoList => self.repo_list_visible(),
            FocusTarget::Modal => self.has_modal_open(),
            FocusTarget::Field(id) => match id {
                FieldId::NewGroup => self.is_adding_group,
                FieldId::ModalPath | FieldId::ModalCustomGroup => self.is_add_modal_open,
                FieldId::SettingsBranch | FieldId::SettingsEditor => self.is_settings_open,
                FieldId::CommitMessage | FieldId::CommitBody => self.commit_bar_visible(),
                FieldId::BranchName => matches!(
                    self.branch_prompt.as_ref().map(|prompt| &prompt.kind),
                    Some(BranchPromptKind::Create { .. } | BranchPromptKind::Rename { .. })
                ),
                FieldId::CloneUrl | FieldId::CloneParent => self.is_clone_modal_open,
                FieldId::NewRepoName | FieldId::NewRepoParent => self.is_new_repo_modal_open,
            },
        }
    }

    /// Keep the keyboard on a live control: restore a dialog's previous focus,
    /// service a one-shot request, or fall back to the repository list when
    /// nothing editable owns the keyboard.
    fn sync_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.has_modal_open() {
            return;
        }

        if let Some(target) = self.pending_focus.take() {
            if self.focus_target_visible(target) {
                window.focus(&self.focus_handle_for(target));
            }
        }

        if std::mem::take(&mut self.restore_focus) {
            let preferred = self.last_main_focus.unwrap_or(FocusTarget::RepoList);
            let target = if self.focus_target_visible(preferred) {
                preferred
            } else {
                FocusTarget::RepoList
            };
            if self.focus_target_visible(target) {
                window.focus(&self.focus_handle_for(target));
            }
        }

        match window
            .focused(cx)
            .and_then(|handle| self.focus_target_for(&handle))
        {
            Some(target) if self.focus_target_visible(target) => {
                self.last_main_focus = Some(target);
            }
            _ if self.repo_list_visible() => {
                window.focus(&self.repo_list_focus);
                self.last_main_focus = Some(FocusTarget::RepoList);
            }
            _ => {}
        }
    }

    /// Give the list keyboard focus, e.g. after a row click.
    pub fn focus_repo_list(&self, window: &mut Window) {
        window.focus(&self.repo_list_focus);
    }

    /// The repository list's focus handle, so the list view can make it the
    /// active key context.
    pub fn repo_list_focus(&self) -> FocusHandle {
        self.repo_list_focus.clone()
    }

    /// New Group toggles the inline field and puts the caret in it.
    pub fn toggle_new_group(&mut self, cx: &mut Context<Self>) {
        self.is_adding_group = !self.is_adding_group;
        if self.is_adding_group {
            self.pending_focus = Some(FocusTarget::Field(FieldId::NewGroup));
        } else {
            self.group_field.clear();
        }
        cx.notify();
    }

    /// Escape closes the topmost dialog, whatever control inside it has focus.
    pub fn dismiss_modal(&mut self, cx: &mut Context<Self>) {
        // Topmost first: this mirrors the order dialogs are painted in `render`.
        if self.is_settings_open {
            self.close_settings(cx);
        } else if self.is_add_modal_open {
            self.close_add_modal(cx);
        } else if self.branch_prompt.is_some() {
            self.cancel_branch_prompt(cx);
        } else if self.confirm.is_some() {
            self.cancel_confirm(cx);
        } else if self.is_clone_modal_open {
            self.close_clone_modal(cx);
        } else if self.is_new_repo_modal_open {
            self.close_new_repo_modal(cx);
        } else if self.stash_modal.is_some() {
            self.close_stash_modal(cx);
        }
    }

    /// Tab and Shift+Tab move between the dialog's tab stops only. No field
    /// behind the dialog is a tab stop, so this cannot escape it.
    pub fn focus_modal_tab_stop(&self, forward: bool, window: &mut Window) {
        if !self.has_modal_open() {
            return;
        }
        if forward {
            window.focus_next();
        } else {
            window.focus_prev();
        }
    }

    /// Tab on a dialog card moves into (or around) the dialog's fields.
    pub fn handle_modal_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        if event.keystroke.key == "tab" {
            self.focus_modal_tab_stop(!event.keystroke.modifiers.shift, window);
        }
    }

    pub fn field_state_mut(&mut self, id: FieldId) -> &mut FieldState {
        match id {
            FieldId::NewGroup => &mut self.group_field,
            FieldId::ModalPath => &mut self.modal_path,
            FieldId::ModalCustomGroup => &mut self.modal_custom_group,
            FieldId::SettingsBranch => &mut self.settings_branch_field,
            FieldId::CommitMessage => &mut self.composers.active_mut().message,
            FieldId::CommitBody => &mut self.composers.active_mut().body,
            FieldId::SettingsEditor => &mut self.settings_editor_field,
            FieldId::BranchName => &mut self.branch_field,
            FieldId::CloneUrl => &mut self.clone_url,
            FieldId::CloneParent => &mut self.clone_parent,
            FieldId::NewRepoName => &mut self.new_repo_name,
            FieldId::NewRepoParent => &mut self.new_repo_parent,
        }
    }

    pub fn field_state(&self, id: FieldId) -> &FieldState {
        match id {
            FieldId::NewGroup => &self.group_field,
            FieldId::ModalPath => &self.modal_path,
            FieldId::ModalCustomGroup => &self.modal_custom_group,
            FieldId::SettingsBranch => &self.settings_branch_field,
            FieldId::CommitMessage => &self.composers.active().message,
            FieldId::CommitBody => &self.composers.active().body,
            FieldId::SettingsEditor => &self.settings_editor_field,
            FieldId::BranchName => &self.branch_field,
            FieldId::CloneUrl => &self.clone_url,
            FieldId::CloneParent => &self.clone_parent,
            FieldId::NewRepoName => &self.new_repo_name,
            FieldId::NewRepoParent => &self.new_repo_parent,
        }
    }

    pub fn field_focus(&self, id: FieldId) -> FocusHandle {
        match id {
            FieldId::NewGroup => self.group_field_focus.clone(),
            FieldId::ModalPath => self.modal_path_focus.clone(),
            FieldId::ModalCustomGroup => self.modal_custom_group_focus.clone(),
            FieldId::SettingsBranch => self.settings_branch_focus.clone(),
            FieldId::SettingsEditor => self.settings_editor_focus.clone(),
            FieldId::CommitMessage => self.commit_focus.clone(),
            FieldId::CommitBody => self.commit_body_focus.clone(),
            FieldId::BranchName => self.branch_focus.clone(),
            FieldId::CloneUrl => self.clone_url_focus.clone(),
            FieldId::CloneParent => self.clone_parent_focus.clone(),
            FieldId::NewRepoName => self.new_repo_name_focus.clone(),
            FieldId::NewRepoParent => self.new_repo_parent_focus.clone(),
        }
    }

    /// Apply a keystroke to the field. Returns whether the field consumed it,
    /// so the caller can stop it bubbling to list-level handlers.
    pub fn handle_field_key(
        &mut self,
        id: FieldId,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        // Tab walks the dialog's own fields; the window's tab stops outside it
        // are not tab stops while a dialog is open.
        if id.is_modal() && event.keystroke.key == "tab" {
            self.focus_modal_tab_stop(!event.keystroke.modifiers.shift, window);
            return true;
        }

        let outcome = {
            let state = self.field_state_mut(id);
            apply_field_key(state, event, cx)
        };

        match outcome {
            FieldKey::Ignored => false,
            FieldKey::Changed => {
                cx.notify();
                true
            }
            FieldKey::Cancel => {
                self.cancel_field(id, cx);
                true
            }
            FieldKey::Submit => {
                self.submit_field(id, cx);
                true
            }
        }
    }

    /// Escape in a field: close whichever surface owns it.
    fn cancel_field(&mut self, id: FieldId, cx: &mut Context<Self>) {
        if id == FieldId::BranchName {
            self.branch_prompt = None;
            self.branch_field.clear();
            cx.notify();
            return;
        }
        if id == FieldId::CommitMessage && self.composer().amending {
            self.cancel_amend(cx);
            return;
        }
        if id == FieldId::NewGroup {
            self.is_adding_group = false;
            self.group_field.clear();
        } else if self.is_settings_open {
            self.close_settings(cx);
        } else if self.is_add_modal_open {
            self.close_add_modal(cx);
        } else if self.selected_repo_id.is_some() {
            self.close_inspector(cx);
        }
        cx.notify();
    }

    /// Enter in a field: run the action that field belongs to.
    fn submit_field(&mut self, id: FieldId, cx: &mut Context<Self>) {
        match id {
            FieldId::NewGroup => {
                let name = self.group_field.value.clone();
                self.group_field.clear();
                self.is_adding_group = false;
                self.create_group(name, cx);
            }
            FieldId::ModalPath | FieldId::ModalCustomGroup => {
                if self.modal_group_choice == "__new__"
                    && self.modal_custom_group.value.trim().is_empty()
                {
                    self.modal_error = Some("Enter a group name.".to_string());
                    cx.notify();
                } else {
                    self.submit_add_modal(cx);
                }
            }
            FieldId::SettingsBranch => self.commit_settings_branch(cx),
            FieldId::SettingsEditor => self.commit_settings_editor(cx),
            FieldId::CommitMessage => self.commit_selected(cx),
            FieldId::BranchName => self.submit_branch_prompt(cx),
            FieldId::CommitBody => self.commit_selected(cx),
            FieldId::CloneUrl => self.submit_clone(cx),
            FieldId::CloneParent => self.submit_clone(cx),
            FieldId::NewRepoName => self.submit_new_repo(cx),
            FieldId::NewRepoParent => self.submit_new_repo(cx),
        }
    }

    /// Focus-tracking text field element.
    pub fn text_field(
        &self,
        id: FieldId,
        placeholder: &'static str,
        variant: crate::ui::FieldVariant,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let focus = self.field_focus(id).tab_stop(id.is_modal());
        let focused = focus.is_focused(window);
        let state = self.field_state(id).clone();
        let theme = self.theme;
        let text_id = gpui::ElementId::named_usize("field-text", id as u8 as usize);

        let row = crate::ui::field(&theme, &state, placeholder, focused, variant, text_id)
            .id(gpui::ElementId::named_usize("field", id as u8 as usize))
            .track_focus(&focus)
            .key_context(TEXT_FIELD_KEY_CONTEXT)
            .cursor_text()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    if matches!(id, FieldId::ModalPath | FieldId::ModalCustomGroup) {
                        this.modal_group_menu_open = false;
                    }
                    // This press belongs to a text field, so the window-level
                    // handler must leave focus alone.
                    this.text_click_claimed = true;
                    window.focus(&this.field_focus(id));
                    let state = this.field_state_mut(id);
                    if event.click_count >= 2 {
                        // No glyph metrics here, so a double-click selects the
                        // whole line rather than guessing a word boundary.
                        state.select_all();
                    } else {
                        state.end();
                    }
                    cx.notify();
                }),
            )
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                if this.handle_field_key(id, event, window, cx) {
                    cx.stop_propagation();
                }
            }));

        row
    }
}

impl Render for VantageApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        self.sync_focus(window, cx);

        let viewport = self.render_viewport(window, cx).into_any_element();
        let inspector = self.render_inspector(window, cx);

        let mut main = div()
            .flex()
            .flex_1()
            .min_w(px(0.0))
            .min_h(px(0.0))
            .h_full()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(px(0.0))
                    .min_h(px(0.0))
                    .h_full()
                    .child(viewport),
            );

        if let Some(panel) = inspector {
            let sidebar_width = self.sidebar_width();
            // Invisible 4px drag target: the panel's own 1px left border is the
            // visible divider, so the splitter does not add a thicker bar.
            let splitter = div()
                .id("right-panel-splitter")
                .w(px(4.0))
                .h_full()
                .flex_shrink_0()
                .cursor(gpui::CursorStyle::ResizeColumn)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, _w, _cx| {
                        this.right_panel_drag_start = Some((
                            f32::from(event.position.x),
                            this.settings.right_panel_fraction,
                        ));
                    }),
                );
            main = main
                .child(splitter)
                .child(panel)
                .on_mouse_move(cx.listener(
                    move |this, event: &gpui::MouseMoveEvent, window, cx| {
                        let Some((start_x, start_fraction)) = this.right_panel_drag_start else {
                            return;
                        };
                        let content_width = f32::from(window.bounds().size.width) - sidebar_width;
                        if content_width <= 0.0 {
                            return;
                        }
                        let delta = (f32::from(event.position.x) - start_x) / content_width;
                        this.settings.right_panel_fraction =
                            (start_fraction - delta).clamp(0.3, 0.8);
                        cx.notify();
                    },
                ))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _event: &gpui::MouseUpEvent, _window, cx| {
                        if this.right_panel_drag_start.take().is_some() {
                            this.save_settings(cx);
                        }
                    }),
                );
        }

        let sidebar_splitter = div()
            .id("sidebar-splitter")
            .w(px(4.0))
            .h_full()
            .flex_shrink_0()
            .cursor(gpui::CursorStyle::ResizeColumn)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, _w, _cx| {
                    this.sidebar_drag_start =
                        Some((f32::from(event.position.x), this.settings.sidebar_width));
                }),
            );

        let content = div()
            .flex()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .child(self.render_sidebar(window, cx))
            .when(!self.sidebar_collapsed, |d| d.child(sidebar_splitter))
            .child(main)
            .on_mouse_move(
                cx.listener(move |this, event: &gpui::MouseMoveEvent, window, cx| {
                    let Some((start_x, start_width)) = this.sidebar_drag_start else {
                        return;
                    };
                    let delta = f32::from(event.position.x) - start_x;
                    this.settings.sidebar_width = (start_width + delta).clamp(200.0, 360.0);
                    let _ = window;
                    cx.notify();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event: &gpui::MouseUpEvent, _window, cx| {
                    if this.sidebar_drag_start.take().is_some() {
                        this.save_settings(cx);
                    }
                }),
            );

        let mut root = div()
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.bg_app)
            .fg(theme.text_app)
            .font_family(crate::theme::FONT_SANS)
            .text_size(px(crate::theme::TEXT_SM))
            .line_height(crate::theme::LH_SM)
            .child(self.render_top_bar(window, cx));
        if let Some(notice) = self.render_state_recovery(cx) {
            root = root.child(notice);
        }
        let mut root = root
            .child(content)
            // Any press a text field did not claim drops focus, so clicking into
            // empty space leaves the commit box blurred instead of still taking
            // keystrokes.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event: &gpui::MouseDownEvent, window, cx| {
                    if std::mem::take(&mut this.text_click_claimed) {
                        return;
                    }
                    window.blur();
                    cx.notify();
                }),
            );

        // A transparent full-window catcher behind the deferred group menu:
        // any press outside the menu closes it. The menu itself is painted in
        // the deferred layer, so its own options still receive the click.
        if self.group_menu_open {
            root = root.child(
                div()
                    .id("group-menu-dismiss")
                    .absolute()
                    .top(px(0.0))
                    .left(px(0.0))
                    .size_full()
                    .bg(rgba(0x00000000))
                    .on_click(cx.listener(|this, _e, _w, cx| {
                        this.group_menu_open = false;
                        cx.notify();
                    })),
            );
        }
        // A right-click menu: a press anywhere else dismisses it, and the panel
        // paints above everything in the deferred layer.
        if let Some(menu) = self.context_menu.clone() {
            let bounds = window.bounds();
            let width = 200.0;
            let height = 260.0;
            let (anchor_x, anchor_y, title) = match &menu {
                ContextMenu::File { path, x, y, .. } => {
                    let name = path
                        .rsplit_once('/')
                        .map_or(path.as_str(), |(_, name)| name)
                        .to_string();
                    (*x, *y, name)
                }
                ContextMenu::Repo { name, x, y, .. } => (*x, *y, name.clone()),
                ContextMenu::Branch { name, x, y, .. } => (*x, *y, name.clone()),
                ContextMenu::Commit {
                    short,
                    subject,
                    x,
                    y,
                    ..
                } => (*x, *y, format!("{short}  {subject}")),
            };
            let items = menu.rows();
            let x = anchor_x
                .min(f32::from(bounds.size.width) - width - 8.0)
                .max(8.0);
            let y = anchor_y
                .min(f32::from(bounds.size.height) - height - 8.0)
                .max(8.0);

            let mut panel = div()
                .absolute()
                .left(px(x))
                .top(px(y))
                .w(px(width))
                .flex()
                .flex_col()
                .p(px(4.0))
                .bg(theme.panel)
                .border_1()
                .border_color(theme.divider)
                .rounded(px(8.0))
                .shadow_lg()
                .child(
                    div()
                        .px(px(8.0))
                        .py(px(4.0))
                        .xs()
                        .fg(theme.label_muted)
                        .truncate()
                        .child(title),
                )
                .child(div().h(px(1.0)).my(px(4.0)).bg(theme.divider));

            for (label, action, destructive) in items {
                panel = panel.child(
                    div()
                        .id(gpui::SharedString::from(format!("ctx-{label}")))
                        .flex()
                        .items_center()
                        .px(px(8.0))
                        .py(px(8.0))
                        .rounded(px(4.0))
                        .cursor_pointer()
                        .xs()
                        .medium()
                        .fg(if destructive {
                            theme.status_red
                        } else {
                            theme.text_soft
                        })
                        .hover(move |s| s.bg(theme.row_hover_bg))
                        .child(label)
                        .on_click(cx.listener(move |this, _e, _w, cx| {
                            this.run_context_action(&action, cx);
                        })),
                );
            }

            root = root
                .child(
                    div()
                        .id("context-menu-dismiss")
                        .absolute()
                        .top(px(0.0))
                        .left(px(0.0))
                        .size_full()
                        .bg(rgba(0x00000000))
                        .on_click(cx.listener(|this, _e, _w, cx| {
                            this.close_file_menu(cx);
                        })),
                )
                .child(deferred(panel));
        }
        if self.stash_modal.is_some() {
            let modal = self.render_stash_modal(window, cx);
            root = root.child(deferred(modal));
        }
        if self.is_new_repo_modal_open {
            let modal = self.render_new_repo_modal(window, cx);
            root = root.child(deferred(modal));
        }
        if self.is_clone_modal_open {
            let modal = self.render_clone_modal(window, cx);
            root = root.child(deferred(modal));
        }
        if self.confirm.is_some() {
            let modal = self.render_confirm_modal(window, cx);
            root = root.child(deferred(modal));
        }
        if self.branch_prompt.is_some() {
            let modal = self.render_branch_modal(window, cx);
            root = root.child(deferred(modal));
        }
        if self.is_add_modal_open {
            root = root.child(self.render_add_repo_modal(window, cx));
        }
        if self.is_settings_open {
            root = root.child(self.render_settings_modal(window, cx));
        }
        if let Some(toast) = self.render_toast(window, cx) {
            root = root.child(toast);
        }
        if let Some(details) = self.render_toast_details(cx) {
            root = root.child(deferred(details));
        }
        root
    }
}

/// Identity of an asynchronous commit, comparison or stash load.
///
/// `generation` makes even a reload of the same target distinct, `repo_id`
/// pins the load to one repository and `kind` names the exact target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadRequest {
    pub generation: u64,
    pub repo_id: String,
    pub kind: LoadKind,
}

/// What a [`LoadRequest`] is loading.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadKind {
    Commit { sha: String },
    Compare { base: String, other: String },
    Stashes,
}

/// Why a clone attempt did not register a repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CloneFailure {
    Cancelled,
    Failed(String),
}

impl CloneFailure {
    pub fn message(&self) -> &str {
        match self {
            Self::Cancelled => "Clone cancelled. Nothing was added to the workspace.",
            Self::Failed(message) => message,
        }
    }
}

/// Messages from the background clone to its UI loop.
enum CloneMessage {
    Progress(String),
    Done(anyhow::Result<()>),
}

/// Turn a backend error into the state the clone dialog shows.
fn clone_failure(error: &anyhow::Error) -> CloneFailure {
    match error.downcast_ref::<vantage_core::AppError>() {
        Some(error) if error.is_cancelled() => CloneFailure::Cancelled,
        Some(error) => CloneFailure::Failed(format!("Clone failed: {}", error.message)),
        None => CloneFailure::Failed(format!("Clone failed: {error}")),
    }
}

/// The stash list, opened as a dialog.
#[derive(Clone, Debug)]
pub struct StashModal {
    pub repo_id: String,
    pub entries: Vec<vantage_core::state::StashEntry>,
    pub loading: bool,
    pub error: Option<String>,
    /// The load this dialog is waiting on; a result for any other request is
    /// discarded.
    pub request: LoadRequest,
}

impl StashModal {
    fn loading(request: LoadRequest, repo_id: &str) -> Self {
        Self {
            repo_id: repo_id.to_string(),
            entries: Vec::new(),
            loading: true,
            error: None,
            request,
        }
    }
}

/// Apply a finished stash load only while the dialog still waits on it.
///
/// Returns whether the result was applied. A dialog that was closed (or
/// replaced by a newer request) ignores the result instead of reopening.
fn apply_stash_result(
    modal: Option<&mut StashModal>,
    request: &LoadRequest,
    result: Result<Vec<vantage_core::state::StashEntry>, String>,
) -> bool {
    let Some(modal) = modal else {
        return false;
    };
    if modal.request != *request {
        return false;
    }
    modal.loading = false;
    match result {
        Ok(entries) => {
            modal.entries = entries;
            modal.error = None;
        }
        Err(error) => {
            modal.entries.clear();
            modal.error = Some(error);
        }
    }
    true
}

/// A destructive action waiting for confirmation.
#[derive(Clone, Debug)]
pub struct ConfirmPrompt {
    pub title: String,
    pub body: String,
    pub label: &'static str,
    pub action: ConfirmAction,
}

#[derive(Clone, Debug)]
pub enum ConfirmAction {
    DiscardRepository { repo_id: String, paths: Vec<String> },
    ForgetRepository { repo_id: String },
    MergeBranch { repo_id: String, target: String },
    UpdateFromDefault { repo_id: String },
    ResetToCommit { repo_id: String, sha: String },
}

/// A commit opened in the detail view: its files, and which one is on screen.
#[derive(Clone, Debug)]
pub struct CommitView {
    pub repo_id: String,
    pub sha: String,
    pub subject: String,
    pub author: String,
    pub author_email: String,
    pub time: String,
    pub files: Vec<crate::model::CommitFile>,
    pub selected: usize,
    pub loading: bool,
    /// True when this is a branch comparison rather than one commit.
    pub comparing: bool,
    /// The load this view is waiting on; a result for any other request is
    /// discarded.
    pub request: LoadRequest,
    /// Git's message when the load failed, shown instead of an empty diff.
    pub error: Option<String>,
}

impl CommitView {
    fn commit(request: LoadRequest, repo: &Repository, commit: &crate::model::CommitEntry) -> Self {
        Self {
            repo_id: repo.id.clone(),
            sha: commit.sha.clone(),
            subject: commit.message.clone(),
            author: commit.author.clone(),
            author_email: commit.author_email.clone(),
            time: commit.relative_time.clone(),
            files: Vec::new(),
            selected: 0,
            loading: true,
            comparing: false,
            request,
            error: None,
        }
    }

    fn comparison(request: LoadRequest, repo: &Repository, base: &str, other: &str) -> Self {
        Self {
            repo_id: repo.id.clone(),
            sha: format!("{base}\u{2026}{other}"),
            subject: format!("Changes from {base} to {other}"),
            author: String::new(),
            author_email: String::new(),
            time: String::new(),
            files: Vec::new(),
            selected: 0,
            loading: true,
            comparing: true,
            request,
            error: None,
        }
    }
}

/// Apply a finished commit or comparison load only while the view still waits
/// on it.
///
/// Returns whether the result was applied. A view that was closed (or replaced
/// by a newer request) ignores the result, so A's files can never land under
/// B's header.
fn apply_commit_result(
    view: Option<&mut CommitView>,
    request: &LoadRequest,
    result: Result<Vec<crate::model::CommitFile>, String>,
) -> bool {
    let Some(view) = view else {
        return false;
    };
    if view.request != *request {
        return false;
    }
    view.loading = false;
    match result {
        Ok(files) => {
            view.files = files;
            view.selected = 0;
            view.error = None;
        }
        Err(error) => {
            view.files.clear();
            view.error = Some(error);
        }
    }
    true
}

/// An in-progress branch action, answered by the inline row in the Branches tab.
///
/// `repo_id` is captured when the prompt opens. Submission always resolves that
/// repository, never the current selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BranchPrompt {
    pub repo_id: String,
    pub kind: BranchPromptKind,
}

/// The branch action a prompt is asking for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BranchPromptKind {
    /// Create a branch from an existing one.
    Create { base: String },
    /// Rename the current branch.
    Rename { from: String },
    /// Delete a branch after a confirm.
    Delete { name: String },
    /// Retry the delete with `-D` after git refused `-d` because the branch
    /// has unmerged commits. Only reached through the second confirmation.
    ForceDelete { name: String },
}

/// Refuse a prompt whose branch has changed state since the menu opened.
///
/// `branches` is a fresh listing from git for the prompt's repository. Git
/// still has the final say when the write runs; this catches the common case of
/// a branch that was deleted or checked out elsewhere while the dialog was open
/// and gives a message that names the branch.
fn validate_branch_prompt(
    kind: &BranchPromptKind,
    branches: &[vantage_core::state::BranchInfo],
) -> Result<(), String> {
    match kind {
        BranchPromptKind::Create { base } => {
            if branches.iter().any(|branch| branch.name == *base) {
                Ok(())
            } else {
                Err(format!(
                    "Branch '{base}' no longer exists, so nothing was created."
                ))
            }
        }
        BranchPromptKind::Rename { from } => match branches.iter().find(|b| b.name == *from) {
            None => Err(format!(
                "Branch '{from}' no longer exists, so nothing was renamed."
            )),
            Some(branch) if !branch.is_head => Err(format!(
                "'{from}' is no longer the checked-out branch; switch back to it before renaming."
            )),
            Some(_) => Ok(()),
        },
        BranchPromptKind::Delete { name } | BranchPromptKind::ForceDelete { name } => {
            match branches.iter().find(|b| b.name == *name) {
                None => Err(format!(
                    "Branch '{name}' no longer exists, so nothing was deleted."
                )),
                Some(branch) if branch.is_head => Err(format!(
                    "'{name}' is now the checked-out branch; switch to another branch first."
                )),
                Some(_) => Ok(()),
            }
        }
    }
}

/// A branch action a row offers without right-click.
///
/// The repository id is part of the action, not looked up at click time, so a
/// row action keeps targeting the branch and repository it was rendered for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BranchRowAction {
    Checkout { repo_id: String, branch: String },
    Create { repo_id: String, base: String },
    Merge { repo_id: String, branch: String },
    Compare { repo_id: String, branch: String },
    Rename { repo_id: String, from: String },
    Delete { repo_id: String, branch: String },
}

impl BranchRowAction {
    /// A short label for the row's action strip and the tests that pin it.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Checkout { .. } => "Checkout",
            Self::Create { .. } => "New branch",
            Self::Merge { .. } => "Merge into current",
            Self::Compare { .. } => "Compare",
            Self::Rename { .. } => "Rename",
            Self::Delete { .. } => "Delete",
        }
    }

    pub fn is_destructive(&self) -> bool {
        matches!(self, Self::Delete { .. })
    }

    pub fn repo_id(&self) -> &str {
        match self {
            Self::Checkout { repo_id, .. }
            | Self::Create { repo_id, .. }
            | Self::Merge { repo_id, .. }
            | Self::Compare { repo_id, .. }
            | Self::Rename { repo_id, .. }
            | Self::Delete { repo_id, .. } => repo_id,
        }
    }

    /// The prompt this action opens, if it needs a name or confirmation.
    pub fn prompt(&self) -> Option<BranchPromptKind> {
        match self {
            Self::Create { base, .. } => Some(BranchPromptKind::Create { base: base.clone() }),
            Self::Rename { from, .. } => Some(BranchPromptKind::Rename { from: from.clone() }),
            Self::Delete { branch, .. } => Some(BranchPromptKind::Delete {
                name: branch.clone(),
            }),
            Self::Checkout { .. } | Self::Merge { .. } | Self::Compare { .. } => None,
        }
    }
}

/// The actions a branch row offers, in display order.
///
/// Checkout, Merge and Compare only make sense against another branch; Rename
/// only against the checked-out one. Delete is offered for any branch that is
/// not checked out and always opens the confirmation dialog.
pub fn branch_row_actions(repo_id: &str, branch: &BranchInfo) -> Vec<BranchRowAction> {
    let mut actions = Vec::new();
    if !branch.is_current {
        actions.push(BranchRowAction::Checkout {
            repo_id: repo_id.to_string(),
            branch: branch.name.clone(),
        });
    }
    actions.push(BranchRowAction::Create {
        repo_id: repo_id.to_string(),
        base: branch.name.clone(),
    });
    if !branch.is_current {
        actions.push(BranchRowAction::Merge {
            repo_id: repo_id.to_string(),
            branch: branch.name.clone(),
        });
        actions.push(BranchRowAction::Compare {
            repo_id: repo_id.to_string(),
            branch: branch.name.clone(),
        });
    }
    if branch.is_current {
        actions.push(BranchRowAction::Rename {
            repo_id: repo_id.to_string(),
            from: branch.name.clone(),
        });
    }
    if !branch.is_current {
        actions.push(BranchRowAction::Delete {
            repo_id: repo_id.to_string(),
            branch: branch.name.clone(),
        });
    }
    actions
}

/// A right-click menu, anchored at the pointer.
#[derive(Clone, Debug)]
pub enum ContextMenu {
    /// Actions for one repository row.
    Repo {
        repo_id: String,
        name: String,
        uncommitted: usize,
        stashes: usize,
        /// Opened from the Changes header: stash/discard belong there; the
        /// repository menu itself only carries repository actions.
        with_changes: bool,
        x: f32,
        y: f32,
    },
    /// Actions for one changed file.
    File {
        repo_id: String,
        path: String,
        x: f32,
        y: f32,
    },
    /// Actions for one branch.
    Branch {
        repo_id: String,
        name: String,
        is_current: bool,
        is_default: bool,
        x: f32,
        y: f32,
    },
    /// Actions for one commit.
    Commit {
        repo_id: String,
        short: String,
        subject: String,
        /// The original description, carried so amend does not delete it.
        body: String,
        /// The tip commit is the only one that can be amended.
        is_tip: bool,
        x: f32,
        y: f32,
    },
}

/// One row of a context menu: its label, the typed action a click runs, and
/// whether the row is styled as destructive.
type ContextMenuRow = (&'static str, ContextAction, bool);

impl ContextMenu {
    /// The rows this menu shows, in display order.
    ///
    /// Labels and destructive styling are derived from the action, so they
    /// cannot drift from what a click actually does.
    fn rows(&self) -> Vec<ContextMenuRow> {
        let actions: Vec<ContextAction> = match self {
            Self::File { repo_id, path, .. } => vec![
                ContextAction::IgnoreFile {
                    repo_id: repo_id.clone(),
                    path: path.clone(),
                },
                ContextAction::DiscardFile {
                    repo_id: repo_id.clone(),
                    path: path.clone(),
                },
            ],
            Self::Repo {
                repo_id,
                uncommitted,
                stashes,
                with_changes,
                ..
            } => {
                // One menu, one subject: the working tree where it was opened
                // from the Changes header, the repository elsewhere.
                let mut actions = Vec::new();
                if *with_changes {
                    if *uncommitted > 0 {
                        actions.push(ContextAction::StashAll {
                            repo_id: repo_id.clone(),
                        });
                    }
                    if *stashes > 0 {
                        actions.push(ContextAction::RestoreStash {
                            repo_id: repo_id.clone(),
                        });
                    }
                    if *uncommitted > 0 {
                        actions.push(ContextAction::DiscardRepo {
                            repo_id: repo_id.clone(),
                        });
                    }
                } else {
                    actions.push(ContextAction::UpdateFromDefault {
                        repo_id: repo_id.clone(),
                    });
                    if *stashes > 0 {
                        actions.push(ContextAction::ManageStashes {
                            repo_id: repo_id.clone(),
                        });
                    }
                    actions.push(ContextAction::ViewOnGitHub {
                        repo_id: repo_id.clone(),
                    });
                    actions.push(ContextAction::RemoveFromList {
                        repo_id: repo_id.clone(),
                    });
                    actions.push(ContextAction::RevealInFinder {
                        repo_id: repo_id.clone(),
                    });
                    actions.push(ContextAction::OpenInTerminal {
                        repo_id: repo_id.clone(),
                    });
                    actions.push(ContextAction::OpenInEditor {
                        repo_id: repo_id.clone(),
                    });
                }
                actions
            }
            Self::Branch {
                repo_id,
                name,
                is_current,
                is_default,
                ..
            } => {
                let mut actions = Vec::new();
                if !*is_current {
                    actions.push(ContextAction::CheckoutBranch {
                        repo_id: repo_id.clone(),
                        branch: name.clone(),
                    });
                    actions.push(ContextAction::MergeBranch {
                        repo_id: repo_id.clone(),
                        branch: name.clone(),
                    });
                    actions.push(ContextAction::CompareWithCurrent {
                        repo_id: repo_id.clone(),
                        branch: name.clone(),
                    });
                }
                actions.push(ContextAction::NewBranchFrom {
                    repo_id: repo_id.clone(),
                    base: name.clone(),
                });
                if *is_current {
                    actions.push(ContextAction::RenameCurrentBranch {
                        repo_id: repo_id.clone(),
                        from: name.clone(),
                    });
                }
                if !*is_current && !*is_default {
                    actions.push(ContextAction::DeleteBranch {
                        repo_id: repo_id.clone(),
                        name: name.clone(),
                    });
                }
                actions
            }
            Self::Commit {
                repo_id,
                short,
                subject,
                body,
                is_tip,
                ..
            } => {
                // Only the tip commit can be rewritten, and only while it is
                // still local.
                let mut actions = vec![
                    ContextAction::RevertCommit {
                        repo_id: repo_id.clone(),
                        sha: short.clone(),
                    },
                    ContextAction::ResetBranchHere {
                        repo_id: repo_id.clone(),
                        sha: short.clone(),
                    },
                ];
                if *is_tip {
                    actions.push(ContextAction::AmendCommit {
                        repo_id: repo_id.clone(),
                        subject: subject.clone(),
                        body: body.clone(),
                    });
                }
                actions
            }
        };

        actions
            .into_iter()
            .map(|action| {
                let label = action.label();
                let destructive = action.is_destructive();
                (label, action, destructive)
            })
            .collect()
    }
}

/// An action a context-menu row performs, with the subject it was opened for.
///
/// Dispatching on this value instead of the row's English label means renaming
/// a label can never silently disable an action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContextAction {
    IgnoreFile {
        repo_id: String,
        path: String,
    },
    DiscardFile {
        repo_id: String,
        path: String,
    },
    StashAll {
        repo_id: String,
    },
    RestoreStash {
        repo_id: String,
    },
    DiscardRepo {
        repo_id: String,
    },
    UpdateFromDefault {
        repo_id: String,
    },
    ManageStashes {
        repo_id: String,
    },
    ViewOnGitHub {
        repo_id: String,
    },
    RemoveFromList {
        repo_id: String,
    },
    RevealInFinder {
        repo_id: String,
    },
    OpenInTerminal {
        repo_id: String,
    },
    OpenInEditor {
        repo_id: String,
    },
    CheckoutBranch {
        repo_id: String,
        branch: String,
    },
    MergeBranch {
        repo_id: String,
        branch: String,
    },
    CompareWithCurrent {
        repo_id: String,
        branch: String,
    },
    NewBranchFrom {
        repo_id: String,
        base: String,
    },
    RenameCurrentBranch {
        repo_id: String,
        from: String,
    },
    DeleteBranch {
        repo_id: String,
        name: String,
    },
    AmendCommit {
        repo_id: String,
        subject: String,
        body: String,
    },
    RevertCommit {
        repo_id: String,
        sha: String,
    },
    ResetBranchHere {
        repo_id: String,
        sha: String,
    },
}

impl ContextAction {
    /// The row text this action shows.
    fn label(&self) -> &'static str {
        match self {
            Self::IgnoreFile { .. } => "Add to .gitignore",
            Self::DiscardFile { .. } => "Discard changes",
            Self::StashAll { .. } => "Stash all changes",
            Self::RestoreStash { .. } => "Restore latest stash",
            Self::DiscardRepo { .. } => "Discard all changes",
            Self::UpdateFromDefault { .. } => "Update from default branch",
            Self::ManageStashes { .. } => "Manage stashes…",
            Self::ViewOnGitHub { .. } => "View on GitHub",
            Self::RemoveFromList { .. } => "Remove from list",
            Self::RevealInFinder { .. } => "Reveal in Finder",
            Self::OpenInTerminal { .. } => "Open in Terminal",
            Self::OpenInEditor { .. } => "Open in editor",
            Self::CheckoutBranch { .. } => "Checkout branch",
            Self::MergeBranch { .. } => "Merge into current branch",
            Self::CompareWithCurrent { .. } => "Compare with current",
            Self::NewBranchFrom { .. } => "New branch from this branch",
            Self::RenameCurrentBranch { .. } => "Rename current branch",
            Self::DeleteBranch { .. } => "Delete branch",
            Self::AmendCommit { .. } => "Amend commit",
            Self::RevertCommit { .. } => "Revert commit",
            Self::ResetBranchHere { .. } => "Reset branch here",
        }
    }

    fn is_destructive(&self) -> bool {
        matches!(
            self,
            Self::DiscardFile { .. }
                | Self::DiscardRepo { .. }
                | Self::RemoveFromList { .. }
                | Self::DeleteBranch { .. }
                | Self::ResetBranchHere { .. }
        )
    }

    #[cfg(test)]
    fn repo_id(&self) -> &str {
        match self {
            Self::IgnoreFile { repo_id, .. }
            | Self::DiscardFile { repo_id, .. }
            | Self::StashAll { repo_id }
            | Self::RestoreStash { repo_id }
            | Self::DiscardRepo { repo_id }
            | Self::UpdateFromDefault { repo_id }
            | Self::ManageStashes { repo_id }
            | Self::ViewOnGitHub { repo_id }
            | Self::RemoveFromList { repo_id }
            | Self::RevealInFinder { repo_id }
            | Self::OpenInTerminal { repo_id }
            | Self::OpenInEditor { repo_id }
            | Self::CheckoutBranch { repo_id, .. }
            | Self::MergeBranch { repo_id, .. }
            | Self::CompareWithCurrent { repo_id, .. }
            | Self::NewBranchFrom { repo_id, .. }
            | Self::RenameCurrentBranch { repo_id, .. }
            | Self::DeleteBranch { repo_id, .. }
            | Self::AmendCommit { repo_id, .. }
            | Self::RevertCommit { repo_id, .. }
            | Self::ResetBranchHere { repo_id, .. } => repo_id,
        }
    }
}

/// Which slice of the list the summary strip is showing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SummaryFilter {
    All,
    Uncommitted,
    Unpushed,
    Clean,
}

/// Repository counts shown above the list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RepoSummary {
    pub total: usize,
    pub uncommitted: usize,
    pub unpushed: usize,
    pub clean: usize,
}

/// What the detail pane offers to undo, if anything.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UndoOffer {
    /// Commit subject the strip shows.
    pub summary: String,
    /// True when this session made the commit, so the strip can say "just now".
    pub just_now: bool,
    /// Local commits that never reached the remote, for the strip's count.
    pub unpushed: u32,
}

/// The implicit bucket for repositories with no group assignment. It is
/// always offered by group selectors, but only stored as a group when it
/// already exists in the configured list.
pub(crate) const UNGROUPED_GROUP: &str = "Ungrouped";

/// Options for a group selector: every configured group plus "Ungrouped",
/// which must be reachable even when an older state never stored it.
pub(crate) fn group_choices(groups: &[String]) -> Vec<String> {
    let mut choices = groups.to_vec();
    if !choices.iter().any(|group| group == UNGROUPED_GROUP) {
        choices.push(UNGROUPED_GROUP.to_string());
    }
    choices
}

/// Group choices that also keep the repository's current group selectable,
/// even when it is missing from the configured list.
pub(crate) fn group_choices_including(groups: &[String], current: &str) -> Vec<String> {
    let mut choices = group_choices(groups);
    if !choices.iter().any(|group| group == current) {
        choices.push(current.to_string());
    }
    choices
}

/// Group a newly added repository starts in. While the user is filtering a
/// specific group, adding into it is the expected default; from "All" there
/// is no such context, so a new repository stays Ungrouped instead of
/// silently joining the first group in the list.
fn default_group_for_new_repo(active_group: &str) -> String {
    if active_group == "All" {
        UNGROUPED_GROUP.to_string()
    } else {
        active_group.to_string()
    }
}

/// Add a user-created group to the configured list. "Ungrouped" is derived
/// from a missing assignment, so a move to it must not create a stored group.
fn add_group_if_missing(groups: &mut Vec<String>, group: &str) {
    if group == UNGROUPED_GROUP {
        return;
    }
    if !groups.iter().any(|existing| existing == group) {
        groups.push(group.to_string());
    }
}

/// Recompute persisted assignments from the rows the engine knows about.
/// "Ungrouped" means no assignment, so it is removed rather than written.
fn sync_assignments(
    repos: &[Repository],
    known: &HashSet<String>,
    assignments: &mut std::collections::BTreeMap<String, String>,
) {
    for repo in repos {
        if !known.contains(&repo.id) {
            continue;
        }
        if repo.workspace == UNGROUPED_GROUP {
            assignments.remove(&repo.id);
        } else {
            assignments.insert(repo.id.clone(), repo.workspace.clone());
        }
    }
}

/// Sort key for "attention first": a conflict, then unpushed work, then
/// uncommitted changes, then anything behind its remote, then the quiet ones.
/// Lower ranks sort first; the second element keeps the newest first within a
/// rank.
fn attention_rank(repo: &Repository) -> (u8, i64) {
    let rank = if repo.sync_state == RepoSyncState::Conflict {
        0
    } else if repo.ahead_count > 0 {
        1
    } else if !repo.uncommitted_files.is_empty() {
        2
    } else if repo.behind_count > 0 {
        3
    } else {
        4
    };
    (rank, repo.last_activity_minutes)
}

/// A repository with nothing to look at: no changes, nothing unpushed, not
/// behind, no conflict.
fn repo_is_clean(repo: &Repository) -> bool {
    repo.uncommitted_files.is_empty()
        && repo.ahead_count <= 0
        && repo.behind_count <= 0
        && repo.sync_state != RepoSyncState::Conflict
}

/// Resolve which files a commit includes: the user's tick wins over the flag the
/// last scan reported, so a rescan landing between a tick and the commit cannot
/// drop that file from the commit.
fn ticked_paths(
    overrides: Option<&std::collections::HashMap<String, bool>>,
    repo: &Repository,
) -> Vec<String> {
    repo.uncommitted_files
        .iter()
        .filter(|file| {
            overrides
                .and_then(|chosen| chosen.get(&file.path))
                .copied()
                .unwrap_or(file.staged)
        })
        .map(|file| file.path.clone())
        .collect()
}

/// A normal commit needs at least one ticked path; an amend can rewrite only
/// the message, so it is valid on a clean tree.
fn commit_needs_selected_paths(amending: bool) -> bool {
    !amending
}

/// The contract shown before a reset: the branch and the tracked working tree
/// move to the selected commit, which is a hard reset.
fn reset_confirmation_body(sha: &str) -> String {
    format!(
        "The branch tip and tracked files move to {sha}. A clean working tree is required: \
         uncommitted and untracked changes are refused, never discarded. A backup ref of the \
         current tip is kept first."
    )
}

/// Why a reset cannot start, from the files the last scan reported. A clean
/// tree has no refusal; the reset itself re-checks under the repository lock.
fn reset_refusal(repo: &Repository) -> Option<String> {
    let count = repo.uncommitted_files.len();
    if count == 0 {
        return None;
    }
    let noun = if count == 1 { "change" } else { "changes" };
    let first = &repo.uncommitted_files[0].path;
    Some(format!(
        "Cannot reset {}: {count} uncommitted {noun} in the working tree (first: {first}). \
         Commit or discard them first.",
        repo.current_branch
    ))
}

/// The commit an Undo would reset, given the row's state and this session's
/// last commit.
///
/// The offer exists whenever the branch has commits the remote does not, which
/// is exactly when a soft reset is safe to offer. The session record widens it
/// by one rescan: right after committing, the row still shows the commit we
/// replaced, so without it the strip would appear a beat late.
fn undo_offer(record: Option<&LastCommit>, repo: &Repository) -> Option<UndoOffer> {
    let tip = repo.recent_commits.first()?;

    if let Some(record) = record {
        if record.repo_id == repo.id {
            // Our commit, before the row list caught up with it.
            if record.previous_tip.as_deref() == Some(tip.sha.as_str()) {
                return Some(UndoOffer {
                    summary: record.summary.clone(),
                    just_now: true,
                    unpushed: repo.ahead_count.max(1) as u32,
                });
            }
            if record.short == tip.sha {
                return Some(UndoOffer {
                    summary: tip.message.clone(),
                    just_now: true,
                    unpushed: repo.ahead_count.max(1) as u32,
                });
            }
        }
    }

    // Any other commit that is still local — including one made before this
    // session, or from a terminal.
    (repo.ahead_count > 0).then(|| UndoOffer {
        summary: tip.message.clone(),
        just_now: false,
        unpushed: repo.ahead_count as u32,
    })
}

/// Run a platform launch command and flatten both detection and spawn errors
/// into a message the UI can show.
fn spawn_external(
    command: Result<vantage_core::detect::LaunchCommand, vantage_core::detect::LaunchError>,
) -> Result<(), String> {
    command
        .map_err(|error| error.to_string())?
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// One line naming the repositories a batch add could not add, capped so the
/// modal stays readable when a whole folder is broken.
fn scan_failure_message(added: usize, failures: &[(String, String)]) -> String {
    let total = added + failures.len();
    let mut message = if added == 0 {
        format!("None of the {total} selected repositories could be added. ")
    } else {
        format!("Added {added} of {total} repositories. ")
    };

    let shown: Vec<String> = failures
        .iter()
        .take(3)
        .map(|(path, error)| {
            let name = std::path::Path::new(path)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.clone());
            format!("{name}: {error}")
        })
        .collect();
    message.push_str(&shown.join("; "));
    if failures.len() > shown.len() {
        message.push_str(&format!("; and {} more", failures.len() - shown.len()));
    }
    message
}

/// `~` expansion for repo paths that come back from the UI model.
pub fn expand_home(path: &str) -> std::path::PathBuf {
    if let Some(home) = dirs::home_dir() {
        if path == "~" {
            return home;
        }
        if let Some(rest) = path.strip_prefix("~/") {
            return home.join(rest);
        }
    }
    std::path::PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use gpui::{AppContext, Modifiers, TestAppContext, VisualTestContext};

    use super::*;
    use crate::model::{ChangedFile, FileChangeStatus};
    use crate::root::RootView;
    fn temp_state_dir(name: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "vantage_app_state_{name}_{}_{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn file(path: &str, staged: bool) -> ChangedFile {
        ChangedFile {
            path: path.to_string(),
            status: FileChangeStatus::Modified,
            additions: 1,
            deletions: 0,
            staged,
            orig_path: None,
            diff_preview: Vec::new(),
            preview_truncated: false,
        }
    }

    fn repo_with(files: Vec<ChangedFile>) -> Repository {
        Repository {
            id: "repo-1".to_string(),
            name: "fixture".to_string(),
            workspace: "Ungrouped".to_string(),
            local_path: "/tmp/fixture".to_string(),
            head_sha: Some("abc1234".to_string()),
            has_remote: true,
            stash_count: 0,
            current_branch: "main".to_string(),
            author: None,
            sync_state: RepoSyncState::Modified,
            ahead_count: 0,
            behind_count: 0,
            has_upstream: true,
            is_detached: false,
            last_activity_label: "just now".to_string(),
            last_activity_minutes: 0,
            uncommitted_files: files,
            branches: Vec::new(),
            recent_commits: Vec::new(),
            local_only_commits: std::collections::HashSet::new(),
        }
    }

    fn root(name: &str) -> PathBuf {
        PathBuf::from(format!("/tmp/{name}"))
    }

    #[test]
    fn dirty_scan_requests_are_coalesced_into_one_follow_up() {
        let mut pending = PendingScans::default();
        pending.push(Some(vec![root("one"), root("two")]), false);
        pending.push(Some(vec![root("two"), root("three")]), false);

        let (roots, silent) = pending.take().expect("one follow-up scan");
        let mut roots = roots.expect("dirty roots stay dirty");
        roots.sort();
        assert_eq!(roots, vec![root("one"), root("three"), root("two")]);
        assert!(!silent);
        assert!(
            pending.take().is_none(),
            "taking the follow-up must empty the queue"
        );
    }

    #[test]
    fn a_full_scan_supersedes_queued_dirty_roots() {
        let mut pending = PendingScans::default();
        pending.push(Some(vec![root("one")]), true);
        pending.push(None, true);

        let (roots, silent) = pending.take().expect("the full scan is queued");
        assert!(roots.is_none(), "one full scan replaces the dirty subset");
        assert!(silent);
    }

    #[test]
    fn a_visible_request_makes_the_coalesced_scan_visible() {
        let mut pending = PendingScans::default();
        pending.push(Some(vec![root("one")]), true);
        pending.push(Some(vec![root("two")]), false);

        let (_, silent) = pending.take().expect("the dirty scan is queued");
        assert!(
            !silent,
            "a scan is shown when any request behind it asked for the spinner"
        );
    }

    #[test]
    fn a_preview_only_matches_the_row_it_was_read_for() {
        let preview = FilePreview {
            staged: false,
            status: FileChangeStatus::Modified,
            additions: 2,
            deletions: 1,
            lines: Vec::new(),
            truncated: false,
            error: None,
        };
        let mut matching = file("src/lib.rs", false);
        matching.additions = 2;
        matching.deletions = 1;
        assert!(preview.matches(&matching));

        matching.additions = 3;
        assert!(
            !preview.matches(&matching),
            "a rescan with new counts must not show the old patch"
        );
    }

    fn details() -> RepoDetails {
        RepoDetails {
            has_remote: true,
            stash_count: 1,
            author: None,
            branches: Vec::new(),
            recent_commits: Vec::new(),
            local_only_commits: std::collections::HashSet::new(),
        }
    }

    #[test]
    fn details_are_loaded_once_per_head_and_branch() {
        let repo = repo_with_commits(&["abc1234"]);
        let mut store = DetailsStore::default();
        assert!(store.needs_load(&repo), "nothing cached for this HEAD");

        let (key, version) = store.begin(&repo).expect("start the first load");
        assert!(
            !store.needs_load(&repo),
            "a running request must not be duplicated"
        );
        store.finish(&repo.id, key, version, Ok(details()));

        assert!(store.cached(&repo).is_some());
        assert!(
            !store.needs_load(&repo),
            "the same HEAD and branch reuse the cached details"
        );
        assert!(
            store.begin(&repo).is_none(),
            "selecting the same row again must not start another load"
        );

        let mut moved = repo.clone();
        moved.head_sha = Some("def5678".to_string());
        assert!(
            store.needs_load(&moved),
            "moving HEAD makes the cached details stale"
        );

        let mut rebased = repo;
        rebased.current_branch = "feature".to_string();
        assert!(
            store.needs_load(&rebased),
            "checking out another branch makes the cached details stale"
        );
    }

    #[test]
    fn invalidating_details_discards_an_in_flight_result() {
        let repo = repo_with_commits(&["abc1234"]);
        let mut store = DetailsStore::default();
        let (key, version) = store.begin(&repo).expect("start the load");

        store.invalidate(&repo.id);
        store.finish(&repo.id, key, version, Ok(details()));

        assert!(
            store.cached(&repo).is_none(),
            "a result started before the operation must not fill the cache"
        );
        assert!(
            store.is_loading(&repo.id),
            "the row stays in its loading state until a fresh overview arrives"
        );
        assert!(!store.needs_load(&repo), "do not read against the old HEAD");
        store.observed(std::slice::from_ref(&repo.id));
        assert!(store.needs_load(&repo));
    }

    #[test]
    fn a_failed_detail_load_stays_visible_until_the_key_changes() {
        let repo = repo_with_commits(&["abc1234"]);
        let mut store = DetailsStore::default();
        let (key, version) = store.begin(&repo).expect("start the load");
        store.finish(
            &repo.id,
            key,
            version,
            Err("git for-each-ref failed".to_string()),
        );

        assert_eq!(store.error(&repo.id), Some("git for-each-ref failed"));
        assert!(
            !store.needs_load(&repo),
            "the same key is not retried in a loop; the error is shown instead"
        );

        let mut moved = repo;
        moved.head_sha = Some("def5678".to_string());
        assert!(
            store.needs_load(&moved),
            "a new HEAD retries the failed load"
        );
    }

    fn repo_with_commits(shas: &[&str]) -> Repository {
        let mut repo = repo_with(Vec::new());
        repo.recent_commits = shas
            .iter()
            .map(|sha| crate::model::CommitEntry {
                sha: sha.to_string(),
                full_sha: sha.to_string(),
                message: format!("subject of {sha}"),
                body: format!("body of {sha}"),
                author: "QA".to_string(),
                author_email: "qa@example.com".to_string(),
                relative_time: "just now".to_string(),
            })
            .collect();
        repo
    }

    fn branch(name: &str, is_current: bool, is_default: bool) -> BranchInfo {
        BranchInfo {
            name: name.to_string(),
            is_current,
            is_default,
            upstream: Some(format!("origin/{name}")),
            ahead: Some(0),
            behind: Some(0),
            last_commit_time: "just now".to_string(),
        }
    }

    #[test]
    fn branch_rows_expose_every_action_without_right_click() {
        let actions = branch_row_actions("repo-row", &branch("feature", false, false));
        assert_eq!(
            actions
                .iter()
                .map(BranchRowAction::label)
                .collect::<Vec<_>>(),
            vec![
                "Checkout",
                "New branch",
                "Merge into current",
                "Compare",
                "Delete"
            ],
            "checkout, create, merge, compare and delete are all visible in the row"
        );

        let current = branch_row_actions("repo-row", &branch("main", true, true));
        assert_eq!(
            current
                .iter()
                .map(BranchRowAction::label)
                .collect::<Vec<_>>(),
            vec!["New branch", "Rename"],
            "the checked-out branch is not offered a checkout against itself"
        );
    }

    #[test]
    fn branch_row_actions_keep_the_repository_they_were_rendered_for() {
        let actions = branch_row_actions("repo-opened", &branch("feature", false, false));
        assert!(
            actions
                .iter()
                .all(|action| action.repo_id() == "repo-opened"),
            "every row action names its own repository instead of the selected one"
        );

        let merge = actions
            .iter()
            .find(|action| matches!(action, BranchRowAction::Merge { .. }))
            .expect("merge action");
        assert_eq!(merge.repo_id(), "repo-opened");

        let prompts: Vec<BranchPromptKind> =
            actions.iter().filter_map(BranchRowAction::prompt).collect();
        assert_eq!(prompts.len(), 2, "create and delete need a prompt");
        assert_eq!(
            prompts[0],
            BranchPromptKind::Create {
                base: "feature".to_string()
            }
        );
        assert_eq!(
            prompts[1],
            BranchPromptKind::Delete {
                name: "feature".to_string()
            }
        );
    }

    #[test]
    fn delete_is_only_offered_for_a_branch_that_is_not_checked_out() {
        let other = branch_row_actions("repo-row", &branch("main", false, true));
        assert!(
            other
                .iter()
                .any(|action| matches!(action, BranchRowAction::Delete { .. })),
            "any non-current branch, including the default one, can be deleted after confirmation"
        );

        let current = branch_row_actions("repo-row", &branch("main", true, true));
        assert!(
            !current
                .iter()
                .any(|action| matches!(action, BranchRowAction::Delete { .. })),
            "the checked-out branch is never offered to itself for deletion"
        );
    }

    fn record(short: &str, previous_tip: Option<&str>) -> LastCommit {
        LastCommit {
            repo_id: "repo-1".to_string(),
            short: short.to_string(),
            summary: "Added commit ability".to_string(),
            previous_tip: previous_tip.map(str::to_string),
        }
    }

    #[test]
    fn undo_is_offered_for_a_local_commit_that_was_never_pushed() {
        let mut repo = repo_with_commits(&["abc1234", "older99"]);
        repo.ahead_count = 1;
        let offer = undo_offer(None, &repo).expect("an unpushed commit can be undone");
        assert_eq!(offer.summary, "subject of abc1234");
        assert!(!offer.just_now, "it was not made from this session");
        assert_eq!(offer.unpushed, 1);
    }

    #[test]
    fn undo_is_not_offered_when_every_commit_is_pushed() {
        let mut repo = repo_with_commits(&["abc1234"]);
        repo.ahead_count = 0;
        assert_eq!(undo_offer(None, &repo), None);
    }

    #[test]
    fn undo_is_offered_immediately_after_committing() {
        // The tip our row still shows; the record's own commit is newer.
        let mut repo = repo_with_commits(&["older99"]);
        repo.ahead_count = 0;
        let record = record("abc1234", Some("older99"));
        let offer = undo_offer(Some(&record), &repo).expect("own commit is undoable at once");
        assert!(offer.just_now);
        assert_eq!(offer.summary, "Added commit ability");
    }

    #[test]
    fn undo_uses_the_tip_subject_once_the_row_catches_up() {
        let mut repo = repo_with_commits(&["abc1234", "older99"]);
        repo.ahead_count = 1;
        let record = record("abc1234", Some("older99"));
        let offer = undo_offer(Some(&record), &repo).expect("tip is our commit");
        assert!(offer.just_now);
        assert_eq!(offer.summary, "subject of abc1234");
    }

    #[test]
    fn commits_from_another_repository_do_not_hide_a_local_commit() {
        let mut repo = repo_with_commits(&["abc1234"]);
        repo.id = "repo-2".to_string();
        repo.ahead_count = 2;
        let record = record("abc1234", None);
        let offer = undo_offer(Some(&record), &repo).expect("repo-2 has its own unpushed work");
        assert!(!offer.just_now);
        assert_eq!(offer.unpushed, 2);
    }

    fn overrides(pairs: &[(&str, bool)]) -> std::collections::HashMap<String, bool> {
        pairs
            .iter()
            .map(|(path, staged)| (path.to_string(), *staged))
            .collect()
    }

    #[test]
    fn attention_first_puts_the_loud_repositories_on_top() {
        let rank = |repo: &Repository| attention_rank(repo).0;

        let mut conflict = repo_with_commits(&["a"]);
        conflict.sync_state = RepoSyncState::Conflict;
        let mut unpushed = repo_with_commits(&["b"]);
        unpushed.ahead_count = 2;
        let mut dirty = repo_with_commits(&["c"]);
        dirty.uncommitted_files = vec![file("x.rs", false)];
        let mut behind = repo_with_commits(&["d"]);
        behind.behind_count = 1;
        let quiet = repo_with_commits(&["e"]);

        let mut ordered = [&quiet, &behind, &dirty, &unpushed, &conflict];
        ordered.sort_by_key(|repo| rank(repo));
        assert_eq!(
            ordered.map(|repo| repo.recent_commits[0].sha.as_str()),
            ["a", "b", "c", "d", "e"],
            "conflict, unpushed, changes, behind, quiet"
        );
    }

    #[test]
    fn commit_includes_the_files_git_reported_as_staged() {
        let repo = repo_with(vec![
            file("staged.rs", true),
            file("untracked.rs", false),
            file("also-staged.md", true),
        ]);
        assert_eq!(
            ticked_paths(None, &repo),
            vec!["staged.rs".to_string(), "also-staged.md".to_string()],
            "unstaged work must never be swept into a commit"
        );
    }

    #[test]
    fn a_tick_survives_a_scan_that_still_reports_the_file_unstaged() {
        // The tick was applied, the git write had not landed when the rescan
        // ran, and the row came back saying "unstaged". The commit must still
        // include it — this was the bug where ticked files stayed behind.
        let repo = repo_with(vec![file("late.rs", false)]);
        let chosen = overrides(&[("late.rs", true)]);
        assert_eq!(
            ticked_paths(Some(&chosen), &repo),
            vec!["late.rs".to_string()]
        );
    }

    #[test]
    fn an_untick_survives_a_scan_that_still_reports_the_file_staged() {
        let repo = repo_with(vec![file("staged.rs", true)]);
        let chosen = overrides(&[("staged.rs", false)]);
        assert!(ticked_paths(Some(&chosen), &repo).is_empty());
    }

    #[test]
    fn commit_with_nothing_ticked_yields_no_paths() {
        let repo = repo_with(vec![file("untracked.rs", false)]);
        assert!(ticked_paths(None, &repo).is_empty());
        assert!(ticked_paths(Some(&overrides(&[("untracked.rs", false)])), &repo).is_empty());
    }

    #[gpui::test]
    fn corrupt_state_warning_is_visible_and_lists_backups(cx: &mut TestAppContext) {
        let dir = temp_state_dir("corrupt");
        let path = dir.join("state.json");
        std::fs::write(&path, "{ not json").unwrap();

        let backend = Backend::with_state_path(path).expect("backend with a temp state file");
        let app = cx.new(|cx| VantageApp::with_backend(cx, backend));

        let warning = cx.read(|cx| app.read(cx).state_warning.clone());
        let warning = warning.expect("the startup warning must reach the app");
        assert!(
            warning.contains("could not read its saved state"),
            "{warning}"
        );

        let backup_count = cx.read(|cx| app.read(cx).state_backups.len());
        assert_eq!(
            backup_count, 1,
            "the corrupt file must be listed for recovery"
        );

        let (_root, cx) = cx.add_window_view(|_window, _cx| RootView::new(app.clone()));
        cx.update(|window, cx| {
            window.draw(cx).clear();
        });
        assert!(
            cx.debug_bounds("state-recovery-banner").is_some(),
            "a corrupt state must show a recovery notice, not a silent empty dashboard"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[gpui::test]
    fn warning_from_the_main_thread_is_kept_for_the_banner(cx: &mut TestAppContext) {
        let dir = temp_state_dir("main_warning");
        let backend = Backend::with_state_path(dir.join("state.json")).unwrap();
        let app = cx.new(|cx| {
            VantageApp::with_backend_and_warning(
                cx,
                backend,
                "state.json was corrupt; backup at /tmp/state.corrupt-1.json".to_string(),
            )
        });

        let warning = cx.read(|cx| app.read(cx).state_warning.clone());
        assert_eq!(
            warning.as_deref(),
            Some("state.json was corrupt; backup at /tmp/state.corrupt-1.json")
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[gpui::test]
    fn failed_workspace_save_is_reported_instead_of_succeeding_silently(cx: &mut TestAppContext) {
        let dir = temp_state_dir("failed_save");
        let blocker = dir.join("not-a-directory");
        std::fs::write(&blocker, "blocked").unwrap();
        let backend = Backend::with_state_path(blocker.join("state.json"))
            .expect("backend with an unwritable state path");
        let app = cx.new(|cx| VantageApp::with_backend(cx, backend));

        let saved = cx.update(|cx| app.update(cx, |app, cx| app.persist(cx)));
        assert!(!saved, "persist must report a failed write");
        let toast = cx
            .read(|cx| {
                app.read(cx)
                    .toast
                    .as_ref()
                    .map(|toast| toast.message.clone())
            })
            .unwrap_or_default();
        assert!(
            toast.contains("not saved") && toast.contains("save the workspace state"),
            "the failed save must be visible: {toast}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn composer_message_is_isolated_per_repository() {
        let mut composers = ComposerStore::default();
        composers.switch_to("alpha");
        composers.active_mut().message.set("alpha work");
        composers.active_mut().body.set("details for alpha");

        composers.switch_to("beta");
        assert!(
            composers.active().message.value.is_empty(),
            "beta must not inherit alpha's message"
        );
        composers.active_mut().message.set("beta work");

        composers.switch_to("alpha");
        assert_eq!(composers.active().message.value, "alpha work");
        assert_eq!(composers.active().body.value, "details for alpha");

        composers.switch_to("beta");
        assert_eq!(
            composers.active().message.value,
            "beta work",
            "switching back restores the unfinished draft"
        );
    }

    #[test]
    fn reset_confirmation_states_the_hard_reset_contract() {
        assert_eq!(
            reset_confirmation_body("c0ffee1"),
            "The branch tip and tracked files move to c0ffee1. A clean working tree is required: \
             uncommitted and untracked changes are refused, never discarded. A backup ref of the \
             current tip is kept first."
        );
    }

    #[test]
    fn a_dirty_tree_is_refused_before_the_reset_confirmation() {
        let repo = repo_with(vec![file("src/a.rs", false), file("notes.md", false)]);
        assert_eq!(
            reset_refusal(&repo),
            Some(
                "Cannot reset main: 2 uncommitted changes in the working tree (first: src/a.rs). \
                 Commit or discard them first."
                    .to_string()
            )
        );
    }

    #[test]
    fn amend_mode_never_survives_a_repository_switch() {
        let mut composers = ComposerStore::default();
        composers.switch_to("alpha");
        composers.active_mut().message.set("alpha work");
        composers.active_mut().amending = true;
        composers.active_mut().base_tip = Some("abc1234".to_string());

        composers.switch_to("beta");
        assert!(!composers.active().amending);
        assert_eq!(composers.active().base_tip, None);

        composers.switch_to("alpha");
        assert!(
            !composers.active().amending,
            "returning to alpha must not resurrect its amend mode"
        );
        assert_eq!(composers.active().base_tip, None);
        assert_eq!(
            composers.active().message.value,
            "alpha work",
            "the plain draft still comes back"
        );
    }

    #[test]
    fn a_normal_commit_targets_the_selected_repository_and_its_tip() {
        let repo = repo_with_commits(&["abc1234"]);
        let mut composer = ComposerState::default();
        composer.message.set("Fix the thing");

        let target = composer.commit_target(&repo);
        assert_eq!(target.repo_id, "repo-1");
        assert_eq!(target.expected_head.as_deref(), Some("abc1234"));
        assert_eq!(
            target.refusal("repo-1", Some("abc1234deadbeef")),
            None,
            "the abbreviated row tip matches the full HEAD"
        );
    }

    #[test]
    fn a_commit_is_refused_when_the_repository_or_head_moved() {
        let repo = repo_with_commits(&["abc1234"]);
        let mut composer = ComposerState::default();
        composer.message.set("Fix the thing");
        let target = composer.commit_target(&repo);

        assert!(
            target.refusal("repo-2", Some("abc1234deadbeef")).is_some(),
            "a job queued for repo-1 must never write to repo-2"
        );
        let refusal = target
            .refusal("repo-1", Some("def5678deadbeef"))
            .expect("a moved HEAD must refuse the write");
        assert!(refusal.contains("different commit"), "{refusal}");
    }

    #[test]
    fn an_amend_targets_the_tip_captured_when_amend_mode_started() {
        let repo = repo_with_commits(&["tip9999"]);
        let composer = ComposerState {
            amending: true,
            base_tip: Some("older11".to_string()),
            ..ComposerState::default()
        };

        let target = composer.commit_target(&repo);
        assert_eq!(target.expected_head.as_deref(), Some("older11"));
        assert!(
            target.refusal("repo-1", Some("tip9999")).is_some(),
            "an amend queued before the tip moved must not rewrite the new tip"
        );
    }

    #[test]
    fn a_successful_commit_spends_only_its_repositorys_draft() {
        let mut composers = ComposerStore::default();
        composers.switch_to("alpha");
        composers.active_mut().message.set("alpha work");
        composers.switch_to("beta");
        composers.active_mut().message.set("beta work");

        composers.clear_for("beta");
        assert!(composers.active().message.value.is_empty());

        composers.switch_to("alpha");
        assert_eq!(
            composers.active().message.value,
            "alpha work",
            "clearing beta's composer must not touch alpha's draft"
        );
    }

    #[test]
    fn amend_does_not_require_a_ticked_file_but_a_commit_does() {
        assert!(
            !commit_needs_selected_paths(true),
            "message-only amend must work on a clean tree"
        );
        assert!(
            commit_needs_selected_paths(false),
            "a normal commit still needs at least one file"
        );
    }

    fn repo_named(id: &str, name: &str) -> Repository {
        let mut repo = repo_with(Vec::new());
        repo.id = id.to_string();
        repo.name = name.to_string();
        repo
    }

    fn core_branch(name: &str, is_head: bool) -> vantage_core::state::BranchInfo {
        vantage_core::state::BranchInfo {
            name: name.to_string(),
            upstream: None,
            is_head,
            updated_at: None,
        }
    }

    #[test]
    fn create_prompt_refuses_a_base_branch_that_disappeared() {
        let error = validate_branch_prompt(
            &BranchPromptKind::Create {
                base: "main".to_string(),
            },
            &[core_branch("feature", false)],
        )
        .expect_err("a missing base branch must be refused");
        assert!(error.contains("'main' no longer exists"), "{error}");
    }

    #[test]
    fn rename_prompt_refuses_a_branch_that_is_no_longer_current() {
        let error = validate_branch_prompt(
            &BranchPromptKind::Rename {
                from: "main".to_string(),
            },
            &[core_branch("main", false), core_branch("other", true)],
        )
        .expect_err("a branch that is no longer checked out must not be renamed");
        assert!(
            error.contains("no longer the checked-out branch"),
            "{error}"
        );
    }

    #[test]
    fn delete_prompt_refuses_a_branch_that_is_now_current() {
        let error = validate_branch_prompt(
            &BranchPromptKind::Delete {
                name: "feature".to_string(),
            },
            &[core_branch("feature", true)],
        )
        .expect_err("git refuses to delete the checked-out branch, so say so first");
        assert!(error.contains("now the checked-out branch"), "{error}");
    }

    #[gpui::test]
    fn branch_prompt_keeps_its_repository_after_the_selection_changes(
        cx: &mut gpui::TestAppContext,
    ) {
        use gpui::AppContext;

        let app = cx.new(VantageApp::new);
        cx.update(|cx| {
            app.update(cx, |app, cx| {
                app.repos = vec![repo_named("repo-1", "alpha"), repo_named("repo-2", "beta")];
                app.selected_repo_id = Some("repo-1".to_string());
                app.start_branch_prompt(
                    "repo-1",
                    BranchPromptKind::Delete {
                        name: "feature".to_string(),
                    },
                    cx,
                );
                assert_eq!(
                    app.branch_prompt
                        .as_ref()
                        .map(|prompt| prompt.repo_id.as_str()),
                    Some("repo-1")
                );

                // The user moves to another repository before answering.
                app.selected_repo_id = Some("repo-2".to_string());
                app.submit_branch_prompt(cx);

                assert!(
                    app.busy_repos.contains_key("repo-1"),
                    "the delete must run against the repository that opened the prompt"
                );
                assert!(
                    !app.busy_repos.contains_key("repo-2"),
                    "the selection at submit time must not receive the operation"
                );
                assert!(
                    app.branch_prompt.is_some(),
                    "the delete prompt stays open until git answers or asks for force confirmation"
                );
            });
        });
    }

    #[gpui::test]
    fn branch_prompt_refuses_a_repository_that_is_gone(cx: &mut gpui::TestAppContext) {
        use gpui::AppContext;

        let app = cx.new(VantageApp::new);
        cx.update(|cx| {
            app.update(cx, |app, cx| {
                app.repos = vec![repo_named("repo-2", "beta")];
                app.selected_repo_id = Some("repo-2".to_string());
                app.start_branch_prompt(
                    "repo-1",
                    BranchPromptKind::Delete {
                        name: "feature".to_string(),
                    },
                    cx,
                );

                app.submit_branch_prompt(cx);

                assert!(
                    app.busy_repos.is_empty(),
                    "nothing may run when the prompt's repository is gone"
                );
                assert!(app.branch_prompt.is_none());
                let toast = app
                    .toast
                    .as_ref()
                    .map(|toast| toast.message.clone())
                    .unwrap_or_default();
                assert!(
                    toast.contains("no longer in the workspace"),
                    "expected a target error, got {toast:?}"
                );
            });
        });
    }

    #[test]
    fn a_clean_tree_can_reach_the_reset_confirmation() {
        assert_eq!(reset_refusal(&repo_with(Vec::new())), None);
    }

    // ---- keyboard context and dialog focus -------------------------------

    fn list_repo(id: &str, name: &str, minutes: i64) -> Repository {
        let mut repo = repo_with(vec![file("src/lib.rs", false)]);
        repo.id = id.to_string();
        repo.name = name.to_string();
        repo.last_activity_minutes = minutes;
        repo
    }

    fn open_test_window(
        cx: &mut TestAppContext,
        repos: Vec<Repository>,
    ) -> (gpui::Entity<VantageApp>, &mut VisualTestContext) {
        let app = cx.new(VantageApp::new);
        let (_root, window) = cx.add_window_view(|_window, _cx| RootView::new(app.clone()));
        window.update(|_window, cx| crate::menu::install(cx, app.clone()));
        window.update(|_window, cx| {
            app.update(cx, |app, cx| {
                app.repos = repos;
                app.selected_repo_id = None;
                app.settings.right_panel_open = false;
                cx.notify();
            });
        });
        (app, window)
    }

    fn draw(window: &mut VisualTestContext) {
        window.update(|window, cx| {
            window.draw(cx).clear();
        });
    }

    fn focused(window: &mut VisualTestContext, handle: &FocusHandle) -> bool {
        window.update(|window, _cx| handle.is_focused(window))
    }

    fn selected(window: &mut VisualTestContext, app: &gpui::Entity<VantageApp>) -> Option<String> {
        window.update(|_window, cx| app.read(cx).selected_repo_id.clone())
    }

    #[gpui::test]
    fn arrows_in_a_text_field_leave_the_repository_selection_alone(cx: &mut TestAppContext) {
        let (app, window) = open_test_window(
            cx,
            vec![
                list_repo("repo-1", "alpha", 0),
                list_repo("repo-2", "beta", 1),
            ],
        );
        window.update(|_window, cx| {
            app.update(cx, |app, cx| {
                app.selected_repo_id = Some("repo-1".to_string());
                app.settings.right_panel_open = true;
                cx.notify();
            });
        });
        draw(window);

        let message = window.update(|_window, cx| app.read(cx).field_focus(FieldId::CommitMessage));
        window.update(|window, _cx| window.focus(&message));
        draw(window);
        assert!(focused(window, &message), "the message field is focused");

        window.simulate_keystrokes("down up");
        assert_eq!(
            selected(window, &app),
            Some("repo-1".to_string()),
            "arrow keys while editing must not walk the repository list"
        );

        window.simulate_keystrokes("a->a b->b");
        assert_eq!(
            window.update(|_window, cx| app.read(cx).composer().message.value.clone()),
            "ab",
            "typed characters land in the focused field"
        );
        assert_eq!(
            selected(window, &app),
            Some("repo-1".to_string()),
            "typing never changes the selection"
        );
    }

    #[gpui::test]
    fn enter_in_a_modal_field_does_not_open_the_repository(cx: &mut TestAppContext) {
        let (app, window) = open_test_window(
            cx,
            vec![
                list_repo("repo-1", "alpha", 0),
                list_repo("repo-2", "beta", 1),
            ],
        );
        window.update(|_window, cx| {
            app.update(cx, |app, cx| {
                app.selected_repo_id = Some("repo-1".to_string());
                app.open_add_modal(cx);
            });
        });
        draw(window);

        let path = window.update(|_window, cx| app.read(cx).field_focus(FieldId::ModalPath));
        assert!(focused(window, &path), "the path field opens focused");

        window.simulate_keystrokes("enter");
        let (error, panel_open) = window.update(|_window, cx| {
            let app = app.read(cx);
            (app.modal_error.clone(), app.settings.right_panel_open)
        });
        assert!(
            error.is_some(),
            "Enter belongs to the field (it asks for a path) and must not open a repository"
        );
        assert!(!panel_open, "no repository view action ran");
    }

    #[gpui::test]
    fn repository_list_keeps_arrow_and_enter_navigation(cx: &mut TestAppContext) {
        let (app, window) = open_test_window(
            cx,
            vec![
                list_repo("repo-1", "alpha", 0),
                list_repo("repo-2", "beta", 1),
                list_repo("repo-3", "gamma", 2),
            ],
        );
        draw(window);

        let list = window.update(|_window, cx| app.read(cx).repo_list_focus());
        assert!(
            focused(window, &list),
            "an empty selection focuses the list so the keyboard works"
        );

        window.simulate_keystrokes("down");
        assert_eq!(selected(window, &app), Some("repo-1".to_string()));
        window.simulate_keystrokes("down");
        assert_eq!(selected(window, &app), Some("repo-2".to_string()));
        window.simulate_keystrokes("up");
        assert_eq!(selected(window, &app), Some("repo-1".to_string()));

        window.simulate_keystrokes("enter");
        assert!(
            window.update(|_window, cx| app.read(cx).settings.right_panel_open),
            "Enter opens the highlighted repository"
        );
    }

    #[gpui::test]
    fn clicking_a_row_establishes_the_list_keyboard_context(cx: &mut TestAppContext) {
        let (app, window) = open_test_window(
            cx,
            vec![
                list_repo("repo-1", "alpha", 0),
                list_repo("repo-2", "beta", 1),
                list_repo("repo-3", "gamma", 2),
            ],
        );
        draw(window);

        let selector: &'static str = Box::leak(format!("repo-row-{}", "repo-2").into_boxed_str());
        let row = window
            .debug_bounds(selector)
            .expect("the second row is drawn");
        window.simulate_click(row.center(), Modifiers::none());

        assert_eq!(selected(window, &app), Some("repo-2".to_string()));
        let list = window.update(|_window, cx| app.read(cx).repo_list_focus());
        assert!(
            focused(window, &list),
            "clicking a row makes the list the active keyboard context"
        );

        window.simulate_keystrokes("down");
        assert_eq!(
            selected(window, &app),
            Some("repo-3".to_string()),
            "the list keeps working after a row click"
        );
    }

    #[gpui::test]
    fn each_dialog_focuses_its_first_control(cx: &mut TestAppContext) {
        let (app, window) = open_test_window(cx, vec![list_repo("repo-1", "alpha", 0)]);
        draw(window);

        let handle = |window: &mut VisualTestContext, id| {
            window.update(|_window, cx| app.read(cx).field_focus(id))
        };
        let card = window.update(|_window, cx| app.read(cx).modal_focus.clone());

        window.update(|_window, cx| {
            app.update(cx, |app, cx| app.open_new_repo_modal(cx));
        });
        draw(window);
        let name = handle(window, FieldId::NewRepoName);
        assert!(focused(window, &name));

        window.update(|_window, cx| {
            app.update(cx, |app, cx| app.close_new_repo_modal(cx));
            app.update(cx, |app, cx| app.open_clone_modal(cx));
        });
        draw(window);
        let url = handle(window, FieldId::CloneUrl);
        assert!(focused(window, &url));

        window.update(|_window, cx| {
            app.update(cx, |app, cx| {
                app.close_clone_modal(cx);
                app.start_branch_prompt(
                    "repo-1",
                    BranchPromptKind::Create {
                        base: "main".to_string(),
                    },
                    cx,
                );
            });
        });
        draw(window);
        let branch = handle(window, FieldId::BranchName);
        assert!(focused(window, &branch));

        window.update(|_window, cx| {
            app.update(cx, |app, cx| {
                app.start_branch_prompt(
                    "repo-1",
                    BranchPromptKind::Delete {
                        name: "old".to_string(),
                    },
                    cx,
                );
            });
        });
        draw(window);
        assert!(
            focused(window, &card),
            "a prompt with no field focuses its card"
        );

        window.update(|_window, cx| {
            app.update(cx, |app, cx| {
                app.cancel_branch_prompt(cx);
                app.confirm = Some(ConfirmPrompt {
                    title: "Remove?".to_string(),
                    body: "Nothing is deleted.".to_string(),
                    label: "Remove",
                    action: ConfirmAction::ForgetRepository {
                        repo_id: "repo-1".to_string(),
                    },
                });
                cx.notify();
            });
        });
        draw(window);
        assert!(focused(window, &card));
    }

    #[gpui::test]
    fn shortcut_opens_the_add_dialog_with_the_path_focused(cx: &mut TestAppContext) {
        let (app, window) = open_test_window(cx, vec![list_repo("repo-1", "alpha", 0)]);
        draw(window);

        window.simulate_keystrokes("cmd-o");
        draw(window);

        let path = window.update(|_window, cx| app.read(cx).field_focus(FieldId::ModalPath));
        assert!(
            window.update(|_window, cx| app.read(cx).is_add_modal_open),
            "the shortcut opens the dialog"
        );
        assert!(
            focused(window, &path),
            "the first field takes focus when the dialog opens"
        );
    }

    #[gpui::test]
    fn toolbar_opens_the_add_dialog_with_the_path_focused(cx: &mut TestAppContext) {
        let (app, window) = open_test_window(cx, vec![list_repo("repo-1", "alpha", 0)]);
        draw(window);

        let button = window
            .debug_bounds("add-repository")
            .expect("toolbar button");
        window.simulate_click(button.center(), Modifiers::none());
        draw(window);

        let path = window.update(|_window, cx| app.read(cx).field_focus(FieldId::ModalPath));
        assert!(window.update(|_window, cx| app.read(cx).is_add_modal_open));
        assert!(focused(window, &path));
    }

    #[gpui::test]
    fn menu_action_opens_the_add_dialog_with_the_path_focused(cx: &mut TestAppContext) {
        let (app, window) = open_test_window(cx, vec![list_repo("repo-1", "alpha", 0)]);
        draw(window);

        window.dispatch_action(crate::menu::AddRepository);
        draw(window);

        let path = window.update(|_window, cx| app.read(cx).field_focus(FieldId::ModalPath));
        assert!(window.update(|_window, cx| app.read(cx).is_add_modal_open));
        assert!(focused(window, &path));
    }

    #[gpui::test]
    fn escape_closes_a_dialog_from_a_non_field_control(cx: &mut TestAppContext) {
        let (app, window) = open_test_window(cx, vec![list_repo("repo-1", "alpha", 0)]);
        draw(window);

        window.dispatch_action(crate::menu::Settings);
        draw(window);
        let card = window.update(|_window, cx| app.read(cx).modal_focus.clone());
        assert!(
            focused(window, &card),
            "Settings with no field focuses its card"
        );

        window.simulate_keystrokes("escape");
        assert!(
            !window.update(|_window, cx| app.read(cx).is_settings_open),
            "Escape closes the dialog even though no text field is focused"
        );
    }

    #[gpui::test]
    fn tab_cycles_inside_an_open_dialog(cx: &mut TestAppContext) {
        let (app, window) = open_test_window(cx, vec![list_repo("repo-1", "alpha", 0)]);
        draw(window);

        window.dispatch_action(crate::menu::CloneRepository);
        draw(window);
        let url = window.update(|_window, cx| app.read(cx).field_focus(FieldId::CloneUrl));
        let parent = window.update(|_window, cx| app.read(cx).field_focus(FieldId::CloneParent));
        assert!(focused(window, &url));

        window.simulate_keystrokes("tab");
        assert!(focused(window, &parent), "Tab moves to the next field");
        window.simulate_keystrokes("tab");
        assert!(
            focused(window, &url),
            "Tab wraps without leaving the dialog"
        );
        window.simulate_keystrokes("shift-tab");
        assert!(
            focused(window, &parent),
            "Shift+Tab walks back inside the dialog"
        );
        assert!(window.update(|_window, cx| app.read(cx).is_clone_modal_open));
    }

    #[gpui::test]
    fn closing_a_dialog_restores_the_previous_field(cx: &mut TestAppContext) {
        let (app, window) = open_test_window(cx, vec![list_repo("repo-1", "alpha", 0)]);
        window.update(|_window, cx| {
            app.update(cx, |app, cx| {
                app.selected_repo_id = Some("repo-1".to_string());
                app.settings.right_panel_open = true;
                cx.notify();
            });
        });
        draw(window);

        let message = window.update(|_window, cx| app.read(cx).field_focus(FieldId::CommitMessage));
        window.update(|window, _cx| window.focus(&message));
        draw(window);
        assert!(focused(window, &message));

        window.dispatch_action(crate::menu::Settings);
        draw(window);
        assert!(window.update(|_window, cx| app.read(cx).is_settings_open));

        window.simulate_keystrokes("escape");
        draw(window);
        assert!(!window.update(|_window, cx| app.read(cx).is_settings_open));
        assert!(
            focused(window, &message),
            "closing the dialog puts the caret back where it was"
        );
    }

    #[gpui::test]
    fn new_group_reveals_with_its_input_focused(cx: &mut TestAppContext) {
        let (app, window) = open_test_window(cx, vec![list_repo("repo-1", "alpha", 0)]);
        draw(window);

        window.update(|_window, cx| {
            app.update(cx, |app, cx| app.toggle_new_group(cx));
        });
        draw(window);

        let group = window.update(|_window, cx| app.read(cx).field_focus(FieldId::NewGroup));
        assert!(window.update(|_window, cx| app.read(cx).is_adding_group));
        assert!(
            focused(window, &group),
            "the New Group input is ready for typing"
        );
    }

    #[gpui::test]
    fn left_right_arrows_still_edit_inside_the_commit_message(cx: &mut TestAppContext) {
        let (app, window) = open_test_window(cx, vec![list_repo("repo-1", "alpha", 0)]);
        window.update(|_window, cx| {
            app.update(cx, |app, cx| {
                app.selected_repo_id = Some("repo-1".to_string());
                app.settings.right_panel_open = true;
                app.composers
                    .active_mut()
                    .message
                    .set("subject".to_string());
                cx.notify();
            });
        });
        draw(window);

        let message = window.update(|_window, cx| app.read(cx).field_focus(FieldId::CommitMessage));
        window.update(|window, _cx| window.focus(&message));
        draw(window);

        window.simulate_keystrokes("left left");
        assert_eq!(
            window.update(|_window, cx| app.read(cx).composer().message.caret),
            5,
            "left/right still edit inside the field"
        );
        assert_eq!(selected(window, &app), Some("repo-1".to_string()));
    }

    #[gpui::test]
    fn checkbox_hit_target_is_at_least_32_square(cx: &mut gpui::TestAppContext) {
        use gpui::AppContext;

        use crate::root::RootView;

        let app = cx.new(VantageApp::new);
        let (_root, cx) = cx.add_window_view(|_window, _cx| RootView::new(app.clone()));

        cx.update(|window, cx| {
            app.update(cx, |app, cx| app.open_new_repo_modal(cx));
            window.draw(cx).clear();
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear();
        });

        let bounds = cx
            .debug_bounds("new-repo-gitignore-target")
            .expect("the checkbox should be rendered in the New Repository dialog");
        assert!(
            f32::from(bounds.size.width) >= 32.0 && f32::from(bounds.size.height) >= 32.0,
            "checkbox target is {}×{}, expected at least 32×32",
            f32::from(bounds.size.width),
            f32::from(bounds.size.height)
        );
    }

    #[test]
    fn success_toasts_keep_the_short_lifetime() {
        let toast = Toast::info("Repository status refreshed");
        assert_eq!(toast.auto_hide_after(), Some(TOAST_INFO_DURATION));
    }

    #[test]
    fn short_failures_stay_up_longer_than_successes() {
        let toast = Toast::error("Clone failed: authentication required");
        assert_eq!(toast.auto_hide_after(), Some(TOAST_ERROR_DURATION));
        assert!(TOAST_ERROR_DURATION > TOAST_INFO_DURATION);
    }

    #[test]
    fn multi_line_and_long_failures_wait_for_dismissal() {
        assert_eq!(
            Toast::error("error: failed to push\nhint: fetch first").auto_hide_after(),
            None
        );

        let long = "error: ".to_string() + &"detail ".repeat(600);
        assert!(Toast::error(&long).is_detailed());
        assert_eq!(Toast::error(&long).auto_hide_after(), None);
    }

    #[test]
    fn a_long_git_error_keeps_full_copy_text_but_shows_a_bounded_preview() {
        let message = long_git_error();
        let toast = Toast::error(message.clone());

        assert!(toast.is_detailed());
        let preview = toast.preview();
        assert!(preview.chars().count() <= TOAST_PREVIEW_CHARS);
        assert!(preview.lines().count() <= TOAST_PREVIEW_LINES + 1);
        assert!(preview.ends_with('…'));
        assert_eq!(toast.copy_text(), message);
    }

    #[test]
    fn a_single_long_line_is_truncated_to_the_preview_budget() {
        let preview = toast_preview(&"x".repeat(4096));
        assert_eq!(preview.chars().count(), TOAST_PREVIEW_CHARS);
        assert!(preview.ends_with('…'));
        assert!(!preview.contains('\n'));
    }

    #[test]
    fn a_multi_line_error_preview_keeps_the_first_lines_and_marks_the_cut() {
        let preview = toast_preview("error: first\nhint: second\nhint: third\nhint: fourth");
        assert!(preview.starts_with("error: first\nhint: second\nhint: third"));
        assert!(preview.ends_with("\n…"));
        assert_eq!(preview.lines().count(), TOAST_PREVIEW_LINES + 1);
    }

    #[test]
    fn a_short_message_is_shown_in_full() {
        let toast = Toast::info("Saved");
        assert!(!toast.is_detailed());
        assert_eq!(toast.preview(), "Saved");
        assert_eq!(toast.copy_text(), "Saved");
    }

    #[test]
    fn toast_width_stays_inside_the_minimum_window() {
        assert!((420.0..=480.0).contains(&TOAST_MAX_WIDTH));

        let width = toast_max_width(1040.0);
        assert_eq!(width, TOAST_MAX_WIDTH);
        assert!(width + 2.0 * TOAST_EDGE_MARGIN <= 1040.0);

        let narrow = toast_max_width(320.0);
        assert_eq!(narrow, 320.0 - 2.0 * TOAST_EDGE_MARGIN);
        assert!(narrow + 2.0 * TOAST_EDGE_MARGIN <= 320.0);
    }

    fn long_git_error() -> String {
        let mut message = String::from("error: failed to push some refs to 'origin'\n");
        for index in 0..80 {
            message.push_str(&format!(
                "hint: updates were rejected because the remote contains work {index}\n"
            ));
        }
        message
    }

    fn file_menu() -> ContextMenu {
        ContextMenu::File {
            repo_id: "repo-1".to_string(),
            path: "src/lib.rs".to_string(),
            x: 20.0,
            y: 40.0,
        }
    }

    fn repo_menu(with_changes: bool, uncommitted: usize, stashes: usize) -> ContextMenu {
        ContextMenu::Repo {
            repo_id: "repo-1".to_string(),
            name: "fixture".to_string(),
            uncommitted,
            stashes,
            with_changes,
            x: 20.0,
            y: 40.0,
        }
    }

    fn branch_menu(is_current: bool, is_default: bool) -> ContextMenu {
        ContextMenu::Branch {
            repo_id: "repo-1".to_string(),
            name: "feature".to_string(),
            is_current,
            is_default,
            x: 20.0,
            y: 40.0,
        }
    }

    fn commit_menu(is_tip: bool) -> ContextMenu {
        ContextMenu::Commit {
            repo_id: "repo-1".to_string(),
            short: "abc1234".to_string(),
            subject: "Tighten the menu".to_string(),
            body: "The original description.".to_string(),
            is_tip,
            x: 20.0,
            y: 40.0,
        }
    }

    fn action_rows(menu: &ContextMenu) -> Vec<ContextAction> {
        menu.rows()
            .into_iter()
            .map(|(_, action, _)| action)
            .collect()
    }

    fn styled_labels(menu: &ContextMenu) -> Vec<(&'static str, bool)> {
        menu.rows()
            .into_iter()
            .map(|(label, _, destructive)| (label, destructive))
            .collect()
    }

    #[test]
    fn every_context_menu_variant_carries_typed_actions() {
        // One menu for every variant and every state that changes its rows.
        // `run_context_action` matches every `ContextAction` without a
        // catch-all, so a row that is generated here is always dispatchable.
        let menus = [
            file_menu(),
            repo_menu(true, 2, 1),
            repo_menu(false, 0, 1),
            branch_menu(false, false),
            branch_menu(true, false),
            branch_menu(false, true),
            commit_menu(true),
            commit_menu(false),
        ];

        for menu in &menus {
            let rows = menu.rows();
            assert!(!rows.is_empty(), "every context menu offers an action");
            for (label, action, destructive) in rows {
                assert_eq!(label, action.label(), "labels are derived from the action");
                assert_eq!(destructive, action.is_destructive());
                assert_eq!(action.repo_id(), "repo-1", "the row targets its own menu");
            }
        }
    }

    #[test]
    fn file_menu_offers_ignore_and_discard_in_order() {
        let menu = file_menu();
        assert_eq!(
            styled_labels(&menu),
            vec![("Add to .gitignore", false), ("Discard changes", true)]
        );
        assert_eq!(
            action_rows(&menu),
            vec![
                ContextAction::IgnoreFile {
                    repo_id: "repo-1".to_string(),
                    path: "src/lib.rs".to_string(),
                },
                ContextAction::DiscardFile {
                    repo_id: "repo-1".to_string(),
                    path: "src/lib.rs".to_string(),
                },
            ]
        );
    }

    #[test]
    fn changes_header_menu_offers_stash_restore_and_discard() {
        let menu = repo_menu(true, 2, 1);
        assert_eq!(
            styled_labels(&menu),
            vec![
                ("Stash all changes", false),
                ("Restore latest stash", false),
                ("Discard all changes", true),
            ]
        );
        assert_eq!(
            action_rows(&menu),
            vec![
                ContextAction::StashAll {
                    repo_id: "repo-1".to_string(),
                },
                ContextAction::RestoreStash {
                    repo_id: "repo-1".to_string(),
                },
                ContextAction::DiscardRepo {
                    repo_id: "repo-1".to_string(),
                },
            ]
        );
    }

    #[test]
    fn repository_menu_keeps_its_action_order() {
        let menu = repo_menu(false, 0, 1);
        assert_eq!(
            styled_labels(&menu),
            vec![
                ("Update from default branch", false),
                ("Manage stashes…", false),
                ("View on GitHub", false),
                ("Remove from list", true),
                ("Reveal in Finder", false),
                ("Open in Terminal", false),
                ("Open in editor", false),
            ]
        );
        assert_eq!(
            action_rows(&menu),
            vec![
                ContextAction::UpdateFromDefault {
                    repo_id: "repo-1".to_string(),
                },
                ContextAction::ManageStashes {
                    repo_id: "repo-1".to_string(),
                },
                ContextAction::ViewOnGitHub {
                    repo_id: "repo-1".to_string(),
                },
                ContextAction::RemoveFromList {
                    repo_id: "repo-1".to_string(),
                },
                ContextAction::RevealInFinder {
                    repo_id: "repo-1".to_string(),
                },
                ContextAction::OpenInTerminal {
                    repo_id: "repo-1".to_string(),
                },
                ContextAction::OpenInEditor {
                    repo_id: "repo-1".to_string(),
                },
            ]
        );

        let without_stashes = repo_menu(false, 0, 0);
        assert!(
            !action_rows(&without_stashes).contains(&ContextAction::ManageStashes {
                repo_id: "repo-1".to_string(),
            })
        );
    }

    #[test]
    fn branch_menu_actions_follow_current_and_default_state() {
        let other = branch_menu(false, false);
        assert_eq!(
            styled_labels(&other),
            vec![
                ("Checkout branch", false),
                ("Merge into current branch", false),
                ("Compare with current", false),
                ("New branch from this branch", false),
                ("Delete branch", true),
            ]
        );
        assert_eq!(
            action_rows(&other),
            vec![
                ContextAction::CheckoutBranch {
                    repo_id: "repo-1".to_string(),
                    branch: "feature".to_string(),
                },
                ContextAction::MergeBranch {
                    repo_id: "repo-1".to_string(),
                    branch: "feature".to_string(),
                },
                ContextAction::CompareWithCurrent {
                    repo_id: "repo-1".to_string(),
                    branch: "feature".to_string(),
                },
                ContextAction::NewBranchFrom {
                    repo_id: "repo-1".to_string(),
                    base: "feature".to_string(),
                },
                ContextAction::DeleteBranch {
                    repo_id: "repo-1".to_string(),
                    name: "feature".to_string(),
                },
            ]
        );

        let current = branch_menu(true, false);
        assert_eq!(
            action_rows(&current),
            vec![
                ContextAction::NewBranchFrom {
                    repo_id: "repo-1".to_string(),
                    base: "feature".to_string(),
                },
                ContextAction::RenameCurrentBranch {
                    repo_id: "repo-1".to_string(),
                    from: "feature".to_string(),
                },
            ]
        );

        let default = branch_menu(false, true);
        assert_eq!(
            action_rows(&default),
            vec![
                ContextAction::CheckoutBranch {
                    repo_id: "repo-1".to_string(),
                    branch: "feature".to_string(),
                },
                ContextAction::MergeBranch {
                    repo_id: "repo-1".to_string(),
                    branch: "feature".to_string(),
                },
                ContextAction::CompareWithCurrent {
                    repo_id: "repo-1".to_string(),
                    branch: "feature".to_string(),
                },
                ContextAction::NewBranchFrom {
                    repo_id: "repo-1".to_string(),
                    base: "feature".to_string(),
                },
            ]
        );
    }

    #[test]
    fn commit_menu_appends_amend_for_the_tip() {
        let tip = commit_menu(true);
        assert_eq!(
            styled_labels(&tip),
            vec![
                ("Revert commit", false),
                ("Reset branch here", true),
                ("Amend commit", false),
            ]
        );
        assert_eq!(
            action_rows(&tip),
            vec![
                ContextAction::RevertCommit {
                    repo_id: "repo-1".to_string(),
                    sha: "abc1234".to_string(),
                },
                ContextAction::ResetBranchHere {
                    repo_id: "repo-1".to_string(),
                    sha: "abc1234".to_string(),
                },
                ContextAction::AmendCommit {
                    repo_id: "repo-1".to_string(),
                    subject: "Tighten the menu".to_string(),
                    body: "The original description.".to_string(),
                },
            ]
        );

        let older = commit_menu(false);
        assert_eq!(
            action_rows(&older),
            vec![
                ContextAction::RevertCommit {
                    repo_id: "repo-1".to_string(),
                    sha: "abc1234".to_string(),
                },
                ContextAction::ResetBranchHere {
                    repo_id: "repo-1".to_string(),
                    sha: "abc1234".to_string(),
                },
            ]
        );
    }

    #[test]
    fn clone_cancellation_is_reported_as_a_cancelled_state() {
        let cancelled = anyhow::Error::new(vantage_core::AppError::cancelled());
        assert_eq!(clone_failure(&cancelled), CloneFailure::Cancelled);

        let failed = anyhow::Error::new(vantage_core::AppError::other("disk full"));
        assert_eq!(
            clone_failure(&failed),
            CloneFailure::Failed("Clone failed: disk full".to_string())
        );
    }

    #[gpui::test]
    fn clone_cancel_button_stops_the_clone_without_closing_the_dialog(
        cx: &mut gpui::TestAppContext,
    ) {
        use gpui::{AppContext, Modifiers};

        let app = cx.new(VantageApp::new);
        let (_root, cx) =
            cx.add_window_view(|_window, _cx| crate::root::RootView::new(app.clone()));
        let cancel = vantage_core::CancelToken::new();

        cx.update(|window, cx| {
            app.update(cx, |app, cx| {
                app.open_clone_modal(cx);
                app.is_cloning = true;
                app.clone_cancel = Some(cancel.clone());
                app.clone_progress = Some("Receiving objects: 42%".to_string());
                cx.notify();
            });
            window.draw(cx).clear();
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear();
        });

        let button = cx
            .debug_bounds("clone-cancel")
            .expect("the clone dialog renders a cancel button");
        cx.simulate_click(button.center(), Modifiers::none());

        assert!(
            cancel.is_cancelled(),
            "Cancel must signal the child process"
        );
        let (open, cloning, progress) = cx.update(|_window, cx| {
            let app = app.read(cx);
            (
                app.is_clone_modal_open,
                app.is_cloning,
                app.clone_progress.clone(),
            )
        });
        assert!(open, "the dialog stays open until cancellation completes");
        assert!(cloning, "the UI waits for the child to be reaped");
        assert_eq!(progress.as_deref(), Some("Cancelling\u{2026}"));
    }

    fn scan_preview(paths: &[&str], scanned: usize) -> ScanPreview {
        ScanPreview::new(
            std::path::Path::new("/tmp/workspace"),
            scanned,
            paths.iter().map(PathBuf::from).collect(),
        )
    }

    #[test]
    fn scan_preview_starts_with_every_candidate_ticked() {
        let scan = scan_preview(&["/tmp/workspace/one", "/tmp/workspace/two"], 2);
        assert!(scan.all_selected());
        assert_eq!(scan.selected_count(), 2);
        assert_eq!(
            scan.selected_paths(),
            vec![
                "/tmp/workspace/one".to_string(),
                "/tmp/workspace/two".to_string()
            ]
        );
        assert_eq!(scan.candidates[0].name, "one");
        assert_eq!(scan.folder, "/tmp/workspace");
    }

    #[test]
    fn scan_preview_selection_tracks_rows_and_select_all() {
        let mut scan = scan_preview(&["/tmp/workspace/one", "/tmp/workspace/two"], 2);

        scan.toggle("/tmp/workspace/one");
        assert!(!scan.all_selected());
        assert_eq!(scan.selected_count(), 1);
        assert_eq!(
            scan.selected_paths(),
            vec!["/tmp/workspace/two".to_string()]
        );

        scan.set_all(false);
        assert!(scan.selected_paths().is_empty());
        scan.set_all(true);
        assert!(scan.all_selected());

        // An unknown path is ignored rather than creating a phantom row.
        scan.toggle("/tmp/workspace/missing");
        assert_eq!(scan.candidates.len(), 2);
    }

    #[test]
    fn scan_preview_keeps_only_the_failed_rows_for_a_retry() {
        let mut scan = scan_preview(
            &[
                "/tmp/workspace/one",
                "/tmp/workspace/two",
                "/tmp/workspace/three",
            ],
            3,
        );
        let failed: HashSet<String> = ["/tmp/workspace/two".to_string()].into_iter().collect();
        scan.retain_paths(&failed);

        assert_eq!(scan.candidates.len(), 1);
        assert_eq!(scan.candidates[0].path, "/tmp/workspace/two");
        assert_eq!(scan.scanned, 1);
    }

    #[test]
    fn batch_failure_message_names_the_first_failures_and_caps_the_rest() {
        let failures = vec![
            (
                "/tmp/workspace/a".to_string(),
                "not a repository".to_string(),
            ),
            (
                "/tmp/workspace/b".to_string(),
                "permission denied".to_string(),
            ),
            ("/tmp/workspace/c".to_string(), "path missing".to_string()),
            ("/tmp/workspace/d".to_string(), "already added".to_string()),
        ];

        let all_failed = scan_failure_message(0, &failures);
        assert!(all_failed.starts_with("None of the 4 selected repositories"));
        assert!(all_failed.contains("a: not a repository"));
        assert!(all_failed.contains("c: path missing"));
        assert!(all_failed.ends_with("and 1 more"));

        let partial = scan_failure_message(2, &failures[..1]);
        assert!(partial.starts_with("Added 2 of 3 repositories."));
        assert!(partial.contains("a: not a repository"));
    }
}
