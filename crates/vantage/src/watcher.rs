//! Filesystem watcher: tracks every scanned working tree and reports which
//! repositories changed, so the UI can rescan only those.
//!
//! Events are collected on notify's own thread into a shared dirty set; the UI
//! drains that set on a timer, which keeps the render thread non-blocking.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use notify::{Event, EventKind, RecursiveMode, Watcher};

#[derive(Default)]
struct State {
    roots: Vec<PathBuf>,
    dirty: HashSet<PathBuf>,
}

pub struct RepoWatcher {
    _watcher: notify::RecommendedWatcher,
    state: Arc<Mutex<State>>,
}

impl RepoWatcher {
    pub fn new() -> anyhow::Result<Self> {
        let state: Arc<Mutex<State>> = Arc::new(Mutex::new(State::default()));
        let sink = state.clone();

        let mut watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
            let Ok(event) = result else {
                return;
            };
            if !matches!(
                event.kind,
                EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
            ) {
                return;
            }

            // Ignore anything under `.git`: our own status calls refresh the
            // index, which would otherwise mark the repo dirty and trigger
            // another scan. Checking a full path component also catches a
            // worktree/submodule `.git` file (not just the `.git` directory).
            if event
                .paths
                .iter()
                .any(|p| p.components().any(|c| c.as_os_str() == ".git"))
            {
                return;
            }

            let Ok(mut state) = sink.lock() else {
                return;
            };
            for changed in &event.paths {
                if let Some(root) = state
                    .roots
                    .iter()
                    .find(|root| changed.starts_with(root))
                    .cloned()
                {
                    state.dirty.insert(root);
                }
            }
        })?;

        // Watch nothing yet; `watch` registers repositories as they appear.
        watcher
            .watch(
                Path::new("/nonexistent-vantage"),
                RecursiveMode::NonRecursive,
            )
            .ok();

        Ok(Self {
            _watcher: watcher,
            state,
        })
    }

    /// Replace the watched set with `roots` (absolute repository paths).
    pub fn set_roots(&mut self, roots: Vec<PathBuf>) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if state.roots == roots {
            return;
        }

        for root in &state.roots {
            self._watcher.unwatch(root).ok();
        }
        for root in &roots {
            // Watch the whole tree recursively; events under `.git` are
            // filtered out in the callback above.
            self._watcher.watch(root, RecursiveMode::Recursive).ok();
        }
        state.roots = roots;
        state.dirty.clear();
    }

    /// Roots that changed since the last call.
    pub fn take_dirty(&self) -> Vec<PathBuf> {
        let Ok(mut state) = self.state.lock() else {
            return Vec::new();
        };
        state.dirty.drain().collect()
    }
}
