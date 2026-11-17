//! Shared measurements for interactive controls.
//!
//! Views should reach for these instead of repeating raw radii and control
//! heights, so the app keeps one geometry as it grows.

pub const RADIUS_MD: f32 = 8.0;

/// Minimum interactive target, independent of the painted glyph. A 16 px
/// checkbox still gets a 32 × 32 hit area so it is comfortable to click.
pub const HIT_TARGET_MIN: f32 = 32.0;

/// Primary actions get a larger target than the minimum.
pub const HIT_TARGET_PRIMARY: f32 = 40.0;

pub const CONTROL_MD: f32 = HIT_TARGET_MIN;

/// Height of a control that sits in a one-line row (the commit bar's message
/// field and its button). Pinning one height keeps them aligned, and means
/// focus styling can never change the field's height.
pub const ROW_CONTROL_HEIGHT: f32 = 36.0;

/// Grow a painted control's hit area to the minimum without growing the glyph.
/// Callers put the visual square inside this outer box.
pub const fn hit_target(visual_size: f32) -> f32 {
    if visual_size > HIT_TARGET_MIN {
        visual_size
    } else {
        HIT_TARGET_MIN
    }
}

/// Same idea for a primary action, which gets the larger target.
pub const fn primary_hit_target(visual_size: f32) -> f32 {
    if visual_size > HIT_TARGET_PRIMARY {
        visual_size
    } else {
        HIT_TARGET_PRIMARY
    }
}

/// The only icon sizes the app should use. Add a variant here when a new size
/// is genuinely needed rather than passing another raw float at a call site.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconSize {
    Xxs,
    Xs,
    Sm,
    Md,
    Xl,
}

impl IconSize {
    pub fn px(self) -> f32 {
        match self {
            Self::Xxs => 10.0,
            Self::Xs => 12.0,
            Self::Sm => 14.0,
            Self::Md => 16.0,
            Self::Xl => 28.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_visuals_grow_to_the_minimum_hit_target() {
        assert_eq!(hit_target(16.0), HIT_TARGET_MIN);
        assert_eq!(hit_target(22.0), HIT_TARGET_MIN);
        assert_eq!(hit_target(HIT_TARGET_MIN), HIT_TARGET_MIN);
        assert_eq!(HIT_TARGET_MIN, 32.0);
    }

    #[test]
    fn larger_visuals_keep_their_own_size() {
        assert_eq!(hit_target(48.0), 48.0);
        assert_eq!(primary_hit_target(52.0), 52.0);
    }

    #[test]
    fn primary_actions_get_a_larger_target() {
        assert_eq!(primary_hit_target(28.0), HIT_TARGET_PRIMARY);
        assert_eq!(primary_hit_target(36.0), HIT_TARGET_PRIMARY);
        assert_eq!(HIT_TARGET_PRIMARY, 40.0);
    }
}
