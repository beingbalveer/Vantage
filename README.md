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

Vantage is the dashboard for that question. It reads the real state of every repository in your workspace, shows the things that need a decision, and keeps the review-loop actions in reach: stage or unstage files, commit, check out branches, fetch, pull and push. It also exposes the recovery operations you need when a run goes wrong — discard, stash, merge, revert, reset and amend — with confirmations and backups for the destructive ones. For a full editor or history work beyond those single-step operations, it hands you back to the tools you already use.

No account. No sync. No telemetry.

---

## What it does

### Multi-repo overview

The **All Repositories** view lists every repository in the workspace with its current branch, working-tree status, ahead/behind badges and last activity. The summary strip above the list counts all repositories, repositories with uncommitted changes, repositories with unpushed commits and clean repositories; each count is also a filter. Repositories are grouped by your own groups, and each group header collapses so a large workspace stays readable.

Click any row to open its inspector. Fetching is per repository, not per row: use **Fetch Origin** in the inspector or `Cmd+Shift+F` for the selected repository.

### Needs Attention

The **Needs Attention** view is the same inventory narrowed to repositories that actually need a look: merge conflicts, commits behind origin, or uncommitted changes. It is reachable from the View menu or `Cmd+2`.

### Pending Changes

The **Pending Changes** view collects uncommitted work into one card per repository, with the current branch and total additions and deletions. Each file shows its status code, whether it is staged, and its line counts. Open the inspector to act on it.

### Repository inspector

The repository detail view is a resizable right-hand pane. Opening a repository gives the inspector about two-thirds of the content area by default; the top-right panel icon shows or hides it without losing the current selection. It has three tabs:

- **Changes** — every changed file with its status, staged state and `+` / `-` counts. A header checkbox stages or unstages the whole list in one git call; click a file's path to expand a real diff (line numbers, hunks, context, tinted additions and deletions). A commit bar stays pinned to the bottom of the pane: the GitHub avatar Vantage resolved for the commit identity (hover it for the name and email), a message field, an optional description field, and a button that commits the ticked files with your own identity. Normal commits run Git's `pre-commit`/`commit-msg` hooks and your signing configuration; the initial commit is the exception (see **Current limitations**). When the branch has unpushed commits, an **Undo** strip appears above the bar — `Committed just now` for one you just made, otherwise `N commit(s) not pushed` — naming the tip commit. Undo soft-resets the tip, so its files come straight back into **Changes** still staged, and it is refused once the commit is on the remote.
- **Branches** — local branches with their current/default state and latest activity. A branch that is not current can be checked out, merged into the current branch, compared with it, or used as the start point for a new branch from its right-click menu; the current branch can be renamed there, and non-default branches deleted. Ahead/behind counts are shown per repository, not per branch.
- **Commits** — recent commits with SHA, author and relative time. A `N commits not pushed` header and a round up-arrow mark commits Vantage identifies as local-only (the boundary is positional, so it can be approximate on a diverged branch). Right-click a commit to revert it, reset the branch to it, or — for the tip only — amend it.

The detail pane header shows the repository name, its status and the local path (click to copy). Under it sits one band with the current branch and the git actions for the repository in focus: **Pull**, **Push** and **Fetch Origin** — the Pull and Push buttons carry the number of commits they would move (remote commits waiting to come in, local commits waiting to go out). An action that cannot run is dimmed, and the one that is running shows a spinner and **Pulling…** / **Pushing…** / **Fetching…** until git returns. Moving a repository between groups lives at the right end of the tab row.

### Repository management

**Add Repository** opens a focused modal: type or browse to a local path, confirm the detected repository, and choose an existing group or create a new one. Invalid paths and non-repositories fail in place without adding anything.

**New Repository** (`Cmd+N`) runs `git init` in a new folder with the default branch from Settings and optionally writes a starter `.gitignore`. **Clone Repository** (`Cmd+Shift+O`) clones a URL into a chosen parent folder and adds the result to the workspace.

Groups are managed from the sidebar: create a group, move repositories into it, or remove it. Group membership is persisted in Vantage's own state file.

### Live working trees

Vantage watches each repository and re-reads only the working trees that changed, so the dashboard stays current while agents edit around you. The watcher deliberately ignores `.git`, so commits, fetches and checkouts made in a terminal may not appear while Vantage stays focused. Vantage refreshes the whole workspace silently whenever it becomes active again, so switching away and back shows the latest status without a scanning state; **Refresh Status** (`Cmd+R`) forces a visible full rescan. Scan failures are shown as a banner instead of blanking the dashboard.

