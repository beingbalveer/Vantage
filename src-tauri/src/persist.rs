use crate::error::AppError;
use crate::state::{Layout, RepoEntry, Settings, WindowBounds};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PersistedState {
    pub version: u32,
    pub repos: Vec<RepoEntry>,
    pub layout: Layout,
    pub settings: Settings,
    pub window_bounds: Option<WindowBounds>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl Default for PersistedState {
    fn default() -> Self {
        Self {
            version: 1,
            repos: Vec::new(),
            layout: Layout::default(),
            settings: Settings::default(),
            window_bounds: None,
            extra: HashMap::new(),
        }
    }
}

pub struct StatePersistence {
    file_path: PathBuf,
}

impl StatePersistence {
    pub fn new() -> Result<Self, AppError> {
        let base_dir = dirs::data_dir()
            .or_else(dirs::config_dir)
            .unwrap_or_else(|| PathBuf::from("."));
        let app_dir = base_dir.join("vantage");
        let legacy_dir = base_dir.join("relay");
        if !app_dir.exists() {
            if legacy_dir.exists() {
                let _ = fs::rename(&legacy_dir, &app_dir);
            }
            if !app_dir.exists() {
                fs::create_dir_all(&app_dir).map_err(AppError::io)?;
            }
        }
        Ok(Self {
            file_path: app_dir.join("state.json"),
        })
    }

    pub fn with_path(file_path: PathBuf) -> Self {
        Self { file_path }
    }

    pub fn path(&self) -> &Path {
        &self.file_path
    }

    pub fn load(&self) -> (PersistedState, Option<String>) {
        if !self.file_path.exists() {
            return (PersistedState::default(), None);
        }

        let content = match fs::read_to_string(&self.file_path) {
            Ok(c) => c,
            Err(e) => {
                return (
                    PersistedState::default(),
                    Some(format!("Failed to read state file: {}", e)),
                );
            }
        };

        match serde_json::from_str::<PersistedState>(&content) {
            Ok(state) => (state, None),
            Err(e) => {
                // Corrupt file handling
                let ts = Utc::now().timestamp();
                let corrupt_name = format!("state.corrupt-{}.json", ts);
                let corrupt_path = self.file_path.with_file_name(corrupt_name);
                let _ = fs::rename(&self.file_path, &corrupt_path);
                (
                    PersistedState::default(),
                    Some(format!(
                        "State file was corrupt (backed up to {:?}): {}",
                        corrupt_path, e
                    )),
                )
            }
        }
    }

    pub fn save(&self, state: &PersistedState) -> Result<(), AppError> {
        let parent = self
            .file_path
            .parent()
            .ok_or_else(|| AppError::other("State file has no parent directory"))?;
        if !parent.exists() {
            fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(state)?;
        let temp_path = self.file_path.with_extension(format!(
            "tmp.{}",
            Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        fs::write(&temp_path, json)?;
        fs::rename(&temp_path, &self.file_path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_state_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "vantage_state_test_{}_{}.json",
            name,
            uuid::Uuid::new_v4()
        ))
    }

    #[test]
    fn loads_old_state_file_and_defaults_new_settings() {
        let path = temp_state_path("old");
        fs::write(
            &path,
            r#"{
                "version": 1,
                "repos": [],
                "layout": { "paneCount": 1, "panes": [{ "repoId": null, "tab": "history", "widths": null }], "sidebarWidth": 232, "sidebarCollapsed": false, "terminalHeight": 240, "terminalOpen": false, "windowBounds": null },
                "settings": { "theme": "dark", "diffMode": "unified", "ignoreWhitespace": false, "contextLines": 3, "autoFetchMinutes": 5, "shell": null, "externalEditorCmd": null, "commitMessageCmd": null, "terminalFontSize": 13, "uiScale": 1.0 },
                "windowBounds": null
            }"#,
        )
        .unwrap();

        let persistence = StatePersistence::with_path(path.clone());
        let (state, warning) = persistence.load();
        assert!(warning.is_none(), "old state should not be corrupt");
        assert_eq!(state.settings.theme, crate::state::Theme::Dark);
        assert_eq!(state.settings.default_branch, "main");
        assert_eq!(state.settings.tab_size, 4);
        assert!(state.settings.confirm_terminal_close);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn loads_partial_state_and_preserves_unknown_fields() {
        let path = temp_state_path("partial");
        fs::write(
            &path,
            r#"{ "version": 1, "settings": { "theme": "light" }, "futureTopLevel": 42 }"#,
        )
        .unwrap();

        let persistence = StatePersistence::with_path(path.clone());
        let (state, warning) = persistence.load();
        assert!(warning.is_none(), "partial state must not brick startup");
        assert_eq!(state.settings.theme, crate::state::Theme::Light);
        assert_eq!(state.settings.terminal_font_size, 13);
        assert_eq!(state.extra.get("futureTopLevel").unwrap(), 42);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn corrupt_state_is_backed_up_and_defaults_are_used() {
        let path = temp_state_path("corrupt");
        fs::write(&path, "{ not json").unwrap();

        let persistence = StatePersistence::with_path(path.clone());
        let (state, warning) = persistence.load();
        assert!(warning.is_some());
        assert_eq!(state.settings.auto_fetch_minutes, 5);
        assert!(!path.exists(), "corrupt file must be renamed away");
        let _ = fs::remove_file(path);
    }
}
