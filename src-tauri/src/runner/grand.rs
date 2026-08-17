//! Grand and advanced automatic card strategy helpers.
//!
//! This module owns advanced auto-pick candidate scoring, Grand servant chain
//! rules, custom Grand card rules, and Grand-class-specific card ordering.

use super::*;

mod berserker;
mod extra;
mod lancer;
mod saber;

use berserker::BerserkerStrategy;
use extra::ExtraStrategy;
use lancer::LancerStrategy;
use saber::SaberStrategy;

static SABER_STRATEGY: SaberStrategy = SaberStrategy;
static LANCER_STRATEGY: LancerStrategy = LancerStrategy;
static BERSERKER_STRATEGY: BerserkerStrategy = BerserkerStrategy;
static EXTRA1_FIRE_STRATEGY: ExtraStrategy = ExtraStrategy::fire();
static EXTRA1_EARTH_STRATEGY: ExtraStrategy = ExtraStrategy::earth();
static EXTRA2_WIND_STRATEGY: ExtraStrategy = ExtraStrategy::wind();
static EXTRA2_WATER_STRATEGY: ExtraStrategy = ExtraStrategy::water();

pub(crate) trait GrandClassStrategy: Sync {
    fn class(&self) -> GrandClass;
    fn definition(&self) -> GrandClassDefinition;

    fn built_in_rules(&self, _strategy: &GrandCardStrategy) -> Vec<GrandCardRule> {
        Vec::new()
    }

    fn incomplete_candidate_priority(&self) -> Option<RuleCandidatePriority> {
        None
    }

    fn choose_built_in_picks(
        &self,
        candidates: &[AdvancedPickCandidate],
        grand_servants: &[GrandServantRuntimeConfig],
        strategy: &GrandCardStrategy,
    ) -> Vec<Pick> {
        choose_with_grand_rules(candidates, grand_servants, self.built_in_rules(strategy))
    }

    fn normalize_servants(&self, servants: &mut Vec<GrandServantConfig>) {
        let definition = self.definition();
        let roles: Vec<&str> = definition
            .roles
            .iter()
            .map(|role| role.role.as_str())
            .collect();
        let mut claimed = HashSet::new();
        for servant in servants.iter_mut() {
            if servant.role.is_none() {
                servant.role = servant.lancer_role.map(|legacy| match legacy {
                    LancerGrandRole::Single => "single".to_string(),
                    LancerGrandRole::Aoe => "aoe".to_string(),
                });
            }
            servant.lancer_role = None;
            if servant
                .role
                .as_deref()
                .is_some_and(|role| !roles.contains(&role) || !claimed.insert(role.to_string()))
            {
                servant.role = None;
            }
        }
        for servant in servants.iter_mut().filter(|servant| servant.role.is_none()) {
            if let Some(role) = roles.iter().find(|role| !claimed.contains(**role)) {
                servant.role = Some((*role).to_string());
                claimed.insert((*role).to_string());
            }
        }
        servants.sort_by_key(|servant| {
            servant
                .role
                .as_deref()
                .and_then(|role| roles.iter().position(|candidate| *candidate == role))
                .unwrap_or(usize::MAX)
        });
    }

    fn validate_servants(&self, servants: &[GrandServantConfig]) -> Result<(), String> {
        let definition = self.definition();
        let roles: HashSet<&str> = servants
            .iter()
            .filter_map(|servant| servant.role.as_deref())
            .collect();
        let slots: HashSet<u32> = servants.iter().map(|servant| servant.slot_index).collect();
        let valid_count = !servants.is_empty() && servants.len() <= definition.roles.len();
        let valid_slots =
            slots.len() == servants.len() && servants.iter().all(|servant| servant.slot_index < 6);
        let all_known = servants.iter().all(|servant| {
            servant.role.as_deref().is_some_and(|role| {
                definition
                    .roles
                    .iter()
                    .any(|candidate| candidate.role == role)
            })
        });
        let all_required = definition
            .roles
            .iter()
            .filter(|role| role.required)
            .all(|role| roles.contains(role.role.as_str()));
        if valid_count && valid_slots && all_known && roles.len() == servants.len() && all_required
        {
            Ok(())
        } else {
            Err(definition.validation_message)
        }
    }

    fn auto_order_change_target<'a>(
        &self,
        servants: &'a [GrandServantRuntimeConfig],
    ) -> Option<&'a GrandServantRuntimeConfig> {
        let definition = self.definition();
        definition.auto_order_change_roles.iter().find_map(|role| {
            servants
                .iter()
                .find(|servant| servant.role == *role && (3..6).contains(&servant.slot_index))
        })
    }
}

pub(crate) fn grand_class_strategies() -> [&'static dyn GrandClassStrategy; 7] {
    [
        &SABER_STRATEGY,
        &LANCER_STRATEGY,
        &BERSERKER_STRATEGY,
        &EXTRA1_FIRE_STRATEGY,
        &EXTRA1_EARTH_STRATEGY,
        &EXTRA2_WIND_STRATEGY,
        &EXTRA2_WATER_STRATEGY,
    ]
}

pub(crate) fn grand_strategy(grand_class: GrandClass) -> &'static dyn GrandClassStrategy {
    grand_class_strategies()
        .into_iter()
        .find(|strategy| strategy.class() == grand_class)
        .expect("every GrandClass must have a registered strategy")
}

pub fn grand_class_definitions() -> Vec<GrandClassDefinition> {
    grand_class_strategies()
        .into_iter()
        .map(GrandClassStrategy::definition)
        .collect()
}

pub(crate) fn standard_definition(
    id: GrandClass,
    label: &str,
    servant_class: &str,
    card_priority_enabled: bool,
    validation_message: &str,
) -> GrandClassDefinition {
    GrandClassDefinition {
        id,
        label: label.into(),
        servant_class: servant_class.into(),
        selection_group: None,
        selection_group_label: None,
        selection_option_label: None,
        roles: vec![
            GrandRoleDefinition {
                role: "main".into(),
                label: "主".into(),
                required: true,
            },
            GrandRoleDefinition {
                role: "deputy".into(),
                label: "副".into(),
                required: false,
            },
        ],
        card_priority_enabled,
        auto_order_change_roles: vec!["main".into()],
        validation_message: validation_message.into(),
    }
}

