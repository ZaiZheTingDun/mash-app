//! IPC wire models shared by Tauri commands, persisted project JSON, and runners.
//! Keep serde field names camelCase-compatible with the TypeScript interfaces.

use crate::runner::{ApRecoveryItem, ApRecoveryLimits};

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

// ---------------------------------------------------------------------------
// Project system
// ---------------------------------------------------------------------------

/// One cell of the team-builder grid. The frontend stores six of these per
/// project (5 servant slots + 1 support slot) along with their order, so
/// drag-and-drop layouts and chosen servants survive across sessions.
///
/// `kind` is either `"servant"` or `"support"`. For support slots,
/// `servant_id` is ignored — the pinned servant lives on
/// `Project::support_servant_id` (kept separate because the runner reads
/// it through `RunConfig::support_servant_id` and we don't want two
/// sources of truth for the same value).
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSlot {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub servant_id: Option<u32>,
    /// Optional UI variant key for servants with multiple gameplay
    /// variants. The runner still acts on the base servant id; this
    /// keeps the team builder showing the exact variant the user picked.
    #[serde(default)]
    pub servant_variant_key: Option<String>,
    /// Pinned craft-essence id for this slot. Persisted alongside the
    /// servant so each loadout can carry its own equipment plan; for
    /// support slots the runner uses this to verify candidate rows on
    /// the support-select screen, party slots store it for future use.
    #[serde(default)]
    pub craft_essence_id: Option<u32>,
    /// Ordered allow-list used by ordinary support selection. Legacy
    /// projects only carry `craft_essence_id`; normalization migrates that
    /// value into this list. The UI and runner cap it at ten unique ids.
    #[serde(default)]
    pub craft_essence_ids: Vec<u32>,
    #[serde(default)]
    pub craft_essence_multi_select: bool,
    #[serde(default = "default_true")]
    pub craft_essence_mlb_required: bool,
}

fn default_true() -> bool {
    true
}

/// Default 6-slot layout used both when creating a fresh project and when
/// deserializing a legacy `projects.json` that predates the `slots` field.
pub(crate) fn default_project_slots() -> Vec<ProjectSlot> {
    let new_slot = |id: &str, kind: &str| ProjectSlot {
        id: id.into(),
        kind: kind.into(),
        servant_id: None,
        servant_variant_key: None,
        craft_essence_id: None,
        craft_essence_ids: Vec::new(),
        craft_essence_multi_select: false,
        craft_essence_mlb_required: true,
    };
    vec![
        new_slot("slot-0", "servant"),
        new_slot("slot-1", "servant"),
        new_slot("slot-2", "support"),
        new_slot("slot-3", "servant"),
        new_slot("slot-4", "servant"),
        new_slot("slot-5", "servant"),
    ]
}

pub(crate) fn default_support_skill_level_mins() -> [Option<u32>; 3] {
    [None; 3]
}

pub(crate) fn default_support_append_skill_level_mins() -> [Option<u32>; 5] {
    [None; 5]
}

pub(crate) fn default_support_grand_craft_essence_ids() -> [Option<u32>; 3] {
    [None; 3]
}

pub(crate) fn default_support_grand_craft_essence_id_lists() -> [Vec<u32>; 3] {
    std::array::from_fn(|_| Vec::new())
}

