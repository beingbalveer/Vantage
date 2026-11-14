//! Exact palettes lifted from the Vantage React app's Tailwind classes.
//!
//! Hex values are the Tailwind v4 `theme.css` tokens (the app pins
//! `tailwindcss@4.3.3`) converted from OKLCH to sRGB, so every value below is
//! the colour the browser actually paints. Nothing here is invented: each field
//! maps to a class used in `src/App.tsx` / `src/components/*.tsx`.

use gpui::{px, rgb, FontWeight, Pixels, Rgba};

pub const FONT_SANS: &str = "Public Sans";
pub const FONT_DISPLAY: &str = "Plus Jakarta Sans";
pub const FONT_MONO: &str = "JetBrains Mono";

// ---- Tailwind v4 palette -------------------------------------------------
pub const WHITE: u32 = 0xffffff;
pub const SLATE_50: u32 = 0xf8fafc;
pub const SLATE_100: u32 = 0xf1f5f9;
pub const SLATE_200: u32 = 0xe2e8f0;
pub const SLATE_300: u32 = 0xcad5e2;
pub const SLATE_400: u32 = 0x90a1b9;
pub const SLATE_500: u32 = 0x62748e;
pub const SLATE_600: u32 = 0x45556c;
pub const SLATE_700: u32 = 0x314158;
pub const SLATE_800: u32 = 0x1d293d;
pub const SLATE_900: u32 = 0x0f172b;
pub const SLATE_950: u32 = 0x020618;

pub const BLUE_50: u32 = 0xeff6ff;
pub const BLUE_400: u32 = 0x51a2ff;
pub const BLUE_500: u32 = 0x2b7fff;
pub const BLUE_600: u32 = 0x155dfc;
pub const BLUE_700: u32 = 0x1447e6;
pub const BLUE_950: u32 = 0x162456;

pub const AMBER_400: u32 = 0xffb900;
pub const AMBER_700: u32 = 0xbb4d00;

pub const EMERALD_400: u32 = 0x00d492;
#[allow(dead_code)]
pub const EMERALD_600: u32 = 0x009966;
pub const EMERALD_700: u32 = 0x007a55;

pub const RED_50: u32 = 0xfef2f2;
pub const RED_200: u32 = 0xffc9c9;
pub const RED_400: u32 = 0xff6467;
#[allow(dead_code)]
pub const RED_600: u32 = 0xe7000b;
pub const RED_700: u32 = 0xc10007;
pub const RED_900: u32 = 0x82181a;
pub const RED_950: u32 = 0x460809;

pub fn alpha(color: Rgba, a: f32) -> Rgba {
    Rgba { a, ..color }
}

/// Tailwind spacing scale (1 unit = 0.25rem = 4px).
#[allow(dead_code)]
pub fn s(units: f32) -> Pixels {
    px(units * 4.0)
}

pub const TEXT_XS: f32 = 12.0;
pub const TEXT_SM: f32 = 14.0;
pub const TEXT_BASE: f32 = 16.0;
pub const TEXT_LG: f32 = 18.0;

pub const LH_XS: Pixels = px(16.0);
pub const LH_SM: Pixels = px(20.0);
pub const LH_BASE: Pixels = px(24.0);
pub const LH_LG: Pixels = px(28.0);

pub const W_REGULAR: FontWeight = FontWeight(400.0);
pub const W_MEDIUM: FontWeight = FontWeight(500.0);
pub const W_SEMIBOLD: FontWeight = FontWeight(600.0);
pub const W_BOLD: FontWeight = FontWeight(700.0);

/// Semantic colour roles resolved per appearance. Each one is the light-mode
/// class from the React source paired with its `dark:` counterpart.
#[derive(Clone, Copy)]
pub struct Theme {
    pub is_dark: bool,

    // Surfaces / text
    pub bg_app: Rgba,        // bg-slate-50            / dark:bg-slate-950
    pub text_app: Rgba,      // text-slate-900         / dark:text-slate-100
    pub panel: Rgba,         // bg-white               / dark:bg-slate-900
    pub header_bg: Rgba,     // bg-white/95            / dark:bg-slate-900/95
    pub divider: Rgba,       // border-slate-200       / dark:border-slate-800
    pub border_strong: Rgba, // border-slate-300       / dark:border-slate-700

