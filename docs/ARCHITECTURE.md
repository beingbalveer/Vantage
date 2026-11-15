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
        ├── ui/          Design system: icons, buttons, inputs, tabs, switches
        ├── widgets.rs   View-specific status and sync helpers
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

## Design system

Shared visual primitives live in `src/ui/` so views do not hand-roll their own
control chrome:

- `ui/tokens.rs` — control heights, radii and the `IconSize` scale
- `ui/icon.rs` — `icon()` plus the standard toolbar `icon_button()`
- `ui/button.rs` — primary and secondary labeled buttons
- `ui/input.rs` — single-line field state, key handling and chrome
- `ui/tabs.rs` — underline tabs and segmented choice chips
- `ui/toggle.rs` — checkbox and switch

Views import from `crate::ui`. `widgets.rs` is reserved for view-specific helpers
such as repository status text and sync arrows.

## Runtime flow

1. `Backend::snapshot()` reads the persisted workspace, asks `vantage-core` for each
   repository's Git status, and maps the result into `model::Repository` rows.
2. `VantageApp` owns those rows and the active view, filter, selection and toast state.
3. `views/` renders the sidebar, top bar, overview, pending changes and
   inspector from that state.
4. User actions call `Backend` on a background executor, then rescan and repaint.
5. Selecting a repository opens the right detail pane beside the overview. The pane is
   resizable, remembered in settings, and can be hidden from the top-right panel button.
6. `watcher.rs` reports changed repository roots; only those roots are re-read with
   `Backend::snapshot_subset`, so a large workspace does not pay for a full scan on every
   file event. Window activation also triggers a throttled, silent full refresh that
   leaves the current rows on screen until the new snapshot arrives.

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

Repositories and their persisted metadata are in `repos`. Dashboard settings,
groups and per-repository assignments share the `extra["vantage"]` object:

```json
{
  "groups": ["Ungrouped", "SE"],
  "assignments": { "<repo-id>": "SE" },
  "settings": {
    "version": 1,
    "theme": "system",
    "defaultView": "overview",
    "sort": "recent",
    "sidebarCollapsed": false,
    "autoFetchMinutes": 0,
    "fetchOnLaunch": false,
    "defaultBranch": "main",
    "rememberWindow": true,
    "windowBounds": null
  }
}
```

Settings writes merge into that object, so group writes preserve settings and settings
writes preserve groups. The legacy top-level `settings`, `layout`, markers and notes are
left untouched. An older build wrote groups under `extra["branchdeck"]`; `Backend::new()`
merges that key once and leaves every other `extra` key untouched.

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
