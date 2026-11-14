//! Single-line text input: state, key handling and chrome.

use gpui::{div, prelude::*, px, Div, KeyDownEvent, Styled};

use crate::theme::{Theme, FONT_SANS, LH_SM, LH_XS, TEXT_SM, TEXT_XS};

use super::icon::icon;
use super::text::Txt;
use super::tokens::IconSize;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FieldVariant {
    /// `px-3 py-2 text-sm` inside the add-repository modal.
    Form,
    /// `px-2.5 py-1.5 text-xs` for the sidebar "new group" row.
    Small,
    /// `pl-8 pr-3 py-1.5 text-xs` with a leading search icon.
    Search,
}

#[derive(Clone, Default)]
pub struct FieldState {
    pub value: String,
    /// Caret position, in characters.
    pub caret: usize,
}

impl FieldState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, value: impl Into<String>) {
        self.value = value.into();
        self.caret = self.value.chars().count();
    }

    pub fn clear(&mut self) {
        self.value.clear();
        self.caret = 0;
    }

    fn byte_at(&self, char_index: usize) -> usize {
        self.value
            .char_indices()
            .nth(char_index)
            .map(|(i, _)| i)
            .unwrap_or(self.value.len())
    }

    pub fn insert(&mut self, text: &str) {
        let at = self.byte_at(self.caret);
        self.value.insert_str(at, text);
        self.caret += text.chars().count();
    }

    pub fn backspace(&mut self) {
        if self.caret == 0 {
            return;
        }
        let end = self.byte_at(self.caret);
        let start = self.byte_at(self.caret - 1);
        self.value.replace_range(start..end, "");
        self.caret -= 1;
    }

    pub fn delete(&mut self) {
        if self.caret >= self.value.chars().count() {
            return;
        }
        let start = self.byte_at(self.caret);
        let end = self.byte_at(self.caret + 1);
        self.value.replace_range(start..end, "");
    }

    pub fn left(&mut self) {
        self.caret = self.caret.saturating_sub(1);
    }

    pub fn right(&mut self) {
        self.caret = (self.caret + 1).min(self.value.chars().count());
    }

    pub fn home(&mut self) {
        self.caret = 0;
    }

    pub fn end(&mut self) {
        self.caret = self.value.chars().count();
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

/// Minimal single-line editing: printable keys, backspace/delete, arrows,
/// home/end and paste. There is no IME or selection model yet.
pub fn apply_field_key(
    field: &mut FieldState,
    event: &KeyDownEvent,
    cx: &mut gpui::App,
) -> FieldKey {
    let keystroke = &event.keystroke;
    let platform = keystroke.modifiers.platform;

    match keystroke.key.as_str() {
        "backspace" => {
            field.backspace();
            FieldKey::Changed
        }
        "delete" => {
            field.delete();
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
        "escape" => FieldKey::Cancel,
        "v" if platform => {
            if let Some(item) = cx.read_from_clipboard() {
                if let Some(text) = item.text() {
                    field.insert(&text);
                    return FieldKey::Changed;
                }
            }
            FieldKey::Ignored
        }
        "a" if platform || keystroke.modifiers.control => {
            field.end();
            FieldKey::Changed
        }
        _ => {
            if let Some(text) = keystroke.key_char.as_ref() {
                if !platform && !keystroke.modifiers.control {
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
        FieldVariant::Small => (px(10.0), px(6.0), LH_XS, TEXT_XS),
        FieldVariant::Search => (px(12.0), px(6.0), LH_XS, TEXT_XS),
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
            theme.accent
        } else {
            theme.field_border
        })
        .bg(theme.field_bg)
        .cursor_text()
        .font_family(FONT_SANS)
        .text_size(px(font_size))
        .line_height(line_height)
        .fg(theme.field_text);

    let empty = state.value.is_empty();
    if empty && !focused {
        root = root.fg(theme.placeholder).child(placeholder);
        return finish(root, theme, variant);
    }

    if empty {
        root = root.child(
            div()
                .w(px(1.0))
                .h(px(font_size + 2.0))
                .flex_shrink_0()
                .bg(theme.selection_handle),
        );
        return finish(root, theme, variant);
    }

    let (before, after) = state.split();
    root = root.child(div().whitespace_nowrap().child(before));
    if focused {
        root = root.child(
            div()
                .w(px(1.0))
                .h(px(font_size + 2.0))
                .flex_shrink_0()
                .bg(theme.selection_handle),
        );
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
            .child(div().absolute().left(px(12.0)).top(px(8.0)).child(icon(
                "search",
                IconSize::Sm,
                theme.text_dim,
            )));
    }
    root
}
