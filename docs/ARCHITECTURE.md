# Vantage architecture

Vantage is a native desktop dashboard for several Git repositories at once. It has
no webview: GPUI draws the UI and the Git engine runs in the same process.

## Workspace

```text
crates/
├── vantage-core/        Git engine, persistence and repository model
│   └── src/
│       ├── git/         runner, status, refs, log, diff, ops, stash, clone, …
│       ├── repos.rs     repository inventory and workspace state load/save
│       ├── persist.rs   state-file location, atomic writes, corruption recovery
│       ├── state.rs     persisted schema and shared status/commit types
│       ├── watch.rs     engine-level watcher/auto-fetch helpers (not used by the GPUI app)
│       └── …            detect, triage, events, error
└── vantage/             GPUI desktop application
    ├── assets/          bundled fonts, Lucide icons and the Vantage mark
    └── src/
        ├── main.rs      entry point, fonts, window and env-driven QA state
        ├── app.rs       VantageApp state, actions, root render and modals
        ├── root.rs      window root that renders VantageApp as a child entity
        ├── backend/     engine adapter, repository mapping and detail queries
        ├── views/       sidebar, top bar, viewport, inspector and modals
        ├── ui/          design system primitives
        ├── settings.rs  typed v2 preferences and diagnostics
        ├── watcher.rs   working-tree watcher
        ├── model.rs     UI view model
        ├── theme.rs     light and dark palettes, fonts and type scale
        ├── icons.rs     embedded SVG/PNG asset source
        ├── avatar_cache.rs  GitHub avatars fetched with `curl`
        ├── menu.rs      native menu bar and shortcuts
        ├── text.rs      ellipsis fitting for commit messages
        ├── path.rs      `~` contraction for displayed paths
        ├── chrome.rs    macOS window-drag helper
        └── app_icon.rs  Dock icon for unbundled builds
```

The dependency direction is one way:

```text
vantage (GPUI) ──> vantage-core (no UI dependencies)
```

## App modules

`views/` modules only add `impl VantageApp` methods; `app.rs` owns the state and
composes the pieces. No view reaches into another view module directly.

| Module | Role |
|---|---|
| `views/viewport.rs` | Scroll shell, section switch (`Overview`/`Attention` show the table, `Changes` shows pending changes), empty state and scan-error banner. |
| `views/repo_table.rs` | Repository list grouped by workspace, with the summary filter strip, row status/sync cells and sort order. |
| `views/pending_changes.rs` | Cross-repository changed-file list with per-repo totals. |
| `views/sidebar.rs` | Group list, create/delete group, group filtering and the Settings entry. Collapses to a 56 px rail. |
| `views/topbar.rs` | Merged 42 px header: window drag region, sidebar toggle, refresh, add repository and right-panel toggle. |
| `views/inspector.rs` | Right detail pane: Changes, Commits and Branches tabs, commit bar, undo strip, commit diff, branch and sync actions. |
| `views/add_repo_modal.rs` | Add-repository dialog with path browsing, detection and group choice. |
| `views/settings.rs` | Settings dialog: General, Appearance, Git, Data and About sections. |

`ui/` is the shared design system. Views import control chrome from `crate::ui`
instead of hand-rolling it; helpers that only one view uses stay beside that view.

| Module | Role |
|---|---|
| `ui/tokens.rs` | Control heights, radii and the `IconSize` scale. |
| `ui/icon.rs` | `icon()`, `icon_button()` and the animated `spinner()`. |
| `ui/button.rs` | Primary, secondary, accent and disabled labeled buttons. |
| `ui/input.rs` | Single-line `FieldState`, key handling and field chrome. |
| `ui/tabs.rs` | Underline tabs and segmented choice chips. |
| `ui/toggle.rs` | Checkbox and switch. |
| `ui/avatar.rs` | Initials tile and GitHub photo avatar. |
| `ui/status.rs` | Repository status badges and sync arrows. |
| `ui/text.rs` | `Txt` type helpers built on the theme scale. |
| `ui/tooltip.rs` | Text and commit-identity tooltips. |

`backend/` isolates the async engine from GPUI:

| Module | Role |
|---|---|
| `backend/mod.rs` | `Backend` owns a private `tokio::runtime::Runtime` and exposes synchronous methods that `block_on` internally. Covers snapshots, workspace add/clone/create/forget, staging, commit/amend/undo/reset, branches, fetch/pull/push, merge/update, revert, stash, discard, ignore, groups and settings. |
| `backend/repo_map.rs` | Maps a persisted `RepoEntry` plus `RepoStatus` onto `model::Repository` (sync state, ahead/behind, activity, stash count). |
| `backend/details.rs` | Branches, recent commits, changed files with stats and bounded diff previews, commit files, branch comparison, commit identity and `origin` detection. |

## Runtime flow

