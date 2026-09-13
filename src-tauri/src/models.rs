//! IPC wire models shared by Tauri commands, persisted project JSON, and runners.
//! Keep serde field names camelCase-compatible with the TypeScript interfaces.

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillSelection {
    #[serde(rename = "type")]
    pub selection_type: String,
    pub index: u32,
    #[serde(rename = "optionCount", default)]
    pub option_count: Option<u32>,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum Action {
    #[serde(rename = "servant")]
    Servant {
        id: String,
        servant: Option<String>,
        #[serde(rename = "servantMemberId", default)]
        servant_member_id: Option<String>,
        #[serde(rename = "servantId", default)]
        servant_id: Option<u32>,
        #[serde(rename = "servantIsSupport", default)]
        servant_is_support: bool,
        skill: Option<String>,
        #[serde(rename = "skillSelection", default)]
        skill_selection: Option<SkillSelection>,
        target: Option<String>,
        #[serde(rename = "targetMemberId", default)]
        target_member_id: Option<String>,
        #[serde(rename = "targetServantId", default)]
        target_servant_id: Option<u32>,
        #[serde(rename = "targetIsSupport", default)]
        target_is_support: bool,
    },
    #[serde(rename = "equipment")]
    Equipment {
        id: String,
        skill: Option<String>,
        #[serde(default)]
        target: Option<String>,
        #[serde(rename = "targetMemberId", default)]
        target_member_id: Option<String>,
        #[serde(rename = "targetServantId", default)]
        target_servant_id: Option<u32>,
        #[serde(rename = "targetIsSupport", default)]
        target_is_support: bool,
        #[serde(rename = "orderChange", default)]
        order_change: Option<OrderChangeSelection>,
    },
    #[serde(rename = "commandSpell")]
    CommandSpell {
        id: String,
        spell: Option<String>,
        #[serde(default)]
        target: Option<String>,
        #[serde(rename = "targetMemberId", default)]
        target_member_id: Option<String>,
        #[serde(rename = "targetServantId", default)]
        target_servant_id: Option<u32>,
        #[serde(rename = "targetIsSupport", default)]
        target_is_support: bool,
    },
    #[serde(rename = "enemyTarget")]
    EnemyTarget { id: String, target: Option<String> },
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct OrderChangeSelection {
    pub front: Option<String>,
    #[serde(rename = "frontMemberId", default)]
    pub front_member_id: Option<String>,
    #[serde(rename = "frontServantId", default)]
    pub front_servant_id: Option<u32>,
    #[serde(rename = "frontIsSupport", default)]
    pub front_is_support: bool,
    pub back: Option<String>,
    #[serde(rename = "backMemberId", default)]
    pub back_member_id: Option<String>,
    #[serde(rename = "backServantId", default)]
    pub back_servant_id: Option<u32>,
    #[serde(rename = "backIsSupport", default)]
    pub back_is_support: bool,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct AttackCard {
    pub id: String,
    pub card: Option<String>,
    #[serde(rename = "memberId", default)]
    pub member_id: Option<String>,
    #[serde(rename = "servantId", default)]
    pub servant_id: Option<u32>,
    #[serde(rename = "isSupport", default)]
    pub is_support: bool,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AttackMode {
    #[default]
    Normal,
    Critical,
    Advanced,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CriticalChainType {
    Mighty,
    Buster,
    Arts,
    Quick,
}

pub(crate) fn default_critical_chain_priority() -> Vec<CriticalChainType> {
    vec![
        CriticalChainType::Mighty,
        CriticalChainType::Buster,
        CriticalChainType::Arts,
        CriticalChainType::Quick,
    ]
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AttackMemberPriorityItem {
    #[serde(default)]
    pub member_id: Option<String>,
    pub slot_index: u32,
    #[serde(default)]
    pub servant_id: Option<u32>,
    #[serde(default)]
    pub is_support: bool,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CriticalAttackStrategy {
    #[serde(default)]
    pub member_priority: Vec<AttackMemberPriorityItem>,
    #[serde(default = "default_critical_chain_priority")]
    pub chain_priority: Vec<CriticalChainType>,
}

impl Default for CriticalAttackStrategy {
    fn default() -> Self {
        Self {
            member_priority: Vec::new(),
            chain_priority: default_critical_chain_priority(),
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct AdvancedCardStrategy {
    #[serde(default)]
    pub custom_rules: Vec<GrandCardRuleConfig>,
}

/// One configured turn inside a battle scene. The runner selects a
/// `BattleScene` from the `BATTLE m/n` HUD and then uses an internal
/// per-scene turn counter to choose which `BattleTurn` runs.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct BattleTurn {
    pub id: String,
    #[serde(rename = "preparationActions", default)]
    pub preparation_actions: Vec<Action>,
    #[serde(rename = "servantActions", default, skip_serializing)]
    pub servant_actions: Vec<Action>,
    #[serde(rename = "equipmentActions", default, skip_serializing)]
    pub equipment_actions: Vec<Action>,
    /// Per-scene Command Spell taps (令咒). Optional for backwards
    /// compatibility: legacy `battle_scenes.json` files written before
    /// this field was added deserialize with an empty list.
    #[serde(rename = "commandSpellActions", default, skip_serializing)]
    pub command_spell_actions: Vec<Action>,
    #[serde(rename = "enemyTarget", default)]
    pub enemy_target: Option<String>,
    #[serde(rename = "attackPriority", default)]
    pub attack_priority: Vec<AttackCard>,
    #[serde(rename = "attackMode", default)]
    pub attack_mode: AttackMode,
    #[serde(rename = "criticalStrategy", default)]
    pub critical_strategy: CriticalAttackStrategy,
    #[serde(rename = "advancedCardStrategy", default)]
    pub advanced_card_strategy: AdvancedCardStrategy,
}

impl BattleTurn {
    pub(crate) fn normalize_preparation_actions(mut self) -> Self {
        if self.preparation_actions.is_empty() {
            self.preparation_actions
                .extend(self.servant_actions.iter().cloned());
            self.preparation_actions
                .extend(self.equipment_actions.iter().cloned());
            self.preparation_actions
                .extend(self.command_spell_actions.iter().cloned());
        }
        self.servant_actions.clear();
        self.equipment_actions.clear();
        self.command_spell_actions.clear();
        self
    }
}

/// One configured battle-scene block. New saves store all normal-mode
/// per-turn config under `turns`. The legacy top-level fields are kept only
/// for reading old `battle_scenes.json` / `turns.json` files and are skipped
/// when serializing.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct BattleScene {
    pub id: String,
    #[serde(default)]
    pub turns: Vec<BattleTurn>,
    #[serde(rename = "preparationActions", default, skip_serializing)]
    pub preparation_actions: Vec<Action>,
    #[serde(rename = "servantActions", default, skip_serializing)]
    pub servant_actions: Vec<Action>,
    #[serde(rename = "equipmentActions", default, skip_serializing)]
    pub equipment_actions: Vec<Action>,
    #[serde(rename = "commandSpellActions", default, skip_serializing)]
    pub command_spell_actions: Vec<Action>,
    #[serde(rename = "enemyTarget", default, skip_serializing)]
    pub enemy_target: Option<String>,
    #[serde(rename = "attackPriority", default, skip_serializing)]
    pub attack_priority: Vec<AttackCard>,
}

impl BattleScene {
    pub(crate) fn normalize_turns(mut self) -> Self {
        if self.turns.is_empty() {
            self.turns.push(
                BattleTurn {
                    id: format!("{}_turn_1", self.id),
                    preparation_actions: std::mem::take(&mut self.preparation_actions),
                    servant_actions: std::mem::take(&mut self.servant_actions),
                    equipment_actions: std::mem::take(&mut self.equipment_actions),
                    command_spell_actions: std::mem::take(&mut self.command_spell_actions),
                    enemy_target: self.enemy_target.take(),
                    attack_priority: std::mem::take(&mut self.attack_priority),
                    attack_mode: AttackMode::Normal,
                    critical_strategy: CriticalAttackStrategy::default(),
                    advanced_card_strategy: AdvancedCardStrategy::default(),
                }
                .normalize_preparation_actions(),
            );
        } else {
            self.turns = self
                .turns
                .into_iter()
                .map(BattleTurn::normalize_preparation_actions)
                .collect();
            self.preparation_actions.clear();
            self.servant_actions.clear();
            self.equipment_actions.clear();
            self.command_spell_actions.clear();
            self.enemy_target = None;
            self.attack_priority.clear();
        }
        self
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AdvancedNpSlotCondition {
    pub servant: String,
    #[serde(default)]
    pub member_id: Option<String>,
    #[serde(default)]
    pub servant_id: Option<u32>,
    #[serde(default)]
    pub is_support: bool,
    pub ready: bool,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AdvancedNpConditionGroup {
    pub id: String,
    #[serde(default)]
    pub slots: Vec<AdvancedNpSlotCondition>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AdvancedCommandCardCondition {
    pub slot: u32,
    pub servant: String,
    #[serde(default)]
    pub member_id: Option<String>,
    #[serde(default)]
    pub servant_id: Option<u32>,
    #[serde(default)]
    pub is_support: bool,
    pub suit: String,
    #[serde(default)]
    pub min_crit_chance: Option<u32>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AdvancedCommandConditionGroup {
    pub id: String,
    #[serde(default)]
    pub cards: Vec<AdvancedCommandCardCondition>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum AdvancedAction {
    #[serde(rename = "servant")]
    Servant {
        id: String,
        servant: Option<String>,
        #[serde(rename = "servantMemberId", default)]
        servant_member_id: Option<String>,
        #[serde(rename = "servantId", default)]
        servant_id: Option<u32>,
        #[serde(rename = "servantIsSupport", default)]
        servant_is_support: bool,
        skill: Option<String>,
        #[serde(rename = "skillSelection", default)]
        skill_selection: Option<SkillSelection>,
        target: Option<String>,
        #[serde(rename = "targetMemberId", default)]
        target_member_id: Option<String>,
        #[serde(rename = "targetServantId", default)]
        target_servant_id: Option<u32>,
        #[serde(rename = "targetIsSupport", default)]
        target_is_support: bool,
    },
    #[serde(rename = "equipment")]
    Equipment {
        id: String,
        skill: Option<String>,
        #[serde(default)]
        target: Option<String>,
        #[serde(rename = "targetMemberId", default)]
        target_member_id: Option<String>,
        #[serde(rename = "targetServantId", default)]
        target_servant_id: Option<u32>,
        #[serde(rename = "targetIsSupport", default)]
        target_is_support: bool,
        #[serde(rename = "orderChange", default)]
        order_change: Option<OrderChangeSelection>,
    },
    #[serde(rename = "commandSpell")]
    CommandSpell {
        id: String,
        spell: Option<String>,
        #[serde(default)]
        target: Option<String>,
        #[serde(rename = "targetMemberId", default)]
        target_member_id: Option<String>,
        #[serde(rename = "targetServantId", default)]
        target_servant_id: Option<u32>,
        #[serde(rename = "targetIsSupport", default)]
        target_is_support: bool,
    },
    #[serde(rename = "enemyTarget")]
    EnemyTarget { id: String, target: Option<String> },
    #[serde(rename = "attack")]
    Attack {
        id: String,
        card: Option<String>,
        #[serde(rename = "memberId", default)]
        member_id: Option<String>,
        #[serde(rename = "servantId", default)]
        servant_id: Option<u32>,
        #[serde(rename = "isSupport", default)]
        is_support: bool,
    },
}

impl AdvancedAction {
    pub(crate) fn as_preparation_action(&self) -> Option<Action> {
        match self {
            Self::Servant {
                id,
                servant,
                servant_member_id,
                servant_id,
                servant_is_support,
                skill,
                skill_selection,
                target,
                target_member_id,
                target_servant_id,
                target_is_support,
            } => Some(Action::Servant {
                id: id.clone(),
                servant: servant.clone(),
                servant_member_id: servant_member_id.clone(),
                servant_id: *servant_id,
                servant_is_support: *servant_is_support,
                skill: skill.clone(),
                skill_selection: skill_selection.clone(),
                target: target.clone(),
                target_member_id: target_member_id.clone(),
                target_servant_id: *target_servant_id,
                target_is_support: *target_is_support,
            }),
            Self::Equipment {
                id,
                skill,
                target,
                target_member_id,
                target_servant_id,
                target_is_support,
                order_change,
            } => Some(Action::Equipment {
                id: id.clone(),
                skill: skill.clone(),
                target: target.clone(),
                target_member_id: target_member_id.clone(),
                target_servant_id: *target_servant_id,
                target_is_support: *target_is_support,
                order_change: order_change.clone(),
            }),
            Self::CommandSpell {
                id,
                spell,
                target,
                target_member_id,
                target_servant_id,
                target_is_support,
            } => Some(Action::CommandSpell {
                id: id.clone(),
                spell: spell.clone(),
                target: target.clone(),
                target_member_id: target_member_id.clone(),
                target_servant_id: *target_servant_id,
                target_is_support: *target_is_support,
            }),
            Self::EnemyTarget { id, target } => Some(Action::EnemyTarget {
                id: id.clone(),
                target: target.clone(),
            }),
            Self::Attack { .. } => None,
        }
    }

    pub(crate) fn as_attack_card(&self) -> Option<AttackCard> {
        match self {
            Self::Attack {
                id,
                card,
                member_id,
                servant_id,
                is_support,
            } => Some(AttackCard {
                id: id.clone(),
                card: card.clone(),
                member_id: member_id.clone(),
                servant_id: *servant_id,
                is_support: *is_support,
            }),
            _ => None,
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AdvancedRule {
    pub id: String,
    #[serde(default)]
    pub np_condition_groups: Vec<AdvancedNpConditionGroup>,
    #[serde(default)]
    pub command_condition_groups: Vec<AdvancedCommandConditionGroup>,
    #[serde(default)]
    pub actions: Vec<AdvancedAction>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub enum AdvancedOutputType {
    Np,
    Critical,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AdvancedMainOutput {
    #[serde(default)]
    pub member_id: Option<String>,
    #[serde(default)]
    pub servant: Option<String>,
    #[serde(default)]
    pub servant_id: Option<u32>,
    #[serde(default)]
    pub is_support: bool,
    #[serde(default)]
    pub output_type: Option<AdvancedOutputType>,
    #[serde(rename = "npCard", default)]
    pub np_card: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AdvancedBattleTurn {
    pub id: String,
    #[serde(default)]
    pub actions: Vec<Action>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AdvancedBattleScene {
    pub id: String,
    #[serde(default)]
    pub enemy_target: Option<String>,
    #[serde(default)]
    pub main_output: Option<AdvancedMainOutput>,
    #[serde(default)]
    pub grand_auto_order_change: Option<bool>,
    #[serde(default)]
    pub command_conditions: Vec<AdvancedCommandCardCondition>,
    #[serde(default)]
    pub control_actions: Vec<Action>,
    #[serde(default)]
    pub turns: Vec<AdvancedBattleTurn>,
    /// Legacy single-turn field. Empty `turns` treats these as Turn 1.
    #[serde(default)]
    pub startup_actions: Vec<Action>,
    #[serde(default)]
    pub rules: Vec<AdvancedRule>,
}

mod project;
pub use project::*;
