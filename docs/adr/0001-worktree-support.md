# ADR 0001: Worktree support scope

- **Status:** Accepted
- **Date:** 2026-10-19
- **Ticket:** ALPH-166
- **Spec reference:** `docs/COMPARISON_GITHUB_DESKTOP.md` §4 item 23 ("Worktrees (optional)")
- **Deciders:** Vantage maintainers
- **Related:** `PRODUCT_SPEC.md` §1.5 and §2 (non-goals, glossary); `docs/PROTOTYPE_REVIEW.md` R01;
  `docs/PRODUCTION_REVIEW_2026-10-16.md` R01, R09, R25, R48, R52, R53

---

## Context

GitHub Desktop recently shipped a worktree UI. The comparison backlog flags worktrees as
**optional / later, not parity** (§4 item 23), on the grounds that Vantage's existing "branch views"
solve a related need more cheaply. This ADR turns that one-line backlog note into a recorded
decision by looking at what the code actually does today and what a worktree UI would cost.

### What a worktree is

A Git worktree is a second working directory attached to the same repository, with its own
`HEAD`, index, and checked-out files, sharing one object database and ref store. `git worktree add`
creates one, `git worktree list` enumerates them, `git worktree remove` deletes one. A linked
worktree contains a **`.git` file** (a `gitdir:` pointer), not a `.git/` directory.

### What "branch view" means in Vantage today

Vantage does not model worktrees. It models **one `RepoEntry` per (path, branch) pair**
(`src-tauri/src/state.rs:5`), and it shells out to `git -C <path>` for every operation
(`src-tauri/src/git/runner.rs:100`).

- Registering a view calls `repo_add(path, branch)` and stores the *worktree's* toplevel path plus
  an optional branch (`src-tauri/src/cmd.rs:59`, `cmd.rs:83-91`, `cmd.rs:150-159`). Duplicate
  detection is keyed on `(path, branch)` (`cmd.rs:96-102`).
- Status is computed against the chosen ref, not the checked-out ref: `query_repo_status` takes
  `target_branch`, resolves it as the comparison ref, and derives `is_checked_out` by comparing it
  to the real `HEAD` (`src-tauri/src/git/status.rs:77`, `status.rs:152`, `status.rs:202-205`).
  For a view whose branch is not checked out, upstream/ahead/behind/dirty are deliberately cleared
  (`status.rs:237-248`), and `is_checked_out = false` is surfaced to the UI
  (`state.rs:36-50`, `src/lib/types.ts:22-35`).
- Writes are gated on the branch actually being checked out at that path:
  `verify_branch_checked_out` rejects commits when `HEAD` differs from the view's branch
  (`src-tauri/src/cmd.rs:635-653`), and pull/push refuse inactive branches the same way
  (`src-tauri/src/git/refs.rs:98-105`, `refs.rs:130-137`).
- The UI creates a view by re-registering the same path with a different branch and focusing the
  resulting pane (`src/lib/components/Header/TopHeader.svelte:116-158`).

So a "branch view" is **read-only browsing of a ref at a path that is physically checked out to
something else**. It is not a directory where that branch can be edited, committed, or run against.

### What the rest of the stack already handles for worktrees

Several pieces are already worktree-aware because they shell out to `git` and resolve paths at
runtime:

- **Watcher:** `watch_repo` resolves the real git dir with `git rev-parse --git-dir`, which follows
  a worktree `.git` pointer (`src-tauri/src/watch.rs:53-75`), then watches `HEAD`, `refs`, `index`,
  `logs/HEAD`, `packed-refs` inside it.
- **Backups/trash:** `get_real_git_dir` uses the same `rev-parse --git-dir`
  (`src-tauri/src/git/ops.rs:22-37`) and stores recovery metadata under
  `<git-dir>/vantage-backups` and `<git-dir>/vantage-trash` (`ops.rs:44-51`, `ops.rs:395-415`).
- **Terminal:** each session is a PTY whose cwd is the entry's path
  (`src-tauri/src/term.rs:120-121`), and `term_open` resolves that from the `RepoEntry`
  (`src-tauri/src/cmd.rs:929-937`).

Because a linked worktree has its own path, registering one as a `RepoEntry` would already give
correct per-worktree status, watching, and terminal — **except** that `get_real_git_dir` returns the
*per-worktree private* git dir (`.git/worktrees/<name>`), so backup metadata and the untracked-file
trash bin would be written per-worktree rather than into the shared common dir
(`git rev-parse --git-common-dir`). Refs (`refs/vantage/backup/*`) are shared, but the JSON records
and trash are not. This is already latent in `ops.rs` and is called out by production review R09.

