# Vantage architecture

Vantage is a native desktop dashboard for several Git repositories at once. It has
no webview: the UI is drawn by GPUI and the Git engine runs in the same process.

## Workspace

```text
crates/
├── vantage-core/        Git engine, persistence and repository model
└── vantage/             GPUI desktop application
    ├── assets/          Fonts, icons and the Vantage mark
    └── src/
        ├── main.rs      Application entry point, assets, window and menus
        ├── app.rs       Application state and actions
        ├── menu.rs      Native macOS menu bar
        ├── model.rs     UI view model
        ├── path.rs      Path display helpers
        ├── theme.rs     Light and dark palettes
        ├── widgets.rs   Shared controls and text-field editing
        ├── text.rs      GPUI text fitting helpers
        ├── icons.rs     Embedded SVG/PNG asset source
        ├── watcher.rs   Repository filesystem watcher
        ├── views/       Dashboard views and inspector
        └── backend/     Engine adapter and repository mapping
```

The dependency direction is one way:

```text
vantage (GPUI) ──> vantage-core (no UI dependencies)
```

## Runtime flow

1. `Backend::snapshot()` reads the persisted workspace, asks `vantage-core` for each
   repository's Git status, and maps the result into `model::Repository` rows.
2. `VantageApp` owns those rows and the active view, filter, selection and toast state.
3. `views/` renders the sidebar, top bar, overview, pending changes and
   inspector from that state.
4. User actions call `Backend` on a background executor, then rescan and repaint.
5. `watcher.rs` reports changed repository roots; only those roots are re-read with
   `Backend::snapshot_subset`, so a large workspace does not pay for a full scan on every
   file event.

Full scans always replace the repository list, including when it becomes empty. Scan
failures are returned separately and shown in the viewport banner, so one broken work
tree cannot blank the dashboard.

## State

The engine owns the workspace file:

```text
macOS:   ~/Library/Application Support/vantage/state.json
Linux:   $XDG_DATA_HOME/vantage/state.json (or ~/.local/share/vantage/state.json)
Windows: %APPDATA%\vantage\state.json
```

Repositories and their persisted metadata are in `repos`. The dashboard's groups and
per-repository assignments live in `extra["vantage"]`:

```json
{
  "groups": ["Ungrouped", "SE"],
  "assignments": { "<repo-id>": "SE" }
}
```

An older build wrote the same object under `extra["branchdeck"]`. `Backend::new()`
migrates that key once and leaves every other `extra` key untouched.

## Actions

All Git writes go through `vantage-core` and the system `git` binary:

- stage/unstage one path with literal pathspecs;
- check out an existing branch;
- fetch from `origin`.

The UI does not optimistically mutate Git state. It shows the result of a fresh scan,
and surfaces Git's own error text when an action fails.

## Tests

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

`vantage-core` tests exercise real temporary Git repositories. The UI tests cover the
diff-preview parser, repository mapping, group migration and modal behavior.
