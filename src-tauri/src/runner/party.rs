//! Party lineup mutation, Order Change, and frontline availability helpers.

use super::*;

// ---------------------------------------------------------------------------
// Position mapping helpers
// ---------------------------------------------------------------------------

pub(crate) fn parse_index(s: &str, prefix: &str) -> Option<usize> {
    s.strip_prefix(prefix)
        .and_then(|n| n.parse::<usize>().ok())
        .map(|n| n.saturating_sub(1))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct PartyMemberRuntime {
    pub(crate) slot_index: usize,
    pub(crate) servant_id: u32,
    pub(crate) is_support: bool,
}

pub(crate) fn party_member_ids(members: &[Option<PartyMemberRuntime>; 6]) -> [Option<u32>; 6] {
    [
        members[0].map(|member| member.servant_id),
        members[1].map(|member| member.servant_id),
        members[2].map(|member| member.servant_id),
        members[3].map(|member| member.servant_id),
        members[4].map(|member| member.servant_id),
        members[5].map(|member| member.servant_id),
    ]
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChangeOrderRule {
    servant_id: u32,
    trigger: ChangeOrderTrigger,
    effect: ChangeOrderEffect,
    #[serde(default)]
    timing: ChangeOrderTiming,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ChangeOrderTiming {
    #[default]
    Immediate,
    EndOfTurn,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(crate) enum ChangeOrderTrigger {
    AttackCard {
        #[serde(rename = "card")]
        card: String,
        #[serde(default)]
        #[serde(rename = "activationUseCount")]
        activation_use_count: Option<u32>,
    },
    ServantSkill {
        skill: String,
    },
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(crate) enum ChangeOrderEffect {
    RemoveSelf,
    RemoveFirstAlly,
    WithdrawSelfToBack,
}

pub(crate) fn change_order_rules() -> &'static [ChangeOrderRule] {
    static RULES: OnceLock<Vec<ChangeOrderRule>> = OnceLock::new();
    RULES
        .get_or_init(|| {
            serde_json::from_str(include_str!("../resources/change_order_servants.json"))
                .unwrap_or_default()
        })
        .as_slice()
}

#[cfg(test)]
pub(crate) fn compact_backline(ids: &mut [Option<u32>; 6]) {
    let backline: Vec<Option<u32>> = ids[3..]
        .iter()
        .copied()
        .filter(|slot| slot.is_some())
        .collect();
    for slot in 3..6 {
        ids[slot] = backline.get(slot - 3).copied().flatten();
    }
}

#[cfg(test)]
pub(crate) fn remove_party_slot(ids: &mut [Option<u32>; 6], index: usize) {
    if index >= 3 || ids[index].is_none() {
        return;
    }
    let replacement = (3..6).find(|slot| ids[*slot].is_some());
    ids[index] = replacement.and_then(|slot| ids[slot]);
    if let Some(slot) = replacement {
        ids[slot] = None;
    }
    compact_backline(ids);
}

#[cfg(test)]
pub(crate) fn withdraw_party_slot_to_back(ids: &mut [Option<u32>; 6], index: usize) {
    if index >= 3 {
        return;
    }
    let Some(servant_id) = ids[index] else {
        return;
    };
    compact_backline(ids);
    let Some(replacement) = (3..6).find(|slot| ids[*slot].is_some()) else {
        return;
    };
    ids[index] = ids[replacement];
    ids[replacement] = Some(servant_id);
}

#[cfg(test)]
pub(crate) fn apply_change_order_effect(
    ids: &mut [Option<u32>; 6],
    source_index: usize,
    effect: &ChangeOrderEffect,
) {
    match effect {
        ChangeOrderEffect::RemoveSelf => remove_party_slot(ids, source_index),
        ChangeOrderEffect::RemoveFirstAlly => {
            if let Some(target) =
                (0..3).find(|index| *index != source_index && ids[*index].is_some())
            {
                remove_party_slot(ids, target);
            }
        }
        ChangeOrderEffect::WithdrawSelfToBack => withdraw_party_slot_to_back(ids, source_index),
    }
}

pub(crate) fn compact_member_backline(members: &mut [Option<PartyMemberRuntime>; 6]) {
    let backline: Vec<Option<PartyMemberRuntime>> = members[3..]
        .iter()
        .copied()
        .filter(|slot| slot.is_some())
        .collect();
    for slot in 3..6 {
        members[slot] = backline.get(slot - 3).copied().flatten();
    }
}

pub(crate) fn remove_party_member_slot(
    members: &mut [Option<PartyMemberRuntime>; 6],
    index: usize,
) {
    if index >= 3 || members[index].is_none() {
        return;
    }
    let replacement = (3..6).find(|slot| members[*slot].is_some());
    members[index] = replacement.and_then(|slot| members[slot]);
    if let Some(slot) = replacement {
        members[slot] = None;
    }
    compact_member_backline(members);
}

pub(crate) fn withdraw_party_member_slot_to_back(
    members: &mut [Option<PartyMemberRuntime>; 6],
    index: usize,
) {
    if index >= 3 {
        return;
    }
    let Some(member) = members[index] else {
        return;
    };
    compact_member_backline(members);
    let Some(replacement) = (3..6).find(|slot| members[*slot].is_some()) else {
        return;
    };
    members[index] = members[replacement];
    members[replacement] = Some(member);
}

pub(crate) fn apply_member_change_order_effect(
    members: &mut [Option<PartyMemberRuntime>; 6],
    source_index: usize,
    effect: &ChangeOrderEffect,
) {
    match effect {
        ChangeOrderEffect::RemoveSelf => remove_party_member_slot(members, source_index),
        ChangeOrderEffect::RemoveFirstAlly => {
            if let Some(target) =
                (0..3).find(|index| *index != source_index && members[*index].is_some())
            {
                remove_party_member_slot(members, target);
            }
        }
        ChangeOrderEffect::WithdrawSelfToBack => {
            withdraw_party_member_slot_to_back(members, source_index)
        }
    }
}

#[cfg(test)]
pub(crate) fn apply_party_lineup_change(ids: &mut [Option<u32>; 6], action: &Action) {
    apply_party_lineup_change_at(ids, action, ChangeOrderTiming::Immediate);
}

#[cfg(test)]
pub(crate) fn apply_party_lineup_change_at(
    ids: &mut [Option<u32>; 6],
    action: &Action,
    timing: ChangeOrderTiming,
) {
    if timing == ChangeOrderTiming::EndOfTurn && !matches!(action, Action::Servant { .. }) {
        return;
    }

    if let Action::Equipment {
        order_change: Some(order_change),
        ..
    } = action
    {
        let Some(front) = order_change
            .front
            .as_deref()
            .and_then(|value| parse_index(value, "servant_"))
        else {
            return;
        };
        let Some(back) = order_change
            .back
            .as_deref()
            .and_then(|value| parse_index(value, "servant_"))
        else {
            return;
        };
        if front < 3 && back < 6 && ids[front].is_some() && ids[back].is_some() {
            ids.swap(front, back);
        }
        return;
    }

    let Action::Servant { servant, skill, .. } = action else {
        return;
    };
    let Some(source_index) = servant
        .as_deref()
        .and_then(|value| parse_index(value, "servant_"))
        .filter(|index| *index < 3)
    else {
        return;
    };
    let (Some(servant_id), Some(skill)) = (ids[source_index], skill.as_deref()) else {
        return;
    };
    for rule in change_order_rules() {
        if rule.servant_id != servant_id {
            continue;
        }
        if rule.timing != timing {
            continue;
        }
        if let ChangeOrderTrigger::ServantSkill {
            skill: trigger_skill,
        } = &rule.trigger
        {
            if trigger_skill == skill {
                apply_change_order_effect(ids, source_index, &rule.effect);
                return;
            }
        }
    }
}

pub(crate) fn apply_party_member_lineup_change(
    members: &mut [Option<PartyMemberRuntime>; 6],
    action: &Action,
) {
    apply_party_member_lineup_change_at(members, action, ChangeOrderTiming::Immediate);
}

pub(crate) fn apply_party_member_lineup_change_at(
    members: &mut [Option<PartyMemberRuntime>; 6],
    action: &Action,
    timing: ChangeOrderTiming,
) {
    if timing == ChangeOrderTiming::EndOfTurn && !matches!(action, Action::Servant { .. }) {
        return;
    }

    if let Action::Equipment {
        order_change: Some(order_change),
        ..
    } = action
    {
        let Some(front) = order_change
            .front
            .as_deref()
            .and_then(|value| parse_index(value, "servant_"))
        else {
            return;
        };
        let Some(back) = order_change
            .back
            .as_deref()
            .and_then(|value| parse_index(value, "servant_"))
        else {
            return;
        };
        if front < 3 && back < 6 && members[front].is_some() && members[back].is_some() {
            members.swap(front, back);
        }
        return;
    }

    let Action::Servant { servant, skill, .. } = action else {
        return;
    };
    let Some(source_index) = servant
        .as_deref()
        .and_then(|value| parse_index(value, "servant_"))
        .filter(|index| *index < 3)
    else {
        return;
    };
    let (Some(member), Some(skill)) = (members[source_index], skill.as_deref()) else {
        return;
    };
    for rule in change_order_rules() {
        if rule.servant_id != member.servant_id {
            continue;
        }
        if rule.timing != timing {
            continue;
        }
        if let ChangeOrderTrigger::ServantSkill {
            skill: trigger_skill,
        } = &rule.trigger
        {
            if trigger_skill == skill {
                apply_member_change_order_effect(members, source_index, &rule.effect);
                return;
            }
        }
    }
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn apply_attack_card_lineup_change(
    ids: &mut [Option<u32>; 6],
    card: &AttackCard,
    np_use_counts: &mut HashMap<u32, u32>,
) {
    let Some(source_index) = card
        .card
        .as_deref()
        .and_then(|value| value.strip_suffix("_np"))
        .and_then(|value| parse_index(value, "servant_"))
        .filter(|index| *index < 3)
    else {
        return;
    };
    let Some(servant_id) = ids[source_index] else {
        return;
    };
    let next_count = np_use_counts.get(&servant_id).copied().unwrap_or(0) + 1;
    np_use_counts.insert(servant_id, next_count);

    for rule in change_order_rules() {
        if rule.servant_id != servant_id {
            continue;
        }
        if rule.timing != ChangeOrderTiming::Immediate {
            continue;
        }
        if let ChangeOrderTrigger::AttackCard {
            card: trigger_card,
            activation_use_count,
        } = &rule.trigger
        {
            let count_matches = activation_use_count
                .map(|count| count == next_count)
                .unwrap_or(true);
            if trigger_card == "np" && count_matches {
                apply_change_order_effect(ids, source_index, &rule.effect);
                return;
            }
        }
    }
}

pub(crate) fn apply_attack_card_member_lineup_change(
    members: &mut [Option<PartyMemberRuntime>; 6],
    card: &AttackCard,
    np_use_counts: &mut HashMap<PartyMemberRuntime, u32>,
) {
    let Some(source_index) = card
        .card
        .as_deref()
        .and_then(|value| value.strip_suffix("_np"))
        .and_then(|value| parse_index(value, "servant_"))
        .filter(|index| *index < 3)
    else {
        return;
    };
    let Some(member) = members[source_index] else {
        return;
    };
    let next_count = np_use_counts.get(&member).copied().unwrap_or(0) + 1;
    np_use_counts.insert(member, next_count);

    for rule in change_order_rules() {
        if rule.servant_id != member.servant_id {
            continue;
        }
        if rule.timing != ChangeOrderTiming::Immediate {
            continue;
        }
        if let ChangeOrderTrigger::AttackCard {
            card: trigger_card,
            activation_use_count,
        } = &rule.trigger
        {
            let count_matches = activation_use_count
                .map(|count| count == next_count)
                .unwrap_or(true);
            if trigger_card == "np" && count_matches {
                apply_member_change_order_effect(members, source_index, &rule.effect);
                return;
            }
        }
    }
}

pub(crate) fn front_slot_with_most_cards(
    cards: &[CommandCardMatch],
    party_ids: &[Option<u32>; 3],
) -> Option<usize> {
    let mut counts = [0usize; 3];
    for card in cards {
        let Some(servant_id) = card.servant_id else {
            continue;
        };
        if let Some(index) = party_ids
            .iter()
            .position(|party_id| *party_id == Some(servant_id))
        {
            counts[index] += 1;
        }
    }
    (0..3)
        .filter(|index| party_ids[*index].is_some())
        .max_by_key(|index| (counts[*index], std::cmp::Reverse(*index)))
}

pub(crate) fn grand_auto_order_change_action(
    cards: &[CommandCardMatch],
    party_ids: &[Option<u32>; 3],
    grand_servants: &[GrandServantRuntimeConfig],
) -> Option<Action> {
    let main = grand_servants.first()?;
    if !(3..6).contains(&main.slot_index) {
        return None;
    }
    let front_index = front_slot_with_most_cards(cards, party_ids)?;
    Some(Action::Equipment {
        id: "auto_grand_order_change".into(),
        skill: Some("skill_3".into()),
        target: None,
        order_change: Some(crate::OrderChangeSelection {
            front: Some(format!("servant_{}", front_index + 1)),
            back: Some(format!("servant_{}", main.slot_index + 1)),
        }),
    })
}

pub(crate) fn front_slot_has_servant(ids: &[Option<u32>; 6], value: Option<&str>) -> bool {
    let Some(index) = value.and_then(|value| parse_index(value, "servant_")) else {
        return false;
    };
    index < 3 && ids[index].is_some()
}

pub(crate) fn optional_front_target_available(ids: &[Option<u32>; 6], value: Option<&str>) -> bool {
    match value {
        Some(target) => front_slot_has_servant(ids, Some(target)),
        None => true,
    }
}

pub(crate) fn action_frontline_available(ids: &[Option<u32>; 6], action: &Action) -> bool {
    match action {
        Action::Servant {
            servant, target, ..
        } => {
            front_slot_has_servant(ids, servant.as_deref())
                && optional_front_target_available(ids, target.as_deref())
        }
        Action::Equipment {
            target,
            order_change: Some(order_change),
            ..
        } => {
            let front = order_change
                .front
                .as_deref()
                .and_then(|value| parse_index(value, "servant_"));
            let back = order_change
                .back
                .as_deref()
                .and_then(|value| parse_index(value, "servant_"));
            matches!((front, back), (Some(front), Some(back)) if front < 3 && back < 6 && ids[front].is_some() && ids[back].is_some())
                && optional_front_target_available(ids, target.as_deref())
        }
        Action::Equipment { target, .. } | Action::CommandSpell { target, .. } => {
            optional_front_target_available(ids, target.as_deref())
        }
    }
}

#[cfg(test)]
pub(crate) fn current_slot_for_original_selection(
    ids: &[Option<u32>; 6],
    original_ids: &[Option<u32>; 6],
    value: &str,
    allowed: std::ops::Range<usize>,
) -> Option<String> {
    let original_index = parse_index(value, "servant_")?;
    let servant_id = original_ids.get(original_index).copied().flatten()?;
    let current_index = ids
        .iter()
        .position(|current_id| *current_id == Some(servant_id))?;
    if !allowed.contains(&current_index) {
        return None;
    }
    Some(format!("servant_{}", current_index + 1))
}

pub(crate) fn current_slot_for_original_member_selection(
    members: &[Option<PartyMemberRuntime>; 6],
    original_members: &[Option<PartyMemberRuntime>; 6],
    value: &str,
    allowed: std::ops::Range<usize>,
) -> Option<String> {
    let original_index = parse_index(value, "servant_")?;
    let original_member = original_members.get(original_index).copied().flatten()?;
    let current_index = members
        .iter()
        .position(|current_member| *current_member == Some(original_member))?;
    if !allowed.contains(&current_index) {
        return None;
    }
    Some(format!("servant_{}", current_index + 1))
}

#[cfg(test)]
pub(crate) fn resolve_required_slot_to_current_position(
    ids: &[Option<u32>; 6],
    original_ids: &[Option<u32>; 6],
    value: Option<&str>,
    allowed: std::ops::Range<usize>,
) -> Option<Option<String>> {
    Some(Some(current_slot_for_original_selection(
        ids,
        original_ids,
        value?,
        allowed,
    )?))
}

#[cfg(test)]
pub(crate) fn resolve_optional_slot_to_current_position(
    ids: &[Option<u32>; 6],
    original_ids: &[Option<u32>; 6],
    value: Option<&str>,
) -> Option<Option<String>> {
    match value {
        Some(value) => Some(Some(current_slot_for_original_selection(
            ids,
            original_ids,
            value,
            0..3,
        )?)),
        None => Some(None),
    }
}

#[cfg(test)]
pub(crate) fn resolve_action_to_current_positions(
    ids: &[Option<u32>; 6],
    original_ids: &[Option<u32>; 6],
    action: &Action,
) -> Option<Action> {
    match action {
        Action::Servant {
            id,
            servant,
            target,
            ..
        } => Some(Action::Servant {
            id: id.clone(),
            servant: resolve_required_slot_to_current_position(
                ids,
                original_ids,
                servant.as_deref(),
                0..3,
            )?,
            skill: match action {
                Action::Servant { skill, .. } => skill.clone(),
                _ => None,
            },
            target: resolve_optional_slot_to_current_position(
                ids,
                original_ids,
                target.as_deref(),
            )?,
        }),
        Action::Equipment {
            id,
            skill,
            target,
            order_change: Some(order_change),
            ..
        } => Some(Action::Equipment {
            id: id.clone(),
            skill: skill.clone(),
            target: resolve_optional_slot_to_current_position(
                ids,
                original_ids,
                target.as_deref(),
            )?,
            order_change: Some(crate::OrderChangeSelection {
                front: resolve_required_slot_to_current_position(
                    ids,
                    original_ids,
                    order_change.front.as_deref(),
                    0..3,
                )?,
                back: resolve_required_slot_to_current_position(
                    ids,
                    original_ids,
                    order_change.back.as_deref(),
                    3..6,
                )?,
            }),
        }),
        Action::Equipment {
            id,
            skill,
            target,
            order_change: None,
        } => Some(Action::Equipment {
            id: id.clone(),
            skill: skill.clone(),
            target: resolve_optional_slot_to_current_position(
                ids,
                original_ids,
                target.as_deref(),
            )?,
            order_change: None,
        }),
        Action::CommandSpell { id, spell, target } => Some(Action::CommandSpell {
            id: id.clone(),
            spell: spell.clone(),
            target: resolve_optional_slot_to_current_position(
                ids,
                original_ids,
                target.as_deref(),
            )?,
        }),
    }
}

pub(crate) fn resolve_required_member_slot_to_current_position(
    members: &[Option<PartyMemberRuntime>; 6],
    original_members: &[Option<PartyMemberRuntime>; 6],
    value: Option<&str>,
    allowed: std::ops::Range<usize>,
) -> Option<Option<String>> {
    Some(Some(current_slot_for_original_member_selection(
        members,
        original_members,
        value?,
        allowed,
    )?))
}

pub(crate) fn resolve_optional_member_slot_to_current_position(
    members: &[Option<PartyMemberRuntime>; 6],
    original_members: &[Option<PartyMemberRuntime>; 6],
    value: Option<&str>,
) -> Option<Option<String>> {
    match value {
        Some(value) => Some(Some(current_slot_for_original_member_selection(
            members,
            original_members,
            value,
            0..3,
        )?)),
        None => Some(None),
    }
}

pub(crate) fn resolve_action_to_current_member_positions(
    members: &[Option<PartyMemberRuntime>; 6],
    original_members: &[Option<PartyMemberRuntime>; 6],
    action: &Action,
) -> Option<Action> {
    match action {
        Action::Servant {
            id,
            servant,
            target,
            ..
        } => Some(Action::Servant {
            id: id.clone(),
            servant: resolve_required_member_slot_to_current_position(
                members,
                original_members,
                servant.as_deref(),
                0..3,
            )?,
            skill: match action {
                Action::Servant { skill, .. } => skill.clone(),
                _ => None,
            },
            target: resolve_optional_member_slot_to_current_position(
                members,
                original_members,
                target.as_deref(),
            )?,
        }),
        Action::Equipment {
            id,
            skill,
            target,
            order_change: Some(order_change),
            ..
        } => Some(Action::Equipment {
            id: id.clone(),
            skill: skill.clone(),
            target: resolve_optional_member_slot_to_current_position(
                members,
                original_members,
                target.as_deref(),
            )?,
            order_change: Some(crate::OrderChangeSelection {
                front: resolve_required_member_slot_to_current_position(
                    members,
                    original_members,
                    order_change.front.as_deref(),
                    0..3,
                )?,
                back: resolve_required_member_slot_to_current_position(
                    members,
                    original_members,
                    order_change.back.as_deref(),
                    3..6,
                )?,
            }),
        }),
        Action::Equipment {
            id,
            skill,
            target,
            order_change: None,
        } => Some(Action::Equipment {
            id: id.clone(),
            skill: skill.clone(),
            target: resolve_optional_member_slot_to_current_position(
                members,
                original_members,
                target.as_deref(),
            )?,
            order_change: None,
        }),
        Action::CommandSpell { id, spell, target } => Some(Action::CommandSpell {
            id: id.clone(),
            spell: spell.clone(),
            target: resolve_optional_member_slot_to_current_position(
                members,
                original_members,
                target.as_deref(),
            )?,
        }),
    }
}

pub(crate) fn action_frontline_label(action: &Action) -> String {
    match action {
        Action::Servant {
            servant, target, ..
        } => servant
            .as_deref()
            .or(target.as_deref())
            .unwrap_or("从者")
            .to_string(),
        Action::Equipment {
            target,
            order_change: Some(order_change),
            ..
        } => order_change
            .front
            .as_deref()
            .or(order_change.back.as_deref())
            .or(target.as_deref())
            .unwrap_or("从者")
            .to_string(),
        Action::Equipment { target, .. } | Action::CommandSpell { target, .. } => {
            target.as_deref().unwrap_or("从者").to_string()
        }
    }
}

pub(crate) fn advanced_startup_flow_actions(
    scene: &AdvancedBattleScene,
    control_count: usize,
    startup_control_count: usize,
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
    actions.extend(scene.startup_actions.iter().cloned());
    actions.extend(
        scene
            .control_actions
            .iter()
            .skip(startup_control_count)
            .take(control_count.saturating_sub(startup_control_count))
            .cloned(),
    );
    actions
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
                    let ids = party_member_ids(&members);
                    if action_frontline_available(&ids, action) {
                        apply_party_member_lineup_change(&mut members, action);
                    }
                }
            }

            if before_current_scene || before_current_turn {
                for card in &turn.attack_priority {
                    apply_attack_card_member_lineup_change(&mut members, card, &mut np_use_counts);
                }
                for action in turn_preparation_actions(turn) {
                    let ids = party_member_ids(&members);
                    if action_frontline_available(&ids, action) {
                        apply_party_member_lineup_change_at(
                            &mut members,
                            action,
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

/// Parse a template-key style servant name like ``"servant_215"`` into its
/// numeric id. Returns ``None`` for any other string shape so callers can
/// gracefully drop unknown supports instead of erroring.
pub(crate) fn parse_servant_name_id(name: &str) -> Option<u32> {
    name.strip_prefix("servant_")?.parse::<u32>().ok()
}

impl Runner {
    /// Build the front-line ``[party_slot_0, party_slot_1, party_slot_2]``
    /// id map. Player-configured slots take precedence; any remaining
    /// front-line slot is assumed to be the support (its id parsed out of
    /// ``support_servant_name`` when the user pinned a specific servant).
    pub(crate) fn build_party_ids(&self) -> [Option<u32>; 3] {
        let full = self.build_full_party_ids();
        [full[0], full[1], full[2]]
    }

    pub(crate) fn build_full_party_ids(&self) -> [Option<u32>; 6] {
        party_member_ids(&self.build_full_party_members())
    }

    pub(crate) fn build_full_party_members(&self) -> [Option<PartyMemberRuntime>; 6] {
        let mut full: [Option<PartyMemberRuntime>; 6] = [None, None, None, None, None, None];
        for sel in &self.config.servant_selections {
            let slot = sel.slot_index as usize;
            if slot < 6 {
                full[slot] = Some(PartyMemberRuntime {
                    slot_index: slot,
                    servant_id: sel.servant_id,
                    is_support: false,
                });
            }
        }

        let support_id = self.config.support_servant_id.or_else(|| {
            self.config
                .support_servant_name
                .as_deref()
                .and_then(parse_servant_name_id)
        });
        if let Some(support_id) = support_id {
            if let Some(slot) = self
                .config
                .support_slot_index
                .and_then(|slot| usize::try_from(slot).ok())
                .filter(|slot| *slot < full.len())
            {
                full[slot] = Some(PartyMemberRuntime {
                    slot_index: slot,
                    servant_id: support_id,
                    is_support: true,
                });
            } else {
                for (slot_index, slot) in full.iter_mut().take(3).enumerate() {
                    if slot.is_none() {
                        *slot = Some(PartyMemberRuntime {
                            slot_index,
                            servant_id: support_id,
                            is_support: true,
                        });
                        break;
                    }
                }
            }
        }
        full
    }

    pub(crate) fn normal_current_party_ids(&self) -> [Option<u32>; 3] {
        let full = normal_current_party_members_from(
            self.build_full_party_members(),
            &self.scenes,
            self.battle.current_scene_index,
            self.battle.current_turn_index,
            self.battle.executed_turn_key,
        );
        let ids = party_member_ids(&full);
        [ids[0], ids[1], ids[2]]
    }

    pub(crate) fn grand_servant_runtime_configs(&self) -> Vec<GrandServantRuntimeConfig> {
        let full = self.build_full_party_members();
        let mut seen = HashSet::new();
        self.config
            .grand_servants
            .iter()
            .filter_map(|config| {
                let slot = usize::try_from(config.slot_index).ok()?;
                if !seen.insert(slot) {
                    return None;
                }
                let member = full.get(slot).copied().flatten()?;
                let servant_id = member.servant_id;
                Some(GrandServantRuntimeConfig {
                    slot_index: slot,
                    servant_id,
                    np_card: config.np_card.clone(),
                    priority: config.priority.clone(),
                })
            })
            .take(2)
            .collect()
    }

    pub(crate) fn advanced_party_ids_after_control(
        &self,
        scene: &AdvancedBattleScene,
        control_count: usize,
    ) -> [Option<u32>; 3] {
        self.advanced_party_ids_after_actions(scene.control_actions.iter().take(control_count))
    }

    pub(crate) fn advanced_party_ids_after_startup_flow(
        &self,
        scene: &AdvancedBattleScene,
        control_count: usize,
        startup_control_count: usize,
    ) -> [Option<u32>; 3] {
        let actions = advanced_startup_flow_actions(
            scene,
            control_count,
            startup_control_count,
            self.battle
                .advanced_auto_order_changes
                .get(&self.battle.current_scene_index),
        );
        self.advanced_party_ids_after_actions(actions.iter())
    }

    pub(crate) fn advanced_party_ids_after_actions<'a>(
        &self,
        actions: impl Iterator<Item = &'a Action>,
    ) -> [Option<u32>; 3] {
        let mut members = self.build_full_party_members();
        for action in actions {
            let ids = party_member_ids(&members);
            if action_frontline_available(&ids, action) {
                apply_party_member_lineup_change(&mut members, action);
            }
        }
        let ids = party_member_ids(&members);
        [ids[0], ids[1], ids[2]]
    }

    pub(crate) fn advanced_actions_and_party_after(
        &self,
        already_executed: impl Iterator<Item = Action>,
        pending: impl Iterator<Item = Action>,
    ) -> (Vec<Action>, [Option<u32>; 3]) {
        let mut members = self.build_full_party_members();
        for action in already_executed {
            let ids = party_member_ids(&members);
            if action_frontline_available(&ids, &action) {
                apply_party_member_lineup_change(&mut members, &action);
            }
        }

        let mut actions = Vec::new();
        for action in pending {
            let ids = party_member_ids(&members);
            if !action_frontline_available(&ids, &action) {
                self.emit(
                    "Battle",
                    &format!("跳过行动：{} 不在前排", action_frontline_label(&action)),
                );
                continue;
            }
            apply_party_member_lineup_change(&mut members, &action);
            actions.push(action);
        }
        let ids = party_member_ids(&members);
        (actions, [ids[0], ids[1], ids[2]])
    }
}
