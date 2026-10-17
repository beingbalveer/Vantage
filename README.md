# Vantage

**Vantage** is a fast, local, multi-repo Git desktop client, read-only code viewer, and terminal workspace designed for fast, focused review of code across multiple repositories.

Built with **Tauri v2 + Rust + Svelte 5 + TypeScript + Vite**, Vantage gives you instant visibility into what was committed across all your projects without the overhead of opening multiple heavy IDE windows or navigating complex branch graph tools.

---

## The Core Loop

1. **Glance at the sidebar:** View dirty status indicators and commit counts showing unpushed and unreviewed commits across all your registered repositories.
2. **Open the repository:** Instantly see a single **combined diff** of everything committed since your last reviewed marker.
3. **Skim & verify:** Expand surrounding code context or peek symbol definitions (`Alt+Click` / `F12`) directly within the diff without leaving the app.
4. **Triage suspicious changes:** Jump directly between flagged commits (`{` / `}`) indicating removed tests, added secrets, dependency changes, or large deletions.
5. **Leave agent or team feedback:** Select code lines, add review notes, and click **Copy as prompt** or **Send to terminal** (via bracketed paste).
6. **Mark reviewed & repeat:** Advance the reviewed marker to HEAD (`r`), then press `Mod+K` to jump to the next repository.

---

## Key Features

