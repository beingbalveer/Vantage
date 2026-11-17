# Accessibility status

Vantage targets WCAG 2.1 AA for the text and controls it paints itself. GPUI
0.2.2 does not expose an accessibility tree, so this document separates what is
implemented and measured from what still needs a newer toolkit.

Last reviewed: 2026-11-16 (ticket P1-07).

## Implemented

### Text contrast

`Theme::text_meta` is the semantic colour for useful secondary text: timestamps,
branch names, paths, counts and activity labels. It replaces `text_dim` in every
one of those places. `text_dim` is now reserved for decoration (separators, the
empty-state illustration and disabled glyphs).

Measured WCAG contrast ratios (AA text needs 4.5:1):

| Pair | Light | Dark |
|---|---:|---:|
| `text_meta` on panel | 7.58 | 6.78 |
| `text_meta` on app background | 7.25 | 7.66 |
| `label_muted` on panel | 4.76 | 6.78 |
| `text_body` on panel | 7.58 | 12.00 |
| Status colours on panel | 4.81–6.83 | 6.17–10.35 |
| `focus_ring` (non-text, needs 3:1) | 5.25 | 6.76 |

`text_dim` measures 2.63:1 in light mode and 2.35:1 in dark mode, which is why it
must never carry text. The ratios above are enforced by
`theme::tests::metadata_text_meets_wcag_aa_in_both_themes`,
`status_and_accent_text_meet_wcag_aa_in_both_themes`,
`diff_text_meets_wcag_aa_in_both_themes` and
`focus_ring_meets_non_text_contrast`.

### Text sizes

- Metadata — timestamps, branch names, paths, counts, status badges — is 12 px
  (`Txt::xs` / `Txt::mono_xs`). The old 10–11 px metadata sizes are gone.
- Patch and commit-diff text is 12 px.
- Body text remains 14 px; headings keep their existing display sizes.

### Hit targets

Targets are measured in logical pixels and include padding around the painted
glyph:

| Control | Target |
|---|---|
| Checkbox | 32 × 32 (16 px square painted) |
| Switch | 44 × 32 (36 × 20 track painted) |
| Toolbar / close icon buttons | 32 × 32 |
| Row menus, commit-wide toggle | 32 × 32 |
| Summary filters and Clear | 32 px high |
| Tabs and segmented choice chips | 32 px high |
| Context-menu items | 32 px high |
| Primary buttons and dialog confirmations | 40 px high |
| One-line row controls (commit bar) | 36 px high |

`ui::tokens::hit_target` and `primary_hit_target` implement this, and
`ui::tokens::tests` plus `app::tests::checkbox_hit_target_is_at_least_32_square`
cover it.

### Labels, tooltips and focus

- Every icon-only control has a tooltip label through
  `ui::tooltip::label_tooltip`: refresh, add repository, both panel toggles,
  close buttons, repository/changes/file row menus, the expanded-commit toggle,
  group removal and the “not pushed” marker.
- `icon_button` now takes its element id and its label together, so an icon
  button cannot be constructed without a name.
- Text fields draw a two-pixel focus ring (`ui::focus::focus_ring`) on top of
  their accent border. The ring is a spread-only shadow, so focus does not move
  the surrounding layout.
- Dialogs with a focused text field state their keyboard contract next to the
  actions: “Enter adds · Esc closes”, “Enter creates · Esc cancels”,
  “Enter clones · Esc cancels” and “Enter confirms · Esc cancels”. Labels sit
  directly above their fields in the Add, New, Clone and Branch dialogs.

### Keyboard coverage

The primary flows have application-menu shortcuts and are keyboard reachable
without a pointer:

- Add repository: `Cmd+O`; new repository: `Cmd+N`; clone: `Cmd+Shift+O`.
- Settings: `Cmd+,`; refresh: `Cmd+R`; sidebar: `Cmd+B`; views: `Cmd+1/2/3`.
- Repository list: `Up`/`Down` to select, `Enter` to open.
- Pull: `Cmd+Shift+P`; push: `Cmd+P`; fetch: `Cmd+Shift+F`; commit: `Cmd+Enter`.
- Focused text fields support arrows, Home/End, Shift-selection, copy/cut/paste,
  Enter to submit and Escape to cancel.

## Not implemented — honest limits

These are limits of the current GPUI version and input primitive, not things
this change pretends to have fixed.

1. **No screen-reader tree.** GPUI 0.2.2 has no role, accessible-name, state or
   action API on `Div`. VoiceOver will not announce the toolbar buttons,
   checkboxes, tabs or status badges. Tooltips are visible hover labels, not
   semantic names. The app does not fake an ARIA layer.
2. **Clickable divs are not Tab stops.** Only text fields call `track_focus`
   today. Toolbar buttons, checkboxes, summary filters, row menus and context
   menus cannot be reached with Tab and activated with Enter/Space. Their
   primary actions exist in the menu bar; the per-row and per-file menus are
   pointer-only. Ticket P1-06 owns scoped key contexts and general focus
   management.
3. **Confirm and stash dialogs do not close on Escape.** They have no focused
   field and no modal-level key handler yet; use Cancel/Close or the backdrop.
   Also P1-06.
4. **The custom text input has no IME, mouse caret placement or undo/redo.** It
   is painted from a plain string without a platform input session or glyph
   metrics. A click moves the caret to the end; double-click selects the whole
   line; drag-select and word-select are unavailable. Composition, dead keys and
   emoji pickers do not work. This needs a real editable-text primitive.
5. **No reduced-motion or high-contrast variants.** The spinner animation and
   palettes are fixed. System appearance (light/dark) is followed, but neither
   `prefers-reduced-motion` nor a high-contrast palette exists in this toolkit
   version.
6. **No automated visual checks.** Contrast and target sizes have unit tests;
   focus rings, tooltip placement and actual screen-reader output do not have a
   headless assertion because GPUI 0.2.2 offers no inspection API for them.
