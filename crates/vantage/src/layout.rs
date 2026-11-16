//! Width policy for the dashboard.
//!
//! Vantage supports a 1040×640 window. With the sidebar expanded and the
//! inspector open, that leaves roughly 780px for both content panes, so the
//! panes cannot both keep their comfortable proportions. These helpers decide
//! how much each pane gets and when a view switches to its compact form.
//!
//! The geometry mirrors [`crate::app::VantageApp::render`]: the sidebar has a
//! 4px splitter, the inspector has a 4px splitter, and the inspector keeps a
//! 420px minimum. The list also keeps a floor where the window allows it, which
//! stops a persisted 80% inspector from squeezing the repository rows into an
//! unusable column.

/// Smallest window the app opens at.
pub const MIN_WINDOW_WIDTH: f32 = 1040.0;
pub const MIN_WINDOW_HEIGHT: f32 = 640.0;

/// The invisible splitter between two panes.
pub const SPLITTER_WIDTH: f32 = 4.0;

/// Width at which the inspector can no longer shrink further.
pub const INSPECTOR_MIN_WIDTH: f32 = 420.0;

/// The list keeps at least this much room while the inspector is open, unless
/// the available content area is too small for both minimums.
pub const MIN_LIST_WIDTH: f32 = 280.0;

/// Below this list width the repository rows switch to their compact form.
pub const COMPACT_LIST_WIDTH: f32 = 360.0;

/// Below this list width the summary strip uses short labels and drops the
/// colour dots. It is wider than the row threshold because four labelled
/// filters with their counts need more room than one row.
pub const COMPACT_SUMMARY_WIDTH: f32 = 460.0;

/// Below this inspector width the header stacks the repository identity and
/// uses icon-sized Pull/Push/Fetch controls so the path keeps its column.
pub const COMPACT_INSPECTOR_WIDTH: f32 = 620.0;

/// Pane widths for one window size, in logical points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DashboardLayout {
    pub window_width: f32,
    /// Sidebar excluded, including its splitter when it is visible.
    pub main_width: f32,
    pub list_width: f32,
    /// Zero while the inspector is closed.
    pub inspector_width: f32,
}

impl DashboardLayout {
    pub fn inspector_open(self) -> bool {
        self.inspector_width > 0.0
    }

    pub fn compact_list(self) -> bool {
        self.list_width < COMPACT_LIST_WIDTH
    }

    pub fn compact_summary(self) -> bool {
        self.list_width < COMPACT_SUMMARY_WIDTH
    }

    pub fn compact_inspector(self) -> bool {
        self.inspector_open() && self.inspector_width < COMPACT_INSPECTOR_WIDTH
    }
}

/// Lay out the list and inspector for `window_width`.
///
/// `sidebar_width` already includes the collapsed rail width; the caller also
/// passes whether the sidebar is collapsed so the hidden splitter is not
/// subtracted.
pub fn dashboard_layout(
    window_width: f32,
    sidebar_width: f32,
    sidebar_collapsed: bool,
    inspector_open: bool,
    inspector_fraction: f32,
) -> DashboardLayout {
    let window_width = window_width.max(0.0);
    let sidebar_gutter = if sidebar_collapsed {
        0.0
    } else {
        SPLITTER_WIDTH
    };
    let main_width = (window_width - sidebar_width - sidebar_gutter).max(0.0);

    if !inspector_open {
        return DashboardLayout {
            window_width,
            main_width,
            list_width: main_width,
            inspector_width: 0.0,
        };
    }

    let available = (main_width - SPLITTER_WIDTH).max(0.0);
    let fraction = if inspector_fraction.is_finite() {
        inspector_fraction.clamp(0.3, 0.8)
    } else {
        0.67
    };
    let desired = (main_width * fraction).max(INSPECTOR_MIN_WIDTH);
    // Give the inspector what it asked for until the list reaches its floor,
    // but never shrink below the inspector's own minimum. On a supported
    // window both floors fit; the min() only protects the arithmetic when the
    // window is below the supported size.
    let widest = (available - MIN_LIST_WIDTH).max(INSPECTOR_MIN_WIDTH);
    let inspector_width = desired.min(widest).min(available);
    let list_width = (available - inspector_width).max(0.0);

    DashboardLayout {
        window_width,
        main_width,
        list_width,
        inspector_width,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIDEBAR: f32 = 256.0;

    #[test]
    fn minimum_window_keeps_both_panes_usable() {
        let layout = dashboard_layout(MIN_WINDOW_WIDTH, SIDEBAR, false, true, 0.67);
        assert!(
            layout.list_width >= MIN_LIST_WIDTH - 0.5,
            "list was {}",
            layout.list_width
        );
        assert!(
            layout.inspector_width >= INSPECTOR_MIN_WIDTH - 0.5,
            "inspector was {}",
            layout.inspector_width
        );
        assert!(layout.compact_list());
        assert!(layout.compact_inspector());
    }

    #[test]
    fn a_wide_window_gets_the_normal_controls() {
        let layout = dashboard_layout(1470.0, SIDEBAR, false, true, 0.67);
        assert!(layout.list_width > COMPACT_LIST_WIDTH);
        assert!(!layout.compact_list());
        assert!(!layout.compact_inspector());
    }

    #[test]
    fn summary_compacts_before_the_rows_do() {
        let layout = dashboard_layout(1470.0, SIDEBAR, false, true, 0.67);
        assert!(!layout.compact_list());
        assert!(
            layout.compact_summary(),
            "four full summary labels do not fit beside the inspector"
        );

        let wide = dashboard_layout(1700.0, SIDEBAR, false, true, 0.67);
        assert!(!wide.compact_summary());
    }

    #[test]
    fn an_extreme_pane_request_leaves_the_list_its_floor() {
        let layout = dashboard_layout(MIN_WINDOW_WIDTH, SIDEBAR, false, true, 0.8);
        assert!((layout.list_width - MIN_LIST_WIDTH).abs() < 0.5);
        assert!(layout.inspector_width >= INSPECTOR_MIN_WIDTH);
    }

    #[test]
    fn a_closed_inspector_gives_the_whole_content_area_to_the_list() {
        let layout = dashboard_layout(MIN_WINDOW_WIDTH, SIDEBAR, false, false, 0.67);
        assert!(!layout.inspector_open());
        assert_eq!(layout.list_width, 780.0);
    }

    #[test]
    fn a_collapsed_sidebar_does_not_reserve_its_splitter() {
        let layout = dashboard_layout(MIN_WINDOW_WIDTH, 56.0, true, true, 0.67);
        assert_eq!(layout.main_width, 984.0);
        assert!(layout.list_width > 300.0);
        assert!(layout.compact_list());
        assert!(
            !layout.compact_inspector(),
            "the collapsed rail gives the inspector enough for full controls"
        );
    }

    #[test]
    fn the_inspector_minimum_wins_over_the_list_floor_when_they_conflict() {
        let layout = dashboard_layout(MIN_WINDOW_WIDTH, 360.0, false, true, 0.8);
        assert!((layout.inspector_width - INSPECTOR_MIN_WIDTH).abs() < 0.5);
        assert!(layout.list_width > 0.0);
        assert!(layout.list_width < MIN_LIST_WIDTH);
    }
}
