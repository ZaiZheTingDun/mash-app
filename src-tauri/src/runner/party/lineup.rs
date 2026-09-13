use super::*;

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
            serde_json::from_str(include_str!("../../resources/change_order_servants.json"))
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
        .cloned()
        .filter(|slot| slot.is_some())
        .collect();
    for slot in 3..6 {
        members[slot] = backline.get(slot - 3).cloned().flatten();
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
    members[index] = replacement.and_then(|slot| members[slot].clone());
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
    let Some(member) = members[index].clone() else {
        return;
    };
    compact_member_backline(members);
    let Some(replacement) = (3..6).find(|slot| members[*slot].is_some()) else {
        return;
    };
    members[index] = members[replacement].clone();
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
    let (Some(member), Some(skill)) = (members[source_index].as_ref(), skill.as_deref()) else {
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
    let Some(member) = members[source_index].clone() else {
        return;
    };
    let next_count = np_use_counts.get(&member).copied().unwrap_or(0) + 1;
    np_use_counts.insert(member.clone(), next_count);

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
