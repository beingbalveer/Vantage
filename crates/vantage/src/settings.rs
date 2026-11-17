//! Vantage v2 preferences.
//!
//! These live in `state.json` under `extra["vantage"]["settings"]`. The old
//! v1 `Settings` object at the top level is deliberately left alone: it still
//! contains terminal, viewer and commit settings that v2 does not expose.

use serde::{Deserialize, Serialize};
use vantage_core::git::refs::PullStrategy;
use vantage_core::persist::PersistedState;
use vantage_core::state::{validate_branch_name, Theme as LegacyTheme, WindowBounds};

use crate::model::NavSection;

const SETTINGS_VERSION: u32 = 1;
const EXTRA_KEY: &str = "vantage";
const LEGACY_EXTRA_KEY: &str = "branchdeck";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DefaultView {
    #[default]
    Overview,
    Attention,
    Changes,
}

impl ThemeChoice {
    /// Whether this choice paints dark for the current window appearance.
    pub fn is_dark(self, appearance: gpui::WindowAppearance) -> bool {
        match self {
            Self::System => matches!(
                appearance,
                gpui::WindowAppearance::Dark | gpui::WindowAppearance::VibrantDark
            ),
            Self::Light => false,
            Self::Dark => true,
        }
    }
}

impl DefaultView {
    pub fn nav_section(self) -> NavSection {
        match self {
            Self::Overview => NavSection::Overview,
            Self::Attention => NavSection::Attention,
            Self::Changes => NavSection::Changes,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SortOrder {
    #[default]
    Recent,
    Name,
    Changes,
    /// Repositories that need a decision first, then by activity.
    Attention,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsSection {
    General,
    Appearance,
    Git,
    Data,
    About,
}

impl SettingsSection {
    pub const ALL: [Self; 5] = [
        Self::General,
        Self::Appearance,
        Self::Git,
        Self::Data,
        Self::About,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Appearance => "Appearance",
            Self::Git => "Git",
            Self::Data => "Data",
            Self::About => "About",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Diagnostics {
    pub version: String,
    pub build: String,
    pub git: String,
    pub os: String,
    pub arch: String,
    pub state_path: String,
}

impl Diagnostics {
    pub fn collect(state_path: &std::path::Path) -> Self {
        Self {
            version: bundle_version().unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string()),
            build: if cfg!(debug_assertions) {
                "debug".to_string()
            } else {
                "release".to_string()
            },
            git: command_version("git", &["--version"])
                .map(|line| line.trim_start_matches("git version ").to_string())
                .unwrap_or_else(|| "git not found".to_string()),
            os: os_version(),
            arch: std::env::consts::ARCH.to_string(),
            state_path: state_path.display().to_string(),
        }
    }

    pub fn text(&self) -> String {
        format!(
            "Vantage {}\nbuild: {}\ngit: {}\nOS: {}\narch: {}\nstate: {}",
            self.version, self.build, self.git, self.os, self.arch, self.state_path
        )
    }
}

#[cfg(target_os = "macos")]
fn bundle_version() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let app = exe
        .ancestors()
        .find(|path| path.extension().is_some_and(|ext| ext == "app"))?;
    let plist = std::fs::read_to_string(app.join("Contents/Info.plist")).ok()?;
    plist_value(&plist, "CFBundleShortVersionString")
}

#[cfg(not(target_os = "macos"))]
fn bundle_version() -> Option<String> {
    None
}

#[cfg(target_os = "macos")]
fn plist_value(plist: &str, key: &str) -> Option<String> {
    let key_tag = format!("<key>{key}</key>");
    let rest = plist.split_once(&key_tag)?.1;
    let rest = rest.split_once("<string>")?.1;
    let value = rest.split_once("</string>")?.0;
    Some(value.trim().to_string())
}

fn command_version(command: &str, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new(command)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(target_os = "macos")]
fn os_version() -> String {
    command_version("sw_vers", &["-productVersion"])
        .map(|version| format!("macOS {version}"))
        .unwrap_or_else(|| "macOS".to_string())
}

#[cfg(not(target_os = "macos"))]
fn os_version() -> String {
    std::env::consts::OS.to_string()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UiSettings {
    pub version: u32,
    pub theme: ThemeChoice,
    pub default_view: DefaultView,
    pub sort: SortOrder,
    /// Last sidebar state; toggling the rail persists this with the debounced
    /// settings writer.
    pub sidebar_collapsed: bool,
    /// Sidebar width in points while it is expanded.
    pub sidebar_width: f32,
    /// Off = 0. Otherwise one of 1, 5, 15, 30 or 60 minutes.
    pub auto_fetch_minutes: u32,
    pub fetch_on_launch: bool,
    /// How `Pull` reconciles a branch that diverged from its upstream.
    pub pull_strategy: PullStrategy,
    /// Display fallback for a repository with no commits and no origin HEAD.
    pub default_branch: String,
    /// Command used by Open in Editor. Empty means automatic detection from
    /// `VISUAL`, `EDITOR`, or a common editor on `PATH`.
    pub editor_command: String,
    pub remember_window: bool,
    pub window_bounds: Option<WindowBounds>,
    /// Repository detail pane visibility and width as a fraction of the
    /// content area (sidebar excluded).
    pub right_panel_open: bool,
    pub right_panel_fraction: f32,
    /// Size of regular interface text in points. Line heights scale with it.
    pub interface_font_size: f32,
    /// Size of commit-history row text in points.
    pub commit_font_size: f32,
    /// Size of monospace code and diff text in points.
    pub code_font_size: f32,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            theme: ThemeChoice::System,
            default_view: DefaultView::Overview,
            sort: SortOrder::Recent,
            sidebar_collapsed: false,
            sidebar_width: 256.0,
            auto_fetch_minutes: 0,
            fetch_on_launch: false,
            pull_strategy: PullStrategy::FastForward,
            default_branch: "main".to_string(),
            editor_command: String::new(),
            remember_window: true,
            window_bounds: None,
            right_panel_open: false,
            right_panel_fraction: 0.67,
            interface_font_size: crate::theme::DEFAULT_INTERFACE_TEXT_SIZE,
            commit_font_size: crate::theme::DEFAULT_COMMIT_TEXT_SIZE,
            code_font_size: crate::theme::DEFAULT_CODE_TEXT_SIZE,
        }
    }
}

impl UiSettings {
    pub fn from_state(state: &PersistedState) -> Self {
        let section = state
            .extra
            .get(EXTRA_KEY)
            .or_else(|| state.extra.get(LEGACY_EXTRA_KEY))
            .and_then(|value| value.as_object());

        if let Some(settings) = section
            .and_then(|section| section.get("settings"))
            .and_then(|value| serde_json::from_value::<Self>(value.clone()).ok())
        {
            return settings.sanitized();
        }

        let mut settings = Self::default();
        settings.theme = match state.settings.theme {
            LegacyTheme::System => ThemeChoice::System,
            LegacyTheme::Light => ThemeChoice::Light,
            LegacyTheme::Dark => ThemeChoice::Dark,
        };
        settings.sanitized()
    }

    /// Clamp persisted values so a hand-edited state file cannot produce a
    /// giant window or an unsupported interval.
    pub fn sanitized(mut self) -> Self {
        self.version = SETTINGS_VERSION;
        if !matches!(self.auto_fetch_minutes, 0 | 1 | 5 | 15 | 30 | 60) {
            self.auto_fetch_minutes = 0;
        }
        self.default_branch = self.default_branch.trim().to_string();
        if validate_branch_name(&self.default_branch).is_err() {
            self.default_branch = "main".to_string();
        }
        self.editor_command = self.editor_command.trim().to_string();
        if let Some(ref bounds) = self.window_bounds {
            let sane = bounds.width >= 1040
                && bounds.height >= 640
                && bounds.width <= 10_000
                && bounds.height <= 10_000;
            if !sane {
                self.window_bounds = None;
            }
        }
        if !self.right_panel_fraction.is_finite() {
            self.right_panel_fraction = 0.67;
        }
        // 0.5 was the old default; move those settings to the new one so the
        // pane opens wider without the user having to drag it.
        if (self.right_panel_fraction - 0.5).abs() < f32::EPSILON {
            self.right_panel_fraction = 0.67;
        }
        self.right_panel_fraction = self.right_panel_fraction.clamp(0.3, 0.8);
        if !self.sidebar_width.is_finite() {
            self.sidebar_width = 256.0;
        }
        self.sidebar_width = self.sidebar_width.clamp(200.0, 360.0);
        self.interface_font_size = sane_font_size(
            self.interface_font_size,
            &[12.0, 13.0, 14.0, 15.0, 16.0],
            crate::theme::DEFAULT_INTERFACE_TEXT_SIZE,
        );
        self.commit_font_size = sane_font_size(
            self.commit_font_size,
            &[11.0, 12.0, 13.0, 14.0, 16.0],
            crate::theme::DEFAULT_COMMIT_TEXT_SIZE,
        );
        self.code_font_size = sane_font_size(
            self.code_font_size,
            &[11.0, 12.0, 13.0, 14.0, 16.0],
            crate::theme::DEFAULT_CODE_TEXT_SIZE,
        );
        self
    }

    /// The three font-size choices as the process-wide typography config.
    pub fn font_sizes(&self) -> crate::theme::FontSizes {
        crate::theme::FontSizes {
            interface: self.interface_font_size,
            commit: self.commit_font_size,
            code: self.code_font_size,
        }
    }
}

/// Accept only a size the Appearance choices actually offer.
fn sane_font_size(value: f32, choices: &[f32], default: f32) -> f32 {
    if value.is_finite() && choices.iter().any(|choice| (choice - value).abs() < 0.01) {
        value
    } else {
        default
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use vantage_core::state::Settings as LegacySettings;

    fn state_with_vantage(value: serde_json::Value) -> PersistedState {
        let mut state = PersistedState::default();
        state.extra.insert(EXTRA_KEY.to_string(), value);
        state
    }

    #[test]
    fn theme_choice_resolves_against_window_appearance() {
        use gpui::WindowAppearance;
        assert!(!ThemeChoice::Light.is_dark(WindowAppearance::Dark));
        assert!(ThemeChoice::Dark.is_dark(WindowAppearance::Light));
        assert!(ThemeChoice::System.is_dark(WindowAppearance::VibrantDark));
        assert!(!ThemeChoice::System.is_dark(WindowAppearance::VibrantLight));
    }

    #[test]
    fn defaults_are_v2_choices() {
        let settings = UiSettings::default();
        assert_eq!(settings.theme, ThemeChoice::System);
        assert_eq!(settings.default_view, DefaultView::Overview);
        assert_eq!(settings.sort, SortOrder::Recent);
        assert_eq!(settings.auto_fetch_minutes, 0);
        assert!(!settings.fetch_on_launch);
        assert_eq!(settings.pull_strategy, PullStrategy::FastForward);
        assert!(settings.editor_command.is_empty());
        assert!(settings.remember_window);
        assert!(!settings.right_panel_open);
        assert_eq!(settings.right_panel_fraction, 0.67);
        assert_eq!(
            settings.interface_font_size,
            crate::theme::DEFAULT_INTERFACE_TEXT_SIZE
        );
        assert_eq!(
            settings.commit_font_size,
            crate::theme::DEFAULT_COMMIT_TEXT_SIZE
        );
        assert_eq!(
            settings.code_font_size,
            crate::theme::DEFAULT_CODE_TEXT_SIZE
        );
    }

    #[test]
    fn unsupported_font_sizes_fall_back_to_defaults() {
        let settings = UiSettings {
            interface_font_size: 99.0,
            commit_font_size: f32::NAN,
            code_font_size: 14.0,
            ..UiSettings::default()
        }
        .sanitized();
        assert_eq!(
            settings.interface_font_size,
            crate::theme::DEFAULT_INTERFACE_TEXT_SIZE
        );
        assert_eq!(
            settings.commit_font_size,
            crate::theme::DEFAULT_COMMIT_TEXT_SIZE
        );
        assert_eq!(settings.code_font_size, 14.0);
    }

    #[test]
    fn the_old_half_width_pane_moves_to_two_thirds() {
        let settings = UiSettings {
            right_panel_fraction: 0.5,
            ..UiSettings::default()
        }
        .sanitized();
        assert_eq!(settings.right_panel_fraction, 0.67);
    }

    #[test]
    fn missing_settings_imports_only_the_legacy_theme() {
        let state = PersistedState {
            settings: LegacySettings {
                theme: LegacyTheme::Dark,
                auto_fetch_minutes: 5,
                ..LegacySettings::default()
            },
            ..PersistedState::default()
        };
        let settings = UiSettings::from_state(&state);
        assert_eq!(settings.theme, ThemeChoice::Dark);
        assert_eq!(
            settings.auto_fetch_minutes, 0,
            "auto-fetch must stay opt-in even if v1 was set to 5"
        );
        assert_eq!(settings.default_view, DefaultView::Overview);
    }

    #[test]
    fn saved_settings_round_trip_and_ignore_legacy_theme() {
        let settings = UiSettings {
            theme: ThemeChoice::Light,
            default_view: DefaultView::Changes,
            sort: SortOrder::Name,
            sidebar_collapsed: true,
            auto_fetch_minutes: 15,
            fetch_on_launch: true,
            default_branch: "trunk".to_string(),
            editor_command: "code --wait".to_string(),
            remember_window: false,
            pull_strategy: PullStrategy::Rebase,
            interface_font_size: 16.0,
            commit_font_size: 13.0,
            code_font_size: 14.0,
            ..UiSettings::default()
        };
        let value = serde_json::to_value(&settings).unwrap();
        let state = PersistedState {
            settings: LegacySettings {
                theme: LegacyTheme::Dark,
                ..LegacySettings::default()
            },
            ..state_with_vantage(serde_json::json!({
                "groups": ["Ungrouped"],
                "assignments": {},
                "settings": value,
            }))
        };

        let loaded = UiSettings::from_state(&state);
        assert_eq!(loaded.theme, ThemeChoice::Light);
        assert_eq!(loaded.default_view, DefaultView::Changes);
        assert_eq!(loaded.sort, SortOrder::Name);
        assert!(loaded.sidebar_collapsed);
        assert_eq!(loaded.auto_fetch_minutes, 15);
        assert!(loaded.fetch_on_launch);
        assert_eq!(loaded.default_branch, "trunk");
        assert_eq!(loaded.editor_command, "code --wait");
        assert!(!loaded.remember_window);
        assert_eq!(loaded.pull_strategy, PullStrategy::Rebase);
        assert_eq!(loaded.interface_font_size, 16.0);
        assert_eq!(loaded.commit_font_size, 13.0);
        assert_eq!(loaded.code_font_size, 14.0);
    }

    #[test]
    fn unknown_interval_and_bad_branch_fall_back() {
        let settings = UiSettings {
            auto_fetch_minutes: 7,
            default_branch: "bad..branch".to_string(),
            window_bounds: Some(WindowBounds {
                x: 0,
                y: 0,
                width: 100,
                height: 100,
            }),
            ..UiSettings::default()
        }
        .sanitized();
        assert_eq!(settings.auto_fetch_minutes, 0);
        assert_eq!(settings.default_branch, "main");
        assert!(settings.window_bounds.is_none());
    }

    #[test]
    fn editor_command_is_trimmed_and_optional() {
        let settings = UiSettings {
            editor_command: "  code --wait  ".to_string(),
            ..UiSettings::default()
        }
        .sanitized();
        assert_eq!(settings.editor_command, "code --wait");

        let settings = UiSettings {
            editor_command: "   ".to_string(),
            ..UiSettings::default()
        }
        .sanitized();
        assert!(settings.editor_command.is_empty());
    }

    #[test]
    fn settings_can_share_the_vantage_object_with_groups() {
        let state = state_with_vantage(serde_json::json!({
            "groups": ["Ungrouped", "SE"],
            "assignments": {"repo": "SE"},
            "settings": {
                "version": 1,
                "theme": "dark",
                "defaultView": "attention",
                "sort": "changes",
                "sidebarCollapsed": false,
                "autoFetchMinutes": 5,
                "fetchOnLaunch": false,
                "defaultBranch": "main",
                "rememberWindow": true,
                "windowBounds": null
            }
        }));
        let settings = UiSettings::from_state(&state);
        assert_eq!(settings.theme, ThemeChoice::Dark);
        assert_eq!(settings.default_view, DefaultView::Attention);
        assert_eq!(settings.sort, SortOrder::Changes);
        // The groups object itself is untouched by settings parsing.
        assert_eq!(
            state.extra[EXTRA_KEY]["groups"],
            serde_json::json!(["Ungrouped", "SE"])
        );
    }

    #[test]
    fn extra_unknown_fields_are_ignored() {
        let settings = UiSettings::from_state(&state_with_vantage(serde_json::json!({
            "settings": {
                "theme": "light",
                "futureSetting": true
            }
        })));
        assert_eq!(settings.theme, ThemeChoice::Light);
        let _: HashMap<String, serde_json::Value> = HashMap::new();
    }
}
