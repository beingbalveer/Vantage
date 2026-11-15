<p align="center">
  <img src="assets/vantage-logo-horizontal.png" alt="Vantage Logo" width="440" />
</p>

<h3 align="center">Your repositories. Every branch. One dashboard.</h3>

<p align="center">
  A fast, local, native GPUI dashboard for multi-repo Git work — branches, pending changes and working-tree state at a glance.
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square" alt="License: MIT" /></a>
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Linux-lightgrey?style=flat-square" alt="Platform Support" />
  <img src="https://img.shields.io/badge/telemetry-none-brightgreen?style=flat-square" alt="No telemetry" />
  <img src="https://img.shields.io/badge/built%20with-Rust%20%2B%20GPUI-B7410E?style=flat-square" alt="Built with Rust and GPUI" />
</p>

---

## Why this exists

I work across several repositories at once, and much of that work is written and committed by AI agents.

The hard part is not writing more code. It is knowing, without opening a dozen windows, which repository is on which branch, what is uncommitted, what is ahead or behind, and which working tree needs attention before the next agent run.

Vantage is the dashboard for that question. It reads the real state of every repository in your workspace, shows the things that need a decision, and gives you the few Git actions that belong in a review loop: stage or unstage a file, commit what you staged, check out a branch, fetch, pull, push. When the work needs an editor or a long commit message, it hands you back to the tools you already use.

No account. No sync. No telemetry.

---

## What it does

### Multi-repo overview

The **All Repositories** view lists every repository in the workspace with its current branch, working-tree status, remote sync and last activity. Each row ends in a **fetch** action that reloads just that repository; while it runs the icon spins, and it stops as soon as git returns. A row with no origin shows the action dimmed. Repositories are grouped by your own groups, and each group header collapses so a large workspace stays readable.

Click any row to open its inspector.

### Needs Attention

The **Needs Attention** view is the same inventory narrowed to repositories that actually need a look: merge conflicts, commits behind origin, or uncommitted changes. The sidebar count tells you how many there are before you open the view.

### Pending Changes

The **Pending Changes** view collects uncommitted work into one card per repository, with the current branch and total additions and deletions. Each file shows its status code, whether it is staged, and its line counts. Open the inspector to act on it.

### Repository inspector

The repository detail view is a resizable right-hand pane. Opening a repository splits the content area roughly 50/50; the top-right panel icon shows or hides it without losing the current selection. It has three tabs:

- **Changes** — every changed file with its status, staged state and `+` / `-` counts. A header checkbox stages or unstages the whole list in one git call; click a file's path to expand a real diff (line numbers, hunks, context, tinted additions and deletions). A commit bar stays pinned to the bottom of the pane: the GitHub avatar of the account the commit will be attributed to (hover it for the name and email), a message field, and a button that commits exactly the ticked files with your own identity, hooks and signing configuration. When the branch has commits the remote does not, an **Undo** strip appears above it — `Committed just now` for one you just made, otherwise `N commit(s) not pushed` — naming the tip commit. Undo soft-resets that commit, so its files come straight back into **Changes** still staged, and it is refused once the commit is on the remote.
- **Branches** — local branches with ahead/behind counts, their current/default state and latest commit. Check out any branch that is not already current.
- **Commits** — recent commits with SHA, author and relative time. Anything on this branch the remote does not have yet is listed under a `N commits not pushed` header and carries a round up-arrow marker on the right, so a local-only commit is never mistaken for a synced one.

The detail pane header shows the repository name, its status and the local path (click to copy). Under it sits one band with the current branch and the git actions for the repository in focus: **Pull**, **Push** and **Fetch Origin** — the Pull and Push buttons carry the number of commits they would move (remote commits waiting to come in, local commits waiting to go out). An action that cannot run is dimmed, and the one that is running shows a spinner and **Pulling…** / **Pushing…** / **Fetching…** until git returns. Moving a repository between groups lives at the right end of the tab row.

### Repository management

**Add Repository** opens a focused modal: type or browse to a local path, confirm the detected repository, and choose an existing group or create a new one. Invalid paths and non-repositories fail in place without adding anything.

Groups are managed from the sidebar: create a group, move repositories into it, or remove it. Group membership is persisted in Vantage's own state file.

### Live working trees

Vantage watches each repository and re-reads only the working trees that changed, so the dashboard stays current while agents commit and edit around you. Whenever Vantage becomes active again, it also refreshes the whole workspace silently in the background, so switching back to the app shows the latest status without a scanning state. **Refresh Status** forces a visible full rescan. Scan failures are shown as a banner instead of blanking the dashboard.

### Settings

**Settings** opens from the Vantage menu, `Cmd+,`, or the **Settings** entry in the sidebar footer. It keeps only preferences that affect the current dashboard:

- **General** — default view, repository sort, window memory.
- **Appearance** — System / Light / Dark, with System following macOS live.
- **Git** — auto-fetch interval, fetch on launch, default branch for empty repos.
- **Data** — state-file location, reveal in Finder, reset preferences.
- **About** — version, build, Git, system and architecture diagnostics, plus copyable diagnostics and links.

Sidebar state is remembered automatically. Preferences are stored separately from the legacy v1 settings, which remain untouched.

### Light and dark

The whole interface has first-class light and dark themes. The theme setting lives in **Settings → Appearance** and supports System, Light and Dark.

---

## What it deliberately leaves out

| Vantage is | Vantage is not |
|---|---|
| A multi-repo branch, status and changes dashboard | An IDE or a code editor |
| A place to stage, commit, check out branches and sync | A full Git client (history rewrite, PR review) |
| A local-first, zero-telemetry desktop app | A cloud service or account system |
| Built on your system Git and existing credentials | A GitHub PR, issue or CI client |

