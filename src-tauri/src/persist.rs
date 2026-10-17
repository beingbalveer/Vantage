use crate::error::AppError;
use crate::state::{Layout, RepoEntry, Settings, WindowBounds};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