### Product constraints

- The stated audience "uses worktrees rarely" (`PRODUCT_SPEC.md:17`).
- "No worktree UI" is an **explicit v1 non-goal**, with the embedded terminal named as the answer
  (`PRODUCT_SPEC.md:47`). A worktree watcher is explicitly deferred to v1.1 (`PRODUCT_SPEC.md:516`,
  `PRODUCT_SPEC.md:721`).
- Multiple windows are a non-goal; panes are the intended answer
  (`docs/COMPARISON_GITHUB_DESKTOP.md:144`).
- The product's differentiating value is the **review loop**, and the P0 backlog is unfinished
  (multi-pane isn't rendered — `src/App.svelte:161` mounts only `paneIndex={0}` — plus sidebar
  badges, needs-review header, triage, destructive entry points; comparison items 1–5).
- The branch-view model already has a known correctness debt: a write can target the checked-out
  branch while the screen names another (production review R01; guarded by
  `cmd.rs:635`/`refs.rs:98`, but the surrounding session/identity model is still being unified per
  R48, R52).

---

## Decision

**Do not build a worktree UI now.** Keep the embedded terminal as the escape hatch, and record the
minimum useful slice below so the door stays open.

Concretely:

1. No worktree create/list/remove UI, no worktree picker, no worktree status surface.
2. Do not auto-convert branch views into worktrees. Branch views stay read-only-by-design
   (`verify_branch_checked_out`, `is_checked_out`), which is the safe behavior.
3. `git worktree …` remains available through the embedded terminal, whose cwd is the repo path
   (`src-tauri/src/term.rs:121`). A user who wants a real second checkout runs
   `git worktree add ../repo-feat -b feat` in the terminal, then adds that directory with the
   existing Add-repository flow; `repo_add` will resolve it correctly via
   `git rev-parse --show-toplevel` (`src-tauri/src/cmd.rs:74-85`).
4. The link from `docs/COMPARISON_GITHUB_DESKTOP.md` item 23 to this ADR is **deferred to a later
   change** (that file is owned by other in-flight work). This ADR is the record of the decision;
   the cross-reference is a follow-up edit.
5. If worktrees are ever built, the first slice is the small one in §"Minimum useful slice", not the
   full GitHub-Desktop-style surface.

---

## Alternatives considered

### A. Full worktree UI (create / list / remove / open)

Rejected for now. Cost is **L** and touches every subsystem:

- **Backend plumbing:** `git worktree add <path> [-b <branch>]`, `worktree list --porcelain`,
  `worktree remove [--force]`, `worktree prune`, and `worktree move` wrappers. All are writes and
  must interact with the per-path repo lock (`src-tauri/src/git/runner.rs:44-51`), which is keyed on
  the *worktree* path while ref-mutating operations act on the *shared* ref store.
- **Identity model:** today an entry is `(path, branch)` (`state.rs:5`); worktrees need a distinct
  kind (main checkout vs linked worktree vs branch view) plus a canonical/common-dir identity for
  grouping, locking, and stable avatars (production review R25, R26, R32).
- **Recovery correctness:** `get_real_git_dir` (`ops.rs:22`) and the trash bin (`ops.rs:395`) must
  switch to `--git-common-dir` for shared recovery, which is entangled with R09 and R52.
- **Dirty-worktree safety:** removing a worktree with uncommitted changes must refuse or back up
  first (R09, R52), and a checkout in a shared directory must identify affected sessions
  (`PROTOTYPE_REVIEW.md:55-57`).
- **Frontend:** sidebar grouping of repo + views + worktrees (R25), repo/worktree disambiguation in
  the header (R26), a workspace session keyed by repository/worktree/view (R48), and create/remove
  dialogs — none of which exist.
- **Tests:** disposable fixtures plus UI/workflow tests including worktrees
  (production review R53).

This is roughly as large as making the existing branch-view write path fully trustworthy (R01 is
itself sized **L**), and it front-loads a non-goal before the core review loop is finished.

### B. Open an existing worktree as a view (the minimum slice)

The smallest genuinely useful increment; see the next section. Cheap (~S) but only valuable once
someone has already created a worktree externally, which the target user rarely does.

### C. Turn branch views into real worktrees automatically

