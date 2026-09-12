//! Support-list scrolling, row matching, CE search regions, and skill-level filters.

use super::*;

pub(crate) fn is_unknown_element_error(err: &str, screen: &str, element: &str) -> bool {
    err.contains(&format!("unknown element: {screen}.{element}"))
}

/// Compute the y-delta a support-list scroll swipe should travel so
/// the *lowest visible row* on the current page ends up at the same
/// Compute the support-list scroll distance from the last visible
/// confirm-button anchor only. The goal is simple and observable:
/// move the bottom-most detected button to the first-row button y
/// (`SUPPORT_SCROLL_TARGET_TOP_ANCHOR_Y`). We deliberately do not
/// extrapolate hidden/partial rows from the visible row pitch because
/// that can skip a servant that is only partially visible at the bottom.
pub(crate) fn scroll_support_list_delta(confirm_button_anchors: &[NormRect]) -> f64 {
    let Some(bottom_y) = confirm_button_anchors
        .iter()
        .map(|anchor| anchor.y)
        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
    else {
        return SUPPORT_SCROLL_FALLBACK_DELTA;
    };
    (bottom_y - SUPPORT_SCROLL_TARGET_TOP_ANCHOR_Y)
        .clamp(SUPPORT_SCROLL_MIN_DELTA, SUPPORT_SCROLL_MAX_DELTA)
}

/// Pick the active-motion (MOVE-phase) duration in ms for a settle
/// swipe. The actual lift-off velocity is governed by the trailing
/// `SUPPORT_SCROLL_SETTLE_MS` hold inside `Adb::swipe_with_settle`, so
/// here we only need to pick a duration that produces visually smooth
/// motion (not too jumpy on long deltas, not too long on tiny ones).
/// `SUPPORT_SCROLL_VELOCITY` is interpreted as normalized screen units
/// per second of *active* motion; with no fling to worry about, we can
/// run this much faster than the old all-linear swipe needed to. The
/// total realized swipe time is roughly
/// `scroll_support_list_duration_ms(delta) + SUPPORT_SCROLL_SETTLE_MS`
/// plus per-event ADB overhead.
pub(crate) fn scroll_support_list_duration_ms(delta: f64) -> u32 {
    let raw_ms = (delta.abs() / SUPPORT_SCROLL_VELOCITY * 1000.0).round();
    let raw_ms = raw_ms.clamp(0.0, u32::MAX as f64) as u32;
    raw_ms.clamp(
        SUPPORT_SCROLL_MIN_DURATION_MS,
        SUPPORT_SCROLL_MAX_DURATION_MS,
    )
}

