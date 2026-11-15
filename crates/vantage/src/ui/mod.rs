//! The Vantage design system.
//!
//! Shared visual primitives live here so views do not hand-roll their own
//! button, input, tab or toggle chrome. View-specific helpers (status cells,
//! table columns, sync arrows) stay next to the views that use them.

pub mod avatar;
pub mod button;
pub mod icon;
pub mod input;
pub mod tabs;
pub mod text;
pub mod toggle;
pub mod tokens;
pub mod tooltip;

pub use avatar::{avatar, AVATAR_SM};
pub use button::{
    accent_button, accent_button_with, primary_button, secondary_button, secondary_button_with,
};
pub use icon::{icon, icon_button, icon_button_sm, spinner};
pub use input::{apply_field_key, field, FieldKey, FieldState, FieldVariant};
pub use tabs::{choice_button, tab};
pub use text::Txt;
pub use toggle::{checkbox, switch};
pub use tokens::IconSize;
pub use tooltip::IdentityTooltip;