#[derive(Clone)]
pub(crate) struct AdvancedPickCandidate {
    pub(crate) pick: Pick,
    pub(crate) servant_index: Option<usize>,
    pub(crate) servant_id: Option<u32>,
    pub(crate) color: Option<String>,
    pub(crate) original_order: u32,
    pub(crate) is_np: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum GrandRole {
    Main,
    Deputy,
    Other,
}

pub(crate) fn grand_role_for_servant(
    servant_id: Option<u32>,
    grand_servants: &[GrandServantRuntimeConfig],
) -> GrandRole {
    match servant_id {
        Some(id)
            if grand_servants
                .first()
                .is_some_and(|config| config.servant_id == id) =>
        {
            GrandRole::Main
        }
        Some(id)
            if grand_servants
                .get(1)
                .is_some_and(|config| config.servant_id == id) =>
        {
            GrandRole::Deputy
        }
        _ => GrandRole::Other,
    }
}

pub(crate) fn grand_role_for_candidate(
    candidate: &AdvancedPickCandidate,
    grand_servants: &[GrandServantRuntimeConfig],
) -> GrandRole {
    if let Some(index) = candidate.servant_index {
        if grand_servants
            .first()
            .is_some_and(|config| config.slot_index == index)
        {
            return GrandRole::Main;
        }
        if grand_servants
            .get(1)
            .is_some_and(|config| config.slot_index == index)
        {
            return GrandRole::Deputy;
        }
    }
    grand_role_for_servant(candidate.servant_id, grand_servants)
}

pub(crate) fn grand_config_for_role(
    role: GrandRole,
    grand_servants: &[GrandServantRuntimeConfig],
) -> Option<&GrandServantRuntimeConfig> {
    match role {
        GrandRole::Main => grand_servants.first(),
        GrandRole::Deputy => grand_servants.get(1),
        GrandRole::Other => None,
    }
}

pub(crate) fn grand_np_color(config: &GrandServantRuntimeConfig) -> Option<&'static str> {
    if config.np_card != "auto" {
        return suit_code(&config.np_card);
    }
    servant_np_card_code(config.servant_id)
}

pub(crate) fn main_output_index(scene: &AdvancedBattleScene) -> Option<usize> {
    scene
        .main_output
        .as_ref()
        .and_then(|main| main.servant.as_deref())
        .and_then(|value| parse_index(value, "servant_"))
        .filter(|index| *index < 3)
}

pub(crate) fn main_np_color(
    scene: &AdvancedBattleScene,
    party_ids: &[Option<u32>; 3],
) -> Option<&'static str> {
    let main = scene.main_output.as_ref()?;
    if let Some(card) = main.np_card.as_deref().filter(|card| *card != "auto") {
        return suit_code(card);
    }
    let index = main_output_index(scene)?;
    let servant_id = party_ids.get(index).copied().flatten()?;
    servant_np_card_code(servant_id)
}

pub(crate) fn candidate_color_counts(
    candidates: &[&AdvancedPickCandidate],
) -> (usize, usize, usize) {
    let b = candidates
        .iter()
        .filter(|c| c.color.as_deref() == Some("b"))
        .count();
    let a = candidates
        .iter()
        .filter(|c| c.color.as_deref() == Some("a"))
        .count();
    let q = candidates
        .iter()
        .filter(|c| c.color.as_deref() == Some("q"))
        .count();
    (b, a, q)
}

pub(crate) fn servant_np_card_code(id: u32) -> Option<&'static str> {
    servant_np_card(id).and_then(|card| suit_code(&card))
}

pub(crate) fn score_advanced_combo(
    scene: &AdvancedBattleScene,
    combo: &[&AdvancedPickCandidate],
    main_index: Option<usize>,
) -> i32 {
    let mut score = 0;
    let np_count = combo.iter().filter(|c| c.is_np).count();
    let main_count = combo
        .iter()
        .filter(|c| c.servant_index.is_some() && c.servant_index == main_index)
        .count();
    let (buster, arts, quick) = candidate_color_counts(combo);

    score += (np_count as i32) * 10_000;
    score += (main_count as i32) * 300;
    if main_count == 3 && buster == 1 && arts == 1 && quick == 1 {
        score += 2_000;
    }

    match scene
        .main_output
        .as_ref()
        .and_then(|main| main.output_type.as_ref())
    {
        Some(AdvancedOutputType::Np) => {
            if arts == 3 {
                score += 1_200;
            }
            score += (arts as i32) * 220;
            score += combo
                .iter()
                .filter(|c| c.color.as_deref() == Some("a") && c.servant_index == main_index)
                .count() as i32
                * 120;
        }
        Some(AdvancedOutputType::Critical) => {
            if buster == 1 && quick == 2 {
                score += 1_200;
            }
            score += (quick as i32) * 220;
            score += (buster as i32) * 80;
            score += combo
                .iter()
                .filter(|c| c.color.as_deref() == Some("q") && c.servant_index == main_index)
                .count() as i32
                * 120;
        }
        None => {}
    }

    score
}

pub(crate) fn sort_advanced_picks(
    scene: &AdvancedBattleScene,
    picks: &mut Vec<AdvancedPickCandidate>,
) {
    match scene
        .main_output
        .as_ref()
        .and_then(|main| main.output_type.as_ref())
    {
        Some(AdvancedOutputType::Critical) => picks.sort_by_key(|candidate| {
            (
                if candidate.is_np { 0 } else { 1 },
                match candidate.color.as_deref() {
                    Some("b") => 1,
                    Some("q") => 2,
                    Some("a") => 3,
                    _ => 4,
                },
                candidate.original_order,
            )
        }),
        _ => picks.sort_by_key(|candidate| {
            (
                match candidate.color.as_deref() {
                    Some("a") => 2,
                    _ => 1,
                },
                candidate.original_order,
            )
        }),
    }
}

