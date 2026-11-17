# Vantage review — engineering TODO

Source: full project review on 2026-11-16 (code, UI, performance, features, ease of use).
Status: all 38 tickets are implemented on branches. 34 projects are integrated into `main`
(safety, security, correctness, docs, tests, performance, UX and refactors); the remaining four
feature branches (`P1-01`, `P2-03`, `P2-08`, `P2-13`) are awaiting review. See
[`REVIEW_BRANCH_SUMMARY.md`](REVIEW_BRANCH_SUMMARY.md) for the integrated set, the left-out
branches, commits, verification and conflict notes. Check a box only when the acceptance criteria
and its tests are in place on that ticket branch.

## Priority legend

| Priority | Meaning |
|---|---|
| **P0** | Data-safety or wrong-target issue. Fix before recommending daily use. |
| **P1** | Blocks a dependable beta: trust, discovery, core performance, accessibility. |
| **P2** | Important product/quality gap or platform/release issue. |
| **P3** | Maintenance, cleanup and polish. |

Effort: **S** = ≤ half a day, **M** = 1–2 days, **L** = 3–5 days, **XL** = more than a week.

**At a glance:** 38 items — 6 P0, 10 P1, 13 P2, 9 P3.

---

## Release gates

- [ ] **Beta gate:** every P0 task is closed, with a regression test for its invariant.
- [ ] **Daily-use gate:** P0 + P1 are closed; `cargo fmt`, `cargo clippy -- -D warnings`,
      `cargo test --workspace` and the macOS package job are green.
- [ ] **1.0 gate:** P0 + P1 + P2 are closed; README claims match the shipped UI;
      release artifacts are signed/notarized and the cask no longer bypasses Gatekeeper.

---

## P0 — Data safety and correct targeting

### [x] P0-06 — Correct reset semantics and confirmation copy
**Area:** data safety / Git · **Effort:** M
**Problem:** the reset confirmation says “Your files are not touched, so the difference appears as uncommitted changes”, but `ops::reset_to` runs `git reset --hard` and refuses a dirty tree. A confirmed reset can therefore change the working tree while the dialog promises it won’t.
**Do:** choose one contract and enforce it. Prefer a soft/mixed reset that leaves the index/worktree untouched (and update the copy to say what happens), or keep a hard reset and rewrite the dialog to say files will be discarded, with an explicit confirmation. Add a backup/refusal test for both clean and dirty trees.
**Files:** `crates/vantage-core/src/git/ops.rs`, `crates/vantage/src/backend/mod.rs`, `crates/vantage/src/app.rs`.
**Acceptance:** the confirmation copy exactly describes the operation; a clean-tree reset does not silently modify files when the copy says it won’t; dirty-tree behavior is explicit and covered by a test.

### [x] P0-01 — Scope the commit composer and amend mode to a repository
**Area:** safety / UI state · **Effort:** M
**Problem:** `commit_field`, `commit_body` and `amending` are app-global. Selecting a different
repository keeps the message and the amend flag, so a message typed for `alpha` can be committed
to `beta`, or an amend started on `alpha` can rewrite `beta`'s HEAD.
**Do:**
- Introduce a `ComposerState` keyed by repository id: message, body, amend flag, base SHA.
- On repository change, persist a draft per repo or clear deliberately; never carry amend mode.
- Capture the repository id and expected HEAD in the commit job; refuse if either moved.
- Disable the commit bar while the selection is in transition.
**Files:** `crates/vantage/src/app.rs` (`ComposerState`, `select_repo`, `commit_selected`),
`crates/vantage/src/views/inspector.rs`.
**Acceptance:** switching repositories with a typed message cannot commit it elsewhere; amend
mode never survives a repository change; tests cover both normal and amend paths.

### [x] P0-02 — Pin branch prompts to the repository that opened them
**Area:** safety / targeting · **Effort:** S
**Problem:** `BranchPrompt` stores only the action; `submit_branch_prompt` uses whichever
repository is selected at submit time. A delete/create/rename can run against the wrong repo.
**Do:** store `repo_id` inside every `BranchPrompt` variant, submit against that id, and re-check
the branch still exists before the write.
**Files:** `crates/vantage/src/app.rs` (`BranchPrompt`, `start_branch_prompt`,
`submit_branch_prompt`, context-menu handlers).
**Acceptance:** opening a prompt, changing selection, then submitting acts on the original repo
or refuses with a clear error; test covers this.