1. `main.rs` registers the bundled fonts and the Dock icon, reads `UiSettings`
   for the saved window bounds, and opens the window with `root::RootView`
   wrapping a new `VantageApp`. It installs the native menu, observes window
   appearance and activation, then starts the first scan and the auto-fetch loop.
2. `VantageApp::new` creates the `Backend` (which migrates the legacy group key
   on startup) and loads groups, assignments and v2 settings. Rows start empty.
3. `start_scan` runs `Backend::snapshot()` on GPUI's background executor. The
   backend reads `PersistedState`, sorts `repos` (pinned first, then persisted
   order), queries each entry's Git status and maps it to a `model::Repository`.
   Per-repository failures are returned separately with Git's own message.
4. The UI-thread update replaces the row list (including an empty result), appends
   groups found on disk, drops a missing group filter or selection, prunes tick
   overrides, rebinds the watcher and performs the one-time fetch-on-launch when
   enabled. A scan sequence number discards results from an older, superseded
   scan.
5. Selecting a repository stores its id, opens the right pane and persists the
   pane state. `views/inspector.rs` renders that repository; selecting another
   one resets the expanded file, context menu and copied-path state.
6. Repository mutations mostly run through `VantageApp::run_repo_op`, which
   allows one operation per repository at a time, marks the row and detail pane
   busy, runs the backend call on the background executor and starts a full
   rescan when it returns. Success shows a toast; failure shows Git's error text.
   Staging, branch checkout, add/create/clone and ignore run their own spawned
   tasks. Staging updates the tick immediately and only rescans that repository
   if the Git call fails; checkout and the workspace actions rescan on
   completion. Git state is not optimistically mutated, with one deliberate
   exception: a file tick flips its checkbox immediately so it does not wait on a
   process, and that choice is kept until the scan reflects it or a commit
   spends it.
7. `start_scan_dirty` uses `Backend::snapshot_subset` for the roots reported by
   the watcher. It replaces only the affected rows and leaves the rest of the
   workspace untouched, adding groups the same way a full scan does.
8. Scan failures are shown in the viewport banner above the active section, so
   one broken working tree cannot blank the dashboard.

## Watcher

`src/watcher.rs` owns one `notify::RecommendedWatcher` and a shared dirty set:

- Each scanned repository root is watched recursively. Events are collected on
  notify's own thread into a shared set; the render thread only drains that set
  on its timer, so it does not wait on filesystem events.
- The callback accepts create, modify and remove events. Any event whose path
  contains a `.git` component is ignored; this covers both the `.git` directory
  and the `.git` file used by worktrees and submodules, and prevents Vantage's
  own status calls (which refresh the index) from marking the repository dirty.
- A changed path is mapped back to the watched root that contains it, and that
  root is added to the dirty set.
- `VantageApp` polls the dirty set every 700 ms, drains it and calls
  `start_scan_dirty`, which performs a subset scan. The poll is the debounce.
- After every full scan, `bind_watcher` replaces the watched root set. If the
  set changed it unwatches the old roots, watches the new ones and clears the
  dirty set; if it is unchanged, the existing watches are kept.
- Window activation also triggers a full refresh, throttled to one scan every
  two seconds. It is silent: the current rows stay on screen until the new
  snapshot arrives. Auto-fetch is a separate 60-second timer that runs
  `fetch_all` (at most four repositories at a time) when the configured interval
  is due.

Limits worth knowing:

- Changes made under `.git` by another program (commits, refs, index updates)
  do not wake the watcher. They appear on window activation, a manual refresh,
  or the next auto-fetch.
- `notify` is the platform backend; directory event semantics differ by OS.
  Vantage only depends on "some path under this root changed".
- Watch registration errors are ignored. A root that does not exist yet is
  skipped, and if the watcher cannot be created the app still scans on demand
  and on activation.

## State

The engine owns the workspace file:

```text
macOS:   ~/Library/Application Support/vantage/state.json
Linux:   $XDG_DATA_HOME/vantage/state.json (or ~/.local/share/vantage/state.json)
Windows: %APPDATA%\vantage\state.json
```

On startup, a legacy data directory named `relay` is renamed to `vantage` before
the state file is opened. The file is written atomically (temporary file plus
rename); a corrupt file is backed up as `state.corrupt-<timestamp>.json` and an
unreadable or corrupt file both start the app from defaults.

`PersistedState` has top-level `version`, `repos`, `layout`, `settings`,
`windowBounds` and any extra keys. The dashboard adds and removes repository
entries through `vantage_core::repos` and keeps its own data under the extra key
`vantage`; because `extra` is flattened, that key appears at the top level:

