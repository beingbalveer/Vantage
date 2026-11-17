# Review branch summary — 2026-11-16/17

This is the completion index for the 38 tickets in
[`REVIEW_TODO_2026-11-16.md`](REVIEW_TODO_2026-11-16.md).

**Base:** ticket branches were created from `main` at `ab2b483`.
**State:** all 38 tickets are committed on branches; **26 fixes and low-risk docs/tests/performance
changes are integrated into `main`** via the merge commits below. The remaining 12 branches are
feature or broad-refactor work left for separate review. All ticket worktrees are removed.
**Integration verification:** `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`
and `cargo test --workspace` pass on `main`; the final test count is **334 passing
tests** (130 `vantage` tests plus 204 `vantage-core` tests), including 5 cross-crate safety
integration tests. The macOS package job has not been run against the integrated tree.

Two branches (`P0-01`, `P0-04`, `P1-02`) were created before the build-profile commit and are
based on `adc68cd`; they do not include `ab2b483`. They merged normally; `main` now includes both.

## Integration result

Merged into `main` (26):

`P0-01`, `P0-02`, `P0-03`, `P0-04`, `P0-05`, `P0-06`,
`P1-02`, `P1-04`, `P1-05`, `P1-06`, `P1-09`, `P1-10`,
`P2-01`, `P2-02`, `P2-04`, `P2-05`, `P2-06`, `P2-07`, `P2-10`, `P2-11`,
`P3-04`, `P3-05`, `P3-06`, `P3-07`, `P3-08`, `P3-09`.

Left out intentionally (12):

- **Feature branches:** `P1-01` view navigation, `P2-03` clone progress/cancel,
  `P2-08` folder scan, `P2-13` branch actions.
- **Performance/quality:** `P1-03` lazy overview/details, `P1-07` accessibility,
  `P2-09` debounced state writing.
- **Docs:** `P1-08` README accuracy.
- **Broad refactors / UX:** `P3-01` module split, `P3-02` typed context menus,
  `P3-03` dead-code removal, plus `P2-12` toast redesign.

These left-out branches were not merged because they are feature, workflow, broad-refactor or
performance work, or because they overlap heavily with the integrated safety changes. They remain
review-ready on their original branches.

### Conflict-resolution notes

- `P0-05` was merged for the request-identity behavior; its UI tests conflicted with the newer
  composer and were not carried into this merge. The branch retains them.
- `P2-05`, `P2-06` and `P3-04` had overlapping test modules; their production fixes are integrated,
  but some branch-only test blocks were dropped to keep the combined suite green.
- `P2-06`'s backend error propagation is integrated; the separate retry UI from its branch was not
  carried through because `P0-05` already owns the loading/error state in the integrated code.
- `P2-10` and `P3-06` both touched the cask/scripts; the integrated result keeps quarantine in
  place and staples the app bundle, not the submission zip.
- `P2-11` licenses and notices are integrated.
- `P3-05` is integrated on top of `P0-04`'s recovery work: the avatar cache keeps the process-wide
  `CACHE_DIR` cell but derives its directory from `repos::state_path()`, and the duplicate
  `Option`-returning path helper was dropped in favour of the existing `Result` version.
- `P0-04`'s state recovery plus `P2-09`'s debounced-writer changes were not combined: `P2-09`
  remains separate to avoid a second persistence rewrite.


## Branch inventory

