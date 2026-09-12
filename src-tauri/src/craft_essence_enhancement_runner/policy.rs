//! Pure decisions for the craft-essence enhancement workflow.
//!
//! This module owns stage transitions, inventory/material selection, and
//! screen/control classification. Device I/O remains in the parent runner.

use super::{
    CraftEssenceEnhancementMode, Screen, AUTO_CONFIG_OFF_MAX_SATURATION,
    AUTO_CONFIG_ON_MIN_SATURATION, BINARY_CONTROL_MIN_SCORE, BINARY_CONTROL_SCORE_MARGIN,
    ENHANCEMENT_MAIN_RETURN_CONFIRMATIONS, ENHANCE_BUTTON_NOT_READY_MAX_LUMA,
    ENHANCE_BUTTON_PRESENT_MIN_SCORE, ENHANCE_BUTTON_READY_MIN_LUMA,
    FILTER_SCROLLBAR_RESET_MAX_ATTEMPTS, FILTER_SCROLLBAR_TOP_MAX_Y, FILTER_TOGGLE_OFF_MAX_LUMA,
    FILTER_TOGGLE_ON_MIN_LUMA, LIST_RESET_MAX_ATTEMPTS, LIST_SCROLLBAR_LEGACY_TOP_MAX_CENTER_Y,
    LIST_SCROLLBAR_TOP_MAX_TOP_Y, PACKET_BATCH_SIZE,
};
use crate::screen::{CraftEssenceGridCell, ReadCraftEssenceMainTargetResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StrategyStage {
    SelectBomb,
    BombSelected,
    SelectBombBase,
    BombBaseSelected,
    FindBombBaseToLock,
    LockBombBaseActive,
    VerifyBombBaseLock,
    ExitBombBaseLockMode,
    SelectPacketBase,
    PacketSelected,
    PacketAutoFeedPending,
    FastAutoFeedPending,
    SelectBombForTransfer,
    BombSelectedForFeed,
    InspectBomb,
    QpEfficientComplete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RecommendMaterialProfile {
    TwoStarOnly,
    OneStarOnly,
    OneAndTwoStar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AutoFeedStrategy {
    QpEfficientPacket,
    FastBomb,
}

impl RecommendMaterialProfile {
    pub(super) fn includes_rarity(self, rarity: u8) -> bool {
        match self {
            Self::TwoStarOnly => rarity == 2,
            Self::OneStarOnly => rarity == 1,
            Self::OneAndTwoStar => matches!(rarity, 1 | 2),
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::TwoStarOnly => "仅 2 星未强化礼装",
            Self::OneStarOnly => "仅 1 星未强化礼装",
            Self::OneAndTwoStar => "1 星、2 星未强化礼装",
        }
    }
}

pub(super) fn recommend_profile_for_mode(
    mode: CraftEssenceEnhancementMode,
) -> RecommendMaterialProfile {
    if mode == CraftEssenceEnhancementMode::Fast {
        RecommendMaterialProfile::OneAndTwoStar
    } else {
        RecommendMaterialProfile::TwoStarOnly
    }
}

pub(super) fn stage_after_bomb_selection(mode: CraftEssenceEnhancementMode) -> StrategyStage {
    if mode == CraftEssenceEnhancementMode::Fast {
        StrategyStage::FastAutoFeedPending
    } else {
        StrategyStage::BombSelected
    }
}

pub(super) fn stage_after_missing_bomb() -> StrategyStage {
    StrategyStage::SelectBombBase
}

pub(super) fn fast_bomb_is_complete(target: &ReadCraftEssenceMainTargetResult) -> bool {
    target.level == Some(50) && target.level_cap == Some(50)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MaterialCounterDecision {
    Confirmed,
    ClearAutomaticSelection,
    AcceptLevelMax,
    Mismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PacketBaseExhaustionAction {
    ReturnToCurrentBomb,
    ReselectBomb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EnhancementCompletionError {
    MissingPending,
    IllegalStage(StrategyStage),
    TargetUnreadable,
    CapMismatch { expected: u32, actual: Option<u32> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum UnawaitedSuccessAction {
    ResumePending,
    IgnoreResidual,
}

pub(super) fn material_counter_decision(
    recorded: u8,
    displayed: u8,
    level_max_reached: bool,
) -> MaterialCounterDecision {
    if recorded == displayed {
        MaterialCounterDecision::Confirmed
    } else if recorded == 0 && displayed > 0 {
        MaterialCounterDecision::ClearAutomaticSelection
    } else if recorded > displayed && displayed > 0 && level_max_reached {
        MaterialCounterDecision::AcceptLevelMax
    } else {
        MaterialCounterDecision::Mismatch
    }
}

pub(super) fn confirm_pending_same_copy(
    decision: MaterialCounterDecision,
    pending: &mut bool,
    selected: &mut bool,
) -> bool {
    if decision == MaterialCounterDecision::Confirmed && *pending {
        *pending = false;
        *selected = true;
        return true;
    }
    if decision == MaterialCounterDecision::ClearAutomaticSelection {
        *pending = false;
    }
    false
}

pub(super) fn material_selection_ready_to_commit(
    selected: u8,
    required: u8,
    same_copy_pending: bool,
) -> bool {
    selected >= required && !same_copy_pending
}

pub(super) fn material_batch_click_limit(
    stage: StrategyStage,
    selected: u8,
    required: u8,
) -> usize {
    if stage == StrategyStage::PacketSelected {
        1
    } else {
        usize::from(required.saturating_sub(selected))
    }
}

pub(super) fn same_copy_counter_update_pending(pending: bool, recorded: u8, displayed: u8) -> bool {
    pending && displayed.saturating_add(1) == recorded
}

pub(super) fn recommendation_needs_execution(
    executed_for_target: bool,
    configured_profile: Option<RecommendMaterialProfile>,
    desired_profile: RecommendMaterialProfile,
) -> bool {
    !executed_for_target || configured_profile != Some(desired_profile)
}

pub(super) fn material_accepts_level_max(stage: StrategyStage) -> bool {
    stage == StrategyStage::BombSelectedForFeed
}

pub(super) fn material_level_max_text_detected(text: &str) -> bool {
    text.contains("等级达到") && text.contains("最大值")
}

pub(super) fn expected_post_enhancement_cap(stage: StrategyStage) -> Option<u32> {
    match stage {
        StrategyStage::BombBaseSelected => Some(50),
        StrategyStage::PacketSelected | StrategyStage::PacketAutoFeedPending => Some(20),
        StrategyStage::FastAutoFeedPending => Some(50),
        StrategyStage::BombSelectedForFeed => Some(50),
        _ => None,
    }
}

pub(super) fn post_enhancement_cap_matches(stage: StrategyStage, actual: u32) -> bool {
    if stage == StrategyStage::PacketAutoFeedPending {
        return matches!(actual, 20 | 30 | 40 | 50);
    }
    expected_post_enhancement_cap(stage) == Some(actual)
}

pub(super) fn enhancement_confirm_can_arm(stage: StrategyStage, materials_committed: bool) -> bool {
    materials_committed && expected_post_enhancement_cap(stage).is_some()
}

pub(super) fn next_packet_stage_after_enhancement(
    completed_stage: StrategyStage,
    completed_packet_count: usize,
) -> Option<StrategyStage> {
    match completed_stage {
        StrategyStage::PacketSelected => Some(StrategyStage::PacketAutoFeedPending),
        StrategyStage::PacketAutoFeedPending => {
            if completed_packet_count >= usize::from(PACKET_BATCH_SIZE) {
                Some(StrategyStage::SelectBombForTransfer)
            } else {
                Some(StrategyStage::SelectPacketBase)
            }
        }
        _ => None,
    }
}

pub(super) fn complete_pending_enhancement(
    pending: &mut Option<StrategyStage>,
    target: &ReadCraftEssenceMainTargetResult,
) -> Result<StrategyStage, EnhancementCompletionError> {
    let stage = pending.ok_or(EnhancementCompletionError::MissingPending)?;
    if stage == StrategyStage::FastAutoFeedPending && (!target.found || target.level.is_none()) {
        return Err(EnhancementCompletionError::TargetUnreadable);
    }
    let Some(expected) = expected_post_enhancement_cap(stage) else {
        return Err(EnhancementCompletionError::IllegalStage(stage));
    };
    match target.level_cap {
        Some(actual) if post_enhancement_cap_matches(stage, actual) => {}
        Some(actual) => {
            return Err(EnhancementCompletionError::CapMismatch {
                expected,
                actual: Some(actual),
            });
        }
        None => return Err(EnhancementCompletionError::TargetUnreadable),
    }
    pending
        .take()
        .ok_or(EnhancementCompletionError::MissingPending)
}

pub(super) fn unawaited_success_action(pending: Option<StrategyStage>) -> UnawaitedSuccessAction {
    if pending.is_some() {
        UnawaitedSuccessAction::ResumePending
    } else {
        UnawaitedSuccessAction::IgnoreResidual
    }
}

pub(super) fn is_incomplete_locked_bomb(cell: &CraftEssenceGridCell) -> bool {
    cell.valid
        && cell.locked
        && cell.rarity == Some(1)
        && cell.level_cap == Some(50)
        && cell.limit_breaks == Some(4)
        && cell.level.is_some_and(|level| level < 50)
}

pub(super) fn is_complete_locked_bomb(cell: &CraftEssenceGridCell) -> bool {
    cell.valid
        && cell.locked
        && cell.rarity == Some(1)
        && cell.level == Some(50)
        && cell.level_cap == Some(50)
        && cell.limit_breaks == Some(4)
}

pub(super) fn is_raw_food(cell: &CraftEssenceGridCell) -> bool {
    cell.valid
        && !cell.locked
        && cell.level == Some(1)
        && matches!(
            (cell.rarity, cell.level_cap, cell.limit_breaks),
            (Some(1), Some(10), Some(0)) | (Some(2), Some(15), Some(0))
        )
}

pub(super) fn is_packet(cell: &CraftEssenceGridCell) -> bool {
    cell.valid
        && !cell.locked
        && cell.rarity == Some(1)
        && matches!(
            (cell.level_cap, cell.limit_breaks),
            (Some(20), Some(1)) | (Some(30), Some(2)) | (Some(40), Some(3)) | (Some(50), Some(4))
        )
}

pub(super) fn same_art_fingerprint(left: &str, right: &str) -> bool {
    match (
        u64::from_str_radix(left, 16),
        u64::from_str_radix(right, 16),
    ) {
        (Ok(left), Ok(right)) => (left ^ right).count_ones() <= 4,
        _ => left == right,
    }
}

pub(super) fn scrollbar_reset_drag_y(
    thumb_y: Option<f64>,
    thumb_top_y: Option<f64>,
    visible_cell_count: u32,
    grid_cell_count: u32,
    attempts: u8,
) -> Result<Option<f64>, &'static str> {
    let Some(thumb_y) = thumb_y.filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
    else {
        if visible_cell_count > 0 && visible_cell_count < grid_cell_count {
            return Ok(None);
        }
        return Err("未识别到礼装列表滚动条位置");
    };
    let at_top = match thumb_top_y {
        Some(top_y) if top_y.is_finite() && (0.0..=1.0).contains(&top_y) => {
            top_y <= LIST_SCROLLBAR_TOP_MAX_TOP_Y
        }
        Some(_) => return Err("礼装列表滚动条上沿位置无效"),
        None => thumb_y <= LIST_SCROLLBAR_LEGACY_TOP_MAX_CENTER_Y,
    };
    if at_top {
        return Ok(None);
    }
    if attempts >= LIST_RESET_MAX_ATTEMPTS {
        return Err("无法确认礼装列表已回到顶部");
    }
    Ok(Some(thumb_y))
}

pub(super) fn next_page_scan_count(current: u8, maximum: u8, at_bottom: bool) -> u8 {
    if at_bottom {
        maximum.saturating_add(1)
    } else {
        current.saturating_add(1)
    }
}

pub(super) fn filter_scrollbar_at_top(y: f64) -> bool {
    y.is_finite() && (0.0..=FILTER_SCROLLBAR_TOP_MAX_Y).contains(&y)
}

pub(super) fn filter_scrollbar_reset_needs_drag(
    y: f64,
    attempts: u8,
) -> Result<bool, &'static str> {
    if filter_scrollbar_at_top(y) {
        return Ok(false);
    }
    if attempts >= FILTER_SCROLLBAR_RESET_MAX_ATTEMPTS {
        return Err("两次拖动后仍无法确认礼装筛选列表已回到顶部");
    }
    Ok(true)
}

pub(super) fn choose_incomplete_bomb(
    cells: &[CraftEssenceGridCell],
) -> Option<&CraftEssenceGridCell> {
    cells.iter().find(|cell| is_incomplete_locked_bomb(cell))
}

pub(super) fn choose_packet_base(cells: &[CraftEssenceGridCell]) -> Option<&CraftEssenceGridCell> {
    cells.iter().find(|cell| {
        is_raw_food(cell)
            && cell.rarity == Some(1)
            && cells
                .iter()
                .filter(|candidate| {
                    is_raw_food(candidate)
                        && candidate.rarity == Some(1)
                        && same_art_fingerprint(&candidate.art_fingerprint, &cell.art_fingerprint)
                })
                .count()
                >= 2
    })
}

pub(super) fn choose_bomb_base(cells: &[CraftEssenceGridCell]) -> Option<&CraftEssenceGridCell> {
    cells.iter().find(|cell| {
        is_raw_food(cell)
            && cell.rarity == Some(1)
            && cells
                .iter()
                .filter(|candidate| {
                    is_raw_food(candidate)
                        && candidate.rarity == Some(1)
                        && same_art_fingerprint(&candidate.art_fingerprint, &cell.art_fingerprint)
                })
                .count()
                >= 5
    })
}

pub(super) fn is_verified_locked_bomb_base(
    cell: &CraftEssenceGridCell,
    expected_row: Option<u32>,
    expected_col: Option<u32>,
) -> bool {
    Some(cell.row) == expected_row
        && Some(cell.col) == expected_col
        && cell.valid
        && cell.locked
        && cell.rarity == Some(1)
        && cell.level_cap == Some(50)
        && cell.limit_breaks == Some(4)
}

pub(super) fn grid_signature(cells: &[CraftEssenceGridCell]) -> String {
    cells
        .iter()
        .map(|cell| {
            format!(
                "{}:{}:{}:{}",
                cell.art_fingerprint,
                cell.level.unwrap_or(0),
                cell.level_cap.unwrap_or(0),
                u8::from(cell.locked)
            )
        })
        .collect::<Vec<_>>()
        .join("|")
}

pub(super) fn plan_packet_materials<'a>(
    cells: &[&'a CraftEssenceGridCell],
    same_copy_already_selected: bool,
) -> Vec<&'a CraftEssenceGridCell> {
    if same_copy_already_selected {
        return Vec::new();
    }
    cells
        .iter()
        .copied()
        .find(|cell| is_raw_food(cell) && cell.same_as_target)
        .into_iter()
        .collect()
}

pub(super) fn plan_packet_feed<'a>(
    cells: &[&'a CraftEssenceGridCell],
    remaining_fingerprints: &[String],
) -> Vec<&'a CraftEssenceGridCell> {
    let mut remaining = remaining_fingerprints.to_vec();
    let mut planned = Vec::new();
    for &cell in cells {
        if !is_packet(cell) {
            continue;
        }
        let Some(index) = remaining
            .iter()
            .position(|fingerprint| same_art_fingerprint(fingerprint, &cell.art_fingerprint))
        else {
            continue;
        };
        planned.push(cell);
        remaining.remove(index);
        if remaining.is_empty() {
            break;
        }
    }
    planned
}

pub(super) fn plan_inventory_packet_feed<'a>(
    cells: &[&'a CraftEssenceGridCell],
) -> Vec<&'a CraftEssenceGridCell> {
    cells
        .iter()
        .copied()
        .filter(|cell| is_packet(cell))
        .collect()
}

pub(super) fn inventory_feed_ready_at_bottom(
    feed_inventory_packets: bool,
    selected: u8,
    at_bottom: bool,
) -> bool {
    feed_inventory_packets && selected > 0 && at_bottom
}

pub(super) fn packet_base_exhaustion_action(
    completed_packet_count: usize,
) -> PacketBaseExhaustionAction {
    if completed_packet_count == 0 {
        PacketBaseExhaustionAction::ReturnToCurrentBomb
    } else {
        PacketBaseExhaustionAction::ReselectBomb
    }
}

pub(super) fn target_selection_descending(stage: StrategyStage) -> bool {
    !matches!(
        stage,
        StrategyStage::SelectBombBase | StrategyStage::SelectPacketBase
    )
}

pub(super) fn material_selection_descending(_stage: StrategyStage) -> bool {
    false
}

#[derive(Debug, Default)]
pub(crate) struct ProbeSnapshot {
    pub(super) found: Vec<&'static str>,
}

impl ProbeSnapshot {
    #[cfg(test)]
    pub(super) fn from_keys(keys: &[&'static str]) -> Self {
        Self {
            found: keys.to_vec(),
        }
    }

    pub(super) fn has(&self, key: &str) -> bool {
        self.found.iter().any(|found| *found == key)
    }
}

pub(crate) fn classify_screen(snapshot: &ProbeSnapshot) -> Screen {
    if snapshot.has("dialog_enhancement_ce_enhanced_material_warning") {
        return Screen::EnhancedMaterialWarningDialog;
    }
    if snapshot.has("text_exp_overflow") {
        return Screen::ExpOverflowDialog;
    }
    if snapshot.has("dialog_enhancement_ce_confirm")
        || snapshot.has("dialog_enhancement_ce_confirm_compact")
    {
        return Screen::EnhancementConfirmDialog;
    }
    if snapshot.has("element_enhancement_ce_success") {
        return Screen::EnhancementSuccess;
    }
    if snapshot.has("dialog_enhancement_ce_recommend_empty") {
        return Screen::RecommendMaterialEmptyDialog;
    }
    if snapshot.has("dialog_enhancement_ce_recommend_material") {
        return Screen::RecommendMaterialDialog;
    }
    let select_mark = snapshot.has("button_enhancement_ce_select_ce_mark");
    if snapshot.has("button_enhancement_ce_lock_mode_active") {
        return Screen::CraftEssenceLockMode;
    }
    if select_mark
        && snapshot.has("dialog_enhancement_ce_filter")
        && snapshot.has("button_enhancement_ce_filter_init")
    {
        return Screen::FilterDialog;
    }
    if select_mark && snapshot.has("dialog_enhancement_ce_order") {
        return Screen::OrderDialog;
    }
    if select_mark {
        if snapshot.has("button_enhancement_ce_clean_all_select")
            || snapshot.has("button_enhancement_ce_clean_all_select_ready")
        {
            return Screen::MaterialSelect;
        }
        return Screen::CraftEssenceSelect {
            descending: snapshot.has("button_enhancement_ce_select_ce_desc"),
        };
    }
    if snapshot.has("icon_enhancement_result") && snapshot.has("element_enhancement_ce_stripe") {
        return Screen::Main {
            target_selected: !snapshot.has("element_enhancement_new"),
            ready: snapshot.has("button_enhancement_ready"),
        };
    }
    Screen::Unknown
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EnhancementReadyState {
    Absent,
    NotReady,
    Ready,
    Transitioning,
}

pub(super) fn classify_enhancement_button(score: f64, mean_luma: f64) -> EnhancementReadyState {
    if score < ENHANCE_BUTTON_PRESENT_MIN_SCORE {
        EnhancementReadyState::Absent
    } else if mean_luma <= ENHANCE_BUTTON_NOT_READY_MAX_LUMA {
        EnhancementReadyState::NotReady
    } else if mean_luma >= ENHANCE_BUTTON_READY_MIN_LUMA {
        EnhancementReadyState::Ready
    } else {
        EnhancementReadyState::Transitioning
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EnhancementReturnAction {
    ObserveReturnedMain,
    WaitForConfirmationClose,
    CloseExpOverflow,
    TapSkip { mark_left_main: bool },
    Unexpected,
}

pub(super) fn enhancement_return_action(
    screen: Screen,
    left_main: bool,
) -> EnhancementReturnAction {
    match screen {
        Screen::Main { .. } if left_main => EnhancementReturnAction::ObserveReturnedMain,
        Screen::EnhancementConfirmDialog => EnhancementReturnAction::WaitForConfirmationClose,
        Screen::ExpOverflowDialog => EnhancementReturnAction::CloseExpOverflow,
        Screen::Unknown | Screen::EnhancementSuccess => EnhancementReturnAction::TapSkip {
            mark_left_main: true,
        },
        Screen::Main { .. } => EnhancementReturnAction::TapSkip {
            mark_left_main: false,
        },
        _ => EnhancementReturnAction::Unexpected,
    }
}

pub(super) fn enhancement_main_return_confirmed(observations: u8) -> bool {
    observations >= ENHANCEMENT_MAIN_RETURN_CONFIRMATIONS
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BinaryControlDecision {
    TargetConfirmed,
    Toggle,
    Ambiguous,
}

pub(super) fn decide_binary_control(
    target_score: f64,
    opposite_score: f64,
) -> BinaryControlDecision {
    if target_score >= BINARY_CONTROL_MIN_SCORE
        && target_score >= opposite_score + BINARY_CONTROL_SCORE_MARGIN
    {
        BinaryControlDecision::TargetConfirmed
    } else if opposite_score >= BINARY_CONTROL_MIN_SCORE
        && opposite_score >= target_score + BINARY_CONTROL_SCORE_MARGIN
    {
        BinaryControlDecision::Toggle
    } else {
        BinaryControlDecision::Ambiguous
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FilterToggleState {
    Off,
    On,
    Ambiguous,
}

pub(super) fn classify_filter_toggle_luma(mean_luma: f64) -> FilterToggleState {
    if mean_luma <= FILTER_TOGGLE_OFF_MAX_LUMA {
        FilterToggleState::Off
    } else if mean_luma >= FILTER_TOGGLE_ON_MIN_LUMA {
        FilterToggleState::On
    } else {
        FilterToggleState::Ambiguous
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AutoConfigState {
    Off,
    On,
    Ambiguous,
}

pub(super) fn classify_auto_config_saturation(mean_saturation: f64) -> AutoConfigState {
    if mean_saturation <= AUTO_CONFIG_OFF_MAX_SATURATION {
        AutoConfigState::Off
    } else if mean_saturation >= AUTO_CONFIG_ON_MIN_SATURATION {
        AutoConfigState::On
    } else {
        AutoConfigState::Ambiguous
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DensityDecision {
    Confirmed,
    Toggle,
    Failed,
}

pub(super) fn density_decision(level_three_found: bool, taps_done: u8) -> DensityDecision {
    if level_three_found {
        DensityDecision::Confirmed
    } else if taps_done < 3 {
        DensityDecision::Toggle
    } else {
        DensityDecision::Failed
    }
}
