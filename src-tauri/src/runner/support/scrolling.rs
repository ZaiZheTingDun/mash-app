//! Support-list scroll geometry, timing, progress detection, and diagnostics.

use super::*;

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

// Keep the icon geometry/threshold aligned with SUPPORT_GRAND_BADGE_* in
// the sidecar. An icon hidden behind the class bar cannot prove an ordinary row.
const SUPPORT_GRAND_ICON_THRESHOLD: f64 = 0.80;
const SUPPORT_GRAND_ICON_DY: f64 = -23.0 / 1440.0;
const SUPPORT_GRAND_ICON_H: f64 = 50.0 / 1440.0;
const SUPPORT_GRAND_ICON_PAD_Y: f64 = 0.008;
const SUPPORT_ROW_TO_CONFIRM_DY: f64 = 0.116;
const SUPPORT_ROW_ANCHOR_TOLERANCE: f64 = 0.035;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SupportGrandRowKind {
    Grand,
    Ordinary,
    Unconfirmed,
}

impl SupportGrandRowKind {
    fn label(self) -> &'static str {
        match self {
            Self::Grand => "冠位",
            Self::Ordinary => "普通",
            Self::Unconfirmed => "未确认",
        }
    }
}

#[derive(Debug)]
struct SupportGrandRowProbe {
    anchor: NormRect,
    score: Option<f64>,
    kind: SupportGrandRowKind,
}

#[derive(Debug)]
pub(crate) struct SupportGrandPageProbe {
    rows: Vec<SupportGrandRowProbe>,
}

impl SupportGrandPageProbe {
    pub(crate) fn from_diagnostics(diagnostics: &crate::screen::SupportDiagnostics) -> Self {
        let rows = diagnostics
            .confirm_button_anchors
            .iter()
            .enumerate()
            .map(|(index, anchor)| {
                let score = diagnostics
                    .grand_icon_anchor_scores
                    .get(index)
                    .copied()
                    .flatten();
                let icon_top = anchor.y + SUPPORT_GRAND_ICON_DY;
                let fully_visible = icon_top.is_finite()
                    && diagnostics.list_region.y.is_finite()
                    && icon_top >= diagnostics.list_region.y + SUPPORT_GRAND_ICON_PAD_Y
                    && icon_top + SUPPORT_GRAND_ICON_H <= 1.0;
                let kind = match score.filter(|s| s.is_finite() && (-1.0..=1.0).contains(s)) {
                    Some(score) if fully_visible && score >= SUPPORT_GRAND_ICON_THRESHOLD => {
                        SupportGrandRowKind::Grand
                    }
                    Some(_) if fully_visible => SupportGrandRowKind::Ordinary,
                    _ => SupportGrandRowKind::Unconfirmed,
                };
                SupportGrandRowProbe {
                    anchor: *anchor,
                    score,
                    kind,
                }
            })
            .collect();
        Self { rows }
    }

    pub(crate) fn exhausted(&self) -> bool {
        // Grand rows precede ordinary rows. A mixed page already contains
        // the boundary, so scan its Grand candidates and never scroll again.
        self.rows
            .iter()
            .any(|row| row.kind == SupportGrandRowKind::Ordinary)
    }

    pub(crate) fn allows_candidate(&self, grand_mode: bool, row: &SupportRowMatch) -> bool {
        if !grand_mode {
            return true;
        }
        // Prefer the exact confirm-button association supplied by the sidecar.
        // Name-only OCR rows use the same row-top projection as that association.
        let expected_y = row
            .score_anchor
            .map(|a| a.y)
            .unwrap_or(row.row_region.y - SUPPORT_ROW_TO_CONFIRM_DY);
        self.rows
            .iter()
            .filter_map(|probe| {
                let distance = (probe.anchor.y - expected_y).abs();
                (distance <= SUPPORT_ROW_ANCHOR_TOLERANCE).then_some((distance, probe.kind))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .is_some_and(|(_, kind)| kind == SupportGrandRowKind::Grand)
    }
}

/// Mode labels describe the configured search, not the current viewport.
pub(crate) fn support_not_found_scroll_message(
    name: &str,
    grand_mode: bool,
    attempt: u32,
) -> String {
    let mode = if grand_mode {
        "冠位模式"
    } else {
        "普通模式"
    };
    format!("未找到从者 {name} [{mode}]，继续滚动 (第 {attempt} 次)")
}

/// Full-track thumb observations are independent of the runner's scroll
/// count: entering a support page can preserve an intermediate scroll offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SupportScrollEndDecision {
    AtEnd,
    Scrollable,
    Unconfirmed,
    NoScrollbar,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct SupportScrollEndProgress {
    absent_at_scroll: Option<u32>,
}

impl SupportScrollEndProgress {
    pub(crate) fn observe(
        &mut self,
        thumb_start: &crate::screen::ElementMatch,
        thumb_end: &crate::screen::ElementMatch,
        bottom_marker_found: bool,
        scroll_count: u32,
    ) -> SupportScrollEndDecision {
        if thumb_start.found || thumb_end.found {
            self.absent_at_scroll = None;
            // A partial or geometrically inconsistent match is uncertain,
            // never evidence of an exhausted or non-scrollable list.
            if !(thumb_start.found
                && thumb_end.found
                && (thumb_start.x - thumb_end.x).abs() <= 0.015
                && thumb_start.y < thumb_end.y)
            {
                return SupportScrollEndDecision::Unconfirmed;
            }
            return if bottom_marker_found && thumb_end.y >= 0.90 {
                SupportScrollEndDecision::AtEnd
            } else {
                SupportScrollEndDecision::Scrollable
            };
        }

        // A short list may have no scrollbar. Require another observation
        // after an actual scroll attempt instead of trusting one CV miss.
        if self
            .absent_at_scroll
            .is_some_and(|count| scroll_count > count)
        {
            self.absent_at_scroll = None;
            SupportScrollEndDecision::NoScrollbar
        } else {
            self.absent_at_scroll = Some(scroll_count);
            SupportScrollEndDecision::Unconfirmed
        }
    }
}

/// Evaluate both independent end conditions. The scrollbar probe also emits
/// the scan diagnostics, so Grand exhaustion must not short-circuit it.
pub(crate) fn support_list_exhausted(
    grand_section_exhausted: bool,
    scrollbar_probe: impl FnOnce() -> AutomationResult<bool>,
) -> AutomationResult<bool> {
    let scrollbar_exhausted = scrollbar_probe()?;
    Ok(grand_section_exhausted || scrollbar_exhausted)
}

pub(crate) fn support_grand_section_probe_message(probe: &SupportGrandPageProbe) -> String {
    let count = |kind| probe.rows.iter().filter(|row| row.kind == kind).count();
    let rows = probe
        .rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let score = row
                .score
                .filter(|s| s.is_finite())
                .map(|s| format!("{s:.3}"))
                .unwrap_or_else(|| "未知".into());
            format!("#{} {}={score}", index + 1, row.kind.label())
        })
        .collect::<Vec<_>>()
        .join(", ");
    let decision = if probe.exhausted() {
        "已出现普通助战，冠位区域扫描结束"
    } else {
        "尚未确认普通助战，继续查找"
    };
    format!(
        "冠位区域: 冠位行={} 普通行={} 未确认行={} 逐行=[{rows}] 判定={decision}",
        count(SupportGrandRowKind::Grand),
        count(SupportGrandRowKind::Ordinary),
        count(SupportGrandRowKind::Unconfirmed),
    )
}