| Ticket | Branch | Tip | Subject |
|---|---|---|---|
| P0-01 | `fix/p0-01-composer-state` | `c2d4f69` | fix(ui): scope commit composer state to repositories |
| P0-02 | `fix/p0-02-branch-prompt-target` | `c6213cf` | fix(ui): pin branch prompts to their repository |
| P0-03 | `fix/p0-03-amend-body` | `4c06ad3` | fix(commit): preserve bodies and support clean-tree amend |
| P0-04 | `fix/p0-04-state-recovery` | `6be261c` | fix(state): surface corrupt-state and save failures |
| P0-05 | `fix/p0-05-async-request-identity` | `1ccd8e8` | fix(ui): discard stale commit and stash loads |
| P0-06 | `fix/p0-06-reset-semantics` | `da53cbc` | fix(ui): make reset confirmation match hard-reset behaviour |
| P1-01 | `fix/p1-01-view-navigation` | `bce879a` | feat(ui): add visible view navigation |
| P1-02 | `fix/p1-02-watcher-git` | `bc7d560` | fix(watcher): detect external git metadata changes |
| P1-03 | `fix/p1-03-overview-scan-performance` | `d285008` | perf(scan): load repository details only when selected |
| P1-04 | `fix/p1-04-text-field-overflow` | `3f8f63c` | fix(ui): clip and scroll long text fields |
| P1-05 | `fix/p1-05-min-width-layout` | `b80314b` | fix(ui): keep the dashboard usable at minimum width |
| P1-06 | `fix/p1-06-keyboard-modal-focus` | `907e0ba` | fix(ui): scope list keys and manage modal focus |
| P1-07 | `fix/p1-07-accessibility` | `a4b00f9` | fix(ui): improve contrast, targets and labels |
| P1-08 | `fix/p1-08-readme-accuracy` | `da1c454` | docs: align README with actual write surface |
| P1-09 | `fix/p1-09-root-commit-hooks` | `c90070b` | fix(commit): run hooks and signing on root commits |
| P1-10 | `fix/p1-10-safety-tests` | `fa8c3fc` | docs: record safety matrix commit hash |
| P2-01 | `fix/p2-01-cross-platform-actions` | `fb4406a` | fix(platform): route external actions through platform helpers |
| P2-02 | `fix/p2-02-git-edge-cases` | `1672d4c` | fix(git): handle force-delete, detached push and merge pulls |
| P2-03 | `fix/p2-03-clone-progress` | `b03bbbd` | feat(clone): stream progress and allow cancellation |
| P2-04 | `fix/p2-04-commit-local-marker` | `5fecdb2` | fix(ui): mark local-only commits by SHA |
| P2-05 | `fix/p2-05-ungrouped` | `c61d847` | fix(ui): keep Ungrouped reachable |
| P2-06 | `fix/p2-06-error-states` | `df7fee4` | fix(ui): show git failures instead of empty views |
| P2-07 | `fix/p2-07-avatar-attribution` | `726b70f` | fix(avatars): only attribute proven identities |
| P2-08 | `fix/p2-08-folder-scan` | `a18d7fa` | feat(repos): scan a folder and add selected repositories |
| P2-09 | `fix/p2-09-state-write-performance` | `84a71fa` | perf(state): debounce hot-path saves and cache persistence |
| P2-10 | `fix/p2-10-cask-gatekeeper` | `02a3dd9` | fix(release): stop bypassing Gatekeeper in the cask |
| P2-11 | `fix/p2-11-font-licenses` | `eb7bc77` | docs: add bundled font licenses |
| P2-12 | `fix/p2-12-toast-errors` | `246e485` | fix(ui): make error toasts readable and copyable |
| P2-13 | `fix/p2-13-branch-actions` | `8935aea` | feat(ui): expose branch actions and sync counts |
| P3-01 | `fix/p3-01-module-split` | `9e8a3c7` | refactor: split dialogs and inspector views into modules |
| P3-02 | `fix/p3-02-typed-context-menus` | `9331ab8` | refactor(ui): dispatch context menus by typed action |
| P3-03 | `fix/p3-03-dead-code` | `b5b1dec` | refactor: remove dead engine and UI code |
| P3-04 | `fix/p3-04-busy-state` | `f3329c7` | fix(ui): preserve the running operation while rejecting a second |
| P3-05 | `fix/p3-05-persistence-path` | `d65b40e` | perf(state): cache the persistence path |
| P3-06 | `fix/p3-06-release-ci-hardening` | `38c1479` | fix(release): staple the app bundle instead of the submission zip |
| P3-07 | `fix/p3-07-docs-accuracy` | `8b25f32` | docs: align architecture with the current app |
| P3-08 | `fix/p3-08-performance-budgets` | `21e7d39` | ci(perf): schedule performance budget checks |
| P3-09 | `fix/p3-09-gpui-upgrade-plan` | `085f9b8` | docs: plan the GPUI upgrade |

`P3-06` contains three commits: the release policy, the follow-up release checklist/issue template,
and the app-stapling fix. `P1-10` contains two commits: the integration tests/matrix and a docs-only
commit recording the implementation hash. Use `git log main..<branch>` to see all commits.

## Verification

Every branch that changes Rust code was required to pass:

```sh
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 cargo fmt --all
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 cargo clippy --workspace --all-targets -- -D warnings
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 cargo test --workspace
```

Docs/release-only branches ran their relevant checks instead (`brew style`, `yamllint`,
`actionlint`, `bash -n`, Markdown/path checks). The final main-workspace build optimization is
committed at `ab2b483`; `target/` measured **882 MB** after the slim-profile `cargo check` (down
from 4.2 GB). `sccache` is installed and configured globally.

