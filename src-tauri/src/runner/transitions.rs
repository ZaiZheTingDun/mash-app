//! Runtime bridge from verified battle actions to the pure transition reducer.

use super::*;
use crate::battle_transitions::{
    apply_action, apply_override, end_turn, resolve_metadata, transition_variant, ActionType,
    BattleStateOverride, MemberTransitionState,
};

#[derive(Debug, Clone)]
pub(crate) struct RuntimeMemberTransition {
    servant_id: u32,
    variant_key: String,
    state: MemberTransitionState,
}

fn runtime_key(member: &PartyMemberRuntime) -> String {
    member.member_id.clone().unwrap_or_else(|| {
        format!(
            "legacy:{}:{}:{}",
            member.slot_index, member.servant_id, member.is_support
        )
    })
}

fn transition_summary(metadata: &crate::battle_transitions::ResolvedBattleMetadata) -> String {
    let skills = metadata
        .skills
        .iter()
        .map(|skill| {
            skill
                .as_ref()
                .map(|skill| skill.id.to_string())
                .unwrap_or_else(|| "-".into())
        })
        .collect::<Vec<_>>()
        .join("/");
    let np = metadata
        .noble_phantasm
        .as_ref()
        .map(|np| format!("{}:{}", np.id, np.card.as_deref().unwrap_or("auto")))
        .unwrap_or_else(|| "-".into());
    let states = if metadata.active_conditions.is_empty() {
        "none".to_string()
    } else {
        metadata
            .active_conditions
            .iter()
            .map(|condition| {
                format!(
                    "{}(turns={},stacks={})",
                    condition.state_key,
                    condition
                        .remaining_turns
                        .map(|turns| turns.to_string())
                        .unwrap_or_else(|| "∞".into()),
                    condition.stacks
                )
            })
            .collect::<Vec<_>>()
            .join(",")
    };
    format!("skills={skills} np={np} states={states}")
}

impl Runner {
    fn transition_member_for_ref(
        &self,
        member_id: Option<&str>,
        servant_id: Option<u32>,
        is_support: bool,
        position: Option<&str>,
    ) -> Option<PartyMemberRuntime> {
        let members = self.build_full_party_members();
        if let Some(member_id) = member_id {
            if let Some(member) = members
                .iter()
                .flatten()
                .find(|member| member.member_id.as_deref() == Some(member_id))
            {
                return Some(member.clone());
            }
        }
        if let Some(index) = position.and_then(|value| parse_index(value, "servant_")) {
            if let Some(member) = members.get(index).cloned().flatten() {
                if servant_id.is_none_or(|id| member.servant_id == id) {
                    return Some(member);
                }
            }
        }
        members
            .into_iter()
            .flatten()
            .find(|member| servant_id == Some(member.servant_id) && member.is_support == is_support)
    }

    fn transition_entry(
        &mut self,
        member: &PartyMemberRuntime,
    ) -> Option<(
        &'static crate::battle_transitions::TransitionVariant,
        &mut MemberTransitionState,
    )> {
        let variant_key = member
            .variant_key
            .clone()
            .unwrap_or_else(|| format!("{}:1", member.servant_id));
        let variant = transition_variant(self.server, member.servant_id, &variant_key)?;
        let entry = self
            .battle
            .transition_states
            .entry(runtime_key(member))
            .or_insert_with(|| RuntimeMemberTransition {
                servant_id: member.servant_id,
                variant_key,
                state: MemberTransitionState::default(),
            });
        Some((variant, &mut entry.state))
    }

    pub(crate) fn apply_battle_state_overrides(&mut self, overrides: &[BattleStateOverride]) {
        let mut logs = Vec::new();
        for item in overrides {
            let Some(member) = self.transition_member_for_ref(
                item.member_id.as_deref(),
                item.servant_id,
                item.is_support,
                None,
            ) else {
                continue;
            };
            let Some((variant, state)) = self.transition_entry(&member) else {
                continue;
            };
            apply_override(
                variant,
                state,
                &item.state_key,
                item.mode,
                item.remaining_turns,
                item.stacks,
            );
            logs.push(format!(
                "member={} manualOverride {}: {}",
                runtime_key(&member),
                item.state_key,
                transition_summary(&resolve_metadata(variant, state, false))
            ));
        }
        for log in logs {
            self.emit_debug("BattleState", &log);
        }
    }

