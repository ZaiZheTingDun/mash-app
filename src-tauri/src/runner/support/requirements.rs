//! Support-row level, skill, score, and diagnostic evaluation.

use super::*;

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