    // Sidebar navigation
    pub nav_active_bg: Rgba,   // bg-slate-100        / dark:bg-slate-800
    pub nav_active_text: Rgba, // text-slate-900      / dark:text-slate-100
    pub nav_idle_text: Rgba,   // text-slate-600      / dark:text-slate-400
    pub nav_hover_bg: Rgba,    // hover:bg-slate-50   / dark:hover:bg-slate-800/50
    pub nav_hover_text: Rgba,  // hover:text-slate-900 / dark:hover:text-slate-100
    pub icon_idle: Rgba,       // text-slate-400      / dark:text-slate-500
    pub label_muted: Rgba,     // text-slate-500      / dark:text-slate-400
    pub subtle_bg: Rgba,       // bg-slate-50         / dark:bg-slate-800/70

    // Accent
    pub accent: Rgba,      // text-blue-600 / blue-400
    pub accent_text: Rgba, // text-blue-700 / blue-400
    pub accent_soft: Rgba, // bg-blue-50/60 / bg-blue-950/30

    // Buttons
    pub primary_bg: Rgba,         // bg-slate-900   / dark:bg-blue-600
    pub primary_bg_hover: Rgba,   // bg-slate-800   / dark:bg-blue-500
    pub primary_text: Rgba,       // text-white
    pub secondary_bg: Rgba,       // bg-white       / dark:bg-slate-800
    pub secondary_bg_hover: Rgba, // bg-slate-50    / dark:bg-slate-700
    pub secondary_border: Rgba,   // border-slate-200 / dark:border-slate-700
    pub secondary_text: Rgba,     // text-slate-700 / dark:text-slate-200

    // Tables
    pub table_head_bg: Rgba, // bg-slate-50/70   / dark:bg-slate-900/50
    pub group_head_bg: Rgba, // bg-slate-50/90   / dark:bg-slate-800/50
    pub row_hover_bg: Rgba,  // bg-slate-50      / dark:bg-slate-800/50
    pub row_border: Rgba,    // divide-slate-200 / dark:divide-slate-800
    pub text_soft: Rgba,     // text-slate-800   / dark:text-slate-200
    pub text_body: Rgba,     // text-slate-600   / dark:text-slate-300
    pub text_dim: Rgba,      // text-slate-400   / dark:text-slate-600

    // Sync / status colours
    pub status_amber: Rgba, // text-amber-700   / dark:text-amber-400
    pub status_red: Rgba,   // text-red-700     / dark:text-red-400
    pub status_blue: Rgba,  // text-blue-700    / dark:text-blue-400
    pub status_green: Rgba, // text-emerald-700 / dark:text-emerald-400
    pub add_text: Rgba,     // text-emerald-700 / dark:text-emerald-400
    pub del_text: Rgba,     // text-red-700     / dark:text-red-400

    // Form fields
    pub field_bg: Rgba,     // bg-slate-50    / dark:bg-slate-950
    pub field_border: Rgba, // border-slate-200 / dark:border-slate-700
    pub field_text: Rgba,   // text-slate-900 / dark:text-slate-100
    pub placeholder: Rgba,  // placeholder:text-slate-400

    // Diff block
    pub diff_bg: Rgba,
    #[allow(dead_code)]
    pub diff_text: Rgba,
    pub diff_meta: Rgba,
    pub diff_add: Rgba,
    pub diff_del: Rgba,
    pub diff_hunk: Rgba,

    // Overlays
    pub overlay: Rgba,      // bg-slate-950/35 … /40
    pub toast_bg: Rgba,     // bg-slate-900 / dark:bg-slate-100
    pub toast_text: Rgba,   // text-white   / dark:text-slate-900
    pub toast_border: Rgba, // border-slate-800 / dark:border-slate-200
    pub error_bg: Rgba,     // bg-red-50 / dark:bg-red-950/40
    pub error_border: Rgba, // border-red-200 / dark:border-red-900
    pub error_text: Rgba,   // text-red-700 / dark:text-red-400
    pub selection_handle: Rgba,
}

