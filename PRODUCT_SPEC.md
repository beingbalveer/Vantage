# Relay: Product & Engineering Spec

> Working name: **Relay** (placeholder, rename freely).
> Audience: an AI coding agent that will build this app end to end. Read this file fully, then read `DESIGN.md`. Build milestone by milestone (section 14). Do not add features that are not in this spec.

---

## 1. Summary

Relay is a **lightweight, local, multi-repo Git client and read-only code viewer** for a solo developer who lets AI agents (Codex, Antigravity, etc.) write and commit the code. The human does not hand-edit code in this app. The human **reviews, understands, and steers**.

One line: *GitHub Desktop's calm simplicity, but multi-repo, history-first, with a "reviewed" marker, split panes, and a terminal per repo.*

### 1.1 The user

- Solo developer, works on **5 to 6 active repositories at the same time**.
- AI agents write the code **and commit it** (mostly). Worktrees are rarely used.
- Reviews **mostly in history after the fact**, sometimes checks uncommitted files before committing.
- Wants to see **two repos side by side** sometimes.
- Likes GitHub Desktop's UI. Finds other clients (branch graphs, PR tooling, merge tools) too complicated.
- Wants a **very lightweight** app. Prefers **Rust**.
- Currently juggles GitHub Desktop plus several VS Code windows. That window switching is the pain to remove.

### 1.2 The problem

Today the user must: open GitHub Desktop to see what changed, open VS Code to read surrounding code, and switch between many VS Code windows, one per project. There is no single place that answers: **"What did the agents do since I last looked, in each of my projects?"**

### 1.3 Core loop (this must feel instant)

1. Glance at the sidebar: `web +7`, `api +2` (new commits since last review).
2. Open `web`. See one **combined diff** of everything since the reviewed marker.
3. Skim. Expand context around a hunk, or peek a symbol's definition, without leaving the app.
4. A commit looks suspicious (flagged), so step into it, or revert it.
5. Press `R` to mark reviewed. Press `Mod+K` to jump to `api`. Repeat.

### 1.4 Design principles

1. **Review, not authoring.** No code editing. No staging complexity.
2. **One mental model.** Uncommitted changes are shown as a pseudo-commit at the top of history.
3. **Calm by default.** Few controls on screen. Power is behind the command palette and keyboard shortcuts.
4. **Lightweight is a feature.** Tauri, system webview, no Monaco, no heavy frameworks. Budgets in section 11 are requirements.
5. **Safe by default.** Every destructive action creates a recoverable backup first.
6. **Shell out to `git`.** 100% compatibility with the user's real setup (config, hooks, credential helpers, signing).

### 1.5 Explicit non-goals (v1)

No PR / issue / GitHub API integration. No merge, rebase, cherry-pick UI. No branch graph visualization. No stash management UI. No worktree UI. No code editing. No LSP. No submodule UI. No Git LFS UI. No accounts or cloud sync. No telemetry. If the user needs any of these, the embedded terminal is the answer.

---

## 2. Core concepts (glossary)

| Term | Definition |
|---|---|
| **Repo** | A local git repository registered in Relay by its absolute path. |
| **Reviewed marker** | Per repo, a stored commit SHA meaning "I have reviewed everything up to here." Stored in Relay's own state file, never inside the repo. |
| **New commits** | Commits reachable from `HEAD` but not from the marker: `git rev-list <marker>..HEAD`. Count shown as `+N` badge. |
| **Review range** | A diff scope. Default scope when a repo has new commits: **marker to HEAD, combined** (net effect). Can also be one commit or a shift-selected range of commits. |
| **Uncommitted pseudo-commit** | A virtual row pinned at the top of the history list when the working tree is dirty. Selecting it shows working tree changes and reveals the commit box. |
| **Triage flags** | Small icons on commits/ranges marking things worth a closer look (large change, deletions, lockfile, config, removed tests, possible secrets). Rules in section 8. |
| **Pane** | A workspace area showing exactly one repo. The main area holds 1 to 3 panes side by side. |
| **Terminal session** | One persistent PTY per repo, with cwd at the repo root. Survives switching panes or repos. |
| **Review note** | A short note the user attaches to a line range while reviewing (feedback for the agent). Collected per repo, copyable as a prompt or sendable to the terminal. |
| **Backup ref** | A ref `refs/relay/backup/<timestamp>-<kind>` created before any destructive action so it can be undone. |

---

## 3. Platform & stack (decisions made, follow these)

| Layer | Choice | Notes |
|---|---|---|
| App shell | **Tauri v2** (Rust backend, system webview) | Small binary, low RAM. |
| Backend language | **Rust**, edition 2021, stable toolchain | |
| Frontend | **Svelte 5 + TypeScript + Vite** | No UI component library. Hand-written CSS with CSS variables (see DESIGN.md). |
| Git access | **Shell out to the `git` CLI** via `tokio::process::Command` | Do not use libgit2/gix in v1. Isolate behind a `GitRunner` trait so `gix` can replace hot paths later. |
| Terminal backend | `portable-pty` crate | Cross-platform (ConPTY on Windows). |
| Terminal frontend | `@xterm/xterm` + `@xterm/addon-fit` | Use the DOM or canvas renderer. No WebGL addon by default. |
| File watching | `notify` crate (debounced) | Watch `.git` internals for commit events. See section 9. |
| Async runtime | `tokio` | |
| Serialization | `serde`, `serde_json` | |
| Diff parsing | Own parser in Rust over `git diff`/`git show` output; `similar` crate for intra-line word diff | |
| Syntax highlighting | `highlight.js` core + lazily imported language modules | Highlight the **whole file blob**, then split into lines (re-open/close spans across line breaks). Skip highlighting for files over 1 MB or lines over 2,000 chars. |
| Icons | `lucide` (ISC license), tree-shaken inline SVG, 16px | |
| Fuzzy search | Small custom scorer in TS (subsequence match with bonuses for word starts) | No heavy dependency. |
| State persistence | Single JSON file in the OS app-data dir (`relay/state.json`), atomic write (temp file + rename), debounced 500 ms | |
| Testing | Rust: `cargo test` with temp repos; Frontend: `vitest` | |

**Operating system:** the user has not stated their OS. Build **cross-platform from day one** (Windows, macOS, Linux). Rules:

- Never assume `/` path separators or a POSIX shell. Use `std::path` and normalize paths sent to the frontend (forward slashes, display only).
- Default shell: `$SHELL` on Unix; on Windows prefer `pwsh.exe`, fall back to `powershell.exe`, then `cmd.exe`. Overridable in settings.
- Keyboard: `Mod` = `Cmd` on macOS, `Ctrl` elsewhere.
- Do not hard-code `git` location. Resolve via PATH; show a clear first-run error if missing (section 12).