### [x] P0-03 — Preserve commit descriptions on amend and support clean-tree amend
**Area:** data safety / commit flow · **Effort:** M
**Problem:** amend pre-fills only the subject; the Description field is ignored; `git commit
--amend -m subject` deletes the original body. The commit bar is also hidden when the working
tree is clean, so “Amend commit” cannot be submitted.
**Do:**
- Load and pre-fill the full commit message (subject and body), and pass body through
  `Backend::amend` → `ops::amend_commit`.
- Render the commit bar whenever `amending` is true, even with an empty file list.
- Keep the original body when the user does not edit it.
**Files:** `crates/vantage/src/app.rs`, `crates/vantage/src/backend/mod.rs`,
`crates/vantage/src/views/inspector.rs`, `crates/vantage-core/src/git/ops.rs`.
**Acceptance:** amending a clean tree works; original body is preserved; a typed description
lands in the amended commit; tests cover both.

### [x] P0-04 — Make corrupt-state and save-failure handling visible and recoverable
**Area:** reliability / persistence · **Effort:** M
**Problem:** corrupt `state.json` is silently renamed and replaced with an empty workspace; the
load warning is discarded. Group persistence ignores write failures. Users can see success toasts
and lose the change at restart. State backups have no recovery UI.
**Do:**
- Surface the load warning at startup with the backup path and a “Restore”/“Open folder” action.
- Keep a last-known-good state and list recoverable backups in Settings → Data.
- Propagate `persist()`/`set_groups()` failures to a visible error toast.
- Consider an app-instance lock so two instances cannot clobber the same state.
**Files:** `crates/vantage-core/src/persist.rs`, `crates/vantage-core/src/repos.rs`,
`crates/vantage/src/backend/mod.rs`, `crates/vantage/src/app.rs`,
`crates/vantage/src/views/settings.rs`.
**Acceptance:** a malformed state opens with a visible recovery notice, not a silent empty
dashboard; a read-only state directory reports the failed save; tests cover both.

### [x] P0-05 — Add request identity to async commit/compare/stash loads
**Area:** correctness / async · **Effort:** M
**Problem:** commit-view, branch-comparison and stash-list results are applied to whatever is
open when the result returns. Rapid A→B opens can pair B's header with A's files; a closed stash
dialog can reopen.
**Do:** tag each request with `(kind, repo_id, sha/request_id)`, store an active request id, and
discard or present an error for stale results. Add explicit loading/error states.
**Files:** `crates/vantage/src/app.rs` (`open_commit_view`, `compare_with_branch`,
`open_stash_modal`).
**Acceptance:** opening A then B quickly always shows B's files; closing a dialog while loading
does not reopen it; tests simulate reversed completion order.

---

## P1 — Dependable daily-use beta

### [x] P1-01 — Add visible navigation for the three views
**Area:** UX / discovery · **Effort:** M
**Problem:** the sidebar contains only Groups and Settings. All Repositories, Needs Attention and
Pending Changes are reachable only from the macOS View menu or `Cmd+1/2/3`; the README claims a
sidebar count that does not exist.
**Do:** add a primary navigation block (or a segmented control beside the heading) with live
counts for Needs Attention and Pending Changes. Make the summary strip filters visibly related
to the views, and show the active destination.
**Files:** `crates/vantage/src/views/sidebar.rs`, `crates/vantage/src/views/topbar.rs`,
`crates/vantage/src/app.rs`, `crates/vantage/src/model.rs`.
**Acceptance:** every view is reachable with the mouse in one click; the active view and counts
are visible; no README claim about navigation is left unimplemented.

