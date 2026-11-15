# ADR 0002 — Scope and Platform Decisions (localization, notifications, LFS, platform)

- **Status:** Accepted
- **Date:** 2026-10-19
- **Ticket:** ALPH-167 (spec: `docs/COMPARISON_GITHUB_DESKTOP.md` §4, item 24)
- **Scope:** Documentation of decisions about existing code. No application code changed.

This ADR records the reasoning behind four closely related scope decisions and a concrete
platform-support assessment. Every claim below is backed by a file inspected in this repo;
file paths are cited inline.

---

## Decision area 1 — Localization / i18n

### Context (current state)

- **No i18n infrastructure exists.** `package.json` dependencies and devDependencies contain
  no `svelte-i18n`, `@formatjs/*`, `i18next`, `typesafe-i18n`, `fluent`, or equivalent. There is
  no `locales/`, `i18n/`, or `lang/` directory anywhere under `src/`.
  - Inspected: `src-tauri/tauri.conf.json`, `package.json`, `src/lib/format.ts`, `src/lib/stores/ui.ts`.
- A case-insensitive grep for `i18n|locale|svelte-i18n|intl|translat` across `src/` and
  `package.json` returns **only false positives**:
  - CSS `translateX(...)`/`translateY(...)` in `ToastContainer.svelte` and `Checkbox.svelte`.
  - `toLocaleDateString(undefined, …)` and `toLocaleString()` in `src/lib/format.ts` — these are
    locale-aware *date formatting*, not string translation.
- **UI strings are hardcoded English literals**, spread across two places:
  - Component markup: **35 `.svelte` files** (~12,434 lines). A grep for English text nodes
    (`>Word …<`) yields ~**41** occurrences, and `aria-label`/`title`/`placeholder`/`alt`
    attributes yield ~**85** occurrences — i.e. **~126 user-visible/assistive strings** in markup alone
    (this excludes toast copy, empty states built from expressions, and dynamic text).
  - TypeScript literals: relative-time strings (`"just now"`, `"yesterday"`, `` `${n}m ago` ``,
    etc.) in `src/lib/format.ts`; toast messages in `src/lib/updater.ts`; default copy in
    `src/lib/stores/ui.ts`.
- Git subprocess output is forced to English: `runner.rs` sets `LC_ALL=C`
  (`src-tauri/src/git/runner.rs`), so raw git errors surface in English regardless of OS locale.

### Decision

**Skip localization. Keep Vantage English-only for now.** Do not introduce an i18n runtime,
extraction pipeline, or locale switcher as part of the current roadmap.

### Alternatives considered

1. **Introduce `svelte-i18n` now and extract all strings.** Rejected: premature. The P0 backlog
   (`docs/COMPARISON_GITHUB_DESKTOP.md` §4 items 1–7) will restructure or rewrite large parts of
   the review UI (multi-pane, range header, context menus, histories). Extracting strings before
   the UI stabilizes means extracting the same strings again immediately after.
2. **RTL-only or a single translated locale (e.g. one non-English language).** Rejected: adds the
   same runtime/selector/formatting cost with none of the strategic benefit; no non-English
   audience is asking yet.
3. **Extract strings without a runtime (prepare for later).** Rejected as busywork: a partial
   extraction that is not enforced by tooling will rot.

### Consequences

- **Positive:** no runtime dependency, no bundle/locale-loading complexity, no selector UI, no
  translation-maintenance burden. Keeps the Tauri binary lean (a stated product value — README
  "small Tauri binary").
- **Negative:** every new string is English-only and increases the eventual retrofitting surface.
- **Cost of retrofitting later vs now:** retrofitting *later* is cheaper overall because the
  string inventory will be stable. The real cost is unavoidable either way: (a) extract ~126+
  markup strings + TS literals, (b) add a runtime, (c) add locale persistence/selector,
  (d) translate, (e) update the **7 unit-test files** in `tests/` if any assert formatted output
  (e.g. `tests/format.test.ts`). Doing it now pays that cost against a moving UI; doing it later
  pays it once. The sunk cost of skipping now is low; the risk of doing it now is churn.

### Revisit trigger

Revisit when **either** (a) a translation request / non-English user base appears, or (b) the P0
review-loop work in §4 items 1–7 is complete and the UI string inventory has stopped changing.
At that point, prefer the **smallest viable** path: extract to a single messages file and evaluate a
lightweight runtime, rather than adopting a heavy framework.

---

## Decision area 2 — Notifications (in-app toasts vs OS notifications)

### Context (current state)