---

## 4. Architecture

```
┌───────────────────────── Tauri window (webview) ─────────────────────────┐
│ Svelte UI: Sidebar · Panes[1..3] · CommandPalette · Dialogs · Toasts      │
│   stores: repos, panes, selection, settings, notes, terminals             │
└───────────────▲──────────────────────────────┬────────────────────────────┘
        events (Rust → UI)                 commands (UI → Rust, invoke)
┌───────────────┴──────────────────────────────▼────────────────────────────┐
│ Rust backend                                                              │
│  state::Store      app state + persistence                                │
│  git::Runner       spawn git, timeouts, per-repo write lock, parsers      │
│  git::{log,diff,status,ops,refs,blob,grep}                                │
│  triage::          flag rules                                             │
│  watch::           per-repo watchers + status poller                      │
│  term::            PTY sessions, ring buffers                             │
│  cmd::             #[tauri::command] handlers (thin)                      │
└───────────────────────────────────────────────────────────────────────────┘
```

### 4.1 Rust project layout

```
relay/
├─ package.json, vite.config.ts, svelte.config.js, tsconfig.json
├─ src/                         # frontend
│  ├─ main.ts, App.svelte
│  ├─ lib/
│  │  ├─ api.ts                 # typed wrappers around invoke() and listen()
│  │  ├─ types.ts               # mirrors Rust models (section 5)
│  │  ├─ stores/                # repos.ts panes.ts settings.ts notes.ts terminals.ts ui.ts
│  │  ├─ keys.ts                # central shortcut registry
│  │  ├─ fuzzy.ts, format.ts, highlight.ts, virtual-list.ts
│  │  └─ components/
│  │     ├─ Sidebar/ (RepoRow, SidebarFooter)
│  │     ├─ Pane/ (PaneHeader, SyncButton, BranchMenu, HistoryList, CommitRow, MarkerDivider,
│  │     │         RangeHeader, FileList, FileRow, CommitBox, FilesTab, FileTree)
│  │     ├─ Diff/ (DiffView, DiffFileHeader, DiffHunk, DiffLine, ExpandContext, WordDiff)
│  │     ├─ Viewer/ (CodeViewer, PeekPopover)
│  │     ├─ Terminal/ (TerminalDrawer, TerminalView)
│  │     ├─ Notes/ (NotesPanel, NoteEditor)
│  │     ├─ Palette/ (CommandPalette)
│  │     └─ common/ (Button, IconButton, Menu, Dialog, Toast, Badge, Chip, Tooltip, Splitter, Kbd)
│  └─ styles/ (tokens.css, base.css, themes.css)
└─ src-tauri/
   ├─ Cargo.toml, tauri.conf.json, capabilities/
   └─ src/
      ├─ main.rs, lib.rs
      ├─ state.rs, persist.rs
      ├─ git/{mod,runner,log,diff,status,ops,refs,blob,grep}.rs
      ├─ triage.rs
      ├─ watch.rs
      ├─ term.rs
      ├─ cmd.rs
      └─ error.rs
```

### 4.2 Concurrency rules

- A **global semaphore (8)** limits concurrent `git` processes.
- A **per-repo `tokio::sync::Mutex`** serializes *write* operations (commit, revert, reset, pull, push, checkout, discard). Read operations do not take it.
- All read-only background `git` calls set env `GIT_OPTIONAL_LOCKS=0` so Relay never blocks the agent's own git operations with `index.lock`.
- Every `git` call has a **timeout** (default 20 s; 120 s for fetch/pull/push) and returns a typed error on expiry.
- Always set env: `LC_ALL=C`, `GIT_TERMINAL_PROMPT=0` (no interactive prompts, fail fast), `GIT_PAGER=cat`, and pass `-c core.quotepath=off -c color.ui=never`.
- Parse machine formats only (`-z`, `--porcelain=v2`, `--format` with `%x1f`/`%x1e` separators). Never parse human output.

---

## 5. Data models

Rust structs derive `Serialize, Deserialize` (camelCase). TypeScript mirrors live in `src/lib/types.ts`.

```rust
pub struct RepoEntry {
    pub id: String,              // uuid v4
    pub path: String,            // absolute, canonical
    pub name: String,            // folder name, user-renamable
    pub pinned: bool,
    pub order: u32,              // manual sort position
    pub marker: Option<Marker>,
    pub notes: Vec<ReviewNote>,
}

pub struct Marker { pub sha: String, pub set_at: String /* ISO-8601 */, pub branch: Option<String> }

pub struct RepoStatus {          // cheap, polled
    pub repo_id: String,
    pub branch: Option<String>,  // None when detached
    pub detached_sha: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32, pub behind: u32,
    pub dirty_count: u32,        // files changed in working tree (tracked + untracked)
    pub new_commit_count: u32,   // marker..HEAD
    pub marker_state: MarkerState, // Ok | Missing | Diverged | None
    pub last_activity_at: Option<String>, // newest of: HEAD reflog time, newest dirty file mtime
    pub error: Option<String>,   // e.g. path missing, not a repo
}

pub struct CommitSummary {
    pub sha: String, pub short: String,
    pub author_name: String, pub author_email: String,
    pub date: String,            // author date, ISO-8601
    pub subject: String, pub body: String,
    pub parents: Vec<String>,
    pub files_changed: u32, pub additions: u32, pub deletions: u32, // from --shortstat
    pub flags: Vec<TriageFlag>,  // computed lazily (see 8)
    pub is_new: bool,            // true if in marker..HEAD
}

pub struct FileChange {
    pub path: String, pub old_path: Option<String>,
    pub status: FileStatus,      // Added | Modified | Deleted | Renamed | Copied | TypeChanged | Untracked | Conflicted
    pub additions: u32, pub deletions: u32, pub binary: bool,
    pub old_mode: Option<String>, pub new_mode: Option<String>,
}

pub struct DiffFile {
    pub change: FileChange,
    pub hunks: Vec<Hunk>,        // empty + `truncated: true` when too large (see 6.4)
    pub truncated: bool,
}
pub struct Hunk { pub old_start: u32, pub old_lines: u32, pub new_start: u32, pub new_lines: u32,
                  pub header: String /* function context */, pub lines: Vec<DiffLine> }
pub struct DiffLine { pub kind: LineKind /* Context|Add|Del|NoNewline */,
                      pub old_no: Option<u32>, pub new_no: Option<u32>, pub text: String,
                      pub word_ranges: Option<Vec<(u32,u32)>> /* intra-line changed ranges */ }

pub enum DiffScope {
    Commit { sha: String },
    Range  { from: String /* exclusive */, to: String /* inclusive */ },   // combined: git diff from...to
    Working,                                                             // uncommitted (tracked + untracked)
}

pub struct TriageFlag { pub kind: FlagKind, pub severity: Severity /* Info|Caution|Warn */, pub detail: String }

pub struct ReviewNote {
    pub id: String, pub file: String, pub line_start: u32, pub line_end: u32,
    pub side: NoteSide /* New | Old */, pub commit_sha: Option<String>,
    pub text: String, pub created_at: String, pub snippet: String /* up to 12 lines at note time */
}

pub struct Settings {
    pub theme: Theme /* System|Light|Dark */, pub diff_mode: DiffMode /* Unified|Split */,
    pub ignore_whitespace: bool, pub context_lines: u32 /* default 3 */,
    pub auto_fetch_minutes: u32 /* 0 = off, default 5 */, pub shell: Option<String>,
    pub external_editor_cmd: Option<String>, // e.g. `code -g {file}:{line}` ; placeholders {repo} {file} {line}
    pub commit_message_cmd: Option<String>,  // reads diff on stdin, prints message on stdout; null = feature hidden
    pub terminal_font_size: u32, pub ui_scale: f32,
}
```

