//! Support-list scrolling, row matching, CE search regions, and skill-level filters.

use super::*;

pub(crate) fn is_unknown_element_error(err: &str, screen: &str, element: &str) -> bool {
    err.contains(&format!("unknown element: {screen}.{element}"))
}

pub(crate) fn ordinary_support_ce_ids(config: &RunConfig) -> Vec<u32> {
    let mut ids = Vec::new();
    for id in config
        .support_craft_essence_ids
        .iter()
        .copied()
        .chain(config.support_craft_essence_id)
    {
        if !ids.contains(&id) {
            ids.push(id);
        }
        if ids.len() == 10 {
            break;
        }
    }
    ids
}

pub(crate) fn grand_support_ce_ids(config: &RunConfig, index: usize) -> Vec<u32> {
    let limit = if index == 1 { 1 } else { 10 };
    let mut ids = Vec::new();
    for id in config.support_grand_craft_essence_id_lists[index]
        .iter()
        .copied()
        .chain(config.support_grand_craft_essence_ids[index])
    {
        if !ids.contains(&id) {
            ids.push(id);
        }
        if ids.len() == limit {
            break;
        }
    }
    ids
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

/// Search window for the support row's craft-essence icon, expressed as
/// fractions of the row bbox. The OCR-derived `row_region` only covers
/// the name + NP text strip (anchored to `SUPPORT_LIST_REGION.x`/`.w`);
/// the CE icon overlay actually sits on the **face card to the left of
/// the row**, so `x` is negative on purpose to push the search window
/// outside the row's left edge. `h > 1.0` lets the window span the
/// face vertically (the face is taller than the text strip).
///
/// Reference screenshot: `tests/test_data/screenshots/support_select.png`
/// (1024×576). row_region for row 1: `x≈0.177, w≈0.466, y≈0.30, h≈0.13`.
/// With the values below the search window resolves to roughly
/// `x≈0.05, w≈0.10, y≈0.39, h≈0.13` — bottom of the face card. Retune
/// via the debug page (`助战识别` panel) against real captures.
pub(crate) const SUPPORT_CE_OFFSET_IN_ROW: NormRect = NormRect {
    x: -0.33,
    y: -1.60,
    w: 0.35,
    h: 3.45,
};
/// Grand support rows show three CE strips in a fixed left-side column.
/// Their vertical position tracks the right-side "助战编队确认" panel in
/// Grand support rows. The panel's top edge is cleaner than the score
/// badge because it has no overflowing numeric text.
pub(crate) const SUPPORT_GRAND_CE_X: f64 = 0.172;
pub(crate) const SUPPORT_GRAND_CE_W: f64 = 0.124;
pub(crate) const SUPPORT_GRAND_CE_H: f64 = 0.064;
pub(crate) const SUPPORT_GRAND_CE_GAP: f64 = 0.000;
pub(crate) const SUPPORT_GRAND_CE_THIRD_CENTER_FROM_BUTTON_TOP_Y: f64 = 0.180;
pub(crate) const SUPPORT_GRAND_CE_THIRD_CENTER_FROM_PANEL_TOP_Y: f64 = 0.220;
/// Support rows have a right-side "助战编队确认" button that opens the
/// friend/support detail page. Use that button as a vertical row anchor,
/// but tap slightly to its left so selecting a support stays on the main
/// row hit-area.
pub(crate) const SUPPORT_SELECT_TAP_LEFT_OF_CONFIRM_BUTTON_X: f64 = 0.030;

pub(crate) fn format_ce_artwork_checks(checks: &[SupportCeArtworkCheck]) -> String {
    checks
        .iter()
        .map(|check| {
            let marker = if check.selected {
                "*"
            } else if check.passed {
                "✓"
            } else {
                "✗"
            };
            format!(
                "{}:{} {:.3}/{:.2}{}",
                check.region_kind, check.variant, check.score, check.threshold, marker
            )
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

fn ce_artwork_variant_label(variant: &str) -> &str {
    match variant {
        "full" => "完整匹配",
        "center" => "中间匹配",
        "top_right" => "右上匹配",
        other => other,
    }
}

fn ce_icon_check_label(kind: &str) -> &str {
    match kind {
        "mlb" => "满破图标",
        "grandBond" | "grandBondNp" => "牵绊图标",
        other => other,
    }
}

pub(crate) fn format_ce_verification_summary(
    artwork_checks: &[SupportCeArtworkCheck],
    icon_checks: &[SupportCeIconCheck],
) -> String {
    let selected_region_kind = artwork_checks
        .iter()
        .find(|check| check.selected)
        .map(|check| check.region_kind.as_str());
    let artwork_parts = artwork_checks
        .iter()
        .filter(|check| {
            selected_region_kind
                .map(|region_kind| check.region_kind == region_kind)
                .unwrap_or(true)
        })
        .map(|check| {
            format!(
                "{}（{:.2}/{:.2}）",
                ce_artwork_variant_label(&check.variant),
                check.score,
                check.threshold
            )
        });
    let icon_parts = icon_checks.iter().map(|check| {
        format!(
            "{}（{:.2}/{:.2}）",
            ce_icon_check_label(&check.kind),
            check.score,
            check.threshold
        )
    });
    artwork_parts
        .chain(icon_parts)
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SupportCeMismatch {
    reason: String,
    debug_summary: Option<String>,
}

impl SupportCeMismatch {
    fn new(reason: String, debug_summary: Option<String>) -> Self {
        Self {
            reason,
            debug_summary,
        }
    }

    fn reason_only(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            debug_summary: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn reason(&self) -> &str {
        &self.reason
    }

    #[cfg(test)]
    pub(crate) fn debug_summary(&self) -> Option<&str> {
        self.debug_summary.as_deref()
    }
}

pub(crate) fn mismatch_from_ce_result(
    label: &str,
    result: &SupportCeVerificationResult,
    effective_threshold: f64,
) -> Option<SupportCeMismatch> {
    let summary = format_ce_verification_summary(&result.artwork_checks, &result.icon_checks);
    let debug_summary = (!summary.is_empty()).then(|| format!("{label}：{summary}"));
    if result.passed {
        None
    } else if !result.full_gate_passed && result.score >= effective_threshold {
        let hint = format!(
            "当完整匹配不满足且中间匹配或右上匹配满足时，要求完整匹配至少为 {:.2}，可在设置调整",
            result.full_gate_threshold
        );
        let reason = format!(
            "{label} 完整匹配不足（{:.2}/{:.2}）：{hint}",
            result.full_gate_score, result.full_gate_threshold
        );
        Some(SupportCeMismatch::new(reason, debug_summary))
    } else if let Some(check) = result.icon_checks.iter().find(|check| !check.passed) {
        let kind = match check.kind.as_str() {
            "mlb" => "满破图标",
            "grandBond" => "原始牵绊图标",
            "grandBondNp" => "冠位连接牵绊图标",
            other => other,
        };
        Some(SupportCeMismatch::new(
            format!("{label} {kind}不匹配"),
            debug_summary,
        ))
    } else {
        Some(SupportCeMismatch::new(
            format!("{label} 不匹配"),
            debug_summary,
        ))
    }
}

/// Pure helper: apply [`SUPPORT_CE_OFFSET_IN_ROW`] (a row-local rect) to
/// `row` (an absolute row bbox) and return the absolute search window for
/// the row's CE icon. Extracted from `Runner::support_ce_search_region`
/// so unit tests can exercise the math directly without needing to build
/// a full `SupportRowMatch`.
pub(crate) fn ce_search_region(row: NormRect) -> NormRect {
    NormRect {
        x: row.x + SUPPORT_CE_OFFSET_IN_ROW.x * row.w,
        y: row.y + SUPPORT_CE_OFFSET_IN_ROW.y * row.h,
        w: SUPPORT_CE_OFFSET_IN_ROW.w * row.w,
        h: SUPPORT_CE_OFFSET_IN_ROW.h * row.h,
    }
}

pub(crate) fn grand_ce_search_region_from_third_center(
    third_center_y: f64,
    slot: usize,
) -> Option<NormRect> {
    if slot >= 3 {
        return None;
    }
    let pitch = SUPPORT_GRAND_CE_H + SUPPORT_GRAND_CE_GAP;
    let center_y = third_center_y - (2 - slot) as f64 * pitch;
    Some(NormRect {
        x: SUPPORT_GRAND_CE_X,
        y: center_y - SUPPORT_GRAND_CE_H / 2.0,
        w: SUPPORT_GRAND_CE_W,
        h: SUPPORT_GRAND_CE_H,
    })
}

pub(crate) fn grand_ce_search_region(row: &SupportRowMatch, slot: usize) -> Option<NormRect> {
    let third_center_y = row
        .score_anchor
        .as_ref()
        .map(|anchor| {
            let offset = if anchor.h <= 0.075 {
                SUPPORT_GRAND_CE_THIRD_CENTER_FROM_BUTTON_TOP_Y
            } else {
                SUPPORT_GRAND_CE_THIRD_CENTER_FROM_PANEL_TOP_Y
            };
            anchor.y + offset
        })
        .unwrap_or_else(|| {
            let r = row.row_region;
            r.y + r.h + 0.13
        });
    grand_ce_search_region_from_third_center(third_center_y, slot)
}

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

mod requirements;
pub(crate) use requirements::*;

mod scrolling;
pub(crate) use scrolling::*;

mod runtime;