Rejected. Silently creating directories and checkouts on disk is a destructive surprise, violates
"calm by default" and "safe by default" (`PRODUCT_SPEC.md:40-42`), is explicitly argued against in
`PROTOTYPE_REVIEW.md:41` ("a proposed direction, not an assumption that the user has selected a
worktree-first workflow"), and would multiply the R01 class of bug rather than fix it.

### D. Do nothing and remove branch views

Rejected. Branch views are a documented differentiator
(`docs/COMPARISON_GITHUB_DESKTOP.md:32`, `:96`) and already cover the read-only multi-branch
browsing that the review loop needs.

### E. Terminal-only (chosen)

Chosen. The terminal already runs in the repo directory (`term.rs:121`), so `git worktree` is
available today without any new surface, at zero recurring UI cost and with no new write risk.

---

## Consequences

### Positive

- No new write surface is added while the existing branch-view write path is still being hardened
  (R01, R48, R52). Worktrees would be a *second* way to have uncommitted state in a directory.
- Effort stays on the unfinished P0 review loop (comparison items 1–5), which is the product's
  reason to exist.
- Consistent with the written spec: worktree UI is an explicit non-goal
  (`PRODUCT_SPEC.md:47`); watcher work is v1.1 (`PRODUCT_SPEC.md:516`).
- The escape hatch is real and already shipped: `git worktree add` in the embedded terminal plus the
  existing Add-repository flow yields a fully working second checkout, including watching and a
  terminal (subject to the shared-recovery caveat below).

### Negative

- Users who want GitHub-Desktop-style worktree management must use the terminal and add the
  directory manually. No discovery of existing worktrees, no one-click create/remove.
- A manually added worktree currently writes backup metadata and the untracked-file trash bin into
  the per-worktree private git dir, not the shared common dir (`ops.rs:22`, `ops.rs:395`). Recovery
  is therefore per-worktree and can look "missing" from another view. This is pre-existing and
  tracked by R09/R52; it is called out here because worktrees make it observable.
- The ref-mutating operations (fetch/prune) act on shared refs while locks are per path
  (`runner.rs:44`), so running the same repo in two worktrees can race on the ref store.
- The decision is only recorded, not linked from the comparison doc yet (deferred per above), so the
  backlog line still reads as an open item until the follow-up link lands.

---

## Minimum useful slice (if ever built)

Order matters; each step is independently shippable and each is small compared with alternative A.

1. **Recognize and label an existing worktree.** Detect a linked worktree when a directory is added
   (`git rev-parse --git-common-dir` ≠ `git rev-parse --git-dir`, or `.git` is a file) and show
   "worktree" context in the row/header instead of pretending it is a normal repo. **~S.**
2. **Share recovery state via the common dir.** Point `get_real_git_dir`-based backup metadata and
   the untracked trash bin at `--git-common-dir` so Undo/Recovery works across views and worktrees
   (folds into R09/R52). **~S–M.**
3. **Open existing worktree as a tab/pane.** Surface the discovered worktrees for a repo
   (`git worktree list --porcelain`) with a one-click "Open". This is mostly sidebar/grouping work
   (R25) on top of step 1. **~S–M.**
4. **Create worktree from a branch.** `git worktree add <sibling-path> -b <branch>` with a chooser
   for path/branch and the same backup-first safety as other writes. **~M.**
5. **Remove / prune worktree.** `git worktree remove [--force]` / `prune`, refusing when dirty
   unless backed up. **~M.**

Steps 1–3 deliver "open existing worktree as a view" without any new creation surface; steps 4–5 are
the authoring half and only make sense once the write path (R01) and recovery model (R09/R52) are
solid.

---

## Revisit trigger

Revisit this decision when **any** of the following holds:

- Parallel-agent workflows make **simultaneous separate checkouts** the common case (e.g. the user
  routinely runs one agent per worktree), rather than the current "rarely used" assumption
  (`PRODUCT_SPEC.md:17`).
- Users report that read-only branch views are insufficient because they need *uncommitted working
  state* on two branches at once — the one thing branch views deliberately do not provide.
- The P0 review-loop items (comparison items 1–5) and the write-identity/recovery fixes (R01, R09,
  R48, R52) have shipped and stabilized, freeing capacity for a non-goal.
- Worktree management becomes table stakes among comparable local Git clients, not just GitHub
  Desktop.

When revisited, start at the minimum slice, not alternative A.
