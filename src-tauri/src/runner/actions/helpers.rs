use super::*;

pub(crate) fn format_skill_tap_debug(label: &str, point: Point, width: u32, height: u32) -> String {
    let (px, py) = point.to_physical(width, height);
    format!("{label}: x={:.3}, y={:.3} ({px}, {py})", point.x, point.y)
}

pub(crate) fn skill_position(servant: Option<&str>, skill: Option<&str>) -> Option<Point> {
    let si = parse_index(servant?, "servant_")?;
    let ki = parse_index(skill?, "skill_")?;
    SERVANT_SKILLS.get(si).and_then(|row| row.get(ki)).copied()
}

pub(crate) fn equipment_skill_position(skill: Option<&str>) -> Option<Point> {
    let ki = parse_index(skill?, "skill_")?;
    EQUIPMENT_SKILLS.get(ki).copied()
}

/// Map a Command Spell name to its index in `COMMAND_SPELL_OPTIONS`.
/// Returns `None` for unknown / missing values so the runner can skip
/// the action gracefully (mirrors how `skill_position` returns `None`
/// for malformed servant/skill strings).
pub(crate) fn command_spell_index(spell: Option<&str>) -> Option<usize> {
    match spell? {
        "np_release" => Some(0),
        "restore" => Some(1),
        _ => None,
    }
}

pub(crate) fn turn_preparation_actions(turn: &BattleTurn) -> std::slice::Iter<'_, Action> {
    turn.preparation_actions.iter()
}

/// Skill targets are always allies (servant_1, servant_2, servant_3).
pub(crate) fn skill_target_position(target: Option<&str>) -> Option<Point> {
    let t = target?;
    let si = parse_index(t, "servant_")?;
    SKILL_TARGETS.get(si).copied()
}

pub(crate) fn skill_selection_option_position(selection: &crate::SkillSelection) -> Option<Point> {
    let option_count = selection.option_count?;
    let index = selection.index as usize;
    match selection.selection_type.as_str() {
        "SelectAddInfo" => match option_count {
            2 => SELECT_ADD_INFO_OPTIONS_2.get(index).copied(),
            3 => SELECT_ADD_INFO_OPTIONS_3.get(index).copied(),
            _ => None,
        },
        "selectTreasureDeviceInfo" | "commandTypeSelfTreasureDevice" => match option_count {
            2 => NP_SELECTION_OPTIONS_2.get(index).copied(),
            3 => NP_SELECTION_OPTIONS_3.get(index).copied(),
            _ => None,
        },
        _ => None,
    }
}

pub(crate) fn classify_skill_use_dialog_luma(
    mean_luma: f64,
    threshold: f64,
) -> SkillUseDialogState {
    if mean_luma < threshold {
        SkillUseDialogState::AlreadyUsed
    } else {
        SkillUseDialogState::Confirm
    }
}

pub(crate) fn classify_skill_use_dialog_probe(
    probe: &SkillUseDialogProbe,
) -> Option<SkillUseDialogState> {
    if !probe.found {
        return None;
    }
    Some(classify_skill_use_dialog_luma(
        probe.mean_luma,
        SKILL_USE_CONFIRM_LUMA_THRESHOLD,
    ))
}

pub(crate) fn display_skill_use_probe_image_path(path: Option<&PathBuf>) -> String {
    path.map(|path| path.display().to_string())
        .unwrap_or_else(|| "<live-frame>".to_string())
}

pub(crate) fn selection_dialog_kind(selection_type: &str) -> Option<SelectionDialogKind> {
    match selection_type {
        "SelectAddInfo" => Some(SelectionDialogKind::AddInfo),
        "selectTreasureDeviceInfo" => Some(SelectionDialogKind::TreasureDevice),
        "commandTypeSelfTreasureDevice" => Some(SelectionDialogKind::SelfTreasureDevice),
        _ => None,
    }
}

pub(crate) fn skill_selection_close_region(kind: SelectionDialogKind) -> NormRect {
    match kind {
        SelectionDialogKind::AddInfo => SELECT_ADD_INFO_CLOSE,
        SelectionDialogKind::TreasureDevice => SELECT_TREASURE_DEVICE_CLOSE,
        SelectionDialogKind::SelfTreasureDevice => COMMAND_TYPE_SELF_TREASURE_DEVICE_CLOSE,
    }
}

pub(crate) fn skill_post_tap_expectation(
    selection: Option<&crate::SkillSelection>,
    target_pos: Option<Point>,
    order_change: bool,
) -> SkillPostTapExpectation {
    if order_change {
        SkillPostTapExpectation::OrderChange
    } else if let Some(selection) =
        selection.and_then(|selection| selection_dialog_kind(selection.selection_type.as_str()))
    {
        SkillPostTapExpectation::SelectionDialog(selection)
    } else if target_pos.is_some() {
        SkillPostTapExpectation::TargetPicker
    } else {
        SkillPostTapExpectation::ActivationStart
    }
}

pub(crate) fn order_change_slot_position(
    target: Option<&str>,
    allowed: std::ops::Range<usize>,
) -> Option<Point> {
    let t = target?;
    let si = parse_index(t, "servant_")?;
    if !allowed.contains(&si) {
        return None;
    }
    ORDER_CHANGE_SLOTS.get(si).copied()
}

pub(crate) fn order_change_selection_position(
    target: Option<&str>,
    allowed: std::ops::Range<usize>,
) -> Option<Point> {
    let t = target?;
    let si = parse_index(t, "servant_")?;
    if !allowed.contains(&si) {
        return None;
    }
    ORDER_CHANGE_SELECTION_POINTS.get(si).copied()
}

/// Enemy targets (enemy_1..enemy_6).
pub(crate) fn enemy_target_position(target: Option<&str>) -> Option<Point> {
    let t = target?;
    let ei = parse_index(t, "enemy_")?;
    ENEMY_TARGETS.get(ei).copied()
}

pub(crate) fn servant_skill_failure_label(
    servant: Option<&str>,
    servant_id: Option<u32>,
    skill_index: u32,
) -> String {
    match (servant, servant_id) {
        (Some(slot), Some(id)) => format!("从者 {slot} (#{id}) 技能 {}", skill_index + 1),
        (Some(slot), None) => format!("从者 {slot} 技能 {}", skill_index + 1),
        (None, Some(id)) => format!("从者 #{id} 技能 {}", skill_index + 1),
        (None, None) => format!("从者技能 {}", skill_index + 1),
    }
}

pub(crate) fn equipment_skill_failure_label(skill_index: u32, order_change: bool) -> String {
    if order_change {
        format!("御主技能 {} / Order Change", skill_index + 1)
    } else {
        format!("御主技能 {}", skill_index + 1)
    }
}

pub(crate) fn command_spell_failure_label(spell: &str) -> String {
    match spell {
        "np_release" => "令咒 开放宝具".into(),
        "restore" => "令咒 回复".into(),
        _ => format!("令咒 {spell}"),
    }
}
