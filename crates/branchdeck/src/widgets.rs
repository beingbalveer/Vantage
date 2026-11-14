//! Small building blocks shared by every view: type scale helpers, buttons,
//! the working-tree status cell, the checkbox, and the text field.

use gpui::{div, prelude::*, px, Div, KeyDownEvent, Pixels, Rgba, Styled};

use crate::icons::icon;
use crate::model::RepoSyncState;
use crate::theme::{
    Theme, FONT_DISPLAY, FONT_MONO, FONT_SANS, LH_BASE, LH_LG, LH_SM, LH_XS, TEXT_BASE, TEXT_LG,
    TEXT_SM, TEXT_XS, W_BOLD, W_MEDIUM, W_REGULAR, W_SEMIBOLD,
};

/// Tailwind type-scale helpers; these cascade to child text like CSS does.
#[allow(dead_code)]
pub trait Txt: Styled + Sized {
    fn fam(self, family: &'static str, size: f32, line_height: Pixels) -> Self {
        self.font_family(family)
            .text_size(px(size))
            .line_height(line_height)
    }
    /// text-xs
    fn xs(self) -> Self {
        self.fam(FONT_SANS, TEXT_XS, LH_XS)
    }
    /// text-sm
    fn sm(self) -> Self {
        self.fam(FONT_SANS, TEXT_SM, LH_SM)
    }
    /// text-base
    fn base(self) -> Self {
        self.fam(FONT_SANS, TEXT_BASE, LH_BASE)
    }
    /// text-lg
    fn lg(self) -> Self {
        self.fam(FONT_SANS, TEXT_LG, LH_LG)
    }
    /// text-[11px]
    fn x11(self) -> Self {
        self.fam(FONT_SANS, 11.0, px(16.0))
    }
    /// font-display text-base
    fn display_base(self) -> Self {
        self.fam(FONT_DISPLAY, TEXT_BASE, LH_BASE)
    }
    /// font-display text-lg
    fn display_lg(self) -> Self {
        self.fam(FONT_DISPLAY, TEXT_LG, LH_LG)
    }
    /// font-display text-sm
    fn display_sm(self) -> Self {
        self.fam(FONT_DISPLAY, TEXT_SM, LH_SM)
    }
    /// font-display text-xs
    fn display_xs(self) -> Self {
        self.fam(FONT_DISPLAY, TEXT_XS, LH_XS)
    }
    /// font-mono text-xs
    fn mono_xs(self) -> Self {
        self.fam(FONT_MONO, TEXT_XS, LH_XS)
    }
    /// font-mono text-sm
    fn mono_sm(self) -> Self {
        self.fam(FONT_MONO, TEXT_SM, LH_SM)
    }
    /// font-mono text-[11px]
    fn mono_x11(self) -> Self {
        self.fam(FONT_MONO, 11.0, px(16.0))
    }
    fn regular(self) -> Self {
        self.font_weight(W_REGULAR)
    }
    fn medium(self) -> Self {
        self.font_weight(W_MEDIUM)
    }
    fn semibold(self) -> Self {
        self.font_weight(W_SEMIBOLD)
    }
    fn bold(self) -> Self {
        self.font_weight(W_BOLD)
    }
    fn fg(self, color: Rgba) -> Self {
        self.text_color(color)
    }
}

impl<T: Styled> Txt for T {}

// ---- buttons -------------------------------------------------------------

pub fn primary_button(theme: &Theme, label: &'static str, icon_name: Option<&'static str>) -> Div {
    let mut button = div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .px(px(14.0))
        .py(px(6.0))
        .rounded(px(8.0))
        .bg(theme.primary_bg)
        .cursor_pointer()
        .whitespace_nowrap()
        .flex_shrink_0()
        .xs()
        .medium()
        .fg(theme.primary_text)
        .hover(move |s| s.bg(theme.primary_bg_hover));

    if let Some(name) = icon_name {
        button = button.child(icon(name, 14.0, theme.primary_text));
    }
    button.child(label)
}