pub(crate) fn combo_is_exquisite(combo: &[&AdvancedPickCandidate]) -> bool {
    candidate_color_counts(combo) == (1, 1, 1)
}

pub(crate) fn combo_same_color(combo: &[&AdvancedPickCandidate]) -> bool {
    matches!(
        candidate_color_counts(combo),
        (3, 0, 0) | (0, 3, 0) | (0, 0, 3)
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RuleOwner {
    MainGrand,
    DeputyGrand,
    AnyGrand,
    ExactSlot(usize),
    ExactServant(u32),
    Any,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RuleKind {
    Np,
    Command,
    Any,
}

#[derive(Clone, Copy)]
pub(crate) enum RuleColor {
    Any,
    Exact(&'static str),
    SameAsNp(GrandRole),
}

#[derive(Clone, Copy)]
pub(crate) enum RuleOwnerPriority {
    MainDeputyOther,
}

#[derive(Clone, Copy)]
pub(crate) enum RuleCandidatePriority {
    MainDeputyOtherThenArtsQuickBuster,
    MainDeputyOtherThenBusterArtsQuick,
}

#[derive(Clone, Copy)]
pub(crate) struct RuleSlot {
    pub(crate) owner: RuleOwner,
    pub(crate) kind: RuleKind,
    pub(crate) color: RuleColor,
    pub(crate) owner_priority: Option<RuleOwnerPriority>,
    pub(crate) candidate_priority: Option<RuleCandidatePriority>,
    pub(crate) np_as_command_owner: Option<RuleOwner>,
    pub(crate) preferred_np_owner: Option<RuleOwner>,
}

#[derive(Clone, Copy)]
pub(crate) struct RuleNeed {
    owner: RuleOwner,
    kind: RuleKind,
}

pub(crate) struct GrandCardRule {
    pub(crate) slots: [RuleSlot; 3],
    pub(crate) same_color: bool,
    pub(crate) color_set_baq: bool,
    pub(crate) include: Vec<RuleNeed>,
    pub(crate) exclude: Vec<RuleNeed>,
    pub(crate) target_role: Option<GrandRole>,
}

pub(crate) struct GrandRuleMatch {
    pub(crate) ordered: Vec<AdvancedPickCandidate>,
    score: i64,
}

pub(crate) fn any_slot() -> RuleSlot {
    RuleSlot {
        owner: RuleOwner::Any,
        kind: RuleKind::Any,
        color: RuleColor::Any,
        owner_priority: None,
        candidate_priority: None,
        np_as_command_owner: None,
        preferred_np_owner: None,
    }
}

pub(crate) fn slot(owner: RuleOwner, kind: RuleKind, color: RuleColor) -> RuleSlot {
    RuleSlot {
        owner,
        kind,
        color,
        owner_priority: None,
        candidate_priority: None,
        np_as_command_owner: None,
        preferred_np_owner: None,
    }
}

pub(crate) fn prioritized_any_slot(kind: RuleKind, color: RuleColor) -> RuleSlot {
    RuleSlot {
        owner: RuleOwner::Any,
        kind,
        color,
        owner_priority: Some(RuleOwnerPriority::MainDeputyOther),
        candidate_priority: None,
        np_as_command_owner: None,
        preferred_np_owner: None,
    }
}

pub(crate) fn with_owner_priority(mut slot: RuleSlot, priority: RuleOwnerPriority) -> RuleSlot {
    slot.owner_priority = Some(priority);
    slot
}

pub(crate) fn with_candidate_priority(
    mut slot: RuleSlot,
    priority: RuleCandidatePriority,
) -> RuleSlot {
    slot.candidate_priority = Some(priority);
    slot
}

pub(crate) fn with_np_as_command_owner(mut slot: RuleSlot, owner: RuleOwner) -> RuleSlot {
    slot.np_as_command_owner = Some(owner);
    slot
}

pub(crate) fn with_preferred_np_owner(mut slot: RuleSlot, owner: RuleOwner) -> RuleSlot {
    slot.preferred_np_owner = Some(owner);
    slot
}

pub(crate) fn need(owner: RuleOwner, kind: RuleKind) -> RuleNeed {
    RuleNeed { owner, kind }
}

pub(crate) fn custom_rule_kind(value: &str) -> Option<RuleKind> {
    match value {
        "np" => Some(RuleKind::Np),
        "command" => Some(RuleKind::Command),
        "any" => Some(RuleKind::Any),
        _ => None,
    }
}

pub(crate) fn custom_rule_color(value: &str) -> Option<RuleColor> {
    match value {
        "buster" => Some(RuleColor::Exact("b")),
        "arts" => Some(RuleColor::Exact("a")),
        "quick" => Some(RuleColor::Exact("q")),
        "any" => Some(RuleColor::Any),
        _ => None,
    }
}

pub(crate) fn custom_rule_target_role(rule: &GrandCardRule) -> Option<GrandRole> {
    if rule
        .slots
        .iter()
        .any(|slot| slot.owner == RuleOwner::MainGrand)
        || rule
            .include
            .iter()
            .any(|need| need.owner == RuleOwner::MainGrand)
    {
        Some(GrandRole::Main)
    } else if rule
        .slots
        .iter()
        .any(|slot| slot.owner == RuleOwner::DeputyGrand)
        || rule
            .include
            .iter()
            .any(|need| need.owner == RuleOwner::DeputyGrand)
    {
        Some(GrandRole::Deputy)
    } else {
        None
    }
}

pub(crate) fn custom_rule_config_to_rule(config: &GrandCardRuleConfig) -> Option<GrandCardRule> {
    if config.slots.len() != 3 {
        return None;
    }

    let slots_vec = config
        .slots
        .iter()
        .map(|slot_config| {
            let kind = custom_rule_kind(&slot_config.kind)?;
            let color = if kind == RuleKind::Np {
                RuleColor::Any
            } else {
                custom_rule_color(&slot_config.color)?
            };
            let owner = if slot_config.grand_servant {
                RuleOwner::AnyGrand
            } else if let Some(slot_index) = slot_config.slot_index {
                RuleOwner::ExactSlot(slot_index as usize)
            } else if let Some(servant_id) = slot_config.servant_id {
                RuleOwner::ExactServant(servant_id)
            } else {
                RuleOwner::Any
            };
            let rule_slot = slot(owner, kind, color);
            Some(if slot_config.grand_servant {
                with_owner_priority(rule_slot, RuleOwnerPriority::MainDeputyOther)
            } else {
                rule_slot
            })
        })
        .collect::<Option<Vec<_>>>()?;
    let slots: [RuleSlot; 3] = slots_vec.try_into().ok()?;
    let mut rule = GrandCardRule {
        slots,
        same_color: false,
        color_set_baq: false,
        include: Vec::new(),
        exclude: Vec::new(),
        target_role: None,
    };
    rule.target_role = custom_rule_target_role(&rule);
    Some(rule)
}

pub(crate) fn custom_grand_card_rules(strategy: &GrandCardStrategy) -> Vec<GrandCardRule> {
    strategy
        .custom_rules
        .iter()
        .filter_map(custom_rule_config_to_rule)
        .collect()
}

pub(crate) fn owner_matches(
    candidate: &AdvancedPickCandidate,
    owner: RuleOwner,
    grand_servants: &[GrandServantRuntimeConfig],
) -> bool {
    match owner {
        RuleOwner::Any => true,
        RuleOwner::MainGrand => {
            grand_role_for_candidate(candidate, grand_servants) == GrandRole::Main
        }
        RuleOwner::DeputyGrand => {
            grand_role_for_candidate(candidate, grand_servants) == GrandRole::Deputy
        }
        RuleOwner::AnyGrand => matches!(
            grand_role_for_candidate(candidate, grand_servants),
            GrandRole::Main | GrandRole::Deputy
        ),
        RuleOwner::ExactSlot(slot_index) => candidate.servant_index == Some(slot_index),
        RuleOwner::ExactServant(servant_id) => candidate.servant_id == Some(servant_id),
    }
}

pub(crate) fn kind_matches(candidate: &AdvancedPickCandidate, kind: RuleKind) -> bool {
    match kind {
        RuleKind::Any => true,
        RuleKind::Np => candidate.is_np,
        RuleKind::Command => !candidate.is_np,
    }
}

pub(crate) fn kind_matches_slot(
    candidate: &AdvancedPickCandidate,
    slot: RuleSlot,
    grand_servants: &[GrandServantRuntimeConfig],
) -> bool {
    if kind_matches(candidate, slot.kind) {
        return true;
    }
    slot.kind == RuleKind::Command
        && candidate.is_np
        && slot
            .np_as_command_owner
            .is_some_and(|owner| owner_matches(candidate, owner, grand_servants))
}

pub(crate) fn color_matches(
    candidate: &AdvancedPickCandidate,
    color: RuleColor,
    grand_servants: &[GrandServantRuntimeConfig],
) -> bool {
    match color {
        RuleColor::Any => true,
        RuleColor::Exact(expected) => candidate.color.as_deref() == Some(expected),
        RuleColor::SameAsNp(role) => target_np_color(Some(role), grand_servants)
            .as_deref()
            .is_some_and(|expected| candidate.color.as_deref() == Some(expected)),
    }
}

pub(crate) fn candidate_matches_need(
    candidate: &AdvancedPickCandidate,
    need: RuleNeed,
    grand_servants: &[GrandServantRuntimeConfig],
) -> bool {
    owner_matches(candidate, need.owner, grand_servants) && kind_matches(candidate, need.kind)
}

pub(crate) fn candidate_matches_slot(
    candidate: &AdvancedPickCandidate,
    slot: RuleSlot,
    grand_servants: &[GrandServantRuntimeConfig],
) -> bool {
    owner_matches(candidate, slot.owner, grand_servants)
        && kind_matches_slot(candidate, slot, grand_servants)
        && color_matches(candidate, slot.color, grand_servants)
}

pub(crate) fn combo_matches_rule_requirements(
    combo: &[&AdvancedPickCandidate],
    rule: &GrandCardRule,
    grand_servants: &[GrandServantRuntimeConfig],
) -> bool {
    if rule.same_color && !combo_same_color(combo) {
        return false;
    }
    if rule.color_set_baq && !combo_is_exquisite(combo) {
        return false;
    }
    if rule.include.iter().any(|need| {
        !combo
            .iter()
            .any(|candidate| candidate_matches_need(candidate, *need, grand_servants))
    }) {
        return false;
    }
    if rule.exclude.iter().any(|need| {
        combo
            .iter()
            .any(|candidate| candidate_matches_need(candidate, *need, grand_servants))
    }) {
        return false;
    }
    true
}

pub(crate) fn ordered_position_score(
    index: usize,
    candidate: &AdvancedPickCandidate,
    grand_servants: &[GrandServantRuntimeConfig],
) -> i32 {
    let original_order_score = 100 - candidate.original_order as i32;
    if candidate.is_np {
        return 10_000 + original_order_score;
    }
    let is_grand = matches!(
        grand_role_for_candidate(candidate, grand_servants),
        GrandRole::Main | GrandRole::Deputy
    );
    if is_grand {
        index as i32 * 1_000 + original_order_score
    } else {
        (2 - index as i32) * 100 + original_order_score
    }
}

pub(crate) fn owner_priority_score(
    ordered: &[&AdvancedPickCandidate; 3],
    rule: &GrandCardRule,
    grand_servants: &[GrandServantRuntimeConfig],
) -> i32 {
    ordered
        .iter()
        .zip(rule.slots.iter())
        .filter_map(|(candidate, slot)| {
            slot.owner_priority.map(|priority| {
                match (
                    priority,
                    grand_role_for_candidate(candidate, grand_servants),
                ) {
                    (RuleOwnerPriority::MainDeputyOther, GrandRole::Main) => 300_000,
                    (RuleOwnerPriority::MainDeputyOther, GrandRole::Deputy) => 200_000,
                    (RuleOwnerPriority::MainDeputyOther, GrandRole::Other) => 100_000,
                }
            })
        })
        .sum()
}

pub(crate) fn preferred_np_owner_score(
    ordered: &[&AdvancedPickCandidate; 3],
    rule: &GrandCardRule,
    grand_servants: &[GrandServantRuntimeConfig],
) -> i32 {
    ordered
        .iter()
        .zip(rule.slots.iter())
        .filter_map(|(candidate, slot)| {
            slot.preferred_np_owner
                .filter(|owner| candidate.is_np && owner_matches(candidate, *owner, grand_servants))
        })
        .count() as i32
        * 500_000
}

pub(crate) fn candidate_priority_score(
    ordered: &[&AdvancedPickCandidate; 3],
    rule: &GrandCardRule,
    grand_servants: &[GrandServantRuntimeConfig],
) -> i64 {
    const POSITION_WEIGHTS: [i64; 3] = [1_000_000_000, 1_000_000, 1];
    ordered
        .iter()
        .zip(rule.slots.iter())
        .enumerate()
        .filter_map(|(index, (candidate, slot))| {
            slot.candidate_priority.map(|priority| {
                let (owner, color) = candidate_priority_rank(priority, candidate, grand_servants);
                let original_order = 100_i64.saturating_sub(candidate.original_order as i64);
                (owner * 10_000 + color * 100 + original_order) * POSITION_WEIGHTS[index]
            })
        })
        .sum()
}

pub(crate) fn candidate_priority_rank(
    priority: RuleCandidatePriority,
    candidate: &AdvancedPickCandidate,
    grand_servants: &[GrandServantRuntimeConfig],
) -> (i64, i64) {
    match priority {
        RuleCandidatePriority::MainDeputyOtherThenArtsQuickBuster => {
            let owner = match grand_role_for_candidate(candidate, grand_servants) {
                GrandRole::Main => 3,
                GrandRole::Deputy => 2,
                GrandRole::Other => 1,
            };
            let color = match candidate.color.as_deref() {
                Some("a") => 3,
                Some("q") => 2,
                Some("b") => 1,
                _ => 0,
            };
            (owner, color)
        }
        RuleCandidatePriority::MainDeputyOtherThenBusterArtsQuick => {
            let owner = match grand_role_for_candidate(candidate, grand_servants) {
                GrandRole::Main => 3,
                GrandRole::Deputy => 2,
                GrandRole::Other => 1,
            };
            let color = match candidate.color.as_deref() {
                Some("b") => 3,
                Some("a") => 2,
                Some("q") => 1,
                _ => 0,
            };
            (owner, color)
        }
    }
}

pub(crate) fn sort_by_candidate_priority(
    candidates: &mut [AdvancedPickCandidate],
    priority: RuleCandidatePriority,
    grand_servants: &[GrandServantRuntimeConfig],
) {
    candidates.sort_by_key(|candidate| {
        let (owner, color) = candidate_priority_rank(priority, candidate, grand_servants);
        (
            std::cmp::Reverse(owner),
            std::cmp::Reverse(color),
            candidate.original_order,
        )
    });
}

pub(crate) fn score_rule_match(
    ordered: &[&AdvancedPickCandidate; 3],
    rule: &GrandCardRule,
    grand_servants: &[GrandServantRuntimeConfig],
) -> i64 {
    let np_count = ordered.iter().filter(|candidate| candidate.is_np).count() as i32;
    let main_count = ordered
        .iter()
        .filter(|candidate| grand_role_for_candidate(candidate, grand_servants) == GrandRole::Main)
        .count() as i32;
    let deputy_count = ordered
        .iter()
        .filter(|candidate| {
            grand_role_for_candidate(candidate, grand_servants) == GrandRole::Deputy
        })
        .count() as i32;
    let target_count = rule
        .target_role
        .map(|role| {
            ordered
                .iter()
                .filter(|candidate| grand_role_for_candidate(candidate, grand_servants) == role)
                .count() as i32
        })
        .unwrap_or(main_count.max(deputy_count));
    let order_score = ordered
        .iter()
        .enumerate()
        .map(|(index, candidate)| ordered_position_score(index, candidate, grand_servants))
        .sum::<i32>();
    let owner_priority_score = owner_priority_score(ordered, rule, grand_servants);
    let preferred_np_owner_score = preferred_np_owner_score(ordered, rule, grand_servants);
    candidate_priority_score(ordered, rule, grand_servants)
        + owner_priority_score as i64
        + preferred_np_owner_score as i64
        + target_count as i64 * 10_000
        + main_count as i64 * 1_000
        + deputy_count as i64 * 800
        + np_count as i64 * 200
        + order_score as i64
}

pub(crate) fn match_grand_rule(
    combo: &[&AdvancedPickCandidate],
    rule: &GrandCardRule,
    grand_servants: &[GrandServantRuntimeConfig],
) -> Option<GrandRuleMatch> {
    if combo.len() != 3 || !combo_matches_rule_requirements(combo, rule, grand_servants) {
        return None;
    }
    let permutations = [
        [0usize, 1usize, 2usize],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    let mut best: Option<GrandRuleMatch> = None;
    for permutation in permutations {
        let ordered = [
            combo[permutation[0]],
            combo[permutation[1]],
            combo[permutation[2]],
        ];
        if !ordered
            .iter()
            .zip(rule.slots.iter())
            .all(|(candidate, slot)| candidate_matches_slot(candidate, *slot, grand_servants))
        {
            continue;
        }
        let score = score_rule_match(&ordered, rule, grand_servants);
        let candidate_match = GrandRuleMatch {
            ordered: ordered
                .iter()
                .map(|candidate| (*candidate).clone())
                .collect(),
            score,
        };
        if best
            .as_ref()
            .is_none_or(|current| candidate_match.score > current.score)
        {
            best = Some(candidate_match);
        }
    }
    best
}

pub(crate) fn normalized_grand_chain_priority(
    strategy: &GrandCardStrategy,
) -> Vec<GrandChainPriorityItem> {
    let defaults = default_grand_chain_priority();
    let mut normalized = Vec::with_capacity(defaults.len());
    for item in &strategy.chain_priority {
        if defaults.contains(item) && !normalized.contains(item) {
            normalized.push(*item);
        }
    }
    for item in defaults {
        if !normalized.contains(&item) {
            normalized.push(item);
        }
    }
    normalized
}

pub(crate) fn target_np_color(
    role: Option<GrandRole>,
    grand_servants: &[GrandServantRuntimeConfig],
) -> Option<String> {
    role.and_then(|role| grand_config_for_role(role, grand_servants))
        .and_then(grand_np_color)
        .map(str::to_string)
}

pub(crate) fn sort_exquisite_grand_picks(
    picks: &mut Vec<AdvancedPickCandidate>,
    target_role: GrandRole,
    grand_servants: &[GrandServantRuntimeConfig],
) {
    let priority = grand_config_for_role(target_role, grand_servants)
        .map(|config| config.priority.as_str())
        .unwrap_or("damage");
    let np_color = target_np_color(Some(target_role), grand_servants);
    let use_damage_order = priority != "np" || np_color.as_deref() == Some("b");

    picks.sort_by_key(|candidate| {
        if use_damage_order {
            (
                if candidate.is_np {
                    3
                } else if np_color
                    .as_deref()
                    .is_some_and(|color| candidate.color.as_deref() == Some(color))
                {
                    2
                } else {
                    1
                },
                candidate.original_order,
            )
        } else {
            (
                if !candidate.is_np && candidate.color.as_deref() == Some("b") {
                    1
                } else if candidate.is_np {
                    2
                } else {
                    3
                },
                candidate.original_order,
            )
        }
    });
}

pub(crate) fn sort_grand_picks(
    picks: &mut Vec<AdvancedPickCandidate>,
    tier_role: Option<GrandRole>,
    grand_servants: &[GrandServantRuntimeConfig],
) {
    if let Some(role) = tier_role {
        let same_servant = picks
            .iter()
            .all(|candidate| grand_role_for_candidate(candidate, grand_servants) == role);
        let refs: Vec<&AdvancedPickCandidate> = picks.iter().collect();
        if same_servant && combo_is_exquisite(&refs) {
            sort_exquisite_grand_picks(picks, role, grand_servants);
            return;
        }
        if combo_same_color(&refs) {
            picks.sort_by_key(|candidate| {
                (
                    if candidate.is_np { 0 } else { 1 },
                    if grand_role_for_candidate(candidate, grand_servants) == role {
                        1
                    } else {
                        0
                    },
                    candidate.original_order,
                )
            });
            return;
        }
    }

    let target_role = if picks
        .iter()
        .any(|candidate| grand_role_for_candidate(candidate, grand_servants) == GrandRole::Main)
    {
        GrandRole::Main
    } else {
        GrandRole::Deputy
    };
    let target_np_slot = picks
        .iter()
        .find(|candidate| {
            candidate.is_np && grand_role_for_candidate(candidate, grand_servants) == target_role
        })
        .map(|candidate| candidate.original_order);
    let preferred_dye = grand_config_for_role(target_role, grand_servants)
        .map(|config| if config.priority == "np" { "a" } else { "b" })
        .unwrap_or("b");

    picks.sort_by_key(|candidate| {
        let role = grand_role_for_candidate(candidate, grand_servants);
        let is_target = role == target_role;
        (
            match (candidate.is_np, is_target, target_np_slot.is_some()) {
                (true, false, true) => 0,
                (true, true, _) => 1,
                (false, false, false) if candidate.color.as_deref() == Some(preferred_dye) => 2,
                (false, true, _) => 4,
                _ => 3,
            },
            match role {
                GrandRole::Main => 2,
                GrandRole::Deputy => 1,
                GrandRole::Other => 0,
            },
            candidate.original_order,
        )
    });
}

pub(crate) fn choose_grand_auto_picks(
    candidates: &[AdvancedPickCandidate],
    grand_servants: &[GrandServantRuntimeConfig],
    strategy: &GrandCardStrategy,
    grand_class: GrandClass,
) -> Vec<Pick> {
    let custom = choose_with_grand_rules(
        candidates,
        grand_servants,
        custom_grand_card_rules(strategy),
    );
    if !custom.is_empty() {
        return custom;
    }
    grand_strategy(grand_class).choose_built_in_picks(candidates, grand_servants, strategy)
}

pub(crate) fn choose_with_grand_rules(
    candidates: &[AdvancedPickCandidate],
    grand_servants: &[GrandServantRuntimeConfig],
    rules: Vec<GrandCardRule>,
) -> Vec<Pick> {
    for rule in rules {
        let mut best: Option<GrandRuleMatch> = None;
        for i in 0..candidates.len() {
            for j in (i + 1)..candidates.len() {
                for k in (j + 1)..candidates.len() {
                    let combo = vec![&candidates[i], &candidates[j], &candidates[k]];
                    let Some(rule_match) = match_grand_rule(&combo, &rule, grand_servants) else {
                        continue;
                    };
                    if best
                        .as_ref()
                        .is_none_or(|current| rule_match.score > current.score)
                    {
                        best = Some(rule_match);
                    }
                }
            }
        }
        if let Some(best) = best {
            return best
                .ordered
                .into_iter()
                .map(|candidate| candidate.pick)
                .collect();
        }
    }
    Vec::new()
}

fn ordinary_advanced_rules(
    strategy: &AdvancedCardStrategy,
    members: &[Option<PartyMemberRuntime>; 3],
) -> Vec<(String, GrandCardRule)> {
    strategy
        .custom_rules
        .iter()
        .cloned()
        .enumerate()
        .filter_map(|(index, mut rule)| {
            let name = if rule.name.trim().is_empty() {
                format!("规则 {}", index + 1)
            } else {
                rule.name.trim().to_string()
            };
            for slot in &mut rule.slots {
                slot.grand_servant = false;
                let has_specific_owner = slot.member_id.is_some()
                    || slot.servant_id.is_some()
                    || slot.slot_index.is_some();
                if !has_specific_owner {
                    continue;
                }
                let current_index = members.iter().position(|candidate| {
                    candidate.as_ref().is_some_and(|member| {
                        slot.member_id
                            .as_deref()
                            .zip(member.member_id.as_deref())
                            .is_some_and(|(left, right)| left == right)
                            || (slot.member_id.is_none()
                                && slot
                                    .slot_index
                                    .is_some_and(|index| index as usize == member.slot_index)
                                && slot.servant_id == Some(member.servant_id)
                                && slot.is_support == member.is_support)
                    })
                });
                slot.slot_index = Some(current_index.map(|index| index as u32).unwrap_or(6));
            }
            custom_rule_config_to_rule(&rule).map(|rule| (name, rule))
        })
        .collect()
}

pub(crate) fn choose_ordinary_advanced_picks(
    cards: &[CommandCardMatch],
    nps: &[NoblePhantasmMatch],
    members: &[Option<PartyMemberRuntime>; 3],
    strategy: &AdvancedCardStrategy,
) -> (Vec<Pick>, Option<String>) {
    let party_ids: [Option<u32>; 3] =
        std::array::from_fn(|index| members[index].as_ref().map(|member| member.servant_id));
    let mut candidates = Vec::new();
    for np in nps.iter().filter(|np| np.ready) {
        let servant_index = Some(np.slot as usize).filter(|index| *index < 3);
        let servant_id = servant_index.and_then(|index| party_ids[index]);
        candidates.push(AdvancedPickCandidate {
            pick: Pick::Np {
                slot: np.slot,
                point: rect_center(&np.card_region),
                from_priority: "高级模式".into(),
            },
            servant_index,
            servant_id,
            color: servant_id
                .and_then(servant_np_card_code)
                .map(str::to_string),
            original_order: np.slot,
            is_np: true,
        });
    }
    for card in cards.iter().filter(|card| !card.is_stunned) {
        let servant_index = card.servant_id.and_then(|servant_id| {
            members.iter().position(|candidate| {
                candidate.as_ref().is_some_and(|member| {
                    member.servant_id == servant_id && member.is_support == card.is_support
                })
            })
        });
        candidates.push(AdvancedPickCandidate {
            pick: Pick::Card {
                slot: card.slot,
                point: Point::new(card.x, card.y),
                servant_id: card.servant_id,
                suit: card.suit.clone(),
                from_priority: Some("高级模式".into()),
            },
            servant_index,
            servant_id: card.servant_id,
            color: card.suit.clone(),
            original_order: 10 + card.slot,
            is_np: false,
        });
    }

    for (name, rule) in ordinary_advanced_rules(strategy, members) {
        let picks = choose_with_grand_rules(&candidates, &[], vec![rule]);
        if !picks.is_empty() {
            return (picks, Some(name));
        }
    }

    let mut fallback: Vec<&CommandCardMatch> =
        cards.iter().filter(|card| !card.is_stunned).collect();
    fallback.sort_by_key(|card| card.slot);
    (
        fallback
            .into_iter()
            .take(3)
            .map(|card| Pick::Card {
                slot: card.slot,
                point: Point::new(card.x, card.y),
                servant_id: card.servant_id,
                suit: card.suit.clone(),
                from_priority: Some("高级模式默认补位".into()),
            })
            .collect(),
        None,
    )
}

fn append_unavailable_card_fallbacks(
    mut picks: Vec<Pick>,
    cards: &[CommandCardMatch],
) -> Vec<Pick> {
    let mut unavailable: Vec<&CommandCardMatch> =
        cards.iter().filter(|card| card.is_stunned).collect();
    unavailable.sort_by_key(|card| card.slot);

    for card in unavailable {
        if picks.len() >= 3 {
            break;
        }
        picks.push(Pick::Card {
            slot: card.slot,
            point: Point::new(card.x, card.y),
            servant_id: card.servant_id,
            suit: card.suit.clone(),
            from_priority: Some("不可用卡补位".into()),
        });
    }
    picks
}

pub(crate) fn choose_advanced_auto_picks_with_grand_class(
    scene: &AdvancedBattleScene,
    cards: &[CommandCardMatch],
    nps: &[NoblePhantasmMatch],
    party_ids: &[Option<u32>; 3],
    party_supports: &[bool; 3],
    grand_servants: &[GrandServantRuntimeConfig],
    grand_card_strategy: &GrandCardStrategy,
    grand_class: GrandClass,
) -> Vec<Pick> {
    let main_index = main_output_index(scene);
    let main_np_color = main_np_color(scene, party_ids);
    let mut candidates: Vec<AdvancedPickCandidate> = Vec::new();

    for np in nps.iter().filter(|np| np.ready) {
        let servant_index = Some(np.slot as usize).filter(|index| *index < 3);
        let servant_id = servant_index.and_then(|index| party_ids.get(index).copied().flatten());
        let color = if let Some(config) = servant_index
            .and_then(|index| {
                grand_servants
                    .iter()
                    .find(|config| config.slot_index == index)
            })
            .or_else(|| {
                servant_id
                    .and_then(|id| grand_servants.iter().find(|config| config.servant_id == id))
            }) {
            grand_np_color(config).map(str::to_string)
        } else if servant_index == main_index {
            main_np_color.map(str::to_string)
        } else {
            servant_id
                .and_then(servant_np_card_code)
                .map(str::to_string)
        };
        candidates.push(AdvancedPickCandidate {
            pick: Pick::Np {
                slot: np.slot,
                point: rect_center(&np.card_region),
                from_priority: "自动宝具".into(),
            },
            servant_index,
            servant_id,
            color,
            original_order: np.slot,
            is_np: true,
        });
    }

    // Unlike normal mode, advanced and Grand strategies build and score all
    // three-card combinations up front. Exclude unable-to-act cards before
    // that search so no rule or score can select them.
    for card in cards.iter().filter(|card| !card.is_stunned) {
        let servant_index = card.servant_id.and_then(|id| {
            party_ids
                .iter()
                .enumerate()
                .find(|(index, party_id)| {
                    **party_id == Some(id) && party_supports[*index] == card.is_support
                })
                .map(|(index, _)| index)
        });
        candidates.push(AdvancedPickCandidate {
            pick: Pick::Card {
                slot: card.slot,
                point: Point::new(card.x, card.y),
                servant_id: card.servant_id,
                suit: card.suit.clone(),
                from_priority: Some("自动策略".into()),
            },
            servant_index,
            servant_id: card.servant_id,
            color: card.suit.clone(),
            original_order: 10 + card.slot,
            is_np: false,
        });
    }

    if candidates.len() <= 3 {
        let mut selected = candidates;
        if grand_servants.is_empty() {
            sort_advanced_picks(scene, &mut selected);
        } else {
            let strategy_picks = choose_grand_auto_picks(
                &selected,
                grand_servants,
                grand_card_strategy,
                grand_class,
            );
            if !strategy_picks.is_empty() {
                return append_unavailable_card_fallbacks(strategy_picks, cards);
            }
            if let Some(priority) = grand_strategy(grand_class).incomplete_candidate_priority() {
                sort_by_candidate_priority(&mut selected, priority, grand_servants);
            } else {
                sort_grand_picks(&mut selected, None, grand_servants);
            }
        }
        return append_unavailable_card_fallbacks(
            selected
                .into_iter()
                .map(|candidate| candidate.pick)
                .collect(),
            cards,
        );
    }

    if !grand_servants.is_empty() {
        return choose_grand_auto_picks(
            &candidates,
            grand_servants,
            grand_card_strategy,
            grand_class,
        );
    }

    let mut best_score = i32::MIN;
    let mut best: Vec<AdvancedPickCandidate> = Vec::new();
    for i in 0..candidates.len() {
        for j in (i + 1)..candidates.len() {
            for k in (j + 1)..candidates.len() {
                let combo = vec![&candidates[i], &candidates[j], &candidates[k]];
                let score = score_advanced_combo(scene, &combo, main_index);
                if score > best_score {
                    best_score = score;
                    best = vec![
                        candidates[i].clone(),
                        candidates[j].clone(),
                        candidates[k].clone(),
                    ];
                }
            }
        }
    }
    sort_advanced_picks(scene, &mut best);
    best.into_iter().map(|candidate| candidate.pick).collect()
}

#[cfg(test)]
pub(crate) fn choose_advanced_auto_picks(
    scene: &AdvancedBattleScene,
    cards: &[CommandCardMatch],
    nps: &[NoblePhantasmMatch],
    party_ids: &[Option<u32>; 3],
    party_supports: &[bool; 3],
    grand_servants: &[GrandServantRuntimeConfig],
    grand_card_strategy: &GrandCardStrategy,
) -> Vec<Pick> {
    choose_advanced_auto_picks_with_grand_class(
        scene,
        cards,
        nps,
        party_ids,
        party_supports,
        grand_servants,
        grand_card_strategy,
        GrandClass::Saber,
    )
}

#[cfg(test)]
mod registry_tests {
    use super::*;

    #[test]
    fn every_grand_class_has_one_definition_with_unique_roles() {
        let definitions = grand_class_definitions();
        assert_eq!(definitions.len(), GrandClass::ALL.len());
        let ids: HashSet<_> = definitions.iter().map(|definition| definition.id).collect();
        assert_eq!(ids.len(), definitions.len());
        assert!(ids.contains(&GrandClass::Saber));
        for definition in definitions {
            let roles: HashSet<_> = definition.roles.iter().map(|role| &role.role).collect();
            assert_eq!(roles.len(), definition.roles.len());
            assert!(!definition.roles.is_empty());
        }
    }

    #[test]
    fn strategy_validation_rejects_duplicate_slots_and_missing_required_roles() {
        let servant = |slot_index, role: &str| GrandServantConfig {
            member_id: None,
            slot_index,
            servant_id: Some(slot_index + 1),
            is_support: false,
            np_card: "auto".into(),
            priority: "damage".into(),
            role: Some(role.into()),
            lancer_role: None,
        };
        assert!(grand_strategy(GrandClass::Saber)
            .validate_servants(&[servant(0, "main")])
            .is_ok());
        assert!(grand_strategy(GrandClass::Saber)
            .validate_servants(&[servant(0, "main"), servant(0, "deputy")])
            .is_err());
        assert!(grand_strategy(GrandClass::Lancer)
            .validate_servants(&[servant(0, "single")])
            .is_err());
        assert!(grand_strategy(GrandClass::Extra1Earth)
            .validate_servants(&[servant(0, "aoe")])
            .is_ok());
        assert!(grand_strategy(GrandClass::Extra1Earth)
            .validate_servants(&[servant(0, "single")])
            .is_err());
    }

    #[test]
    fn extra_definitions_group_stage_options_and_keep_single_optional() {
        let definitions = grand_class_definitions();
        let earth = definitions
            .iter()
            .find(|definition| definition.id == GrandClass::Extra1Earth)
            .expect("Extra1 earth definition");
        assert_eq!(earth.selection_group.as_deref(), Some("extra1"));
        assert_eq!(
            earth.selection_group_label.as_deref(),
            Some("额外职阶 Ⅰ 冠位")
        );
        assert_eq!(earth.selection_option_label.as_deref(), Some("地"));
        assert_eq!(earth.servant_class, "Extra1");
        assert_eq!(earth.roles[0].role, "aoe");
        assert!(earth.roles[0].required);
        assert_eq!(earth.roles[1].role, "single");
        assert!(!earth.roles[1].required);
    }
}
