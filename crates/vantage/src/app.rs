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

use crate::backend::{Backend, BranchDeleteOutcome};
use crate::model::{NavSection, RepoSyncState, Repository};
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

/// Which text field a keystroke belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldId {
    NewGroup,
    ModalPath,
    ModalCustomGroup,
    SettingsBranch,
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
    pub status_filter: Option<RepoSyncState>,
    /// Filter chosen from the summary strip above the repository list.
    pub summary_filter: Option<SummaryFilter>,
    pub search_query: String,
    pub selected_repo_id: Option<String>,
    pub is_add_modal_open: bool,
    pub is_refreshing: bool,
    pub toast: Option<String>,
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

    // Settings modal
    pub settings_branch_field: FieldState,
    settings_branch_focus: FocusHandle,

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

        Self {
            theme: if settings.theme == ThemeChoice::Dark {
                Theme::dark()
            } else {
                Theme::light()
            },
            settings: settings.clone(),
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
            status_filter: None,
            summary_filter: None,
            search_query: String::new(),
            selected_repo_id: None,
            is_add_modal_open: false,
            is_refreshing: false,
            toast: None,
            toast_seq: 0,
            state_warning,
            state_warning_dismissed: false,
            state_backups,
            state_data_dir,
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

            settings_branch_field: FieldState::new(),
            settings_branch_focus: cx.focus_handle(),

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
            modal_path_focus: cx.focus_handle(),
            modal_custom_group_focus: cx.focus_handle(),
            repo_list_focus: cx.focus_handle(),
            modal_focus: cx.focus_handle(),
            last_main_focus: None,
            pending_focus: None,
            restore_focus: false,
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

    fn begin_scan(&mut self, roots: Option<Vec<PathBuf>>, silent: bool, cx: &mut Context<Self>) {
        let Some(backend) = self.backend.clone() else {
            self.scan_errors = vec!["vantage-core engine unavailable".to_string()];
            cx.notify();
            return;
        };

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
                if app.scan_seq != seq {
                    return;
                }
                app.is_scanning = false;
                app.scan_errors = errors;

                if dirty {
                    for repository in repositories {
                        match app.repos.iter().position(|repo| repo.id == repository.id) {
                            Some(index) => app.repos[index] = repository,
                            None => app.repos.push(repository),
                        }
                    }
                    for repo in &app.repos {
                        if !app.groups.contains(&repo.workspace) {
                            app.groups.push(repo.workspace.clone());
                        }
                    }
                    app.prune_selection();
                    cx.notify();
                    return;
                }

                app.repos = repositories;

                // Keep the configured order, then append groups only found
                // on disk.
                for repo in &app.repos {
                    if !app.groups.contains(&repo.workspace) {
                        app.groups.push(repo.workspace.clone());
                    }
                }

                if app.active_group != "All" && !app.groups.contains(&app.active_group) {
                    app.active_group = "All".to_string();
                }
                if let Some(id) = app.selected_repo_id.clone() {
                    if !app.repos.iter().any(|repo| repo.id == id) {
                        app.composers.leave();
                        app.selected_repo_id = None;
                    }
                }

                app.prune_selection();
                app.bind_watcher(cx);
                app.maybe_fetch_on_launch(cx);
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

    pub fn show_toast(&mut self, message: impl Into<String>, cx: &mut Context<Self>) {
        self.toast = Some(message.into());
        self.toast_seq = self.toast_seq.wrapping_add(1);
        let seq = self.toast_seq;
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(2600))
                .await;
            this.update(cx, |this, cx| {
                if this.toast_seq == seq {
                    this.toast = None;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
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

        if let Some(filter) = self.status_filter {
            list.retain(|r| match filter {
                RepoSyncState::Behind => matches!(
                    r.sync_state,
                    RepoSyncState::Behind | RepoSyncState::Conflict
                ),
                RepoSyncState::Modified => !r.uncommitted_files.is_empty(),
                RepoSyncState::Ahead => r.ahead_count > 0,
                other => r.sync_state == other,
            });
        }

        let query = self.search_query.trim().to_lowercase();
        if !query.is_empty() {
            list.retain(|r| {
                r.name.to_lowercase().contains(&query)
                    || r.current_branch.to_lowercase().contains(&query)
                    || r.local_path.to_lowercase().contains(&query)
                    || r.workspace.to_lowercase().contains(&query)
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
        cx.notify();
    }

    pub fn close_add_modal(&mut self, cx: &mut Context<Self>) {
        if self.modal_is_adding {
            return;
        }
        self.is_add_modal_open = false;
        self.modal_group_menu_open = false;
        self.restore_focus = true;
        cx.notify();
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
        self.settings.right_panel_open = true;
        self.save_settings(cx);
        if let Some(repo) = self.selected_repo().cloned() {
            self.ensure_avatar(&repo, cx);
            let seen: Vec<(String, String)> = repo
                .recent_commits
                .iter()
                .map(|commit| (commit.author_email.clone(), repo.local_path.clone()))
                .collect();
            for (email, path) in seen {
                self.ensure_avatar_for(&email, &path, cx);
            }
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

    /// Resolve and fetch the profile photo for a repository's commit identity.
    ///
    /// Runs off the UI thread: the lookup may read `gh`'s config and the fetch
    /// shells out to `curl`. Both the download and the decode are cached, so
    /// selecting the same repository again costs nothing.
    fn ensure_avatar(&mut self, repo: &Repository, cx: &mut Context<Self>) {
        let Some(author) = repo.author.clone() else {
            return;
        };
        self.ensure_avatar_for(&author.email, &repo.local_path, cx);
    }

    /// Fetch the GitHub photo for one email, once. Commit rows use this for the
    /// author icons, so history shows faces where the cache can resolve a login.
    fn ensure_avatar_for(&mut self, email: &str, local_path: &str, cx: &mut Context<Self>) {
        if email.trim().is_empty() {
            return;
        }
        let key = email.to_string();
        if self.avatars.contains_key(&key) || self.avatars_pending.contains(&key) {
            return;
        }
        self.avatars_pending.insert(key.clone());

        let path = expand_home(local_path);
        let load_key = key.clone();
        cx.spawn(async move |this, cx| {
            let avatar = cx
                .background_executor()
                .spawn(async move { crate::avatar_cache::load(&load_key, &path) })
                .await;
            this.update(cx, |app, cx| {
                app.avatars_pending.remove(&key);
                let photo = avatar.map(|avatar| {
                    // GitHub serves PNG or JPEG depending on the uploaded
                    // avatar, so the detected format is what we decode with.
                    let format = match avatar.format {
                        crate::avatar_cache::AvatarFormat::Png => gpui::ImageFormat::Png,
                        crate::avatar_cache::AvatarFormat::Jpeg => gpui::ImageFormat::Jpeg,
                        crate::avatar_cache::AvatarFormat::Gif => gpui::ImageFormat::Gif,
                        crate::avatar_cache::AvatarFormat::Webp => gpui::ImageFormat::Webp,
                    };
                    std::sync::Arc::new(gpui::Image::from_bytes(format, avatar.bytes))
                });
                app.avatars.insert(key, photo);
                cx.notify();
            })
            .ok();
        })
        .detach();
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
            self.show_toast(error, cx);
            return;
        }
        self.settings.default_branch = branch;
        if self.save_settings(cx) {
            self.show_toast("Default branch saved", cx);
        }
    }

    pub fn reset_settings(&mut self, appearance: WindowAppearance, cx: &mut Context<Self>) {
        self.settings = UiSettings::default();
        self.sidebar_collapsed = false;
        self.settings_branch_field
            .set(self.settings.default_branch.clone());
        self.settings_reset_confirm = false;
        self.apply_theme(appearance);
        if self.save_settings(cx) {
            self.show_toast("Preferences reset", cx);
        }
    }

    pub fn save_settings(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(backend) = self.backend.clone() else {
            return false;
        };
        match backend.set_ui_settings(&self.settings) {
            Ok(()) => true,
            Err(error) => {
                self.show_toast(format!("Could not save settings: {error}"), cx);
                false
            }
        }
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
        #[cfg(target_os = "macos")]
        let result = std::process::Command::new("open").arg(&path).spawn();
        #[cfg(target_os = "windows")]
        let result = std::process::Command::new("explorer").arg(&path).spawn();
        #[cfg(all(unix, not(target_os = "macos")))]
        let result = std::process::Command::new("xdg-open").arg(&path).spawn();

        if let Err(error) = result {
            self.show_toast(format!("Could not open the data folder: {error}"), cx);
        }
    }

    /// Reveal one state backup in the platform file manager.
    pub fn reveal_state_backup(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        #[cfg(target_os = "macos")]
        let result = std::process::Command::new("open")
            .arg("-R")
            .arg(&path)
            .spawn();
        #[cfg(target_os = "windows")]
        let result = std::process::Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn();
        #[cfg(all(unix, not(target_os = "macos")))]
        let result = std::process::Command::new("xdg-open")
            .arg(path.parent().unwrap_or(path.as_path()))
            .spawn();

        if let Err(error) = result {
            self.show_toast(format!("Could not reveal the backup: {error}"), cx);
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
            self.show_toast(format!("Could not open link: {error}"), cx);
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
                    app.show_toast(
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
                    app.show_toast(format!("Could not update {toast_path}: {error}"), cx);
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
                    app.show_toast(format!("Could not {verb} {total} files: {error}"), cx);
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
                        app.show_toast(format!("Checkout failed for {name}: {error}"), cx)
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
        // The summary and the "Needs Attention" view are different lenses; the
        // summary filter wins so the numbers always match the list.
        self.status_filter = None;
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
                    Err(error) => self.show_toast(format!("Could not remove it: {error}"), cx),
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
                        app.show_toast(format!("Could not write .gitignore: {error}"), cx)
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
        self.context_menu = None;
        self.clone_url.clear();
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
        self.is_clone_modal_open = false;
        self.is_cloning = false;
        self.restore_focus = true;
        cx.notify();
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
                    Err(error) => app.show_toast(format!("Could not create it: {error}"), cx),
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
        self.is_cloning = true;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(
                    async move { backend.clone_repo(&url, &parent_path.to_string_lossy(), None) },
                )
                .await;
            this.update(cx, |app, cx| {
                app.is_cloning = false;
                match result {
                    Ok(()) => {
                        app.is_clone_modal_open = false;
                        app.clone_url.clear();
                        app.restore_focus = true;
                        app.show_toast("Cloned and added to the list", cx);
                        app.start_scan(cx);
                    }
                    Err(error) => app.show_toast(format!("Clone failed: {error}"), cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
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
                Err(error) => app.show_toast(format!("Could not read the remote: {error}"), cx),
            })
            .ok();
        })
        .detach();
    }

    /// Show the repository in Finder.
    pub fn reveal_repo(&mut self, repo_id: &str, cx: &mut Context<Self>) {
        self.context_menu = None;
        let Some(repo) = self.repos.iter().find(|repo| repo.id == repo_id).cloned() else {
            return;
        };
        let path = expand_home(&repo.local_path);
        match std::process::Command::new("open")
            .arg("-R")
            .arg(&path)
            .spawn()
        {
            Ok(_) => cx.notify(),
            Err(error) => self.show_toast(format!("Could not reveal the folder: {error}"), cx),
        }
    }

    /// Open the repository in Terminal.
    pub fn open_repo_terminal(&mut self, repo_id: &str, cx: &mut Context<Self>) {
        self.context_menu = None;
        let Some(repo) = self.repos.iter().find(|repo| repo.id == repo_id).cloned() else {
            return;
        };
        let path = expand_home(&repo.local_path);
        match std::process::Command::new("open")
            .args(["-a", "Terminal"])
            .arg(&path)
            .spawn()
        {
            Ok(_) => cx.notify(),
            Err(error) => self.show_toast(format!("Could not open Terminal: {error}"), cx),
        }
    }

    /// Open the repository in the editor the environment names.
    pub fn open_repo_editor(&mut self, repo_id: &str, cx: &mut Context<Self>) {
        self.context_menu = None;
        let Some(repo) = self.repos.iter().find(|repo| repo.id == repo_id).cloned() else {
            return;
        };
        let path = expand_home(&repo.local_path);
        let editor = std::env::var("VISUAL")
            .or_else(|_| std::env::var("EDITOR"))
            .ok()
            .filter(|value| !value.trim().is_empty());
        let spawned = match editor {
            Some(editor) => std::process::Command::new(editor).arg(&path).spawn(),
            // `code` is on PATH for most VS Code users; otherwise open the folder.
            None => std::process::Command::new("code").arg(&path).spawn(),
        };
        match spawned.or_else(|_| std::process::Command::new("open").arg(&path).spawn()) {
            Ok(_) => cx.notify(),
            Err(error) => self.show_toast(format!("Could not open the editor: {error}"), cx),
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
        self.selected_repo_id = Some(id);
        cx.notify();
    }

    /// Open the highlighted repository's detail pane.
    pub fn open_selected(&mut self, cx: &mut Context<Self>) {
        if let Some(repo) = self.selected_repo().cloned() {
            self.settings.right_panel_open = true;
            self.save_settings(cx);
            self.ensure_avatar(&repo, cx);
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
                                    .fg(theme.label_muted)
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
                                    .px(px(6.0))
                                    .py(px(2.0))
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
                                    .px(px(6.0))
                                    .py(px(2.0))
                                    .rounded(px(4.0))
                                    .cursor_pointer()
                                    .xs()
                                    .fg(theme.label_muted)
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
            .child(self.text_field(
                FieldId::NewRepoName,
                "Repository name",
                crate::ui::FieldVariant::Form,
                window,
                cx,
            ))
            .child(self.text_field(
                FieldId::NewRepoParent,
                "Destination folder",
                crate::ui::FieldVariant::Form,
                window,
                cx,
            ))
            .child(toggle)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        crate::ui::secondary_button(&theme, "Cancel", None)
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
        if !self.modal_focus.contains_focused(window, cx) {
            window.focus(&self.clone_url_focus);
        }

        let label = if self.is_cloning {
            "Cloning…"
        } else {
            "Clone"
        };
        let mut confirm = crate::ui::accent_button(&theme, label)
            .id("clone-confirm")
            .on_click(cx.listener(|this, _e, _w, cx| this.submit_clone(cx)));
        if self.is_cloning {
            confirm = confirm.opacity(0.6);
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
            .child(self.text_field(
                FieldId::CloneUrl,
                "https://github.com/owner/repo.git",
                crate::ui::FieldVariant::Form,
                window,
                cx,
            ))
            .child(self.text_field(
                FieldId::CloneParent,
                "Destination folder",
                crate::ui::FieldVariant::Form,
                window,
                cx,
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        crate::ui::secondary_button(&theme, "Cancel", None)
                            .id("clone-cancel")
                            .on_click(cx.listener(|this, _e, _w, cx| this.close_clone_modal(cx))),
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
                            .id("confirm-cancel")
                            .on_click(cx.listener(|this, _e, _w, cx| this.cancel_confirm(cx))),
                    )
                    .child(
                        crate::ui::destructive_button(&theme, prompt.label)
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
                body = body.child(self.text_field(
                    FieldId::BranchName,
                    "Branch name",
                    crate::ui::FieldVariant::Form,
                    window,
                    cx,
                ))
            }
        }

        let confirm_button = if destructive {
            crate::ui::destructive_button(&theme, action_label)
        } else {
            crate::ui::accent_button(&theme, action_label)
        };
        let confirm = confirm_button
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
        // the repo lock, so queueing a second one only delays the UI.
        if self.busy_repos.insert(repo.id.clone(), op_kind).is_some() {
            return;
        }
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
                        if let Some(on_success) = report.on_success {
                            on_success(app, value);
                        }
                        app.show_toast(report.ok_message, cx);
                    }
                    Err(error) => app.show_toast(format!("{}: {error}", report.fail_message), cx),
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
        self.search_query.clear();
        self.status_filter = None;
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

        let resolved_group = if self.modal_group_choice == "__new__" {
            let custom = self.modal_custom_group.value.trim();
            if custom.is_empty() {
                self.modal_error = Some("Enter a name for the new group.".to_string());
                cx.notify();
                return;
            }
            custom.to_string()
        } else {
            self.modal_group_choice.clone()
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
                        let existing_group = app
                            .groups
                            .iter()
                            .find(|group| group.to_lowercase() == resolved_group.to_lowercase())
                            .cloned();
                        let resolved_group = match existing_group {
                            Some(existing) => existing,
                            None => {
                                add_group_if_missing(&mut app.groups, &resolved_group);
                                resolved_group.clone()
                            }
                        };
                        repository.workspace = resolved_group.clone();
                        app.repos.retain(|repo| repo.id != repository.id);
                        app.repos.insert(0, repository.clone());
                        app.composers.switch_to(&repository.id);
                        app.selected_repo_id = Some(repository.id.clone());
                        app.settings.right_panel_open = true;

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

    pub fn render_toast(&self) -> Option<gpui::AnyElement> {
        let message = self.toast.clone()?;
        let theme = self.theme;
        Some(
            div()
                .absolute()
                .bottom(px(20.0))
                .right(px(20.0))
                .px(px(16.0))
                .py(px(10.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(theme.toast_border)
                .bg(theme.toast_bg)
                .fg(theme.toast_text)
                .shadow_lg()
                .xs()
                .medium()
                .child(message)
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
                            crate::ui::icon_button(&theme, "x")
                                .id("state-recovery-dismiss")
                                .on_click(cx.listener(|this, _e, _w, cx| {
                                    this.state_warning_dismissed = true;
                                    cx.notify();
                                })),
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
                FieldId::SettingsBranch => self.is_settings_open,
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
            let (anchor_x, anchor_y, title, items) = match &menu {
                ContextMenu::File { path, x, y, .. } => {
                    let name = path
                        .rsplit_once('/')
                        .map_or(path.as_str(), |(_, name)| name)
                        .to_string();
                    let items: Vec<(&'static str, bool, u64)> = vec![
                        ("Add to .gitignore", false, 0),
                        ("Discard changes", true, 0),
                    ];
                    (*x, *y, name, items)
                }
                ContextMenu::Repo {
                    name,
                    uncommitted,
                    stashes,
                    with_changes,
                    x,
                    y,
                    ..
                } => {
                    // One menu, one subject: the working tree where it was
                    // opened from the Changes header, the repository elsewhere.
                    let mut items: Vec<(&'static str, bool, u64)> = Vec::new();
                    if *with_changes {
                        if *uncommitted > 0 {
                            items.push(("Stash all changes", false, 0));
                        }
                        if *stashes > 0 {
                            items.push(("Restore latest stash", false, 0));
                        }
                        if *uncommitted > 0 {
                            items.push(("Discard all changes", true, 0));
                        }
                    } else {
                        items.push(("Update from default branch", false, 0));
                        if *stashes > 0 {
                            items.push(("Manage stashes…", false, 0));
                        }
                        items.push(("View on GitHub", false, 0));
                        items.push(("Remove from list", true, 0));
                        items.push(("Reveal in Finder", false, 0));
                        items.push(("Open in Terminal", false, 0));
                        items.push(("Open in editor", false, 0));
                    }
                    (*x, *y, name.clone(), items)
                }
                ContextMenu::Branch {
                    name,
                    is_current,
                    is_default,
                    x,
                    y,
                    ..
                } => {
                    let mut items: Vec<(&'static str, bool, u64)> = Vec::new();
                    if !*is_current {
                        items.push(("Checkout branch", false, 0));
                    }
                    if !*is_current {
                        items.push(("Merge into current branch", false, 0));
                        items.push(("Compare with current", false, 0));
                    }
                    items.push(("New branch from this branch", false, 0));
                    if *is_current {
                        items.push(("Rename current branch", false, 0));
                    }
                    if !*is_current && !*is_default {
                        items.push(("Delete branch", true, 0));
                    }
                    (*x, *y, name.clone(), items)
                }
                ContextMenu::Commit {
                    short,
                    subject,
                    is_tip,
                    x,
                    y,
                    ..
                } => {
                    // Only the tip commit can be rewritten, and only while it is
                    // still local.
                    let mut revert_only: Vec<(&'static str, bool, u64)> =
                        vec![("Revert commit", false, 0), ("Reset branch here", true, 0)];
                    if *is_tip {
                        revert_only.push(("Amend commit", false, 0));
                    }
                    let _ = is_tip;
                    (*x, *y, format!("{short}  {subject}"), revert_only)
                }
            };
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

            let menu_for_actions = menu.clone();
            for (label, destructive, _) in items {
                let action_menu = menu_for_actions.clone();
                panel = panel.child(
                    div()
                        .id(gpui::SharedString::from(format!("ctx-{label}")))
                        .flex()
                        .items_center()
                        .px(px(8.0))
                        .py(px(6.0))
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
                            match (&action_menu, label) {
                                (ContextMenu::File { repo_id, path, .. }, "Add to .gitignore") => {
                                    let id = repo_id.clone();
                                    let file = path.clone();
                                    this.ignore_file(&id, &file, cx);
                                }
                                (ContextMenu::File { repo_id, path, .. }, "Discard changes") => {
                                    this.discard_file(repo_id, path, cx)
                                }
                                (ContextMenu::Repo { repo_id, .. }, "Stash all changes") => {
                                    let id = repo_id.clone();
                                    this.stash_repo(&id, cx);
                                }
                                (ContextMenu::Repo { repo_id, .. }, "Restore latest stash") => {
                                    let id = repo_id.clone();
                                    this.pop_stash(&id, cx);
                                }
                                (ContextMenu::Repo { repo_id, .. }, "Discard all changes") => {
                                    let id = repo_id.clone();
                                    this.ask_discard_repo(&id, cx);
                                }
                                (
                                    ContextMenu::Repo { repo_id, .. },
                                    "Update from default branch",
                                ) => {
                                    let id = repo_id.clone();
                                    this.ask_update_from_default(&id, cx);
                                }
                                (
                                    ContextMenu::Branch { repo_id, name, .. },
                                    "Merge into current branch",
                                ) => {
                                    let id = repo_id.clone();
                                    let branch = name.clone();
                                    this.ask_merge_branch(&id, &branch, cx);
                                }
                                (ContextMenu::Repo { repo_id, .. }, "Manage stashes…") => {
                                    let id = repo_id.clone();
                                    this.open_stash_modal(&id, cx);
                                }
                                (
                                    ContextMenu::Branch { repo_id, name, .. },
                                    "Compare with current",
                                ) => {
                                    let id = repo_id.clone();
                                    let other = name.clone();
                                    this.compare_with_branch(&id, &other, cx);
                                }
                                (
                                    ContextMenu::Commit { repo_id, short, .. },
                                    "Reset branch here",
                                ) => {
                                    let id = repo_id.clone();
                                    let sha = short.clone();
                                    this.ask_reset_commit(&id, &sha, cx);
                                }
                                (ContextMenu::Repo { repo_id, .. }, "View on GitHub") => {
                                    let id = repo_id.clone();
                                    this.open_repo_page(&id, cx);
                                }
                                (ContextMenu::Repo { repo_id, .. }, "Remove from list") => {
                                    let id = repo_id.clone();
                                    this.ask_forget_repo(&id, cx);
                                }
                                (ContextMenu::Repo { repo_id, .. }, "Reveal in Finder") => {
                                    let id = repo_id.clone();
                                    this.reveal_repo(&id, cx);
                                }
                                (ContextMenu::Repo { repo_id, .. }, "Open in Terminal") => {
                                    let id = repo_id.clone();
                                    this.open_repo_terminal(&id, cx);
                                }
                                (ContextMenu::Repo { repo_id, .. }, "Open in editor") => {
                                    let id = repo_id.clone();
                                    this.open_repo_editor(&id, cx);
                                }
                                (ContextMenu::Commit { repo_id, short, .. }, "Revert commit") => {
                                    let id = repo_id.clone();
                                    let sha = short.clone();
                                    this.revert_commit(&id, &sha, cx);
                                }
                                (ContextMenu::Branch { repo_id, name, .. }, "Checkout branch") => {
                                    if this.selected_repo_id.as_deref() == Some(repo_id.as_str()) {
                                        this.close_file_menu(cx);
                                        this.switch_branch(repo_id, name, cx);
                                    } else {
                                        this.close_file_menu(cx);
                                    }
                                }
                                (
                                    ContextMenu::Branch { repo_id, name, .. },
                                    "New branch from this branch",
                                ) => {
                                    this.start_branch_prompt(
                                        repo_id,
                                        BranchPromptKind::Create { base: name.clone() },
                                        cx,
                                    );
                                }
                                (
                                    ContextMenu::Branch { repo_id, name, .. },
                                    "Rename current branch",
                                ) => {
                                    this.start_branch_prompt(
                                        repo_id,
                                        BranchPromptKind::Rename { from: name.clone() },
                                        cx,
                                    );
                                }
                                (ContextMenu::Branch { repo_id, name, .. }, "Delete branch") => {
                                    this.start_branch_prompt(
                                        repo_id,
                                        BranchPromptKind::Delete { name: name.clone() },
                                        cx,
                                    );
                                }
                                (
                                    ContextMenu::Commit {
                                        repo_id,
                                        subject,
                                        body,
                                        ..
                                    },
                                    "Amend commit",
                                ) => {
                                    // Only the repo the menu came from can be
                                    // amended, even if the selection moved on.
                                    if this.selected_repo_id.as_deref() == Some(repo_id.as_str()) {
                                        this.begin_amend(subject, body, cx);
                                    } else {
                                        this.close_file_menu(cx);
                                    }
                                }
                                _ => this.close_file_menu(cx),
                            }
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
        if let Some(toast) = self.render_toast() {
            root = root.child(toast);
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
        let toast = cx.read(|cx| app.read(cx).toast.clone()).unwrap_or_default();
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

    fn branch(name: &str, is_head: bool) -> vantage_core::state::BranchInfo {
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
            &[branch("feature", false)],
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
            &[branch("main", false), branch("other", true)],
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
            &[branch("feature", true)],
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
                let toast = app.toast.clone().unwrap_or_default();
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
}