- An **in-app toast system already exists and is complete**:
  - Store/model: `src/lib/stores/ui.ts` — `ToastMessage` (`info | error | success`, optional
    action + `actionLabel` + `details`), `addToast` (max 3 stacked, 5 s / 8 s auto-dismiss),
    `removeToast`, `pauseToastTimer`, `resumeToastTimer`.
  - Renderer: `src/lib/components/common/ToastContainer.svelte` — accessible container
    (`role="status"`, `aria-live="polite"`; each item `role="alert"`).
  - Consumers: `src/lib/updater.ts` (update available / downloading / installed / errors),
    `SettingsDialog.svelte`, error banners in dialogs.
- **No OS / native notifications exist:**
  - `src-tauri/capabilities/default.json` grants only `core:default`, `updater:default`,
    `process:default` — **no `notification:*` permission**.
  - No `@tauri-apps/plugin-notification` in `package.json`; no `tauri-plugin-notification` in
    `src-tauri/Cargo.toml`.
  - No Web `Notification` API usage (`requestPermission`, `new Notification`) anywhere in `src/`.
    (`objc2-user-notifications` appears in `Cargo.lock` only as a transitive dependency; it is not
    wired by this app's code.)

### Decision

**Keep in-app toasts as the only notification channel. Do not add OS notifications.**

### Alternatives considered

1. **Add `tauri-plugin-notification` for background events** (fetch finished, auto-fetch results,
   long-running operation done). Rejected for now: Vantage's workflow is that the user is
   *already looking at the app* while reviewing; there is no background scheduler today, and
   §4 already flags that `autoFetchMinutes` "has no scheduler consumer."
2. **OS notifications to replace toasts.** Rejected: toasts are richer (actions, detail text,
   pause-on-hover), in-context, and require no OS permission prompts.

### Consequences

- **Positive:** zero per-OS permission prompts, zero new plugin/permission surface, no risk of
  notification spam. Toasts already meet all current needs.
- **Negative:** operations that finish while the window is unfocused/backgrounded produce no
  signal. Today that is acceptable because nothing runs in the background.
- **Cost if adopted later:** small-to-moderate — add the plugin + crate, add `notification:*` to
  the capability, handle macOS/Windows/Linux permission grants, and add a user preference toggle.
  Roughly half a day plus per-OS permission UX. Risk is low.

### Revisit trigger

Revisit **only if** a background feature lands that can complete without the user watching —
e.g. scheduled auto-fetch, a long-running fetch/pull, or CI/PR polling (currently explicit
non-goals per §5). At that point, add OS notifications **alongside** toasts, never instead of them.

---

## Decision area 3 — Git LFS

### Context (current state) — what actually happens with an LFS repo

- **The app shells out to the system `git` CLI** and never bundles or calls git-lfs.
  - `src-tauri/src/git/runner.rs`: `Command::new("git")` with `LC_ALL=C`,
    `GIT_TERMINAL_PROMPT=0`, `GIT_PAGER=cat`, and `GIT_OPTIONAL_LOCKS=0` for reads.
  - A grep for `lfs` across `src/` and `src-tauri/src/` returns **no matches**: the app never
    invokes `git lfs` and has no LFS-specific handling.
- **Content is read back through git plumbing that does not smudge by default:**
  - Read-only viewer / single-file content: `git show <rev>:<path>` and `git cat-file -s`
    (`src-tauri/src/git/blob.rs`, `get_blob_text`; also `diff.rs` around line 480).
  - Commit and range file lists: `git show --numstat`/`--name-status` and
    `git diff <from> <to> --numstat`/`--name-status` (`src-tauri/src/git/diff.rs`).
  - Working-tree changes: `git diff … HEAD` (`src-tauri/src/git/diff.rs`).
- **Consequence for an LFS-tracked file:** the object stored in git is the ~130-byte *pointer
  text* (`version https://git-lfs.github.com/spec/v1 …`). Git's clean/smudge filters run at
  checkout/add; `git show <rev>:<path>` does **not** smudge, so the read-only code viewer and
  commit-to-commit diffs will display the **pointer text**, not the real content. Working-tree
  diffs (`git diff HEAD`) usually still behave because git cleans the on-disk file back to the
  pointer for comparison — but if git-lfs is not installed, the on-disk file is the pointer
  anyway. The existing 5 MB `too_large` cap is moot for pointers.
- Nothing is corrupted: the app is read-only for file content, so LFS correctness is a
  **display-fidelity** issue, not a data-safety issue.

### Decision

**Do not add LFS support.** Accept the known limitation: LFS-tracked files render as pointer text
in the viewer/diffs. Document it; do not block on it.

### Alternatives considered

1. **Detect LFS via `.gitattributes` (`filter=lfs`) and run `git lfs smudge` / `git cat-file`
   with the filter** to fetch and display real content. Deferred: requires git-lfs to be installed
   by the user, adds a network fetch path, and adds large-binary rendering concerns — none of
   which the solo-dev review audience needs.
2. **Show a "Git LFS file — open externally" placeholder instead of pointer text.** Cheap
   (works without git-lfs installed), but still requires detection logic; treated as a
   *revisit* task, not current scope.
3. **Status quo (pointer text shown).** Chosen for now, with the limitation documented.

### Consequences

- **Positive:** no dependency on git-lfs, no network fetch during review, no large-blob handling.
- **Negative:** repos using LFS (media/game/ML asset repos) will show confusing pointer text in
  the viewer and commit diffs. This is a documented fidelity gap, not a correctness/safety bug.

### Revisit trigger

Revisit if the audience shifts toward repos that store binaries via LFS (media, ML/datasets,
game assets), or if user reports show pointer text in the viewer is a real friction point. Cheapest
first step is the detection + "open externally" placeholder (option 2), not full smudge.

---

## Decision area 4 — Platform scope (support, auto-update, signing)

### Context (current state) — evidence inspected

**Build targets** — `src-tauri/tauri.conf.json`:
- `bundle.active: true`, `bundle.targets: "all"`, `bundle.createUpdaterArtifacts: true`.
- Icons include `icon.icns` (macOS) and `icon.ico` (Windows).
- Window uses macOS-specific chrome (`titleBarStyle: "Overlay"`, `hiddenTitle`, `trafficLightPosition`).

**Release pipeline** — `.github/workflows/release.yml` (trigger: `v*` tags):
- Matrix: `macos-latest` with `--target universal-apple-darwin`; `ubuntu-22.04`; `windows-latest`.
- Uses `tauri-apps/tauri-action@v0`, `releaseDraft: true`, and passes only
  `TAURI_SIGNING_PRIVATE_KEY` (the **update** signing key) plus `GITHUB_TOKEN`.
- **No OS code-signing or notarization configuration**: no `APPLE_CERTIFICATE`,
  `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_TEAM_ID`, `WINDOWS_CERTIFICATE`,
  `signingIdentity` (absent from `tauri.conf.json`), no `signCommand`, no notarize step.
- CI (`.github/workflows/ci.yml`) runs `npm run check`, `npm test`, `cargo fmt`, `cargo test`,
  `cargo clippy -D warnings` on `ubuntu-latest` + `macos-latest` (no Windows CI job).

**Updater** — wired end-to-end for in-app updates:
- Rust: `src-tauri/src/lib.rs` registers `tauri_plugin_updater` and `tauri_plugin_process`.
- Capability: `src-tauri/capabilities/default.json` grants `updater:default` + `process:default`.
- Config: `tauri.conf.json` `plugins.updater` has a `pubkey` and endpoint
  `https://github.com/beingbalveer/Vantage/releases/latest/download/latest.json`.
- Frontend: `src/lib/updater.ts` uses `check()` → `downloadAndInstall` → `relaunch()`, surfaced
  in `SettingsDialog.svelte` (manual "About") and `App.svelte` (startup check).

**README install instructions** — `README.md`:
- macOS: Homebrew cask recommended ("no security prompts"); DMG documented with an explicit
  **Gatekeeper workaround** (`right-click → Open` or `xattr -cr /Applications/Vantage.app`) —
  direct evidence that macOS builds are **unsigned/un-notarized**.
- Windows: "Download and run the `.msi` or `.exe`" (no signing claim → SmartScreen warning).
- Linux: "Download the `.deb` package or `.AppImage`".

**Issue template** confirms intended platform coverage: macOS (Intel + Apple Silicon),
Windows 10/11, Linux (Ubuntu/Debian and Arch/Fedora/other) — `.github/ISSUE_TEMPLATE/bug_report.yml`.

### Decision

Support **macOS, Windows, and Linux**, all delivered as **unsigned** builds from GitHub
Releases. Keep **in-app auto-update** as the update mechanism where the platform permits it.
Accept the current install-time friction (Gatekeeper / SmartScreen / manual Linux package
upgrade) and treat closing those gaps as **post-core-loop** work (see §5 below).

### Platform support matrix (as-built)

| Platform | Artifacts | Architectures | Signed? | In-app auto-update? | User install friction |
|---|---|---|---|---|---|
| **macOS** | `.dmg` (+ `.app`) | Universal (Intel + Apple Silicon) | **No** (unsigned, un-notarized) | Yes (via updater) | Gatekeeper block; `right-click → Open` or `xattr -cr`; Homebrew cask avoids prompts |
| **Windows** | `.msi`, NSIS `.exe` | x64 | **No** | Yes (updater, NSIS/MSI) | SmartScreen "unknown publisher" warning |
| **Linux** | `.deb`, `.rpm`, `.AppImage` | x64 | N/A | **Only `.AppImage`** | `.deb`/`.rpm` users must upgrade via their distro mechanism / re-download |

**Auto-update is not universal:** macOS and Windows builds can self-update through the Tauri
updater; on Linux **only the AppImage** consumes the updater endpoint. `.deb`/`.rpm` installs are
outside the distro's package channel and must be upgraded manually (the app cannot safely
self-replace a package-managed install).