### Settings

**Settings** opens from the Vantage menu, `Cmd+,`, or the **Settings** entry in the sidebar footer. It keeps only preferences that affect the current dashboard:

- **General** — default view, repository sort, window memory.
- **Appearance** — System / Light / Dark, with System following macOS live.
- **Git** — pull strategy, auto-fetch interval, fetch on launch, default branch for empty repos.
- **Data** — state-file location, reveal the state file, reset preferences.
- **About** — version, build, Git, system and architecture diagnostics, plus copyable diagnostics and links.

Sidebar state is remembered automatically. Preferences are stored separately from the legacy v1 settings, which remain untouched.

### Light and dark

The whole interface has first-class light and dark themes. The theme setting lives in **Settings → Appearance** and supports System, Light and Dark.

---

## What it deliberately leaves out

| Vantage is | Vantage is not |
|---|---|
| A multi-repo branch, status and changes dashboard | An IDE or a code editor |
| A place to stage, commit, check out branches, sync and recover with discard, stash, merge, undo and amend | A full Git client (interactive rebase, cherry-pick, PR review) |
| A local-first, zero-telemetry desktop app | A cloud service or account system |
| Built on your system Git and existing credentials | A GitHub PR, issue or CI client |

There is no file editor, no embedded terminal, no command palette, no fuzzy file finder, no AI chat and no plugin marketplace. History work is deliberately single-step: amend the tip, undo an unpushed commit, revert a commit, or reset the branch to a commit. There is no interactive rebase, cherry-pick UI or conflict-resolution editor; use your usual Git tools for those.

---

## Safety and privacy

- **System Git, invoked directly.** Vantage invokes the `git` on your `PATH` with explicit arguments, so your existing SSH keys, credential helpers, hooks and signing configuration apply. It does not go through a shell.
- **A real write surface, kept visible.** Vantage is not read-only, and it does not keep your working tree untouched. It writes through Git when you stage or unstage paths, commit the files you tick, amend the tip commit, undo the newest unpushed commit (a soft reset that keeps its files staged), revert a commit, reset the branch to a commit (a hard reset that requires a clean tree), merge a branch or the remote default branch, check out, create, rename or delete a branch, discard individual files or every uncommitted change, stash, restore or drop stashes, pull, push or fetch, or clone a repository. It also writes to disk directly when you add an untracked path to `.gitignore` or create a new repository with a starter `.gitignore`. Discard-all, reset, merge and branch deletion ask before running. Discard, undo, amend and reset record a backup ref under `refs/vantage/backup/` first, and discard moves untracked files into `.git/vantage-trash/` instead of deleting them. There is no in-app browser for those backups yet — use `git` to inspect and restore them.
- **Hooks and signing.** A normal commit and an amend run `git commit`, so your `pre-commit`/`commit-msg` hooks and signing configuration apply. The commit bar names the identity a commit will carry (`user.name` / `user.email`, repository config overriding global) before you press Enter. The first commit in a repository is the exception: it is created with `commit-tree` + `update-ref` and does not run those hooks or sign (see **Current limitations**).
- **Pull does not rewrite by default.** Pull uses `git pull --ff-only` unless you choose Merge or Rebase in **Settings → Git**. The default reports a diverged branch instead of creating a merge commit or rebasing your history.
- **Your state stays local.** Repository inventory, groups and assignments live in `state.json` in the platform data directory. No analytics endpoint, no account, no sync.
- **Optional avatar lookups.** The commit bar attempts to show the GitHub profile photo for the identity it will commit as. Vantage resolves a login from a GitHub noreply address, `github.user` or a logged-in `gh` CLI, asks `github.com/<login>.png` once per account through the system `curl`, and caches the image beside `state.json` for a week. There is no token, no other endpoint and no telemetry; when no login can be resolved, the photo is skipped and initials are drawn locally instead.
- **Failures are visible.** A repository that cannot be read is reported by name; the rest of the dashboard keeps working.

---

## Current limitations

