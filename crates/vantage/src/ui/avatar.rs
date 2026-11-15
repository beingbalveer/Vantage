//! Initials/photo avatar for the git identity a commit will be attributed to.
//!
//! The tile shows the GitHub profile photo when it has been fetched, and falls
//! back to initials drawn locally — on a colour derived from the email, so the
//! same person always gets the same tile — while a download is in flight or when
//! the machine cannot prove which account the identity belongs to.

use std::sync::Arc;

use gpui::{
    div, img, prelude::*, px, Div, Image, ImageSource, ObjectFit, Rgba, Styled, StyledImage,
};

use crate::theme::{Theme, FONT_SANS};
use crate::ui::tokens::ROW_CONTROL_HEIGHT;
use crate::ui::Txt;

/// Diameter of the tile in the commit footer: the same height as the message
/// field beside it, so the row reads as one control strip.
pub const AVATAR_SM: f32 = ROW_CONTROL_HEIGHT;

/// A round tile for one git identity.
pub fn avatar(theme: &Theme, name: &str, email: &str, photo: Option<Arc<Image>>, size: f32) -> Div {
    let frame = div()
        .flex()
        .items_center()
        .justify_center()
        .w(px(size))
        .h(px(size))
        .rounded(px(size / 2.0))
        .overflow_hidden()
        .flex_shrink_0();

    match photo {
        Some(photo) => frame.child(
            // The radius lives on the image itself: gpui clips an image to its
            // own corner radii, while a rounded parent only clips to a
            // rectangle. Without this the photo renders with square corners.
            img(ImageSource::Image(photo))
                .w(px(size))
                .h(px(size))
                .rounded(px(size / 2.0))
                .object_fit(ObjectFit::Cover),
        ),
        None => frame
            .bg(palette_color(theme, palette_index(email)))
            .font_family(FONT_SANS)
            .text_size(px(size * 0.4))
            .line_height(px(size * 0.4))
            .semibold()
            .fg(theme.primary_text)
            .child(initials(name, email)),
    }
}

/// Up to two letters from the name, falling back to the email's local part.
pub fn initials(name: &str, email: &str) -> String {
    let from_name: String = name
        .split_whitespace()
        .filter_map(|word| word.chars().next())
        .take(2)
        .collect();
    if !from_name.is_empty() {
        return from_name.to_uppercase();
    }

    let local = email.split('@').next().unwrap_or_default();
    let from_email: String = local
        .chars()
        .filter(|c| c.is_alphanumeric())
        .take(2)
        .collect();
    if from_email.is_empty() {
        "?".to_string()
    } else {
        from_email.to_uppercase()
    }
}

/// Stable bucket for an email: the same address always gets the same colour.
fn palette_index(email: &str) -> usize {
    // FNV-1a over the lowercased address: tiny, stable across runs and
    // platforms, and good enough to spread a handful of identities out.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in email.trim().to_lowercase().bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    (hash % PALETTE_SIZE as u64) as usize
}

const PALETTE_SIZE: usize = 5;

/// Identity colours, drawn from the shared palette so the tile never clashes
/// with the surrounding chrome in either appearance.
fn palette_color(theme: &Theme, index: usize) -> Rgba {
    match index {
        0 => theme.status_blue,
        1 => theme.status_green,
        2 => theme.status_amber,
        3 => theme.status_red,
        _ => theme.accent_text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initials_come_from_the_name_first() {
        assert_eq!(initials("Balveer Singh", "b@example.com"), "BS");
        assert_eq!(initials("Vantage", "v@example.com"), "V");
        assert_eq!(initials("  ", "balveer@example.com"), "BA");
        assert_eq!(initials("", "@example.com"), "?");
    }

    #[test]
    fn palette_bucket_is_stable_and_case_insensitive() {
        let first = palette_index("Balveer@Example.com");
        assert_eq!(first, palette_index("  balveer@example.com  "));
        assert!(first < PALETTE_SIZE);
        // Different people should not all land on the same colour.
        let buckets: std::collections::HashSet<usize> = [
            "a@example.com",
            "b@example.com",
            "c@example.com",
            "d@example.com",
        ]
        .iter()
        .map(|email| palette_index(email))
        .collect();
        assert!(buckets.len() > 1);
    }
}
