use super::*;

pub(crate) fn advanced_startup_conditions_match(
    scene: &AdvancedBattleScene,
    cards: &[CommandCardMatch],
    party_ids: &[Option<u32>; 3],
    party_supports: &[bool; 3],
) -> bool {
    let active: Vec<&AdvancedCommandCardCondition> = scene
        .command_conditions
        .iter()
        .filter(|condition| condition.servant != "any" || condition.suit != "any")
        .collect();
    if active.is_empty() {
        return true;
    }
    let mut used_slots = HashSet::new();
    startup_conditions_match_from(
        0,
        &active,
        cards,
        party_ids,
        party_supports,
        &mut used_slots,
    )
}

pub(crate) fn has_advanced_startup_conditions(scene: &AdvancedBattleScene) -> bool {
    scene
        .command_conditions
        .iter()
        .any(|condition| condition.servant != "any" || condition.suit != "any")
}

pub(crate) fn grand_startup_can_run_before_attack(
    scene: &AdvancedBattleScene,
    grand_servants: &[GrandServantRuntimeConfig],
) -> bool {
    if grand_servants.is_empty()
        || !(scene.rules.is_empty() || uses_advanced_strategy_flow(scene))
        || has_advanced_startup_conditions(scene)
    {
        return false;
    }
    let backline_main_needs_swap = scene.grand_auto_order_change == Some(true)
        && grand_servants
            .first()
            .is_some_and(|main| main.slot_index >= 3);
    !backline_main_needs_swap
}

pub(crate) fn startup_conditions_match_from(
    condition_index: usize,
    conditions: &[&AdvancedCommandCardCondition],
    cards: &[CommandCardMatch],
    party_ids: &[Option<u32>; 3],
    party_supports: &[bool; 3],
    used_slots: &mut HashSet<u32>,
) -> bool {
    if condition_index >= conditions.len() {
        return true;
    }

    let condition = conditions[condition_index];
    for card in cards {
        if used_slots.contains(&card.slot) {
            continue;
        }
        if !command_condition_matches_card(condition, card, party_ids, party_supports) {
            continue;
        }
        used_slots.insert(card.slot);
        if startup_conditions_match_from(
            condition_index + 1,
            conditions,
            cards,
            party_ids,
            party_supports,
            used_slots,
        ) {
            return true;
        }
        used_slots.remove(&card.slot);
    }
    false
}

pub(crate) fn uses_advanced_strategy_flow(scene: &AdvancedBattleScene) -> bool {
    scene.main_output.is_some()
        || scene.grand_auto_order_change == Some(true)
        || !scene.command_conditions.is_empty()
        || !scene.control_actions.is_empty()
        || !scene.turns.is_empty()
        || !scene.startup_actions.is_empty()
}

pub(crate) fn normal_turn_for_current_state(
    scenes: &[BattleScene],
    current_scene_index: usize,
    current_turn_index: usize,
) -> Option<(BattleTurn, bool)> {
    let turns = &scenes.get(current_scene_index)?.turns;
    let last_index = turns.len().checked_sub(1)?;
    let over_configured_turns = current_turn_index > last_index;
    let effective_index = current_turn_index.min(last_index);
    turns
        .get(effective_index)
        .cloned()
        .map(|turn| (turn, over_configured_turns))
}

pub(crate) fn advanced_enemy_target_for_current_scene(
    scenes: &[AdvancedBattleScene],
    current_scene_index: usize,
    should_select: bool,
) -> Option<&str> {
    if !should_select {
        return None;
    }
    scenes
        .get(current_scene_index)
        .and_then(|scene| scene.enemy_target.as_deref())
}

pub(crate) fn attack_priority_for_current_scene(
    advanced_mode: bool,
    scene_config_used: bool,
    scenes: &[BattleScene],
    current_scene_index: usize,
    current_turn_index: usize,
) -> Option<&[AttackCard]> {
    if advanced_mode && !scene_config_used {
        return None;
    }
    let turns = &scenes.get(current_scene_index)?.turns;
    let last_index = turns.len().checked_sub(1)?;
    let effective_index = current_turn_index.min(last_index);
    scenes
        .get(current_scene_index)
        .and_then(|scene| scene.turns.get(effective_index))
        .map(|turn| turn.attack_priority.as_slice())
}

pub(crate) fn attack_card_requires_command_card_recognition(card: &AttackCard) -> bool {
    card.card
        .as_deref()
        .and_then(parse_priority_card)
        .map(|(_, kind)| kind != "np")
        .unwrap_or(false)
}

pub(crate) fn normal_scenes_need_command_card_recognition(scenes: &[BattleScene]) -> bool {
    scenes.iter().any(|scene| {
        scene.turns.iter().any(|turn| {
            turn.attack_mode != AttackMode::Normal
                || turn
                    .attack_priority
                    .iter()
                    .any(attack_card_requires_command_card_recognition)
        })
    })
}

pub(crate) fn normal_turn_for_current_scene(
    scenes: &[BattleScene],
    current_scene_index: usize,
    current_turn_index: usize,
) -> Option<&BattleTurn> {
    let turns = &scenes.get(current_scene_index)?.turns;
    let last_index = turns.len().checked_sub(1)?;
    turns.get(current_turn_index.min(last_index))
}
