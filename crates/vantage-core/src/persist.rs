use crate::error::AppError;
use crate::state::{Layout, RepoEntry, Settings, WindowBounds};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

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

enum WriteMsg {
    Write(PathBuf, String),
}

/// What a file beside the live state file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateBackupKind {
    /// The previous state file was malformed and was renamed aside on load.
    Corrupt,
    /// A copy of the state file from before the most recent successful save.
    LastKnownGood,
    /// Any other `state.json.*` file the user (or an older build) left behind.
    Other,
}

impl StateBackupKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Corrupt => "Corrupt state backup",
            Self::LastKnownGood => "Last known good",
            Self::Other => "State backup",
        }
    }
}

/// A file next to `state.json` that can be inspected or copied back by hand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateBackup {
    pub path: PathBuf,
    pub kind: StateBackupKind,
}

/// `<state file>.last-good`, e.g. `state.json.last-good`.
fn last_good_path(path: &Path) -> Option<PathBuf> {
    let name = path.file_name()?;
    let mut backup_name = name.to_os_string();
    backup_name.push(".last-good");
    Some(path.with_file_name(backup_name))
}

/// Copy the previous state aside before replacing it. A corrupt file found at
/// the next launch then has a nearby, parseable recovery source. This never
/// restores anything automatically; the copy is explicit and read-only.
fn preserve_last_good(path: &Path) -> Result<(), AppError> {
    if !path.is_file() {
        return Ok(());
    }
    let backup =
        last_good_path(path).ok_or_else(|| AppError::other("State file has no file name"))?;
    fs::copy(path, &backup).map_err(AppError::io)?;
    Ok(())
}

/// Serialize `state` and write it atomically (temp file + rename).
fn write_atomic(path: &Path, json: &str) -> Result<(), AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::other("State file has no parent directory"))?;
    if !parent.exists() {
        fs::create_dir_all(parent)?;
    }

    preserve_last_good(path)?;

    let temp_path = path.with_extension(format!(
        "tmp.{}",
        Utc::now().timestamp_nanos_opt().unwrap_or(0)
    ));
    fs::write(&temp_path, json)?;
    fs::rename(&temp_path, path)?;
    Ok(())
}

/// Background writer that coalesces bursts of low-priority state changes
/// (settings sliders, layout drags) into one atomic write. Critical mutations
/// still call `save` directly for immediate durability.
fn spawn_writer() -> Sender<WriteMsg> {
    let (tx, rx) = mpsc::channel::<WriteMsg>();
    std::thread::spawn(move || {
        while let Ok(first) = rx.recv() {
            let mut latest = match first {
                WriteMsg::Write(path, json) => (path, json),
            };

            loop {
                match rx.recv_timeout(Duration::from_millis(150)) {
                    Ok(WriteMsg::Write(path, json)) => latest = (path, json),
                    Err(RecvTimeoutError::Timeout) => break,
                    Err(RecvTimeoutError::Disconnected) => {
                        if let Err(error) = write_atomic(&latest.0, &latest.1) {
                            eprintln!("failed to persist state: {error}");
                        }
                        return;
                    }
                }
            }

            if let Err(error) = write_atomic(&latest.0, &latest.1) {
                eprintln!("failed to persist state: {error}");
            }
        }
    });
    tx
}