pub fn secondary_button(
    theme: &Theme,
    label: &'static str,
    icon_name: Option<&'static str>,
) -> Div {
    let mut button = div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .px(px(12.0))
        .py(px(6.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(theme.secondary_border)
        .bg(theme.secondary_bg)
        .cursor_pointer()
        .whitespace_nowrap()
        .flex_shrink_0()
        .xs()
        .medium()
        .fg(theme.secondary_text)
        .hover(move |s| s.bg(theme.secondary_bg_hover));

    if let Some(name) = icon_name {
        button = button.child(icon(name, 14.0, theme.secondary_text));
    }
    button.child(label)
}

// ---- status cell ---------------------------------------------------------

/// `StatusText.tsx` — icon + text, no badge chrome.
pub fn status_text(theme: &Theme, state: RepoSyncState, uncommitted: usize) -> Div {
    let (name, color, text) = match state {
        RepoSyncState::Conflict => ("alert-triangle", theme.status_red, "Conflict".to_string()),
        RepoSyncState::Behind => ("arrow-down", theme.status_red, "Behind origin".to_string()),
        RepoSyncState::Modified => (
            "file-diff",
            theme.status_amber,
            format!("{uncommitted} uncommitted"),
        ),
        RepoSyncState::Ahead => (
            "arrow-up",
            theme.status_blue,
            "Unpushed commits".to_string(),
        ),
        RepoSyncState::Clean => (
            "circle-check-big",
            theme.status_green,
            "Clean & synced".to_string(),
        ),
    };

    div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .flex_shrink_0()
        .whitespace_nowrap()
        .xs()
        .medium()
        .fg(color)
        .child(icon(name, 14.0, color))
        .child(div().child(text))
}

// ---- checkbox ------------------------------------------------------------

pub fn checkbox(theme: &Theme, checked: bool) -> Div {
    let base = div()
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .w(px(14.0))
        .h(px(14.0))
        .rounded(px(3.0))
        .border_1()
        .cursor_pointer();

    if checked {
        base.bg(theme.accent).border_color(theme.accent).child(icon(
            "check",
            10.0,
            theme.primary_text,
        ))
    } else {
        base.border_color(theme.border_strong).bg(theme.panel)
    }
}

// ---- text field ----------------------------------------------------------

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
        return root;
    }

    if empty {
        root = root.child(
            div()
                .w(px(1.0))
                .h(px(font_size + 2.0))
                .flex_shrink_0()
                .bg(theme.selection_handle),
        );
        return root;
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
    root.child(div().whitespace_nowrap().child(after))
}

// ---- misc ----------------------------------------------------------------

#[allow(dead_code)]
pub fn divider(theme: &Theme) -> Div {
    div().w_full().h(px(1.0)).bg(theme.divider)
}

pub fn sync_arrows(theme: &Theme, ahead: i32, behind: i32) -> Div {
    div()
        .flex()
        .items_center()
        .justify_end()
        .gap(px(8.0))
        .font_family(FONT_MONO)
        .text_size(px(TEXT_XS))
        .line_height(LH_XS)
        .whitespace_nowrap()
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(2.0))
                .fg(if ahead > 0 {
                    theme.status_blue
                } else {
                    theme.text_dim
                })
                .when(ahead > 0, |d| d.medium())
                .child(icon(
                    "arrow-up",
                    12.0,
                    if ahead > 0 {
                        theme.status_blue
                    } else {
                        theme.text_dim
                    },
                ))
                .child(ahead.to_string()),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(2.0))
                .fg(if behind > 0 {
                    theme.status_red
                } else {
                    theme.text_dim
                })
                .when(behind > 0, |d| d.medium())
                .child(icon(
                    "arrow-down",
                    12.0,
                    if behind > 0 {
                        theme.status_red
                    } else {
                        theme.text_dim
                    },
                ))
                .child(behind.to_string()),
        )
}
