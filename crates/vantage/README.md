# Vantage — native GPUI dashboard

The native desktop app for Vantage, built with [GPUI](https://gpui.rs).
It renders Vantage's real git data through `vantage-core` — no webview.

Run these commands from the repository root:

```sh
cargo build -p vantage   # build the app
cargo run -p vantage     # build and launch it
```

The binaries are written to `target/debug/vantage` (debug) and
`target/release/vantage` (release).

## Where the data comes from

`crates/vantage-core` is Vantage's shared engine: git CLI runner, status,
log, refs, diffs, stash and JSON state persistence. This crate adds the GPUI
surface and a thin adapter over that engine.

| File | Role |
|---|---|
| `src/main.rs` | Binary entry point: embedded assets, application setup and window creation. |
| `src/app.rs` | Dashboard state, list filtering/sorting, background jobs, prompts and the root view. |
| `src/root.rs` | Thin window root that restores GPUI invalidation for the dashboard entity. |
| `src/backend/mod.rs` | Owns a `tokio::runtime::Runtime`, loads and saves `PersistedState`, and exposes blocking snapshot and repository operations, groups and settings. |
| `src/backend/repo_map.rs` | Maps `RepoStatus` + `RepoEntry` onto the UI's `Repository` (sync state, ahead/behind, activity, stash). |
| `src/backend/details.rs` | Branches, recent commits, changed files with per-file stats and diff previews, plus remote normalisation and commit identity. |
| `src/model.rs` | View model (`Repository`, `BranchInfo`, `ChangedEntry`, …). |
| `src/theme.rs` | The dashboard palette. |
| `src/views/` | The sidebar, top bar, repository table, viewport, pending changes, inspector, add-repository modal and settings modal. |
| `src/ui/` | Shared design-system primitives (buttons, inputs, tabs, toggles, status badges, avatars and tooltips). |
| `src/settings.rs` | Typed v2 preferences, legacy-theme import, diagnostics and validation. |
| `src/menu.rs` | Native application menu bar and shortcuts. |
| `src/chrome.rs` | Native window-drag helper for the custom title bar. |
| `src/icons.rs` | Embedded Lucide icons and brand raster assets. |
| `src/app_icon.rs` | Launcher / Dock icon for unbundled builds. |
| `src/avatar_cache.rs` | Cached GitHub avatars for the commit identity. |
| `src/path.rs` | Home-directory contraction for displayed paths. |
| `src/text.rs` | Ellipsis fitting via GPUI's own line wrapper. |
| `src/watcher.rs` | `notify` watchers over repo paths; a debounced poll refreshes the changed rows through the backend. |

Groups and per-repo group assignments are persisted inside Vantage's own
`state.json` under `extra["vantage"]`, so there is a single source of truth.

## Dev flags

```sh
VANTAGE_STATE=dark,sidebar,section:changes,inspector:first,itab:branches,modal cargo run -p vantage
cargo run -p vantage -- --print-fonts
```

## Known gaps

* Text inputs: printable keys, paste, backspace/delete and caret movement; no
  IME, selection ranges or mouse caret placement.
* GPUI 0.2.2 does not re-layout nowrap text when a definite width arrives, hence
  `src/text.rs` pre-fitting for the two truncating table columns.
* `animate-spin` / `animate-pulse` are static; backdrop blur has no GPUI
  equivalent (the scrim colour is unchanged).