### Alternatives considered

1. **Ship macOS signed + notarized now.** Deferred: requires Apple Developer Program membership
   and CI secrets; better sequenced after the core review loop (P0) is credible.
2. **Add Windows code signing now.** Deferred: certificate cost and (for EV) identity-validation
   lead time; SmartScreen warning is survivable for an early solo-dev tool.
3. **Drop Linux or drop per-platform packaging.** Rejected: Linux support is an explicit
   advantage over GitHub Desktop (§3 of the comparison doc).
4. **Add a Linux apt/rpm repo or Flathub for managed updates.** Deferred: high ongoing effort for
   a small audience; document manual upgrades instead.

### Consequences

- **Positive:** broad coverage (3 OSes, Intel+ARM Macs) for low cost; no certificate spend;
  auto-update works for the majority (mac + AppImage + Windows).
- **Negative:** first-run trust friction on macOS (Gatekeeper) and Windows (SmartScreen) directly
  hurts adoption; `.deb`/`.rpm` users get no in-app update. These are the main credibility gaps
  once the product loop is solid.

### Revisit trigger

Revisit signing when the review loop is complete and distribution/adoption becomes the
bottleneck, or at the first external release meant for non-developers. Revisit the Linux update
path only if `.deb`/`.rpm` usage proves material.

