//! Runner-owned party state and action resolution.
//!
//! Pure lineup transformations remain in the parent module; this module
//! adapts them to the active runner configuration and operation log.

use super::*;

pub(crate) fn build_party_members(config: &RunConfig) -> [Option<PartyMemberRuntime>; 6] {
    let mut members = std::array::from_fn(|_| None);
    for selection in &config.servant_selections {
        let index = selection.slot_index as usize;
        if let Some(slot) = members.get_mut(index) {
            *slot = Some(PartyMemberRuntime {
                member_id: selection.member_id.clone(),
                slot_index: index,
                servant_id: Some(selection.servant_id),
                is_support: false,
            });
        }
    }

    let support_id = config.support_servant_id.or_else(|| {
        config.support_servant_name.as_deref().and_then(parse_servant_name_id)
    });
    let support_index = config
        .support_slot_index
        .map(|index| index as usize)
        .filter(|&index| index < members.len())
        .or_else(|| support_id.and_then(|_| members[..3].iter().position(Option::is_none)));
    if let Some(index) = support_index {
        members[index] = Some(PartyMemberRuntime {
            member_id: config.support_member_id.clone(),
            slot_index: index,
            servant_id: support_id,
            is_support: true,
        });
    }
    members
}

impl Runner {
    pub(crate) fn build_party_ids(&self) -> [Option<u32>; 3] {
        let full = self.build_full_party_ids();
        [full[0], full[1], full[2]]
    }

    pub(crate) fn build_full_party_ids(&self) -> [Option<u32>; 6] {
        party_member_ids(&self.build_full_party_members())
    }

    pub(crate) fn build_full_party_members(&self) -> [Option<PartyMemberRuntime>; 6] {
        build_party_members(&self.config)
    }

    pub(crate) fn normal_current_party_members(&self) -> [Option<PartyMemberRuntime>; 3] {
        let full = normal_current_party_members_from(
            self.build_full_party_members(),
            &self.scenes,
            self.battle.current_scene_index,
            self.battle.current_turn_index,
            self.battle.executed_turn_key,
        );
        frontline_party_members(&full)
    }

    pub(crate) fn resolve_normal_turn_for_current_members(&self, turn: &BattleTurn) -> BattleTurn {
        let original_members = self.build_full_party_members();
        let mut members = normal_current_party_members_from(
            original_members.clone(),
            &self.scenes,
            self.battle.current_scene_index,
            self.battle.current_turn_index,
            self.battle.executed_turn_key,
        );
        let mut preparation_actions = Vec::new();

        for action in turn_preparation_actions(turn) {
            let Some(resolved) =
                resolve_available_member_action(&members, &original_members, action)
            else {
                self.emit_action("跳过行动：目标不在前排", skipped_action_log_meta(action));
                continue;
            };
            apply_party_member_lineup_change(&mut members, &resolved);
            preparation_actions.push(resolved);
        }

        BattleTurn {
            id: turn.id.clone(),
            preparation_actions,
            servant_actions: Vec::new(),
            equipment_actions: Vec::new(),
            command_spell_actions: Vec::new(),
            enemy_target: turn.enemy_target.clone(),
            attack_priority: turn.attack_priority.clone(),
            attack_mode: turn.attack_mode,
            critical_strategy: turn.critical_strategy.clone(),
            advanced_card_strategy: turn.advanced_card_strategy.clone(),
        }
    }