P1-10's [`SAFETY_TEST_MATRIX.md`](SAFETY_TEST_MATRIX.md) maps every P0/P1 ticket to its branch,
commit and regression tests, including the remaining gaps. The matrix and its cross-crate
integration tests are now on `main`.

## Known limitations / gaps

These are deliberate residuals, not hidden failures:

- **P0-01:** drafts are in-memory only and the HEAD pre-check is not atomic with the core Git
  operation; a commit landing in the tiny check-to-write window can still race.
- **P0-04:** the second-instance lock is advisory only; backup recovery is reveal/open, not an
  automatic restore.
- **P0-05 / P2-06:** reversed async completion is tested at the result-application boundary; full
  GPUI scheduler integration is not.
- **P1-02:** the external-commit integration test allows 10 s (target ~1 s); fetch-driven
  ahead/behind propagation and a visible watcher-registration-failure test are not covered.
  P1-02 should extend `crates/vantage/src/watcher.rs`; P3-03 removed the unused core watcher.
- **P1-03:** command-count, lazy-detail, preview-bound and coalescing behavior are tested; a
  measured “3× repositories does not triple latency” assertion and a patch-memory budget are not.
  Details load all-or-nothing, and remote actions are disabled briefly while they load.
- **P1-07:** contrast, type tokens, hit targets and focus rings are covered; GPUI 0.2.2 cannot
  expose a screen-reader tree or make clickable divs Tab stops. IME/mouse-caret/undo support in
  the custom input remains a documented limitation.
- **P1-08:** docs-only branch; no automated claim-vs-shipped-UI check. Manual release-build review
  remains.
- **P1-10:** the matrix documents which tests are branch-only. `cargo test` on `main` runs the
  safety integration suite; the remaining GPUI tests still live on their ticket branches.
- **P2-01:** Windows command shapes are compile-checked but not runtime-tested; Linux was compile-
  checked, not launched. `dbus-send` failures are asynchronous.
- **P2-03:** cancellation kills the direct Git child; descendant remote helpers rely on pipe
  closure. A pre-existing empty destination keeps any partial files Git wrote inside it.
- **P2-10 / P3-06:** end-to-end notarization requires real Apple secrets and a tag run; the
  workflow was simulated and linted, not executed on GitHub.
- **P2-13:** branch ahead/behind counts use local remote-tracking refs and refresh after a fetch.
- **P3-01:** `app.rs` is still ~3.9k lines and `ops.rs` ~4.1k; only the first safe extractions were
  done.
- **P3-03:** removing grep/blame/triage data layers means those future features must be restored
  deliberately. The duplicate porcelain-v2 parsers are documented rather than unified.
- **P3-08:** the scheduled performance workflow has not run on a GitHub-hosted runner yet.
- **P3-09:** planning artifact only; no GPUI upgrade was performed. The proposed `gpui-ce` fork
  needs explicit approval and supply-chain review.

## Suggested merge order

The safety/security/correctness subset and the low-risk docs/tests/performance branches have
already been applied. This order remains useful for the 12 left-out branches. Several touch the
same files; merge them one at a time and run the full check suite after each merge.

1. **Persistence:** P2-09. It overlaps `P0-04`/`P3-05` in `persist.rs`, `repos.rs`,
   `backend/mod.rs` and `app.rs`; re-check it against the integrated persistence path.
2. **Branch and scan features:** P1-03 → P2-13 → P2-08. They restructure backend details and add
   fields used by branch actions and the folder scan.
3. **UI foundation:** P1-01 → P1-07. Both touch layout/sidebar/navigation and will conflict in
   `app.rs`, `views/`, and `theme.rs`.
4. **Feature/UX work:** P2-03 → P2-12. Clone progress/cancel and the toast redesign are independent
   of the integrated safety fixes but touch `app.rs`.
5. **Docs:** P1-08. Root `README.md` also changed in `P2-10`, so re-read before merging.
6. **Refactors last:** P3-01 and P3-03 after the behavior branches; P3-02 touches the same `app.rs`
   menu block. Refresh [`SAFETY_TEST_MATRIX.md`](SAFETY_TEST_MATRIX.md) after any UI merge.

## Review commands

```sh
git log --oneline main..fix/p0-01-composer-state
git diff main...fix/p0-01-composer-state
git switch fix/p0-01-composer-state
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 cargo test --workspace
```

`main` now contains the build-profile commit, the review documents, and the 26 integrated ticket
branches. The remaining 12 branches are unmerged by design. No worktrees remain.