---

## 5. If scope changes — prioritized order of work

Ranked by trust/adoption leverage per unit cost. None of these are required for the core review
loop; all are "close the gap after the product is credible."

1. **macOS code signing + notarization (highest leverage).**
   Add Apple Developer Program membership, a Developer ID Application certificate, and CI secrets
   (`APPLE_CERTIFICATE`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`/`APPLE_TEAM_ID` or App Store Connect
   API key); set `signingIdentity` + notarize in the Tauri build. Removes the Gatekeeper dialog
   documented in `README.md` and is the single biggest first-run conversion win.
   *Cost:* ~$99/yr + ~half a day of CI work. *Risk:* medium (secret handling, cert rotation).

2. **Windows code signing.**
   Acquire an OV/EV code-signing certificate (or use Azure Trusted Signing) and add
   `WINDOWS_CERTIFICATE` + timestamping to `release.yml`. Removes the SmartScreen warning.
   *Cost:* ~$200–$600/yr + ~half a day. *Risk:* medium (cert identity validation lead time).

3. **Linux update path.**
   Either document manual `.deb`/`.rpm` upgrades clearly, or publish to a package channel
   (apt/rpm repo or Flathub) so the majority of Linux users stop being manual. Prefer
   documentation first; packaging channels are a larger ongoing commitment.
   *Cost:* docs ≈ negligible; a real repo ≈ days + upkeep. *Risk:* low–high depending on option.

4. **OS notifications** (only if background work lands: scheduled auto-fetch, CI/PR polling).
   Add `tauri-plugin-notification` + `notification:*` capability + per-OS permission handling +
   a preference toggle, alongside (not replacing) toasts. *Cost:* ~half a day. *Risk:* low.

5. **Git LFS display fidelity.**
   Start cheap: detect `filter=lfs` in `.gitattributes` and show an "open externally" placeholder
   instead of pointer text; only then consider `git lfs smudge` for real content.
   *Cost:* placeholder ≈ 1 day; full smudge ≈ several days + new failure modes. *Risk:* medium.

6. **Localization / i18n (last).**
   Only after the UI string inventory stabilizes (P0 items 1–7 done) and a non-English audience
   exists. Extract once, add a lightweight runtime, add a locale selector, translate.
   *Cost:* days–weeks + ongoing translation upkeep. *Risk:* medium (churn if done early).

---

## Files inspected (evidence index)

- `docs/COMPARISON_GITHUB_DESKTOP.md` (§3, §4 item 24, §5)
- `src-tauri/tauri.conf.json` (bundle targets, updater plugin, absence of `signingIdentity`)
- `.github/workflows/release.yml`, `.github/workflows/ci.yml`
- `.github/ISSUE_TEMPLATE/bug_report.yml`
- `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` (dependency search)
- `src-tauri/capabilities/default.json`
- `src-tauri/src/lib.rs` (plugin registration)
- `src-tauri/src/git/runner.rs`, `blob.rs`, `diff.rs`, `status.rs`, `ops.rs` (git invocation, `git show`, LFS grep)
- `src/lib/updater.ts`, `src/lib/stores/ui.ts`, `src/lib/components/common/ToastContainer.svelte`
- `src/lib/format.ts`, `src/lib/keys.ts`, `src/lib/components/Dialogs/SettingsDialog.svelte`
- `package.json`, `README.md`, `docs/decisions.md`
- `src/**/*.svelte` (35 files) via string greps; `tests/*.ts` (7 files)