### [x] P1-02 — Watch Git metadata so external commits/fetches appear
**Area:** correctness / watcher · **Effort:** M
**Problem:** the app watcher ignores every path containing `.git`. An agent commit, fetch, push or
checkout performed in a terminal does not update the dashboard while Vantage is in the foreground.
**Do:** watch `.git/HEAD`, `.git/refs`, `.git/index`, `.git/packed-refs` (or reuse
`vantage-core/src/watch.rs`), debounce bursts, ignore `target/`, `node_modules/`, `dist/`,
`.venv/`, `vendor/`, and report watcher registration failures instead of `.ok()`.
**Files:** `crates/vantage/src/watcher.rs`, `crates/vantage-core/src/watch.rs`,
`crates/vantage/src/app.rs`, `crates/vantage-core/src/git/status.rs`.
**Acceptance:** committing from a terminal updates the row within ~1 s while Vantage is focused;
a fetch updates ahead/behind without a manual refresh; watcher-limit failures are visible.

### [x] P1-03 — Split overview scans from repository details and bound Git work
**Area:** performance / engine · **Effort:** L
**Problem:** each repository scan runs roughly 15–17 Git processes, serially, and loads branches,
commits, file diffs and identity for every row. Two unbounded full diffs and full commit/compare
patches are read into memory. Scans can overlap and queue behind each other.
**Do:**
- Add an overview snapshot that runs one `git status --porcelain=v2 --branch -z` per repo and
  gets activity/identity from cached or cheap config reads.
- Load details lazily when a repository is selected, and cache them by `(repo_id, HEAD)`.
- Map repositories in parallel with a bounded pool.
- Use `--numstat` first and load per-file patches on demand; cap patch bytes as well as preview
  lines.
- Gate dirty scans/auto-refresh while a scan is already running.
**Files:** `crates/vantage/src/backend/mod.rs`, `backend/repo_map.rs`, `backend/details.rs`,
`crates/vantage/src/app.rs`.
**Acceptance:** adding 3× the repositories does not triple overview latency; a clean repository
costs one status process; no unbounded patch is held in memory; a scan queue cannot grow during
a build.

### [x] P1-04 — Fix text-field clipping, scrolling and long-path overflow
**Area:** UI / input · **Effort:** M
**Problem:** the custom field paints the full nowrap value with no clipping or horizontal scroll.
Long paths paint outside the input and card; commit messages can paint over the Commit button.
**Do:** clip the field, implement horizontal scrolling/caret reveal, ellipsize where editing is
not active, and add a max width to tooltips/paths. Use the same treatment in the inspector path
line and any URL/branch fields.
**Files:** `crates/vantage/src/ui/input.rs`, `crates/vantage/src/app.rs` (`text_field`),
`crates/vantage/src/views/inspector.rs`, `crates/vantage/src/views/add_repo_modal.rs`.
**Acceptance:** a long `/private/var/...` path stays inside the field; the caret remains visible
at both ends; capture and Resize tests cover long text at 1040 px and 1470 px.

### [x] P1-05 — Make the minimum-width layout usable
**Area:** UI / responsive · **Effort:** M
**Problem:** at the 1040×640 minimum the summary strip wraps, the repository list is about
250 px wide, the inspector path runs under action buttons, and multiple status badges can force
a row to overflow.
**Do:** constrain/wrap badges, add a compact mode for the summary strip, reserve a minimum path
column, and ensure header actions stay visible. Decide whether the inspector should default
closed below a width threshold.
**Files:** `crates/vantage/src/views/repo_table.rs`, `views/inspector.rs`, `views/topbar.rs`,
`crates/vantage/src/app.rs`.
**Acceptance:** at 1040×640 all actions are reachable, no text paints outside a card, and a
repository with changes + ahead + behind badges does not overflow its row.

### [x] P1-06 — Scope keyboard bindings and finish modal focus management
**Area:** UX / input · **Effort:** M
**Problem:** `up`/`down`/`enter` are registered globally with no key context. GPUI dispatches
bindings before the focused div’s `on_key_down`, and the custom field does not stop propagation,
so list navigation can run while editing a field. Modals do not trap or restore focus; the New
Group field is not focused when revealed.
**Do:** give the repository list and any future file/commit list explicit key contexts; scope
bindings to them; stop propagation in fields; add a shared modal focus trap and restore focus on
close; focus the first field when a dialog opens.
**Files:** `crates/vantage/src/menu.rs`, `crates/vantage/src/app.rs`
(`handle_field_key`, `text_field`), `views/add_repo_modal.rs`, `views/settings.rs`,
`views/sidebar.rs`.
**Acceptance:** typing in a field never moves the selected repository; `Esc` closes every modal
from any control; Tab stays inside an open modal; closing restores the previous focus.

