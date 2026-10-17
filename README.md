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
Installing via Homebrew provides seamless 1-click launching with no security prompts:

```bash
brew tap beingbalveer/vantage
brew install --cask vantage
```

#### Direct Download (.dmg)
1. Download the latest `Vantage_universal.dmg` from [GitHub Releases](https://github.com/beingbalveer/Vantage/releases).
2. Open the DMG and drag **Vantage** to your **Applications** folder.

> **macOS Gatekeeper Note:**  
> If macOS warns that *"Vantage cannot be opened because Apple cannot check it for malicious software"*, you can either:
> - **Right-click** (or `Control` + Click) `Vantage.app` in `/Applications` and select **Open**, then click **Open** in the dialog.
> - Or run this one-line command in your Terminal:
>   ```bash
>   xattr -cr /Applications/Vantage.app
>   ```

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
| `Mod+B` | Toggle sidebar (full / collapsed icons) |
| `` Mod+` `` | Toggle terminal drawer in focused pane |
| `Mod+[` / `Mod+]` | Switch to previous / next repository in focused pane |
| `Mod+,` | Open Settings |
| `Mod+Shift+F` | Fetch repository |
| `j` / `k` | Navigate next / previous item in focused list (commits, files) |
| `Shift+j` / `Shift+k` | Extend commit selection |
| `{` / `}` | Jump to previous / next flagged commit |
| `r` | Mark reviewed (advances marker to HEAD) |
| `]` / `[` | Jump to next / previous file in diff |
| `n` / `p` | Jump to next / previous hunk |
| `e` | Expand all context in current file |
| `w` | Toggle ignore whitespace |
| `c` | Copy commit SHA |
| `/` | Search history or filter files |
| `Alt+Click` / `F12` | Peek symbol definition |
| `Mod+Enter` | Commit staged changes / Save review note |
| `Esc` | Close dialog, palette, or popover |

*`Mod` corresponds to `Cmd` on macOS and `Ctrl` on Windows / Linux.*

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
