//! Embedded Lucide icons (SVG path data taken from lucide 0.546, the version the
//! React app depends on) served through GPUI's `AssetSource`.
//!
//! GPUI rasterises an SVG to a monochrome mask and tints it with the element's
//! text colour, so every icon is rendered as black strokes here.

use std::borrow::Cow;

use gpui::{px, svg, AssetSource, Result, Rgba, SharedString, Styled};

macro_rules! icons {
    ($($name:literal => $file:literal),* $(,)?) => {
        pub const ALL: &[(&str, &[u8])] = &[
            $(($name, include_bytes!(concat!("../assets/icons/", $file))),)*
        ];
    };
}

icons! {
    "alert-circle" => "alert-circle.svg",
    "alert-triangle" => "alert-triangle.svg",
    "arrow-down" => "arrow-down.svg",
    "arrow-up" => "arrow-up.svg",
    "check" => "check.svg",
    "chevron-down" => "chevron-down.svg",
    "chevron-right" => "chevron-right.svg",
    "circle-check-big" => "circle-check-big.svg",
    "copy" => "copy.svg",
    "external-link" => "external-link.svg",
    "file-diff" => "file-diff.svg",
    "folder-git-2" => "folder-git-2.svg",
    "git-branch" => "git-branch.svg",
    "git-commit-horizontal" => "git-commit-horizontal.svg",
    "layers" => "layers.svg",
    "moon" => "moon.svg",
    "plus" => "plus.svg",
    "refresh-cw" => "refresh-cw.svg",
    "search" => "search.svg",
    "sun" => "sun.svg",
    "x" => "x.svg",
}

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        let name = path
            .strip_prefix("icons/")
            .and_then(|rest| rest.strip_suffix(".svg"));
        Ok(name
            .and_then(|name| ALL.iter().find(|(n, _)| *n == name))
            .map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, _path: &str) -> Result<Vec<SharedString>> {
        Ok(ALL
            .iter()
            .map(|(name, _)| SharedString::from(format!("icons/{name}.svg")))
            .collect())
    }
}

/// A Lucide icon at `size` px, tinted with `color` (Tailwind `h-3.5 w-3.5 text-…`).
pub fn icon(name: &'static str, size: f32, color: Rgba) -> impl gpui::IntoElement {
    svg()
        .path(format!("icons/{name}.svg"))
        .flex_shrink_0()
        .w(px(size))
        .h(px(size))
        .text_color(color)
}
