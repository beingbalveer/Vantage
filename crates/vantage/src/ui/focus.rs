//! Shared visible-focus treatment.
//!
//! GPUI 0.2.2 has no `:focus-visible` selector or accessibility tree. This
//! module only supplies the ring a focusable control draws while it owns the
//! keyboard. Controls that never call `track_focus` cannot use it.

use gpui::{point, px, BoxShadow};

use crate::theme::Theme;

/// A two-pixel ring that sits outside the control's border. A spread-only
/// shadow keeps the control's layout size unchanged, so taking focus cannot
/// shift the row around it.
pub fn focus_ring(theme: &Theme) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: theme.focus_ring.into(),
        offset: point(px(0.0), px(0.0)),
        blur_radius: px(0.0),
        spread_radius: px(2.0),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_ring_is_a_two_pixel_spread_shadow() {
        let ring = focus_ring(&Theme::light());
        assert_eq!(ring.len(), 1);
        assert_eq!(ring[0].blur_radius, px(0.0));
        assert_eq!(ring[0].spread_radius, px(2.0));
    }
}