### 5.1 Persisted state file

`state.json`: `{ version: 1, repos: [RepoEntry], layout: { paneCount, panes: [{repoId, tab, widths..}], sidebarWidth, sidebarCollapsed, terminalHeight, terminalOpen }, settings: Settings, windowBounds }`. Unknown fields must be preserved on load (forward compatible). On corrupt JSON: rename to `state.corrupt-<ts>.json`, start fresh, show a toast.

---

## 6. Git operations (exact commands)

All run in the repo root (`-C <path>`). `{M}` = marker sha.

### 6.1 Reads

| Purpose | Command |
|---|---|
| Verify repo | `git rev-parse --show-toplevel --git-dir` |
| Status (poll) | `git status --porcelain=v2 --branch -z --untracked-files=normal` (parse branch.ab for ahead/behind, branch.head, branch.upstream) |
| New commit count | `git rev-list --count {M}..HEAD` |
| Marker validity | `git cat-file -e {M}^{commit}` (Missing) and `git merge-base --is-ancestor {M} HEAD` (Diverged if false) |
| History page | `git log --format=%H%x1f%an%x1f%ae%x1f%aI%x1f%P%x1f%s%x1f%b%x1e --shortstat -n 200 --skip N HEAD` (also `--first-parent` toggle later) |
| Search history | add `--grep=<q> -i` and/or `-S`/`-G` for pickaxe, or `--author=` |
| Commit files | `git show --numstat --summary -M --format= -z <sha>` (root commit: add `--root`) |
| Commit diff (one file) | `git show -M -U{ctx} --no-color --no-ext-diff --format= <sha> -- <path>` |
| Range diff files | `git diff --numstat -M -z {from} {to}` |
| Range diff (one file) | `git diff -M -U{ctx} --no-color --no-ext-diff {from} {to} -- <path>` |
| Working files | `git status` (above) plus `git diff --numstat -z HEAD` |
| Working diff (one file) | `git diff -M -U{ctx} --no-color --no-ext-diff HEAD -- <path>`; untracked: `git diff --no-index -- /dev/null <path>` (use `NUL` on Windows) |
| Whitespace toggle | append `-w` to diff commands |
| Blob at rev | `git show <rev>:<path>` (binary-safe, cap at 5 MB, else return `tooLarge`) |
| Tree at rev | `git ls-tree -r --name-only -z <rev>` (cached per HEAD sha, used by Files tab and palette) |
| Definition search | `git grep -n -I -E <pattern> <rev> -- <globs>` (see F10) |
| Branches | `git for-each-ref --format=%(refname:short)%1f%(upstream:short)%1f%(HEAD) refs/heads` |
| Remote check | `git branch -r --contains <sha>` (is a commit already on a remote) |

### 6.2 Writes (take the per-repo lock; create backup first where noted)

| Action | Command | Backup |
|---|---|---|
| Commit | `git commit -m <subject> [-m <body>]` with selected paths: `git add -- <paths>` then commit (or `git commit --only -- <paths>`). Respect hooks and signing (do not pass `--no-verify`). | none |
| Discard file(s) | tracked: `git checkout HEAD -- <paths>`; untracked: **move** to `.git/relay-trash/<ts>/<path>` | `git stash create` stored as `refs/relay/backup/<ts>-discard` (tracked part) plus the trash dir (untracked part) |
| Revert commit | `git revert --no-edit <sha>` (merge commits: `-m 1`). On conflict: `git revert --abort`, show error. Requires clean tree. | none (revert is itself reversible) |
| Reset to marker | `git reset --hard {M}` | `git update-ref refs/relay/backup/<ts>-reset HEAD` first |
| Checkout branch | `git checkout <branch>` (blocked with message if dirty and checkout would overwrite) | none |
| Fetch | `git fetch --prune` | none |
| Pull | `git pull --ff-only` | none |
| Push | `git push` (or `git push -u origin <branch>` to publish) | none |
| Undo backup | restore from backup ref (`git reset --hard <ref>` or `git stash apply <ref>`) | |

### 6.3 Authentication

Rely entirely on the user's configured credential helper / ssh-agent. With `GIT_TERMINAL_PROMPT=0` an auth failure returns quickly. Map known stderr patterns (`Authentication failed`, `Permission denied (publickey)`, `could not read Username`) to the friendly message: *"Git needs credentials. Run `git fetch` in the terminal below once to sign in."* with a **Open terminal** action.

### 6.4 Large diffs

- A file with more than **2,000 changed lines** or more than **1 MB** of diff text returns `truncated: true` and no hunks. UI shows "Large diff. Load anyway" which re-requests with `force: true`.
- Diff file list is returned immediately (numstat); hunks are **fetched lazily per file** as the file nears the viewport (or on expand). Never parse the whole range diff upfront.
- Binary files: show a placeholder (size old to new). Image preview is v1.1.

---

## 7. Features (detailed behavior)

Each feature has an ID. Acceptance criteria are in section 14.

### F1. Repository registry

- **Add repo:** `Mod+O` (folder picker), drag and drop a folder onto the sidebar, or palette command "Add repository". Validate with `rev-parse`; if the folder is not a repo, toast: "That folder isn't a Git repository." If a parent folder contains repos, offer "Scan this folder for repositories" (depth 2, list with checkboxes).
- **Duplicate** (same canonical path): select the existing one instead.
- **First add:** marker initializes to current `HEAD` (nothing new). Dialog option: "Treat the last N commits as unreviewed" (N input, default 0).
- **Remove:** context menu "Remove from Relay" (does not touch disk). Confirm only if the repo has review notes.
- **Rename (display only), pin, reorder** (drag), "Reveal in file manager", "Copy path", "Open in external editor".
- **Missing path** (folder deleted/moved): row shows a warning icon, dimmed; actions: "Locate…" (re-pick folder) or "Remove".
- Order: pinned first (manual order), then by `last_activity_at` descending. A setting "Sort: Activity / Manual" (default Activity).