- **The first commit skips Git commit machinery.** Root commits are written with `commit-tree` + `update-ref`, so `pre-commit`/`commit-msg` hooks and commit signing do not run for a repository's initial commit. Later commits and amends use `git commit` as usual.
- **Amend is subject-only.** Amend pre-fills the subject but not the description field; the amend path sends no body, so an existing commit body is replaced. A changed file must be ticked before amend can be submitted, so a message-only amend is not available from the UI.
- **The watcher ignores `.git`.** Working-tree edits update live, but commits, fetches and checkouts performed in a terminal may not appear until Vantage is re-activated or you press `Cmd+R`.
- **Local-only markers are positional.** Vantage marks the newest `ahead` commits as not pushed. On a branch that has both diverged from and fallen behind its upstream, that boundary can be approximate.
- **Desktop integrations are macOS-first.** Reveal in file manager and Open in Terminal currently call macOS `open`; the editor fallback also calls `open`. These actions may not work on Linux builds. The native menu bar and `Cmd`-based shortcuts are macOS-oriented too.
- **No recovery browser.** Backup refs and the discard trash exist in the repository, but Vantage has no UI to list or restore them yet.

---

## Installation

macOS 13 or newer, Apple silicon or Intel. Download the latest DMG from the
[releases page](https://github.com/beingbalveer/Vantage/releases), open it and
drag **Vantage** into Applications.

Each release publishes four files:

| Asset | What it is |
|---|---|
| `Vantage_<version>_universal.dmg` | Drag-to-Applications installer, arm64 + x86_64 |
| `Vantage_<version>_universal.app.zip` | The same bundle, zipped |
| `SHA256SUMS` | Checksums for both artifacts |
| `vantage.rb` | Homebrew cask for that exact build, pinned to its `sha256` |

Release builds are signed with a Developer ID certificate and notarized by
Apple when those credentials are configured; builds made without them are
ad-hoc signed and need **right-click → Open** on first launch. Vantage has no
account, no telemetry and no updater daemon: check the releases page for new
versions.

Linux builds from source (below); the release pipeline currently publishes
macOS only.

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
| Stage or unstage every file | Click the header checkbox in **Changes** |
| Commit staged changes | Type a message in the bottom bar of **Changes** and press `Enter`, or `Cmd+Enter` |
| Undo the last commit | Click **Undo** in the strip above the commit bar (only while that commit is unpushed) |
| Amend the tip commit | Right-click it in **Commits** → **Amend commit**, then commit |
| Revert a commit | Right-click it in **Commits** → **Revert commit** |
| Reset the branch to a commit | Right-click it in **Commits** → **Reset branch here** |
| Check out a branch | Right-click it in **Branches** → **Checkout branch** |
| Merge, create, rename or delete a branch | Right-click it in **Branches** |
| Discard a file's changes | Right-click it in **Changes** → **Discard changes** |
| Stash or discard everything | Right-click the **Changes** header → **Stash all changes** / **Discard all changes** |
| Manage stashes | Right-click the repository row → **Manage stashes…** (when stashes exist) |
| Add an untracked file to `.gitignore` | Right-click it in **Changes** → **Add to .gitignore** |
| Fetch the selected repository | **Fetch Origin** in the detail pane, or `Cmd+Shift+F` |
| Pull or push the selected repository | **Pull** / **Push** in the detail pane bar, or `Cmd+Shift+P` / `Cmd+P` |
| Add a repository | **Add Repository** in the empty state, the `+` header button, or `Cmd+O` |
| Create or clone a repository | File → **New Repository…** (`Cmd+N`) / **Clone Repository…** (`Cmd+Shift+O`) |
| Refresh status | Click the refresh icon in the header, or `Cmd+R` |
| Switch views | View menu, or `Cmd+1` All Repositories, `Cmd+2` Needs Attention, `Cmd+3` Pending Changes |
| Open Settings | `Cmd+,`, Vantage menu, or the sidebar **Settings** entry |
| Toggle the sidebar | Click the header button, or `Cmd+B` |
| Toggle light/dark | Open **Settings → Appearance**, or `Cmd+Shift+L` |
| Full screen / minimize / close | `Ctrl+Cmd+F` / `Cmd+M` / `Cmd+W` |
| Close the inspector or a dialog | Click outside a dialog, or press `Esc` in a focused field (Add Repository, branch prompt, amend or inspector) |
| Submit a group or add-repository field | Press `Enter` in the field |

The three views are currently reachable from the View menu and `Cmd+1/2/3`; the current build does not ship a sidebar view switcher. The menu bar provides the same actions under **Vantage**, **File**, **View**, **Window** and **Help**, including About, Services, Hide, Quit and the standard window commands.

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
