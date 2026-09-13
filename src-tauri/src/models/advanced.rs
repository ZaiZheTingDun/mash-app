//! Advanced battle conditions, actions, rules, and scene wire models.

use super::{Action, AttackCard, OrderChangeSelection, SkillSelection};

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