### F2. Sidebar status badges

Per repo row (see DESIGN.md for visuals), driven by `RepoStatus`:

- `+N` blue pill: new commits since marker (hidden when 0; show `99+` cap).
- Amber dot: uncommitted changes present; tooltip "3 files changed".
- Activity pulse: green animated ring around the avatar when `last_activity_at` is within the last **90 seconds**, meaning "an agent is probably working here."
- Terminal glyph: tiny icon when the repo's terminal had output in the last **5 seconds**.
- Branch name as secondary text (truncate middle). Ahead/behind shown as `↑2 ↓1` in tooltip and in the pane's sync button, not in the row (reduces noise).
- Warning icon on error states.

### F3. History list (the hero screen)

Opening a repo in a pane shows the **History** tab by default.

- Virtualized list, paged 200 commits at a time with infinite scroll; first page must render in under 100 ms for a 100k-commit repo (use `-n 200`, never full log).
- **Row 0 (conditional):** the **uncommitted pseudo-commit** when `dirty_count > 0` (F5).
- **Marker divider:** a thin labeled line "Reviewed up to here" between new and reviewed commits (only when `new_commit_count > 0`). Commits above it are visually emphasized (blue dot, bold subject); commits below are muted.
- **Range header (sticky, above the list) when new commits exist:** "7 new commits · 12 files · +340 −120" with flag icons aggregated, and a primary button **Mark as reviewed (R)**. Clicking the header (default state) selects the combined range `{M}..HEAD`.
- **Selection:** click = single commit. `Shift+click` or `Shift+j/k` = contiguous range (combined diff `oldest^..newest`). `Mod+click` toggles are NOT supported (keep it simple).
- **Live updates:** new commits appear at the top instantly (slide-in with a brief highlight, see DESIGN.md); selection is preserved; if the user was viewing the combined range, the range grows in place and the header counts update.
- **Search:** `/` focuses an inline search field filtering by message/author (`git log --grep`, `--author`). `Esc` clears.
- **Context menu on a commit:** Copy SHA, Copy as Markdown (subject, body, file list with stats), Copy diff, Revert this commit…, Mark reviewed up to here, Reset repo to here… (only offered for HEAD ancestors; see F9), Open on remote (if origin is GitHub/GitLab, builds URL; no API calls), Reveal in Files tab.
- **Mark reviewed up to here:** sets the marker to that commit (can move backwards too, to re-review).

### F4. Combined range diff & commit-by-commit stepping

- Default view for a repo with new commits is the **combined diff** `{M}..HEAD`, one file list, one scrollable diff.
- Toggle in the range header: **Combined | Step through**. Step mode shows one commit at a time; `j/k` move between commits, diff updates.
- File list shows per-file `+/−`, status icon, and a small marker if the file has a triage flag.
- For combined ranges where a file was added then deleted (net zero), it does not appear (git handles this).

### F5. Uncommitted pseudo-commit & commit box

- Row styled as a dashed hollow circle, title "Uncommitted changes", subtitle "3 files · +24 −6". Shown only when dirty.
- Selecting it shows the working tree file list with **checkboxes** (all checked by default) and the **commit box** pinned at the bottom of the file list column.
- Commit box: single-line **Summary** (soft limit 72 chars, counter turns amber at 73+), collapsible **Description** textarea, button **Commit to `<branch>`**. Disabled when summary is empty or no file is checked.
- If `commit_message_cmd` is configured, show a ✨ **Draft message** icon button: runs the command with the selected diff on stdin (timeout 60 s), fills summary and description, never auto-commits.
- After commit: toast "Committed 7f3a2c1", selection moves to the new commit, marker is **not** moved automatically.
- Untracked files appear with an "Untracked" badge and are included in commit when checked.
- Right-click a file: Discard changes… (F9 safety), Copy path, Reveal, Open in editor, Ignore file (append to `.gitignore`).

### F6. Diff viewer

Single scroll container for all files in scope, **virtualized by line**, with **sticky file headers**.

- **Modes:** Unified (default) and Split (side by side), toggle `s`. Remembered in settings.
- **Line numbers** both sides in unified; click a line number to select it, shift-click to extend. Selected range shows a floating toolbar: **Copy reference** (`path:L10-L24`), **Copy snippet**, **Add note** (F11).
- **Hunks:** header shows `@@` range plus the function context text. Between hunks and at file top/bottom: **expand buttons** ("Show 20 more lines ↑/↓", "Show all N hidden lines") fetching from the blob (`new` side) and rendering as context lines. Keyboard: `e` expands all in current file.
- **Word-level highlight** inside changed line pairs (via `similar`, only for pairs where both lines under 500 chars).
- **Whitespace:** `w` toggles ignore-whitespace (adds `-w`).
- **File header:** path (dirname muted, filename strong), status badge, `+N −N`, collapse chevron, buttons: **View file** (opens in code viewer at that revision), **Copy path**. Renames show `old → new`.
- **Collapse** per file; "Collapse all / Expand all" in the toolbar. Files over the large-diff threshold start collapsed with "Load diff".
- **Navigation:** `]` / `[` jump to next/previous file; `n` / `p` jump to next/previous change hunk.
- **Binary / mode-only changes** render a single descriptive line.
- Highlight syntax per file as it enters the viewport; cache by `blobSha`.
- **Copy as prompt context** (header menu): copies a Markdown block with commit subject(s) and the diff for the current file or whole scope, truncated at 200 KB with a notice. Purpose: pasting into an agent.

### F7. Branch & sync controls (GitHub Desktop style)

- **Branch dropdown** in the pane header: searchable list of local branches, current checked; selecting checks it out. No create/delete/merge in v1 (a "Create branch…" entry is NOT included).
- **One contextual Sync button** to the right, exactly one state at a time:

| Condition | Label | Action |
|---|---|---|
| No remote | (hidden) | |
| No upstream, remote exists | **Publish branch** | `push -u origin <branch>` |
| ahead>0 and behind=0 | **Push origin ↑N** | `push` |
| behind>0 and ahead=0 | **Pull origin ↓N** | `pull --ff-only` |
| ahead>0 and behind>0 | **Diverged ↑N ↓M** (disabled; tooltip: "Resolve in the terminal") | opens terminal on click |
| ahead=0 and behind=0 | **Fetch origin** (shows "Last fetched 3m ago") | `fetch --prune` |