There is no file editor, no embedded terminal, no command palette, no fuzzy file finder, no AI chat and no plugin marketplace. Vantage commits what you staged with a summary line; longer messages, amends and rebases stay in the tools built for them, so the cross-repository picture stays clear.

---

## Safety and privacy

- **System Git, invoked directly.** Vantage invokes the `git` on your `PATH` with explicit arguments, so your existing SSH keys, credential helpers, hooks and signing configuration apply. It does not go through a shell.
- **A small write surface.** Vantage does not edit your files. Its writes are limited to staging and unstaging paths, committing the paths you staged, checking out an existing branch, fetching, pulling and pushing. A commit runs your own hooks, identity and signing configuration — the commit bar names that identity (`user.name` / `user.email`, repository config overriding global) before you press Enter, and Vantage never amends, rebases or rewrites history for you. Pull never merges or rebases unless you pick that strategy in **Settings → Git**: the default fast-forward only reports a diverged branch instead of rewriting your history.
- **Your state stays local.** Repository inventory, groups and assignments live in `state.json` in the platform data directory. No analytics endpoint, no account, no sync.
- **One optional network call.** The commit bar shows the GitHub profile photo of the identity it will commit as. To do that, Vantage asks `github.com/<login>.png` once per account, through the system `curl`, and caches the image beside `state.json` for a week. There is no token, no other endpoint and no telemetry; when no login can be proven from your git identity, `github.user` or the `gh` CLI, the photo is skipped and initials are drawn locally instead.
- **Failures are visible.** A repository that cannot be read is reported by name; the rest of the dashboard keeps working.

---

## Installation

Vantage v2 is **not published yet**. There is no v2 DMG, installer or Homebrew release. The existing cask and older release artifacts predate this dashboard and do not contain it.

Until a v2 build is published, run it from source:

```bash
cargo run -p vantage
```

The build instructions are below.

---

## Building from source

Requires a stable Rust toolchain ([rustup](https://rustup.rs/)) and the system `git` CLI on `PATH`.

```bash
cargo build -p vantage   # build the app
cargo run -p vantage     # build and launch it
```

The debug binary is written to `target/debug/vantage`. For an optimized build:

```bash
cargo build --release -p vantage
```

The release binary is written to `target/release/vantage`.

The workspace has two crates:

- **`crates/vantage-core`** — the engine: a runner around the system `git` CLI, repository status, log, refs, diffs, a file watcher and state persistence.
- **`crates/vantage`** — the native GPUI desktop app.

Interested in contributing? Check out the [Contributing Guide](CONTRIBUTING.md).

---

## Interactions

Vantage is mouse-first, with a native macOS menu bar and a small set of
standard shortcuts.

| Action | How |
|---|---|
| Open the repository detail pane | Click a repository row |
| Show or hide the right pane | Click the panel icon at the top right |
| Resize the right pane | Drag the divider between the overview and detail pane |
| Expand a file diff | Click the changed file's path |
| Stage or unstage a file | Click its checkbox in **Changes** |
| Commit staged changes | Type a message in the bottom bar of **Changes** and press `Enter`, or `Cmd+Enter` |
| Undo the last commit | Click **Undo** in the strip above the commit bar (only while that commit is unpushed) |
| Check out a branch | Click **Checkout** in **Branches** |
| Fetch a repository | Click the row's refresh action, **Fetch Origin** in the detail pane, or `Cmd+Shift+F` |
| Pull or push a repository | **Pull** / **Push** in the detail pane bar, or `Cmd+Shift+P` / `Cmd+P` |
| Add a repository | Click **Add Repository**, or `Cmd+O` |
| Refresh status | Click **Refresh Status**, or `Cmd+R` |
| Switch views | `Cmd+1` All Repositories, `Cmd+2` Needs Attention, `Cmd+3` Pending Changes |
| Open Settings | `Cmd+,`, Vantage menu, or the sidebar **Settings** entry |
| Toggle the sidebar | Click the header button, or `Cmd+B` |
| Toggle light/dark | Open **Settings → Appearance**, or `Cmd+Shift+L` |
| Full screen / minimize / close | `Ctrl+Cmd+F` / `Cmd+M` / `Cmd+W` |
| Close the inspector or modal | Click outside it, or press `Esc` in a focused field |
| Submit a group or add-repository field | Press `Enter` in the field |

The menu bar provides the same actions under **Vantage**, **File**, **View**,
**Window** and **Help**, including About, Services, Hide, Quit and the standard
window commands.

---

## Under the hood

- **Native, not a webview.** The UI is built with [GPUI](https://gpui.rs/), the Rust UI framework behind Zed. There is no bundled Chromium and no IPC layer between the interface and the Git engine.
- **One shared state file.** The dashboard reads and writes through `vantage-core`, which owns the workspace `state.json`, so the inventory and group assignments have a single source of truth.
- **Incremental by default.** A `notify` watcher reports which working trees changed; only those repositories are re-read.
- **Documented layout.** See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the workspace map and data flow.
- **Tested and gated.** `cargo fmt --all -- --check`, `cargo clippy`, and `cargo test --workspace` run on macOS and Linux in CI, alongside a macOS build of the app.

---

## Acknowledgments & Trademarks

- Built with [Rust](https://www.rust-lang.org/) and [GPUI](https://gpui.rs/).
- Git is a registered trademark of Software Freedom Conservancy.
- GitHub Desktop is a trademark of GitHub, Inc. Vantage is an independent open-source project and is not affiliated with or endorsed by GitHub, Inc.

---

## License

[MIT](LICENSE) © 2026 Balveer Dhanoriya
