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

use crate::backend::Backend;
use crate::model::{NavSection, RepoSyncState, Repository};
use crate::settings::{
    DefaultView, Diagnostics, SettingsSection, SortOrder, ThemeChoice, UiSettings,
};
use crate::theme::Theme;
use crate::ui::{apply_field_key, FieldKey, FieldState, Txt};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum InspectorTab {
    Changes,
    Branches,
    Commits,
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
}

impl RepoOp {
    /// Button label while this operation runs: "Fetching…", "Pulling…",
    /// "Pushing…", "Committing…".
    pub fn in_progress_label(self) -> &'static str {
        match self {
            Self::Fetch => "Fetching…",
            Self::Discard => "Discarding…",
            Self::Amend => "Amending…",
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

/// Which text field a keystroke belongs to.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FieldId {
    NewGroup,
    ModalPath,
    ModalCustomGroup,
    SettingsBranch,
    CommitMessage,
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
    /// Message for the next commit in the focused repository.
    pub commit_field: FieldState,
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
    /// The commit bar is in amend mode: the next commit rewrites the tip.
    pub amending: bool,
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
    modal_path_focus: FocusHandle,
    modal_custom_group_focus: FocusHandle,
}

impl VantageApp {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let backend = Backend::new().map(std::sync::Arc::new).ok();
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
            commit_field: FieldState::new(),
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
            amending: false,
            selection: std::collections::HashMap::new(),
            avatars: std::collections::HashMap::new(),
            avatars_pending: HashSet::new(),
            text_click_claimed: false,
            scan_errors: Vec::new(),
            watcher: None,
            scan_seq: 0,
            modal_path_focus: cx.focus_handle(),
            modal_custom_group_focus: cx.focus_handle(),
        }
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
                Err(_) => return,
            }
        }
        if let Some(watcher) = self.watcher.as_mut() {
            watcher.set_roots(roots);
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
    pub fn persist(&mut self, _cx: &mut Context<Self>) {
        let Some(backend) = self.backend.clone() else {
            return;
        };
        let known = backend.repo_ids();
        for repo in &self.repos {
            if !known.contains(&repo.id) {
                continue;
            }
            if repo.workspace == "Ungrouped" {
                self.assignments.remove(&repo.id);
            } else {
                self.assignments
                    .insert(repo.id.clone(), repo.workspace.clone());
            }
        }
        backend.set_groups(&self.groups, &self.assignments).ok();
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
        self.modal_group_choice = if self.active_group != "All" {
            self.active_group.clone()
        } else {
            self.groups
                .first()
                .cloned()
                .unwrap_or_else(|| "Ungrouped".to_string())
        };
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
        cx.notify();
    }

    pub fn select_repo(&mut self, id: String, cx: &mut Context<Self>) {
        if self.selected_repo_id.as_deref() != Some(id.as_str()) {
            self.expanded_file = None;
            self.context_menu = None;
            self.group_menu_open = false;
            self.copied_path = false;
        }
        self.selected_repo_id = Some(id);
        self.settings.right_panel_open = true;
        self.save_settings(cx);
        if let Some(repo) = self.selected_repo().cloned() {
            self.ensure_avatar(&repo, cx);
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
        let key = author.email.clone();
        if self.avatars.contains_key(&key) || self.avatars_pending.contains(&key) {
            return;
        }
        self.avatars_pending.insert(key.clone());

        let path = expand_home(&repo.local_path);
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
        cx.notify();
    }

    pub fn close_settings(&mut self, cx: &mut Context<Self>) {
        self.is_settings_open = false;
        self.settings_reset_confirm = false;
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
        self.save_settings(cx);
        self.show_toast("Default branch saved", cx);
    }

    pub fn reset_settings(&mut self, appearance: WindowAppearance, cx: &mut Context<Self>) {
        self.settings = UiSettings::default();
        self.sidebar_collapsed = false;
        self.settings_branch_field
            .set(self.settings.default_branch.clone());
        self.settings_reset_confirm = false;
        self.apply_theme(appearance);
        self.save_settings(cx);
        self.show_toast("Preferences reset", cx);
    }

    pub fn save_settings(&mut self, cx: &mut Context<Self>) {
        let Some(backend) = self.backend.clone() else {
            return;
        };
        if let Err(error) = backend.set_ui_settings(&self.settings) {
            self.show_toast(format!("Could not save settings: {error}"), cx);
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

    pub fn reveal_state_path(&mut self, cx: &mut Context<Self>) {
        let path = self.state_path();
        let mut command = std::process::Command::new(if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        });
        if cfg!(target_os = "macos") {
            command.arg("-R");
        }
        let result = command.arg(&path).spawn();
        if let Err(error) = result {
            self.show_toast(format!("Could not reveal the data folder: {error}"), cx);
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
        self.persist(cx);
        self.show_toast(format!("Created group \"{trimmed}\""), cx);
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
        self.persist(cx);
        self.show_toast(format!("Removed group \"{name}\""), cx);
    }

    pub fn move_repo_group(&mut self, repo_id: &str, group: String, cx: &mut Context<Self>) {
        if !self.groups.contains(&group) {
            self.groups.push(group.clone());
        }
        let mut moved = None;
        for repo in &mut self.repos {
            if repo.id == repo_id {
                repo.workspace = group.clone();
                moved = Some(repo.name.clone());
            }
        }
        if let Some(name) = moved {
            self.persist(cx);
            self.show_toast(format!("Moved {name} to {group}"), cx);
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
        let message = self.commit_field.value.trim().to_string();
        if message.is_empty() {
            self.show_toast("Write a commit message first", cx);
            return;
        }
        let paths = self.ticked_paths(&repo);
        if paths.is_empty() {
            self.show_toast("Stage the files you want to commit", cx);
            return;
        }

        if self.amending {
            let name = repo.name.clone();
            let record_repo = repo.id.clone();
            let record_summary = message.clone();
            let previous_tip = repo.recent_commits.first().map(|commit| commit.sha.clone());
            let paths_for_op = paths.clone();
            self.run_repo_op(
                &repo,
                cx,
                RepoOp::Amend,
                RepoOpReport::new(
                    format!("Amended the last commit in {name}"),
                    format!("Could not amend the last commit in {name}"),
                )
                .on_success(move |app, short: String| {
                    app.amending = false;
                    app.commit_field.clear();
                    app.selection.remove(&record_repo);
                    app.last_commit = Some(LastCommit {
                        repo_id: record_repo,
                        short,
                        summary: record_summary,
                        previous_tip,
                    });
                }),
                move |backend, id| backend.amend(id, &paths_for_op, &message),
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
        self.run_repo_op(
            &repo,
            cx,
            RepoOp::Commit,
            RepoOpReport::new(ok_message, format!("Commit failed for {name}"))
                // The message is only spent once git accepted it, so a failed
                // commit leaves the composer intact for a retry.
                .on_success(move |app, short: String| {
                    app.commit_field.clear();
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
            move |backend, repo_id| backend.commit(repo_id, &paths, &message),
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
        is_tip: bool,
        x: f32,
        y: f32,
        cx: &mut Context<Self>,
    ) {
        self.context_menu = Some(ContextMenu::Commit {
            repo_id: repo_id.to_string(),
            short: short.to_string(),
            subject: subject.to_string(),
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

    /// Rewrite the tip commit instead of making a new one: the message field is
    /// pre-filled with the commit's subject and the usual file ticks apply.
    pub fn begin_amend(&mut self, subject: &str, cx: &mut Context<Self>) {
        self.context_menu = None;
        self.amending = true;
        // The files being amended live in Changes, so go there: the mode is
        // about what the commit will contain, not just its message.
        self.inspector_tab = InspectorTab::Changes;
        self.commit_field.set(subject.to_string());
        cx.notify();
    }

    pub fn cancel_amend(&mut self, cx: &mut Context<Self>) {
        self.amending = false;
        self.commit_field.clear();
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
                                app.groups.push(resolved_group.clone());
                                resolved_group.clone()
                            }
                        };
                        repository.workspace = resolved_group.clone();
                        app.repos.retain(|repo| repo.id != repository.id);
                        app.repos.insert(0, repository.clone());
                        app.selected_repo_id = Some(repository.id.clone());
                        app.settings.right_panel_open = true;

                        app.modal_path.clear();
                        app.modal_custom_group.clear();
                        app.modal_error = None;
                        app.is_add_modal_open = false;
                        app.modal_group_menu_open = false;

                        app.persist(cx);
                        app.save_settings(cx);
                        app.show_toast(
                            format!("Added {} to {}", repository.name, resolved_group),
                            cx,
                        );
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

    // ---- text fields -----------------------------------------------------

    pub fn field_state_mut(&mut self, id: FieldId) -> &mut FieldState {
        match id {
            FieldId::NewGroup => &mut self.group_field,
            FieldId::ModalPath => &mut self.modal_path,
            FieldId::ModalCustomGroup => &mut self.modal_custom_group,
            FieldId::SettingsBranch => &mut self.settings_branch_field,
            FieldId::CommitMessage => &mut self.commit_field,
        }
    }

    pub fn field_state(&self, id: FieldId) -> &FieldState {
        match id {
            FieldId::NewGroup => &self.group_field,
            FieldId::ModalPath => &self.modal_path,
            FieldId::ModalCustomGroup => &self.modal_custom_group,
            FieldId::SettingsBranch => &self.settings_branch_field,
            FieldId::CommitMessage => &self.commit_field,
        }
    }

    pub fn field_focus(&self, id: FieldId) -> FocusHandle {
        match id {
            FieldId::NewGroup => self.group_field_focus.clone(),
            FieldId::ModalPath => self.modal_path_focus.clone(),
            FieldId::ModalCustomGroup => self.modal_custom_group_focus.clone(),
            FieldId::SettingsBranch => self.settings_branch_focus.clone(),
            FieldId::CommitMessage => self.commit_focus.clone(),
        }
    }

    pub fn handle_field_key(
        &mut self,
        id: FieldId,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let outcome = {
            let state = self.field_state_mut(id);
            apply_field_key(state, event, cx)
        };

        match outcome {
            FieldKey::Ignored => {}
            FieldKey::Changed => cx.notify(),
            FieldKey::Cancel => self.cancel_field(id, cx),
            FieldKey::Submit => self.submit_field(id, cx),
        }
    }

    /// Escape in a field: close whichever surface owns it.
    fn cancel_field(&mut self, id: FieldId, cx: &mut Context<Self>) {
        if id == FieldId::CommitMessage && self.amending {
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
        let focus = self.field_focus(id);
        let focused = focus.is_focused(window);
        let state = self.field_state(id).clone();
        let theme = self.theme;

        let row = crate::ui::field(&theme, &state, placeholder, focused, variant)
            .id(gpui::ElementId::named_usize("field", id as u8 as usize))
            .track_focus(&focus)
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
                this.handle_field_key(id, event, window, cx);
            }));

        row
    }
}

impl Render for VantageApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;

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
            let sidebar_width = if self.sidebar_collapsed { 56.0 } else { 256.0 };
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
                            (start_fraction - delta).clamp(0.3, 0.7);
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

        let content = div()
            .flex()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .child(self.render_sidebar(window, cx))
            .child(main);

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
            .child(self.render_top_bar(window, cx))
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
            let height = 96.0;
            let (anchor_x, anchor_y, title, items) = match &menu {
                ContextMenu::File { path, x, y, .. } => {
                    let name = path
                        .rsplit_once('/')
                        .map_or(path.as_str(), |(_, name)| name)
                        .to_string();
                    let items: Vec<(&'static str, bool, u64)> = vec![("Discard changes", true, 0)];
                    (*x, *y, name, items)
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
                    let items: Vec<(&'static str, bool, u64)> = if *is_tip {
                        vec![("Amend commit", false, 0)]
                    } else {
                        Vec::new()
                    };
                    (*x, *y, format!("{short}  {subject}"), items)
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
                                (ContextMenu::File { repo_id, path, .. }, "Discard changes") => {
                                    this.discard_file(repo_id, path, cx)
                                }
                                (
                                    ContextMenu::Commit {
                                        repo_id, subject, ..
                                    },
                                    "Amend commit",
                                ) => {
                                    // Only the repo the menu came from can be
                                    // amended, even if the selection moved on.
                                    if this.selected_repo_id.as_deref() == Some(repo_id.as_str()) {
                                        this.begin_amend(subject, cx);
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

/// A right-click menu, anchored at the pointer.
#[derive(Clone, Debug)]
pub enum ContextMenu {
    /// Actions for one changed file.
    File {
        repo_id: String,
        path: String,
        x: f32,
        y: f32,
    },
    /// Actions for one commit.
    Commit {
        repo_id: String,
        short: String,
        subject: String,
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
    use super::*;
    use crate::model::{ChangedFile, FileChangeStatus};

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
            current_branch: "main".to_string(),
            author: None,
            sync_state: RepoSyncState::Modified,
            ahead_count: 0,
            behind_count: 0,
            has_upstream: true,
            last_activity_label: "just now".to_string(),
            last_activity_minutes: 0,
            uncommitted_files: files,
            branches: Vec::new(),
            recent_commits: Vec::new(),
        }
    }

    fn repo_with_commits(shas: &[&str]) -> Repository {
        let mut repo = repo_with(Vec::new());
        repo.recent_commits = shas
            .iter()
            .map(|sha| crate::model::CommitEntry {
                sha: sha.to_string(),
                message: format!("subject of {sha}"),
                author: "QA".to_string(),
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
}
