//! Support craft-essence configuration, search geometry, and verification diagnostics.

use super::*;

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

/// Ordinary support CE thumbnails occupy a narrow strip at the lower-left
/// of the row. The confirm button anchor gives a stable vertical reference
/// even when OCR row bounds vary; x is fixed because CE thumbnails stay in
/// the left column. The window includes a small margin around the rendered
/// CE image while excluding the servant portrait.
pub(crate) const SUPPORT_CE_X: f64 = 0.030;
pub(crate) const SUPPORT_CE_W: f64 = 0.145;
pub(crate) const SUPPORT_CE_H: f64 = 0.080;
pub(crate) const SUPPORT_CE_CENTER_FROM_BUTTON_TOP_Y: f64 = 0.200;
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
    pub(super) reason: String,
    pub(super) debug_summary: Option<String>,
}

impl SupportCeMismatch {
    fn new(reason: String, debug_summary: Option<String>) -> Self {
        Self {
            reason,
            debug_summary,
        }
    }

    pub(super) fn reason_only(reason: impl Into<String>) -> Self {
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

/// Project the ordinary CE strip from the row's right-side confirm-button
/// anchor. Extracted as a pure helper so its normalized geometry can be
/// tested independently from OCR and device state.
pub(crate) fn ce_search_region(confirm_button: NormRect) -> NormRect {
    NormRect {
        x: SUPPORT_CE_X,
        y: confirm_button.y + SUPPORT_CE_CENTER_FROM_BUTTON_TOP_Y - SUPPORT_CE_H / 2.0,
        w: SUPPORT_CE_W,
        h: SUPPORT_CE_H,
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
