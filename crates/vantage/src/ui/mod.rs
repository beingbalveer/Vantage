//! The Vantage design system.
//!
//! Shared visual primitives live here so views do not hand-roll their own
//! button, input, tab or toggle chrome. View-specific helpers (status cells,
//! table columns, sync arrows) stay next to the views that use them.

pub mod button;
pub mod icon;
pub mod input;
pub mod tabs;
pub mod text;
pub mod toggle;
pub mod tokens;

pub use button::{primary_button, secondary_button};
pub use icon::{icon, icon_button};
pub use input::{apply_field_key, field, FieldKey, FieldState, FieldVariant};
pub use tabs::{choice_button, tab};
pub use text::Txt;
pub use toggle::{checkbox, switch};
pub use tokens::IconSize;
