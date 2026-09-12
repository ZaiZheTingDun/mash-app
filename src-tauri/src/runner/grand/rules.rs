//! Grand and advanced card-rule modeling, matching, and scoring.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RuleOwner {
    MainGrand,
    DeputyGrand,
    AnyGrand,
    ExactSlot(usize),
    ExactServant(u32),
    ExactServantWithSupport(u32, bool),
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
            } else if slot_config.member_id.is_some() && slot_config.servant_id.is_some() {
                // Configurations with member metadata must follow the
                // servant through an in-battle Order Change. Keep the
                // legacy slot fallback below for older saved strategies.
                RuleOwner::ExactServantWithSupport(
                    slot_config.servant_id.unwrap(),
                    slot_config.is_support,
                )
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
        RuleOwner::ExactServantWithSupport(servant_id, is_support) => {
            candidate.servant_id == Some(servant_id) && candidate.is_support == is_support
        }
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

pub(crate) fn ordinary_advanced_rules(
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
