//! Data-driven servant skill / Noble Phantasm transitions.
//!
//! Static servant variants answer "which kit does this form start with?".
//! This module owns the pure battle-state reducer that answers "what is active
//! after the actions already performed in this battle?".

use crate::paths::app_assets_dir;
use crate::server::Server;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransitionFile {
    schema_version: u32,
    servants: Vec<TransitionServant>,
}

#[derive(Debug, Clone, Deserialize)]
struct TransitionServant {
    id: u32,
    variants: Vec<TransitionVariant>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransitionVariant {
    variant_ids: Vec<u32>,
    base_skill_ids: [Option<u32>; 3],
    base_noble_phantasm_id: Option<u32>,
    skill_forms: Vec<SkillForm>,
    noble_phantasm_forms: Vec<NoblePhantasmForm>,
    conditions: Vec<Condition>,
    actions: Vec<TransitionAction>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SkillForm {
    pub(crate) id: u32,
    pub(crate) slot: u32,
    pub(crate) name: String,
    pub(crate) icon: Option<String>,
    #[serde(default)]
    pub(crate) target_types: Vec<String>,
    #[serde(default)]
    pub(crate) selection: Option<TransitionSkillSelection>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransitionSkillSelection {
    #[serde(rename = "type")]
    pub(crate) selection_type: String,
    pub(crate) options: Vec<TransitionSelectionOption>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct TransitionSelectionOption {
    pub(crate) index: u32,
    pub(crate) label: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct NoblePhantasmForm {
    pub(crate) id: u32,
    pub(crate) name: String,
    pub(crate) card: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Condition {
    state_key: String,
    name: String,
    duration_turns: Option<u32>,
    #[serde(default)]
    max_stacks: Option<u32>,
    external: bool,
    #[serde(default)]
    effects: Vec<ConditionEffect>,
    #[serde(default)]
    turn_end_effects: Vec<ActionEffect>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum ConditionEffect {
    ReplaceSkill {
        slot: u32,
        #[serde(rename = "skillId")]
        skill_id: u32,
        #[serde(default, rename = "minStacks")]
        min_stacks: Option<u32>,
    },
    ReplaceNoblePhantasm {
        #[serde(rename = "noblePhantasmId")]
        noble_phantasm_id: u32,
        #[serde(default, rename = "minStacks")]
        min_stacks: Option<u32>,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransitionAction {
    #[serde(rename = "type")]
    action_type: ActionType,
    id: u32,
    #[serde(default)]
    selection_index: Option<u32>,
    effects: Vec<ActionEffect>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ActionType {
    Skill,
    NoblePhantasm,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum ActionEffect {
    ApplyState {
        #[serde(rename = "stateKey")]
        state_key: String,
        #[serde(default)]
        stacks: Option<u32>,
    },
    ConsumeState {
        #[serde(rename = "stateKey")]
        state_key: String,
        stacks: u32,
    },
    AdjustCooldown {
        slot: u32,
        amount: u32,
    },
    InvokeSkill {
        #[serde(rename = "skillId")]
        skill_id: u32,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct MemberTransitionState {
    active: BTreeMap<String, ActiveCondition>,
    cooldown_adjustments: BTreeMap<u32, u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ActiveCondition {
    remaining_turns: Option<u32>,
    stacks: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResolvedBattleMetadata {
    pub(crate) skills: [Option<SkillForm>; 3],
    pub(crate) noble_phantasm: Option<NoblePhantasmForm>,
    pub(crate) available_conditions: Vec<AvailableCondition>,
    pub(crate) active_conditions: Vec<ResolvedCondition>,
    pub(crate) cooldown_adjustments: BTreeMap<u32, u32>,
    pub(crate) preview_uncertain: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AvailableCondition {
    pub(crate) state_key: String,
    pub(crate) name: String,
    pub(crate) duration_turns: Option<u32>,
    pub(crate) max_stacks: Option<u32>,
    pub(crate) external: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResolvedCondition {
    pub(crate) state_key: String,
    pub(crate) name: String,
    pub(crate) remaining_turns: Option<u32>,
    pub(crate) stacks: u32,
    pub(crate) external: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BattleStateOverride {
    pub member_id: Option<String>,
    pub servant_id: Option<u32>,
    #[serde(default)]
    pub is_support: bool,
    pub state_key: String,
    pub mode: BattleStateOverrideMode,
    #[serde(default)]
    pub remaining_turns: Option<u32>,
    #[serde(default)]
    pub stacks: Option<u32>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum BattleStateOverrideMode {
    Set,
    Clear,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(crate) enum PreviewEvent {
    Skill {
        slot: u32,
        #[serde(default)]
        selection_index: Option<u32>,
    },
    NoblePhantasm,
    TurnEnd,
    Override {
        #[serde(rename = "stateKey")]
        state_key: String,
        mode: BattleStateOverrideMode,
        #[serde(default)]
        remaining_turns: Option<u32>,
        #[serde(default)]
        stacks: Option<u32>,
    },
    Uncertain,
}

fn transition_file(server: Server) -> Option<&'static TransitionFile> {
    static JP: OnceLock<Option<TransitionFile>> = OnceLock::new();
    static CN: OnceLock<Option<TransitionFile>> = OnceLock::new();
    let slot = match server {
        Server::Jp => &JP,
        Server::Cn => &CN,
    };
    slot.get_or_init(|| {
        let raw = match server {
            Server::Jp => include_str!("resources/servant_battle_transitions.json"),
            Server::Cn => include_str!("resources/servant_battle_transitions_cn.json"),
        };
        match serde_json::from_str::<TransitionFile>(raw) {
            Ok(file) if file.schema_version == 1 => Some(file),
            Ok(file) => {
                eprintln!(
                    "[battle-transitions] unsupported schemaVersion {}",
                    file.schema_version
                );
                None
            }
            Err(error) => {
                eprintln!("[battle-transitions] invalid bundled resource: {error}");
                None
            }
        }
    })
    .as_ref()
}

fn static_variant_ids(server: Server, servant_id: u32, variant_key: &str) -> Option<Vec<u32>> {
    let raw = match server {
        Server::Jp => include_str!("resources/servants_variants.json"),
        Server::Cn => include_str!("resources/servants_variants_cn.json"),
    };
    let entries: Vec<serde_json::Value> = serde_json::from_str(raw).ok()?;
    let index = variant_key
        .split(':')
        .nth(1)
        .and_then(|value| value.parse::<usize>().ok())
        .map(|value| value.saturating_sub(1))
        .unwrap_or(0);
    entries
        .iter()
        .find(|entry| entry.get("id").and_then(|value| value.as_u64()) == Some(servant_id as u64))?
        .get("variants")?
        .as_array()?
        .get(index)?
        .get("ids")?
        .as_array()
        .map(|ids| {
            ids.iter()
                .filter_map(|id| id.as_u64().map(|id| id as u32))
                .collect()
        })
}

pub(crate) fn transition_variant(
    server: Server,
    servant_id: u32,
    variant_key: &str,
) -> Option<&'static TransitionVariant> {
    let ids = static_variant_ids(server, servant_id, variant_key)?;
    transition_file(server)?
        .servants
        .iter()
        .find(|servant| servant.id == servant_id)?
        .variants
        .iter()
        .find(|variant| variant.variant_ids == ids)
}

fn condition<'a>(variant: &'a TransitionVariant, state_key: &str) -> Option<&'a Condition> {
    variant
        .conditions
        .iter()
        .find(|condition| condition.state_key == state_key)
}

fn apply_effect(
    variant: &TransitionVariant,
    state: &mut MemberTransitionState,
    effect: &ActionEffect,
) {
    match effect {
        ActionEffect::ApplyState { state_key, stacks } => {
            let Some(definition) = condition(variant, state_key) else {
                return;
            };
            let active = state
                .active
                .entry(state_key.clone())
                .or_insert(ActiveCondition {
                    remaining_turns: definition.duration_turns,
                    stacks: 0,
                });
            active.remaining_turns = definition.duration_turns;
            active.stacks = active.stacks.saturating_add(stacks.unwrap_or(1));
            if let Some(max) = definition.max_stacks {
                active.stacks = active.stacks.min(max);
            }
        }
        ActionEffect::ConsumeState { state_key, stacks } => {
            if let Some(active) = state.active.get_mut(state_key) {
                active.stacks = active.stacks.saturating_sub(*stacks);
                if active.stacks == 0 {
                    state.active.remove(state_key);
                }
            }
        }
        ActionEffect::AdjustCooldown { slot, amount } => {
            *state.cooldown_adjustments.entry(*slot).or_default() = state
                .cooldown_adjustments
                .get(slot)
                .copied()
                .unwrap_or_default()
                .saturating_add(*amount);
        }
        ActionEffect::InvokeSkill { skill_id } => {
            apply_action(variant, state, ActionType::Skill, *skill_id, None);
        }
    }
}

pub(crate) fn apply_action(
    variant: &TransitionVariant,
    state: &mut MemberTransitionState,
    action_type: ActionType,
    action_id: u32,
    selection_index: Option<u32>,
) {
    let matching: Vec<_> = variant
        .actions
        .iter()
        .filter(|action| {
            action.action_type == action_type
                && action.id == action_id
                && (action.selection_index == selection_index
                    || (selection_index.is_none() && action.selection_index.is_none()))
        })
        .flat_map(|action| action.effects.clone())
        .collect();
    for effect in matching {
        apply_effect(variant, state, &effect);
    }
}

pub(crate) fn apply_override(
    variant: &TransitionVariant,
    state: &mut MemberTransitionState,
    state_key: &str,
    mode: BattleStateOverrideMode,
    remaining_turns: Option<u32>,
    stacks: Option<u32>,
) {
    match mode {
        BattleStateOverrideMode::Clear => {
            state.active.remove(state_key);
        }
        BattleStateOverrideMode::Set => {
            let Some(definition) = condition(variant, state_key) else {
                return;
            };
            state.active.insert(
                state_key.to_string(),
                ActiveCondition {
                    remaining_turns: remaining_turns.or(definition.duration_turns),
                    stacks: stacks
                        .unwrap_or(1)
                        .min(definition.max_stacks.unwrap_or(u32::MAX)),
                },
            );
        }
    }
}

pub(crate) fn end_turn(variant: &TransitionVariant, state: &mut MemberTransitionState) {
    let existing_keys: Vec<String> = state.active.keys().cloned().collect();
    let effects: Vec<ActionEffect> = existing_keys
        .iter()
        .flat_map(|key| {
            condition(variant, key)
                .into_iter()
                .flat_map(|value| value.turn_end_effects.clone())
        })
        .collect();
    for effect in effects {
        apply_effect(variant, state, &effect);
    }
    for key in existing_keys {
        let Some(active) = state.active.get_mut(&key) else {
            continue;
        };
        if let Some(turns) = active.remaining_turns.as_mut() {
            *turns = turns.saturating_sub(1);
            if *turns == 0 {
                state.active.remove(&key);
            }
        }
    }
}

pub(crate) fn resolve_metadata(
    variant: &TransitionVariant,
    state: &MemberTransitionState,
    preview_uncertain: bool,
) -> ResolvedBattleMetadata {
    let mut skills: [Option<SkillForm>; 3] = [None, None, None];
    for (index, skill_id) in variant.base_skill_ids.iter().enumerate() {
        skills[index] = skill_id.and_then(|id| {
            variant
                .skill_forms
                .iter()
                .find(|form| form.id == id)
                .cloned()
        });
    }
    let mut noble_phantasm = variant
        .base_noble_phantasm_id
        .and_then(|id| {
            variant
                .noble_phantasm_forms
                .iter()
                .find(|form| form.id == id)
                .cloned()
        })
        .or_else(|| variant.noble_phantasm_forms.first().cloned());
    for (state_key, active) in &state.active {
        let Some(definition) = condition(variant, state_key) else {
            continue;
        };
        for effect in &definition.effects {
            match effect {
                ConditionEffect::ReplaceSkill {
                    slot,
                    skill_id,
                    min_stacks,
                } if active.stacks >= min_stacks.unwrap_or(1) => {
                    if let Some(form) = variant.skill_forms.iter().find(|form| form.id == *skill_id)
                    {
                        skills[*slot as usize - 1] = Some(form.clone());
                    }
                }
                ConditionEffect::ReplaceNoblePhantasm {
                    noble_phantasm_id,
                    min_stacks,
                } if active.stacks >= min_stacks.unwrap_or(1) => {
                    noble_phantasm = variant
                        .noble_phantasm_forms
                        .iter()
                        .find(|form| form.id == *noble_phantasm_id)
                        .cloned();
                }
                _ => {}
            }
        }
    }
    let active_conditions = state
        .active
        .iter()
        .filter_map(|(state_key, active)| {
            let definition = condition(variant, state_key)?;
            Some(ResolvedCondition {
                state_key: state_key.clone(),
                name: definition.name.clone(),
                remaining_turns: active.remaining_turns,
                stacks: active.stacks,
                external: definition.external,
            })
        })
        .collect();
    ResolvedBattleMetadata {
        skills,
        noble_phantasm,
        available_conditions: variant
            .conditions
            .iter()
            .map(|condition| AvailableCondition {
                state_key: condition.state_key.clone(),
                name: condition.name.clone(),
                duration_turns: condition.duration_turns,
                max_stacks: condition.max_stacks,
                external: condition.external,
            })
            .collect(),
        active_conditions,
        cooldown_adjustments: state.cooldown_adjustments.clone(),
        preview_uncertain,
    }
}

pub(crate) fn resolve_default_metadata(
    server: Server,
    servant_id: u32,
    variant_key: &str,
) -> Option<ResolvedBattleMetadata> {
    transition_variant(server, servant_id, variant_key)
        .map(|variant| resolve_metadata(variant, &MemberTransitionState::default(), false))
}

#[tauri::command]
pub(crate) fn resolve_battle_metadata(
    app: tauri::AppHandle,
    servant_id: u32,
    variant_key: String,
    #[allow(unused_variables)] server_state: tauri::State<'_, Mutex<Server>>,
    events: Vec<PreviewEvent>,
) -> Result<Option<ResolvedBattleMetadata>, String> {
    let server = *server_state
        .lock()
        .map_err(|_| "服务器设置锁已损坏".to_string())?;
    let Some(variant) = transition_variant(server, servant_id, &variant_key) else {
        return Ok(None);
    };
    let mut state = MemberTransitionState::default();
    let mut uncertain = false;
    for event in events {
        match event {
            PreviewEvent::Skill {
                slot,
                selection_index,
            } => {
                let resolved = resolve_metadata(variant, &state, false);
                if let Some(skill) = resolved
                    .skills
                    .get(slot.saturating_sub(1) as usize)
                    .and_then(Clone::clone)
                {
                    apply_action(
                        variant,
                        &mut state,
                        ActionType::Skill,
                        skill.id,
                        selection_index,
                    );
                }
            }
            PreviewEvent::NoblePhantasm => {
                if let Some(np) = resolve_metadata(variant, &state, false).noble_phantasm {
                    apply_action(variant, &mut state, ActionType::NoblePhantasm, np.id, None);
                }
            }
            PreviewEvent::TurnEnd => end_turn(variant, &mut state),
            PreviewEvent::Override {
                state_key,
                mode,
                remaining_turns,
                stacks,
            } => apply_override(
                variant,
                &mut state,
                &state_key,
                mode,
                remaining_turns,
                stacks,
            ),
            PreviewEvent::Uncertain => uncertain = true,
        }
    }
    let mut resolved = resolve_metadata(variant, &state, uncertain);
    let icons_dir = app_assets_dir(&app).join("icons");
    for skill in resolved.skills.iter_mut().flatten() {
        skill.icon = skill
            .icon
            .as_ref()
            .map(|filename| icons_dir.join(filename))
            .filter(|path| path.is_file())
            .map(|path| path.to_string_lossy().into_owned());
    }
    Ok(Some(resolved))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mash_np_changes_skill_and_np_for_two_following_turns() {
        let variant = transition_variant(Server::Jp, 1, "1:3").unwrap();
        let mut state = MemberTransitionState::default();
        apply_action(variant, &mut state, ActionType::NoblePhantasm, 800107, None);
        let active = resolve_metadata(variant, &state, false);
        assert_eq!(
            active.skills[1].as_ref().map(|skill| skill.id),
            Some(2477450)
        );
        assert_eq!(
            active
                .noble_phantasm
                .as_ref()
                .and_then(|np| np.card.as_deref()),
            Some("buster")
        );
        end_turn(variant, &mut state);
        assert_eq!(
            resolve_metadata(variant, &state, false).active_conditions[0].remaining_turns,
            Some(2)
        );
        end_turn(variant, &mut state);
        assert_eq!(
            resolve_metadata(variant, &state, false).skills[1]
                .as_ref()
                .map(|skill| skill.id),
            Some(2477450)
        );
        end_turn(variant, &mut state);
        assert_ne!(
            resolve_metadata(variant, &state, false).skills[1]
                .as_ref()
                .map(|skill| skill.id),
            Some(2477450)
        );
    }

    #[test]
    fn emiya_selection_uses_target_np_card() {
        let variant = transition_variant(Server::Jp, 11, "11:1").unwrap();
        let mut state = MemberTransitionState::default();
        apply_action(variant, &mut state, ActionType::Skill, 754650, Some(1));
        assert_eq!(
            resolve_metadata(variant, &state, false)
                .noble_phantasm
                .unwrap()
                .card
                .as_deref(),
            Some("arts")
        );
        end_turn(variant, &mut state);
        assert_eq!(
            resolve_metadata(variant, &state, false)
                .noble_phantasm
                .unwrap()
                .card
                .as_deref(),
            Some("buster")
        );
    }

    #[test]
    fn kiara_scheduled_rank_up_stacks_and_consumes() {
        let variant = transition_variant(Server::Jp, 285, "285:1").unwrap();
        let mut state = MemberTransitionState::default();
        apply_action(variant, &mut state, ActionType::Skill, 767650, None);
        end_turn(variant, &mut state);
        assert_eq!(
            resolve_metadata(variant, &state, false).skills[1]
                .as_ref()
                .map(|skill| skill.id),
            Some(964647)
        );
        end_turn(variant, &mut state);
        assert_eq!(
            resolve_metadata(variant, &state, false).skills[1]
                .as_ref()
                .map(|skill| skill.id),
            Some(964648)
        );
        apply_action(variant, &mut state, ActionType::Skill, 964648, None);
        assert_eq!(
            resolve_metadata(variant, &state, false).skills[1]
                .as_ref()
                .map(|skill| skill.id),
            Some(682450)
        );
    }

    #[test]
    fn external_condition_can_be_set_and_cleared() {
        let variant = transition_variant(Server::Jp, 417, "417:1").unwrap();
        let mut state = MemberTransitionState::default();
        apply_override(
            variant,
            &mut state,
            "battlePoint:3300200:7",
            BattleStateOverrideMode::Set,
            None,
            None,
        );
        assert_eq!(
            resolve_metadata(variant, &state, false)
                .noble_phantasm
                .unwrap()
                .id,
            3300298
        );
        apply_override(
            variant,
            &mut state,
            "battlePoint:3300200:7",
            BattleStateOverrideMode::Clear,
            None,
            None,
        );
        assert_eq!(
            resolve_metadata(variant, &state, false)
                .noble_phantasm
                .unwrap()
                .id,
            3300201
        );
    }
}