- **Multi-Repo Split Panes (1 to 3 Columns):** Review multiple repositories or multiple branches side by side (`Mod+\` to cycle columns, `Mod+1`/`Mod+2`/`Mod+3` to switch focus). Each pane preserves its own active tab, selection, scroll position, and terminal state.
- **Reviewed Marker & Combined Diffs:** Track what you've personally reviewed per repo with an internal marker that never pollutes your repository's Git history. View net unreviewed changes or step through individual commits.
- **Embedded Persistent Terminal:** Each repository gets a dedicated PTY terminal session (`portable-pty` + `@xterm/xterm`) at the repo root. Sessions persist when switching repos or tabs with a 256 KB ring-buffer replay.
- **Read-Only Code Viewer & Definition Peek:**
  - Fast whole-blob syntax highlighting via `highlight.js` (files >1MB or lines >2,000 characters automatically gated).
  - Blame-lite inspection with inline author and commit details.
  - Heuristic symbol definition search (`grep_defs`) across project files.
  - Safe by design: read-only viewing ensures you never accidentally edit files or dirty the working tree while browsing.
- **Automated Triage Flags:** Heuristic advisory badges for:
  - `Large`: Commits touching >15 files or >500 lines
  - `DeletionHeavy`: Significant net deletions (>50 deletions and >2× additions)
  - `Dependencies`: Modifications to lockfiles or package manifests
  - `Config`: CI workflows, Dockerfiles, Kubernetes manifests, migrations, Terraform
  - `TestsRemoved`: Deletion of test files or disabled test suites (`skip`, `only`, `@Ignore`, `#[ignore]`)
  - `Secrets`: Added private keys, AWS tokens, GitHub tokens, OpenAI keys, or `.env` files
  - `BigFile`: Added or modified blobs exceeding 1 MB
  - `Binary`: Binary assets added or updated
- **Safety by Default:** Every destructive action creates an immediate recoverable backup ref:
  - Discarding tracked changes creates `refs/vantage/backup/<timestamp>-discard`.
  - Discarding untracked changes moves files to `.git/vantage-trash/<timestamp>/`.
  - Hard reset creates `refs/vantage/backup/<timestamp>-reset` pointing to previous HEAD.
  - Full restore and recovery interface in Settings -> Recovery.
- **Zero Telemetry & 100% System Git Compatibility:** Vantage shells out directly to your system `git` CLI, inheriting your existing SSH keys, GPG signing configs, credential helpers, and Git hooks.

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
| `s` | Toggle unified / split diff mode |
| `c` | Copy commit SHA |
| `/` | Search history or filter files |
| `Alt+Click` / `F12` | Peek symbol definition |
| `Mod+Enter` | Commit staged changes / Save review note |
| `Esc` | Close dialog, palette, or popover |

*`Mod` corresponds to `Cmd` on macOS and `Ctrl` on Windows / Linux.*

---

## Architecture & Technology Stack

```
┌───────────────────────── Tauri Desktop Window ─────────────────────────┐
│ Svelte 5 UI: Sidebar · Panes[1..3] · CommandPalette · Dialogs · Toasts │
│   stores: repos, panes, settings, ui, repoIcons                       │
└───────────────▲──────────────────────────────┬─────────────────────────┘
        events (Rust → UI)                 commands (UI → Rust, invoke)
┌───────────────┴──────────────────────────────▼─────────────────────────┐
│ Rust Backend (src-tauri)                                               │
│  state::Store      atomic JSON persistence (<data_dir>/vantage/state.json)│
│  git::Runner       concurrency semaphore (8), per-repo write mutex      │
│  git::{status,log,diff,ops,refs,blob,grep}                             │
│  triage::          flag heuristics                                     │
│  term::            portable-pty manager + 256 KB ring buffer           │
│  watch::           notify file-system watcher (.git internals)         │
└────────────────────────────────────────────────────────────────────────┘
```

- **Backend:** Rust edition 2021, `tauri` v2, `tokio`, `portable-pty`, `notify`, `similar`, `chrono`, `dunce`, `uuid`, `regex`, `dirs`, `which`.
- **Frontend:** Svelte 5 (runes reactivity), TypeScript, Vite, `@xterm/xterm`, `@xterm/addon-fit`, `highlight.js`.
- **Design System:** Custom CSS variables based on GitHub Primer tokens supporting automatic Light and Dark modes.

---

## Development & Build Instructions

### Prerequisites

1. **Rust:** Stable toolchain (`rustc`, `cargo` edition 2021+). [Install Rust](https://rustup.rs/).
2. **Node.js:** Node 18+ and `npm`. [Install Node.js](https://nodejs.org/).
3. **Git:** System `git` CLI installed and available on `PATH`.

#### Linux Dependencies
On Debian/Ubuntu-based distributions, install the following required system development packages:
```bash
sudo apt-get update && sudo apt-get install -y \
  libwebkit2gtk-4.1-dev \
  build-essential \
  curl \
  wget \
  file \
  libssl-dev \
  libgtk-3-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev
```

### Running Locally

```bash
# Install frontend dependencies
npm install

# Start the Tauri development window with hot reload
npm run tauri dev
```

### Running Tests & Quality Checks

```bash
# Run frontend unit tests (Vitest)
npm test

# Check frontend types and templates
npm run check

# Build frontend production bundle (Vite)
npm run build

# Run Rust backend unit tests
cargo test --manifest-path src-tauri/Cargo.toml

# Run Rust strict linter
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings

# Check Rust code formatting
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
```

### Building Release Installers

To package native installers (e.g., `.dmg` / `.app` on macOS, `.msi` / `.exe` on Windows, `.deb` / `.AppImage` on Linux):

```bash
npm run tauri build
```

The compiled binary and installers will be output to `src-tauri/target/release/bundle/`.

#### Note on Unsigned macOS Releases
If running an unsigned build on macOS, Apple Gatekeeper may show a warning. You can allow the app via **System Settings → Privacy & Security → Open Anyway**, or by running:
```bash
xattr -cr /Applications/Vantage.app
```

---

## Acknowledgments & Trademarks

- Built with [Tauri](https://tauri.app/), [Svelte](https://svelte.dev/), [xterm.js](https://xtermjs.org/), [Highlight.js](https://highlightjs.org/), and [Lucide Icons](https://lucide.dev/).
- Git is a registered trademark of Software Freedom Conservancy.
- GitHub Desktop is a trademark of GitHub, Inc. Vantage is an independent open-source project and is not affiliated with or endorsed by GitHub, Inc.

---

## License

[MIT](LICENSE) © 2026 Balveer Dhanoriya
