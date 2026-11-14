//! Application state and the root view. Layout metrics follow the React app's
//! Tailwind classes: 256px sidebar, 56px top bar, 1440px content cap,
//! 576px inspector, 448px modal.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use gpui::{
    div, prelude::*, px, rgba, ClipboardItem, Context, FocusHandle, IntoElement, KeyDownEvent,
    MouseButton, PathPromptOptions, Window, WindowAppearance,
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

/// Which text field a keystroke belongs to.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FieldId {
    NewGroup,
    ModalPath,
    ModalCustomGroup,
    SettingsBranch,
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

    pub fn attention_count(&self) -> usize {
        self.repos.iter().filter(|r| r.needs_attention()).count()
    }

    pub fn changed_file_count(&self) -> usize {
        self.repos.iter().map(|r| r.uncommitted_files.len()).sum()
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
            self.group_menu_open = false;
            self.copied_path = false;
        }
        self.selected_repo_id = Some(id);
        self.settings.right_panel_open = true;
        self.save_settings(cx);
        cx.notify();
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
        if let Some(repository) = self.selected_repo().cloned() {
            self.fetch_repo(&repository, cx);
        } else {
            self.show_toast("Select a repository first", cx);
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
        let Some(staged) = self
            .repos
            .iter()
            .find(|repo| repo.id == repo_id)
            .and_then(|repo| {
                repo.uncommitted_files
                    .iter()
                    .find(|file| file.path == path)
                    .map(|file| file.staged)
            })
        else {
            return;
        };

        let id = repo_id.to_string();
        let file_path = path.to_string();
        let toast_path = file_path.clone();
        let target = !staged;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { backend.set_staged(&id, &file_path, target) })
                .await;
            this.update(cx, |app, cx| {
                if let Err(error) = result {
                    app.show_toast(format!("Could not update {toast_path}: {error}"), cx);
                }
                app.start_scan(cx);
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
                app.start_scan(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Phase 2: actually fetch, then rescan the tree.
    pub fn fetch_repo(&mut self, repo: &Repository, cx: &mut Context<Self>) {
        let Some(backend) = self.backend.clone() else {
            return;
        };
        let id = repo.id.clone();
        let name = repo.name.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { backend.fetch(&id) })
                .await;
            this.update(cx, |app, cx| {
                match result {
                    Ok(()) => app.show_toast(format!("Fetched origin for {name}"), cx),
                    Err(error) => app.show_toast(format!("Fetch failed for {name}: {error}"), cx),
                }
                app.start_scan(cx);
            })
            .ok();
        })
        .detach();
    }

    /// Phase 2: hand the folder to GitHub Desktop.
    pub fn open_in_desktop(&mut self, repo: &Repository, cx: &mut Context<Self>) {
        let name = repo.name.clone();
        let opened = match &self.backend {
            Some(backend) => backend.open_in_github_desktop(&repo.id),
            None => Err(anyhow::anyhow!("engine unavailable")),
        };
        match opened {
            Ok(()) => self.show_toast(format!("Opening {name} in GitHub Desktop"), cx),
            Err(error) => self.show_toast(format!("Could not open {name}: {error}"), cx),
        }
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
        }
    }

    pub fn field_state(&self, id: FieldId) -> &FieldState {
        match id {
            FieldId::NewGroup => &self.group_field,
            FieldId::ModalPath => &self.modal_path,
            FieldId::ModalCustomGroup => &self.modal_custom_group,
            FieldId::SettingsBranch => &self.settings_branch_field,
        }
    }

    pub fn field_focus(&self, id: FieldId) -> FocusHandle {
        match id {
            FieldId::NewGroup => self.group_field_focus.clone(),
            FieldId::ModalPath => self.modal_path_focus.clone(),
            FieldId::ModalCustomGroup => self.modal_custom_group_focus.clone(),
            FieldId::SettingsBranch => self.settings_branch_focus.clone(),
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
            FieldKey::Cancel => {
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
            FieldKey::Submit => match id {
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
                FieldId::SettingsBranch => {
                    self.commit_settings_branch(cx);
                }
            },
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
                cx.listener(move |this, _e, window, cx| {
                    if matches!(id, FieldId::ModalPath | FieldId::ModalCustomGroup) {
                        this.modal_group_menu_open = false;
                    }
                    window.focus(&this.field_focus(id));
                    let state = this.field_state_mut(id);
                    state.end();
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
            let splitter = div()
                .id("right-panel-splitter")
                .w(px(4.0))
                .h_full()
                .flex_shrink_0()
                .cursor(gpui::CursorStyle::ResizeColumn)
                .bg(theme.divider)
                .hover(move |s| s.bg(theme.accent))
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
            .child(content);

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
