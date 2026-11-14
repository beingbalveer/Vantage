//! Text fitting helpers.
//!
//! GPUI 0.2.2 only ellipsizes a `Text` element when it is measured with a
//! definite width on the very first pass; text nested inside a wrapper gets its
//! max-content layout cached and is then clipped without a suffix. For the two
//! places where the React app guarantees truncation (`truncate` with a known
//! column width) we therefore fit the string up front with the same shaper the
//! renderer uses.

use gpui::{px, App, Font, FontWeight, Pixels, SharedString, TextRun};

pub fn font(family: &'static str, weight: FontWeight) -> Font {
    Font {
        family: family.into(),
        features: Default::default(),
        fallbacks: None,
        weight,
        style: gpui::FontStyle::Normal,
    }
}

/// Truncate `text` to `max_width`, appending "…" when it does not fit.
pub fn ellipsize(
    cx: &App,
    text: &str,
    family: &'static str,
    weight: FontWeight,
    font_size: f32,
    max_width: Pixels,
) -> SharedString {
    if max_width <= px(1.0) {
        return SharedString::default();
    }

    let font = font(family, weight);
    let mut runs = vec![TextRun {
        len: text.len(),
        font: font.clone(),
        color: gpui::black(),
        background_color: None,
        underline: None,
        strikethrough: None,
    }];

    let mut wrapper = cx.text_system().line_wrapper(font, px(font_size));
    wrapper.truncate_line(text.to_string().into(), max_width, "\u{2026}", &mut runs)
}

/// Width available to the "Latest Commit" column of a table that spans
/// `container_width`, given the fixed column ratios either side of it.
pub fn column_width(container_width: Pixels, fraction: f32, padding: f32) -> Pixels {
    let content = (container_width - px(2.0)).max(px(0.0));
    let width = content * fraction - px(padding * 2.0);
    width.max(px(0.0))
}

/// Width of the content area of a table: `min(1440, viewport - sidebar) - px-6`.
pub fn container_width(viewport_width: Pixels) -> Pixels {
    let usable = (viewport_width - px(256.0)).max(px(0.0));
    (usable.min(px(1440.0)) - px(48.0)).max(px(0.0))
}
