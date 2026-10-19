# Vantage vs. GitHub Desktop — comparison and improvement backlog

Date: 2026-10-19
Scope: current `src/` + `src-tauri/src/` working tree vs. GitHub Desktop's documented behavior
(docs.github.com/en/desktop).

---

## 1. Positioning in one line

GitHub Desktop is a **general-purpose authoring client** (clone → edit elsewhere → commit → push → open a PR).
Vantage is a **review deck for AI-written commits** (see what landed across many repos, understand it, flag it, steer the agent).

So the goal is **not** to clone GitHub Desktop. The goal is: (a) actually deliver the review-deck workflow in the UI,
and (b) close only the authoring gaps that block day-to-day use of the review loop.

---

## 2. Feature matrix

Legend: ✅ present and reachable · ⚠️ partially present (backend/dialog exists but no UI path, or incomplete) · ❌ missing

| Area | Capability | GitHub Desktop | Vantage |
|---|---|---|---|
| **Acquire repos** | Add existing local repo / drag-and-drop | ✅ | ✅ |
| | Clone from URL / GitHub / Enterprise | ✅ | ❌ |
| | Create new repository (gitignore, license, README) | ✅ | ❌ |
| | Fork, alias repos, change remote URL | ✅ | ❌ |
| | Sign in to GitHub/Enterprise (OAuth, no separate credential manager) | ✅ | ❌ (system git only) |
| **Multi-repo** | Repo list / recents, multiple windows | ✅ | ✅ sidebar |
| | Side-by-side panes (1–3) | ❌ | ⚠️ advertised in README/shortcuts/palette, **only pane 0 is rendered** (`src/App.svelte:139`) |
| | Branch "views" without checking out | ❌ (must checkout/worktree) | ✅ (unique) |
| | Worktree management | ✅ (new in GH Desktop) | ❌ |
| **Review** | Combined diff of everything since last look | ❌ | ⚠️ backend supports range; no visible "needs review" header / mark-reviewed button; marker auto-set to HEAD on add (`src-tauri/src/cmd.rs:116`) |
| | Per-repo "new commits" / dirty / error badges | ✅ | ❌ sidebar row renders none (`src/lib/components/Sidebar/RepoRow.svelte:220+`) |
| | Triage flags (deps, deletions, secrets, removed tests) | ❌ | ⚠️ computed in Rust + `api.commitFlags`, never called by UI (`src-tauri/src/triage.rs`, `src/lib/api.ts:94`) |
| | Review notes / anchors | ❌ | ⚠️ line selection + copy ref/snippet only; `noteAdd`/`NotesPanel` have no UI |
| | Blame + inline author/commit | ❌ | ✅ (unique) |
| | Heuristic "peek definition" | ❌ | ✅ (unique) |
| | Read-only code browser (no accidental edits) | ❌ | ✅ (unique) |
| | Embedded persistent terminal per repo | ❌ | ✅ (unique) |
| **Diff** | Unified diff | ✅ | ✅ |
| | Split diff | ✅ | ❌ `diffMode` is passed to `DiffView` but never used (`src/lib/components/Diff/DiffView.svelte:24`) |
| | Hide whitespace | ✅ | ✅ |
| | Expand context / expand whole file | ✅ | ⚠️ context expansion only; no "expand whole file" |
| | Image / binary diff rendering | ✅ | ❌ binary shows "No changes in this file" |
| | Per-line staging (partial commit) | ✅ | ❌ file-level only |
| | Per-line discard | ✅ (with Trash recovery) | ❌ file-level only |
| **Commit** | File-level partial commit | ✅ | ✅ |
| | Line-level partial commit | ✅ | ❌ |
| | Co-authors | ✅ | ❌ |
| | Amend last commit | ✅ | ❌ |
| | Undo commit / reset to commit | ✅ | ⚠️ `reset_to` backend + `ResetDialog` exist, **no trigger in UI** |
| | Revert commit / revert range | ✅ | ⚠️ backend + `RevertDialog` exist, **no trigger in UI** |
| | Cherry-pick | ✅ | ❌ |
| | Reorder / squash commits | ✅ | ❌ |
| | Tags (create, view, push, delete) | ✅ | ❌ |
| | Bypass commit hooks | ✅ | ❌ (hooks always run — arguably fine) |
| | Ruleset/protected-branch warnings before commit | ✅ | ❌ |
| | AI commit message | ✅ Copilot (or your own LLM provider) | ✅ any local command (`commitMessageCmd`) |
| **Branches** | Create / rename / delete | ✅ | ❌ (only view existing branches, checkout) |
| | Publish branch / upstream-aware push | ✅ | ✅ publish on push when no upstream; origin-only |
| | Switch with dirty tree (stash / bring changes) | ✅ | ⚠️ no stash; checkout likely blocked/dirty |
| | Merge / rebase UI | ❌ (both rely on CLI/PR) | ❌ parity |
| | Conflict resolution flow (banner, continue/abort) | ✅ | ❌ status shows `conflicted` only |
| | Checkout a previous commit | ✅ | ⚠️ read-only commit selection |
| **Remote / GitHub** | Fetch / pull / push | ✅ | ✅ |
| | Pull request / issue creation, view PR, check status | ✅ | ❌ (explicit non-goal) |
| | CI checks + re-run, notifications | ✅ | ❌ (explicit non-goal) |
| **Safety / recovery** | Backup before discard/reset, restore UI | ⚠️ Trash for discards | ✅ stronger (`refs/vantage/backup/*`, Recovery pane) |
| | Unreachable destructive actions | — | ❌ revert/reset/discard dialogs mounted but nothing opens them (grep `openDialog(` → only `remove_repo`/`change_icon`) |
| **Settings** | Git identity (name/email), default branch | ✅ | ❌ |
| | Editor/shell pickers (detected) | ✅ | ⚠️ raw command/path strings |
| | Prompts toggles, Advanced, Accessibility panes | ✅ | ⚠️ General/Git/Terminal/Integrations/Recovery/About |
| | Auto-fetch honored | ✅ | ❌ `autoFetchMinutes` has no scheduler consumer |
| | Terminal font size honored | ✅ | ❌ setting exists, terminal hardcodes it |
| **Platform / polish** | Native app menus | ✅ | ❌ custom header only |
| | Auto-update | ✅ | ✅ |
| | Telemetry | opt-out | ✅ none (advantage) |
| | Linux support | ❌ (Win/macOS only) | ✅ (advantage) |
| | Every advertised keyboard shortcut works | ✅ | ❌ Help lists `j/k`, `{`, `}`, `r`, `c`, `]`, `[`, `n`, `p`, `e`, `w`, `/`, pane keys; only `Mod+K/O/B/,/?` are wired |
| | Loading / empty / error states | ✅ | ⚠️ mostly absent; failures log to console |
| | Modal focus trap + Esc + restore focus | ✅ | ⚠️ Escape handled inconsistently |
| **Branding** | Consistent product name in UI | — | ❌ UI still says "Relay" in several dialogs/menus |