pub struct StatePersistence {
    file_path: PathBuf,
    writer: Mutex<Sender<WriteMsg>>,
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
            writer: Mutex::new(spawn_writer()),
        })
    }

    pub fn with_path(file_path: PathBuf) -> Self {
        Self {
            file_path,
            writer: Mutex::new(spawn_writer()),
        }
    }

    pub fn path(&self) -> &Path {
        &self.file_path
    }

    /// Directory holding the state file and its backups.
    pub fn data_dir(&self) -> &Path {
        self.file_path.parent().unwrap_or_else(|| Path::new("."))
    }

    /// Backups beside the live state file, newest first. The live `state.json`
    /// is never included; partial temp files are ignored.
    pub fn backups(&self) -> Result<Vec<StateBackup>, AppError> {
        let dir = self.data_dir();
        let live_name = self
            .file_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "state.json".to_string());
        let stem = self
            .file_path
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_string())
            .unwrap_or_else(|| "state".to_string());
        let corrupt_prefix = format!("{stem}.corrupt-");
        let backup_prefix = format!("{live_name}.");
        let last_good = last_good_path(&self.file_path)
            .and_then(|path| path.file_name().map(|name| name.to_owned()));

        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(AppError::io(error)),
        };

        let mut found: Vec<(Option<SystemTime>, StateBackup)> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = entry.file_name();
            let name_string = name.to_string_lossy();
            let kind = if last_good.as_deref() == Some(name.as_os_str()) {
                Some(StateBackupKind::LastKnownGood)
            } else if name_string.starts_with(&corrupt_prefix) {
                Some(StateBackupKind::Corrupt)
            } else if name_string.starts_with(&backup_prefix) {
                Some(StateBackupKind::Other)
            } else {
                None
            };
            let Some(kind) = kind else {
                continue;
            };
            let modified = entry.metadata().and_then(|meta| meta.modified()).ok();
            found.push((modified, StateBackup { path, kind }));
        }

        found.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| b.1.path.cmp(&a.1.path))
                .then_with(|| b.1.kind.label().cmp(a.1.kind.label()))
        });
        Ok(found.into_iter().map(|(_, backup)| backup).collect())
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
                // Corrupt file handling: move the malformed file aside so the
                // next save cannot silently overwrite it, and tell the user
                // exactly where it went.
                let ts = Utc::now().timestamp();
                let stem = self
                    .file_path
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().to_string())
                    .unwrap_or_else(|| "state".to_string());
                let corrupt_name = format!("{stem}.corrupt-{ts}.json");
                let corrupt_path = self.file_path.with_file_name(corrupt_name);
                let warning = match fs::rename(&self.file_path, &corrupt_path) {
                    Ok(()) => format!(
                        "Vantage could not read its saved state, so it opened with defaults. \
                         The malformed file was moved to {} and can be inspected there. \
                         Error: {e}",
                        corrupt_path.display()
                    ),
                    Err(rename_error) => format!(
                        "Vantage could not read its saved state and could not move the malformed \
                         file aside. It opened with defaults. Before saving, copy {} somewhere \
                         safe. Error: {e}. Move failed: {rename_error}",
                        self.file_path.display()
                    ),
                };
                (PersistedState::default(), Some(warning))
            }
        }
    }

    /// Write immediately (compact JSON). Used for critical mutations and on
    /// quit, where losing the last change would be surprising.
    pub fn save(&self, state: &PersistedState) -> Result<(), AppError> {
        let json = serde_json::to_string(state)?;
        write_atomic(&self.file_path, &json)
    }

    /// Queue a write that is coalesced with nearby changes. Used for the
    /// high-frequency settings/layout paths so dragging a slider does not do
    /// synchronous disk I/O on every input event.
    pub fn save_debounced(&self, state: &PersistedState) -> Result<(), AppError> {
        let json = serde_json::to_string(state)?;
        let sent = self
            .writer
            .lock()
            .map_err(|_| AppError::other("State writer lock poisoned"))?
            .send(WriteMsg::Write(self.file_path.clone(), json));
        if sent.is_err() {
            // Writer thread is gone; fall back to a synchronous write.
            return write_atomic(&self.file_path, &serde_json::to_string(state)?);
        }
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

    fn temp_state_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "vantage_state_dir_{}_{}",
            name,
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn loads_old_state_file_and_defaults_new_settings() {
        let path = temp_state_path("old");
        fs::write(
            &path,
            r#"{
                "version": 1,
                "repos": [],
                "layout": { "paneCount": 1, "panes": [{ "repoId": null, "tab": "history", "widths": null }], "terminalHeight": 240, "terminalOpen": false, "windowBounds": null },
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
        assert!(state.settings.confirm_terminal_restart);
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
        let warning = warning.expect("corrupt state must report a warning");
        assert!(warning.contains("could not read its saved state"));
        assert_eq!(state.settings.auto_fetch_minutes, 5);
        assert!(!path.exists(), "corrupt file must be renamed away");

        let backups = persistence.backups().unwrap();
        let corrupt = backups
            .iter()
            .find(|backup| backup.kind == StateBackupKind::Corrupt)
            .expect("the renamed file must be listed as a corrupt backup");
        assert!(corrupt.path.exists());
        assert!(
            warning.contains(&corrupt.path.display().to_string()),
            "the warning must name the backup users can recover: {warning}"
        );

        let _ = fs::remove_file(&corrupt.path);
    }

    #[test]
    fn backups_keep_the_previous_state_and_ignore_temp_files() {
        let dir = temp_state_dir("backups");
        let path = dir.join("state.json");
        let persistence = StatePersistence::with_path(path.clone());

        let mut first = PersistedState::default();
        first.settings.auto_fetch_minutes = 15;
        persistence.save(&first).unwrap();

        let mut second = PersistedState::default();
        second.settings.auto_fetch_minutes = 30;
        persistence.save(&second).unwrap();

        // A partial write and an unrelated file must not look recoverable.
        fs::write(dir.join("state.tmp.123"), "{ partial").unwrap();
        fs::write(dir.join("notes.txt"), "not state").unwrap();

        let backups = persistence.backups().unwrap();
        assert_eq!(
            backups.len(),
            1,
            "only the last-known-good copy: {backups:?}"
        );
        assert_eq!(backups[0].kind, StateBackupKind::LastKnownGood);
        assert_eq!(backups[0].path, dir.join("state.json.last-good"));

        let previous: PersistedState =
            serde_json::from_str(&fs::read_to_string(&backups[0].path).unwrap()).unwrap();
        assert_eq!(
            previous.settings.auto_fetch_minutes, 15,
            "the backup must hold the state from before the newest save"
        );

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_reports_a_state_directory_that_cannot_be_written() {
        let dir = temp_state_dir("failed_save");
        // A regular file where the state directory should be makes the write
        // fail deterministically, including when tests run as root.
        let blocker = dir.join("not-a-directory");
        fs::write(&blocker, "blocked").unwrap();
        let persistence = StatePersistence::with_path(blocker.join("state.json"));

        let error = persistence.save(&PersistedState::default()).unwrap_err();
        assert_eq!(error.code, "Io");
        assert!(!error.message.is_empty());

        fs::remove_dir_all(&dir).ok();
    }
}
