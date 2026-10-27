# Vantage: Design Specification

> Companion to `PRODUCT_SPEC.md`. This file defines **how it looks, feels, and behaves** at the pixel and interaction level. Where this file and the product spec disagree on behavior, the product spec wins; where they disagree on visuals, this file wins.

---

## 1. Design philosophy

**Reference point: GitHub Desktop's calm.** Flat, quiet, generous whitespace, one clear primary action per area, no toolbars full of icons. Vantage adds multi-repo and review ideas **without adding visual noise**.

1. **Quiet by default, informative at a glance.** The sidebar and commit list carry the signal (badges, dots, flags). Everything else recedes.
2. **One accent color (blue)** means "new / needs your attention / primary action". Amber means "uncommitted / caution". Red is only for deletions and destructive actions. Green is only for additions and "working/active".
3. **Content over chrome.** The diff gets the most space. Panels collapse. No decorative borders; use 1px hairlines and background tints.
4. **Keyboard-first, mouse-friendly.** Every list is navigable with `j/k`. Hints appear in tooltips and the palette, not as permanent labels.
5. **Density:** compact but not cramped. Base text 13px. Row heights fixed (virtualization requires it).
6. **Both themes are first class.** Follow the OS by default; no flash of wrong theme at startup.
7. **No animation for its own sake.** Motion only to explain a change (new commit arriving, panel sliding). Respect `prefers-reduced-motion`.

---

## 2. Design tokens

Define as CSS custom properties in `src/styles/tokens.css`; themes override in `themes.css` via `:root[data-theme="light"|"dark"]`. Components must **only** use tokens, never raw hex.

### 2.1 Color: light

| Token | Value | Use |
|---|---|---|
| `--bg` | `#ffffff` | Main surfaces (diff, lists) |
| `--bg-subtle` | `#f6f8fa` | Sidebar, headers, commit box, hover |
| `--bg-muted` | `#eaeef2` | Selected row (inactive pane), chips, kbd |
| `--bg-inset` | `#f0f2f5` | Terminal strip, code viewer gutter |
| `--border` | `#d0d7de` | Hairlines, input borders |
| `--border-subtle` | `#e5e9ed` | Row separators |
| `--text` | `#1f2328` | Primary text |
| `--text-muted` | `#59636e` | Secondary text, metadata |
| `--text-faint` | `#8c959f` | Placeholders, disabled, reviewed commits |
| `--accent` | `#0969da` | Primary buttons, focus ring, new badge, links |
| `--accent-bg` | `#ddf4ff` | Selected row (active pane), highlight pulse |
| `--accent-fg` | `#ffffff` | Text on accent |
| `--success` | `#1a7f37` | Additions text, active pulse |
| `--danger` | `#cf222e` | Deletions text, destructive buttons |
| `--warning` | `#9a6700` | Uncommitted dot, caution flags |
| `--warning-bg` | `#fff8c5` | Notices |
| `--diff-add-bg` | `#e6ffec` | Added line background |
| `--diff-add-word` | `#abf2bc` | Added word highlight |
| `--diff-del-bg` | `#ffebe9` | Deleted line background |
| `--diff-del-word` | `#ffc1ba` | Deleted word highlight |
| `--diff-hunk-bg` | `#ddf4ff` | Hunk header background |
| `--diff-gutter-add` | `#ccffd8` | Line-number gutter on added lines |
| `--diff-gutter-del` | `#ffd7d5` | Line-number gutter on deleted lines |
| `--shadow-pop` | `0 8px 24px rgba(31,35,40,.16)` | Popovers, palette, dialogs |

### 2.2 Color: dark

| Token | Value |
|---|---|
| `--bg` | `#1b1f24` |
| `--bg-subtle` | `#22272e` |
| `--bg-muted` | `#2d333b` |
| `--bg-inset` | `#171b20` |
| `--border` | `#3d444d` |
| `--border-subtle` | `#2f353d` |
| `--text` | `#d1d7e0` |
| `--text-muted` | `#9198a1` |
| `--text-faint` | `#6e7681` |
| `--accent` | `#4493f8` |
| `--accent-bg` | `#16314f` |
| `--accent-fg` | `#0d1117` |
| `--success` | `#3fb950` |
| `--danger` | `#f85149` |
| `--warning` | `#d29922` |
| `--warning-bg` | `#3b2e12` |
| `--diff-add-bg` | `rgba(46,160,67,.15)` |
| `--diff-add-word` | `rgba(46,160,67,.40)` |
| `--diff-del-bg` | `rgba(248,81,73,.15)` |
| `--diff-del-word` | `rgba(248,81,73,.40)` |
| `--diff-hunk-bg` | `rgba(56,139,253,.15)` |
| `--diff-gutter-add` | `rgba(46,160,67,.25)` |
| `--diff-gutter-del` | `rgba(248,81,73,.25)` |
| `--shadow-pop` | `0 8px 24px rgba(1,4,9,.6)` |

### 2.3 Syntax highlight colors (map highlight.js classes to tokens)

| Token | Light | Dark | Classes |
|---|---|---|---|
| `--syn-keyword` | `#cf222e` | `#ff7b72` | keyword, built_in, literal |
| `--syn-string` | `#0a3069` | `#a5d6ff` | string, regexp |
| `--syn-number` | `#0550ae` | `#79c0ff` | number, symbol |
| `--syn-comment` | `#6e7781` | `#8b949e` | comment, doctag |
| `--syn-function` | `#8250df` | `#d2a8ff` | title.function, title.class |
| `--syn-type` | `#953800` | `#ffa657` | type, class, params |
| `--syn-attr` | `#116329` | `#7ee787` | attr, tag, name |
| `--syn-meta` | `#0550ae` | `#79c0ff` | meta, variable |

