<p align="center">
  <img src="assets/vantage-logo-horizontal.png" alt="Vantage Logo" width="460" />
</p>

<p align="center">
  <strong>Fast, local, multi-repo Git desktop client, read-only code viewer, and terminal workspace.</strong>
</p>

<p align="center">
  <a href="https://github.com/beingbalveer/Vantage/releases"><img src="https://img.shields.io/github/v/release/beingbalveer/Vantage?style=flat-square" alt="Latest Release" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square" alt="License: MIT" /></a>
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey?style=flat-square" alt="Platform Support" />
</p>

---

Vantage gives you instant visibility into unreviewed commits and diffs across all your projects without the overhead of opening multiple heavy IDE windows or navigating slow web interfaces.

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

## Why Vantage?

- **Multi-Repo Split Panes (1 to 3 Columns):** Review multiple repositories or branches side by side (`Mod+\` to cycle columns, `Mod+1`/`Mod+2`/`Mod+3` to switch focus). Each pane preserves its own active tab, selection, scroll position, and terminal state.
- **Combined Unreviewed Diffs:** Track what you've personally reviewed with an internal marker that never pollutes your repository's Git history. View net unreviewed changes across commits in a single, unified view.
- **Embedded Persistent Terminal:** Each repository gets a dedicated PTY terminal session at the repo root. Sessions persist when switching repos or tabs with a 256 KB replay buffer.
- **Read-Only Code Viewer & Definition Peek:**
  - Fast syntax highlighting across languages.
  - Blame inspection with inline author and commit details.
  - Heuristic symbol definition peek (`Alt+Click` / `F12`) across project files without opening an external editor.
  - Safe by design: read-only viewing ensures you never accidentally edit files or dirty the working tree while browsing.
- **Automated Triage Badges:** Immediate heuristic detection for commits touching dependencies, large file deletions, secrets/keys, config changes, or removed test suites.
- **Safety by Default:** Every destructive action (discard, hard reset) creates an immediate recoverable backup ref (`refs/vantage/backup/...`) with a one-click restore UI in Settings.
- **100% System Git Compatible & Zero Telemetry:** Vantage uses your existing system `git` CLI, inheriting your existing SSH keys, GPG signing configs, credential helpers, and Git hooks. No data ever leaves your computer.

---

## Keyboard Shortcuts

Press `?` inside Vantage at any time to open the interactive keyboard shortcut overlay.

| Shortcut | Action |
|---|---|
| `Mod+K` / `Ctrl+Shift+P` | Open command palette (repos, `>` commands, `/` files, `#` commits, `@` notes) |
| `Mod+O` | Add repository |
| `Mod+1` / `Mod+2` / `Mod+3` | Focus pane 1, 2, or 3 |
| `Mod+\` | Cycle pane count (1, 2, 3 columns) |
| `Mod+W` | Close focused pane |
| `Mod+B` | Toggle sidebar (full / collapsed icons) |
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
| `/` | Filter the file list (Files tab) |
| `F12` | Peek symbol definition (code viewer; `Alt+Click` also works) |
| `Mod+F` | Find in file (code viewer) |
| `Mod+Enter` | Commit selected changes (when the commit box is open) |
| `t` | Switch between History and Files tabs |
| `?` | Keyboard shortcuts help |
| `Esc` | Close dialog, palette, menu, or popover |

*`Mod` corresponds to `Cmd` on macOS and `Ctrl` on Windows / Linux. Single-letter shortcuts never fire while typing in an input or the terminal; the terminal keeps only the modifier shortcuts it needs (`` Mod+` ``, `Mod+1/2/3`, `Mod+B`, `Mod+[` / `Mod+]`, and `Ctrl+Shift+P` on Windows / Linux).*

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