pub(crate) fn default_support_grand_craft_essence_mlb_required() -> [bool; 3] {
    [true; 3]
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SupportGrandBondCeMode {
    Any,
    Bond,
    BondNp,
}

impl Default for SupportGrandBondCeMode {
    fn default() -> Self {
        Self::Any
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum GrandClass {
    Saber,
    Lancer,
    Berserker,
    Extra1Fire,
    Extra1Earth,
    Extra2Wind,
    Extra2Water,
}

impl GrandClass {
    pub const ALL: [Self; 7] = [
        Self::Saber,
        Self::Lancer,
        Self::Berserker,
        Self::Extra1Fire,
        Self::Extra1Earth,
        Self::Extra2Wind,
        Self::Extra2Water,
    ];
}

impl Default for GrandClass {
    fn default() -> Self {
        Self::Saber
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub enum ProjectRepeatMode {
    Single,
    Infinite,
    Count,
}

impl Default for ProjectRepeatMode {
    fn default() -> Self {
        Self::Single
    }
}

pub(crate) fn default_grand_np_card() -> String {
    "auto".into()
}

pub(crate) fn default_grand_card_priority() -> String {
    "damage".into()
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum GrandChainPriorityItem {
    MainBraveChain,
    MainReadyNp,
    DeputyBraveChain,
    MainColorChain,
    DeputyColorChain,
    Fallback,
}

pub(crate) fn default_grand_chain_priority() -> Vec<GrandChainPriorityItem> {
    vec![
        GrandChainPriorityItem::MainBraveChain,
        GrandChainPriorityItem::MainReadyNp,
        GrandChainPriorityItem::DeputyBraveChain,
        GrandChainPriorityItem::MainColorChain,
        GrandChainPriorityItem::DeputyColorChain,
        GrandChainPriorityItem::Fallback,
    ]
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GrandCardStrategy {
    #[serde(default = "default_grand_chain_priority")]
    pub chain_priority: Vec<GrandChainPriorityItem>,
    #[serde(default)]
    pub custom_rules: Vec<GrandCardRuleConfig>,
}

impl Default for GrandCardStrategy {
    fn default() -> Self {
        Self {
            chain_priority: default_grand_chain_priority(),
            custom_rules: Vec::new(),
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GrandCardRuleSlotConfig {
    #[serde(default)]
    pub member_id: Option<String>,
    #[serde(default)]
    pub slot_index: Option<u32>,
    #[serde(default)]
    pub servant_id: Option<u32>,
    #[serde(default)]
    pub is_support: bool,
    #[serde(default)]
    pub grand_servant: bool,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub color: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GrandCardRuleConfig {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub slots: Vec<GrandCardRuleSlotConfig>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GrandServantConfig {
    #[serde(default)]
    pub member_id: Option<String>,
    pub slot_index: u32,
    #[serde(default)]
    pub servant_id: Option<u32>,
    #[serde(default)]
    pub is_support: bool,
    #[serde(default = "default_grand_np_card")]
    pub np_card: String,
    #[serde(default = "default_grand_card_priority")]
    pub priority: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Legacy persisted field. Normalization migrates it into `role`, and
    /// serialization intentionally omits it from newly saved projects.
    #[serde(default, skip_serializing)]
    pub lancer_role: Option<LancerGrandRole>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LancerGrandRole {
    Single,
    Aoe,
}

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GrandRoleDefinition {
    pub role: String,
    pub label: String,
    pub required: bool,
}

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GrandClassDefinition {
    pub id: GrandClass,
    pub label: String,
    pub servant_class: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection_group: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection_group_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection_option_label: Option<String>,
    pub roles: Vec<GrandRoleDefinition>,
    pub card_priority_enabled: bool,
    pub auto_order_change_roles: Vec<String>,
    pub validation_message: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRecognitionSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub support_ce_threshold: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub support_ce_full_gate_threshold: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub support_mlb_icon_threshold: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub support_bond_icon_threshold: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify_skill_activation: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enable_extra_class_filter: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_on_five_star_ce_drop: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub five_star_ce_drop_target_count: Option<u32>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    /// Advanced teams use rule-based battle configuration stored separately
    /// from the legacy preparation/attack scene list.
    #[serde(default)]
    pub advanced_mode: bool,
    /// Pinned support-select servant id. The runner's `handle_support_select`
    /// reads this through `RunConfig::support_servant_id` to drive the OCR
    /// detector. `None` means the user hasn't pinned anyone yet, in which
    /// case the runner falls back to tapping the topmost visible support.
    /// `#[serde(default)]` so legacy `projects.json` rows without the field
    /// continue to deserialize.
    #[serde(default)]
    pub support_servant_id: Option<u32>,
    #[serde(default)]
    pub support_servant_variant_key: Option<String>,
    #[serde(default)]
    pub support_grand_mode: bool,
    #[serde(default = "default_support_grand_craft_essence_ids")]
    pub support_grand_craft_essence_ids: [Option<u32>; 3],
    #[serde(default = "default_support_grand_craft_essence_id_lists")]
    pub support_grand_craft_essence_id_lists: [Vec<u32>; 3],
    #[serde(default = "default_support_grand_craft_essence_mlb_required")]
    pub support_grand_craft_essence_mlb_required: [bool; 3],
    #[serde(default)]
    pub support_grand_bond_ce_mode: SupportGrandBondCeMode,
    #[serde(default)]
    pub grand_class: GrandClass,
    #[serde(default)]
    pub grand_servants: Vec<GrandServantConfig>,
    #[serde(default)]
    pub grand_card_strategy: GrandCardStrategy,
    /// Optional minimum servant level required for the chosen support row.
    #[serde(default)]
    pub support_servant_level_min: Option<u32>,
    /// Optional support-search NP minimum level. `None` means "任意".
    #[serde(default)]
    pub support_noble_phantasm_level_min: Option<u32>,
    /// Optional minimum ordinary star-map score (0-62).
    #[serde(default)]
    pub support_star_map_score_min: Option<u32>,
    /// Optional minimum Grand star-map score (0-16), used in Grand mode.
    #[serde(default)]
    pub support_grand_star_map_score_min: Option<u32>,
    /// Optional support-search owned skill minimum levels, one entry per
    /// skill slot. `None` means "任意".
    #[serde(default = "default_support_skill_level_mins")]
    pub support_skill_level_mins: [Option<u32>; 3],
    /// Optional support-search append skill minimum levels, one entry per
    /// append slot. `None` means "任意".
    #[serde(default = "default_support_append_skill_level_mins")]
    pub support_append_skill_level_mins: [Option<u32>; 5],
    /// Optional project-specific recognition thresholds. `None` means this
    /// project inherits the global recognition settings.
    #[serde(default)]
    pub recognition_settings: Option<ProjectRecognitionSettings>,
    #[serde(default)]
    pub disable_auto_skill_target_recognition: bool,
    #[serde(default)]
    pub mystic_code_id: Option<u32>,
    /// When two command cards belong to the same member and have the same
    /// color, prefer the one with the higher recognized critical chance.
    #[serde(default)]
    pub prefer_higher_critical_chance: bool,
    /// Team-builder grid layout (chosen servants + slot order). Persisted
    /// so the user's selections survive app restarts and project switches.
    /// Defaulted via `default_project_slots` for legacy rows.
    #[serde(default = "default_project_slots")]
    pub slots: Vec<ProjectSlot>,
    /// When `true`, the runner taps "Next" on the post-battle continue
    /// screen so the same quest is queued again; when `false`, it taps
    /// "Close" and the run terminates. `#[serde(default)]` keeps legacy
    /// rows (no field) defaulting to `false` = single-run behaviour.
    #[serde(default)]
    pub repeat_mission: bool,
    /// Three-way repeat selector persisted for the start page. `None`
    /// indicates a legacy row and is normalized from `repeat_mission`.
    #[serde(default)]
    pub repeat_mode: Option<ProjectRepeatMode>,
    /// Persisted run count used when `repeat_mode == Count`.
    #[serde(default)]
    pub repeat_count: Option<u32>,
    /// Persisted AP recovery items in UI priority order.
    #[serde(default)]
    pub ap_recovery_items: Vec<ApRecoveryItem>,
    /// Persisted per-run use caps. Missing/null item limits mean unlimited.
    #[serde(default)]
    pub ap_recovery_limits: ApRecoveryLimits,
}

/// User-defined, one-level grouping for projects. Project execution data stays
/// in `projects.json`; this catalog only owns organization and display order.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectGroup {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub project_ids: Vec<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCatalog {
    #[serde(default = "default_project_catalog_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub groups: Vec<ProjectGroup>,
    #[serde(default)]
    pub ungrouped_project_ids: Vec<String>,
}

fn default_project_catalog_schema_version() -> u32 {
    1
}

impl Default for ProjectCatalog {
    fn default() -> Self {
        Self {
            schema_version: default_project_catalog_schema_version(),
            groups: Vec::new(),
            ungrouped_project_ids: Vec::new(),
        }
    }
}