- Auto-fetch every `auto_fetch_minutes` for repos visible in a pane; background (non-visible) repos fetch at 3x the interval. Never auto-pull or auto-push.
- While running: spinner in the button, disabled. Errors: toast with the git stderr first line and "Show details" (dialog with full stderr).

### F8. Terminal (per repo)

- Bottom drawer inside each pane, toggled with `` Mod+` `` (backtick) or the pane header terminal button. Default height 240 px, resizable, remembered.
- **One session per repo**, spawned lazily on first open, cwd = repo root, user's shell (setting). Session persists while the app runs, even if hidden or the repo is not in any pane. Closing the app terminates all sessions.
- Backend keeps a **256 KB ring buffer** of output per session; on re-attach, replay it so scrollback is not lost. Resize events propagate to the PTY.
- Header strip: repo name, cwd, buttons **Clear**, **Restart**, **Hide**.
- If the same repo is shown in two panes, only one pane shows the terminal at a time. The other shows "Terminal is open in pane 2. **Move here**".
- Activity detection feeds F2's terminal glyph: output within 5 s.
- Shortcuts that must keep working while the terminal is focused: `` Mod+` ``, `Mod+1/2/3`, `Mod+B`, `Mod+[`, `Mod+]`. On Windows/Linux, the palette in terminal focus is `Ctrl+Shift+P` (since `Ctrl+K` belongs to shells); outside the terminal both work. Everything else goes to the terminal.
- Copy/paste: `Cmd+C/V` on macOS; `Ctrl+Shift+C/V` on Windows/Linux (and `Ctrl+C` copies when a selection exists).
- Links: Ctrl/Cmd+click on `path:line` patterns opens that file in the code viewer (F10); URLs open externally.

### F9. Safety: undo, revert, reset, discard

Principle: **nothing destructive happens without a backup and a confirmation that states exactly what will be lost.**

- **Revert this commit…** dialog: shows commit summary and files; button "Create revert commit". Disabled with explanation if working tree is dirty ("Commit or discard your changes first").
- **Reset to reviewed marker…** dialog:
  - Lists commits that will be removed (up to 20, "and N more").
  - Shows whether any are already on a remote (via `branch -r --contains`). If yes: red warning "These commits are already pushed. Resetting will require a force push. Prefer reverting." plus a primary alternative button **Revert the range instead** (creates revert commits for `{M}..HEAD`, newest first, `--no-edit`).
  - Disabled if working tree is dirty unless the user ticks "Also discard uncommitted changes (backed up)".
  - Confirm button label includes the count: **Reset and drop 7 commits**.
  - Backup ref created; toast "Reset done. **Undo**" valid for 30 seconds, and always available later via Settings, Recovery (list of backup refs with date, kind, and Restore button).
- **Discard changes…** dialog states file count; creates backup (section 6.2); toast with **Undo**.
- Backup refs older than 30 days are pruned on app start (only `refs/relay/backup/*`).

### F10. Code viewer (read-only)

- **Files tab** in each pane: collapsible file tree at HEAD (from cached `ls-tree`) with filter input; selecting a file opens a read-only viewer to the right.
- Viewer: line numbers, syntax highlight, soft-wrap toggle, find-in-file (`Mod+F`, simple substring, highlight all, next/prev), **revision selector** chip ("HEAD", or a specific commit when opened from a diff), **Blame-lite:** hover a line gutter shows last-change commit via `git blame -L n,n --porcelain` (on demand, cached).
- Opens at a given line (from diff "View file", terminal links, palette).
- Large files (above 2 MB): show first 5,000 lines with notice and "Open in editor".
- Always a button **Open in editor** using `external_editor_cmd` (hidden if unset).
- **Peek definition (heuristic, v1):** clicking an identifier (word of `[A-Za-z_][A-Za-z0-9_]*`) with `Alt/Opt+click` (or `F12`) opens a popover listing candidate definitions. Algorithm:
  1. Build patterns for the identifier `X`: `(fn|function|def|class|struct|enum|trait|interface|type|const|let|var|func|impl|module)\s+X\b`, `X\s*[:=]\s*(async\s*)?(function|\()`, `X\s*\(.*\)\s*\{` (methods).
  2. Run `git grep -n -I -E` on the rev, limited to the same file extension family first, then repo-wide if no hits.
  3. Show up to 8 candidates: `path:line` plus the matched line and 2 lines of context. Click navigates the viewer. `Esc` closes.
  - Mark as heuristic in the popover footer ("Text-based match"). Tree-sitter based symbols are a future upgrade.

### F11. Review notes ("feedback for the agent")

- Select lines in a diff or the viewer, **Add note** opens a small inline editor (text, `Mod+Enter` saves). Note stores file, lines, side, commit sha, snippet.
- Notes show as small markers in the gutter; hover shows the text; click edits/deletes.
- **Notes panel** (pane header button with count badge): slide-over listing the repo's notes grouped by file.
  - **Copy as prompt:** produces Markdown:
    ```
    Please address this review feedback on <repo name> (commit range <short>..<short>):

    1. `src/api/user.ts` L42-L58
       ```ts
       <snippet>
       ```
       <note text>
    ```
  - **Send to terminal:** writes the same text into the repo's PTY using bracketed paste (does **not** press Enter). Opens the terminal if hidden.
  - **Clear all** (with confirm), per-note delete.
- Notes persist in `state.json` per repo and survive restarts. Notes whose file no longer exists are marked "stale" but kept.

### F12. Split panes

- Main area holds **1, 2, or 3 panes** (columns). Controls: `Mod+\` cycles pane count 1, 2, 3 (new pane picks the next repo from the sidebar by activity), or palette "Split pane".
- Assign a repo to a pane by: click a repo (goes to focused pane), drag a repo row onto a pane, `Mod+1/2/3` to focus panes, palette "Open in pane N".
- Each pane remembers its own tab (History/Files), selection, scroll positions, terminal state.
- Resizable dividers; minimum pane width 420 px; if the window cannot fit N panes at minimum width, extra panes collapse to the next lower count automatically.
- **Saved layouts:** palette "Save layout as…" / "Load layout…" stores `{paneCount, repoIds}` by name (for example "frontend + backend").
- Closing a pane: pane header `✕` (hidden when only one pane).

### F13. Command palette

`Mod+K` (`Ctrl+Shift+P` also, required when terminal focused on Win/Linux). Centered overlay. Fuzzy matching. Mode by prefix:

| Prefix | Searches |
|---|---|
| (none) | repos first, then commands |
| `>` | commands only |
| `/` | files in the focused repo (from cached `ls-tree`; plus untracked from status) |
| `#` | commits in the focused repo (message search, shows 20) |
| `@` | review notes |