### 2.4 Typography

| Token | Value |
|---|---|
| `--font-ui` | `-apple-system, BlinkMacSystemFont, "Segoe UI Variable", "Segoe UI", system-ui, "Noto Sans", sans-serif` |
| `--font-mono` | `"JetBrains Mono", "Cascadia Code", "SF Mono", ui-monospace, Menlo, Consolas, "Liberation Mono", monospace` (do **not** bundle fonts in v1; use whatever is installed) |

| Style | Size / line height / weight | Use |
|---|---|---|
| `text-xs` | 11px / 16px / 500 | Badges, chips, kbd, meta |
| `text-sm` | 12px / 16px / 400 | Secondary text, tooltips |
| `text-base` | 13px / 20px / 400 | Default UI text |
| `text-strong` | 13px / 20px / 600 | Repo names, file names, headers |
| `text-lg` | 15px / 24px / 600 | Dialog titles, empty-state titles |
| `code` | 12.5px / 20px / 400 mono | Diff, viewer (line height **exactly 20px** for virtualization) |
| `term` | 13px mono (user setting) | Terminal |

Tabular numbers (`font-variant-numeric: tabular-nums`) for all counts, line numbers, and timestamps.

### 2.5 Spacing, radius, elevation, sizing

- **Spacing scale (4px base):** `--s-1: 4px`, `--s-2: 8px`, `--s-3: 12px`, `--s-4: 16px`, `--s-5: 20px`, `--s-6: 24px`, `--s-8: 32px`.
- **Radius:** flat chrome. `--r-sm`, `--r-md` and `--r-lg` are all `0`, so buttons, inputs, menus, chips and dialogs have square corners. Only true circles (status dots, radio indicators, spinners) use `border-radius: 50%`. Radii quoted elsewhere in this document are historical.
- **Borders:** 1px hairline. Never 2px except the focus ring.
- **Focus ring:** `outline: 2px solid var(--accent); outline-offset: 1px;` on `:focus-visible` for every interactive element. Inside lists, the focused row shows the ring inset.
- **Z-index layers:** content 0, sticky headers 10, pane drawers 20, popovers/menus 100, palette/dialogs 200, toasts 300.
- **Scrollbars:** thin (8px), transparent track, thumb `--border` (hover `--text-faint`); `scrollbar-gutter: stable` on scroll areas.

### 2.6 Control sizes

| Control | Height | Notes |
|---|---|---|
| Button (default) | 28px | Horizontal padding 12px, text-base 600 |
| Button (small) | 24px | Padding 8px |
| Icon button | 28 x 28 (24 x 24 small) | Icon 16px, hover bg `--bg-muted` |
| Input / select | 28px | Border `--border`, focus border `--accent` plus ring |
| Segmented control | 28px | Used for tabs (History / Files), diff mode |
| Toolbar / pane header | 44px | |
| Status bar | 24px | Global bottom strip (sync/notice text, git version on error) |

Button variants: **Primary** (accent bg, accent-fg text), **Default** (bg-subtle, 1px border, text), **Ghost** (transparent, text-muted, hover bg-muted), **Danger** (danger bg, white text; only in confirm dialogs). Disabled: 45% opacity, no hover.

---

## 3. Application layout

### 3.1 Window

Native window chrome (OS title bar) in v1. Minimum window size **900 x 560**. Default **1440 x 900**. Layout is a three-zone flex: **Sidebar | Main (panes) | (none)**; the status bar spans the bottom (24px). Below 1100px total window width the sidebar collapses to its 56px icon mode automatically (the user can re-expand it with `Mod+B`).

### 3.2 Main wireframe (2 panes, terminal open in pane 1)

