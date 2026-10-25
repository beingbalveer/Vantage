# Vantage: Architectural & Design Decisions

## 1. Stack & Runtime
- **Desktop Shell**: Tauri v2 (`@tauri-apps/api` v2, `@tauri-apps/cli` v2) with Rust backend and native system webview. Keeps release binary lightweight and RAM under budget.
- **Frontend**: Svelte 5 + TypeScript + Vite. Hand-written CSS using GitHub Primer and Vantage design tokens.
- **Git Interop**: Standard `tokio::process::Command` shelling out to the local `git` CLI with `GIT_OPTIONAL_LOCKS=0`, `LC_ALL=C`, and `GIT_TERMINAL_PROMPT=0`.
- **Terminal**: `portable-pty` on Rust backend, `@xterm/xterm` + `@xterm/addon-fit` on frontend with 256 KB ring-buffer session replay.

## 2. Dependencies
- **Crates**:
  - `tauri` (v2): Desktop app shell and IPC bridge.
  - `serde`, `serde_json`: Machine-readable data serialization.
  - `tokio`: Async runtime with global semaphore (8) and timeout protection.
  - `portable-pty`: Cross-platform pseudo-terminal management.
  - `notify`: File system event watching for `.git/HEAD`, `.git/refs`, and `.git/index`.
  - `similar`: Intra-line word diff computation on changed lines under 500 characters.
  - `chrono`: ISO-8601 timestamps and relative time handling.
  - `dunce`: Path normalization without Windows verbatim path prefixes.
  - `uuid`: Unique identifier generation for repositories and notes.
  - `regex`: Triage rule heuristics and identifier definition matching.
  - `dirs`: Cross-platform OS application data directory lookup.
  - `which`: Shell discovery across Windows, macOS, and Linux.
  - `rfd`: Cross-platform native folder and file dialog integration (Cocoa NSOpenPanel on macOS, IFileDialog on Windows, zenity/portal on Linux).

- **NPM Packages**:
  - `svelte` (v5), `@sveltejs/vite-plugin-svelte`: Reactive UI components.
  - `@xterm/xterm`, `@xterm/addon-fit`: Fast, canvas/DOM terminal emulator.
  - `highlight.js`: Lightweight whole-blob syntax highlighting with cross-line span tracking.
  - `vitest`: Fast TypeScript unit testing for fuzzy scoring, date formatting, and keys.

## 3. Storage & State Persistence
- App state is persisted in `<data_dir>/relay/state.json`.
- Uses atomic writes (temp file write + rename).
- Unrecognized JSON keys are retained through `extra: HashMap<String, Value>` to guarantee forward compatibility.
- Corrupted JSON is moved to `state.corrupt-<timestamp>.json` and an initial state is restored.

## 4. Safety & Destruction Prevention
- Discarding tracked files creates a stash backup under `refs/relay/backup/<timestamp>-discard`.
- Untracked files are moved to `.git/relay-trash/<timestamp>/` rather than deleted.
- Hard reset to marker creates `refs/relay/backup/<timestamp>-reset` pointing to `HEAD`.
- Backups older than 30 days are automatically pruned on startup.