    pub(crate) fn apply_confirmed_skill_transition(
        &mut self,
        member_id: Option<&str>,
        servant_id: Option<u32>,
        is_support: bool,
        position: Option<&str>,
        slot: u32,
        selection_index: Option<u32>,
    ) {
        let Some(member) =
            self.transition_member_for_ref(member_id, servant_id, is_support, position)
        else {
            return;
        };
        let Some((variant, state)) = self.transition_entry(&member) else {
            return;
        };
        let skill_id = resolve_metadata(variant, state, false)
            .skills
            .get(slot.saturating_sub(1) as usize)
            .and_then(|skill| skill.as_ref())
            .map(|skill| skill.id);
        if let Some(skill_id) = skill_id {
            apply_action(variant, state, ActionType::Skill, skill_id, selection_index);
            let log = format!(
                "member={} skillConfirmed slot={slot} skill={skill_id}: {}",
                runtime_key(&member),
                transition_summary(&resolve_metadata(variant, state, false))
            );
            self.emit_debug("BattleState", &log);
        }
    }

    pub(crate) fn apply_submitted_np_transition(&mut self, slot: usize, servant_id: u32) {
        let member = if self.advanced_mode {
            self.advanced_scenes
                .get(self.battle.current_scene_index)
                .and_then(|scene| {
                    self.advanced_current_party_members(scene)
                        .get(slot)
                        .cloned()
                        .flatten()
                })
                .filter(|member| member.servant_id == servant_id)
        } else {
            self.normal_current_party_members()
                .get(slot)
                .cloned()
                .flatten()
                .or_else(|| {
                    self.build_full_party_members()
                        .into_iter()
                        .flatten()
                        .find(|member| member.servant_id == servant_id)
                })
        };
        let Some(member) = member else { return };
        let Some((variant, state)) = self.transition_entry(&member) else {
            return;
        };
        if let Some(np) = resolve_metadata(variant, state, false).noble_phantasm {
            apply_action(variant, state, ActionType::NoblePhantasm, np.id, None);
            let log = format!(
                "member={} noblePhantasmSubmitted np={}: {}",
                runtime_key(&member),
                np.id,
                transition_summary(&resolve_metadata(variant, state, false))
            );
            self.emit_debug("BattleState", &log);
        }
    }

    pub(crate) fn end_transition_turn(&mut self) {
        let server = self.server;
        let mut logs = Vec::new();
        for (member_key, entry) in self.battle.transition_states.iter_mut() {
            if let Some(variant) = transition_variant(server, entry.servant_id, &entry.variant_key)
            {
                end_turn(variant, &mut entry.state);
                logs.push(format!(
                    "member={member_key} turnEnd: {}",
                    transition_summary(&resolve_metadata(variant, &entry.state, false))
                ));
            }
        }
        for log in logs {
            self.emit_debug("BattleState", &log);
        }
    }

    pub(crate) fn resolved_np_card_for_member(
        &self,
        member: &PartyMemberRuntime,
    ) -> Option<String> {
        let variant_key = member
            .variant_key
            .as_deref()
            .map(str::to_string)
            .unwrap_or_else(|| format!("{}:1", member.servant_id));
        let variant = transition_variant(self.server, member.servant_id, &variant_key)?;
        let empty = MemberTransitionState::default();
        let state = self
            .battle
            .transition_states
            .get(&runtime_key(member))
            .map(|entry| &entry.state)
            .unwrap_or(&empty);
        resolve_metadata(variant, state, false).noble_phantasm?.card
    }
}
