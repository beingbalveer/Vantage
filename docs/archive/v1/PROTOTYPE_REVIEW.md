# Vantage prototype review and improvement backlog

Reviewed: 15 October 2026. Baseline: `7fc5b04`.

## Product assessment

Vantage already has the right ingredients for a Git client used alongside coding agents: a persistent repository sidebar, branch views, history, combined review ranges, diffs, a read-only file browser, terminal sessions, and review-marker storage. The existing Git/backend work is a useful foundation. Evaluate the UI technology against the macOS experience requirement below before committing to a visual redesign; a complete rewrite has not been established as necessary.

The main gap is that several capabilities exist in the specification or backend without a complete user workflow. The highest priority is making displayed context trustworthy: which repository, branch, working directory, commit range, and selected files an action actually affects. After that, improve navigation and complete the review-to-agent loop.

Suggested product promise: **Quickly move between projects and branches, understand what agents changed, and give precise feedback without opening an IDE.**

## Confirmed platform and visual requirements

The user clarified that Vantage is **macOS only**. Its interface must feel refined, polished, clean, and native to macOS, with Codex and ChatGPT as visual quality references. This is a core product requirement, not an optional final polish pass. GitHub Desktop remains a reference for understandable Git workflows; the visual direction is a premium Mac application.

- Design the window, title bar, toolbar, sidebar, tabs, lists, menus, sheets, and Settings as one coherent Mac application. Avoid a separate web-style application header beneath otherwise disconnected window chrome.
- Use system typography, carefully tuned density and spacing, consistent Mac-appropriate symbols, restrained color, subtle separators, and clear selection/focus states. Give code a legible monospace treatment. Replace mixed emoji/text-glyph controls and heavy boxed toolbars.
- Use translucency/materials selectively where they improve hierarchy; keep code and diff surfaces readable. Support light/dark appearance, inactive-window states, increased contrast, reduced transparency, and reduced motion.
- Treat behavior as part of the native experience: standard Command shortcuts and menu-bar actions, keyboard navigation, contextual menus, focus restoration, native file/folder selection, appropriate confirmation sheets, and predictable window resizing/full screen.
- Make first launch, loading, empty, error, and recovery states as considered as the populated review screen. Judge the result on a real Mac at normal and compact window widths, including split panes and long repository/branch names.
- Target macOS development, packaging, testing, and distribution. Windows/Linux compatibility is outside the product scope and should not constrain design choices.

The requirement specifies the experience, not a mandatory SwiftUI rewrite. Evaluate whether the existing Tauri shell can meet the window/control/interaction bar, where native integration is needed, and whether SwiftUI/AppKit would materially improve the result. Preserve useful Git/backend logic if the UI layer changes. A dark theme, rounded corners, or blur alone does not satisfy the requirement.

## Review scope and evidence

- Built and opened the native macOS app using its existing saved repository registrations.
- Inspected a dirty working directory and an existing non-checked-out branch view; exercised single and split panes. The split-pane layout visibly squeezed diffs into narrow strips and wrapped the toolbar.
- Read the frontend stores/components, Git command handlers, watcher, persistence, terminal integration, and relevant tests. Used the codebase-memory index and checked coverage, with source searches for missing connections.
- Confirmed the selected-file commit issue using an isolated temporary Git repository. No commit, checkout, push, discard, reset, or review-marker action was performed on the user's project repositories.
- `npm run check`: zero errors and warnings. `npm test`: 19 tests passed. Rust tests: 13 passed. Native debug app bundle built successfully.
- The remaining runtime failure scenarios below are source-derived unless explicitly described as observed or reproduced. Passing existing tests does not validate these untested workflows. Performance budgets were not benchmarked.

## Recommended interaction model

Use a focused repository/worktree tab as the primary working surface, with the persistent sidebar for all projects. Keep split panes optional for deliberate comparison.

Three plausible approaches are: (1) focused tabs with optional splitting, (2) the current split-first approach, and (3) a cross-project activity dashboard as the main screen. Tabs best support frequent context switching while preserving enough width for review. Split-first repeats too much navigation in every pane. A dashboard could later help triage but should not delay reliable Git and review workflows.

