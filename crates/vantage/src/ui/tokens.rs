//! Shared measurements for interactive controls.
//!
//! Views should reach for these instead of repeating raw radii and control
//! heights, so the app keeps one geometry as it grows.

pub const RADIUS_MD: f32 = 8.0;
pub const CONTROL_MD: f32 = 30.0;

/// Height of a control that sits in a one-line row (the commit bar's message
/// field and its button). Pinning one height keeps them aligned, and means
/// focus styling can never change the field's height.
pub const ROW_CONTROL_HEIGHT: f32 = 28.0;

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
