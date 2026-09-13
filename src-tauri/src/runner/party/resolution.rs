use super::*;

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
        Action::EnemyTarget { .. } => true,
    }
}

pub(crate) fn resolve_available_member_action(
    members: &[Option<PartyMemberRuntime>; 6],
    original_members: &[Option<PartyMemberRuntime>; 6],
    action: &Action,
) -> Option<Action> {
    let resolved = resolve_action_to_current_member_positions(members, original_members, action)?;
    let ids = party_member_ids(members);
    action_frontline_available(&ids, &resolved).then_some(resolved)
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
    let original_member = original_members.get(original_index).cloned().flatten()?;
    if let Some(current_index) = members
        .iter()
        .position(|current_member| current_member.as_ref() == Some(&original_member))
    {
        if !allowed.contains(&current_index) {
            return None;
        }
        return Some(format!("servant_{}", current_index + 1));
    }
    // Original servant was removed from the field (e.g. NP self-death).
    // Fall back to whoever currently occupies the same slot index so that
    // subsequent actions configured for that position still execute.
    if allowed.contains(&original_index) && members[original_index].is_some() {
        return Some(format!("servant_{}", original_index + 1));
    }
    None
}

fn current_slot_for_member_ref(
    members: &[Option<PartyMemberRuntime>; 6],
    original_members: &[Option<PartyMemberRuntime>; 6],
    fallback_value: Option<&str>,
    member_id: Option<&str>,
    servant_id: Option<u32>,
    is_support: bool,
    allowed: std::ops::Range<usize>,
) -> Option<Option<String>> {
    if let Some(member_id) = member_id {
        if let Some(current_index) = members.iter().position(|member| {
            member
                .as_ref()
                .and_then(|member| member.member_id.as_deref())
                == Some(member_id)
        }) {
            if !allowed.contains(&current_index) {
                return None;
            }
            return Some(Some(format!("servant_{}", current_index + 1)));
        }
        // member_id not found: servant was removed from the field; fall through
    }

    if let Some(servant_id) = servant_id {
        if let Some(current_index) = members.iter().position(|member| {
            member.as_ref().is_some_and(|member| {
                member.servant_id == servant_id && member.is_support == is_support
            })
        }) {
            if !allowed.contains(&current_index) {
                return None;
            }
            return Some(Some(format!("servant_{}", current_index + 1)));
        }
        // servant_id not found: servant was removed from the field; fall through
    }

    match fallback_value {
        Some(value) => Some(Some(current_slot_for_original_member_selection(
            members,
            original_members,
            value,
            allowed,
        )?)),
        None => Some(None),
    }
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
            skill_selection,
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
            servant_member_id: None,
            servant_id: None,
            servant_is_support: false,
            skill: match action {
                Action::Servant { skill, .. } => skill.clone(),
                _ => None,
            },
            skill_selection: skill_selection.clone(),
            target: resolve_optional_slot_to_current_position(
                ids,
                original_ids,
                target.as_deref(),
            )?,
            target_member_id: None,
            target_servant_id: None,
            target_is_support: false,
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
            target_member_id: None,
            target_servant_id: None,
            target_is_support: false,
            order_change: Some(crate::OrderChangeSelection {
                front: resolve_required_slot_to_current_position(
                    ids,
                    original_ids,
                    order_change.front.as_deref(),
                    0..3,
                )?,
                front_member_id: None,
                front_servant_id: None,
                front_is_support: false,
                back: resolve_required_slot_to_current_position(
                    ids,
                    original_ids,
                    order_change.back.as_deref(),
                    3..6,
                )?,
                back_member_id: None,
                back_servant_id: None,
                back_is_support: false,
            }),
        }),
        Action::Equipment {
            id,
            skill,
            target,
            order_change: None,
            ..
        } => Some(Action::Equipment {
            id: id.clone(),
            skill: skill.clone(),
            target: resolve_optional_slot_to_current_position(
                ids,
                original_ids,
                target.as_deref(),
            )?,
            target_member_id: None,
            target_servant_id: None,
            target_is_support: false,
            order_change: None,
        }),
        Action::CommandSpell {
            id, spell, target, ..
        } => Some(Action::CommandSpell {
            id: id.clone(),
            spell: spell.clone(),
            target: resolve_optional_slot_to_current_position(
                ids,
                original_ids,
                target.as_deref(),
            )?,
            target_member_id: None,
            target_servant_id: None,
            target_is_support: false,
        }),
        Action::EnemyTarget { id, target } => Some(Action::EnemyTarget {
            id: id.clone(),
            target: target.clone(),
        }),
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
            servant_member_id,
            servant_id,
            servant_is_support,
            skill_selection,
            target,
            target_member_id,
            target_servant_id,
            target_is_support,
            ..
        } => Some(Action::Servant {
            id: id.clone(),
            servant: current_slot_for_member_ref(
                members,
                original_members,
                servant.as_deref(),
                servant_member_id.as_deref(),
                *servant_id,
                *servant_is_support,
                0..3,
            )?,
            servant_member_id: servant_member_id.clone(),
            servant_id: *servant_id,
            servant_is_support: *servant_is_support,
            skill: match action {
                Action::Servant { skill, .. } => skill.clone(),
                _ => None,
            },
            skill_selection: skill_selection.clone(),
            target: current_slot_for_member_ref(
                members,
                original_members,
                target.as_deref(),
                target_member_id.as_deref(),
                *target_servant_id,
                *target_is_support,
                0..3,
            )?,
            target_member_id: target_member_id.clone(),
            target_servant_id: *target_servant_id,
            target_is_support: *target_is_support,
        }),
        Action::Equipment {
            id,
            skill,
            target,
            target_member_id,
            target_servant_id,
            target_is_support,
            order_change: Some(order_change),
            ..
        } => Some(Action::Equipment {
            id: id.clone(),
            skill: skill.clone(),
            target: current_slot_for_member_ref(
                members,
                original_members,
                target.as_deref(),
                target_member_id.as_deref(),
                *target_servant_id,
                *target_is_support,
                0..3,
            )?,
            target_member_id: target_member_id.clone(),
            target_servant_id: *target_servant_id,
            target_is_support: *target_is_support,
            order_change: Some(crate::OrderChangeSelection {
                front: current_slot_for_member_ref(
                    members,
                    original_members,
                    order_change.front.as_deref(),
                    order_change.front_member_id.as_deref(),
                    order_change.front_servant_id,
                    order_change.front_is_support,
                    0..3,
                )?,
                front_member_id: order_change.front_member_id.clone(),
                front_servant_id: order_change.front_servant_id,
                front_is_support: order_change.front_is_support,
                back: current_slot_for_member_ref(
                    members,
                    original_members,
                    order_change.back.as_deref(),
                    order_change.back_member_id.as_deref(),
                    order_change.back_servant_id,
                    order_change.back_is_support,
                    3..6,
                )?,
                back_member_id: order_change.back_member_id.clone(),
                back_servant_id: order_change.back_servant_id,
                back_is_support: order_change.back_is_support,
            }),
        }),
        Action::Equipment {
            id,
            skill,
            target,
            target_member_id,
            target_servant_id,
            target_is_support,
            order_change: None,
            ..
        } => Some(Action::Equipment {
            id: id.clone(),
            skill: skill.clone(),
            target: current_slot_for_member_ref(
                members,
                original_members,
                target.as_deref(),
                target_member_id.as_deref(),
                *target_servant_id,
                *target_is_support,
                0..3,
            )?,
            target_member_id: target_member_id.clone(),
            target_servant_id: *target_servant_id,
            target_is_support: *target_is_support,
            order_change: None,
        }),
        Action::CommandSpell {
            id,
            spell,
            target,
            target_member_id,
            target_servant_id,
            target_is_support,
            ..
        } => Some(Action::CommandSpell {
            id: id.clone(),
            spell: spell.clone(),
            target: current_slot_for_member_ref(
                members,
                original_members,
                target.as_deref(),
                target_member_id.as_deref(),
                *target_servant_id,
                *target_is_support,
                0..3,
            )?,
            target_member_id: target_member_id.clone(),
            target_servant_id: *target_servant_id,
            target_is_support: *target_is_support,
        }),
        Action::EnemyTarget { id, target } => Some(Action::EnemyTarget {
            id: id.clone(),
            target: target.clone(),
        }),
    }
}
