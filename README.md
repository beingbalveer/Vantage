<p align="center">
  <img src="assets/vantage-logo-horizontal.png" alt="Vantage Logo" width="440" />
</p>

<h3 align="center">Read the code. Check the branch. Run the agent. One app.</h3>

<p align="center">
  A fast, local, read-only review deck for AI-agent commits — across every repo you have open.
</p>

<p align="center">
  <a href="https://github.com/beingbalveer/Vantage/releases"><img src="https://img.shields.io/github/v/release/beingbalveer/Vantage?style=flat-square" alt="Latest Release" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square" alt="License: MIT" /></a>
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey?style=flat-square" alt="Platform Support" />
  <img src="https://img.shields.io/badge/telemetry-none-brightgreen?style=flat-square" alt="No telemetry" />
  <img src="https://img.shields.io/badge/built%20with-Tauri%20%2B%20Rust-24C8DB?style=flat-square" alt="Built with Tauri and Rust" />
</p>

---

## Why this exists

I work across several repositories at once, and most of my code is written by AI agents — in a terminal, or in the agent's own app.

That left me with three windows open all day:

- **VS Code** — not to edit, but to *read* what the agent wrote.
- **GitHub Desktop** — to check which branch the agent was on and what it had committed.
- **A terminal** — because that's where the agents run.

Three apps, three memory footprints, and constant alt-tabbing. So I built the tool I wanted: one fast, local app for the review loop, and nothing else.

If you run agents across more than one repo and want to see what they actually did before it ships, this is for you.

---

## The three things Vantage does

### 1. Read code without editing it

A syntax-highlighted, read-only viewer with blame, find-in-file, and heuristic definition peek (`F12` / `Alt+Click`). Files open in tabs with VS Code-style close shortcuts, a right-click tab menu, and a centered fuzzy file finder (`Mod+B`). Browsing is read-only on purpose: when you are auditing agent output, the last thing you want is a stray keystroke dirtying the working tree. When you do need to change something, the pencil icon opens an explicit edit mode — tick or `Mod+S` saves, `Esc` discards, and leaving with unsaved edits asks first. Saves always target the working-tree copy, never the revision you are viewing.

Ignored local config is still readable: `.env` and `.env.*` files appear in the explorer and the file finder even though Git never tracks them.

### 2. See what changed across repos

Open one to three repositories side by side. Each pane keeps its own active tab, selection, scroll position, and terminal state. A personal review marker — stored outside Git history — shows the net diff of everything that landed since you last looked, so you can review across commits instead of one commit at a time. Heuristic triage badges flag dependency changes, large deletions, secrets, config changes, and removed tests.

Uncommitted work is shown as one working diff from `HEAD` to the worktree. There is no staging area to maintain: every changed file is visible at once, and line-level discard is there when you need to drop a specific selection.

### 3. Run the agent where it lives

Every repository gets a persistent PTY terminal at its root, with a 256 KB replay buffer. Switch repos or tabs and the session is still there. No second terminal app, no `cd` archaeology. The drawer hides instantly without killing the shell, shows which shell is running, and can be maximized or dragged to the height you want.

---

## Scope, on purpose

| Vantage is | Vantage is not |
|---|---|
| A review deck for agent-written commits | An IDE or a code editor |
| A read-only-by-default browser with an explicit edit mode | An IDE or a multi-file refactor tool |
| A multi-repo branch and commit monitor | A GitHub PR / issue / CI client |
| A per-repo terminal workspace | A terminal emulator replacement |
| Local-first, zero telemetry, system Git | A cloud service or account system |

Keeping the scope small is the feature. There is no PR integration, no AI chat, no plugin marketplace — and no plan to add them.

---

## The loop

<!--
  TODO(portfolio): record assets/demo.gif (45–60s) and replace this comment with:
  <p align="center"><img src="assets/demo.gif" alt="Vantage demo" width="900" /></p>

  Shot list:
  1. Three panes, three repos, one keystroke between them.
  2. Unreviewed diff across commits, then press `r` to mark reviewed.
  3. Jump from a diff hunk into the read-only viewer; `F12` for definition peek.
  4. `Mod+B` centered fuzzy file search, `Enter` to open.
  5. Drop into the repo's terminal and start the next agent task.
