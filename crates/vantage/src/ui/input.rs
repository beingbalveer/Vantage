//! Single-line text input: state, key handling and chrome.
//!
//! This is a deliberately small custom editor painted from a plain string.
//! GPUI 0.2.2 does not give a div-based field an IME session or glyph metrics,
//! so the known limits are:
//!
//! - no IME composition (CJK, dead keys, emoji pickers);
//! - no mouse caret placement — a click moves the caret to the end, and a
//!   double-click selects the whole line;
//! - no undo/redo history and no drag-to-select.
//!
//! Those are tracked as a larger input replacement, not something this module
//! pretends to solve. Keyboard editing, selection, copy/cut/paste and a visible
//! focus ring are implemented here.

use gpui::{div, prelude::*, px, ClipboardItem, Div, KeyDownEvent, Styled};

use crate::theme::{Theme, FONT_SANS, LH_SM, LH_XS, TEXT_SM, TEXT_XS};

use super::focus::focus_ring;
use super::icon::icon;
use super::text::Txt;
use super::tokens::{IconSize, ROW_CONTROL_HEIGHT};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FieldVariant {
    /// `px-3 py-2 text-sm` inside the add-repository modal.
    Form,
    /// `px-2.5 py-1.5 text-xs` for the sidebar "new group" row.
    Small,
    /// `pl-8 pr-3 py-1.5 text-xs` with a leading search icon.
    Search,
    /// One-line control pinned to [`ROW_CONTROL_HEIGHT`]: the commit message
    /// field, which has to line up with the button beside it and must not
    /// resize itself when it takes focus.
    Row,
}

impl FieldVariant {
    /// Whether this variant owns a fixed height rather than deriving one from
    /// its padding.
    fn fixed_height(self) -> bool {
        matches!(self, Self::Row)
    }
}

#[derive(Clone, Default)]
pub struct FieldState {
    pub value: String,
    /// Caret position, in characters.
    pub caret: usize,
    /// Where the current selection started, in characters. `None` means there
    /// is no selection; equal to `caret` is also treated as none.
    pub anchor: Option<usize>,
}

