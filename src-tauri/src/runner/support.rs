//! Support-list scrolling, row matching, CE search regions, and skill-level filters.

use super::*;

pub(crate) fn is_unknown_element_error(err: &str, screen: &str, element: &str) -> bool {
    err.contains(&format!("unknown element: {screen}.{element}"))
}

// ---------------------------------------------------------------------------
// Runner
// ---------------------------------------------------------------------------

pub(crate) const POLL_INTERVAL: Duration = Duration::from_millis(500);
pub(crate) const ACTION_DELAY: Duration = Duration::from_millis(300);
pub(crate) const SKILL_TAP_DELAY: Duration = Duration::from_millis(500);

/// Hard cap on friend-list refreshes inside `handle_support_select`. After
/// this many refreshes (each preceded by a full scroll cycle) without
/// finding the pinned servant, the runner aborts with an error so the user
/// isn't stuck looping forever on a servant that simply isn't available.
pub(crate) const SUPPORT_MAX_REFRESHES: u32 = 99;
/// Settle time after tapping the "refresh friend list" button. The friend
/// list refetch and re-render takes ~2.5s on slow devices; one extra second
/// of buffer keeps us from OCRing a half-loaded list.
pub(crate) const SUPPORT_REFRESH_SETTLE: Duration = Duration::from_secs(3);
/// The game keeps the support refresh button disabled for roughly ten
/// seconds after a refresh. Poll the button template before tapping so we do
/// not waste a tap on the cooldown state.
pub(crate) const SUPPORT_REFRESH_AVAILABLE_TIMEOUT: Duration = Duration::from_secs(12);
pub(crate) const SUPPORT_REFRESH_AVAILABLE_POLL: Duration = Duration::from_millis(500);
/// Small timeout for the refresh-confirm dialog to animate in after tapping
/// the support refresh button.
pub(crate) const SUPPORT_REFRESH_DIALOG_APPEAR_TIMEOUT: Duration = Duration::from_secs(2);
/// Poll cadence while waiting for the refresh-confirm dialog to appear or
/// disappear.
pub(crate) const SUPPORT_REFRESH_DIALOG_POLL: Duration = Duration::from_millis(300);
/// Maximum wait for the refresh-confirm dialog to close after tapping OK.
pub(crate) const SUPPORT_REFRESH_DIALOG_DISMISS_TIMEOUT: Duration = Duration::from_secs(8);
/// Extra pause after the refresh-confirm dialog is first seen, before we tap
/// the confirm button. The detect template can match while the modal is
/// still mid-fade-in and the button hit-area isn't fully interactive yet;
/// this short settle keeps taps from being eaten by the animation.
pub(crate) const SUPPORT_REFRESH_DIALOG_CONFIRM_SETTLE: Duration = Duration::from_millis(500);

/// Refresh-friend-list button on the support-select screen, captured from
/// a 2560x1440 landscape device. Calibrated alongside the class-tab strip
/// (same row, x further right).
pub(crate) const SUPPORT_REFRESH_BUTTON: Point = Point::new(0.726, 0.178);
/// Confirm button inside the support refresh dialog.
pub(crate) const SUPPORT_REFRESH_CONFIRM_BUTTON: Point = Point::new(0.650, 0.779);
pub(crate) const SUPPORT_REFRESH_AVAILABLE_ELEMENT: &str = "refresh_available";
pub(crate) const SUPPORT_REFRESH_DIALOG_ELEMENT: &str = "dialog_refresh_support";

/// "技能显示切换" toggle on the support-select screen — the button that
/// cycles which skill panel (owned vs append) is shown for every support
/// row. Three-state cycle: 固定持有 → 固定追加 → 间隔切换 → … The runner
/// can't tell which mode the user has the game in (the icon variants
/// don't ship as templates), but we don't need to: each tap advances
/// the cycle, so worst case 3 taps will surface every panel layout.
/// Sits immediately to the left of `SUPPORT_REFRESH_BUTTON` in the same
/// settings row, so it shares y with the class-tab strip.
pub(crate) const SUPPORT_SKILL_PANEL_TOGGLE_BUTTON: Point = Point::new(0.658, 0.178);
/// Settle time after tapping the panel toggle: long enough for the
/// support-list rows to redraw their skill icons and for OCR to see
/// the new panel. This is intentionally longer than ordinary support
/// row settles because the toggle can briefly show transitional/blank
/// skill slots on real devices.
pub(crate) const SUPPORT_SKILL_PANEL_TOGGLE_SETTLE: Duration = Duration::from_millis(800);
/// Cap on how many times we'll tap the toggle for a single candidate
/// row before giving up on it. The cycle is length 3 plus we may need
/// to ride out the auto-switching mode's flip, so 3 attempts is the
/// minimum that's guaranteed to expose every panel layout in every
/// starting state.
pub(crate) const SUPPORT_SKILL_PANEL_MAX_TOGGLE_TAPS: u32 = 3;

/// Support rows have a right-side "助战编队确认" button that opens the
/// friend/support detail page. Use that button as a vertical row anchor,
/// but tap slightly to its left so selecting a support stays on the main
/// row hit-area.
pub(crate) const SUPPORT_SELECT_TAP_LEFT_OF_CONFIRM_BUTTON_X: f64 = 0.030;

pub(crate) fn support_row_tap_point(row: &SupportRowMatch) -> Point {
    row.score_anchor
        .as_ref()
        .map(|anchor| {
            Point::new(
                anchor.x - SUPPORT_SELECT_TAP_LEFT_OF_CONFIRM_BUTTON_X,
                anchor.y + anchor.h / 2.0,
            )
        })
        .unwrap_or(row.tap)
}

mod class_filter;
pub(crate) use class_filter::*;

mod class_filter_runtime;

mod craft_essence;
pub(crate) use craft_essence::*;

mod craft_essence_runtime;

mod requirements;
pub(crate) use requirements::*;

mod scrolling;
pub(crate) use scrolling::*;

mod runtime;