### [x] P1-07 — Raise accessibility: contrast, sizes, labels and hit targets
**Area:** accessibility · **Effort:** L
**Problem:** useful metadata is 10–11 px and uses `text_dim`, measured at 2.5–2.7:1 in both
themes. Checkboxes are 16×16, row menus 22×22, summary filters ~22 px high. Most clickable divs
have no role, accessible name or focus style; the custom input has no IME or mouse caret
placement.
**Do:**
- Add semantic text colours that meet 4.5:1; reserve `text_dim` for decoration.
- Raise the minimum type size for timestamps, branch names and paths.
- Bring hit targets to at least 32×32 (preferably 40×40 for primary actions).
- Add accessible names/roles/actions where GPUI supports them, focus rings, and tooltips for
  icon-only controls.
- Document or fix the input limitations (IME, undo/redo, mouse caret).
**Files:** `crates/vantage/src/theme.rs`, `ui/tokens.rs`, `ui/toggle.rs`, `ui/input.rs`,
`views/repo_table.rs`, `views/inspector.rs`.
**Acceptance:** measured contrast passes for active text; a keyboard-only user can reach and
activate the primary flows; a screen reader announces buttons, checkboxes and tabs; hit targets
meet the stated minimum.

### [x] P1-08 — Align README and product copy with the shipped app
**Area:** docs / trust · **Effort:** S
**Problem:** the README claims a per-row fetch action, per-branch ahead/behind counts, a sidebar
count, and that Vantage never edits files/amends/resets history. It also says hooks/signing apply
to every commit and that the add flow reads a stack, neither of which is true.
**Do:** update “What it does”, “Safety and privacy” and the interactions table. For every claim,
either implement it or remove it. Add a short “Current limitations” section covering root-commit
hooks/signing, amend body preservation, watcher coverage and cross-platform actions.
**Files:** `README.md`, `docs/ARCHITECTURE.md`, `crates/vantage/README.md`,
`views/add_repo_modal.rs` hint text.
**Acceptance:** every user-facing claim can be demonstrated on a release build; no safety text
describes a smaller write surface than the app actually has.

### [x] P1-09 — Make root commits run hooks and signing like normal commits
**Area:** Git correctness / safety · **Effort:** M
**Problem:** the unborn-HEAD path uses `commit-tree` + `update-ref`, bypassing `pre-commit`,
`commit-msg`, signing and other `git commit` behaviour, while the README says the user's hooks
and signing apply.
**Do:** perform the root commit through `git commit` with a temporary index (or explicitly run
the hook/signing path), and document any unavoidable exception.
**Files:** `crates/vantage-core/src/git/ops.rs` (`commit_with_author` root-commit branch).
**Acceptance:** an initial commit runs a `pre-commit` hook and a signed-commit configuration;
tests cover both; README text matches.

### [x] P1-10 — Add regression coverage for the safety invariants
**Area:** tests · **Effort:** M
**Do:** add tests for:
- composer/amend state across repository switches;
- branch prompt targeting after selection changes;
- amend body preservation and clean-tree amend;
- corrupt state recovery and failed group persistence;
- stale async commit/compare/stash results;
- watcher detection of `.git` changes;
- long text in fields at both supported widths.

**Files:** `crates/vantage/src/app.rs` test module, `views/*` tests,
`crates/vantage-core/src/persist.rs`, `crates/vantage-core/src/git/ops.rs`.
**Acceptance:** each P0/P1 fix has a test that fails on the previous behaviour.

---

## P2 — Product, platform and release quality

