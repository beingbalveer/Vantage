# Vantage — native GPUI dashboard

The native desktop app for Vantage, built with [GPUI](https://gpui.rs).
It renders Vantage's real Git data through `vantage-core` — no webview.

Run these commands from the repository root:

```sh
cargo build -p vantage   # build the app
cargo run -p vantage     # build and launch it
```

The binaries are written to `target/debug/vantage` (debug) and
`target/release/vantage` (release).

## Where the data comes from

`crates/vantage-core` is the Git engine: a `git` CLI runner plus status, refs,
log, diff, stash, commit/amend/reset operations, the workspace repository model
and `state.json` persistence. This crate adds the GPUI adapter:

| File | Role |
|---|---|
| `src/backend/mod.rs` | Owns a private `tokio::runtime::Runtime`; its synchronous methods `block_on` that runtime. Reads: `snapshot`, `snapshot_subset`, `groups`, `ui_settings`, `state_path`, `repo_path`, `repo_ids`, `stashes`, `compare`, `commit_files`, `origin_page`. Writes: `add_repo`, `create_repo`, `clone_repo`, `forget`, `fetch`, `pull`, `push`, `fetch_all`, `checkout`, `create_branch`, `rename_branch`, `delete_branch`, `merge`, `update_from_default`, `revert`, `commit`, `amend`, `undo_commit`, `reset_to`, `discard`, `discard_paths`, `stash_all`, `stash_pop`, `stash_drop`, `set_staged`, `set_all_staged`, `ignore_path`, `set_groups`, `set_ui_settings`. |
| `src/backend/repo_map.rs` | Maps `RepoEntry` + `RepoStatus` onto the UI's `Repository` (sync state, ahead/behind, activity, stash count). |
| `src/backend/details.rs` | Branches, recent commits, changed files with per-file stats and bounded diff previews, commit files, branch comparison, commit identity and `origin` detection. |
| `src/model.rs` | View model (`Repository`, `BranchInfo`, `ChangedFile`, `CommitFile`, …). |
| `src/app.rs` | `VantageApp`: rows, filters, selection, scan scheduling, git actions, context menus and modals, plus the root render. |
| `src/views/` | Sidebar, top bar, viewport (repository table / pending changes), inspector, add-repository modal and settings dialog. Each module only adds `impl VantageApp` methods. |
| `src/ui/` | Design system: tokens, icons and spinner, buttons, single-line inputs, tabs, toggle, avatar, status chrome, text helpers and tooltips. |
| `src/settings.rs` | Typed v2 preferences, legacy-theme import, validation and diagnostics. |
| `src/watcher.rs` | `notify` watcher over repository roots; the app drains its dirty set on a timer and rescans only those roots. |
| `src/avatar_cache.rs` | GitHub profile photos downloaded with `curl` and cached beside `state.json`. |
| `src/menu.rs` | Native application menu bar and shortcuts. |
| `src/root.rs`, `src/chrome.rs` | Root entity for GPUI invalidation and the macOS window-drag helper. |
| `src/path.rs`, `src/text.rs`, `src/icons.rs`, `src/app_icon.rs` | Path contraction, ellipsis fitting, the embedded asset source and the Dock icon for unbundled builds. |

Groups and per-repository group assignments are persisted inside Vantage's own
`state.json` under `extra["vantage"]`, alongside the v2 settings, so groups,
assignments and preferences share one workspace file.

## Dev flags

```sh
VANTAGE_STATE=dark,sidebar,section:changes,inspector:first,itab:branches,modal cargo run -p vantage
cargo run -p vantage -- --print-fonts
```

`VANTAGE_STATE` accepts `dark`, `light`, `sidebar`, `newgroup`, `modal`,
`modalpath:<path>`, `section:overview|attention|changes`, `inspector:first`,
`inspector:<repo-id>`, `itab:changes|commits|branches`, `group:<name>`, `persist`
and `fetch`.

## Known gaps

* Text fields are single-line. They handle printable characters, paste, cut,
  copy, select-all, shift-selection, caret movement and backspace/delete, but not
  IME composition, click-to-place-a-caret (a click focuses the field and moves
  the caret to the end, or selects all on a double-click), drag selection,
  undo/redo or multiline input.
* GPUI 0.2.2 does not re-layout nowrap text when a definite width arrives, so
  `src/text.rs` pre-fits commit subjects in the Commits tab. Other truncation
  uses GPUI's own `truncate` styling.
* Watcher events under `.git` are ignored, so commits, ref changes or index
  updates made by another program appear on window activation, manual refresh or
  the next auto-fetch (when enabled) rather than immediately.
* Modal backdrops are a solid overlay colour, not a blurred backdrop.
