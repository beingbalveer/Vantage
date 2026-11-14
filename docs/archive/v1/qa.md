# Vantage: Manual QA Checklist & Verification Notes

This checklist verifies the requirements from Section 15 of `PRODUCT_SPEC.md` across core workflows, error conditions, edge cases, and performance invariants.

---

## 1. Core Review Loop Verification (Section 1.3)
Test with at least 6 real repositories simultaneously registered in Vantage.

- [ ] **Sidebar Badges:**
  - Registered repos show accurate dirty file dots and `+N` new commit badges relative to their reviewed marker.
  - Making a commit externally in a registered repo updates the `+N` badge within 1 second.
- [ ] **Combined Diff Scope:**
  - Selecting a repo with new commits defaults to the combined diff scope (`marker..HEAD`).
  - Net diff accurately reflects all files and lines changed across the unreviewed commits.
- [ ] **Intra-line & Context Expansion:**
  - Intra-line word diffs highlight exact modified characters/words on changed lines.
  - Clicking "Expand context" cleanly loads surrounding blob lines without layout distortion.
- [ ] **Definition Peek:**
  - Selecting a symbol or double-clicking an identifier displays the definition peek popover with file and line context.
  - Works across Rust, TypeScript, and Python codebases.
- [ ] **Triage Flags Navigation:**
  - Flagged commits (Large, Deletions, Dependencies, Config, TestsRemoved, Secrets, BigFile, Binary) display severity chips.
  - Pressing `{` and `}` cycles focus directly between flagged commits in the history list.
- [ ] **Mark Reviewed:**
  - Pressing `r` moves the reviewed marker to HEAD.
  - The `+N` badge clears immediately, and the timeline transitions to reviewed state.
- [ ] **Command Palette Repo Switch:**
  - Pressing `Mod+K` opens the palette with immediate keyboard focus.
  - Typing a repo name and pressing `Enter` switches the focused pane immediately.

---

## 2. Resilience, Error Handling & Chaos Tests
- [ ] **Network Severed during Remote Sync:**
  - Initiate a `git pull` or `git fetch` and disconnect network/Wi-Fi mid-operation.
  - Vantage displays a non-modal error toast with standard Git network failure message; the UI does not hang or freeze.
- [ ] **Repo Folder Deleted While Open:**
  - Delete or rename a registered repository's directory on disk while Vantage is actively viewing it.
  - The sidebar marks the repo with a missing/warning indicator; switching panes or clicking other repos continues to function normally.
- [ ] **Rapid Agent Commits (Stress Test):**
  - Run a script committing changes every 2 seconds into a registered repository.
  - The debounced ref watcher (250 ms) and status poller update state smoothly without CPU spikes or UI lockups.
- [ ] **Concurrent Git Write / Lock Contention:**
  - If a Git process holds `.git/index.lock`, Vantage retries with exponential backoff and gracefully displays "Another Git process is running" if locked beyond timeout.
- [ ] **Detached HEAD & Empty Repos:**
  - Checkout a raw commit (detached HEAD): branch shows `HEAD detached at <short_sha>`; marker and diffs remain functional; sync button hides safely.
  - Open an empty repository with no commits: shows "No commits yet", pseudo-commit allows creating the initial commit without crashing.

---

## 3. Responsive Panes & Multi-Window Layouts
- [ ] **3-Column Pane Setup:**
  - Cycle pane count to 3 columns via `Mod+\`.
  - Assign different repositories to each pane; each pane independently maintains its active tab, selection, and terminal state.
- [ ] **Minimum Window Constraint:**
  - Shrink the app window to the minimum size (900x560 px).
  - Panes enforce their minimum 420 px width and automatically collapse to 2 or 1 column as necessary without layout overflow.
- [ ] **High-DPI / Retina Displays:**
  - App renders sharp text and 1px hairlines at 100%, 150%, and 200% OS scale factors.
- [ ] **Theme Switching:**
  - Switching between Light and Dark OS themes (or toggling theme in Settings) instantly updates all CSS variables, terminal colors, and diff gutter tokens without requiring a restart.

---

## 4. Embedded Terminal (PTY & xterm.js)
- [ ] **Session Persistence & Ring Buffer:**
  - Run a command with multi-screen output (e.g., `cargo test` or `git log`).
  - Switch to another repository and return; the terminal output is restored from the 256 KB ring buffer.
- [ ] **Bracketed Paste for Review Notes:**
  - Add notes in the review panel and click "Send to terminal".
  - Notes are pasted into the terminal using bracketed paste mode without automatically executing (no trailing newline).
- [ ] **Terminal Resizing:**
  - Resizing the pane or drawer dynamically updates cols/rows via `term_resize` and refits the xterm canvas.

---

## 5. Destructive Safety & Recovery
- [ ] **Discard Changes:**
  - Discarding tracked changes creates a backup stash under `refs/relay/backup/<timestamp>-discard`.
  - Discarding untracked files moves them to `.git/relay-trash/<timestamp>/` rather than permanently unlinking.
- [ ] **Hard Reset to Marker:**
  - Resetting to a reviewed marker creates `refs/relay/backup/<timestamp>-reset` pointing to previous HEAD before resetting.
  - If unpushed commits exist, a caution dialog warns before proceeding.
- [ ] **Settings Recovery View:**
  - Settings -> Recovery displays all created backup refs with a one-click Restore action.