```
┌────────────┬────────────────────────────────────────────┬───────────────────────────────────────────┐
│ Vantage  ⌘B│ ① api-server   ⎇ main ▾      [↓ Pull origin 2]│ ② web-app      ⎇ main ▾    [⟳ Fetch origin] │
│ ┌────────┐ │ [History][Files]            ▤ Notes(2)  ▭ >_ ✕│ [History][Files]            ▤ Notes  ▭ >_ ✕ │
│ │ Search │ ├──────────────┬──────────┬──────────────────────┤──────────────┬──────────┬───────────────┤
│ └────────┘ │ ┌──────────────────────────────────────────┐ │                                           │
│            │ │ 3 new commits · 7 files · +120 −34  ⚑    │ │                                           │
│ PINNED     │ │ [Combined|Step]        [Mark reviewed R] │ │                                           │
│ ◉ api  +3  │ └──────────────────────────────────────────┘ │  (same structure; see section 5)          │
│   main  ●  │ ◌ Uncommitted changes     3 files             │                                           │
│            │ ● add auth middleware     Codex · 4m  7f3a2c1 │                                           │
│ REPOS      │ ● fix token expiry        Codex · 9m  a19be02 ⚑│                                           │
│ ◉ web  +7  │ ● update user tests       Codex · 12m c03d9f4 │                                           │
│   main     │ ─── ✓ Reviewed up to here ───                 │                                           │
│ ○ cli      │ ○ refactor db layer       you · 2d    b77e1a0 │                                           │
│   dev      │ ○ initial schema          you · 3d    90ab12c │                                           │
│ ○ docs     │                                               │                                           │
│ ○ infra    ├───────────────────────────────────────────────┤                                           │
│            │ ▾ api-server  ~/code/api-server      ⟲  ⌫  ▾  │                                           │
│ + Add repo │ $ npm test                                    │                                           │
│            │ ✓ 42 passing                                  │                                           │
├────────────┴───────────────────────────────────────────────┴───────────────────────────────────────────┤
│ Fetched api-server 2m ago                                                          git 2.46.0           │
└──────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

(Circled digits ①② are pane indicators; the glyphs shown are placeholders for Lucide icons.)

---

## 4. Sidebar

**Width:** default 232px, min 184px, max 320px (drag handle on the right edge, 4px hit area, cursor `col-resize`). **Collapsed:** 56px, showing only avatars with badge dots (toggle `Mod+B`; remembers state).

**Background:** `--bg-subtle`, right border `--border`.

### 4.1 Structure (top to bottom)

1. **Header (44px):** app name "Vantage" (text-strong), collapse button (right).
2. **Filter input (28px, margin 8px):** placeholder "Filter repositories" (focused by clicking; `Mod+K` opens the palette instead). Hidden when 6 or fewer repos to reduce clutter; appears automatically at 7+.
3. **Section label "PINNED"** (text-xs, uppercase, text-faint, 8px padding) only if any pinned.
4. **Repo rows** (scrollable).
5. **Section label "REPOSITORIES"** if pinned exists, else no label.
6. **Footer (40px, sticky):** `+ Add repository` ghost button (full width, left-aligned), `Mod+O` shown as a kbd chip on the right.

### 4.2 Repo row (height 48px)

```
┌──────────────────────────────────────────────┐
│ ┌──┐  api-server                    [+3] ●   │   line 1: name (text-strong), badges right
│ │ A│  ⎇ main                         ▭       │   line 2: branch (text-sm, muted), terminal glyph right
│ └──┘                                         │
└──────────────────────────────────────────────┘
```

- **Avatar (28 x 28, radius 6):** first letter of the repo name, uppercase, 12px 600. Background color is derived from a hash of the repo id into a fixed palette of 8 desaturated hues (same palette in both themes, with text contrast AA). Gives each project a stable identity and speeds scanning.
- **Activity pulse:** when active (agent probably working), a 2px `--success` ring around the avatar animating opacity 1 to 0.3 over 1.6s ease-in-out infinite. Static ring if `prefers-reduced-motion`.
- **Name:** text-strong, truncate with ellipsis.
- **New commits badge:** pill, height 18, padding 0 6px, radius 9, bg `--accent`, text `--accent-fg`, text-xs 600. Shows `+3`; caps at `99+`.
- **Uncommitted dot:** 8px circle `--warning`, 8px left of the badge (or alone). Tooltip "3 files changed".
- **Branch line:** `⎇ main` (branch icon 12px). Detached: `detached 7f3a2c1`. Middle-truncate long names.
- **Terminal glyph:** 12px `>_` icon, `--success` color, visible only when terminal output occurred in the last 5s.
- **Pane chip:** if the repo is open in a pane, a tiny 14px rounded square with the pane number (1 to 3) appears at the row's far right, `--bg-muted` bg; the focused pane's repo has it in `--accent`.
- **Error state:** warning icon replaces badges; row text-faint; tooltip with message.

### 4.3 Row states

| State | Style |
|---|---|
| Default | transparent |
| Hover | `--bg-muted` at 60% |
| Open in focused pane | `--accent-bg` background, 3px left bar `--accent` |
| Open in another pane | `--bg-muted` background, no bar |
| Keyboard focus | focus ring (inset) |
| Dragging | 90% opacity, shadow; drop targets: other rows (reorder, insertion line 2px `--accent`) and panes (pane gets a dashed `--accent` outline and the overlay text "Open here") |

### 4.4 Interactions

- **Click:** opens the repo in the **focused pane**. **Mod+Click:** opens in the next pane (creates one if under 3). **Middle click:** same as Mod+Click.
- **Right-click menu:** Open in pane 1 / 2 / 3 · Mark reviewed · Fetch · Pin / Unpin · Rename… · Copy path · Reveal in file manager · Open in editor · separator · Remove from Vantage.
- **Hover tooltip (400ms delay):** repo name, full path, `main ↑2 ↓1`, "3 new commits, 3 files changed", "Last activity 2m ago".
- **Drop a folder from the OS onto the sidebar:** shows a dashed accent overlay "Drop to add repository".
- **Sorting:** pinned, then by activity (default). A live reorder uses a 160ms FLIP animation but **does not move the row under the cursor while hovering it** (freeze reordering while pointer is over the sidebar to avoid mis-clicks; apply on mouse leave).

---

## 5. Pane

A pane is a vertical flex: **Pane header** (44px), **Pane body** (flex 1), **Terminal drawer** (optional, resizable).

Focused pane: 2px `--accent` line along the **top edge** of its header (subtle focus indicator). Unfocused panes dim their header text slightly (`--text-muted`). Clicking anywhere in a pane focuses it.

### 5.1 Pane header (44px, `--bg-subtle`, bottom border)

Left to right:

1. **Pane number chip** (`①`), only when 2+ panes.
2. **Repo name** (text-strong), click opens a repo switcher menu (same list as sidebar, searchable). Useful when the sidebar is collapsed.
3. **Branch dropdown** button: `⎇ main ▾` (default button style, small). Menu: search input, branch list (current has a check), footer "Branches are managed in the terminal" (muted, only if more than 8 branches).
4. **Sync button** (primary only when action is Push or Pull; default otherwise). Content per state:

| State | Icon + label | Variant |
|---|---|---|
| Fetch | `⟳ Fetch origin` plus text-xs "3m ago" | Default |
| Pull | `↓ Pull origin` plus count badge | Primary |
| Push | `↑ Push origin` plus count badge | Primary |
| Publish | `⇪ Publish branch` | Primary |
| Diverged | `⇅ Diverged ↑2 ↓1` | Default, disabled look; click opens terminal |
| Busy | spinner plus verb ("Pulling…") | Disabled |

5. Spacer.
6. **Tabs** (segmented): **History** | **Files**. (Placed center-left in wide panes; in narrow panes they move under the header as a second 32px row.)
7. **Notes** icon button, always visible; the count badge appears only when there is at least one note.
8. **Terminal** icon button (toggles drawer; highlighted when open).
9. **Close pane** `✕` icon button (hidden when only one pane).

### 5.2 Responsive pane body (based on the pane's own width, via ResizeObserver)

| Pane width | History tab layout |
|---|---|
| **1000px and above** | 3 columns: **Commits** (300px, resizable 240 to 420) / **Files** (240px, resizable 180 to 360) / **Diff** (flex) |
| **640 to 999px** | 2 columns: left column (300px) split vertically: **Commits** (top, flex) over **Files** (bottom, 40% height, resizable, collapsible); right: **Diff** |
| **420 to 639px** (typical with 3 panes) | **Single column drill-in:** Commits list, then Enter or click pushes **Files and Diff** view (with a back chevron `‹ Commits` in a 32px sub-header). `Esc` or `‹` goes back. Slide transition 160ms (instant if reduced motion). |

Files tab layout: 2 columns (**Tree** 240px / **Viewer** flex) at 640px and above; single column drill-in below.

### 5.3 Commits column

**Range header** (sticky top, only when `new_commit_count > 0`, height auto about 72px):

```
┌──────────────────────────────────────────────┐
│ 7 new commits                                │   text-strong
│ 12 files · +340 −120      ⚑ ⚑ ⚑              │   text-sm muted; flag icons (aggregated, max 4 + "+2")
│ [ Combined | Step through ]  [✓ Mark reviewed]│   segmented control + primary small button, tooltip "R"
└──────────────────────────────────────────────┘
```

- Background `--accent-bg` at 50%, bottom border `--border`. When selected (default), a 3px left bar `--accent`.
- Clicking the header selects the combined range.
- **Diverged/missing marker notice** replaces this with a `--warning-bg` card: "History was rewritten since you last reviewed." plus buttons **Compare from merge-base** / **Reset marker to HEAD**.

**Search field** (28px) appears when `/` pressed (inline above list, with `Esc` to clear); otherwise hidden to save space.

**Commit rows (height 52px):**

```
┌───────────────────────────────────────────────┐
│ ●─  add auth middleware                        │  line 1: subject (text-base; bold if new)
│ │   (A) Codex · 4m ago · 7f3a2c1      ⚑ ⚑    │  line 2: meta (text-sm muted), flags right
└───────────────────────────────────────────────┘
```

- **Gutter (24px)** with a vertical hairline connecting rows and a dot (10px):
  - New/unreviewed: solid `--accent` dot.
  - Reviewed: hollow `--text-faint` ring (1.5px).
  - Uncommitted: dashed `--warning` ring.
  - Merge commit: dot with a small inner diamond; plus a "merge" chip in the meta line.
- **Subject:** one line, ellipsis. Reviewed commits use `--text-muted`; new commits `--text` with weight 600.
- **Meta:** 16px author avatar (initials, same hash palette; agent-looking authors, e.g. emails containing "codex", "bot", "agent", get a tiny sparkle overlay), name, relative time, short SHA in mono. Tooltip on time shows absolute time.
- **Flags:** icons 14px, colored by severity (Info `--text-muted`, Caution `--warning`, Warn `--danger`); at most 3 shown, then `+N`. Tooltip: all flags with detail.
- **Selection:** active pane: `--accent-bg` plus left bar `--accent`. Inactive pane: `--bg-muted`.
- **Hover:** `--bg-subtle`. Hover reveals a ghost "⋯" icon button over the SHA position (opens the context menu); flags stay visible.

**Uncommitted row (52px):** dashed warning ring, title "Uncommitted changes", meta "3 files · +24 −6". Always first. Separated from the rest by a hairline.

**Marker divider (24px):** centered text-xs `--text-faint` label "✓ Reviewed up to here" with hairlines on both sides. Hover shows the marker date ("Marked 2 days ago"). Right-click: "Move marker here…".

**New commit arrival animation:** the new row is inserted at the top; height animates 0 to 52px over 180ms ease-out and the row background starts at `--accent-bg` fading to transparent over 1.4s. No scroll jump: if the user has scrolled down, the list keeps position and a floating pill appears at the top "↑ 2 new commits" (click scrolls to top). Reduced motion: no animation, row appears with a static highlight for 1.4s.

**Load more:** a 24px "Loading…" row at the bottom while paging.

### 5.4 Files column (changed files for current scope)

- Header (32px): "12 files changed" text-sm muted, and a tiny menu for "Collapse all / Expand all" for the diff, flat vs tree toggle (flat default; tree groups by directory).
- **File row (28px):** status icon (A green, M amber, D red, R blue, ? gray; 14px, inside a 16px square, text-xs letter), filename (text-base, strong) with directory text-faint after it (e.g. `middleware.ts  src/auth/`), right-aligned `+12 −3` (success/danger, text-xs), flag dot if flagged. Truncate directory first.
- Selected file: highlight and the diff scrolls to it. Scroll-sync: as the diff scrolls, the file list highlights the file currently under the sticky header.
- Uncommitted mode: leading **checkbox** (14px) on each row; header has a tri-state "select all" checkbox with label "3 of 3 files".
- **Commit box** (only in uncommitted mode), pinned to the column bottom, `--bg-subtle`, top border, padding 12px:

```
┌────────────────────────────────────┐
│ ┌──────────────────────────────┐✨ │  Summary input (placeholder "Summary (required)"), ✨ icon button only if configured
│ └──────────────────────────────┘   │
│ ▸ Description                      │  collapsible; expands to a 4-row textarea
│ [  Commit 3 files to main   ]      │  Primary, full width, disabled until valid
└────────────────────────────────────┘
```

  Summary counter appears only when above 60 chars (text-xs, amber at 73+).

### 5.5 Diff column

**Toolbar (32px, sticky top, `--bg`, bottom hairline):** scope label (left: "Combined · 7 commits" / "7f3a2c1 add auth middleware" / "Uncommitted changes"), then right: whitespace toggle `¶` (tooltip "Ignore whitespace (W)"), mode segmented **Unified | Split** (S), Collapse/Expand all, "Copy as prompt context" icon, and **Notes** quick-add hint appears only when lines are selected.

**Commit header block** (single-commit scope only, top of the diff, scrolls with content): subject (text-lg), body (text-base, pre-wrap, collapsible if more than 6 lines), meta row (avatar, author `<email>`, absolute date, full SHA with copy button), parents.

**File section:**

```
┌ ▾ src/auth/ middleware.ts   M   +12 −3        [View file] [⋯] ┐  sticky header, 36px, --bg-subtle
│ @@ -10,6 +10,9 @@ export function verify(token) {                 │  hunk header 24px, --diff-hunk-bg, mono text-sm muted
│  10  10   const secret = env.SECRET                           │  context
│  11      - const ttl = 3600                                   │  deletion: bg --diff-del-bg
│      11  + const ttl = 900                                    │  addition: bg --diff-add-bg
│  12  12   if (!token) return null                             │
│  ⋮  ↕ Show 20 more lines  (↑) (↓) (all 143)                   │  expand row, 28px, --bg-subtle, text-sm accent
│ @@ -48,3 +51,3 @@ ...                                            │
└───────────────────────────────────────────────────────────────┘
```

**Line anatomy (20px):** two gutters (old/new line numbers, 48px each in unified, mono text-xs `--text-faint`, right-aligned, tinted with `--diff-gutter-*` on changed lines), a 16px sign column (`+`/`−`, muted), then code (padding-left 8px, `white-space: pre`, horizontal scroll per file section, not per line). Tab size 4 (`tab-size: 4`). Long lines truncated at 2,000 chars with an inline "… show full line" link.

**Word-level highlight:** changed ranges inside a line get `--diff-add-word` / `--diff-del-word` background with radius 2px.

**Split mode:** two equal halves with a 1px divider, each with one line-number gutter; deletions on the left, additions on the right, with empty placeholder cells (`--bg-subtle` with a diagonal hatch pattern, very subtle) opposite unmatched lines. Horizontal scrolling is synchronized between halves.

**Selection:** clicking a line number selects that line; shift-click extends. Selected lines get an `--accent-bg` overlay and a floating action bar anchored at the end of the selection (height 32, `--shadow-pop`, radius 6): `Copy reference` · `Copy snippet` · `Add note` · `✕`.

**Collapsed file:** only the header; chevron rotates. Large/truncated: header plus a centered card "Large diff · 4,200 changed lines · [Load diff]".

**Binary:** a single row "Binary file changed · 120 KB → 134 KB". **Mode change:** "File mode changed 100644 → 100755". **Rename only:** "File renamed without changes".

**Empty diff:** centered muted text "No changes in this range".

**Note markers:** gutter icon (12px speech bubble, `--accent`) on lines with notes; hover shows a popover with the note text and Edit/Delete.

### 5.6 Files tab

- **Tree (240px):** filter input (28px) at top ("Filter files", `/`), then a virtualized tree (row 24px, 12px indent per level, folder chevrons, file icons are a **single generic file icon**, no per-language icon set in v1). Files changed in the current review scope get an amber dot; flagged ones a small flag. Current file is highlighted.
- **Viewer:** header (36px): path breadcrumb, revision chip (`HEAD` ▾; menu "HEAD", "Selected commit", "Working tree"), buttons: soft-wrap toggle, find (`Mod+F`), **Open in editor** (if configured), Copy path. Body: line numbers (48px gutter, `--bg-inset`), highlighted code (same typography as diff, no row backgrounds except the "target line" highlight `--accent-bg` for 2s after navigation). Hover on the gutter shows a blame-lite tooltip: `a19be02 · Codex · 9m ago · fix token expiry`.
- **Find bar:** top-right floating, 280px: input, count "3 of 12", prev/next, close. Matches `--warning-bg` background, current match with `--accent` outline.
- **Peek popover (Alt+Click / F12):** anchored below the identifier, width 520px max, max-height 320px, radius 6, shadow. Header "Definitions of `verify` (2)" text-sm; each candidate: `path:line` (mono, accent) and a 3-line code excerpt with the match line emphasized; click navigates; arrow keys move; `Esc` closes. Footer text-xs faint "Text-based match".

### 5.7 Terminal drawer

- Height default 240px, min 120, max 70% of the pane; drag handle on the top edge (4px hit, `row-resize`); double-click toggles between default and maximized.
- **Header strip (28px, `--bg-inset`):** chevron (collapse), repo name (text-sm strong), cwd (text-sm muted, middle-truncated), spacer, icon buttons **Clear**, **Restart**, **Hide**. A small `--success` dot pulses while output is arriving.
- **Body:** xterm.js, background `--bg-inset`, foreground `--text`, cursor `--accent`, selection `--accent-bg`. ANSI palette derived from tokens (provide light and dark palettes: red `--danger`, green `--success`, yellow `--warning`, blue `--accent`, magenta `#8250df`/`#d2a8ff`, cyan `#1b7c83`/`#56d4dd`). 8px padding.
- **Owned elsewhere state:** centered text-sm muted "This terminal is open in pane 2." with a default button **Move here**.
- **Process exited:** last line "[Process exited with code 0]" and an overlay button **Restart**.

### 5.8 Notes panel (slide-over)

- Slides in from the right edge of the pane, width 340px (or 100% in narrow panes), `--bg`, left border, shadow. 160ms ease-out.
- Header (44px): "Review notes (3)" and close. Sub-header buttons: **Copy as prompt** (primary small), **Send to terminal** (default small), **Clear all** (ghost, danger text).
- Body: notes grouped by file (file header text-strong sticky); each note card: `L42–L58` link (accent, navigates), 3-line code snippet (mono text-sm, `--bg-subtle`, radius 4), note text, `Edit` / `Delete` ghost icon buttons on hover. Stale notes show a "stale" chip.
- Empty: "No notes yet. Select lines in a diff and choose **Add note** to leave feedback for your agent."

---

## 6. Overlays

### 6.1 Command palette

- Centered horizontally, top offset 14vh, width **560px** (max 92vw), radius 10, `--shadow-pop`, bg `--bg`, 1px border. Backdrop: `rgba(0,0,0,.25)` (light) / `rgba(0,0,0,.5)` (dark), click to close.
- **Input row (44px):** search icon, input (text-lg size 15px), right-aligned mode hint chips (`>` commands, `/` files, `#` commits, `@` notes) shown only when the input is empty.
- **Results (max 8 visible, 36px rows, scroll):** grouped with 24px group labels (text-xs uppercase faint): "Repositories", "Commands", "Files", "Commits", "Notes".
  - Repo result: avatar (20px), name, branch muted, `+3` badge, `Mod+Enter: new pane` hint on the selected row.
  - Command result: icon, label, right-aligned kbd chips.
  - File result: filename strong plus dir faint. Commit result: sha mono plus subject plus relative time.
- Match highlighting: matched characters in `--accent` weight 600.
- Footer (28px, `--bg-subtle`): `↑↓ navigate · ↵ open · ⌘↵ new pane · esc close` (text-xs faint).
- Empty results: "No matches". Opening animation 100ms fade and 4px rise (none if reduced motion).

### 6.2 Dialogs

Centered, width 440px (confirm) / 560px (wider content), radius 10, padding 20px, shadow; title text-lg; body text-base; footer buttons right-aligned (**Cancel** default, then the primary/danger action). `Esc` cancels, `Enter` triggers the primary action unless it is destructive (destructive requires clicking, or `Mod+Enter`). Focus is trapped; initial focus on Cancel for destructive dialogs.

Dialog specs:

- **Add repository:** folder picker button plus path field; detected-repo preview (name, branch, commit count); optional "Treat the last [ 0 ] commits as unreviewed". Primary **Add**.
- **Scan folder:** checkbox list of found repos with paths; **Add selected (N)**.
- **Revert commit:** shows subject, SHA, file list (max 8). Primary **Create revert commit**.
- **Reset to marker:**
  ```
  Reset api-server to your reviewed marker?
  This will remove 7 commits from main:
    ● add auth middleware     7f3a2c1
    ● fix token expiry        a19be02
    … and 5 more
  ┌ ⚠ 3 of these commits are already pushed. Resetting will need a force push.
  │   Prefer reverting.                          [ Revert the range instead ]
  └
  A backup is saved so you can undo this.
  [ Cancel ]                       [ Reset and drop 7 commits ]   (danger)
  ```
- **Discard changes:** "Discard changes to 3 files? A backup is saved so you can undo this for 30 days." **Discard 3 files** (danger).
- **Error details:** monospace pre block of stderr (max-height 240, scroll), **Copy** button, **Close**.
- **Rename repo:** single input.

### 6.3 Menus & popovers

Width auto (min 180), padding 4px, items 28px tall, radius 6, shadow; separator hairline; item hover uses `--bg-muted` (no accent-colored hover) to stay calm. Shortcuts right-aligned text-xs faint. Submenus open to the right with 80ms delay. Menus close on `Esc`, outside click, or action.

### 6.4 Toasts

Bottom-center, above the status bar (16px gap), stacked up to 3, width up to 420px, bg `--text` inverted (dark bubble in light theme, light bubble in dark theme), text-base, radius 8, optional action button text in `--accent` (inverted-appropriate), auto-dismiss 5s (8s with an action, pause on hover), `role="status"`. Error toasts have a leading red dot and stay 8s with a **Details** action.

### 6.5 Tooltips

Delay 400ms, 150ms fade, text-sm, bg `--text` inverted, radius 4, padding 4 8, max-width 280. Include shortcut kbd chips where relevant. Never essential info only in a tooltip.

### 6.6 Keyboard help overlay (`?`)

Dialog, two-column list of shortcuts grouped by Navigation, Review, Diff, Panes, Terminal. kbd chips styled: bg `--bg-muted`, border `--border`, radius 4, text-xs mono, padding 0 5px, min-width 18px.

---

## 7. Screens & states (catalog)

### 7.1 First run (no repos)

Main area is a centered empty state (max-width 420):
- Icon (48px, muted, a stylized branch).
- Title "Review your agents' work in one place" (text-lg).
- Text "Add the repositories you're working on. Vantage shows what changed since you last looked."
- Buttons: **Add repository** (primary), **Scan a folder for repositories** (default).
- Hint row: "Tip: drag a folder here".
The sidebar shows only the footer's Add button.

### 7.2 Git missing

Full window overlay, centered card: title "Vantage needs Git", text "Install Git, then reopen Vantage.", button **Download Git** (opens git-scm.com), **Retry**.

### 7.3 Repo selected, nothing new, clean

History list is shown without a range header. Above the list a quiet line (32px, text-sm muted, check icon): "All caught up. You've reviewed everything." The diff column shows the most recent commit selected by default.

### 7.4 Repo with new commits (default hero state)

Range header visible; combined range selected; Files and Diff show combined results. First file auto-scrolled into view. Pressing `r` plays a brief confirm: the range header collapses (height to 0, 180ms), the blue dots turn to hollow rings, the marker divider moves to the top, the sidebar badge disappears, and a toast "Marked as reviewed" with **Undo** appears (undo restores the previous marker).

### 7.5 Uncommitted selected

Files column in checkbox mode with the commit box; diff shows the first file; History list's first row is highlighted.

### 7.6 Empty states (copy)

| Where | Copy |
|---|---|
| Empty repo | "No commits yet. Make your first commit from the terminal or the Uncommitted changes row." |
| History search no results | "No commits match "<query>"." |
| No changed files in scope | "No changes in this range." |
| Files tab, empty repo | "No files yet." |
| Notes panel | (see 5.8) |
| Recovery list | "Nothing to recover. Backups appear here when you reset or discard." |
| Peek no hits | "No definitions found." |

### 7.7 Loading states

- Lists: 6 skeleton rows (same height as real rows, `--bg-muted` bars with a very subtle shimmer; static if reduced motion). Show only if loading exceeds 150ms (avoid flicker).
- Diff file: header immediately; body a 3-line skeleton until hunks arrive.
- Sync button: spinner as described.
- Never block the whole UI with a spinner.

### 7.8 Error states

Inline, close to the cause: a `--warning-bg` banner (36px, text-sm, icon, message, action link) at the top of the affected column. Examples: "Couldn't reach origin. [Retry]", "Operation in progress (rebase). Finish it in the terminal. [Open terminal]". Fatal-for-repo errors show in the sidebar row (warning icon) and in the pane body as a centered state with an action.

---

## 8. Settings dialog

Modal, 720 x 520, left nav (160px) with sections General · Git · Terminal · Integrations · Recovery · About; right content scrolls. Controls use the standard 28px inputs. Each setting has a label (text-strong), a one-line description (text-sm muted), and the control on the right (or below for wide inputs).

- **General:** Theme (System / Light / Dark segmented), UI scale (slider 90% to 130%, step 5), Repository sort (Activity / Manual).
- **Git:** Auto-fetch interval (select: Off, 1, 5, 15 min), Context lines (stepper 0 to 20), Ignore whitespace by default (switch).
- **Terminal:** Shell (text input with placeholder showing the detected default, "Browse…"), Font size (stepper 10 to 20).
- **Integrations:** External editor command (text, helper text "Placeholders: {repo} {file} {line}. Example: code -g {repo}/{file}:{line}"), **Test** button; Commit message command (text, helper text "Receives the diff on stdin, prints the message to stdout"), **Test** button.
- **Recovery:** table (Date · Repo · Kind · Description · **Restore**), top note "Backups are kept for 30 days."
- **About:** version, git version, data folder with **Reveal**, license.

Switch control: 32 x 18 track, thumb 14px, on = `--accent`.

---

## 9. Motion & feedback

| Interaction | Duration / easing |
|---|---|
| Hover color changes | 80ms linear |
| Popover/menu appear | 100ms ease-out, 4px translate |
| Panel slide (notes, drill-in) | 160ms ease-out |
| New commit row | 180ms height, 1.4s highlight fade |
| Mark reviewed collapse | 180ms ease-in-out |
| Sidebar reorder (FLIP) | 160ms ease-out |
| Activity pulse | 1.6s ease-in-out infinite |
| Toast in/out | 160ms ease-out, 8px rise |

`@media (prefers-reduced-motion: reduce)`: all transitions to 0ms except opacity fades of 80ms; pulses become static.

Instant feedback rule: any action that takes over 150ms shows a busy state on the **control that triggered it**, never a global spinner.

---

## 10. Iconography

Lucide icons, 16px, stroke width 1.5, `currentColor`. Suggested mapping: branch `git-branch`, commit dot (custom CSS), fetch `refresh-cw`, pull `arrow-down-to-line`, push `arrow-up-from-line`, publish `upload`, terminal `terminal`, notes `message-square-text`, flag `flag`, large `files`, deletion-heavy `trash-2`, dependencies `package`, config `settings-2`, tests-removed `flask-conical` (struck through via overlay), secrets `key-round`, big file `hard-drive`, binary `binary`, check `check`, search `search`, close `x`, more `ellipsis`, collapse `chevron-down`, drill back `chevron-left`, warning `triangle-alert`, sparkle `sparkles`, copy `copy`, external `external-link`, folder `folder`, file `file`. Only import the icons used.

App icon: simple rounded-square, accent-blue background with a white two-node-and-line "relay" glyph. Provide 1024px source SVG and generated platform sizes (placeholder acceptable until final art).

---

## 11. Accessibility

- All interactive controls reachable by keyboard; visible focus ring; no keyboard traps except dialogs (intentional trap with `Esc` exit).
- Lists use `role="listbox"` / `option` (commit list, file list), tree uses `role="tree"` / `treeitem` with `aria-expanded`; dialogs `role="dialog"` with `aria-modal`, labelled by the title; toasts `role="status"` (errors `role="alert"`); palette `role="combobox"` with `aria-activedescendant`.
- Color is never the only signal: flags have icons, additions/deletions have `+`/`−` signs, status has letters, unreviewed has bold text plus a filled dot.
- Contrast: text 4.5:1 minimum (7:1 not required), icons and borders conveying meaning 3:1. Verify all tokens in both themes.
- Hit targets at least 24 x 24 CSS px; sidebar and list rows already exceed this.
- Respect OS text scaling through the UI scale setting; layout must not break at 130%.
- Screen reader labels: repo row "api-server, branch main, 3 new commits, uncommitted changes"; commit row "add auth middleware, by Codex, 4 minutes ago, 2 flags, unreviewed".

---

## 12. Copy & tone

Plain, calm, short. Sentence case everywhere. No exclamation marks, no emoji (the sparkle icon is an icon, not an emoji). Say what happened and what the user can do.

| Context | Copy |
|---|---|
| Mark reviewed | "Marked as reviewed" (Undo) |
| Commit done | "Committed 7f3a2c1" |
| Push done | "Pushed 2 commits to origin/main" |
| Pull done | "Pulled 3 commits" |
| Fetch done (status bar) | "Fetched api-server just now" |
| Reset done | "Reset to reviewed marker" (Undo) |
| Discard done | "Discarded 3 files" (Undo) |
| Auth needed | "Git needs credentials. Run `git fetch` in the terminal to sign in." (Open terminal) |
| Diverged | "Local and remote have diverged. Resolve this in the terminal." |
| Dirty blocks revert | "Commit or discard your changes first." |
| Not a repo | "That folder isn't a Git repository." |
| Another git process | "Another Git process is running. Try again in a moment." |
| History rewritten | "History was rewritten since you last reviewed." |

---

## 13. Component inventory (build list)

Common: `Button`, `IconButton`, `Segmented`, `Input`, `Textarea`, `Switch`, `Checkbox`, `Select`, `Menu`/`MenuItem`, `Popover`, `Tooltip`, `Dialog`, `Toast`, `Badge`, `Chip`, `Kbd`, `Avatar`, `Splitter`, `VirtualList`, `Skeleton`, `EmptyState`, `Banner`.

Feature: `Sidebar`, `RepoRow`, `PaneHost`, `Pane`, `PaneHeader`, `BranchMenu`, `SyncButton`, `HistoryList`, `CommitRow`, `UncommittedRow`, `MarkerDivider`, `RangeHeader`, `FileList`, `FileRow`, `CommitBox`, `DiffView`, `DiffToolbar`, `DiffFileSection`, `DiffHunk`, `DiffLine`, `SplitDiffLine`, `ExpandRow`, `LineSelectionBar`, `NoteMarker`, `FilesTab`, `FileTree`, `CodeViewer`, `FindBar`, `PeekPopover`, `TerminalDrawer`, `TerminalView`, `NotesPanel`, `NoteEditor`, `CommandPalette`, `SettingsDialog`, `RecoveryList`, `ShortcutsHelp`, `FirstRun`, `GitMissing`.

Each component: typed props, a minimal story or usage example in `src/lib/components/_playground.svelte` (a dev-only page with fixture data) so visuals can be checked in both themes without a live repo.

---

## 14. Visual QA checklist (run at M10)

1. Both themes: every screen in section 7 looks intentional; no hard-coded colors (grep for `#` in component CSS).
2. 900x560 window: sidebar collapses to icons automatically below 1100px total width; one pane; no clipped controls.
3. 3 panes at 1440px wide: single-column drill-in works, back navigation works, no horizontal page scroll.
4. 130% UI scale: no overlap or truncation of badges, headers, buttons.
5. Long names: 80-char repo name, 200-char commit subject, 300-char file path truncate with ellipsis and tooltips.
6. 10k-line diff scrolls at 60 fps; sticky headers never overlap content incorrectly.
7. Reduced motion on: no pulses, no slides.
8. Keyboard-only run through the core loop (add repo, review combined diff, mark reviewed, jump to next repo, open terminal) without touching the mouse.
9. High-DPI and standard-DPI: 1px hairlines render crisply; icons aligned to pixel grid.
10. Screenshot every state in section 7 and save in `docs/screens/` for the final README.