Commands include: Add repository, Mark reviewed, Switch tab, Toggle terminal, Toggle sidebar, Toggle diff mode, Toggle whitespace, Fetch/Pull/Push, Open in editor, Reveal in file manager, Copy repo path, Save/Load layout, Settings, Recovery, Reload repo state. Each shows its shortcut. `Enter` runs. `Mod+Enter` on a repo opens it in a **new pane**.

### F14. Triage flags

Defined in section 8. Shown as icon chips on commit rows, range header, and file rows.

### F15. Settings

Page (modal) with sections: General (theme, UI scale, sort order), Git (auto-fetch interval, context lines, ignore whitespace default), Terminal (shell, font size), Integrations (external editor command, commit message command; each with a "Test" button), Recovery (backup refs list), About (versions, `git --version`, data folder path with Reveal button).

---

## 8. Triage flag rules (exact heuristics)

Computed in Rust (`triage.rs`) from the commit's numstat (`FileChange` list) plus a cheap diff scan for patterns. Pure functions, unit-tested. Computed lazily when a commit row first becomes visible, cached by SHA.

| Flag | Severity | Rule | Detail text |
|---|---|---|---|
| `Large` | Caution | files_changed > 15 **or** (additions + deletions) > 500 | "23 files, 812 lines" |
| `DeletionHeavy` | Caution | deletions >= 50 **and** deletions > 2 x additions | "−420 vs +60" |
| `Dependencies` | Info | any path matches: `package.json`, `package-lock.json`, `yarn.lock`, `pnpm-lock.yaml`, `bun.lockb`, `Cargo.toml`, `Cargo.lock`, `go.mod`, `go.sum`, `requirements*.txt`, `pyproject.toml`, `poetry.lock`, `Pipfile*`, `Gemfile*`, `composer.*`, `pom.xml`, `build.gradle*` | "Cargo.lock, Cargo.toml" |
| `Config` | Caution | path matches: `.github/workflows/**`, `Dockerfile*`, `docker-compose*`, `.env*`, `**/migrations/**`, `*.tf`, `*.tfvars`, `.gitlab-ci.yml`, `k8s/**`, `helm/**`, `nginx*.conf` | "Dockerfile, .github/workflows/ci.yml" |
| `TestsRemoved` | Warn | a test file (`*test*`, `*spec*`, `__tests__/`, `tests/`) was **deleted**, **or** a test file has deletions > additions with deletions >= 10, **or** added lines match `\b(it|test|describe)\.(skip|only)\b`, `\bx(it|describe)\(`, `@Ignore`, `#\[ignore\]`, `pytest\.mark\.skip`, `t\.Skip\(` | "2 test files deleted" |
| `Secrets` | Warn | added lines match: `-----BEGIN [A-Z ]*PRIVATE KEY-----`, `AKIA[0-9A-Z]{16}`, `ghp_[A-Za-z0-9]{36}`, `sk-[A-Za-z0-9]{32,}`, `(api[_-]?key|secret|token|passwd|password)\s*[:=]\s*['"][^'"]{8,}['"]`, or a file named `.env` (not `.env.example`) was added | "Possible secret in config.ts" |
| `BigFile` | Caution | added/changed blob larger than 1 MB (via `cat-file -s`) | "assets/video.mp4 (4.2 MB)" |
| `Binary` | Info | any binary file added/changed | "3 binary files" |

Ranges aggregate flags across their commits (union, with counts). Flags are advisory only. Never block an action. Tooltip lists `kind: detail`. Settings has no per-flag toggles in v1.

---

## 9. Watching & polling

Goal: sidebar and history are live without burning CPU, even with 6 repos.

1. **Commit/ref watcher (per registered repo, always on):** `notify` watching (non-recursive where possible) `.git/HEAD`, `.git/refs/` (recursive), `.git/logs/HEAD`, `.git/index`, `.git/packed-refs`. Debounce 250 ms. On event: refresh `RepoStatus` and, if HEAD moved, emit `repo://head-changed` with the new SHA. For worktrees or `.git` files (gitdir pointer), resolve the real git dir first.
2. **Status poller:** `git status` per repo to refresh `dirty_count`:
   - Visible repo: every **4 s** while the window is focused, and immediately on window focus, and after any Relay write op.
   - Non-visible repos: every **20 s** while focused; paused when the window is unfocused (resume on focus).
   - Skip a cycle if the previous status call for that repo is still running. If a status call exceeds 5 s, double that repo's interval (max 60 s) and show "Large repository: status updates slowed" in the repo row tooltip.
3. **Activity:** `last_activity_at` = max(HEAD reflog mtime, newest `dirty` file mtime from the last status). No extra watching required.
4. **(v1.1) Worktree watcher:** walk with the `ignore` crate honoring `.gitignore` and register non-recursive watches on non-ignored directories only (never `node_modules`, `target`, `.git`). Replaces polling for the visible repo. Do not attempt in v1.
5. Events (Rust to UI): `repo://status` (RepoStatus), `repo://head-changed`, `term://output` (`{repoId, data}`), `term://exit`, `app://toast` (backend-initiated messages).

---

## 10. IPC commands (Tauri `invoke`)

All return `Result<T, AppError>` where `AppError { code, message, stderr? }`. Codes: `GitMissing`, `NotARepo`, `PathMissing`, `Timeout`, `AuthFailed`, `Dirty`, `Conflict`, `NotFound`, `TooLarge`, `Io`, `Other`.

```
// repos
repo_add(path, unreviewed_last_n?) -> RepoEntry
repo_scan(path, depth) -> Vec<String>
repo_remove(id) ; repo_update(id, {name?, pinned?, order?}) ; repo_relocate(id, new_path)
repo_list() -> Vec<RepoEntry>
repo_status(id) -> RepoStatus ; repo_refresh_all()
// marker
marker_set(id, sha) ; marker_set_to_head(id)
// history & diff
log_page(id, skip, limit, query?) -> Vec<CommitSummary>
commit_flags(id, shas[]) -> Map<sha, Vec<TriageFlag>>
scope_files(id, scope: DiffScope, ignore_ws) -> Vec<FileChange>
scope_file_diff(id, scope, path, ctx_lines, ignore_ws, force) -> DiffFile
blob_lines(id, rev, path, start, end) -> Vec<String>    // for expand context & viewer
blob_text(id, rev, path) -> {text, tooLarge, binary}
tree_list(id, rev) -> Vec<String>
blame_line(id, rev, path, line) -> {sha, author, date, subject}
grep_defs(id, rev, ident, ext_hint) -> Vec<DefHit>
// writes
commit(id, paths[], summary, body?) -> CommitSummary
discard(id, paths[]) -> BackupInfo
revert(id, sha) -> CommitSummary ; revert_range(id, from, to) -> Vec<CommitSummary>
reset_to(id, sha, also_discard_dirty) -> BackupInfo
backup_list(id) -> Vec<BackupInfo> ; backup_restore(id, ref)
branch_list(id) -> Vec<BranchInfo> ; checkout(id, branch)
sync_fetch(id) ; sync_pull(id) ; sync_push(id, publish)
remote_url(id) -> Option<String>
// terminal
term_open(id, cols, rows) -> {replay: String} ; term_input(id, data) ; term_resize(id, cols, rows)
term_restart(id) ; term_kill(id) ; term_paste(id, text)   // bracketed paste
// misc
settings_get() / settings_set(Settings) ; layout_get() / layout_set(Layout)
open_external_editor(id, path, line?) ; reveal_in_file_manager(path)
run_commit_message_cmd(id, paths[]) -> {summary, body}
env_check() -> {git_version, shell}
```