### [x] P2-01 — Replace macOS-only external commands with platform helpers
**Area:** cross-platform · **Effort:** M
**Problem:** reveal uses `open -R`, terminal uses `open -a Terminal`, and the editor fallback uses
`open` on every platform. These fail on Linux, which the README supports. `$EDITOR="code --wait"`
also fails because it is treated as one executable name.
**Do:** add `reveal_in_file_manager`, `open_terminal`, `open_editor` helpers with `cfg` and
`which`-based detection; parse the editor command into program + arguments; add a Settings
editor preference.
**Files:** `crates/vantage/src/app.rs`, `crates/vantage-core/src/detect.rs`,
`crates/vantage/src/views/settings.rs`.
**Acceptance:** Reveal/Terminal/Editor work on macOS and Linux; an editor command with arguments
works; invalid commands produce an actionable error.

### [x] P2-02 — Fix Git edge cases: force delete, detached push, merge pull
**Area:** Git correctness · **Effort:** M
**Problem:** branch delete never offers force; a detached HEAD can enable Push and try
`git push -u origin HEAD`; `git pull --no-rebase` can try to open an editor for a merge commit.
**Do:** ask for a second confirmation with `is_branch_merged`/`-D`; disable Push in detached HEAD
or require an explicit branch; add `--no-edit` or a non-interactive editor to merge pull.
**Files:** `crates/vantage/src/app.rs`, `crates/vantage-core/src/git/refs.rs`.
**Acceptance:** unmerged branches can be deleted after explicit confirmation; detached HEAD never
attempts a publish; merge pulls never block on an editor.

### [x] P2-03 — Wire clone progress and cancellation
**Area:** UX / feature · **Effort:** M
**Problem:** the clone dialog only shows “Cloning…” because the engine progress sink is discarded.
Large clones look frozen and cannot be cancelled.
**Do:** stream progress through the backend to the dialog, show the current phase/percent, and add
a cancel action that kills the child process cleanly.
**Files:** `crates/vantage/src/backend/mod.rs`, `crates/vantage/src/app.rs`,
`crates/vantage/src/views/*` clone modal, `crates/vantage-core/src/git/clone.rs`.
**Acceptance:** cloning a large repository shows live progress; cancel terminates the child and
leaves no half-registered entry.

### [x] P2-04 — Correct the “commits not pushed” marker and diverged history
**Area:** Git correctness / UI · **Effort:** S
**Problem:** the header is inserted at the top of the commit list, so it labels the whole history
as unpushed. `commit_is_local_only(index)` assumes the newest N commits are the local-only ones,
which is false on a diverged branch.
**Do:** place a separator after the exact local-only count, and identify local-only commits from
the actual upstream range (`@{upstream}..HEAD` or remote containment), not by index.
**Files:** `crates/vantage/src/views/inspector.rs`, `crates/vantage/src/model.rs`,
`crates/vantage/src/backend/details.rs`.
**Acceptance:** on a diverged branch, only commits absent from the upstream carry the local-only
marker; the header sits at the boundary.

### [x] P2-05 — Make “Ungrouped” reachable and stop defaulting new repos to a group
**Area:** UX / repository management · **Effort:** S
**Problem:** when “Ungrouped” is absent from `groups`, neither the Add modal nor the inspector
dropdown offers it. The Add modal defaults to the first existing group.
**Do:** always include “Ungrouped” in group options; default to “Ungrouped” unless the user is
filtering a specific group; allow moving a repo to Ungrouped.
**Files:** `crates/vantage/src/app.rs` (`open_add_modal`), `views/add_repo_modal.rs`,
`views/inspector.rs`.
**Acceptance:** a new repo defaults to Ungrouped; a repo can be moved to Ungrouped from both the
modal and the inspector.

### [x] P2-06 — Show failures instead of empty commit/compare/stash views
**Area:** UX / error handling · **Effort:** S
**Problem:** `unwrap_or_default()` turns Git failures into empty data states (“This commit has
no textual changes”), misleading the user.
**Do:** carry errors into the view model, show a clear error with Git's message and a retry
action, and distinguish loading/empty/error.
**Files:** `crates/vantage/src/app.rs` (`open_commit_view`, `compare_with_branch`,
`open_stash_modal`), `views/inspector.rs`.
**Acceptance:** a missing ref or permission error is visible and retryable, never shown as empty.

