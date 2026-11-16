use crate::error::AppError;
use crate::state::{Layout, RepoEntry, Settings, WindowBounds};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::fs::{self, File, TryLockError};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How long the writer waits for more changes before committing a batch.
/// Long enough to absorb a burst of clicks or a slider drag, short enough that
/// a change is on disk almost immediately.
const COALESCE_WINDOW: Duration = Duration::from_millis(150);
/// Upper bound on how long a caller waits for an immediate write or flush.
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

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

/// Serialize `state` and write it atomically (temp file + rename).
fn write_atomic(path: &Path, json: &str) -> Result<(), AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::other("State file has no parent directory"))?;
    if !parent.exists() {
        fs::create_dir_all(parent)?;
    }

    let temp_path = path.with_extension(format!(
        "tmp.{}",
        Utc::now().timestamp_nanos_opt().unwrap_or(0)
    ));
    fs::write(&temp_path, json)?;
    fs::rename(&temp_path, path)?;
    Ok(())
}

/// Read the state file without going through a handle. Shared by `load` and the
/// background writer, which must re-read the latest on-disk state before
/// applying a queued update.
fn load_at(file_path: &Path) -> (PersistedState, Option<String>) {
    if !file_path.exists() {
        return (PersistedState::default(), None);
    }

    let content = match fs::read_to_string(file_path) {
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
            let corrupt_path = file_path.with_file_name(corrupt_name);
            let _ = fs::rename(file_path, &corrupt_path);
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

/// A read-modify-write applied by the writer thread.
type StateUpdate = Box<dyn FnOnce(&mut PersistedState) -> Result<(), AppError> + Send + 'static>;

/// One state mutation queued for the writer.
enum WriteOp {
    /// Replace the whole file with already serialized state.
    Replace(String),
    /// Load the latest state, apply this change, and write it back.
    Update(StateUpdate),
}

enum WriteMsg {
    /// Coalesced with nearby changes before being written.
    Op(WriteOp),
    /// Apply a critical mutation after any queued changes, then acknowledge.
    Apply {
        update: StateUpdate,
        /// Write even when the serialized state is unchanged. Used by an
        /// explicit `save`, which must leave a file behind on first run.
        force: bool,
        reply: Sender<Result<(), AppError>>,
    },
    /// Write everything queued, then acknowledge.
    Flush(Sender<Result<(), AppError>>),
}

/// Health of the writer thread, shared with `StatePersistence` so the UI can
/// surface failures that happen after a debounced call returned successfully.
#[derive(Default)]
struct WriterHealth {
    write_error: Mutex<Option<AppError>>,
    notices: Mutex<VecDeque<String>>,
    writes: AtomicUsize,
}

impl WriterHealth {
    fn current_error(&self) -> Option<AppError> {
        self.write_error.lock().ok().and_then(|error| error.clone())
    }

    fn clear_error(&self) {
        if let Ok(mut error) = self.write_error.lock() {
            *error = None;
        }
    }

    fn record_error(&self, error: AppError) {
        let message = format!("Could not save the workspace state: {error}");
        eprintln!("{message}");
        if let Ok(mut slot) = self.write_error.lock() {
            *slot = Some(error);
        }
        if let Ok(mut notices) = self.notices.lock() {
            notices.push_back(message);
        }
    }

    fn record_notice(&self, message: String) {
        eprintln!("{message}");
        if let Ok(mut notices) = self.notices.lock() {
            notices.push_back(message);
        }
    }
}

/// Apply a batch to the latest on-disk state and write it back once.
///
/// Loading inside this function is what keeps a debounced settings change from
/// overwriting a repository change another code path made in the meantime.
fn apply_ops(
    file_path: &Path,
    ops: &mut Vec<WriteOp>,
    health: &WriterHealth,
    force: bool,
) -> Result<(), AppError> {
    let (mut state, warning) = load_at(file_path);
    if let Some(warning) = warning {
        health.record_notice(warning);
    }

    let before = serde_json::to_string(&state)?;
    for op in ops.drain(..) {
        match op {
            WriteOp::Replace(json) => state = serde_json::from_str(&json)?,
            WriteOp::Update(update) => update(&mut state)?,
        }
    }

    let json = serde_json::to_string(&state)?;
    if !force && json == before {
        // A click that did not change anything must not touch the file.
        return Ok(());
    }
    write_atomic(file_path, &json)?;
    health.writes.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

fn flush_ops(
    file_path: &Path,
    ops: &mut Vec<WriteOp>,
    health: &WriterHealth,
    force: bool,
) -> Result<(), AppError> {
    if ops.is_empty() {
        return health.current_error().map_or(Ok(()), Err);
    }

    let result = apply_ops(file_path, ops, health, force);
    match &result {
        Ok(()) => health.clear_error(),
        Err(error) => health.record_error(error.clone()),
    }
    result
}

fn apply_single_update(
    file_path: &Path,
    update: StateUpdate,
    health: &WriterHealth,
    force: bool,
) -> Result<(), AppError> {
    let mut ops = vec![WriteOp::Update(update)];
    flush_ops(file_path, &mut ops, health, force)
}

/// Background writer that coalesces bursts of low-priority state changes
/// (settings, selection, layout drags) into one atomic write. Critical
/// mutations are applied through the same thread and wait for the result, so a
/// debounced settings write can never clobber a repository change.
fn spawn_writer(file_path: PathBuf, health: Arc<WriterHealth>) -> Sender<WriteMsg> {
    let (tx, rx) = mpsc::channel::<WriteMsg>();
    std::thread::spawn(move || {
        let mut pending: Vec<WriteOp> = Vec::new();
        loop {
            match rx.recv() {
                Ok(WriteMsg::Op(op)) => {
                    pending.push(op);
                    loop {
                        match rx.recv_timeout(COALESCE_WINDOW) {
                            Ok(WriteMsg::Op(op)) => pending.push(op),
                            Ok(WriteMsg::Apply {
                                update,
                                force,
                                reply,
                            }) => {
                                let _ = flush_ops(&file_path, &mut pending, &health, false);
                                let result =
                                    apply_single_update(&file_path, update, &health, force);
                                let _ = reply.send(result);
                                break;
                            }
                            Ok(WriteMsg::Flush(reply)) => {
                                let result = flush_ops(&file_path, &mut pending, &health, false);
                                let _ = reply.send(result);
                                break;
                            }
                            Err(RecvTimeoutError::Timeout) => break,
                            Err(RecvTimeoutError::Disconnected) => {
                                let _ = flush_ops(&file_path, &mut pending, &health, false);
                                return;
                            }
                        }
                    }
                    let _ = flush_ops(&file_path, &mut pending, &health, false);
                }
                Ok(WriteMsg::Apply {
                    update,
                    force,
                    reply,
                }) => {
                    let _ = flush_ops(&file_path, &mut pending, &health, false);
                    let result = apply_single_update(&file_path, update, &health, force);
                    let _ = reply.send(result);
                }
                Ok(WriteMsg::Flush(reply)) => {
                    let result = flush_ops(&file_path, &mut pending, &health, false);
                    let _ = reply.send(result);
                }
                Err(_) => {
                    let _ = flush_ops(&file_path, &mut pending, &health, false);
                    return;
                }
            }
        }
    });
    tx
}

/// Take an exclusive lock beside the state file so a second instance can warn
/// instead of overwriting the first instance's changes silently.
///
/// The lock is advisory: a second instance still starts and still writes, but
/// it warns the user. Blocking startup on a lock would be worse than the
/// problem because a stale lock file, a killed process or a shared home
/// directory could leave the app permanently closed out.
fn acquire_instance_lock(file_path: &Path, health: &WriterHealth) -> Option<File> {
    let lock_path = file_path.with_extension("lock");
    if !lock_path.parent().is_some_and(|parent| parent.exists()) {
        return None;
    }

    let file = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .ok()?;

    match file.try_lock() {
        Ok(()) => Some(file),
        Err(TryLockError::WouldBlock) => {
            health.record_notice(
                "Another Vantage instance is already open. Changes made here may overwrite that \
                 window's settings and repository list."
                    .to_string(),
            );
            None
        }
        Err(TryLockError::Error(error)) => {
            health.record_notice(format!("Could not lock {}: {error}", lock_path.display()));
            None
        }
    }
}

pub struct StatePersistence {
    file_path: PathBuf,
    writer: Mutex<Sender<WriteMsg>>,
    health: Arc<WriterHealth>,
    /// Held for the life of the process; dropping it releases the advisory lock.
    _lock: Option<File>,
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
        Ok(Self::with_path_inner(app_dir.join("state.json")))
    }

    pub fn with_path(file_path: PathBuf) -> Self {
        Self::with_path_inner(file_path)
    }

    fn with_path_inner(file_path: PathBuf) -> Self {
        let health = Arc::new(WriterHealth::default());
        let lock = acquire_instance_lock(&file_path, &health);
        let writer = spawn_writer(file_path.clone(), health.clone());
        Self {
            file_path,
            writer: Mutex::new(writer),
            health,
            _lock: lock,
        }
    }

    pub fn path(&self) -> &Path {
        &self.file_path
    }

    pub fn load(&self) -> (PersistedState, Option<String>) {
        load_at(&self.file_path)
    }

    /// Write immediately (compact JSON). Used for critical mutations and on
    /// quit. It goes through the writer thread so it cannot race a queued
    /// settings update, and it waits until the file is on disk.
    pub fn save(&self, state: &PersistedState) -> Result<(), AppError> {
        let state = state.clone();
        self.update_and_wait_inner(
            move |current| {
                *current = state;
                Ok(())
            },
            true,
        )
    }

    /// Queue a write that is coalesced with nearby changes. Used for the
    /// high-frequency settings/layout paths so dragging a slider does not do
    /// synchronous disk I/O on every input event.
    pub fn save_debounced(&self, state: &PersistedState) -> Result<(), AppError> {
        let json = serde_json::to_string(state)?;
        let writer = self
            .writer
            .lock()
            .map_err(|_| AppError::other("State writer lock poisoned"))?;
        match writer.send(WriteMsg::Op(WriteOp::Replace(json))) {
            Ok(()) => Ok(()),
            Err(mpsc::SendError(WriteMsg::Op(WriteOp::Replace(json)))) => {
                drop(writer);
                write_atomic(&self.file_path, &json)
            }
            Err(_) => Err(AppError::other("State writer is not running")),
        }
    }

    /// Apply `update` to the latest state in the background. Returns as soon as
    /// the change is queued; failures from the actual write are reported by
    /// [`StatePersistence::take_notice`] and by [`StatePersistence::flush`].
    pub fn update_debounced<F>(&self, update: F) -> Result<(), AppError>
    where
        F: FnOnce(&mut PersistedState) -> Result<(), AppError> + Send + 'static,
    {
        let writer = self
            .writer
            .lock()
            .map_err(|_| AppError::other("State writer lock poisoned"))?;
        match writer.send(WriteMsg::Op(WriteOp::Update(Box::new(update)))) {
            Ok(()) => Ok(()),
            Err(mpsc::SendError(WriteMsg::Op(WriteOp::Update(update)))) => {
                drop(writer);
                apply_single_update(&self.file_path, update, &self.health, false)
            }
            Err(_) => Err(AppError::other("State writer is not running")),
        }
    }

    /// Apply a critical mutation to the latest state and wait for the write.
    /// Queued low-priority changes are committed first, then `update` sees
    /// their result.
    pub fn update_and_wait<F>(&self, update: F) -> Result<(), AppError>
    where
        F: FnOnce(&mut PersistedState) -> Result<(), AppError> + Send + 'static,
    {
        self.update_and_wait_inner(update, false)
    }

    fn update_and_wait_inner<F>(&self, update: F, force: bool) -> Result<(), AppError>
    where
        F: FnOnce(&mut PersistedState) -> Result<(), AppError> + Send + 'static,
    {
        let (reply_tx, reply_rx) = mpsc::channel();
        let message = WriteMsg::Apply {
            update: Box::new(update),
            force,
            reply: reply_tx,
        };
        {
            let writer = self
                .writer
                .lock()
                .map_err(|_| AppError::other("State writer lock poisoned"))?;
            match writer.send(message) {
                Ok(()) => {}
                Err(mpsc::SendError(WriteMsg::Apply { update, .. })) => {
                    drop(writer);
                    return apply_single_update(&self.file_path, update, &self.health, force);
                }
                Err(_) => return Err(AppError::other("State writer is not running")),
            }
        }

        reply_rx
            .recv_timeout(WRITE_TIMEOUT)
            .map_err(|_| AppError::other("Timed out waiting for the state writer"))?
    }

    /// Commit everything queued and wait until it is on disk. Called before a
    /// normal quit so a debounced setting cannot be lost on exit.
    pub fn flush(&self) -> Result<(), AppError> {
        let (reply_tx, reply_rx) = mpsc::channel();
        {
            let writer = self
                .writer
                .lock()
                .map_err(|_| AppError::other("State writer lock poisoned"))?;
            writer
                .send(WriteMsg::Flush(reply_tx))
                .map_err(|_| AppError::other("State writer is not running"))?;
        }

        reply_rx
            .recv_timeout(WRITE_TIMEOUT)
            .map_err(|_| AppError::other("Timed out waiting for the state writer"))?
    }

    /// The oldest asynchronous problem (write failure, corrupt state, second
    /// instance) since the last call, for the UI to show as a visible notice.
    pub fn take_notice(&self) -> Option<String> {
        self.health.notices.lock().ok()?.pop_front()
    }

    /// Number of actual file writes performed by the writer thread. Used by
    /// tests to prove a batch was coalesced.
    #[cfg(test)]
    pub fn write_count(&self) -> usize {
        self.health.writes.load(Ordering::Relaxed)
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
        assert!(warning.is_some());
        assert_eq!(state.settings.auto_fetch_minutes, 5);
        assert!(!path.exists(), "corrupt file must be renamed away");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn a_batch_of_debounced_updates_is_written_once() {
        let path = temp_state_path("batch");
        let health = WriterHealth::default();
        let mut batch: Vec<WriteOp> = (1..=25)
            .map(|minutes| {
                WriteOp::Update(Box::new(move |state: &mut PersistedState| {
                    state.settings.auto_fetch_minutes = minutes;
                    Ok(())
                }))
            })
            .collect();

        flush_ops(&path, &mut batch, &health, false).unwrap();

        assert_eq!(health.writes.load(Ordering::Relaxed), 1);
        assert!(batch.is_empty());
        let (state, warning) = load_at(&path);
        assert!(warning.is_none());
        assert_eq!(state.settings.auto_fetch_minutes, 25);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn queued_update_survives_a_flush() {
        let path = temp_state_path("flush");
        let persistence = StatePersistence::with_path(path.clone());

        persistence
            .update_debounced(|state| {
                state.settings.auto_fetch_minutes = 15;
                Ok(())
            })
            .unwrap();
        persistence.flush().unwrap();

        let (state, warning) = persistence.load();
        assert!(warning.is_none());
        assert_eq!(state.settings.auto_fetch_minutes, 15);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn an_immediate_update_keeps_a_pending_debounced_change() {
        let path = temp_state_path("ordered");
        let persistence = StatePersistence::with_path(path.clone());

        persistence
            .update_debounced(|state| {
                state.settings.auto_fetch_minutes = 15;
                Ok(())
            })
            .unwrap();
        persistence
            .update_and_wait(|state| {
                state.layout.terminal_height = 321.0;
                Ok(())
            })
            .unwrap();

        let (state, warning) = persistence.load();
        assert!(warning.is_none());
        assert_eq!(state.settings.auto_fetch_minutes, 15);
        assert_eq!(state.layout.terminal_height, 321.0);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn an_unchanged_update_does_not_rewrite_the_file() {
        let path = temp_state_path("noop");
        let persistence = StatePersistence::with_path(path.clone());
        persistence.save(&PersistedState::default()).unwrap();
        assert_eq!(persistence.write_count(), 1);

        persistence.update_debounced(|_state| Ok(())).unwrap();
        persistence.flush().unwrap();

        assert_eq!(
            persistence.write_count(),
            1,
            "a no-op update must not touch the state file"
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn a_failed_debounced_write_is_reported_and_kept_as_a_notice() {
        let dir = std::env::temp_dir().join(format!("vantage_state_dir_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        // A directory at the state-file path makes the atomic rename fail.
        let blocked = dir.join("state.json");
        fs::create_dir_all(&blocked).unwrap();

        let persistence = StatePersistence::with_path(blocked.clone());
        persistence
            .update_debounced(|state| {
                state.settings.auto_fetch_minutes = 15;
                Ok(())
            })
            .unwrap();

        let flushed = persistence.flush();
        assert!(flushed.is_err(), "flush must report a failed write");

        let mut notices = Vec::new();
        while let Some(notice) = persistence.take_notice() {
            notices.push(notice);
        }
        assert!(
            notices
                .iter()
                .any(|notice| notice.contains("Could not save")),
            "write failure must stay visible: {notices:?}"
        );

        drop(persistence);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_second_handle_is_told_about_the_other_instance() {
        let path = temp_state_path("lock");
        let first = StatePersistence::with_path(path.clone());
        let second = StatePersistence::with_path(path.clone());

        let notice = second
            .take_notice()
            .expect("the second handle must record a warning");
        assert!(
            notice.contains("Another Vantage instance"),
            "unexpected notice: {notice}"
        );

        drop(second);
        drop(first);
        let _ = fs::remove_file(path.with_extension("lock"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn a_handle_keeps_its_path_between_calls() {
        let path = temp_state_path("path");
        let persistence = StatePersistence::with_path(path.clone());

        assert_eq!(persistence.path(), path.as_path());
        assert_eq!(persistence.path(), path.as_path());
        let _ = fs::remove_file(path);
    }
}
