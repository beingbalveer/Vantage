//! Typography helpers. These cascade to children like CSS text classes.

use gpui::{px, Pixels, Rgba, Styled};

use crate::theme::{
    FONT_DISPLAY, FONT_MONO, FONT_SANS, LH_BASE, LH_LG, LH_SM, LH_XS, TEXT_BASE, TEXT_LG, TEXT_SM,
    TEXT_XS, W_MEDIUM, W_SEMIBOLD,
};

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
    /// font-mono text-[11px]
    fn medium(self) -> Self {
        self.font_weight(W_MEDIUM)
    }
    fn semibold(self) -> Self {
        self.font_weight(W_SEMIBOLD)
    }
    fn fg(self, color: Rgba) -> Self {
        self.text_color(color)
    }
}

impl<T: Styled> Txt for T {}