Frontend must **never** run git directly; everything goes through these commands.

---

## 11. Performance & resource budgets (acceptance requirements)

| Metric | Budget |
|---|---|
| Release binary size | under 15 MB (Windows/Linux), under 20 MB (macOS universal not required) |
| Cold start to interactive window | under 600 ms on a mid-range laptop (SSD) |
| Idle RAM, 6 repos registered, 2 terminals open | under 180 MB total (webview included) |
| Idle CPU (window focused, nothing changing) | under 2% average; 0% when window unfocused |
| History first page (200 commits), 100k-commit repo | under 150 ms to paint |
| Open combined diff with 500 changed files | file list under 300 ms; first visible file's diff under 200 ms after |
| Scrolling a 10k-line diff | 60 fps (virtualized; DOM nodes capped around 400) |
| Switching repo in a pane | under 100 ms to show cached state, then refresh |
| Terminal keystroke echo | no perceptible lag (under 30 ms added by app) |

If a budget is missed, fix it before moving to the next milestone (profile first).

---

## 12. Error handling & edge cases

- **`git` not found:** full-window first-run screen: "Relay needs Git. Install it from git-scm.com, then restart." with a Retry button (re-runs `env_check`).
- **Repo path missing / permissions:** row warning state (F1). No crash.
- **Detached HEAD:** branch shown as `HEAD detached at 7f3a2c1`; sync button hidden; marker logic still works.
- **Empty repo (no commits):** History shows "No commits yet." Uncommitted pseudo-commit still works; first commit allowed. Marker is `None`.
- **Marker missing (gc'd) or diverged (rebase/reset happened outside Relay):** `marker_state` set. Range header shows a yellow notice: "History was rewritten since you last reviewed." with buttons **Compare from merge-base** (uses `git merge-base {M} HEAD` if the marker still exists, else shows last 20 commits) and **Reset marker to HEAD**.
- **Shallow clones / unborn branches / bare repos:** bare repos rejected on add ("Bare repositories aren't supported").
- **Merge commits in log:** shown as normal rows with a small "merge" chip. Their diff uses first-parent (`-m --first-parent` for `git show`). Never crash on multiple parents.
- **Rebase/merge in progress** (`.git/rebase-merge`, `MERGE_HEAD`): banner in the pane "Operation in progress. Finish it in the terminal." and write actions disabled.
- **Concurrent agent operations:** if a write fails with `index.lock` present, retry up to 3 times with 500 ms backoff, then show "Another Git process is running."
- **Very long lines / huge files:** cap line rendering at 2,000 chars with "… show full line" toggle.
- **Unicode paths / spaces / CRLF:** all supported (use `-z` parsing). Preserve and display `\r\n` correctly, no extra blank lines.
- **Submodules:** show as a single "Submodule updated" line (`Subproject commit` diff), no recursion.
- **Case-insensitive file systems / path normalization on Windows:** canonicalize paths for duplicate detection with `dunce` crate (avoid `\\?\` prefix leaking to UI).
- **Window close with running terminal processes:** no prompt in v1; processes are killed.
- **Clock / time display:** relative ("3m ago", "yesterday", then `MMM d`), tooltip shows the full local timestamp.

---

## 13. Keyboard shortcuts (central registry in `keys.ts`)

`Mod` = Cmd (macOS) / Ctrl (Win/Linux). Single-letter shortcuts only fire when focus is **not** in an input, textarea, or the terminal.

| Shortcut | Action |
|---|---|
| `Mod+K` (also `Ctrl+Shift+P`) | Command palette |
| `Mod+O` | Add repository |
| `Mod+1` / `2` / `3` | Focus pane N |
| `Mod+\` | Cycle pane count 1, 2, 3 |
| `Mod+B` | Toggle sidebar (full / collapsed icons) |
| `` Mod+` `` | Toggle terminal drawer in focused pane |
| `Mod+[` / `Mod+]` | Previous / next repo in the focused pane (sidebar order) |
| `Mod+,` | Settings |
| `Mod+Shift+R` | Refresh all repos |
| `j` / `k` | Next / previous item in focused list (commits, files) |
| `Shift+j` / `Shift+k` | Extend commit selection |
| `{` / `}` | Previous / next **flagged** commit |
| `Enter` | Move focus from commit list to file list to diff |
| `Esc` | Back one level / close popover, palette, dialog |
| `r` | **Mark reviewed** (marker to HEAD; with a range selected, to the newest commit in selection) |
| `]` / `[` | Next / previous file in diff |
| `n` / `p` | Next / previous hunk |
| `e` | Expand all context in current file |
| `w` | Toggle ignore whitespace |
| `s` | Toggle unified / split diff |
| `c` | Copy SHA of selected commit |
| `/` | Search history (or filter file list when files focused) |
| `Alt+Click` / `F12` | Peek definition |
| `Mod+F` | Find in file (viewer) |
| `Mod+Enter` | Commit (when commit box focused) / save note |
| `Mod+Shift+F` | Fetch/Pull/Push (runs the current sync button action) |

Show shortcuts in tooltips and in the palette. Provide a "Keyboard shortcuts" help overlay (`?`).

---

## 14. Milestones & acceptance criteria

Build in order. Each milestone ends with: all tests pass, `cargo clippy -- -D warnings` clean, `cargo fmt`, the app runs, and a git commit with a clear message. Do not start the next milestone with failing checks.

### M0. Scaffold
- Tauri v2 + Svelte 5 + TS + Vite project, folder structure per 4.1, lint/format configs, CI-less local scripts (`npm run dev`, `npm run build`, `cargo test`).
- Theme tokens loaded (DESIGN.md section 2), light/dark following system, empty app shell with sidebar and one empty pane.
- **Accept:** `npm run tauri dev` opens a themed window; `env_check` shows git version.

### M1. Repos, sidebar, status
- F1 (add/remove/rename/pin/reorder/missing handling), F2 (badges except activity pulse and terminal glyph), persistence, `GitRunner`, status parsing, `repo_status`, poller (section 9, items 2 and 3).
- **Accept:** add 3 real repos; badges for dirty/ahead/behind visible within 5 s of changing a file or committing from the command line; state survives restart; missing folder handled gracefully. Unit tests for porcelain-v2 parser.

### M2. History & diff viewer
- F3 (without marker/range/live), F6 (unified mode, expand context, word diff, collapse, file navigation, whitespace toggle, large-diff gating, lazy per-file loading), file list, log paging, syntax highlight.
- **Accept:** browse history of a 50k-commit repo smoothly; open a commit with 300 files without freezing; expanded context lines match file contents; budgets in section 11 for these items met. Unit tests: log parser (with weird characters), diff parser (renames, binary, no-newline, CRLF, mode change), numstat `-z` parser.

### M3. Reviewed marker, ranges, live updates
- Marker storage, `+N` badge, marker divider, range header, combined vs step-through, `R` shortcut, marker states (missing/diverged), ref watcher (section 9, item 1), live insertion animation, activity pulse.
- **Accept:** make 3 commits from the terminal in a registered repo: badge goes 0 to 3 within 1 s, history updates live, combined diff matches `git diff <marker> HEAD`, pressing `R` clears the badge. Rebase the repo externally: diverged notice appears, both options work.

### M4. Uncommitted & sync
- F5 (pseudo-commit, checkboxes, commit box, untracked, hooks respected), F7 (branch dropdown, sync button states, auto-fetch), F9's discard with backup.
- **Accept:** commit selected subset of files; failing pre-commit hook shows its output in the error dialog; push/pull/fetch work against a real remote; discard can be undone via toast and via Recovery.

### M5. Terminal
- F8 complete including ring buffer replay, activity glyph, multi-pane ownership message, shell detection.
- **Accept:** run a long command, switch repos and back, output intact; resize works; works on the user's OS; 2 terminals idle within the RAM budget.

### M6. Split panes & palette
- F12, F13, shortcuts from section 13 (except those for later features), saved layouts, sidebar collapsed mode, responsive pane layouts (DESIGN.md section 5.2).
- **Accept:** 3 panes with 3 different repos; layout restored after restart; palette finds repos, files, commits; all listed shortcuts work, including with terminal focused.

### M7. Triage flags & safety
- F14 and section 8 rules with unit tests per rule (fixtures); F9 (revert, reset with remote warning, revert-range alternative, Recovery list, backup pruning).
- **Accept:** each flag triggers on a crafted test repo; `{`/`}` jump between flagged commits; reset to marker followed by Undo restores the exact previous HEAD; reset on pushed commits shows warning.

### M8. Code viewer & peek
- F10 (Files tab, tree filter, viewer, find, revision chip, blame-lite, open in editor, peek definition heuristic).
- **Accept:** from a diff, "View file" opens at the correct line; Alt+Click on a function call shows its definition in a Rust, a TypeScript, and a Python repo; viewer opens a 1 MB file in under 300 ms.

### M9. Review notes
- F11 complete with notes panel, copy as prompt, send to terminal (bracketed paste, no Enter).
- **Accept:** add 3 notes across 2 files; copy-as-prompt output matches the format in F11; send to terminal pastes it into the running shell without executing it; notes persist after restart.

### M10. Polish & packaging
- Settings page (F15), shortcut help overlay, empty states and microcopy from DESIGN.md, accessibility pass (focus rings, labels, contrast AA), reduced-motion support, performance pass against section 11, app icon, installers (`tauri build`) for the user's OS, README with build instructions.
- **Accept:** all budgets in section 11 met and recorded in `docs/perf.md`; manual QA checklist (section 15) passes.

---

## 15. Testing strategy

- **Rust unit tests:** parsers (status v2, log, numstat -z, diff incl. edge cases in M2), triage rules, marker state logic, sync-button state selection, backup ref naming/pruning.
- **Rust integration tests:** helper `TempRepo` builds repos with scripted commits (renames, binary, merges, CRLF, unicode paths, empty repo, detached HEAD, rewritten history). Tests run the real `GitRunner` against them. Remote ops tested against a local bare repo.
- **Frontend tests (vitest):** fuzzy scorer, relative time formatting, keys registry (no duplicate bindings), notes-to-prompt formatter, diff line virtualization math.
- **Manual QA checklist** (kept in `docs/qa.md`, run at M10): the core loop (section 1.3) end to end with 6 real repos; kill the network during pull; delete a repo folder while open; agent committing every 2 seconds during a review; 3 panes at the minimum window size; high-DPI scaling; light/dark switch while open.

## 16. Quality bar & working rules for the AI builder

1. Follow this spec literally. If something is ambiguous, choose the simplest behavior consistent with the design principles and note it in `docs/decisions.md`. Do not silently add features.
2. Keep dependencies minimal. Before adding any crate or npm package, check whether the standard library or a few lines of code suffice. Record each addition and reason in `docs/decisions.md`.
3. Keep Tauri command handlers thin; logic lives in testable modules.
4. No `unwrap()`/`expect()` in non-test Rust code paths reachable from user actions. Return `AppError`.
5. No telemetry, no network calls except `git` itself and opening URLs the user clicks.
6. Frontend: no global CSS frameworks; use tokens from DESIGN.md; components small and typed; all list views virtualized.
7. Accessibility is not optional: every interactive element keyboard reachable with a visible focus ring and an accessible name.
8. Commit after each milestone (and after logical sub-steps) with descriptive messages.

## 17. Open decisions (defaults chosen; user can override later)

| Topic | Default |
|---|---|
| App name | Relay (placeholder) |
| Operating system | Cross-platform; confirm user's OS for first packaging |
| Marker scope | Per repo (not per branch) |
| Who pushes | User pushes manually; no auto-push/pull |
| Commit-message drafting | Optional external command; hidden until configured |
| Peek definition | Heuristic `git grep` in v1, tree-sitter later |
| Image diff preview, worktree watcher, multiple terminals per repo, pane layouts beyond columns | v1.1 |

## 18. Kickoff prompt (give this to the AI agent)

> You are building "Relay", a lightweight Tauri v2 + Rust + Svelte 5 desktop Git client for reviewing AI-written code across multiple repos. Read `PRODUCT_SPEC.md` and `DESIGN.md` completely before writing code. Work strictly milestone by milestone (section 14), starting at M0. After each milestone: run tests, clippy, fmt, verify the acceptance criteria, update `docs/decisions.md`, and commit. Do not implement features outside the spec. Keep the app lightweight (budgets in section 11 are requirements). When finished with a milestone, summarize what you did, what you verified, and any deviations, then continue to the next.