```json
{
  "repos": [
    {
      "id": "4f6c2f7e-8a1d-4c3b-9f2e-1a2b3c4d5e6f",
      "path": "/Users/me/Projects/vantage",
      "name": "vantage",
      "branch": null,
      "pinned": false,
      "order": 0,
      "marker": null,
      "notes": []
    }
  ],
  "vantage": {
    "groups": ["Ungrouped", "SE"],
    "assignments": { "4f6c2f7e-8a1d-4c3b-9f2e-1a2b3c4d5e6f": "SE" },
    "settings": {
      "version": 1,
      "theme": "system",
      "defaultView": "overview",
      "sort": "recent",
      "sidebarCollapsed": false,
      "sidebarWidth": 256.0,
      "autoFetchMinutes": 0,
      "fetchOnLaunch": false,
      "pullStrategy": "fastForward",
      "defaultBranch": "main",
      "rememberWindow": true,
      "windowBounds": null,
      "rightPanelOpen": false,
      "rightPanelFraction": 0.67
    }
  }
}
```

- `groups` is the ordered group list and `assignments` maps repository ids to
  group names. A repository with no assignment is shown as `Ungrouped`.
- The v2 preferences live under `vantage.settings`, version `1`. `theme` is
  `system`, `light` or `dark`; `defaultView` is `overview`, `attention` or
  `changes`; `sort` is `recent`, `name`, `changes` or `attention`;
  `pullStrategy` is `fastForward`, `merge` or `rebase`.
- Group writes merge into the `vantage` object so settings survive, and settings
  writes merge so groups survive. Any other key in that object is preserved.
- The legacy top-level `settings`, `layout` and `windowBounds`, and per-repo
  `marker`/`notes`, are left untouched. On first run without a v2 object, only
  the old theme choice is imported; auto-fetch stays off.
- State saved by a pre-rename build could carry groups and assignments under
  `extra["branchdeck"]`. `Backend::new` copies them once, then drops that key;
  group writes also remove it.
- Loaded settings are sanitized: `autoFetchMinutes` must be `0`, `1`, `5`, `15`,
  `30` or `60`; `defaultBranch` must be a valid branch name (falling back to
  `main`); saved window bounds must be sane (at least 1040×640); `sidebarWidth`
  is clamped to 200–360 and `rightPanelFraction` to 0.3–0.8. The old 0.5 pane
  fraction migrates to 0.67.

## Git write surface

All Git history, index and working-tree mutations go through `vantage-core`'s
Git layer, which drives the system `git` binary; history-changing operations take
a per-repository lock. The action path only writes files directly for the
optional `.gitignore` append and the `.gitignore` seed of a newly created
repository; avatar images and workspace state are the app's other writes.

| Area | Behaviour |
|---|---|
| Workspace | Add validates a work tree with Git, then persists it. New repository runs `git init -b <defaultBranch>` and optionally seeds `.gitignore`. Clone runs `git clone` and registers the result. Forget removes the entry only; the folder and history stay. |
| Staging | Stage or unstage one path, or the whole list, with literal pathspecs. Stage-all is one Git call, not a loop. |
| Commit | Only ticked paths are committed. The backend stages that exact list, then `git commit --only -- <paths>`; core checks for unresolved conflicts and a usable commit identity first. A failure leaves the message in place for a retry. Success records the short SHA for the Undo strip. |
| Amend | Rewrites the tip: selected paths are staged, a backup ref is written, then `git commit --amend --only -- <paths>`. The app sends the typed subject; the commit body is not currently passed. Core refuses while a merge, rebase, cherry-pick or revert is in progress. |
| Undo | The strip is offered while the row reports unpushed commits (`ahead_count > 0`). The session record makes it appear immediately after committing, before the row list catches up. The backend refuses a commit that is already on a remote; core writes a backup ref, then runs `git reset --soft HEAD~1`, leaving its changes staged. |
| Reset | Confirmed first, then the branch tip moves to a commit with `git reset --hard <sha>`. The backend refuses when the working tree is dirty; a backup ref of the old `HEAD` is written first. |
| Discard | One file or every changed path. Tracked changes are backed up as a stash and restored with `git checkout HEAD --`; untracked files are moved to `<git-dir>/vantage-trash/<id>`. |
| Stash | Stash all changes including untracked files with `git stash push -u`. The stash dialog lists entries and can pop or drop one; the repository menu can also pop the newest. |
| Branches | Check out an existing branch; create at a start point and check it out; rename; delete with the non-forcing `git branch -d`, so Git refuses unmerged work. |
| Sync | Fetch (`--prune`), pull with the configured fast-forward/merge/rebase strategy, push (publishing with `--set-upstream` when the branch tracks nothing), merge another branch, fetch and merge `origin/<default>`, and revert a commit as a new commit. |

Destructive and history-rewriting operations write recovery data inside the
repository under `refs/vantage/backup/*` with a matching record in
`<git-dir>/vantage-backups/`. A discard, amend, undo or reset therefore leaves
recoverable data in the repository itself rather than only in Vantage's state.

## Tests

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

`vantage-core` tests exercise real temporary Git repositories (status parsing,
refs, stash, commit/amend/undo, discard and state persistence). The app tests
cover repository mapping, settings validation and legacy import, group
persistence, text-field editing, summary sorting, the undo offer and ticked-file
selection.