impl Theme {
    pub fn light() -> Self {
        Self {
            is_dark: false,
            bg_app: rgb(SLATE_50),
            text_app: rgb(SLATE_900),
            panel: rgb(WHITE),
            header_bg: alpha(rgb(WHITE), 0.95),
            divider: rgb(SLATE_200),
            border_strong: rgb(SLATE_300),

            nav_active_bg: rgb(SLATE_100),
            nav_active_text: rgb(SLATE_900),
            nav_idle_text: rgb(SLATE_600),
            nav_hover_bg: rgb(SLATE_50),
            nav_hover_text: rgb(SLATE_900),
            icon_idle: rgb(SLATE_400),
            label_muted: rgb(SLATE_500),
            subtle_bg: rgb(SLATE_50),

            accent: rgb(BLUE_600),
            accent_text: rgb(BLUE_700),
            accent_soft: alpha(rgb(BLUE_50), 0.6),

            primary_bg: rgb(SLATE_900),
            primary_bg_hover: rgb(SLATE_800),
            primary_text: rgb(WHITE),
            secondary_bg: rgb(WHITE),
            secondary_bg_hover: rgb(SLATE_50),
            secondary_border: rgb(SLATE_200),
            secondary_text: rgb(SLATE_700),

            table_head_bg: alpha(rgb(SLATE_50), 0.7),
            group_head_bg: alpha(rgb(SLATE_50), 0.9),
            row_hover_bg: rgb(SLATE_50),
            row_border: rgb(SLATE_200),
            text_soft: rgb(SLATE_800),
            text_body: rgb(SLATE_600),
            text_dim: rgb(SLATE_400),

            status_amber: rgb(AMBER_700),
            status_red: rgb(RED_700),
            status_blue: rgb(BLUE_700),
            status_green: rgb(EMERALD_700),
            add_text: rgb(EMERALD_700),
            del_text: rgb(RED_700),

            field_bg: rgb(SLATE_50),
            field_border: rgb(SLATE_200),
            field_text: rgb(SLATE_900),
            placeholder: rgb(SLATE_400),

            diff_bg: rgb(SLATE_950),
            diff_text: rgb(SLATE_200),
            diff_meta: rgb(SLATE_300),
            diff_add: rgb(EMERALD_400),
            diff_del: rgb(RED_400),
            diff_hunk: rgb(BLUE_400),

            overlay: alpha(rgb(SLATE_950), 0.35),
            toast_bg: rgb(SLATE_900),
            toast_text: rgb(WHITE),
            toast_border: rgb(SLATE_800),
            error_bg: rgb(RED_50),
            error_border: rgb(RED_200),
            error_text: rgb(RED_700),
            selection_handle: rgb(BLUE_600),
        }
    }

    pub fn dark() -> Self {
        Self {
            is_dark: true,
            bg_app: rgb(SLATE_950),
            text_app: rgb(SLATE_100),
            panel: rgb(SLATE_900),
            header_bg: alpha(rgb(SLATE_900), 0.95),
            divider: rgb(SLATE_800),
            border_strong: rgb(SLATE_700),

            nav_active_bg: rgb(SLATE_800),
            nav_active_text: rgb(SLATE_100),
            nav_idle_text: rgb(SLATE_400),
            nav_hover_bg: alpha(rgb(SLATE_800), 0.5),
            nav_hover_text: rgb(SLATE_100),
            icon_idle: rgb(SLATE_500),
            label_muted: rgb(SLATE_400),
            subtle_bg: alpha(rgb(SLATE_800), 0.7),

            accent: rgb(BLUE_400),
            accent_text: rgb(BLUE_400),
            accent_soft: alpha(rgb(BLUE_950), 0.3),

            primary_bg: rgb(BLUE_600),
            primary_bg_hover: rgb(BLUE_500),
            primary_text: rgb(WHITE),
            secondary_bg: rgb(SLATE_800),
            secondary_bg_hover: rgb(SLATE_700),
            secondary_border: rgb(SLATE_700),
            secondary_text: rgb(SLATE_200),

            table_head_bg: alpha(rgb(SLATE_900), 0.5),
            group_head_bg: alpha(rgb(SLATE_800), 0.5),
            row_hover_bg: alpha(rgb(SLATE_800), 0.5),
            row_border: rgb(SLATE_800),
            text_soft: rgb(SLATE_200),
            text_body: rgb(SLATE_300),
            text_dim: rgb(SLATE_600),

            status_amber: rgb(AMBER_400),
            status_red: rgb(RED_400),
            status_blue: rgb(BLUE_400),
            status_green: rgb(EMERALD_400),
            add_text: rgb(EMERALD_400),
            del_text: rgb(RED_400),

            field_bg: rgb(SLATE_950),
            field_border: rgb(SLATE_700),
            field_text: rgb(SLATE_100),
            placeholder: rgb(SLATE_400),

            diff_bg: rgb(SLATE_950),
            diff_text: rgb(SLATE_200),
            diff_meta: rgb(SLATE_300),
            diff_add: rgb(EMERALD_400),
            diff_del: rgb(RED_400),
            diff_hunk: rgb(BLUE_400),

            overlay: alpha(rgb(SLATE_950), 0.35),
            toast_bg: rgb(SLATE_100),
            toast_text: rgb(SLATE_900),
            toast_border: rgb(SLATE_200),
            error_bg: alpha(rgb(RED_950), 0.4),
            error_border: rgb(RED_900),
            error_text: rgb(RED_400),
            selection_handle: rgb(BLUE_400),
        }
    }
}