/// Render a one-line, human-readable summary of a support-list scroll
/// decision for the debug log. Kept compact (single line, three digits
/// of precision) so the operation-log panel stays readable when many
/// scrolls scroll past in a row.
pub(crate) fn format_scroll_debug(
    confirm_button_anchors: &[NormRect],
    delta: f64,
    from_y: f64,
    to_y: f64,
    swipe_ms: u32,
    settle_ms: u32,
) -> String {
    let mut anchor_ys: Vec<f64> = confirm_button_anchors.iter().map(|a| a.y).collect();
    anchor_ys.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let anchor_list = if anchor_ys.is_empty() {
        "无".to_string()
    } else {
        anchor_ys
            .iter()
            .map(|y| format!("{y:.3}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "滚动助战列表: 按钮 y=[{anchor_list}] (n={}) Δ={delta:.3} swipe={from_y:.2}→{to_y:.2} ({swipe_ms}ms+{settle_ms}ms settle)",
        confirm_button_anchors.len(),
    )
}

pub(crate) fn support_grand_section_exhausted_after_probe(
    visible: Option<bool>,
    seen: &mut bool,
    consecutive_misses: &mut u8,
) -> bool {
    match visible {
        Some(true) => {
            *seen = true;
            *consecutive_misses = 0;
            false
        }
        Some(false) if *seen => {
            *consecutive_misses = consecutive_misses.saturating_add(1);
            *consecutive_misses >= 2
        }
        Some(false) => false,
        None => false,
    }
}

/// Format the normal-log message emitted when a support scan found no
/// matching servant and will scroll the list. Make the visible Grand-row
/// detection explicit so operators can distinguish a Grand scan from an
/// ordinary support-list scroll at a glance.
pub(crate) fn support_not_found_scroll_message(
    name: &str,
    grand_visible_in_grand_mode: bool,
    attempt: u32,
) -> String {
    let (grand_label, scroll_action) = if grand_visible_in_grand_mode {
        ("冠位", "继续滚动识别冠位从者")
    } else {
        ("非冠位", "继续滚动")
    };
    format!("未找到从者 {name} [{grand_label}]，{scroll_action} (第 {attempt} 次)")
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
/// Settle time after the support-list scroll swipe completes, before
/// the next OCR pass. The settle hold *inside* `swipe_with_settle`
/// already lifts the finger at near-zero velocity (see
/// `SUPPORT_SCROLL_SETTLE_MS`), so the list isn't bouncing or flinging
/// when we wake up — this is just the buffer for the row cards to
/// re-render at their new positions. Empirically the redraw completes
/// in well under 300 ms on real devices; 450 ms keeps comfortable
/// headroom for slower emulators / a low-end CPU without pinning the
/// runner at the old "wait nearly a second between every scroll"
/// cadence (was 900 ms before the fling-avoiding settle gesture made
/// the longer wait redundant).
pub(crate) const SUPPORT_SCROLL_SETTLE: Duration = Duration::from_millis(450);
/// The y from which the scroll swipe starts (finger-down point). Sits in
/// the lower half of the list so the symmetric `to` point can always
/// stay above it for an "up" swipe even at `SUPPORT_SCROLL_MAX_DELTA`.
pub(crate) const SUPPORT_SCROLL_FROM_Y: f64 = 0.78;
/// Target y for the bottom-most detected support confirm button after
/// a scroll. This is the first-row button position on the support list.
pub(crate) const SUPPORT_SCROLL_TARGET_TOP_ANCHOR_Y: f64 = 0.30;
/// Minimum scroll delta. Guarantees forward progress when the bottom-most
/// detected confirm button is already near the first-row target.
pub(crate) const SUPPORT_SCROLL_MIN_DELTA: f64 = 0.10;
/// Maximum scroll delta. Caps the swipe so a bogus anchor near the very
/// bottom of the screen can't fling the list past several pages in one go.
pub(crate) const SUPPORT_SCROLL_MAX_DELTA: f64 = 0.65;
/// Legacy fixed scroll delta — used only when no confirm-button anchors
/// were detected so we still make progress on devices / resolutions
/// where the template / shape detector misses the button column.
pub(crate) const SUPPORT_SCROLL_FALLBACK_DELTA: f64 = 0.40;
/// Normalized screen units (fraction of screen height) per second
/// for the *active* motion phase of the settle swipe. The
/// fling-protection settle hold (see `SUPPORT_SCROLL_SETTLE_MS`)
/// decouples lift-off velocity from this constant, so we can pick a
/// value purely for visual / cycle-time reasons — it does NOT need
/// to stay under Android's per-device fling threshold the way a
/// plain `input swipe` would.
///
/// 2.0 norm/s ≈ two full screen heights per second; for the
/// canonical Δ≈0.555 support-list scroll that's ~278 ms of motion +
/// the ~250 ms settle hold ≈ ~530 ms total active gesture. The 50 Hz
/// MOVE cadence is preserved at this velocity because
/// `settle_swipe_move_steps` scales the step count with `swipe_ms`
/// (≈14 MOVE events at 278 ms), so the active phase stays visibly
/// smooth instead of degenerating into a few jumpy steps.
pub(crate) const SUPPORT_SCROLL_VELOCITY: f64 = 2.0;
/// Lower bound on the swipe duration so a tiny min-delta scroll still
/// reads as a deliberate gesture to the touch dispatcher.
pub(crate) const SUPPORT_SCROLL_MIN_DURATION_MS: u32 = 250;
/// Upper bound on the swipe duration so a pathologically large delta
/// can't stall the runner with a multi-second swipe.
pub(crate) const SUPPORT_SCROLL_MAX_DURATION_MS: u32 = 2000;

/// How long the finger holds at the destination before lifting off in a
/// settle-style support-list scroll. Must exceed Android's velocity
/// tracker sliding window (~100 ms on most devices) so the tracker
/// sees a stretch of "no motion" right before UP and reports ~0 px/s.
/// 250 ms gives ~2.5× the velocity-tracker window on modern Android
/// (the window shortened from ~100 ms on older versions to ~80 ms on
/// 12+), comfortably above the fling threshold while shaving 150 ms
/// off each scroll cycle vs. the original 400 ms — empirically the
/// list still stops dead at the lift point on the devices we've
/// tested. Bump back up if a future device leaks a fling.
pub(crate) const SUPPORT_SCROLL_SETTLE_MS: u32 = 250;
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
pub(crate) const SUPPORT_EXTRA_FILTER_DIALOG_ELEMENT: &str = "dialog_extra_class_filter";
pub(crate) const SUPPORT_EXTRA_FILTER_LONG_PRESS_MS: u32 = 2000;
pub(crate) const SUPPORT_EXTRA_FILTER_CONFIRM_PRESS_MS: u32 = 100;
pub(crate) const SUPPORT_EXTRA_FILTER_DIALOG_TIMEOUT: Duration = Duration::from_secs(3);
pub(crate) const SUPPORT_EXTRA_FILTER_DIALOG_POLL: Duration = Duration::from_millis(200);
pub(crate) const SUPPORT_EXTRA_FILTER_ACTION_SETTLE: Duration = Duration::from_millis(300);
pub(crate) const SUPPORT_EXTRA_FILTER_RESULT_SETTLE: Duration = Duration::from_secs(2);
pub(crate) const SUPPORT_EXTRA_FILTER_RESET_BUTTON: Point = Point::new(0.292, 0.778);
pub(crate) const SUPPORT_EXTRA_FILTER_CONFIRM_BUTTON: Point = Point::new(0.742, 0.778);

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

/// Class-filter tab bar across the top of the support-select screen. All
/// tabs share the same y. Order mirrors the FGO UI: all → saber → ...
/// → berserker → extra → mix. Lookup happens via `class_tab_for` which
/// maps Atlas Academy `className` strings into one of these tabs.
pub(crate) const SUPPORT_CLASS_TAB_Y: f64 = 0.178;
pub(crate) const SUPPORT_TAB_SABER: Point = Point::new(0.1246, SUPPORT_CLASS_TAB_Y);
pub(crate) const SUPPORT_TAB_ARCHER: Point = Point::new(0.1773, SUPPORT_CLASS_TAB_Y);
pub(crate) const SUPPORT_TAB_LANCER: Point = Point::new(0.2301, SUPPORT_CLASS_TAB_Y);
pub(crate) const SUPPORT_TAB_RIDER: Point = Point::new(0.2828, SUPPORT_CLASS_TAB_Y);
pub(crate) const SUPPORT_TAB_CASTER: Point = Point::new(0.3355, SUPPORT_CLASS_TAB_Y);
pub(crate) const SUPPORT_TAB_ASSASSIN: Point = Point::new(0.3883, SUPPORT_CLASS_TAB_Y);
pub(crate) const SUPPORT_TAB_BERSERKER: Point = Point::new(0.4410, SUPPORT_CLASS_TAB_Y);
pub(crate) const SUPPORT_TAB_EXTRA: Point = Point::new(0.4938, SUPPORT_CLASS_TAB_Y);

#[derive(Debug, Clone, Copy)]
pub(crate) struct ExtraClassFilter {
    pub(crate) label: &'static str,
    pub(crate) point: Point,
}

impl ExtraClassFilter {
    const fn new(label: &'static str, x: f64, y: f64) -> Self {
        Self {
            label,
            point: Point::new(x, y),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum SupportClassFilterAction {
    Tap(Point),
    CnExtra(ExtraClassFilter),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SupportExtraFilterDialogWait {
    Matched,
    TimedOut,
    Aborted,
}

pub(crate) fn should_retry_support_extra_filter_dialog(wait: SupportExtraFilterDialogWait) -> bool {
    wait == SupportExtraFilterDialogWait::TimedOut
}

/// Settle time after tapping a class tab. The list animates a quick fade
/// when filtering; ~600ms is enough for the new rows to render before we
/// kick off the OCR pass.
pub(crate) const SUPPORT_CLASS_TAB_SETTLE: Duration = Duration::from_millis(600);

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SupportLevelFilter {
    Pass,
    /// The visible panel produced an OCR'd level that's below the
    /// configured minimum. The payload is a short human-readable reason
    /// (e.g. `"持有技能 2 ≥ 10（实际 8）"`) so the runner can surface
    /// *which* requirement failed in the UI log instead of just a
    /// generic "等级不匹配".
    Fail(String),
    WaitingForPanel,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct SupportLevelPanelProgress {
    pub(crate) candidate_key: Option<String>,
    pub(crate) owned_met: bool,
    pub(crate) append_met: bool,
    /// How many times we've tapped the "技能显示切换" button while still
    /// trying to verify this candidate's skill panels. Bounded by
    /// [`SUPPORT_SKILL_PANEL_MAX_TOGGLE_TAPS`]; exceeding it makes the
    /// runner give up on this row and continue scrolling. Reset
    /// implicitly whenever `candidate_key` rolls over.
    pub(crate) panel_toggle_taps: u32,
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

#[cfg(test)]
pub(crate) fn support_level_meets(actual: Option<u32>, required_min: Option<u32>) -> bool {
    match required_min {
        None => true,
        Some(required) => actual.is_some_and(|level| level >= required),
    }
}

/// Find the first slot whose configured minimum isn't satisfied by the
/// OCR'd value, returning `(slot_index, actual_level, required_min)`
/// for the caller to format. Slots whose `required` is `None` are
/// skipped entirely. Returns `None` when every required slot meets its
/// minimum.
pub(crate) fn support_first_level_mismatch(
    actual: &[Option<u32>],
    required: &[Option<u32>],
) -> Option<(usize, Option<u32>, u32)> {
    required
        .iter()
        .enumerate()
        .find_map(|(index, required_min)| {
            let min = (*required_min)?;
            let actual_level = actual.get(index).copied().flatten();
            if actual_level.is_some_and(|level| level >= min) {
                None
            } else {
                Some((index, actual_level, min))
            }
        })
}

/// Format an OCR'd level for log output. `None` becomes a plain `-` so
/// "the value wasn't read" reads distinctly from "the value was zero".
pub(crate) fn format_actual_level(actual: Option<u32>) -> String {
    actual.map(|v| v.to_string()).unwrap_or_else(|| "-".into())
}

pub(crate) fn support_level_candidate_key(row: &SupportRowMatch) -> String {
    format!(
        "{}|{}|{:.3}",
        row.name_text, row.np_matched_name, row.row_region.y
    )
}

pub(crate) fn format_optional_levels(levels: &[Option<u32>]) -> String {
    if levels.is_empty() {
        return "-".into();
    }
    levels
        .iter()
        .map(|level| format_actual_level(*level))
        .collect::<Vec<_>>()
        .join("/")
}

pub(crate) fn support_level_wait_diagnostic(
    row: &SupportRowMatch,
    previous_key: Option<&str>,
    current_key: &str,
) -> String {
    let key_state = match previous_key {
        None => "新候选".to_string(),
        Some(prev) if prev == current_key => "同一候选".to_string(),
        Some(prev) => format!("候选变化 {prev} -> {current_key}"),
    };
    format!(
        "panel={} score={}/{} np={} owned=[{}] append=[{}] y={:.3} key={}",
        row.skill_panel.as_deref().unwrap_or("-"),
        format_actual_level(row.star_map_score),
        format_actual_level(row.grand_star_map_score),
        format_actual_level(row.np_level),
        format_optional_levels(&row.skill_levels),
        format_optional_levels(&row.append_skill_levels),
        row.row_region.y,
        key_state,
    )
}

pub(crate) fn support_level_filtering_enabled(server: Server) -> bool {
    matches!(server, Server::Jp | Server::Cn)
}

pub(crate) fn support_score_filter_mismatch(
    row: &SupportRowMatch,
    grand_mode: bool,
    star_map_score_min: Option<u32>,
    grand_star_map_score_min: Option<u32>,
) -> Option<String> {
    if let Some(min) = star_map_score_min {
        if !row.star_map_score.is_some_and(|score| score >= min) {
            return Some(format!(
                "星图分值 ≥ {}（实际 {}）",
                min,
                format_actual_level(row.star_map_score),
            ));
        }
    }
    if grand_mode {
        if let Some(min) = grand_star_map_score_min {
            if !row.grand_star_map_score.is_some_and(|score| score >= min) {
                return Some(format!(
                    "冠位星图分值 ≥ {}（实际 {}）",
                    min,
                    format_actual_level(row.grand_star_map_score),
                ));
            }
        }
    }
    None
}

pub(crate) fn support_row_matches_level_requirements_with_progress(
    server: Server,
    config: &RunConfig,
    row: &SupportRowMatch,
    progress: &mut SupportLevelPanelProgress,
) -> SupportLevelFilter {
    let needs_score = config.support_star_map_score_min.is_some();
    let needs_grand_score =
        config.support_grand_mode && config.support_grand_star_map_score_min.is_some();
    let needs_servant_level = config.support_servant_level_min.is_some();
    let needs_np = config.support_noble_phantasm_level_min.is_some();
    let needs_owned = config.support_skill_level_mins.iter().any(Option::is_some);
    let needs_append = config
        .support_append_skill_level_mins
        .iter()
        .any(Option::is_some);
    if !support_level_filtering_enabled(server)
        || (!needs_score
            && !needs_grand_score
            && !needs_servant_level
            && !needs_np
            && !needs_owned
            && !needs_append)
    {
        return SupportLevelFilter::Pass;
    }
    if let Some(reason) = support_score_filter_mismatch(
        row,
        config.support_grand_mode,
        config.support_star_map_score_min,
        config.support_grand_star_map_score_min,
    ) {
        return SupportLevelFilter::Fail(reason);
    }
    if let Some(min) = config.support_servant_level_min {
        if !row.servant_level.is_some_and(|level| level >= min) {
            return SupportLevelFilter::Fail(format!(
                "从者等级 ≥ {}（实际 {}）",
                min,
                format_actual_level(row.servant_level),
            ));
        }
    }
    if let Some(min) = config.support_noble_phantasm_level_min {
        if !row.np_level.is_some_and(|level| level >= min) {
            return SupportLevelFilter::Fail(format!(
                "宝具 ≥ {}（实际 {}）",
                min,
                format_actual_level(row.np_level),
            ));
        }
    }
    if !needs_owned && !needs_append {
        return SupportLevelFilter::Pass;
    }

    let key = support_level_candidate_key(row);
    if progress.candidate_key.as_deref() != Some(key.as_str()) {
        progress.candidate_key = Some(key);
        progress.owned_met = false;
        progress.append_met = false;
        progress.panel_toggle_taps = 0;
    }

    match row.skill_panel.as_deref() {
        Some("owned") if needs_owned => {
            if let Some((index, actual, min)) =
                support_first_level_mismatch(&row.skill_levels, &config.support_skill_level_mins)
            {
                progress.owned_met = false;
                return SupportLevelFilter::Fail(format!(
                    "持有技能 {} ≥ {}（实际 {}）",
                    index + 1,
                    min,
                    format_actual_level(actual),
                ));
            }
            progress.owned_met = true;
        }
        Some("append") if needs_append => {
            if let Some((index, actual, min)) = support_first_level_mismatch(
                &row.append_skill_levels,
                &config.support_append_skill_level_mins,
            ) {
                progress.append_met = false;
                return SupportLevelFilter::Fail(format!(
                    "追加技能 {} ≥ {}（实际 {}）",
                    index + 1,
                    min,
                    format_actual_level(actual),
                ));
            }
            progress.append_met = true;
        }
        Some(_) | None => {}
    }

    if (!needs_owned || progress.owned_met) && (!needs_append || progress.append_met) {
        SupportLevelFilter::Pass
    } else {
        SupportLevelFilter::WaitingForPanel
    }
}

pub(crate) fn support_skill_diag_message(row: &SupportRowMatch) -> String {
    if row.skill_level_diagnostics.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = row
        .skill_level_diagnostics
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let level = item
                .get("level")
                .and_then(|v| v.as_u64())
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".into());
            let score = item.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let source = item.get("source").and_then(|v| v.as_str()).unwrap_or("");
            format!("{}:{}@{:.2}/{}", index + 1, level, score, source)
        })
        .collect();
    format!(" skill [{}]", parts.join(" "))
}

fn format_support_star_map_score(score: Option<u32>) -> String {
    score
        .map(|value| format!("+{value}"))
        .unwrap_or_else(|| "-".into())
}

fn format_support_skill_levels(levels: &[Option<u32>]) -> String {
    if levels.is_empty() {
        return "-".into();
    }
    levels
        .iter()
        .map(|level| format_actual_level(*level))
        .collect::<Vec<_>>()
        .join(",")
}

pub(crate) fn support_found_summary(name: &str, row: &SupportRowMatch) -> String {
    format!(
        "找到助战 [{name}] 宝具等级 [{}] 技能 [{}] 星图分值 [{}/{}]",
        format_actual_level(row.np_level),
        format_support_skill_levels(&row.skill_levels),
        format_support_star_map_score(row.star_map_score),
        format_support_star_map_score(row.grand_star_map_score),
    )
}

pub(crate) fn support_found_debug_detail(name: &str, row: &SupportRowMatch) -> String {
    format!(
        "找到助战 {} (name {:.2}, np {:.2}){}",
        name,
        row.name_score,
        row.np_score,
        support_skill_diag_message(row),
    )
}

/// Map an Atlas Academy `className` (already lowercased by
/// `load_servant_metadata`) to the support-select class-filter tab.
/// Returns `None` for unknown / boss-only class variants so the
/// caller can skip the tap and log it instead of guessing wrong.
pub(crate) fn class_tab_for(class_name: &str) -> Option<Point> {
    match class_name {
        "saber" => Some(SUPPORT_TAB_SABER),
        "archer" => Some(SUPPORT_TAB_ARCHER),
        "lancer" => Some(SUPPORT_TAB_LANCER),
        "rider" => Some(SUPPORT_TAB_RIDER),
        "caster" => Some(SUPPORT_TAB_CASTER),
        "assassin" => Some(SUPPORT_TAB_ASSASSIN),
        "berserker" => Some(SUPPORT_TAB_BERSERKER),
        // "Extra" tab covers every non-knight / non-cavalry class:
        // shielder, ruler, avenger, alterego, mooncancer, foreigner,
        // pretender, plus the three playable Beast classes. Atlas mixes camelCase and lowercase forms so
        // `load_servant_metadata` lowercases before we land here.
        "shielder" | "ruler" | "avenger" | "mooncancer" | "alterego" | "foreigner"
        | "pretender" | "beast" | "beasteresh" | "unbeastolgamarie" => Some(SUPPORT_TAB_EXTRA),
        _ => None,
    }
}

/// CN exposes a second-level EXTRA class dialog on long press. Other
/// servers retain the original single-tap EXTRA behaviour.
pub(crate) fn support_class_filter_action(
    server: Server,
    class_name: &str,
    cn_extra_filter_enabled: bool,
    cn_extra_configured: bool,
) -> Option<SupportClassFilterAction> {
    if server == Server::Cn {
        let extra = match class_name {
            "shielder" => Some(ExtraClassFilter::new("盾兵", 0.260, 0.427)),
            "ruler" => Some(ExtraClassFilter::new("裁定者", 0.420, 0.427)),
            "avenger" => Some(ExtraClassFilter::new("复仇者", 0.580, 0.427)),
            "mooncancer" => Some(ExtraClassFilter::new("月之癌", 0.740, 0.427)),
            "alterego" => Some(ExtraClassFilter::new("他人格", 0.260, 0.649)),
            "foreigner" => Some(ExtraClassFilter::new("降临者", 0.420, 0.649)),
            "pretender" => Some(ExtraClassFilter::new("身披角色者", 0.580, 0.649)),
            "beast" | "beasteresh" | "unbeastolgamarie" => {
                Some(ExtraClassFilter::new("兽", 0.740, 0.649))
            }
            _ => None,
        };
        if let Some(extra) = extra {
            return Some(if !cn_extra_filter_enabled || cn_extra_configured {
                SupportClassFilterAction::Tap(SUPPORT_TAB_EXTRA)
            } else {
                SupportClassFilterAction::CnExtra(extra)
            });
        }
    }
    class_tab_for(class_name).map(SupportClassFilterAction::Tap)
}

mod runtime;