impl FieldState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, value: impl Into<String>) {
        self.value = value.into();
        self.caret = self.value.chars().count();
        self.anchor = None;
    }

    pub fn clear(&mut self) {
        self.value.clear();
        self.caret = 0;
        self.anchor = None;
    }

    /// Ordered, non-empty selection as `(start, end)` in characters.
    pub fn selection(&self) -> Option<(usize, usize)> {
        let anchor = self.anchor?;
        let length = self.value.chars().count();
        let anchor = anchor.min(length);
        let caret = self.caret.min(length);
        (anchor != caret).then(|| (anchor.min(caret), anchor.max(caret)))
    }

    pub fn select_all(&mut self) {
        self.anchor = Some(0);
        self.caret = self.value.chars().count();
    }

    pub fn selected_text(&self) -> String {
        match self.selection() {
            Some((start, end)) => self.slice(start, end),
            None => String::new(),
        }
    }

    /// `(before, selected, after)` for the current selection.
    pub fn split_selection(&self) -> Option<(String, String, String)> {
        let (start, end) = self.selection()?;
        Some((
            self.slice(0, start),
            self.slice(start, end),
            self.slice(end, self.value.chars().count()),
        ))
    }

    fn slice(&self, start: usize, end: usize) -> String {
        let start = self.byte_at(start);
        let end = self.byte_at(end);
        self.value[start..end].to_string()
    }

    /// Drop the selection, returning whether anything was deleted.
    pub fn delete_selection(&mut self) -> bool {
        let Some((start, end)) = self.selection() else {
            self.anchor = None;
            return false;
        };
        let (start_byte, end_byte) = (self.byte_at(start), self.byte_at(end));
        self.value.replace_range(start_byte..end_byte, "");
        self.caret = start;
        self.anchor = None;
        true
    }

    fn byte_at(&self, char_index: usize) -> usize {
        self.value
            .char_indices()
            .nth(char_index)
            .map(|(i, _)| i)
            .unwrap_or(self.value.len())
    }

    pub fn insert(&mut self, text: &str) {
        // Typing over a selection replaces it, like every other text field.
        self.delete_selection();
        let at = self.byte_at(self.caret);
        self.value.insert_str(at, text);
        self.caret += text.chars().count();
    }

    pub fn backspace(&mut self) {
        if self.delete_selection() {
            return;
        }
        if self.caret == 0 {
            return;
        }
        let end = self.byte_at(self.caret);
        let start = self.byte_at(self.caret - 1);
        self.value.replace_range(start..end, "");
        self.caret -= 1;
    }

    pub fn delete(&mut self) {
        if self.delete_selection() {
            return;
        }
        if self.caret >= self.value.chars().count() {
            return;
        }
        let start = self.byte_at(self.caret);
        let end = self.byte_at(self.caret + 1);
        self.value.replace_range(start..end, "");
    }

    pub fn left(&mut self) {
        // Without shift an arrow collapses the selection instead of moving.
        if let Some((start, _)) = self.selection() {
            self.caret = start;
            self.anchor = None;
            return;
        }
        self.caret = self.caret.saturating_sub(1);
    }

    pub fn right(&mut self) {
        if let Some((_, end)) = self.selection() {
            self.caret = end;
            self.anchor = None;
            return;
        }
        self.caret = (self.caret + 1).min(self.value.chars().count());
    }

    pub fn home(&mut self) {
        self.caret = 0;
        self.anchor = None;
    }

    pub fn end(&mut self) {
        self.caret = self.value.chars().count();
        self.anchor = None;
    }

    // ---- selection extending (shift + arrows / home / end) ----------------

    /// Start a selection at the caret if none is in progress, then move.
    fn extend_to(&mut self, caret: usize) {
        self.anchor.get_or_insert(self.caret);
        self.caret = caret;
    }

    pub fn extend_left(&mut self) {
        let caret = self.caret.saturating_sub(1);
        self.extend_to(caret);
    }

    pub fn extend_right(&mut self) {
        let caret = (self.caret + 1).min(self.value.chars().count());
        self.extend_to(caret);
    }

    pub fn extend_home(&mut self) {
        self.extend_to(0);
    }

    pub fn extend_end(&mut self) {
        self.extend_to(self.value.chars().count());
    }

    pub fn split(&self) -> (String, String) {
        let at = self.byte_at(self.caret.min(self.value.chars().count()));
        (self.value[..at].to_string(), self.value[at..].to_string())
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FieldKey {
    Ignored,
    Changed,
    Submit,
    Cancel,
}

/// Single-line editing: printable keys, backspace/delete, arrows, home/end,
/// paste, and a selection model driven by shift + the same keys or `Cmd+A`,
/// with `Cmd+C` / `Cmd+X` on the selection.
pub fn apply_field_key(
    field: &mut FieldState,
    event: &KeyDownEvent,
    cx: &mut gpui::App,
) -> FieldKey {
    let keystroke = &event.keystroke;
    // `platform` is Cmd on macOS and Ctrl elsewhere; both are accepted so the
    // same shortcuts work on either platform.
    let shortcut = keystroke.modifiers.platform || keystroke.modifiers.control;
    let extend = keystroke.modifiers.shift;

    match keystroke.key.as_str() {
        "backspace" => {
            field.backspace();
            FieldKey::Changed
        }
        "delete" => {
            field.delete();
            FieldKey::Changed
        }
        "left" if extend => {
            field.extend_left();
            FieldKey::Changed
        }
        "right" if extend => {
            field.extend_right();
            FieldKey::Changed
        }
        "home" if extend => {
            field.extend_home();
            FieldKey::Changed
        }
        "end" if extend => {
            field.extend_end();
            FieldKey::Changed
        }
        "left" => {
            field.left();
            FieldKey::Changed
        }
        "right" => {
            field.right();
            FieldKey::Changed
        }
        "home" => {
            field.home();
            FieldKey::Changed
        }
        "end" => {
            field.end();
            FieldKey::Changed
        }
        "enter" => FieldKey::Submit,
        "escape" => {
            if field.selection().is_some() {
                field.anchor = None;
                FieldKey::Changed
            } else {
                FieldKey::Cancel
            }
        }
        "a" if shortcut => {
            field.select_all();
            FieldKey::Changed
        }
        "c" if shortcut => {
            let selected = field.selected_text();
            if !selected.is_empty() {
                cx.write_to_clipboard(ClipboardItem::new_string(selected));
            }
            FieldKey::Ignored
        }
        "x" if shortcut => {
            let selected = field.selected_text();
            if selected.is_empty() {
                return FieldKey::Ignored;
            }
            cx.write_to_clipboard(ClipboardItem::new_string(selected));
            field.delete_selection();
            FieldKey::Changed
        }
        "v" if shortcut => {
            if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                field.insert(&text);
                return FieldKey::Changed;
            }
            FieldKey::Ignored
        }
        _ => {
            if let Some(text) = keystroke.key_char.as_ref() {
                if !shortcut {
                    field.insert(text);
                    return FieldKey::Changed;
                }
            }
            FieldKey::Ignored
        }
    }
}