---

## 3. Where Vantage is already ahead — protect these

1. **Cross-repo review marker + combined range** — nothing in GitHub Desktop answers "what changed in all my repos since I last looked." This is the whole product; finish it before adding anything else.
2. **Triage flags** — heuristic "look here first" signals for commits. GitHub Desktop has no equivalent.
3. **Embedded per-repo terminal + read-only code viewer + blame + definition peek** — removes the VS Code window juggling that is the stated pain.
4. **Safety backups** with one-click restore for discard/reset (GitHub Desktop only offers OS Trash for discards).
5. **Any-CLI AI commit message** (works offline, any provider) vs GitHub Desktop's Copilot-centric flow.
6. **System git, zero telemetry, small Tauri binary, Linux support.**
7. **Branch views without checkout** — safe multi-branch browsing in one window.

---

## 4. Improvement backlog

Priorities: **P0** = the review loop is currently not usable/credible; **P1** = blocks daily authoring parity; **P2** = polish/quality.

### P0 — Finish the core review loop (the differentiation)

1. **Render multi-pane or remove the claims.** `src/App.svelte` mounts only `paneIndex={0}`, while `panes.ts`, the palette (`cmd_split_pane`, `cmd_focus_pane_1..3`), `keys.ts`, and the README all promise 1–3 panes. Either render `$panes.slice(0, $paneCount)` with per-pane width/focus, or delete the multi-pane surface. *Acceptance:* `Mod+\` visibly shows 2–3 panes, each with its own repo/tab/terminal.
2. **Surface review state in the sidebar.** Render `+N` new commits, dirty dot, error icon, and unpushed count per repo row (data already in `RepoStatus`). *Acceptance:* the sidebar alone tells you which repo needs attention, matching the README mock.
3. **Add the "Needs review" range header.** Mount a header for `{ from: marker, to: HEAD }` showing commit/file counts, `+/−`, and **Mark reviewed** + **Undo**; handle `MarkerState::Missing | Diverged | None`. Today the range is only auto-selected once and `r` exists only in the palette. *Acceptance:* README's core loop (glance → open → skim → mark reviewed) is performable entirely from the visible UI.
4. **Render commit triage flags.** Call `api.commitFlags` for the loaded page and show flags on `CommitRow` and the range header (`{`/`}` jump keys expect them). *Acceptance:* a commit touching lockfiles/secrets/tests is visibly flagged in history.
5. **Give destructive actions entry points.** `RevertDialog`, `ResetDialog`, `DiscardDialog` are mounted in `App.svelte` but nothing opens them. Add a commit-row context menu (Revert, Reset to here, Copy SHA, Create branch from commit) and a file-row/diff action (Discard changes) wired to `openDialog`. *Acceptance:* revert/discard/reset reachable by mouse and keyboard, with backup toast + Recovery restore.
6. **History paging + search.** `logPage(repo.id, 0, 200)` is a hard first page with no search/filter. Add keyset pagination and an inline filter (message/author/path). *Acceptance:* find and review a commit older than the first 200.
7. **Selection/loading correctness.** Requests use generation guards but keep showing stale content and never show a loading/pending state (`Pane.svelte` diff/history/file loads). Add skeletons, clear stale diff on scope change, and keep selection stable across refresh (see also `docs/PRODUCTION_REVIEW_2026-10-16.md` R11–R13).

### P1 — Minimum authoring parity to keep the loop unblocked

8. **Clone repository (URL + local path).** Even review-first users occasionally need a repo. `git clone` + register the result. Optionally accept `owner/repo` against GitHub for convenience. (Create-new-repo and auth/PR flows can stay non-goals.)
9. **Branch create / rename / delete + publish.** `branch_list`/`checkout` exist; add `git branch`/`-m`/`-d` and a "New branch from <commit|HEAD>" action. Publishing already works via push-without-upstream.
10. **Stash + dirty-branch switch handling.** `git stash push/pop/list` and a switch dialog offering leave/bring/stash. Without this, checking out a branch with a dirty tree silently dead-ends.
11. **Amend last commit + undo last commit.** High-value, low-risk: `commit --amend`, and `reset --soft HEAD~1`. Undo commit is GitHub Desktop's most-used "oops" affordance.
12. **Co-authors.** `Co-authored-by:` trailer from a small picker (recent contributors from `git log`).
13. **Conflict awareness.** Detect merge-in-progress / `conflicted` entries; show a banner with "Open in editor" / "Continue"/"Abort" and block commit until resolved. GH Desktop's flow is the reference.
14. **Tag create/view.** Right-click commit → Create tag; show tags in commit metadata; push tag on push.
15. **Split diff + image/binary diff.** `diffMode` is plumbed but unrendered; binary files currently read as "No changes," which is misleading in review.
16. **Whole-file expand in diff.** One action to expand all context (GH Desktop: right-click → Expand Whole File).
17. **Line-level partial commit and discard.** The biggest authoring gap vs GitHub Desktop. Gate behind the existing hunk/line model; reuse the line selection already built for copy-reference.

### P2 — Quality, settings, platform

18. **Implement every advertised shortcut** with a context-aware registry (dialog/terminal ownership), and stop intercepting keys inside inputs. Share command IDs/handlers between `keys.ts`, palette, menus, and Help so labels can't diverge (`c` currently means two things).
19. **Settings parity:** Git identity (name/email), default branch, detected editor/shell pickers, tab size, prompt toggles; make `autoFetchMinutes` and `terminalFontSize` actually take effect or remove them.
20. **Native menu bar** (macOS/Windows) plus window restore; a menu is also the discovery path for clone/branch/tag actions.
21. **Accessibility pass:** focus trap + Esc + focus restore in `Modal`, labeled checkboxes/inputs, `aria-current` on nav, splitter keyboard semantics, contrast fixes for `--text-faint` metadata, `prefers-reduced-motion`, and allow selecting metadata text.
22. **Branding cleanup:** UI strings still say "Relay" (`RemoveRepoDialog`, `RepoRow`, palette). Fix app-name consistency.
23. **Worktrees (optional):** GitHub Desktop now has a worktree UI; Vantage's "branch views" solve a related need more cheaply, so treat as later, not parity.
24. **Localization / notifications / LFS:** low priority for the solo-dev audience; skip unless scope changes.

---

## 5. Deliberate non-goals (don't copy just because GH Desktop has it)

- GitHub account/OAuth, PR & issue creation, CI checks, notifications — Vantage's edge is local, telemetry-free review. If PR visibility is ever needed, "Open commit on GitHub" (a URL) is the cheap 90% without auth.
- Merge/rebase/squash/reorder UI — GitHub Desktop doesn't ship this either; deep history rewrites belong in the terminal, which Vantage already embeds.
- Copilot-style cloud AI lock-in — keep the bring-your-own-command model.
- Multiple windows — panes are the better answer for this product; fix panes instead of adding windows.

---

## 6. Suggested sequence

1. **Milestone A (credibility):** items 1–5 — panes, sidebar badges, needs-review header + mark reviewed, triage flags, destructive-action entry points.
2. **Milestone B (usable daily):** items 6–7 plus 8–11 (clone, branch CRUD, stash, amend/undo) — the minimum to stop reaching for GitHub Desktop at all.
3. **Milestone C (review depth):** 12–17 — co-authors, conflicts, tags, split/image diff, line staging.
4. **Milestone D (product quality):** 18–23 — shortcuts, settings, native menus, a11y, branding.

Measure success by the README's core loop, not by feature count: *glance at all repos → review combined diff → flag/triage → mark reviewed → move on*, without opening another window.
