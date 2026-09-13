use super::*;

pub(crate) fn skipped_action_log_meta(action: &Action) -> ActionLogMeta {
    let servant_id = match action {
        Action::Servant {
            servant_id,
            target_servant_id,
            ..
        } => (*servant_id).or(*target_servant_id),
        Action::Equipment {
            target_servant_id, ..
        }
        | Action::CommandSpell {
            target_servant_id, ..
        } => *target_servant_id,
        Action::EnemyTarget { .. } => None,
    };
    ActionLogMeta::SkippedAction { servant_id }
}

pub(crate) fn advanced_startup_flow_actions(
    scene: &AdvancedBattleScene,
    control_count: usize,
    startup_control_count: usize,
    completed_turn_count: usize,
    auto_order_change: Option<&Action>,
) -> Vec<Action> {
    let control_count = control_count.min(scene.control_actions.len());
    let startup_control_count = startup_control_count.min(control_count);
    let mut actions = Vec::new();
    if let Some(action) = auto_order_change {
        actions.push(action.clone());
    }
    actions.extend(
        scene
            .control_actions
            .iter()
            .take(startup_control_count)
            .cloned(),
    );
    let mut remaining_control_actions = scene
        .control_actions
        .iter()
        .skip(startup_control_count)
        .take(control_count.saturating_sub(startup_control_count));
    for turn_index in 0..completed_turn_count {
        let Some(turn_actions) = advanced_turn_actions(scene, turn_index) else {
            break;
        };
        actions.extend(turn_actions.iter().cloned());
        if let Some(control_action) = remaining_control_actions.next() {
            actions.push(control_action.clone());
        }
    }
    actions.extend(remaining_control_actions.cloned());
    actions
}

pub(crate) fn advanced_turn_actions(
    scene: &AdvancedBattleScene,
    turn_index: usize,
) -> Option<&[Action]> {
    if scene.turns.is_empty() {
        return (turn_index == 0).then_some(scene.startup_actions.as_slice());
    }
    scene
        .turns
        .get(turn_index)
        .map(|turn| turn.actions.as_slice())
}

#[cfg(test)]
pub(crate) fn normal_current_party_ids_from(
    ids: [Option<u32>; 6],
    scenes: &[BattleScene],
    current_scene_index: usize,
    current_turn_index: usize,
    executed_turn_key: Option<(usize, usize)>,
) -> [Option<u32>; 3] {
    let mut members: [Option<PartyMemberRuntime>; 6] = [None, None, None, None, None, None];
    for (slot_index, servant_id) in ids.into_iter().enumerate() {
        members[slot_index] = servant_id.map(|servant_id| PartyMemberRuntime {
            member_id: None,
            slot_index,
            servant_id,
            is_support: false,
        });
    }
    let full = normal_current_party_members_from(
        members,
        scenes,
        current_scene_index,
        current_turn_index,
        executed_turn_key,
    );
    let ids = party_member_ids(&full);
    [ids[0], ids[1], ids[2]]
}

pub(crate) fn normal_current_party_members_from(
    mut members: [Option<PartyMemberRuntime>; 6],
    scenes: &[BattleScene],
    current_scene_index: usize,
    current_turn_index: usize,
    executed_turn_key: Option<(usize, usize)>,
) -> [Option<PartyMemberRuntime>; 6] {
    let original_members = members.clone();
    let mut np_use_counts: HashMap<PartyMemberRuntime, u32> = HashMap::new();
    for (scene_index, scene) in scenes.iter().enumerate() {
        if scene_index > current_scene_index {
            break;
        }
        for (turn_index, turn) in scene.turns.iter().enumerate() {
            let before_current_scene = scene_index < current_scene_index;
            let before_current_turn =
                scene_index == current_scene_index && turn_index < current_turn_index;
            let is_current_turn =
                scene_index == current_scene_index && turn_index == current_turn_index;
            let prep_has_executed = before_current_scene
                || before_current_turn
                || executed_turn_key == Some((scene_index, turn_index));
            if !prep_has_executed && !is_current_turn {
                continue;
            }

            if prep_has_executed {
                for action in turn_preparation_actions(turn) {
                    if let Some(resolved) =
                        resolve_available_member_action(&members, &original_members, action)
                    {
                        apply_party_member_lineup_change(&mut members, &resolved);
                    }
                }
            }

            if before_current_scene || before_current_turn {
                for card in &turn.attack_priority {
                    apply_attack_card_member_lineup_change(&mut members, card, &mut np_use_counts);
                }
                for action in turn_preparation_actions(turn) {
                    if let Some(resolved) =
                        resolve_available_member_action(&members, &original_members, action)
                    {
                        apply_party_member_lineup_change_at(
                            &mut members,
                            &resolved,
                            ChangeOrderTiming::EndOfTurn,
                        );
                    }
                }
            }
        }
        if scene_index == current_scene_index {
            break;
        }
    }
    members
}