/// Renders one text field's chrome + content. The caller supplies the id,
/// focus wiring and key handler.
pub fn field(
    theme: &Theme,
    state: &FieldState,
    placeholder: &'static str,
    focused: bool,
    variant: FieldVariant,
) -> Div {
    let (pad_x, pad_y, line_height, font_size) = match variant {
        FieldVariant::Form => (px(12.0), px(8.0), LH_SM, TEXT_SM),
        FieldVariant::Small => (px(10.0), px(8.0), LH_XS, TEXT_XS),
        FieldVariant::Search => (px(12.0), px(8.0), LH_XS, TEXT_XS),
        // No vertical padding: the height comes from the token below, which
        // keeps the field and the button beside it on one baseline.
        FieldVariant::Row => (px(10.0), px(0.0), LH_SM, TEXT_SM),
    };
    let left_pad = if variant == FieldVariant::Search {
        px(32.0)
    } else {
        pad_x
    };

    let mut root = div()
        .flex()
        .items_center()
        .w_full()
        .min_w(px(0.0))
        .pl(left_pad)
        .pr(pad_x)
        .py(pad_y)
        .rounded(px(8.0))
        .border_1()
        .border_color(if focused {
            theme.focus_ring
        } else {
            theme.field_border
        })
        .bg(theme.field_bg)
        .cursor_text()
        .font_family(FONT_SANS)
        .text_size(px(font_size))
        .line_height(line_height)
        .fg(theme.field_text);

    // A spread-only ring makes keyboard focus visible without changing the
    // control's size, so focusing cannot shift the row it sits in.
    if focused {
        root = root.shadow(focus_ring(theme));
    }

    // A fixed height is set explicitly, so neither focus nor content can grow
    // the control and shift the row it lives in.
    if variant.fixed_height() {
        root = root.h(px(ROW_CONTROL_HEIGHT));
    }

    let empty = state.value.is_empty();
    if empty && !focused {
        root = root.fg(theme.placeholder).child(placeholder);
        return finish(root, theme, variant);
    }

    let caret_bar = || {
        div()
            .w(px(1.0))
            .h(px(font_size + 2.0))
            .flex_shrink_0()
            .bg(theme.selection_handle)
    };

    if empty {
        root = root.child(caret_bar());
        return finish(root, theme, variant);
    }

    // A selection is painted as three spans with the middle tinted, and the
    // caret stays visible at whichever end of it the user is typing from.
    if let (Some((start, _)), Some((head, selected, tail))) =
        (state.selection(), state.split_selection())
    {
        if focused && state.caret == start {
            root = root.child(caret_bar());
        }
        root = root.child(div().whitespace_nowrap().child(head)).child(
            div()
                .whitespace_nowrap()
                .rounded(px(2.0))
                .bg(crate::theme::alpha(theme.accent, 0.25))
                .child(selected),
        );
        if focused && state.caret != start {
            root = root.child(caret_bar());
        }
        return finish(
            root.child(div().whitespace_nowrap().child(tail)),
            theme,
            variant,
        );
    }

    let (before, after) = state.split();
    root = root.child(div().whitespace_nowrap().child(before));
    if focused {
        root = root.child(caret_bar());
    }
    finish(
        root.child(div().whitespace_nowrap().child(after)),
        theme,
        variant,
    )
}