Within each project, distinguish a **branch view** (browse a ref) from a **worktree** (a directory where a branch is checked out and commands can run). Git worktrees allow multiple checked-out branches with separate working directories and indexes; see the [Git documentation](https://git-scm.com/docs/git-worktree). This is a proposed direction, not an assumption that the user has selected a worktree-first workflow.

Retain the familiar Changes/History distinction, clear current-branch context, file selection, and a large diff surface. Keep read-only file browsing and terminals convenient. Agent execution can remain in external tools or the terminal; a built-in agent platform is a later decision.

## Prioritized backlog

P0 = correctness or review trust. P1 = necessary for a useful daily client. P2 = polish and expansion after the core loop works. S/M/L estimates are relative scope, not delivery commitments.

### P0 — Make context and actions trustworthy

**R01 — Separate branch browsing from working-directory operations. Size L. Evidence: live UI and source.**

The development branch view displayed “Publish branch” even though the working directory was on another branch. `query_repo_status` sets upstream to `None` for a non-checked-out view; the header interprets this as an unpublished branch. Push/pull and terminal handlers resolve the entry's directory, not its displayed ref. Publishing can therefore operate on the checked-out branch while the screen names another branch.

Improve: show explicit “Viewing branch” versus “Checked out in …” context. Gate working-directory writes in browsing views on both frontend and backend. Discover existing worktrees and offer a deliberate route to open or create one. A checkout in a shared directory must identify the affected sessions.

Done when: opening branch A while B is checked out cannot silently publish, pull, commit, or run branch-specific work against B. Terminal context identifies its actual directory and checked-out branch. Existing worktrees are distinguished from additional branch views.

Sources: [status.rs:196](../src-tauri/src/git/status.rs:196), [PaneHeader.svelte:118](../src/lib/components/Pane/PaneHeader.svelte:118), [cmd.rs:801](../src-tauri/src/cmd.rs:801).

**R02 — Commit exactly the selected content and protect multi-step writes. Size M. Evidence: reproduced Git command sequence and source.**

The commit implementation stages selected paths, then invokes an unrestricted `git commit`. An already-staged unchecked file is included too. In a temporary repository, pre-staging `unselected.txt`, then selecting/staging `selected.txt` and using the current commit sequence committed both files. Staging a selected path also includes its current unstaged changes. The write mutex covers each Git process separately, allowing another app operation between staging and committing.

Improve: define selection semantics, preserve unrelated staged work, and hold an app operation lock through the complete write transaction. Revalidate branch/index/content where external agents may change them; the app mutex cannot lock out external tools.

Done when: unchecked pre-staged changes stay out of the commit; partial staging behaves as advertised; concurrent app actions cannot switch context mid-commit; hook/signing failures preserve recoverable staging state.

Sources: [ops.rs:23](../src-tauri/src/git/ops.rs:23), [runner.rs:66](../src-tauri/src/git/runner.rs:66).

**R03 — Keep the file list, diff, and file viewer on the same revision. Size M. Evidence: source.**

The initial combined range targets `repo.branch || HEAD`, but selecting another file reconstructs the range with `to: HEAD`. On a branch view, the file list and selected diff can therefore describe different branches. “View file” ignores its path and opens a browser that selects its own first file at the branch/HEAD revision, rather than the selected historical commit. Asynchronous file/diff requests also have no stale-response protection.

Improve: keep an explicit immutable scope and resolved revision for the current review. Pass file, revision, side, and line to the viewer. Apply request-generation checks and show loading/error states without leaving an old diff under a new filename.

Done when: every file in a combined branch review uses the same endpoints; viewing a historical file opens that exact file/revision; rapid selections never pair one filename with another file's diff.

Source: [Pane.svelte:87](../src/lib/components/Pane/Pane.svelte:87), [Pane.svelte:131](../src/lib/components/Pane/Pane.svelte:131), [Pane.svelte:288](../src/lib/components/Pane/Pane.svelte:288).

**R04 — Mark only the snapshot actually reviewed. Size M. Evidence: source.**

“Mark reviewed” resolves the branch tip at click time. An agent can commit after the displayed range loaded, causing unseen work to be marked reviewed. The frontend does not reload the persisted marker/status after this action, and watcher callbacks capture the marker supplied when the watcher was created. Missing/diverged marker warnings are inside a component only mounted when the new commit count is positive; those error states can have a zero count and remain hidden.

Improve: record the displayed end SHA, mark that SHA, leave newer commits pending, refresh marker state, and render recovery states independently of counts. Allow undo of the last review action.

Done when: a commit arriving during review remains unreviewed; badges update immediately; rewritten history produces a visible recovery choice instead of silently appearing reviewed.

Sources: [cmd.rs:358](../src-tauri/src/cmd.rs:358), [Pane.svelte:160](../src/lib/components/Pane/Pane.svelte:160), [Pane.svelte:213](../src/lib/components/Pane/Pane.svelte:213), [watch.rs:59](../src-tauri/src/watch.rs:59).

### P1 — Complete the daily workflow

**R17 — Establish and deliver a native macOS design system. Size L. Confirmed user requirement.**

The current custom toolbar, mixed glyph/emoji icons, repeated boxed controls, and crowded split-pane headers need a coherent macOS redesign. Use Codex and ChatGPT as references for visual restraint, hierarchy, and overall finish while retaining Git-specific clarity. Coordinate this with R06 (layout), R07 (state), R12 (navigation), and R14 (interaction quality).

Done when: the window chrome and content feel integrated; typography, symbols, controls, spacing, materials, and selection states are consistent; single/split views remain clean at supported sizes; menu-bar and keyboard behavior are complete; light/dark and accessibility appearances are reviewed on macOS. Produce a concrete visual design before implementing the UI overhaul. Decide the UI framework from a focused technical assessment of these requirements.

**R05 — Refresh reliably when agents edit, commit, or switch branches. Size L. Evidence: source.**

Watchers are registered when adding a repository, but saved repositories are loaded without registering them at startup. The watcher assumes `.git` is a directory, watches Git metadata rather than ordinary file edits, and its polling flag has no polling loop. Frontend head/status listeners update status but do not reload an open pane's history, file list, or diff. The debounce delays each event independently rather than coalescing a burst.

Done when: after restart, external edits, new/untracked files, commits, and checkouts refresh the correct view without reselection. Resolve actual Git/common directories for linked worktrees; refresh on focus; coalesce events and retain selection while updating.

Sources: [cmd.rs:39](../src-tauri/src/cmd.rs:39), [watch.rs:35](../src-tauri/src/watch.rs:35), [repos.ts:61](../src/lib/stores/repos.ts:61), [Pane.svelte:47](../src/lib/components/Pane/Pane.svelte:47).

**R06 — Give diffs enough space, especially with multiple projects. Size L. Evidence: observed live.**

Each pane reserves 300 px for commits and 240 px for files. At the default 1440 px window, the 232 px sidebar leaves roughly 604 px per pane in a two-pane layout: very little remains for the diff. The live toolbar wrapped heavily and code became a narrow strip.

Done when: focused tabs preserve a generous diff area; history/files can collapse or share a navigation area; separators resize; long branch names truncate with tooltips; two/three-pane modes use a compact layout at supported widths. Add a visible layout control so splits are discoverable without shortcuts.

Source: [Pane.svelte:326](../src/lib/components/Pane/Pane.svelte:326).

**R07 — Preserve each project's workspace and review position. Size M. Evidence: source.**

Changing repositories recreates the pane through the keyed component in `App.svelte`, losing local selected commit/file, scroll position, and commit-message draft. Pane stores keep only a small subset of view state. Layout get/set endpoints exist but are not connected to frontend restoration/persistence.

Done when: A → B → A restores the selected commit/file, scroll, Changes/History/Files mode, and draft; restart restores tabs, active repository, pane arrangement, and dimensions. Terminal-process persistence remains separate from UI persistence.

Sources: [App.svelte:120](../src/App.svelte:120), [panes.ts:5](../src/lib/stores/panes.ts:5), [api.ts:269](../src/lib/api.ts:269).

**R08 — Finish the review tools already promised by the interface. Size L. Evidence: source.**

The diff component always renders a unified list; the split setting only changes its class. Context expansion is declared but unused. “Load diff anyway” invokes the normal request with `force: false` again. Diff syntax-highlighting imports are unused. The virtualization helper has tests but is not connected to the rendered diff. These gaps directly affect reviewing code without an IDE.

Done when: split/unified modes visibly work; context expands; forced loading succeeds; search and next-file/hunk navigation work; large diffs stay responsive through rendering limits/virtualization; binary and truncated files have honest states.

Sources: [DiffView.svelte](../src/lib/components/Diff/DiffView.svelte:1), [Pane.svelte:145](../src/lib/components/Pane/Pane.svelte:145), [format.ts:77](../src/lib/format.ts:77).

**R09 — Complete inline feedback and agent handoff. Size M. Evidence: source.**

The notes panel tells users to select diff lines and add a note, but the diff has no line-selection/note-creation UI and no frontend caller of `noteAdd`. Export exists, so the input half of the feedback loop is missing. Sending notes opens the terminal then immediately pastes, without waiting for terminal creation.

Done when: select added/deleted lines → add/edit note → copy a useful prompt with repository, branch, revision, path, line side, and snippet. Notes survive restart and can be resolved. Terminal paste waits for the intended session and leaves submission under user control.

Sources: [NotesPanel.svelte](../src/lib/components/Notes/NotesPanel.svelte:1), [DiffView.svelte](../src/lib/components/Diff/DiffView.svelte:1), [api.ts:248](../src/lib/api.ts:248).

**R10 — Use Git metadata for file status and rename identity. Size M. Evidence: live UI and source.**

`parse_numstat_z` infers Added/Deleted from line counts. A modification containing only insertions is consequently labeled Added; this was visible on an existing file in the development view. Rename parsing also does not consume Git's empty-path/old-path/new-path numstat record correctly.

Done when: file statuses come from name/status metadata, additions/deletions remain separate metrics, and renames preserve both paths. Cover insertion-only edits, deletion-only edits, renames, binary files, mode changes, and unusual filenames with temporary-repository fixtures.

Source: [diff.rs:8](../src-tauri/src/git/diff.rs:8).

**R11 — Complete basic Git workflows with visible recovery. Size L; split into smaller tickets. Evidence: source and feature inventory.**

The app can add existing repositories, check out local branches, and commit/fetch/pull/push. Branch creation and remote-branch discovery, stash-assisted switching, clone/init, conflict guidance, and a complete discard/revert/recovery UI need further work. Several recovery/write endpoints exist without user-facing callers; Settings has no Recovery section despite the README advertising one.

Done when: users can perform the normal daily loop and understand dirty-checkout, auth, detached-HEAD, divergence, merge-conflict, and hook failures. Destructive operations name the exact target and expose recovery. Keep fetch, push, pull, and publish understandable; a diverged “Sync” button currently falls back to fetch.

Sources: [PaneHeader.svelte:118](../src/lib/components/Pane/PaneHeader.svelte:118), [SettingsDialog.svelte:6](../src/lib/components/Dialogs/SettingsDialog.svelte:6), [api.ts:162](../src/lib/api.ts:162).

### P2 — Extend multi-project navigation and attention workflows

Core native appearance and interaction quality are P1 requirements under R17; this section covers additional navigation and workflow capabilities.

**R12 — Group repositories, branches, and worktrees clearly. Size M. Proposed product improvement.**

The live sidebar repeats Point-App as multiple unrelated rows, duplicates branch labels, and truncates the repository name. Group branch/worktree children under their repository; support stable pinned/manual order, always-available filtering, recent projects, and distinct badges for dirty changes, unreviewed commits, errors, and ahead/behind. Avoid automatic list movement interrupting a click.

Done when: users can find a specific project and branch in a 20-project workspace and distinguish identical folder names by path/owner. Preserve consistent repository identity/color across its branch views.

**R13 — Make advertised navigation work end to end. Size M. Evidence: source.**

The palette recognizes `/`, `#`, and `@` modes but only builds command/repository results. Many shortcuts in `keys.ts` and Help have no corresponding action handlers, including several review/navigation shortcuts. Triage evaluation endpoints exist but are not requested by the frontend, so advisory flags are not fully connected to review.

Done when: every visible command, search mode, and help shortcut works in the focused context, with no interception of terminal typing or dialog input. Either connect a capability or stop advertising it until ready.

Sources: [CommandPalette.svelte:15](../src/lib/components/Palette/CommandPalette.svelte:15), [keys.ts](../src/lib/keys.ts:1), [App.svelte:57](../src/App.svelte:57).

**R14 — Tighten interaction quality and error handling. Size M. Evidence: source and live UI.**

The close-pane handler only decrements the count, so closing the first/middle pane actually hides the last pane and can leave focus pointing at a hidden pane. The global footer says “Ready” regardless of activity. Several failed loads only log to the console. File/branch labels compete for width; icons mix text glyphs and emoji.

Done when: the clicked pane closes; focus moves predictably; loading/error/empty states differ; retries are visible; dialogs support Escape, focus trapping/restoration, and accessible names; consistent icons and spacing improve scanability.

Sources: [PaneHeader.svelte:142](../src/lib/components/Pane/PaneHeader.svelte:142), [App.svelte:29](../src/App.svelte:29), [repos.ts:28](../src/lib/stores/repos.ts:28).

**R15 — Add cross-project attention and lightweight agent context. Size M/L. Proposed product improvement.**

After refresh and markers are reliable, offer filters such as “Needs review”, “Uncommitted”, and “Errors”, plus optional labels tying worktrees/terminals to agent tasks. Do not infer agent running/completed status from commit timestamps; integrate a real status source or let users label it manually.

Done when: users can identify the next project needing attention and open its existing review context. Add PR/check status later if it materially reduces trips to the browser; start with useful links before building a full hosting integration.

**R16 — Replace feature/performance claims with measured release evidence. Size M. Evidence: documentation and current tests.**

The README and performance document present several unfinished paths as complete, including inline notes, keyboard navigation, recovery UI, persistent view state, polling, and virtualized diffs. Current frontend tests cover helpers rather than mounted component workflows. A Rust diff test depends on a hard-coded commit in this checkout.

Done when: maintain an implemented/partial/planned capability matrix, use disposable repository fixtures, and add focused end-to-end scenarios for R01–R11. Record measured startup, idle CPU/RAM, large-diff rendering, terminal replay, and many-repository refresh results on named builds/hardware. Treat unmeasured budgets as targets.

Sources: [README.md](../README.md:1), [perf.md](../docs/perf.md:1), [diff.rs:477](../src-tauri/src/git/diff.rs:477), [tests](../tests).

## Suggested delivery sequence

1. **Trust:** R01–R05 and R10. Validate wrong-branch protection, selected-file commits, stable review snapshots, and external agent changes before expanding functionality.
2. **Mac experience and daily usability:** R17 and R06–R09; establish the native macOS visual direction and deliver a focused tab, state restoration, a complete diff viewer, and inline notes → prompt handoff. Start the visual design alongside correctness work; native quality is part of this milestone.
3. **Git completeness:** break R11 into small workflows and ship by frequency of use. Include recovery before exposing more destructive actions.
4. **Scale and polish:** R12–R15. Apply R16 throughout, with release measurements after the core workflows stabilize.

First milestone: a polished native macOS workspace where two projects and two branches remain easy to identify; an agent can change files while Vantage is open; the correct diff refreshes; a review note becomes a usable prompt; switching away and back preserves context; committing cannot include an unchecked file or affect the wrong branch.

Defer a full editor, language-server stack, built-in agent orchestration, complex branch graph, plugin platform, and elaborate dashboards until the first milestone works reliably. These are scope recommendations, not conclusions that those features can never be useful.