### [x] P2-07 — Make avatar attribution private and bounded
**Area:** privacy / performance · **Effort:** S
**Problem:** the avatar lookup falls back to the machine's `gh` login for any commit email, so
your photo can appear beside someone else's commit. Selecting a repo starts up to 15 curl
processes.
**Do:** only resolve an avatar when the email proves a login (noreply address) or
`github.user` is explicit; for the commit bar, use the effective Git identity. Cache per email
and throttle/concurrency-limit downloads; do not fetch for history until visible.
**Files:** `crates/vantage/src/avatar_cache.rs`, `crates/vantage/src/app.rs`.
**Acceptance:** no avatar is shown for an unproven identity; selecting a repo makes at most one
network call per uncached author; no more than N downloads run concurrently.

### [x] P2-08 — Add folder scanning and remove the false “stack” hint
**Area:** onboarding / feature · **Effort:** M
**Problem:** adding repos is one path at a time even though `repos::scan_folder` exists. The Add
modal says it reads the stack, but no stack is shown or stored.
**Do:** add “Scan folder for repositories”, preview the found repositories, and let the user add
selected repos to a group. Either surface stack/language or remove the hint.
**Files:** `crates/vantage/src/app.rs`, `views/add_repo_modal.rs`,
`crates/vantage-core/src/repos.rs`.
**Acceptance:** adding a folder with 10 repositories takes one flow; no unshown “stack” claim
remains.

### [x] P2-09 — Stop synchronous state writes on hot UI paths
**Area:** performance / persistence · **Effort:** M
**Problem:** every repository selection calls `save_settings`, which loads and rewrites the whole
state file on the UI thread. `state_path()` constructs a new persistence object (and writer
thread) per call. Two app instances can overwrite each other.
**Do:** use `save_debounced` for settings/layout/selection; cache `state_path`; add a process
lock or warn on a second instance; keep critical mutations immediate.
**Files:** `crates/vantage/src/backend/mod.rs`, `crates/vantage/src/app.rs`,
`crates/vantage-core/src/persist.rs`.
**Acceptance:** clicking rows does not perform synchronous disk I/O; state changes survive a
normal quit; two instances do not silently corrupt state.

### [x] P2-10 — Remove the Gatekeeper bypass from the cask
**Area:** release security · **Effort:** S
**Problem:** the cask runs `xattr -dr com.apple.quarantine` after installation, bypassing
Gatekeeper. The release pipeline can publish an ad-hoc build when signing secrets are absent.
**Do:** remove the postflight step; require signing + notarization for published releases; if
ad-hoc builds remain, document the right-click flow instead of silently disabling the check.
**Files:** `Casks/vantage.rb`, `scripts/bundle-macos.sh`, `.github/workflows/release.yml`.
**Acceptance:** `brew install` leaves the quarantine attribute in place; published releases pass
`spctl --assess`; no release job can publish an unnotarized build without a deliberate opt-in.

### [x] P2-11 — Add third-party font licenses
**Area:** legal · **Effort:** S
**Problem:** Public Sans, Plus Jakarta Sans and JetBrains Mono are bundled OFL fonts but no
license texts are included.
**Do:** add the OFL texts and a `THIRD_PARTY_NOTICES.md` linked from the README and the app’s
About section.
**Acceptance:** every bundled font has its license file and attribution.

### [x] P2-12 — Constrain error toasts and allow copying details
**Area:** UX / errors · **Effort:** S
**Problem:** Git errors are shown in an unbounded toast with no max width; long stderr can run
off-screen and disappears after 2.6 s.
**Do:** cap width, wrap text, add a copy button and optionally a details modal for multi-line
Git output.
**Files:** `crates/vantage/src/app.rs` (`show_toast`, `render_toast`).
**Acceptance:** a 4 KB Git error stays readable and copyable.

### [x] P2-13 — Surface branch actions and ahead/behind counts
**Area:** feature / discoverability · **Effort:** M
**Problem:** branch rows expose checkout only through right-click, and the README promises
ahead/behind counts that the model does not carry.
**Do:** add visible Checkout / Create / Merge actions (right-click can remain a shortcut), show
each branch's ahead/behind and upstream, and add an empty state.
**Files:** `crates/vantage/src/views/inspector.rs`, `crates/vantage/src/backend/details.rs`,
`crates/vantage/src/model.rs`.
**Acceptance:** every branch action is discoverable without right-click; counts match `git
for-each-ref`/upstream data.

