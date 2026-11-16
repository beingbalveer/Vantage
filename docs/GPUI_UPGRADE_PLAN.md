# GPUI upgrade plan (P3-09)

Status: proposal, 2026-11-16. This is a planning artifact: it does not change
`Cargo.toml`, `Cargo.lock` or application code.

Ticket: P3-09 — Plan a GPUI upgrade (2026-11-16 review).

## Summary

- **Current version:** `gpui 0.2.2`, both declared in
  `crates/vantage/Cargo.toml` and resolved in `Cargo.lock`. The dev-dependency
  enables the same version with `test-support`.
- **Target version:** `gpui-ce 0.3.0` (published 2026-10-20), imported under the
  existing `gpui` name, plus `gpui_ce_platform 0.3.0` imported as
  `gpui_platform`.
- **Why this target:** there is still no newer release of Zed's official `gpui`
  crate. `gpui-ce` is the only newer published line with the application-facing
  APIs Vantage needs, including proper text truncation and a maintained text
  input implementation. A compile spike against it produced 27 compile errors;
  all were mechanical and the test suite passed after the port, with the
  custom text-helper stub called out below.
- **Effort:** the mechanical port is small, but text metrics, text input and
  macOS window chrome need visual validation. Estimate 5–8 focused days for the
  required migration and 2–4 additional days if the custom input is replaced.
  This remains an **XL** item because it touches every view and both CI
  platforms.
- **Recommendation:** approve the fork/version choice, then run the migration as
  a separate branch after the P0/P1 safety work is closed. Do not merge a
  partial migration.

## What is actually available

Checked on 2026-11-16:

| Channel | Version | Published | Finding |
|---|---|---|---|
| crates.io `gpui` | 0.2.2 | 2025-10-22 | Current pinned release. No newer `gpui` version exists on crates.io. |
| Zed `main` / `v1.28.1` | `gpui 0.2.2` in its manifest | v1.28.1 on 2026-11-11 | Source-only multi-crate layout: `gpui_platform`, `gpui_macos`, `gpui_apple`, `gpui_linux`, and others. The crate version was not bumped for the newer APIs. |
| crates.io `gpui-ce` | 0.3.0 | 2026-10-20 | Community fork of GPUI aimed at standalone apps. Latest non-yanked release. |
| crates.io `gpui-ce` | 0.2.2 | 2026-08-28 | Earlier fork release; not needed for this plan. |

The important conclusion is that there is **no official `gpui 0.3.x` release to
upgrade to**. Upstream Zed is effectively a git dependency now, while
`gpui-ce 0.3.0` is a normal crates.io release. The plan uses the community fork
as the concrete target and keeps upstream Zed as a fallback, rather than
pretending a version bump exists.

References:

- [current gpui 0.2.2 API](https://docs.rs/gpui/0.2.2/gpui/)
- [gpui-ce 0.3.0 API](https://docs.rs/gpui-ce/0.3.0/gpui/)
- [gpui_ce_platform 0.3.0 API](https://docs.rs/gpui_ce_platform/0.3.0/gpui_ce_platform/)
- [gpui-ce repository](https://github.com/gpui-ce/gpui-ce)
- [gpui-ce changelog](https://github.com/gpui-ce/gpui-ce/blob/main/CHANGELOG.md)
- [upstream porting notes](https://docs.rs/gpui-ce/0.3.0/gpui/_documentation/upstream/index.html)
- [Zed v1.28.1 release](https://github.com/zed-industries/zed/releases/tag/v1.28.1)
- [Zed's gpui manifest at v1.28.1](https://github.com/zed-industries/zed/blob/v1.28.1/crates/gpui/Cargo.toml)
- [Zed's gpui_platform crate](https://github.com/zed-industries/zed/tree/v1.28.1/crates/gpui_platform)

## Current usage

`gpui` is the only UI dependency. The app does not use a component framework
or a webview.

- **Files:** 25 Rust files under `crates/vantage/src` reference `gpui`:
  `app.rs`, `main.rs`, `menu.rs`, `root.rs`, `settings.rs`, `text.rs`,
  `theme.rs`, `icons.rs`, `ui/` (nine files) and `views/` (eight files).
- **Size:** `app.rs` is 4,401 lines; the views are 4,269 lines; `ui/` is 1,376
  lines. The work is concentrated in `app.rs` and the inspector.
- **Declared dependency:**
  - normal: `gpui = "0.2.2"`
  - dev: `gpui = { version = "0.2.2", features = ["test-support"] }`
- **Resolved graph:** `Cargo.lock` pins `gpui 0.2.2` and the matching
  `gpui_* 0.2.2` crates (`gpui_collections`, `gpui_http_client`, `gpui_media`,
  `gpui_refineable`, `gpui_sum_tree`, `gpui_util`, and macros).
- **Platform workarounds:**
  - `text.rs` fits text with `TextSystem::line_wrapper` because 0.2.2 does not
    reliably ellipsize nested text.
  - `ui/input.rs` is a self-contained 522-line single-line editor built on
    `KeyDownEvent`; it handles selection, clipboard and caret movement itself.
  - `chrome.rs` calls AppKit directly because 0.2.2 does not implement
    `Window::start_window_move` on macOS.
- **HTTP:** Vantage deliberately has no HTTP client dependency; avatars are
  fetched with `curl` in `avatar_cache.rs`. The only reason for the
  `[patch.crates-io] xattr` workaround is `gpui_http_client` in the old graph.
- **Tests:** 173 test functions, two of them `#[gpui::test]` UI tests
  (`add_repo_modal.rs`, `settings.rs`).

The custom text and input code is the clearest evidence of the limitations
named in the ticket. It should be deleted or reduced, not ported around.

## Target choice

### Primary: gpui-ce 0.3.0

The migration would use:

```toml
[dependencies]
gpui = { package = "gpui-ce", version = "=0.3.0" }
gpui_platform = { package = "gpui_ce_platform", version = "=0.3.0" }

[dev-dependencies]
gpui = { package = "gpui-ce", version = "=0.3.0", features = ["test-support"] }
```

Pinning the exact version is intentional. `gpui-ce` has yanked 0.3.2 and 0.3.3,
and its repository has 0.4.0-alpha components; `0.3` would be less predictable
than `=0.3.0`.

Why it fits Vantage:

- It is published, versioned and pinned in `Cargo.lock`, unlike a Zed git
  revision.
- It is explicitly maintained for applications outside Zed and ships the
  platform selection that a standalone app needs.
- Its text system is Parley-based and implements truncation in the layout path,
  which directly addresses the reason `text.rs` exists.
- `gpui_ce_elements` 0.3.0 provides `EditableTextState`, `text_input` and
  `text_area`, with selection, undo, clipboard and input-method support. This is
  the natural replacement for the custom field if it is adopted.
- It implements `Window::start_window_move` on macOS, so the custom AppKit drag
  helper can be removed.

Trade-offs:

- This is a community fork, not an official Zed release. Its release cadence,
  future maintenance and API drift are external risks.
- It switches the text backend to Parley on every platform. Measurements and
  glyph fallback can differ from 0.2.2 even when the API compiles.
- The crate names differ from the official package, so every dependency
  reference and the lock file change.

### Fallback: Zed `v1.28.1` as a git dependency

If the project does not want to depend on `gpui-ce`, the same source is
available from Zed's tag:

```toml
gpui = { git = "https://github.com/zed-industries/zed", tag = "v1.28.1" }
gpui_platform = { git = "https://github.com/zed-industries/zed", tag = "v1.28.1" }
```

This has the same application-facing changes (`gpui_platform::application`,
`WindowOptions` background fields, focus/blur context arguments, text changes),
but:

- it is not a crates.io release and cannot be checked against a published
  checksum;
- it pulls a multi-crate workspace from a large repository;
- Zed can move without a semver signal;
- it has not been measured in a spike for Vantage.

It is a valid fallback if the fork is rejected, not the default.

### Rejected: stay on 0.2.2

Staying avoids the migration cost but keeps both limitations that motivated
P3-09: the custom truncation workaround and the custom input field. It also
leaves the app on a dependency snapshot that is now more than a year old. This
is acceptable only as a temporary decision if the fork cannot be approved.

## Risk inventory

Severity is product risk after compiling, not the number of compiler errors.

| ID | Area | What changed | Vantage evidence | Severity | Mitigation |
|---|---|---|---|---|---|
| R1 | App bootstrap | `Application::new()` and `Application::headless()` are gone; use `gpui_platform::application()` / `headless()`. | `main.rs` uses both; headless mode powers `--print-fonts`. | High | Add `gpui_platform`; keep one bootstrap path for normal and headless runs; test both. |
| R2 | Window options | `WindowOptions::window_background` was replaced by platform-specific fields, including `macos_window_background`; `app_owns_titlebar_drag` is new. | `main.rs` sets `WindowBackgroundAppearance::Opaque`; Vantage draws a custom 42px titlebar. | Medium | Set `macos_window_background: MacosWindowBackground::Opaque`; set `app_owns_titlebar_drag: true`; verify traffic lights and drag. |
| R3 | Window focus/blur | `Window::focus` and `Window::blur` now take `&mut App`; `Frame::clear` also takes `&mut App`. | 5 `window.focus(...)` calls, 1 `window.blur()`, 4 test `clear()` calls. | Low | Mechanical context propagation; covered by compile errors. |
| R4 | Text layout | Text now uses Parley. `TextSystem::line_wrapper` and `LineWrapperHandle` are removed; `Font` gained `width`, `TextRun` gained `letter_spacing`; truncation moved into the text element. | `text.rs` uses `line_wrapper`; 19 `.truncate()` calls; one custom `ellipsize` in the commit list. | High | Delete the custom shaper; use `Styled::truncate` / `text_ellipsis_start` / `text_ellipsis_middle`; compare long paths, commit subjects and CJK/emoji strings. |
| R5 | Text input / IME | The core still exposes key events; `gpui_ce_elements` adds a real editable-text state with selection, undo, clipboard and IME. The old custom field has no IME composition path. | `ui/input.rs` is 522 lines and feeds on `KeyDownEvent`; all modal fields use it. | Medium–High | Keep the existing field for the first merge if needed; adopt `EditableTextState` in a separate phase and test typing, paste, selection, undo and IME. |
| R6 | Assets | `Application::with_assets` now takes `impl Into<AssetRegistry>`; `AssetSource` still works through a conversion; `AssetRegistry` adds preloaded/on-demand entries. | `icons.rs` implements `AssetSource` for 25 SVGs and one PNG; `main.rs` passes `Assets`. | Low | Keep the existing `AssetSource` initially; add a load test for an icon and the PNG. |
| R7 | Actions and menus | `Menu` gained a `disabled` field. `actions!`, `on_action`, `KeyBinding` and menu item constructors still compile. | 5 menu literals in `menu.rs`; 23 `KeyBinding::new` calls; `actions!` with 28 actions. | Low | Add `disabled: false` or switch to `Menu::new(...).items(...)`; no keybinding rewrite expected. |
| R8 | Test support | `#[gpui::test]`, `TestAppContext`, `add_window_view`, `simulate_click` and `debug_bounds` still work; `draw(cx).clear()` became `draw(cx).clear(cx)`. | Two GPUI tests; the rest are pure Rust tests. | Low | Update four call sites; keep test-support only as a dev-dependency. |
| R9 | HTTP client | The old graph pulled `gpui_http_client` and `zed-async-tar`, which required the vendored `xattr` patch. `gpui-ce` has no HTTP client and Vantage uses `curl`. | `Cargo.toml` `[patch.crates-io] xattr`; `avatar_cache.rs` uses `curl`. | None | Remove the patch and `vendor/xattr` after confirming `cargo tree -i xattr` is empty; do not add an HTTP dependency. |
| R10 | macOS chrome | `Window::start_window_move` is implemented by the new macOS backend, so the AppKit workaround is no longer needed. | `chrome.rs` exists only because 0.2.2 did not implement it; `topbar.rs` calls it from a mouse-move handler. | Medium | Delete `chrome.rs`; call `window.start_window_move()`; set `app_owns_titlebar_drag`; manual drag and double-click test. |
| R11 | Build and toolchain | `gpui-ce 0.3.0` requires Rust 1.95 and edition 2024; its lock graph adds Parley, AccessKit, WGPU/GL stack pieces and replaces the old blade/cosmic-text/hyper path. | The repository currently builds on stable with `gpui 0.2.2`; CI uses stable Rust. | Medium | Check `rustc --version` on every CI runner; update the Linux package list if Parley/fontconfig/accesskit needs differ; keep `--locked`. |
| R12 | Cross-platform behavior | The new platform crate chooses macOS, Linux, Windows, web, iOS or Android backends explicitly; only macOS and Linux are in CI today. | `gpui_ce_platform` is a new direct dependency; macOS titlebar and Linux font fallback are the visible surfaces. | Medium | Run the existing Ubuntu and macOS jobs; add a headless smoke run on both; treat Windows as unsupported until it is built and tested. |
| R13 | Fork governance | `gpui-ce` is a community fork; versions can be yanked and the project can diverge from Zed. | The only newer published line is the fork. | Medium | Pin `=0.3.0`, commit the lock, record the source/checksum, review the changelog before each bump, and keep the Zed git fallback documented. |
| R14 | Visual regression | Font metrics, line height, truncation and native window chrome can change without a compiler error. | 19 truncation sites, custom 42px header, dense tables and a diff pane. | High | Screenshot matrix in light/dark at 1440×900 and 1040×640; compare before/after; inspect the inspector, commit list and modals manually. |

The compile-time work is not the hard part. R4, R5 and R14 are the actual
schedule risk.

## Compile-spike evidence

A temporary copy of the workspace was upgraded to `gpui-ce 0.3.0` in
`/tmp` (not in this repository, not committed) to measure the breakage. No
application behaviour was changed in the worktree.

Result before fixes:

- `cargo check -p vantage`: **23 errors**.
- `cargo check -p vantage --tests`: **4 additional errors**.

The errors were concentrated in nine source files:

| Change | Sites |
|---|---|
| `Application::new` / `headless` -> `gpui_platform::*` | 2 |
| `on_window_closed` callback and window options | 2 |
| `Window::focus` / `blur` context arguments | 6 |
| `Frame::clear(cx)` in tests | 4 |
| `Menu { ... }` missing `disabled` | 5 |
| `flex_shrink()` now needs a factor | 3 |
| `Font.width`, `TextRun.letter_spacing`, `line_wrapper` | 3 |
| `Rgba` field `a` -> `alpha` | 1 |
| `Entity::update(...).ok()` return-type change | 1 |

After those 27 edits:

- `cargo check -p vantage --tests` passes.
- `cargo test --workspace` passes: **64 `vantage` tests + 178
  `vantage-core` tests** (242 total).
- `cargo run -p vantage -- --print-fonts` starts the headless platform and
  prints `Public Sans`, `Plus Jakarta Sans` and `JetBrains Mono`, confirming
  that bundled fonts still register through the new text system.
- The custom `ellipsize` was stubbed with a pass-through for this compile
  spike, so its visual behaviour was **not** validated. That is deliberately
  left to Phase 3.

This is evidence that the API port is bounded and mostly mechanical. It is not
evidence that the UI looks or feels the same.

## Phased migration plan

### Phase 0 — Baseline and decision gate (0.5 day)

- Create `spike/gpui-0.3` from `main`; do not reuse the P3-09 planning branch
  for implementation.
- Record a baseline: `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets --locked -- -D warnings`,
  `cargo test --workspace --locked`, and the macOS package job.
- Capture light/dark screenshots with `VANTAGE_STATE` for overview, attention,
  changes, inspector, modals, settings, tooltips and toast.
- Approve the fork/version choice or select the Zed git fallback before
  changing manifests.
- Add the exact pins and `gpui_platform`; update `Cargo.lock` once and commit it.

Exit criteria: baseline is green, screenshots exist, target version is approved,
and `cargo tree -i gpui-ce` resolves to one version.

### Phase 1 — Bootstrap and window lifecycle (0.5 day)

- Replace `Application::new()` with `gpui_platform::application()`.
- Replace `Application::headless()` with `gpui_platform::headless()`.
- Update `on_window_closed` to the two-argument callback.
- Replace `window_background` with the macOS-specific field; explicitly set
  `app_owns_titlebar_drag` for the custom titlebar.
- Remove the now-unused `Application` import and `WindowBackgroundAppearance`
  import.

Exit criteria: app launches from `cargo run`, `--print-fonts` works, window
bounds and traffic lights are unchanged, and the app quits on the last window
close.

### Phase 2 — Mechanical API and style fixes (0.5 day)

- Propagate `cx` to `window.focus(...)` and `window.blur(...)`.
- Use `.flex_shrink_1()` where the old code used `.flex_shrink()`; use
  `.flex_shrink(0.0)` or `.flex_shrink_0()` where a pinned child was intended.
- Add `disabled: false` to `Menu` literals or switch them to `Menu::new`.
- Add `width: Default::default()` to `Font` literals and
  `letter_spacing: None` to `TextRun` literals.
- Change `Rgba { a, ..color }` to the palette field `alpha`.
- Update test `clear()` calls to pass `cx`.
- Remove the obsolete `.ok()` on the changed entity-update return.

Exit criteria: `cargo check --workspace --all-targets` is clean and no UI code
has been intentionally restyled.

### Phase 3 — Text and truncation (1–2 days)

- Delete the `line_wrapper` implementation in `text.rs`; use the styled text
  truncation APIs instead:
  - `.truncate()` for single-line rows;
  - `.text_ellipsis_start()` for paths or suffixes;
  - `.text_ellipsis_middle()` where both ends matter.
- Add a regression test for the one helper that remains (if any): long ASCII,
  multi-byte text, emoji, empty text and a width below one pixel.
- Inspect all 19 `.truncate()` sites, especially repository paths, commit
  subjects, branch names and the diff header.
- Compare screenshots against the baseline and correct column widths or line
  heights if Parley measures differently.

Exit criteria: no custom shaper remains, long text is ellipsized without
clipping or panics, and the screenshot diff is reviewed.

### Phase 4 — Text input decision (0.5 day to decide, 2–4 days to adopt)

The first merge can keep `ui/input.rs`; it compiles and its unit tests pass.
The ticket's input limitation is only fully addressed if the app adopts
`gpui_ce_elements`:

- Add `gpui_ce_elements = "=0.3.0"`.
- Replace `FieldState` with `Entity<EditableTextState>` and use `text_input`
  or `text_area`.
- Install `default_bindings()` and keep Enter/Escape submission behaviour.
- Move placeholder, focus, selection and caret styling to the new field.
- Test typing, select-all, copy/paste, undo/redo, IME composition and focus
  changes with `TestApp`/`TestAppWindow` or the existing `TestAppContext`.

If this is not funded now, record it as a follow-up and do not claim that P3-09
fixed input. Keeping the custom field is an explicit scope decision.

Exit criteria: all modal fields work with the new field, or the decision to
defer is documented and the old field still passes its tests.

### Phase 5 — macOS window chrome (0.5 day)

- Delete `chrome.rs` and its AppKit drag implementation.
- Call `window.start_window_move()` from the existing header mouse-move
  handler.
- Set `app_owns_titlebar_drag: true` in `WindowOptions` and verify the
  double-click/titlebar behaviour.
- Keep `objc2` for `app_icon.rs` and the About panel; do not remove the
  dependency wholesale.

Exit criteria: dragging the header moves the window, double-click zooms as
before, and traffic-light placement is unchanged.

### Phase 6 — Assets, tests and CI (1 day)

- Confirm the `AssetSource` conversion still loads every SVG and PNG.
- Remove `[patch.crates-io] xattr` and `vendor/xattr` only after
  `cargo tree -i xattr` is empty; this is a cleanup, not a prerequisite.
- Update or add tests for the changed call sites.
- Run the full existing CI matrix, including the universal macOS package job.
- Check the Linux job with the Parley/fontconfig/accesskit dependency set.

Exit criteria: both CI platforms pass, the package job produces a valid DMG,
and `cargo test --workspace --locked` is green.

### Phase 7 — Release validation and cleanup (1 day)

- Build both Apple targets and package with `scripts/bundle-macos.sh`.
- Smoke-test the universal app: launch, fonts, theme switching, window drag,
  resizing, menus, modals, repository scan and commit flow.
- Update `docs/ARCHITECTURE.md` and `crates/vantage/README.md` if `text.rs`,
  `chrome.rs` or the dependency description changed.
- Write a follow-up note for any deferred input work.

Exit criteria: release artifact verified and documentation is accurate.

## Test and CI strategy

### Gates

The migration is mergeable only when all of these pass on the migration branch:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build -p vantage --locked
./scripts/bundle-macos.sh
```

The last two run in the existing CI jobs. Do not weaken `--locked`; a stale
lock is exactly the kind of regression this change can hide.

### Automated coverage

- Existing 242 tests must stay green. They cover input state, theme selection
  and the two GPUI modal interaction tests.
- Add a small text test for any helper retained after Phase 3.
- If `gpui_ce_elements` is adopted, add tests for typing, selection, paste,
  undo and IME. The new test API exposes `simulate_input` and
  `simulate_keystrokes`, which are a better fit than raw `KeyDownEvent`
  construction.
- Keep the two existing GPUI tests using `TestAppContext` initially; migrate
  them to `TestApp` only if it reduces boilerplate or fixes a real limitation.

### Visual matrix

Use the existing `VANTAGE_STATE` hook to capture the same states before and
after:

- light and dark;
- 1440×900 and the 1040×640 minimum;
- overview, attention, changes and inspector;
- add-repository, clone, new-repository, branch and stash modals;
- settings, toast, tooltip and context menu;
- long repository paths, long commit subjects, unicode and emoji.

Compare font metrics, truncation, line heights, focus rings, selection
highlighting and the 42px header. The new text backend is allowed to produce
different pixel widths; it is not allowed to clip text or change the layout
hierarchy.

### Manual checks

- macOS: launch, drag the header, double-click the header, zoom/minimize,
  traffic lights, full screen, native menus, About panel.
- Input: click into every modal field, type, select all, copy, paste, undo,
  Escape, Enter and tab order.
- Assets: icons in the sidebar, toolbar, status cells and modals; the avatar
  fallback and the PNG app icon.
- Packaging: universal binary, codesign verification, DMG mount and checksum.

## Rollback plan

The migration has no persisted-state changes: `state.json`, repository data and
Git operations are untouched. A rollback is therefore a code and dependency
rollback, not a data migration.

- Keep the migration on `spike/gpui-0.3`, with one commit per phase.
- Do not merge until the Phase 7 gate passes. A reviewer can then revert the
  phase commits individually.
- For a full rollback, restore `Cargo.toml` and `Cargo.lock` from `main` and
  revert the source commits. `git revert` is preferred over resetting a shared
  branch.
- Retain the old `vendor/xattr` directory and `[patch.crates-io]` block until
  the migration is accepted; removing them is the last cleanup step.
- Pin `gpui-ce = "=0.3.0"` and commit the checksum-bearing lock. If the release
  is yanked or the fork is abandoned, switch to the documented Zed `v1.28.1`
  fallback or return to `0.2.2`.
- If only the input migration causes trouble, revert that phase and keep the
  rest; it is deliberately isolated.

Rollback triggers: any data loss, inability to launch, broken titlebar drag or
menus on macOS, a Linux CI failure that cannot be fixed within the phase, or a
truncation/input regression that cannot be covered by a test.

## Estimated scope

| Work | Files | Estimate |
|---|---|---|
| Manifest, lock and platform crate | `Cargo.toml`, `Cargo.lock` | 0.5 day |
| Bootstrap and window options | `main.rs` | 0.5 day |
| Mechanical API fixes | `app.rs`, `menu.rs`, `theme.rs`, `text.rs`, `views/topbar.rs`, `views/inspector.rs`, tests | 0.5–1 day |
| Text and truncation | `text.rs`, view row styles, screenshots | 1–2 days |
| Input adoption (optional) | `ui/input.rs`, modal fields, tests | 2–4 days |
| Window chrome cleanup | `chrome.rs`, `topbar.rs`, `main.rs` | 0.5 day |
| CI, packaging and docs | `.github/workflows`, `scripts`, `docs`, crate README | 1 day |
| Review buffer | — | 1–2 days |

Expected code touch points: about **9 source files**, plus `Cargo.toml`,
`Cargo.lock` and one deleted module. The compile spike changed 27 call sites;
the production patch is expected to be in the same order of magnitude.

Dependency impact measured in the spike:

- current graph: 750 `Cargo.lock` packages;
- target graph: 769 packages (net +19);
- about 204 old entries removed and 223 new entries added, because the new
  platform/text/render stack replaces the old `blade`, `cosmic-text` and HTTP
  path;
- `gpui 0.2.2` and its `gpui_* 0.2.2` crates disappear;
- `gpui_ce_* 0.3.0`, Parley, AccessKit and WGPU-related crates appear;
- the old `xattr` patch becomes unused.

The mechanical migration is a **few days**. The full item is **XL** because it
requires visual review on both platforms and a product decision about input.

## Open questions and residual concerns

1. **Fork approval.** Is the project willing to depend on `gpui-ce` rather than
   Zed's official crate? This is the only decision that can invalidate the
   target; the fallback is the Zed git tag.
2. **Text metrics.** Parley can change wrapping, fallback and line heights.
   Screenshots are mandatory; unit tests cannot catch a clipped label.
3. **Input scope.** Keeping the custom field is a valid first merge, but it does
   not fix IME or the ticket's input limitation. Decide before Phase 4.
4. **Linux.** The spike was run on macOS only. The Ubuntu CI job must prove
   that the new Parley/AccessKit/X11/Wayland dependencies link and that bundled
   fonts are found.
5. **macOS chrome.** The native `start_window_move` implementation must be
   checked with the custom 42px header and the new `app_owns_titlebar_drag`
   flag; this is not covered by `cargo test`.
6. **Supply chain.** The lock graph churn is large even though the net package
   count is only +19. Review the new crates and record the exact versions in the
   migration PR.
7. **Official upstream.** If Zed publishes a real `gpui 0.3.x` before this work
   starts, re-run the spike against it. The migration shape should be similar,
   but the target should be reconsidered.

## References

- [GPUI 0.2.2 documentation](https://docs.rs/gpui/0.2.2/gpui/)
- [gpui-ce 0.3.0 documentation](https://docs.rs/gpui-ce/0.3.0/gpui/)
- [gpui_ce_platform 0.3.0 documentation](https://docs.rs/gpui_ce_platform/0.3.0/gpui_ce_platform/)
- [gpui-ce repository](https://github.com/gpui-ce/gpui-ce)
- [gpui-ce changelog](https://github.com/gpui-ce/gpui-ce/blob/main/CHANGELOG.md)
- [GPUI-CE upstream porting notes](https://docs.rs/gpui-ce/0.3.0/gpui/_documentation/upstream/index.html)
- [GPUI-CE text guide](https://docs.rs/gpui-ce/0.3.0/gpui/_documentation/guides/text/index.html)
- [GPUI-CE text input guide](https://docs.rs/gpui-ce/0.3.0/gpui/_documentation/guides/text_input/index.html)
- [GPUI-CE WindowOptions](https://docs.rs/gpui-ce/0.3.0/gpui/struct.WindowOptions.html)
- [Zed v1.28.1 release](https://github.com/zed-industries/zed/releases/tag/v1.28.1)
- [Zed gpui manifest](https://github.com/zed-industries/zed/blob/v1.28.1/crates/gpui/Cargo.toml)
- [Zed gpui_platform crate](https://github.com/zed-industries/zed/tree/v1.28.1/crates/gpui_platform)
