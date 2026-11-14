# BranchDeck (native GPUI)

The repository dashboard UI for Vantage, built with [GPUI](https://gpui.rs).
It renders Vantage's real git data through `vantage-core` — no webview.

```
cargo run -p branchdeck
```

## Where the data comes from

`crates/vantage-core` is Vantage's original engine (git CLI runner, status,
log, refs, diffs, stash, watcher, JSON state). This crate adds a thin adapter:

| File | Role |
|---|---|
| `src/backend/mod.rs` | Owns a `tokio::runtime::Runtime`, loads `PersistedState` via `StatePersistence`, exposes blocking `snapshot` / `refresh` / `add_repo` / `fetch` / `open_in_github_desktop` / `groups` / `set_groups`. |
| `src/backend/repo_map.rs` | Maps `RepoStatus` + `RepoEntry` onto the UI's `Repository` (sync state, ahead/behind, activity, stash). |
| `src/backend/details.rs` | Branches, recent commits, changed files with per-file stats and diff previews, remote normalisation, language detection. |
| `src/model.rs` | View model (`Repository`, `BranchInfo`, `ChangedEntry`, …). |
| `src/theme.rs` | The dashboard palette. |
| `src/sidebar.rs`, `topbar.rs`, `repo_table.rs`, `branch_matrix.rs`, `pending_changes.rs`, `inspector.rs`, `add_repo_modal.rs` | The five views, inspector and modal. |
| `src/text.rs` | Ellipsis fitting via GPUI's own line wrapper. |
| `src/watcher.rs` | `notify` watchers over repo paths; a debounced poll refreshes through the backend. |

Groups and per-repo group assignments are persisted inside Vantage's own
`state.json` under `extra["branchdeck"]`, so there is a single source of truth.

## Dev flags

```sh
BRANCHDECK_STATE=dark,section:branches,inspector:first,itab:branches,modal cargo run -p branchdeck
cargo run -p branchdeck -- --print-fonts
```

## Known gaps

* Text inputs: printable keys, paste, backspace/delete and caret movement; no
  IME, selection ranges or mouse caret placement.
* GPUI 0.2.2 does not re-layout nowrap text when a definite width arrives, hence
  `src/text.rs` pre-fitting for the two truncating table columns.
* `animate-spin` / `animate-pulse` are static; backdrop blur has no GPUI
  equivalent (the scrim colour is unchanged).