---

## P3 — Maintenance and polish

### [x] P3-01 — Break up the largest modules
**Area:** code quality · **Effort:** L
Split `app.rs` (4.4k lines), `views/inspector.rs` (1.8k) and `git/ops.rs` (4.3k) by
responsibility: composer/session state, repository actions, modals, diff rendering, recovery
operations. Keep the existing public behaviour and tests; move code in small, reviewable commits.

### [x] P3-02 — Replace string-dispatched context menus with typed actions
**Area:** code quality · **Effort:** M
Today menu items are matched by English label strings. Use `ContextAction` enums or action
ids so renaming a label cannot silently disable an action; add an exhaustiveness test.

### [x] P3-03 — Remove dead code and duplicate implementations
**Area:** maintenance · **Effort:** M
- Remove or wire `search_query`, `status_filter`, `scan_folder`, `stack_of`, core triage/blob/
  grep/binary modules, and the unused core watcher.
- Unify the two status parsers and relative-time formatters.
- Resolve the `notify` 6 / `notify` 8 duplication.
- Remove stale `open_in_github_desktop` references from the crate README.

### [x] P3-04 — Fix the rejected-operation busy-state bug
**Area:** correctness · **Effort:** S
`run_repo_op` inserts the new op before checking for an existing one, so a rejected second
operation overwrites the visible spinner label. Use `contains_key` or `entry` and leave the
running op untouched.

### [x] P3-05 — Remove per-call persistence/writer construction
**Area:** performance · **Effort:** S
`state_path` constructs a new `StatePersistence` and writer thread on every call; cache the path
and use the process-wide persistence handle for all reads/writes.

### [x] P3-06 — Finish release/CI polish
**Area:** release · **Effort:** S
- Fix the two `brew style` offenses.
- Align the issue template’s Windows option with the supported macOS/Linux matrix.
- Add a post-release smoke checklist (launch, add repo, stage/commit, fetch, theme, settings).
- Consider a Windows build job or explicitly mark Windows unsupported.

### [x] P3-07 — Update architecture and crate documentation
**Area:** docs · **Effort:** S
`docs/ARCHITECTURE.md` still describes a smaller write surface and a watcher that no longer
matches `src/watcher.rs`; the crate README mentions APIs that do not exist. Regenerate the
module map from the current tree and link this TODO from `docs/README.md`.

### [x] P3-08 — Add performance budgets and a benchmark fixture
**Area:** performance · **Effort:** M
Measure and record:
- overview scan time for 5 / 20 / 50 repositories;
- idle CPU/watcher cost while a repository builds;
- time-to-detail when selecting a repository;
- memory with 100 changed files and a 10 MB patch.

Run the benchmark in CI or a scheduled job; fail on a documented regression budget.

### [x] P3-09 — Plan a GPUI upgrade
**Area:** maintenance · **Effort:** XL
GPUI 0.2.2’s truncation and input limitations have already leaked into the UI. Evaluate a newer
GPUI release in a branch, measure the API breakage, and decide whether to upgrade before adding
more custom primitives.

---

## Suggested execution order

1. **Safety batch:** P0-01 → P0-02 → P0-03 → P0-04 → P0-05 → P1-10.
   Freeze new features until these are green.
2. **Trust batch:** P1-01 → P1-02 → P1-08 → P1-09.
3. **Performance batch:** P1-03 → P2-09 → P3-08.
4. **Usability batch:** P1-04 → P1-05 → P1-06 → P1-07 → P2-12.
5. **Platform/release batch:** P2-01 → P2-02 → P2-03 → P2-10 → P2-11 → P3-06.
6. **Cleanup:** P3-01 → P3-02 → P3-03 → P3-04 → P3-05, then P3-09 as a separate spike.

**Rule of thumb:** do not close a P0/P1 item without a regression test that fails on the previous
behaviour. The engine is good enough that the remaining work is mostly about making state and
targeting explicit, not about inventing more Git features.
