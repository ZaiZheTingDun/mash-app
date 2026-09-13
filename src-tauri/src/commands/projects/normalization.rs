//! Persisted project compatibility and invariant normalization.

use super::*;
use crate::runner::grand_strategy;

pub(crate) fn normalize_project(mut project: Project) -> Project {
    for slot in &mut project.slots {
        let mut ids = Vec::new();
        for id in slot
            .craft_essence_ids
            .iter()
            .copied()
            .chain(slot.craft_essence_id)
        {
            if !ids.contains(&id) {
                ids.push(id);
            }
            if ids.len() == 10 {
                break;
            }
        }
        if slot.kind != "support" || !slot.craft_essence_multi_select {
            ids.truncate(1);
            slot.craft_essence_multi_select = false;
        }
        slot.craft_essence_id = ids.first().copied();
        slot.craft_essence_ids = ids;
    }
    for index in 0..3 {
        let mut ids = Vec::new();
        for id in project.support_grand_craft_essence_id_lists[index]
            .iter()
            .copied()
            .chain(project.support_grand_craft_essence_ids[index])
        {
            if !ids.contains(&id) {
                ids.push(id);
            }
            if ids.len() == if index == 1 { 1 } else { 10 } {
                break;
            }
        }
        if index == 1 {
            ids.truncate(1);
        }
        project.support_grand_craft_essence_ids[index] = ids.first().copied();
        project.support_grand_craft_essence_id_lists[index] = ids;
    }
    let repeat_mode = match project.repeat_mode {
        Some(mode) => mode,
        None if project.repeat_mission => ProjectRepeatMode::Infinite,
        None => ProjectRepeatMode::Single,
    };
    project.repeat_mission = !matches!(repeat_mode, ProjectRepeatMode::Single);
    project.repeat_mode = Some(repeat_mode);
    if !matches!(project.repeat_mode, Some(ProjectRepeatMode::Count)) {
        project.repeat_count = None;
    }
    project.support_star_map_score_min = project
        .support_star_map_score_min
        .map(|score| score.min(62));
    project.support_grand_star_map_score_min = project
        .support_grand_star_map_score_min
        .map(|score| score.min(16));
    project.support_servant_level_min = project
        .support_servant_level_min
        .map(|level| level.clamp(1, 120));
    normalize_grand_card_rule_slots(&mut project);
    normalize_grand_servants(&mut project);
    normalize_project_recognition_settings(&mut project);
    project
}

fn normalize_project_recognition_settings(project: &mut Project) {
    if let Some(settings) = &mut project.recognition_settings {
        if settings.stop_on_five_star_ce_drop == Some(true)
            && settings
                .five_star_ce_drop_target_count
                .is_none_or(|count| count == 0)
        {
            settings.five_star_ce_drop_target_count = Some(1);
        }
        if settings.stop_on_five_star_ce_drop != Some(true)
            && settings.five_star_ce_drop_target_count == Some(1)
        {
            settings.five_star_ce_drop_target_count = None;
        }
    }
}

fn slot_member_metadata(
    slot: &ProjectSlot,
    support_servant_id: Option<u32>,
) -> (String, Option<u32>, bool) {
    let is_support = slot.kind == "support";
    let servant_id = if is_support {
        support_servant_id
    } else {
        slot.servant_id
    };
    (slot.id.clone(), servant_id, is_support)
}

fn normalize_grand_card_rule_slots(project: &mut Project) {
    let slots = project.slots.clone();
    let support_servant_id = project.support_servant_id;
    for rule in &mut project.grand_card_strategy.custom_rules {
        for slot in &mut rule.slots {
            if slot.grand_servant {
                slot.member_id = None;
                slot.slot_index = None;
                continue;
            }
            let valid_slot = slot
                .member_id
                .as_deref()
                .and_then(|member_id| {
                    slots
                        .iter()
                        .enumerate()
                        .find(|(_, project_slot)| project_slot.id == member_id)
                        .map(|(index, project_slot)| (index as u32, project_slot))
                })
                .or_else(|| {
                    slot.slot_index.and_then(|index| {
                        slots
                            .get(index as usize)
                            .map(|project_slot| (index, project_slot))
                    })
                })
                .and_then(|(index, project_slot)| {
                    let (_member_id, actual_id, is_support) =
                        slot_member_metadata(project_slot, support_servant_id);
                    (is_support == slot.is_support
                        && actual_id.is_some()
                        && actual_id == slot.servant_id)
                        .then_some(index)
                });
            let resolved_slot = valid_slot.or_else(|| {
                slot.servant_id.and_then(|servant_id| {
                    slots.iter().enumerate().find_map(|(index, project_slot)| {
                        let (_member_id, actual_id, is_support) =
                            slot_member_metadata(project_slot, support_servant_id);
                        (is_support == slot.is_support && actual_id == Some(servant_id))
                            .then_some(index as u32)
                    })
                })
            });
            slot.slot_index = resolved_slot;
            if let Some(index) = resolved_slot.and_then(|index| slots.get(index as usize)) {
                let (member_id, servant_id, is_support) =
                    slot_member_metadata(index, support_servant_id);
                slot.member_id = Some(member_id);
                slot.servant_id = servant_id;
                slot.is_support = is_support;
            }
        }
    }
}

fn normalize_grand_servants(project: &mut Project) {
    let slots = project.slots.clone();
    let support_servant_id = project.support_servant_id;
    for config in &mut project.grand_servants {
        let resolved_slot = config
            .member_id
            .as_deref()
            .and_then(|member_id| {
                slots
                    .iter()
                    .enumerate()
                    .find(|(_, project_slot)| project_slot.id == member_id)
                    .map(|(index, _)| index as u32)
            })
            .or_else(|| {
                slots
                    .get(config.slot_index as usize)
                    .map(|_| config.slot_index)
            })
            .or_else(|| {
                config.servant_id.and_then(|servant_id| {
                    slots.iter().enumerate().find_map(|(index, project_slot)| {
                        let (_member_id, actual_id, is_support) =
                            slot_member_metadata(project_slot, support_servant_id);
                        (is_support == config.is_support && actual_id == Some(servant_id))
                            .then_some(index as u32)
                    })
                })
            });
        if let Some(index) = resolved_slot {
            config.slot_index = index;
            if let Some(slot) = slots.get(index as usize) {
                let (member_id, servant_id, is_support) =
                    slot_member_metadata(slot, support_servant_id);
                config.member_id = Some(member_id);
                config.servant_id = servant_id;
                config.is_support = is_support;
            }
        }
    }
    grand_strategy(project.grand_class).normalize_servants(&mut project.grand_servants);
}