fn finish(mut root: Div, theme: &Theme, variant: FieldVariant) -> Div {
    if variant == FieldVariant::Search {
        root = root
            .relative()
            .child(div().absolute().left(px(12.0)).top(px(9.0)).child(icon(
                "search",
                IconSize::Sm,
                theme.label_muted,
            )));
    }
    root
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(value: &str) -> FieldState {
        let mut state = FieldState::new();
        state.set(value);
        state
    }

    #[test]
    fn select_all_then_typing_replaces_the_value() {
        let mut state = field("hello world");
        state.select_all();
        assert_eq!(state.selection(), Some((0, 11)));
        assert_eq!(state.selected_text(), "hello world");

        state.insert("x");
        assert_eq!(state.value, "x");
        assert_eq!(state.caret, 1);
        assert_eq!(state.selection(), None);
    }

    #[test]
    fn shift_arrows_extend_and_backspace_removes_the_selection() {
        let mut state = field("abcd");
        state.home();
        state.extend_right();
        state.extend_right();
        assert_eq!(state.selection(), Some((0, 2)));
        assert_eq!(state.selected_text(), "ab");

        state.backspace();
        assert_eq!(state.value, "cd");
        assert_eq!(state.caret, 0);
        assert_eq!(state.selection(), None);
    }

    #[test]
    fn plain_arrow_collapses_the_selection_to_its_edge() {
        let mut state = field("abcd");
        state.home();
        state.extend_end();
        state.left();
        assert_eq!(state.selection(), None);
        assert_eq!(state.caret, 0);

        state.extend_end();
        state.right();
        assert_eq!(state.selection(), None);
        assert_eq!(state.caret, 4);
    }

    #[test]
    fn selecting_multibyte_text_counts_characters_not_bytes() {
        let mut state = field("héllo");
        state.home();
        state.extend_right();
        state.extend_right();
        assert_eq!(state.selected_text(), "hé");

        state.delete_selection();
        assert_eq!(state.value, "llo");
        assert_eq!(state.caret, 0);
    }

    #[test]
    fn selection_reports_start_and_end_regardless_of_direction() {
        let mut state = field("abcd");
        state.end();
        state.extend_left();
        state.extend_left();
        assert_eq!(
            state.selection(),
            Some((2, 4)),
            "backwards selection is ordered"
        );
        assert_eq!(state.selected_text(), "cd");
    }

    #[gpui::test]
    fn enter_submits_and_escape_cancels_without_typing(cx: &mut gpui::TestAppContext) {
        use gpui::{Keystroke, Modifiers};

        fn event(key: &str) -> KeyDownEvent {
            KeyDownEvent {
                keystroke: Keystroke {
                    modifiers: Modifiers::none(),
                    key: key.to_string(),
                    key_char: None,
                },
                is_held: false,
            }
        }

        let mut state = field("commit message");
        let submit = cx.update(|cx| apply_field_key(&mut state, &event("enter"), cx));
        assert!(matches!(submit, FieldKey::Submit));
        assert_eq!(state.value, "commit message");

        state.select_all();
        let clear_selection = cx.update(|cx| apply_field_key(&mut state, &event("escape"), cx));
        assert!(matches!(clear_selection, FieldKey::Changed));
        assert_eq!(state.selection(), None);

        let cancel = cx.update(|cx| apply_field_key(&mut state, &event("escape"), cx));
        assert!(matches!(cancel, FieldKey::Cancel));
    }
}