    pub(crate) fn grand_servant_runtime_configs(&self) -> Vec<GrandServantRuntimeConfig> {
        let full = self.build_full_party_members();
        let mut seen = HashSet::new();
        let mut configs: Vec<_> = self.config.grand_servants.iter().collect();
        let roles = grand_strategy(self.config.grand_class).definition().roles;
        configs.sort_by_key(|config| {
            config
                .role
                .as_deref()
                .and_then(|role| roles.iter().position(|candidate| candidate.role == role))
                .unwrap_or(usize::MAX)
        });
        configs
            .into_iter()
            .filter_map(|config| {
                let slot = usize::try_from(config.slot_index).ok()?;
                if !seen.insert(slot) {
                    return None;
                }
                let member = config
                    .member_id
                    .as_deref()
                    .and_then(|member_id| {
                        full.iter()
                            .flatten()
                            .find(|member| member.member_id.as_deref() == Some(member_id))
                    })
                    .or_else(|| {
                        config.servant_id.and_then(|servant_id| {
                            full.iter().flatten().find(|member| {
                                member.servant_id == Some(servant_id)
                                    && member.is_support == config.is_support
                            })
                        })
                    })
                    .or_else(|| full.get(slot).and_then(|member| member.as_ref()))?;
                let servant_id = member.servant_id?;
                Some(GrandServantRuntimeConfig {
                    slot_index: slot,
                    servant_id,
                    is_support: member.is_support,
                    np_card: config.np_card.clone(),
                    priority: config.priority.clone(),
                    role: config.role.clone().unwrap_or_default(),
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
        let members = self
            .advanced_party_members_after_actions(scene.control_actions.iter().take(control_count));
        let (ids, _) = frontline_party_ids_and_supports(&members);
        ids
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
            *self
                .battle
                .advanced_turn_indices
                .get(&self.battle.current_scene_index)
                .unwrap_or(&0),
            self.battle
                .advanced_auto_order_changes
                .get(&self.battle.current_scene_index),
        );
        let members = self.advanced_party_members_after_actions(actions.iter());
        let (ids, _) = frontline_party_ids_and_supports(&members);
        ids
    }

    pub(crate) fn advanced_party_supports_after_control(
        &self,
        scene: &AdvancedBattleScene,
        control_count: usize,
    ) -> [bool; 3] {
        let members = self
            .advanced_party_members_after_actions(scene.control_actions.iter().take(control_count));
        let (_, supports) = frontline_party_ids_and_supports(&members);
        supports
    }

    pub(crate) fn advanced_party_supports_after_startup_flow(
        &self,
        scene: &AdvancedBattleScene,
        control_count: usize,
        startup_control_count: usize,
    ) -> [bool; 3] {
        let actions = advanced_startup_flow_actions(
            scene,
            control_count,
            startup_control_count,
            *self
                .battle
                .advanced_turn_indices
                .get(&self.battle.current_scene_index)
                .unwrap_or(&0),
            self.battle
                .advanced_auto_order_changes
                .get(&self.battle.current_scene_index),
        );
        let members = self.advanced_party_members_after_actions(actions.iter());
        let (_, supports) = frontline_party_ids_and_supports(&members);
        supports
    }

    pub(crate) fn advanced_party_members_after_actions<'a>(
        &self,
        actions: impl Iterator<Item = &'a Action>,
    ) -> [Option<PartyMemberRuntime>; 3] {
        let original_members = self.build_full_party_members();
        let mut members = original_members.clone();
        for action in actions {
            if let Some(resolved) =
                resolve_available_member_action(&members, &original_members, action)
            {
                apply_party_member_lineup_change(&mut members, &resolved);
            }
        }
        frontline_party_members(&members)
    }

    pub(crate) fn advanced_actions_and_members_after(
        &self,
        already_executed: impl Iterator<Item = Action>,
        pending: impl Iterator<Item = Action>,
    ) -> (Vec<Action>, [Option<PartyMemberRuntime>; 3]) {
        let original_members = self.build_full_party_members();
        let mut members = original_members.clone();
        for action in already_executed {
            if let Some(resolved) =
                resolve_available_member_action(&members, &original_members, &action)
            {
                apply_party_member_lineup_change(&mut members, &resolved);
            }
        }

        let mut actions = Vec::new();
        for action in pending {
            let Some(resolved) =
                resolve_available_member_action(&members, &original_members, &action)
            else {
                self.emit_action("跳过行动：目标不在前排", skipped_action_log_meta(&action));
                continue;
            };
            apply_party_member_lineup_change(&mut members, &resolved);
            actions.push(resolved);
        }
        (actions, frontline_party_members(&members))
    }
}
