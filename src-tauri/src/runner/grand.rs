//! Grand and advanced automatic card strategy helpers.
//!
//! This module owns advanced auto-pick candidate scoring, Grand servant chain
//! rules, custom Grand card rules, and Grand-class-specific card ordering.

use super::*;

#[derive(Clone)]
pub(crate) struct AdvancedPickCandidate {
    pick: Pick,
    servant_index: Option<usize>,
    servant_id: Option<u32>,
    color: Option<String>,
    original_order: u32,
    is_np: bool,
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
pub(crate) struct RuleSlot {
    pub(crate) owner: RuleOwner,
    pub(crate) kind: RuleKind,
    pub(crate) color: RuleColor,
    pub(crate) owner_priority: Option<RuleOwnerPriority>,
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
    ordered: Vec<AdvancedPickCandidate>,
    score: i32,
}

pub(crate) fn any_slot() -> RuleSlot {
    RuleSlot {
        owner: RuleOwner::Any,
        kind: RuleKind::Any,
        color: RuleColor::Any,
        owner_priority: None,
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
        np_as_command_owner: None,
        preferred_np_owner: None,
    }
}

pub(crate) fn with_owner_priority(mut slot: RuleSlot, priority: RuleOwnerPriority) -> RuleSlot {
    slot.owner_priority = Some(priority);
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
            } else {
                RuleOwner::ExactServant(slot_config.servant_id?)
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
            grand_role_for_servant(candidate.servant_id, grand_servants) == GrandRole::Main
        }
        RuleOwner::DeputyGrand => {
            grand_role_for_servant(candidate.servant_id, grand_servants) == GrandRole::Deputy
        }
        RuleOwner::AnyGrand => matches!(
            grand_role_for_servant(candidate.servant_id, grand_servants),
            GrandRole::Main | GrandRole::Deputy
        ),
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
        grand_role_for_servant(candidate.servant_id, grand_servants),
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
                    grand_role_for_servant(candidate.servant_id, grand_servants),
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

pub(crate) fn score_rule_match(
    ordered: &[&AdvancedPickCandidate; 3],
    rule: &GrandCardRule,
    grand_servants: &[GrandServantRuntimeConfig],
) -> i32 {
    let np_count = ordered.iter().filter(|candidate| candidate.is_np).count() as i32;
    let main_count = ordered
        .iter()
        .filter(|candidate| {
            grand_role_for_servant(candidate.servant_id, grand_servants) == GrandRole::Main
        })
        .count() as i32;
    let deputy_count = ordered
        .iter()
        .filter(|candidate| {
            grand_role_for_servant(candidate.servant_id, grand_servants) == GrandRole::Deputy
        })
        .count() as i32;
    let target_count = rule
        .target_role
        .map(|role| {
            ordered
                .iter()
                .filter(|candidate| {
                    grand_role_for_servant(candidate.servant_id, grand_servants) == role
                })
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
    owner_priority_score
        + preferred_np_owner_score
        + target_count * 10_000
        + main_count * 1_000
        + deputy_count * 800
        + np_count * 200
        + order_score
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
            .all(|candidate| grand_role_for_servant(candidate.servant_id, grand_servants) == role);
        let refs: Vec<&AdvancedPickCandidate> = picks.iter().collect();
        if same_servant && combo_is_exquisite(&refs) {
            sort_exquisite_grand_picks(picks, role, grand_servants);
            return;
        }
        if combo_same_color(&refs) {
            picks.sort_by_key(|candidate| {
                (
                    if candidate.is_np { 0 } else { 1 },
                    if grand_role_for_servant(candidate.servant_id, grand_servants) == role {
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

    let target_role = if picks.iter().any(|candidate| {
        grand_role_for_servant(candidate.servant_id, grand_servants) == GrandRole::Main
    }) {
        GrandRole::Main
    } else {
        GrandRole::Deputy
    };
    let target_np_slot = picks
        .iter()
        .find(|candidate| {
            candidate.is_np
                && grand_role_for_servant(candidate.servant_id, grand_servants) == target_role
        })
        .map(|candidate| candidate.original_order);
    let preferred_dye = grand_config_for_role(target_role, grand_servants)
        .map(|config| if config.priority == "np" { "a" } else { "b" })
        .unwrap_or("b");

    picks.sort_by_key(|candidate| {
        let role = grand_role_for_servant(candidate.servant_id, grand_servants);
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

pub(crate) fn saber_rules_for_priority_item(item: GrandChainPriorityItem) -> Vec<GrandCardRule> {
    match item {
        GrandChainPriorityItem::MainBraveChain => vec![
            GrandCardRule {
                slots: [
                    slot(RuleOwner::MainGrand, RuleKind::Command, RuleColor::Any),
                    slot(RuleOwner::MainGrand, RuleKind::Command, RuleColor::Any),
                    slot(RuleOwner::MainGrand, RuleKind::Np, RuleColor::Any),
                ],
                same_color: false,
                color_set_baq: true,
                include: Vec::new(),
                exclude: Vec::new(),
                target_role: Some(GrandRole::Main),
            },
            GrandCardRule {
                slots: [
                    slot(
                        RuleOwner::MainGrand,
                        RuleKind::Command,
                        RuleColor::Exact("b"),
                    ),
                    slot(
                        RuleOwner::MainGrand,
                        RuleKind::Command,
                        RuleColor::Exact("a"),
                    ),
                    slot(
                        RuleOwner::MainGrand,
                        RuleKind::Command,
                        RuleColor::Exact("q"),
                    ),
                ],
                same_color: false,
                color_set_baq: true,
                include: Vec::new(),
                exclude: vec![need(RuleOwner::MainGrand, RuleKind::Np)],
                target_role: Some(GrandRole::Main),
            },
        ],
        GrandChainPriorityItem::MainReadyNp => vec![GrandCardRule {
            slots: [
                any_slot(),
                any_slot(),
                slot(RuleOwner::MainGrand, RuleKind::Np, RuleColor::Any),
            ],
            same_color: false,
            color_set_baq: false,
            include: Vec::new(),
            exclude: Vec::new(),
            target_role: Some(GrandRole::Main),
        }],
        GrandChainPriorityItem::DeputyBraveChain => vec![
            GrandCardRule {
                slots: [
                    slot(RuleOwner::DeputyGrand, RuleKind::Command, RuleColor::Any),
                    slot(RuleOwner::DeputyGrand, RuleKind::Command, RuleColor::Any),
                    slot(RuleOwner::DeputyGrand, RuleKind::Np, RuleColor::Any),
                ],
                same_color: false,
                color_set_baq: true,
                include: Vec::new(),
                exclude: Vec::new(),
                target_role: Some(GrandRole::Deputy),
            },
            GrandCardRule {
                slots: [
                    slot(
                        RuleOwner::DeputyGrand,
                        RuleKind::Command,
                        RuleColor::Exact("b"),
                    ),
                    slot(
                        RuleOwner::DeputyGrand,
                        RuleKind::Command,
                        RuleColor::Exact("a"),
                    ),
                    slot(
                        RuleOwner::DeputyGrand,
                        RuleKind::Command,
                        RuleColor::Exact("q"),
                    ),
                ],
                same_color: false,
                color_set_baq: true,
                include: Vec::new(),
                exclude: vec![need(RuleOwner::DeputyGrand, RuleKind::Np)],
                target_role: Some(GrandRole::Deputy),
            },
        ],
        GrandChainPriorityItem::MainColorChain => vec![GrandCardRule {
            slots: [any_slot(), any_slot(), any_slot()],
            same_color: true,
            color_set_baq: false,
            include: vec![need(RuleOwner::MainGrand, RuleKind::Any)],
            exclude: Vec::new(),
            target_role: Some(GrandRole::Main),
        }],
        GrandChainPriorityItem::DeputyColorChain => vec![GrandCardRule {
            slots: [any_slot(), any_slot(), any_slot()],
            same_color: true,
            color_set_baq: false,
            include: vec![need(RuleOwner::DeputyGrand, RuleKind::Any)],
            exclude: Vec::new(),
            target_role: Some(GrandRole::Deputy),
        }],
        GrandChainPriorityItem::Fallback => vec![GrandCardRule {
            slots: [any_slot(), any_slot(), any_slot()],
            same_color: false,
            color_set_baq: false,
            include: Vec::new(),
            exclude: Vec::new(),
            target_role: None,
        }],
    }
}

pub(crate) fn saber_grand_card_rules(strategy: &GrandCardStrategy) -> Vec<GrandCardRule> {
    normalized_grand_chain_priority(strategy)
        .into_iter()
        .flat_map(saber_rules_for_priority_item)
        .collect()
}

pub(crate) fn berserker_grand_card_rules() -> Vec<GrandCardRule> {
    vec![
        GrandCardRule {
            slots: [
                prioritized_any_slot(RuleKind::Command, RuleColor::SameAsNp(GrandRole::Main)),
                with_preferred_np_owner(
                    with_np_as_command_owner(
                        prioritized_any_slot(
                            RuleKind::Command,
                            RuleColor::SameAsNp(GrandRole::Main),
                        ),
                        RuleOwner::DeputyGrand,
                    ),
                    RuleOwner::DeputyGrand,
                ),
                slot(RuleOwner::MainGrand, RuleKind::Np, RuleColor::Any),
            ],
            same_color: false,
            color_set_baq: false,
            include: Vec::new(),
            exclude: Vec::new(),
            target_role: Some(GrandRole::Main),
        },
        GrandCardRule {
            slots: [
                prioritized_any_slot(RuleKind::Any, RuleColor::Any),
                with_preferred_np_owner(
                    prioritized_any_slot(RuleKind::Any, RuleColor::Any),
                    RuleOwner::DeputyGrand,
                ),
                slot(RuleOwner::MainGrand, RuleKind::Np, RuleColor::Any),
            ],
            same_color: false,
            color_set_baq: false,
            include: Vec::new(),
            exclude: Vec::new(),
            target_role: Some(GrandRole::Main),
        },
        GrandCardRule {
            slots: [
                prioritized_any_slot(RuleKind::Command, RuleColor::SameAsNp(GrandRole::Deputy)),
                prioritized_any_slot(RuleKind::Command, RuleColor::SameAsNp(GrandRole::Deputy)),
                slot(RuleOwner::DeputyGrand, RuleKind::Np, RuleColor::Any),
            ],
            same_color: false,
            color_set_baq: false,
            include: Vec::new(),
            exclude: Vec::new(),
            target_role: Some(GrandRole::Deputy),
        },
        GrandCardRule {
            slots: [
                prioritized_any_slot(RuleKind::Any, RuleColor::Any),
                prioritized_any_slot(RuleKind::Any, RuleColor::Any),
                prioritized_any_slot(RuleKind::Any, RuleColor::Any),
            ],
            same_color: true,
            color_set_baq: false,
            include: vec![need(RuleOwner::MainGrand, RuleKind::Any)],
            exclude: vec![need(RuleOwner::MainGrand, RuleKind::Np)],
            target_role: Some(GrandRole::Main),
        },
        GrandCardRule {
            slots: [
                prioritized_any_slot(RuleKind::Any, RuleColor::Any),
                prioritized_any_slot(RuleKind::Any, RuleColor::Any),
                prioritized_any_slot(RuleKind::Any, RuleColor::Any),
            ],
            same_color: true,
            color_set_baq: false,
            include: vec![need(RuleOwner::DeputyGrand, RuleKind::Any)],
            exclude: vec![need(RuleOwner::DeputyGrand, RuleKind::Np)],
            target_role: Some(GrandRole::Deputy),
        },
        GrandCardRule {
            slots: [
                prioritized_any_slot(RuleKind::Any, RuleColor::Exact("b")),
                prioritized_any_slot(RuleKind::Any, RuleColor::Exact("a")),
                prioritized_any_slot(RuleKind::Any, RuleColor::Exact("q")),
            ],
            same_color: false,
            color_set_baq: true,
            include: vec![need(RuleOwner::AnyGrand, RuleKind::Any)],
            exclude: Vec::new(),
            target_role: None,
        },
        GrandCardRule {
            slots: [
                prioritized_any_slot(RuleKind::Any, RuleColor::Any),
                prioritized_any_slot(RuleKind::Any, RuleColor::Any),
                prioritized_any_slot(RuleKind::Any, RuleColor::Any),
            ],
            same_color: false,
            color_set_baq: false,
            include: Vec::new(),
            exclude: Vec::new(),
            target_role: None,
        },
    ]
}

pub(crate) fn grand_card_rules_for_class(
    grand_class: GrandClass,
    strategy: &GrandCardStrategy,
) -> Vec<GrandCardRule> {
    let mut rules = custom_grand_card_rules(strategy);
    let mut built_in_rules = match grand_class {
        GrandClass::Saber => saber_grand_card_rules(strategy),
        GrandClass::Berserker => berserker_grand_card_rules(),
    };
    rules.append(&mut built_in_rules);
    rules
}

pub(crate) fn choose_grand_auto_picks(
    candidates: &[AdvancedPickCandidate],
    grand_servants: &[GrandServantRuntimeConfig],
    strategy: &GrandCardStrategy,
    grand_class: GrandClass,
) -> Vec<Pick> {
    for rule in grand_card_rules_for_class(grand_class, strategy) {
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

pub(crate) fn choose_advanced_auto_picks_with_grand_class(
    scene: &AdvancedBattleScene,
    cards: &[CommandCardMatch],
    nps: &[NoblePhantasmMatch],
    party_ids: &[Option<u32>; 3],
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
        let color = if let Some(config) =
            servant_id.and_then(|id| grand_servants.iter().find(|config| config.servant_id == id))
        {
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

    for card in cards {
        let servant_index = card
            .servant_id
            .and_then(|id| party_ids.iter().position(|party_id| *party_id == Some(id)));
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
        } else if selected.len() == 3 {
            return choose_grand_auto_picks(
                &selected,
                grand_servants,
                grand_card_strategy,
                grand_class,
            );
        } else {
            sort_grand_picks(&mut selected, None, grand_servants);
        }
        return selected
            .into_iter()
            .map(|candidate| candidate.pick)
            .collect();
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
    grand_servants: &[GrandServantRuntimeConfig],
    grand_card_strategy: &GrandCardStrategy,
) -> Vec<Pick> {
    choose_advanced_auto_picks_with_grand_class(
        scene,
        cards,
        nps,
        party_ids,
        grand_servants,
        grand_card_strategy,
        GrandClass::Saber,
    )
}