-->

> **Open three repos → see what landed since you last looked → mark it reviewed → jump into the file and the diff → `Mod+B` to fuzzy-find a file → drop into that repo's terminal and run the next agent task.**

Everything is reachable from the keyboard. `Mod+K` opens the command palette, `?` shows every binding, and `Mod+1/2/3` moves between panes.

---

## Built to stay small

Vantage is a [Tauri](https://tauri.app/) app: a Rust backend around the system WebView. There is no bundled Chromium, and no second copy of your repository index.

On my machine (macOS, debug build, no repositories open) it idles around **120 MB**. The VS Code + GitHub Desktop pair I used to keep open sat around **900 MB** before I opened a terminal. A release-build benchmark across one and three panes is the next number I want to publish.

<!-- TODO: replace the paragraph above with a proper benchmark table (release build; idle / 1 repo / 3 panes + terminal). -->

---

## Safety by default

- **System Git, invoked directly.** Your existing SSH keys, GPG signing config, credential helpers, and hooks just work. Git operations are executed as arguments, not through a shell.
- **A commit dialog, not a commit hotkey.** Committing opens a centered dialog with the selected files, the author identity (repository, global, or a one-off custom author), and — when the GitHub CLI is signed in — the active account and its push permission on the remote. Nothing is written until you press Commit.
- **Backups before destructive actions.** Discard and hard reset create a recoverable ref under `refs/vantage/backup/…`, with a restore UI in Settings.
- **File-scoped writes.** Commits and discards use literal pathspecs and a temporary index for partial commits, so an unselected file cannot ride along.
- **Zero telemetry.** Nothing leaves your machine. There is no account, no sync, and no analytics endpoint.

---

## Installation

### macOS

#### Via Homebrew (Recommended)
The cask lives in this repository, so point Homebrew at it explicitly:

```bash
brew tap beingbalveer/vantage https://github.com/beingbalveer/Vantage
brew trust beingbalveer/vantage
brew install --cask vantage
```

`brew trust` is the Homebrew 7 confirmation for third-party taps; on older Homebrew the tap can be installed directly.

If you previously installed Vantage from a DMG, quit the app and remove `/Applications/Vantage.app` first (or run `brew install --cask --force vantage`).

The cask clears the download quarantine flag during install, so the app launches without a Gatekeeper prompt.

#### Direct Download (.dmg)
1. Download the latest `Vantage_<version>_universal.dmg` from [GitHub Releases](https://github.com/beingbalveer/Vantage/releases).
2. Open the DMG and drag **Vantage** to your **Applications** folder.

> **macOS Gatekeeper Note:**
> The current build is ad-hoc signed, not notarized. If macOS warns that *"Vantage cannot be opened because Apple cannot check it for malicious software"*, either:
> - **Right-click** (or `Control` + Click) **Vantage.app** in `/Applications`, choose **Open**, then click **Open**; or
> - run `xattr -cr /Applications/Vantage.app` in Terminal.

### Windows
Download and run the `.msi` or `.exe` installer from [GitHub Releases](https://github.com/beingbalveer/Vantage/releases).

### Linux
Download the `.deb` package or `.AppImage` from [GitHub Releases](https://github.com/beingbalveer/Vantage/releases).

---

## Keyboard Shortcuts

Press `?` inside Vantage at any time to open the interactive keyboard shortcut overlay.

| Shortcut | Action |
|---|---|
| `Mod+K` / `Ctrl+Shift+P` | Open command palette (repos, `>` commands, `/` files, `#` commits, `@` notes) |
| `Mod+O` | Add repository |
| `Mod+1` / `Mod+2` / `Mod+3` | Focus pane 1, 2, or 3 |
| `Mod+\` | Cycle pane count (1, 2, 3 columns) |
| `Mod+W` | Close active file tab (Files tab); close focused pane elsewhere |
| `Mod+Shift+W` | Close all file tabs (Files tab) |
| `Ctrl+Tab` / `Ctrl+Shift+Tab` | Next / previous file tab (Files tab) |
| `Mod+B` | Toggle sidebar; search files in the Files tab |
| `Mod+Shift+E` | Search files (Files tab) |
| `Mod+S` | Save the file being edited (Files tab) |
| `` Mod+` `` | Toggle terminal drawer in focused pane |
| `Mod+[` / `Mod+]` | Switch to previous / next repository in focused pane |
| `Mod+,` | Open Settings |
| `Mod+Shift+R` | Refresh all repositories |
| `Mod+Shift+F` | Fetch / Pull / Push (Sync) |
| `j` / `k` | Navigate next / previous item in focused list (commits or files) |
| `Shift+j` / `Shift+k` | Extend commit selection |
| `{` / `}` | Jump to previous / next flagged commit |
| `Enter` | Move focus: commits → files → diff (History tab) |
| `r` | Mark reviewed (advances marker to HEAD, or to the newest selected commit) |
| `c` | Copy commit SHA (History tab) / copy repository path (Files tab) |
| `]` / `[` | Jump to next / previous file in diff |
| `n` / `p` | Jump to next / previous hunk |
| `e` | Expand all context in current file |
| `w` | Toggle ignore whitespace |
| `/` | Search files (Files tab) |
| `F12` | Peek symbol definition (code viewer; `Alt+Click` also works) |
| `Mod+F` | Find in file (code viewer) |
| `Mod+Enter` | Commit selected changes (when the commit dialog is open) |
| `t` | Switch between History and Files tabs |
| `?` | Keyboard shortcuts help |
| `Esc` | Close dialog, palette, menu, or popover |

*`Mod` corresponds to `Cmd` on macOS and `Ctrl` on Windows / Linux. Single-letter shortcuts never fire while typing in an input or the terminal; the terminal keeps only the modifier shortcuts it needs (`` Mod+` ``, `Mod+1/2/3`, `Mod+B`, `Mod+[` / `Mod+]`, and `Ctrl+Shift+P` on Windows / Linux).*

Right-click a file tab (or press `Shift+F10`) for **Close**, **Close Others**, **Close to the Right**, **Close All**, **Copy Path**, **Reveal in File Manager**, and **Open in External Editor**. File search (`Mod+B`, `Mod+Shift+E`, or `/` in the Files tab) opens a centered quick-open overlay with fuzzy path matching, arrow-key navigation, and `Enter` to open.

---

## Under the hood

- **Tauri 2 + Svelte 5 + Rust.** The UI is Svelte; Git, diffing, the PTY, and file previews live in Rust.
- **Typed IPC end to end.** Every backend command has a typed wrapper on both sides of the bridge.
- **Tested and gated.** 270+ frontend tests and 160+ Rust tests, plus `svelte-check`, `clippy`, `rustfmt`, and a design-token ratchet, run on macOS and Linux in CI.
- **Decisions are written down.** Stack, scope, and platform choices live in [`docs/decisions.md`](docs/decisions.md) and [`docs/adr/`](docs/adr/).
- **Cross-platform releases.** macOS (universal), Windows, and Linux builds are produced by the release workflow, with auto-update and a Homebrew cask.

---

## Contributing & Building from Source

Interested in developing or building Vantage locally? Check out the [Contributing Guide](CONTRIBUTING.md).

---

## Acknowledgments & Trademarks

- Built with [Tauri](https://tauri.app/), [Svelte](https://svelte.dev/), [xterm.js](https://xtermjs.org/), [Highlight.js](https://highlightjs.org/), and [Lucide Icons](https://lucide.dev/).
- Git is a registered trademark of Software Freedom Conservancy.
- GitHub Desktop is a trademark of GitHub, Inc. Vantage is an independent open-source project and is not affiliated with or endorsed by GitHub, Inc.

---

## License

[MIT](LICENSE) © 2026 Balveer Dhanoriya
