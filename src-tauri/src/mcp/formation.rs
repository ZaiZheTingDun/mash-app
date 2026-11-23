//! Compile a slot-based MCP draft into the ordinary team and command models.

use crate::commands::{catalog, projects};
use crate::runner::{
    apply_completed_turn_lineup_changes, apply_party_member_lineup_change, build_party_members,
    resolve_available_member_action, PartyMemberRuntime, RunConfig,
};
use crate::{
    Action, AttackCard, BattleScene, BattleTurn, OrderChangeSelection, Project, ProjectSlot,
    SkillSelection,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tauri::AppHandle;

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct FormationDraft {
    pub name: String,
    /// Owned servants in their initial party slots (1–6).
    #[serde(default)]
    pub servants: Vec<DraftServant>,
    /// The default support slot is 3. An unpinned support is allowed.
    pub support: Option<DraftSupport>,
    pub mystic_code_id: Option<u32>,
    /// One array of turns per wave, in wave order.
    pub waves: Vec<Vec<DraftTurn>>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct DraftServant {
    pub slot: u32,
    pub servant_id: u32,
    pub variant_key: Option<String>,
    pub craft_essence_id: Option<u32>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct DraftSupport {
    /// Initial support slot (1–6). A servant ID is needed to use or target its skills.
    pub slot: Option<u32>,
    pub servant_id: Option<u32>,
    pub variant_key: Option<String>,
    #[serde(default)]
    pub craft_essence_ids: Vec<u32>,
    pub require_max_limit_break: Option<bool>,
    pub minimum_level: Option<u32>,
    pub minimum_np_level: Option<u32>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct DraftTurn {
    /// Actions are issued in the supplied order.
    #[serde(default)]
    pub actions: Vec<DraftAction>,
    /// Up to three ordered priorities. Empty positions use Mash's normal fallback.
    #[serde(default)]
    pub attack_priority: Vec<DraftAttack>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(super) enum DraftAction {
    ServantSkill {
        /// Front-line slot (1–3) after earlier scripted order changes.
        slot: u32,
        skill: u32,
        target_slot: Option<u32>,
        selection_index: Option<u32>,
    },
    MysticCodeSkill {
        skill: u32,
        target_slot: Option<u32>,
        /// Supply the two slots for an order change skill.
        order_change: Option<DraftOrderChange>,
    },
    EnemyTarget {
        slot: u32,
    },
}

#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct DraftOrderChange {
    /// Current front-line slot (1–3).
    front_slot: u32,
    /// Current back-line slot (4–6).
    back_slot: u32,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct DraftAttack {
    /// Front-line slot after the turn's scripted order changes.
    pub slot: u32,
    pub card: Card,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(super) enum Card {
    Np,
    Buster,
    Arts,
    Quick,
}

impl Card {
    fn key(&self) -> &'static str {
        match self {
            Self::Np => "np",
            Self::Buster => "buster",
            Self::Arts => "arts",
            Self::Quick => "quick",
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ValidatedFormation {
    pub config: projects::ConfigExportEntry,
    pub warnings: Vec<String>,
}

pub(super) fn validate(
    app: &AppHandle,
    draft: FormationDraft,
) -> Result<ValidatedFormation, String> {
    if draft.name.trim().is_empty() {
        return Err("请输入编队名称".into());
    }
    if draft.waves.is_empty() || draft.waves.iter().any(Vec::is_empty) {
        return Err("请至少提供一面，且每面至少包含一个回合".into());
    }

    let line_up = build_lineup(&draft)?;
    let mut compiler = Compiler {
        app,
        party: DraftParty::new(&line_up),
        warnings: Vec::new(),
    };
    let scenes = draft
        .waves
        .iter()
        .enumerate()
        .map(|(wave, turns)| {
            let turns = turns
                .iter()
                .enumerate()
                .map(|(turn, draft)| compiler.compile_turn(draft, wave + 1, turn + 1))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(BattleScene {
                id: new_id(),
                turns,
                preparation_actions: Vec::new(),
                servant_actions: Vec::new(),
                equipment_actions: Vec::new(),
                command_spell_actions: Vec::new(),
                enemy_target: None,
                attack_priority: Vec::new(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let warnings = compiler.warnings;
    Ok(ValidatedFormation {
        config: projects::ConfigExportEntry {
            project: line_up,
            battle_scenes: scenes,
            advanced_battle_scenes: Vec::new(),
        },
        warnings,
    })
}

fn build_lineup(draft: &FormationDraft) -> Result<Project, String> {
    let mut project =
        projects::new_project(draft.name.trim().to_owned(), false, Default::default());
    let support_slot = draft
        .support
        .as_ref()
        .and_then(|support| support.slot)
        .unwrap_or(3);
    let support_index = position(support_slot, 6)?;
    project.slots.swap(2, support_index);
    let mut occupied = HashSet::from([support_slot]);

    let find_variant = |id: u32, variant: Option<&str>| -> Result<String, String> {
        let entries = catalog::selectable_servants_data()?;
        entries
            .iter()
            .find(|entry| entry.id == id && variant.is_none_or(|key| entry.variant_key == key))
            .map(|entry| entry.variant_key.clone())
            .ok_or_else(|| format!("目录中没有对应的从者或形态（{id}，{variant:?}）"))
    };
    let validate_essence = |id: u32| -> Result<(), String> {
        if catalog::craft_essences_data()?
            .iter()
            .any(|entry| entry.id == id)
        {
            Ok(())
        } else {
            Err(format!("目录中没有礼装 {id}"))
        }
    };
    for servant in &draft.servants {
        let index = position(servant.slot, 6)?;
        if !occupied.insert(servant.slot) {
            return Err(format!("编队槽位 {} 重复占用", servant.slot));
        }
        let slot = &mut project.slots[index];
        slot.servant_id = Some(servant.servant_id);
        slot.servant_variant_key = Some(find_variant(
            servant.servant_id,
            servant.variant_key.as_deref(),
        )?);
        if let Some(id) = servant.craft_essence_id {
            validate_essence(id)?;
            slot.craft_essence_id = Some(id);
            slot.craft_essence_ids = vec![id];
        }
    }
    if let Some(support) = &draft.support {
        match support.servant_id {
            Some(id) => {
                project.support_servant_id = Some(id);
                project.support_servant_variant_key =
                    Some(find_variant(id, support.variant_key.as_deref())?);
            }
            None if support.variant_key.is_some() => return Err("设置助战形态时需要从者 ID".into()),
            None => {}
        }
        if support
            .minimum_level
            .is_some_and(|level| !(1..=120).contains(&level))
            || support
                .minimum_np_level
                .is_some_and(|level| !(1..=5).contains(&level))
        {
            return Err("助战等级范围为 1–120，宝具等级范围为 1–5".into());
        }
        project.support_servant_level_min = support.minimum_level;
        project.support_noble_phantasm_level_min = support.minimum_np_level;
        let mut ids = Vec::new();
        for &id in &support.craft_essence_ids {
            if !ids.contains(&id) {
                validate_essence(id)?;
                ids.push(id);
            }
        }
        if ids.len() > 10 {
            return Err("普通助战最多可配置 10 张不同礼装".into());
        }
        let slot = &mut project.slots[support_index];
        slot.craft_essence_mlb_required = support.require_max_limit_break.unwrap_or(true);
        slot.craft_essence_multi_select = ids.len() > 1;
        slot.craft_essence_id = ids.first().copied();
        slot.craft_essence_ids = ids;
    }
    project.mystic_code_id = draft.mystic_code_id;
    Ok(project)
}

struct Compiler<'a> {
    app: &'a AppHandle,
    party: DraftParty<'a>,
    warnings: Vec<String>,
}

impl Compiler<'_> {
    fn compile_turn(
        &mut self,
        draft: &DraftTurn,
        wave: usize,
        turn: usize,
    ) -> Result<BattleTurn, String> {
        let location = format!("第 {wave} 面第 {turn} 回合");
        let preparation_actions = draft
            .actions
            .iter()
            .enumerate()
            .map(|(index, action)| {
                let location = format!("{location}第 {} 项行动", index + 1);
                let result = self.compile_action(action, &location).and_then(|action| {
                    self.party.apply_action(&action)?;
                    Ok(action)
                });
                result.map_err(|error| format!("{location}：{error}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if draft.attack_priority.len() > 3 {
            return Err(format!("{location}：最多配置 3 项出卡优先级"));
        }
        let mut attack_priority = draft
            .attack_priority
            .iter()
            .enumerate()
            .map(|(index, attack)| {
                self.party
                    .attack_card(attack)
                    .map_err(|error| format!("{location}第 {} 项出卡：{error}", index + 1))
            })
            .collect::<Result<Vec<_>, String>>()?;
        attack_priority.resize_with(3, || AttackCard {
            id: new_id(),
            card: None,
            member_id: None,
            servant_id: None,
            is_support: false,
        });
        let turn = BattleTurn {
            id: new_id(),
            preparation_actions,
            servant_actions: Vec::new(),
            equipment_actions: Vec::new(),
            command_spell_actions: Vec::new(),
            enemy_target: None,
            attack_priority,
            attack_mode: Default::default(),
            critical_strategy: Default::default(),
            advanced_card_strategy: Default::default(),
        };
        self.party.finish_turn(&turn);
        Ok(turn)
    }

    fn compile_action(&mut self, action: &DraftAction, location: &str) -> Result<Action, String> {
        match *action {
            DraftAction::EnemyTarget { slot } => {
                position(slot, 6)?;
                Ok(Action::EnemyTarget {
                    id: new_id(),
                    target: Some(format!("enemy_{slot}")),
                })
            }
            DraftAction::ServantSkill {
                slot,
                skill,
                target_slot,
                selection_index,
            } => {
                position(skill, 3)?;
                let source = self.party.skill_member(slot)?;
                let source_id = source.servant_id;
                let source_variant = source.variant.map(str::to_owned);
                let servant_member_id = Some(source.slot.id.clone());
                let servant_is_support = source.is_support();
                let target = target_slot
                    .map(|slot| self.party.skill_member(slot))
                    .transpose()?;
                let target_member_id = target.as_ref().map(|member| member.slot.id.clone());
                let target_servant_id = target.as_ref().and_then(|member| member.servant_id);
                let target_is_support = target.as_ref().is_some_and(Member::is_support);
                let skill_selection = self.servant_skill(
                    source_id.zip(source_variant.as_deref()),
                    skill,
                    target_slot.is_some(),
                    selection_index,
                    location,
                )?;
                Ok(Action::Servant {
                    id: new_id(),
                    servant: slot_key(Some(slot)),
                    servant_member_id,
                    servant_id: source_id,
                    servant_is_support,
                    skill: Some(format!("skill_{skill}")),
                    skill_form_id: None,
                    skill_selection,
                    target: slot_key(target_slot),
                    target_member_id,
                    target_servant_id,
                    target_is_support,
                })
            }
            DraftAction::MysticCodeSkill {
                skill,
                target_slot,
                order_change,
            } => {
                position(skill, 3)?;
                let code_id = self
                    .party
                    .project
                    .mystic_code_id
                    .ok_or("请先指定御主礼装")?;
                use catalog::MysticCodeSkillMode::Unknown;
                let mode = match catalog::get_mystic_codes(self.app.clone()) {
                    Ok(codes) => {
                        let code = codes
                            .iter()
                            .find(|code| code.id == code_id)
                            .ok_or_else(|| format!("目录中没有御主礼装 {code_id}"))?;
                        let ability = code
                            .skills
                            .iter()
                            .find(|entry| entry.slot == skill)
                            .ok_or("该御主礼装没有此技能")?;
                        if ability.targeting_mode == Unknown {
                            self.warnings
                                .push(format!("{location}：无法确认御主技能目标要求"));
                        }
                        ability.targeting_mode
                    }
                    Err(_) => {
                        self.warnings
                            .push(format!("{location}：御主礼装技能资料不可用"));
                        Unknown
                    }
                };
                validate_mystic_code_target(mode, target_slot.is_some(), order_change.is_some())?;
                self.party.equipment_action(skill, target_slot, order_change)
            }
        }
    }

    fn servant_skill(
        &mut self,
        servant: Option<(u32, &str)>,
        slot: u32,
        has_target: bool,
        selection_index: Option<u32>,
        location: &str,
    ) -> Result<Option<SkillSelection>, String> {
        let Some((id, variant_key)) = servant else {
            if selection_index.is_some() {
                return Err("指定技能选项时需要从者形态".into());
            }
            self.warnings
                .push(format!("{location}：无法确认从者形态，无法校验技能"));
            return Ok(None);
        };
        use catalog::SkillTargetingMode::*;
        let targeting =
            catalog::get_servant_skill_targeting(self.app.clone(), id, variant_key.into())
                .ok()
                .and_then(|entries| entries.into_iter().find(|entry| entry.skill_num == slot));
        match targeting.map(|entry| entry.targeting_mode) {
            Some(NeedsTarget) => validate_target(true, has_target)?,
            Some(NoTarget) => validate_target(false, has_target)?,
            Some(Mixed) => self.warnings.push(format!(
                "{location}：该技能有多种目标规则，请核对游戏内形态"
            )),
            _ => self
                .warnings
                .push(format!("{location}：无法确认技能目标要求")),
        }
        let selections =
            catalog::get_servant_skill_selection(self.app.clone(), id, variant_key.into());
        let selection = match selections {
            Ok(entries) => entries.into_iter().find(|entry| entry.skill_num == slot),
            Err(_) if selection_index.is_some() => {
                return Err("技能选项资料不可用，无法生成选项指令".into())
            }
            Err(_) => None,
        };
        match (selection, selection_index) {
            (None, None) => Ok(None),
            (Some(_), None) => Err("技能需要指定 selectionIndex 选项".into()),
            (None, Some(_)) => Err("该技能没有可配置选项".into()),
            (Some(entry), Some(index)) => {
                let option = entry
                    .options
                    .iter()
                    .find(|option| option.index == index)
                    .ok_or("技能选项索引超出范围")?;
                Ok(Some(SkillSelection {
                    selection_type: entry.selection_type,
                    index,
                    option_count: Some(entry.options.len() as u32),
                    label: Some(option.label.clone()),
                }))
            }
        }
    }
}

struct DraftParty<'a> {
    project: &'a Project,
    original_members: [Option<PartyMemberRuntime>; 6],
    members: [Option<PartyMemberRuntime>; 6],
    np_use_counts: HashMap<PartyMemberRuntime, u32>,
}

impl<'a> DraftParty<'a> {
    fn new(project: &'a Project) -> Self {
        let members = build_party_members(&RunConfig::from_project(project));
        Self {
            project,
            original_members: members.clone(),
            members,
            np_use_counts: HashMap::new(),
        }
    }

    fn apply_action(&mut self, action: &Action) -> Result<(), String> {
        let resolved = resolve_available_member_action(&self.members, &self.original_members, action)
            .ok_or("行动的从者不在可用位置")?;
        apply_party_member_lineup_change(&mut self.members, &resolved);
        Ok(())
    }

    fn finish_turn(&mut self, turn: &BattleTurn) {
        apply_completed_turn_lineup_changes(
            &mut self.members,
            &self.original_members,
            turn,
            &mut self.np_use_counts,
        );
    }

    fn attack_card(&self, attack: &DraftAttack) -> Result<AttackCard, String> {
        let member = self.front_member(attack.slot)?;
        Ok(AttackCard {
            id: new_id(),
            card: Some(format!("servant_{}_{}", attack.slot, attack.card.key())),
            member_id: Some(member.slot.id.clone()),
            servant_id: member.servant_id,
            is_support: member.is_support(),
        })
    }

    fn equipment_action(
        &self,
        skill: u32,
        target_slot: Option<u32>,
        order_change: Option<DraftOrderChange>,
    ) -> Result<Action, String> {
        let order_change = order_change.map(|swap| self.swap(swap)).transpose()?;
        let target = target_slot
            .map(|slot| self.skill_member(slot))
            .transpose()?;
        Ok(Action::Equipment {
            id: new_id(),
            skill: Some(format!("skill_{skill}")),
            target: slot_key(target_slot),
            target_member_id: target.as_ref().map(|member| member.slot.id.clone()),
            target_servant_id: target.as_ref().and_then(|member| member.servant_id),
            target_is_support: target.as_ref().is_some_and(Member::is_support),
            order_change,
        })
    }

    fn swap(&self, swap: DraftOrderChange) -> Result<OrderChangeSelection, String> {
        if !(4..=6).contains(&swap.back_slot) {
            return Err("换人的 backSlot 必须是后排槽位 4–6".into());
        }
        let front_index =
            position(swap.front_slot, 3).map_err(|_| "换人的 frontSlot 必须是前排槽位 1–3")?;
        let back_index = (swap.back_slot - 1) as usize;
        let front = self.member_at(front_index)?;
        let back = self.member_at(back_index)?;
        Ok(OrderChangeSelection {
            front: slot_key(Some(swap.front_slot)),
            front_member_id: Some(front.slot.id.clone()),
            front_servant_id: front.servant_id,
            front_is_support: front.is_support(),
            back: slot_key(Some(swap.back_slot)),
            back_member_id: Some(back.slot.id.clone()),
            back_servant_id: back.servant_id,
            back_is_support: back.is_support(),
        })
    }

    fn skill_member(&self, slot: u32) -> Result<Member<'_>, String> {
        let member = self.front_member(slot)?;
        if member.servant_id.is_none() {
            return Err(format!("槽位 {slot} 未指定从者"));
        }
        Ok(member)
    }

    fn front_member(&self, slot: u32) -> Result<Member<'_>, String> {
        self.member_at(position(slot, 3)?)
    }

    fn member_at(&self, index: usize) -> Result<Member<'_>, String> {
        let member = self.members[index]
            .as_ref()
            .ok_or_else(|| format!("槽位 {} 没有从者", index + 1))?;
        let slot = &self.project.slots[member.slot_index];
        let variant = if member.is_support {
            self.project.support_servant_variant_key.as_deref()
        } else {
            slot.servant_variant_key.as_deref()
        };
        Ok(Member {
            slot,
            servant_id: member.servant_id,
            variant,
        })
    }
}

struct Member<'a> {
    slot: &'a ProjectSlot,
    servant_id: Option<u32>,
    variant: Option<&'a str>,
}

impl Member<'_> {
    fn is_support(&self) -> bool {
        self.slot.kind == "support"
    }
}

fn validate_target(required: bool, supplied: bool) -> Result<(), String> {
    match (required, supplied) {
        (true, false) => Err("该技能需要指定 targetSlot".into()),
        (false, true) => Err("该技能无需指定目标".into()),
        _ => Ok(()),
    }
}

fn validate_mystic_code_target(
    mode: catalog::MysticCodeSkillMode,
    has_target: bool,
    has_swap: bool,
) -> Result<(), String> {
    use catalog::MysticCodeSkillMode::*;
    if has_target && has_swap {
        return Err("换人技能请只指定 orderChange".into());
    }
    match mode {
        OrderChange if !has_swap => Err("换人技能需要指定 orderChange 的前排和后排槽位".into()),
        NeedsTarget | NoTarget if has_swap => Err("该御主技能不支持换人".into()),
        NeedsTarget => validate_target(true, has_target),
        NoTarget => validate_target(false, has_target),
        OrderChange | Unknown => Ok(()),
    }
}

fn position(slot: u32, count: u32) -> Result<usize, String> {
    if (1..=count).contains(&slot) {
        Ok((slot - 1) as usize)
    } else {
        Err(format!("槽位或技能编号 {slot} 超出 1–{count}"))
    }
}

fn slot_key(slot: Option<u32>) -> Option<String> {
    slot.map(|slot| format!("servant_{slot}"))
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_numbers_and_target_requirements_reject_confirmed_invalid_values() {
        assert_eq!(position(1, 3), Ok(0));
        assert!(position(0, 3).is_err());
        assert!(position(4, 3).is_err());
        assert!(validate_target(true, false).is_err());
        assert!(validate_target(false, true).is_err());
        assert!(validate_target(false, false).is_ok());
    }

    #[test]
    fn unpinned_support_occupies_every_configurable_slot() {
        for slot in 1..=6 {
            let draft: FormationDraft = serde_json::from_value(serde_json::json!({
                "name": "编队", "support": { "slot": slot }, "waves": [[{}]]
            }))
            .unwrap();
            let project = build_lineup(&draft).unwrap();
            let party = DraftParty::new(&project);
            let member = party.member_at((slot - 1) as usize).unwrap();
            assert!(member.is_support());
            assert!(member.servant_id.is_none());
            for front in 1..=3 {
                assert!(party.skill_member(front).is_err());
            }
        }
    }

    #[test]
    fn shared_lineup_replay_uses_the_reserve_after_miss_cranes_np() {
        let mut project = projects::new_project("退场".into(), false, Default::default());
        project.slots.swap(2, 5);
        project.slots[0].servant_id = Some(307);
        project.slots[1].servant_id = Some(16);
        project.slots[3].servant_id = Some(9);
        let mut party = DraftParty::new(&project);
        let mut turn: BattleTurn = serde_json::from_value(serde_json::json!({
            "id": "turn-1", "attackPriority": [], "preparationActions": []
        }))
        .unwrap();
        turn.attack_priority.push(
            party.attack_card(&DraftAttack { slot: 1, card: Card::Np }).unwrap()
        );
        party.finish_turn(&turn);

        let action = party.equipment_action(1, Some(1), None).unwrap();
        party.apply_action(&action).unwrap();
        assert!(matches!(action, Action::Equipment {
            target_member_id: Some(ref id),
            target_servant_id: Some(9),
            ..
        } if id == &project.slots[3].id));
    }

    #[test]
    fn unpinned_support_can_swap_without_becoming_a_skill_target() {
        let mut project = projects::new_project("换人".into(), false, Default::default());
        project.slots.swap(2, 4);
        project.slots[0].servant_id = Some(16);
        let mut party = DraftParty::new(&project);
        let swap = party.equipment_action(3, None, Some(DraftOrderChange {
            front_slot: 1,
            back_slot: 5,
        })).unwrap();
        party.apply_action(&swap).unwrap();
        assert!(party.front_member(1).unwrap().is_support());
        assert!(party.equipment_action(1, Some(1), None).is_err());
    }

    #[test]
    fn order_change_keeps_member_identity_for_subsequent_actions_and_attacks() {
        use catalog::MysticCodeSkillMode::OrderChange;
        let action: DraftAction = serde_json::from_value(serde_json::json!({
            "type": "mysticCodeSkill", "skill": 3,
            "orderChange": {"frontSlot": 1, "backSlot": 4}
        }))
        .unwrap();
        let DraftAction::MysticCodeSkill {
            skill,
            target_slot,
            order_change,
        } = action
        else {
            panic!("expected a mystic code skill");
        };
        assert_eq!(
            validate_mystic_code_target(OrderChange, target_slot.is_some(), order_change.is_some()),
            Ok(())
        );

        let mut project = projects::new_project("换人".into(), false, Default::default());
        project.slots.swap(0, 2);
        project.support_servant_id = Some(16);
        project.slots[3].servant_id = Some(16);
        let mut party = DraftParty::new(&project);

        let swap = party
            .equipment_action(skill, target_slot, order_change)
            .unwrap();
        party.apply_action(&swap).unwrap();
        let swap = serde_json::to_value(swap).unwrap();
        assert_eq!(swap["skill"], "skill_3");
        assert!(swap["target"].is_null());
        assert_eq!(swap["orderChange"]["front"], "servant_1");
        assert_eq!(swap["orderChange"]["frontMemberId"], project.slots[0].id);
        assert_eq!(swap["orderChange"]["frontIsSupport"], true);
        assert_eq!(swap["orderChange"]["back"], "servant_4");
        assert_eq!(swap["orderChange"]["backMemberId"], project.slots[3].id);
        assert_eq!(swap["orderChange"]["backIsSupport"], false);

        let buff = party.equipment_action(1, Some(1), None).unwrap();
        let buff = serde_json::to_value(buff).unwrap();
        assert_eq!(buff["targetMemberId"], project.slots[3].id);
        assert_eq!(buff["targetIsSupport"], false);

        let attack = party
            .attack_card(&DraftAttack {
                slot: 1,
                card: Card::Np,
            })
            .unwrap();
        assert_eq!(attack.card.as_deref(), Some("servant_1_np"));
        assert_eq!(
            attack.member_id.as_deref(),
            Some(project.slots[3].id.as_str())
        );
        assert!(!attack.is_support);
    }

    #[test]
    fn order_change_requires_two_occupied_slots_in_the_correct_rows() {
        use catalog::MysticCodeSkillMode::{NoTarget, OrderChange};
        assert!(validate_mystic_code_target(OrderChange, false, false).is_err());
        assert!(validate_mystic_code_target(OrderChange, true, true).is_err());
        assert!(validate_mystic_code_target(NoTarget, false, true).is_err());

        let project = projects::new_project("换人".into(), false, Default::default());
        let party = DraftParty::new(&project);
        for (front_slot, back_slot) in [(0, 4), (4, 5), (1, 3), (3, 7), (3, 4)] {
            assert!(party
                .swap(DraftOrderChange {
                    front_slot,
                    back_slot
                })
                .is_err());
        }
    }
}
