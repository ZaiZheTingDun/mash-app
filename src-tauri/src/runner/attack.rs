//! Normal and advanced attack pick helpers.
//!
//! This module owns command-card priority parsing, fallback command-card
//! selection, and non-Grand advanced startup conditions.

use super::*;
use crate::commands::settings::{
    consume_simulate_stuck_attack_selection, NoblePhantasmDetectionMode,
};
use std::collections::VecDeque;

const COMMAND_CARD_FRONTLINE_OWNER_FAILURE_LIMIT: u32 = 3;
const NP_GAUGE_GLOW_READY_THRESHOLD: f64 = 0.5;
const NP_GAUGE_MIN_VALID_SAMPLES: usize = 3;

// ---------------------------------------------------------------------------
// Attack pick logic
// ---------------------------------------------------------------------------

/// One card chosen by the priority walk / fill step. Carries enough info to
/// log + tap.
#[derive(Debug, Clone)]
pub(crate) enum Pick {
    Card {
        slot: u32,
        point: Point,
        servant_id: Option<u32>,
        suit: Option<String>,
        /// Source of the pick: priority entry text (e.g. ``"servant_2_arts"``)
        /// or ``None`` if it came from the leftmost-fill step.
        from_priority: Option<String>,
    },
    Np {
        slot: u32,
        point: Point,
        from_priority: String,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct AttackRetryPlan {
    picks: Vec<Pick>,
    cards: Vec<CommandCardMatch>,
    party_ids: [Option<u32>; 3],
}

/// Map ``"quick"|"arts"|"buster"`` to the suit code returned by the sidecar.
pub(crate) fn suit_code(suit: &str) -> Option<&'static str> {
    match suit {
        "quick" => Some("q"),
        "arts" => Some("a"),
        "buster" => Some("b"),
        _ => None,
    }
}

pub(crate) fn command_cards_log_meta(cards: &[CommandCardMatch]) -> Vec<AttackLogCommandCard> {
    cards
        .iter()
        .map(|card| AttackLogCommandCard {
            slot: card.slot,
            suit: card.suit.clone(),
            servant_id: card.servant_id,
            is_support: card.is_support,
            is_stunned: card.is_stunned,
        })
        .collect()
}

fn party_slot_matches_card(
    card: &CommandCardMatch,
    expected_id: u32,
    expected_support: bool,
) -> bool {
    card.servant_id == Some(expected_id) && card.is_support == expected_support
}

pub(crate) fn advanced_rule_matches(
    rule: &AdvancedRule,
    cards: &[CommandCardMatch],
    nps: &[NoblePhantasmMatch],
    party_ids: &[Option<u32>; 3],
    party_supports: &[bool; 3],
) -> bool {
    let np_matches = if rule.np_condition_groups.is_empty() {
        true
    } else {
        rule.np_condition_groups.iter().any(|group| {
            group
                .slots
                .iter()
                .all(|slot| np_condition_matches(slot, nps))
        })
    };
    let command_matches = if rule.command_condition_groups.is_empty() {
        true
    } else {
        rule.command_condition_groups.iter().any(|group| {
            group.cards.iter().all(|condition| {
                command_condition_matches(condition, cards, party_ids, party_supports)
            })
        })
    };
    np_matches && command_matches
}

pub(crate) fn should_retry_command_card_owner_detection(
    cards: &[CommandCardMatch],
    candidate_ids: &[u32],
    configured_frontline_count: usize,
) -> bool {
    configured_frontline_count >= 3
        && !candidate_ids.is_empty()
        && (cards.len() < COMMAND_CARD_COUNT
            || cards
                .iter()
                .any(|card| !card.is_stunned && card.servant_id.is_none()))
}

pub(crate) fn command_card_candidate_ids(ids: &[Option<u32>]) -> Vec<u32> {
    let mut candidates = Vec::with_capacity(ids.len());
    for id in ids.iter().flatten() {
        if !candidates.contains(id) {
            candidates.push(*id);
        }
    }
    candidates
}

pub(crate) fn command_cards_visible(cards: &[CommandCardMatch]) -> bool {
    cards.len() == COMMAND_CARD_COUNT
        && cards
            .iter()
            .all(|card| card.suit.is_some() && card.icon_region.is_some())
}

/// Return only command cards that can currently act. Unable-to-act cards stay
/// outside the priority pool and are considered only by the final positional
/// fallback when fewer than three actionable picks exist.
pub(crate) fn actionable_command_cards(cards: &[CommandCardMatch]) -> Vec<CommandCardMatch> {
    cards
        .iter()
        .filter(|card| !card.is_stunned)
        .cloned()
        .collect()
}

pub(crate) fn np_card_read_complete(nps: &[NoblePhantasmMatch]) -> bool {
    nps.len() == 3 && nps.iter().all(|np| np.card_ready.is_some())
}

pub(crate) fn np_gauge_read_complete(nps: &[NoblePhantasmMatch]) -> bool {
    nps.len() == 3 && nps.iter().all(|np| np.np_glow_score.is_some())
}

pub(crate) fn apply_np_detection_mode(
    nps: &mut [NoblePhantasmMatch],
    mode: NoblePhantasmDetectionMode,
) {
    match mode {
        NoblePhantasmDetectionMode::Card => {
            for np in nps {
                if let Some(card_ready) = np.card_ready {
                    np.ready = card_ready;
                    np.ready_source = Some("card".into());
                }
            }
        }
        NoblePhantasmDetectionMode::Gauge => {}
        NoblePhantasmDetectionMode::GaugeBeforeAttack => {}
    }
}

pub(crate) fn reads_np_gauge_before_attack(mode: NoblePhantasmDetectionMode) -> bool {
    matches!(mode, NoblePhantasmDetectionMode::GaugeBeforeAttack)
}

/// Aggregate one-second NP-gauge samples by slot.
///
/// The median rejects a single bright frame caused by a dialogue or visual
/// effect. The representative record is updated with the median score so its
/// readiness fields remain consistent with the value the runner uses.
pub(crate) fn aggregate_np_gauge_samples(
    samples: &[Vec<NoblePhantasmMatch>],
) -> Vec<NoblePhantasmMatch> {
    let mut slot_ids = Vec::new();
    for sample in samples {
        for slot in sample {
            if !slot_ids.contains(&slot.slot) {
                slot_ids.push(slot.slot);
            }
        }
    }
    slot_ids.sort_unstable();

    slot_ids
        .into_iter()
        .filter_map(|slot_id| {
            let mut readings: Vec<(f64, NoblePhantasmMatch)> = samples
                .iter()
                .filter_map(|sample| {
                    sample
                        .iter()
                        .find(|slot| slot.slot == slot_id)
                        .and_then(|slot| slot.np_glow_score.map(|score| (score, slot.clone())))
                })
                .collect();
            if readings.len() < NP_GAUGE_MIN_VALID_SAMPLES {
                return None;
            }

            readings.sort_by(|left, right| left.0.total_cmp(&right.0));
            let median = if readings.len() % 2 == 1 {
                readings[readings.len() / 2].0
            } else {
                let upper = readings.len() / 2;
                (readings[upper - 1].0 + readings[upper].0) / 2.0
            };
            let mut representative = readings[readings.len() / 2].1.clone();
            let ready = median >= NP_GAUGE_GLOW_READY_THRESHOLD;
            representative.ready = ready;
            representative.ready_source = Some("glow".into());
            representative.np_glow_score = Some(median);
            representative.np_glow_ready = Some(ready);
            Some(representative)
        })
        .collect()
}

pub(crate) fn np_condition_matches(
    condition: &crate::AdvancedNpSlotCondition,
    nps: &[NoblePhantasmMatch],
) -> bool {
    let Some(index) = parse_index(&condition.servant, "servant_") else {
        return false;
    };
    nps.iter()
        .find(|np| np.slot as usize == index)
        .map(|np| np.ready == condition.ready)
        .unwrap_or(false)
}

pub(crate) fn command_condition_matches(
    condition: &AdvancedCommandCardCondition,
    cards: &[CommandCardMatch],
    party_ids: &[Option<u32>; 3],
    party_supports: &[bool; 3],
) -> bool {
    let Some(card) = cards.iter().find(|card| card.slot == condition.slot) else {
        return false;
    };

    if condition.servant != "any" {
        let Some(index) = parse_index(&condition.servant, "servant_") else {
            return false;
        };
        let Some(expected_id) = party_ids.get(index).copied().flatten() else {
            return false;
        };
        if !party_slot_matches_card(card, expected_id, party_supports[index]) {
            return false;
        }
    }

    if condition.suit != "any" {
        let Some(expected_suit) = suit_code(&condition.suit) else {
            return false;
        };
        if card.suit.as_deref() != Some(expected_suit) {
            return false;
        }
    }

    if let Some(min_crit) = condition.min_crit_chance {
        if card.crit_chance.unwrap_or(0) < min_crit {
            return false;
        }
    }

    true
}

pub(crate) fn command_condition_matches_card(
    condition: &AdvancedCommandCardCondition,
    card: &CommandCardMatch,
    party_ids: &[Option<u32>; 3],
    party_supports: &[bool; 3],
) -> bool {
    if condition.servant != "any" {
        let Some(index) = parse_index(&condition.servant, "servant_") else {
            return false;
        };
        let Some(expected_id) = party_ids.get(index).copied().flatten() else {
            return false;
        };
        if !party_slot_matches_card(card, expected_id, party_supports[index]) {
            return false;
        }
    }

    if condition.suit != "any" {
        let Some(expected_suit) = suit_code(&condition.suit) else {
            return false;
        };
        if card.suit.as_deref() != Some(expected_suit) {
            return false;
        }
    }

    if let Some(min_crit) = condition.min_crit_chance {
        if card.crit_chance.unwrap_or(0) < min_crit {
            return false;
        }
    }

    true
}

/// Walk the first three priority rows as fixed chain slots, then fill missing
/// slots from remaining fallback priorities. Empty fixed slots inherit the
/// previous non-NP fixed slot, so a partial ``NP, All, empty`` chain still
/// tries to fill the third card from the ``All`` rule. Fallback rows are
/// repeated while they can still match, before moving to the next priority.
/// Cards / NPs already picked are tracked in `used_card_slots` /
/// `used_np_slots` and will not be re-selected.
pub(crate) fn pick_by_priority(
    priority: &[AttackCard],
    cards: &[CommandCardMatch],
    nps: &[NoblePhantasmMatch],
    party_ids: &[Option<u32>; 3],
    party_supports: &[bool; 3],
    used_card_slots: &mut HashSet<u32>,
    used_np_slots: &mut HashSet<u32>,
) -> Vec<Pick> {
    let mut picks: Vec<Option<Pick>> = (0..3).map(|_| None).collect();
    let mut inherited_fixed_card: Option<String> = None;

    for (idx, entry) in priority.iter().take(3).enumerate() {
        let card_str = entry.card.as_deref().or(inherited_fixed_card.as_deref());
        if let Some(card_str) = card_str {
            if let Some(pick) = pick_one_priority(
                card_str,
                cards,
                nps,
                party_ids,
                party_supports,
                used_card_slots,
                used_np_slots,
            ) {
                picks[idx] = Some(pick);
            }
        }
        if let Some(card) = entry.card.as_deref() {
            if !priority_card_is_np(card) && parse_priority_card(card).is_some() {
                inherited_fixed_card = Some(card.to_string());
            }
        }
    }

    for entry in priority.iter().skip(3) {
        let Some(card_str) = entry.card.as_deref() else {
            continue;
        };
        while let Some(slot_idx) = picks.iter().position(Option::is_none) {
            let Some(pick) = pick_one_priority(
                card_str,
                cards,
                nps,
                party_ids,
                party_supports,
                used_card_slots,
                used_np_slots,
            ) else {
                break;
            };
            picks[slot_idx] = Some(pick);
            if priority_card_is_np(card_str) {
                break;
            }
        }
    }

    let fixed_len = priority.len().min(3);
    fill_empty_pick_slots(&mut picks[..fixed_len], cards, used_card_slots);
    picks.into_iter().flatten().collect()
}

pub(crate) fn parse_priority_card(card_str: &str) -> Option<(usize, &str)> {
    let rest = card_str.strip_prefix("servant_")?;
    let (idx_str, kind) = rest.split_once('_')?;
    let field_pos = idx_str.parse::<usize>().ok()?;
    if !(1..=3).contains(&field_pos) {
        return None;
    }
    Some((field_pos, kind))
}

pub(crate) fn priority_card_is_np(card_str: &str) -> bool {
    parse_priority_card(card_str)
        .map(|(_, kind)| kind == "np")
        .unwrap_or(false)
}

pub(crate) fn pick_one_priority(
    card_str: &str,
    cards: &[CommandCardMatch],
    nps: &[NoblePhantasmMatch],
    party_ids: &[Option<u32>; 3],
    party_supports: &[bool; 3],
    used_card_slots: &mut HashSet<u32>,
    used_np_slots: &mut HashSet<u32>,
) -> Option<Pick> {
    let (field_pos, kind) = parse_priority_card(card_str)?;

    if kind == "np" {
        let np_slot = (field_pos - 1) as u32;
        if used_np_slots.contains(&np_slot) {
            return None;
        }
        let np = nps.iter().find(|n| n.slot == np_slot && n.ready)?;
        used_np_slots.insert(np_slot);
        return Some(Pick::Np {
            slot: np_slot,
            point: rect_center(&np.card_region),
            from_priority: card_str.to_string(),
        });
    }

    let wanted_id = party_ids[field_pos - 1]?;
    let wanted_support = party_supports[field_pos - 1];
    let wanted_suit = if kind == "all" {
        None
    } else {
        Some(suit_code(kind)?)
    };

    let mut best: Option<&CommandCardMatch> = None;
    for c in cards {
        if c.is_stunned || used_card_slots.contains(&c.slot) {
            continue;
        }
        if !party_slot_matches_card(c, wanted_id, wanted_support) {
            continue;
        }
        if let Some(wanted_suit) = wanted_suit {
            if c.suit.as_deref() != Some(wanted_suit) {
                continue;
            }
        } else if c.suit.is_none() {
            continue;
        }
        if best.map_or(true, |b| c.slot < b.slot) {
            best = Some(c);
        }
    }

    let c = best?;
    used_card_slots.insert(c.slot);
    Some(Pick::Card {
        slot: c.slot,
        point: Point::new(c.x, c.y),
        servant_id: c.servant_id,
        suit: c.suit.clone(),
        from_priority: Some(card_str.to_string()),
    })
}

/// Fill still-empty fixed-chain positions with the leftmost remaining command
/// cards, preserving the user's configured three-card order. Actionable cards
/// are considered before unavailable cards.
pub(crate) fn fill_empty_pick_slots(
    picks: &mut [Option<Pick>],
    cards: &[CommandCardMatch],
    used_card_slots: &mut HashSet<u32>,
) {
    let mut sorted: Vec<&CommandCardMatch> = cards.iter().collect();
    sorted.sort_by_key(|c| (c.is_stunned, c.slot));

    let mut next_card_idx = 0;
    for pick in picks.iter_mut().filter(|pick| pick.is_none()) {
        while next_card_idx < sorted.len() && used_card_slots.contains(&sorted[next_card_idx].slot)
        {
            next_card_idx += 1;
        }
        if next_card_idx >= sorted.len() {
            break;
        }

        let c = sorted[next_card_idx];
        used_card_slots.insert(c.slot);
        *pick = Some(Pick::Card {
            slot: c.slot,
            point: Point::new(c.x, c.y),
            servant_id: c.servant_id,
            suit: c.suit.clone(),
            from_priority: None,
        });
        next_card_idx += 1;
    }
}

/// After the priority walk, top picks up to 3 by choosing the leftmost
/// command cards that have not yet been used. Actionable cards are considered
/// before unavailable cards.
pub(crate) fn fill_remaining(
    picks: &mut Vec<Pick>,
    cards: &[CommandCardMatch],
    used_card_slots: &mut HashSet<u32>,
) {
    let mut sorted: Vec<&CommandCardMatch> = cards.iter().collect();
    sorted.sort_by_key(|c| (c.is_stunned, c.slot));
    for c in sorted {
        if picks.len() >= 3 {
            break;
        }
        if used_card_slots.contains(&c.slot) {
            continue;
        }
        used_card_slots.insert(c.slot);
        picks.push(Pick::Card {
            slot: c.slot,
            point: Point::new(c.x, c.y),
            servant_id: c.servant_id,
            suit: c.suit.clone(),
            from_priority: None,
        });
    }
}

/// Build positional command-card fallbacks that are not already reserved by
/// the planned chain. These are consumed when a planned NP opens the game's
/// "cannot use Noble Phantasm" dialog, so the runner can still submit three
/// actual selections without tapping the same command card twice.
pub(crate) fn replacement_command_card_picks(
    picks: &[Pick],
    cards: &[CommandCardMatch],
) -> VecDeque<Pick> {
    let reserved_slots: HashSet<u32> = picks
        .iter()
        .filter_map(|pick| match pick {
            Pick::Card { slot, .. } => Some(*slot),
            Pick::Np { .. } => None,
        })
        .collect();
    let mut candidates: Vec<&CommandCardMatch> = cards
        .iter()
        .filter(|card| !reserved_slots.contains(&card.slot))
        .collect();
    candidates.sort_by_key(|card| (card.is_stunned, card.slot));
    candidates
        .into_iter()
        .map(|card| Pick::Card {
            slot: card.slot,
            point: Point::new(card.x, card.y),
            servant_id: card.servant_id,
            suit: card.suit.clone(),
            from_priority: Some("宝具不可用补位".into()),
        })
        .collect()
}

/// Refresh a cached retry plan against the current NP readiness state. A
/// retry can happen after the original NP read became stale, so unavailable
/// NP picks are replaced in-place with unused command cards.
pub(crate) fn refresh_retry_picks_for_np_state(
    picks: &[Pick],
    cards: &[CommandCardMatch],
    nps: &[NoblePhantasmMatch],
) -> Vec<Pick> {
    let mut replacements = replacement_command_card_picks(picks, cards);
    let mut refreshed = Vec::with_capacity(picks.len());

    for pick in picks {
        match pick {
            Pick::Np { slot, .. }
                if !nps.iter().any(|np| np.slot == *slot && np.ready) =>
            {
                if let Some(replacement) = replacements.pop_front() {
                    refreshed.push(replacement);
                }
            }
            _ => refreshed.push(pick.clone()),
        }
    }

    refreshed
}

pub(crate) fn advanced_startup_conditions_match(
    scene: &AdvancedBattleScene,
    cards: &[CommandCardMatch],
    party_ids: &[Option<u32>; 3],
    party_supports: &[bool; 3],
) -> bool {
    let active: Vec<&AdvancedCommandCardCondition> = scene
        .command_conditions
        .iter()
        .filter(|condition| condition.servant != "any" || condition.suit != "any")
        .collect();
    if active.is_empty() {
        return true;
    }
    let mut used_slots = HashSet::new();
    startup_conditions_match_from(
        0,
        &active,
        cards,
        party_ids,
        party_supports,
        &mut used_slots,
    )
}

pub(crate) fn has_advanced_startup_conditions(scene: &AdvancedBattleScene) -> bool {
    scene
        .command_conditions
        .iter()
        .any(|condition| condition.servant != "any" || condition.suit != "any")
}

pub(crate) fn grand_startup_can_run_before_attack(
    scene: &AdvancedBattleScene,
    grand_servants: &[GrandServantRuntimeConfig],
) -> bool {
    if grand_servants.is_empty()
        || !(scene.rules.is_empty() || uses_advanced_strategy_flow(scene))
        || has_advanced_startup_conditions(scene)
    {
        return false;
    }
    let backline_main_needs_swap = scene.grand_auto_order_change == Some(true)
        && grand_servants
            .first()
            .is_some_and(|main| main.slot_index >= 3);
    !backline_main_needs_swap
}

pub(crate) fn startup_conditions_match_from(
    condition_index: usize,
    conditions: &[&AdvancedCommandCardCondition],
    cards: &[CommandCardMatch],
    party_ids: &[Option<u32>; 3],
    party_supports: &[bool; 3],
    used_slots: &mut HashSet<u32>,
) -> bool {
    if condition_index >= conditions.len() {
        return true;
    }

    let condition = conditions[condition_index];
    for card in cards {
        if used_slots.contains(&card.slot) {
            continue;
        }
        if !command_condition_matches_card(condition, card, party_ids, party_supports) {
            continue;
        }
        used_slots.insert(card.slot);
        if startup_conditions_match_from(
            condition_index + 1,
            conditions,
            cards,
            party_ids,
            party_supports,
            used_slots,
        ) {
            return true;
        }
        used_slots.remove(&card.slot);
    }
    false
}

pub(crate) fn uses_advanced_strategy_flow(scene: &AdvancedBattleScene) -> bool {
    scene.main_output.is_some()
        || scene.grand_auto_order_change == Some(true)
        || !scene.command_conditions.is_empty()
        || !scene.control_actions.is_empty()
        || !scene.turns.is_empty()
        || !scene.startup_actions.is_empty()
}

pub(crate) fn normal_turn_for_current_state(
    scenes: &[BattleScene],
    current_scene_index: usize,
    current_turn_index: usize,
) -> Option<(BattleTurn, bool)> {
    let turns = &scenes.get(current_scene_index)?.turns;
    let last_index = turns.len().checked_sub(1)?;
    let over_configured_turns = current_turn_index > last_index;
    let effective_index = current_turn_index.min(last_index);
    turns
        .get(effective_index)
        .cloned()
        .map(|turn| (turn, over_configured_turns))
}

pub(crate) fn advanced_enemy_target_for_current_scene(
    scenes: &[AdvancedBattleScene],
    current_scene_index: usize,
    should_select: bool,
) -> Option<&str> {
    if !should_select {
        return None;
    }
    scenes
        .get(current_scene_index)
        .and_then(|scene| scene.enemy_target.as_deref())
}

pub(crate) fn attack_priority_for_current_scene<'a>(
    advanced_mode: bool,
    scene_config_used: bool,
    scenes: &'a [BattleScene],
    current_scene_index: usize,
    current_turn_index: usize,
) -> Option<&'a [AttackCard]> {
    if advanced_mode && !scene_config_used {
        return None;
    }
    let turns = &scenes.get(current_scene_index)?.turns;
    let last_index = turns.len().checked_sub(1)?;
    let effective_index = current_turn_index.min(last_index);
    scenes
        .get(current_scene_index)
        .and_then(|scene| scene.turns.get(effective_index))
        .map(|turn| turn.attack_priority.as_slice())
}

pub(crate) fn attack_card_requires_command_card_recognition(card: &AttackCard) -> bool {
    card.card
        .as_deref()
        .and_then(parse_priority_card)
        .map(|(_, kind)| kind != "np")
        .unwrap_or(false)
}

pub(crate) fn normal_scenes_need_command_card_recognition(scenes: &[BattleScene]) -> bool {
    scenes.iter().any(|scene| {
        scene.turns.iter().any(|turn| {
            turn.attack_mode != AttackMode::Normal
                || turn
                    .attack_priority
                    .iter()
                    .any(attack_card_requires_command_card_recognition)
        })
    })
}

pub(crate) fn normal_turn_for_current_scene(
    scenes: &[BattleScene],
    current_scene_index: usize,
    current_turn_index: usize,
) -> Option<&BattleTurn> {
    let turns = &scenes.get(current_scene_index)?.turns;
    let last_index = turns.len().checked_sub(1)?;
    turns.get(current_turn_index.min(last_index))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CriticalPickSummary {
    pub(crate) chain: Option<CriticalChainType>,
    pub(crate) relaxed_alternation: bool,
    pub(crate) relaxed_reason: Option<&'static str>,
    pub(crate) owner_labels: Vec<String>,
}

fn critical_chain_rank(
    cards: [&CommandCardMatch; 3],
    priority: &[CriticalChainType],
) -> (usize, Option<CriticalChainType>) {
    let counts = cards
        .iter()
        .fold((0usize, 0usize, 0usize), |mut counts, card| {
            match card.suit.as_deref() {
                Some("b") => counts.0 += 1,
                Some("a") => counts.1 += 1,
                Some("q") => counts.2 += 1,
                _ => {}
            }
            counts
        });
    let chain = match counts {
        (1, 1, 1) => Some(CriticalChainType::Mighty),
        (3, 0, 0) => Some(CriticalChainType::Buster),
        (0, 3, 0) => Some(CriticalChainType::Arts),
        (0, 0, 3) => Some(CriticalChainType::Quick),
        _ => None,
    };
    let rank = chain
        .and_then(|chain| priority.iter().position(|candidate| *candidate == chain))
        .unwrap_or(priority.len());
    (rank, chain)
}

fn member_priority_rank(
    card: &CommandCardMatch,
    member: Option<&PartyMemberRuntime>,
    priority: &[AttackMemberPriorityItem],
) -> usize {
    priority
        .iter()
        .position(|configured| {
            member.is_some_and(|member| {
                configured
                    .member_id
                    .as_deref()
                    .zip(member.member_id.as_deref())
                    .is_some_and(|(left, right)| left == right)
                    || (configured.slot_index as usize == member.slot_index
                        && configured.servant_id == Some(member.servant_id)
                        && configured.is_support == member.is_support)
            }) || (configured.servant_id == card.servant_id
                && configured.is_support == card.is_support)
        })
        .unwrap_or_else(|| {
            priority.len()
                + member
                    .map(|member| member.slot_index)
                    .unwrap_or(usize::MAX / 2)
        })
}

fn command_card_member<'a>(
    card: &CommandCardMatch,
    members: &'a [Option<PartyMemberRuntime>; 3],
) -> Option<&'a PartyMemberRuntime> {
    let servant_id = card.servant_id?;
    members
        .iter()
        .flatten()
        .find(|member| member.servant_id == servant_id && member.is_support == card.is_support)
}

fn command_card_owner_key(
    card: &CommandCardMatch,
    members: &[Option<PartyMemberRuntime>; 3],
) -> Option<String> {
    let member = command_card_member(card, members)?;
    Some(
        member
            .member_id
            .clone()
            .unwrap_or_else(|| format!("{}:{}", member.is_support, member.servant_id)),
    )
}

fn normalized_critical_chain_priority(configured: &[CriticalChainType]) -> Vec<CriticalChainType> {
    let defaults = [
        CriticalChainType::Mighty,
        CriticalChainType::Buster,
        CriticalChainType::Arts,
        CriticalChainType::Quick,
    ];
    let mut priority = Vec::with_capacity(defaults.len());
    for chain in configured {
        if defaults.contains(chain) && !priority.contains(chain) {
            priority.push(*chain);
        }
    }
    for chain in defaults {
        if !priority.contains(&chain) {
            priority.push(chain);
        }
    }
    priority
}

pub(crate) fn choose_critical_picks(
    cards: &[CommandCardMatch],
    members: &[Option<PartyMemberRuntime>; 3],
    strategy: &CriticalAttackStrategy,
) -> (Vec<Pick>, CriticalPickSummary) {
    let mut actionable: Vec<&CommandCardMatch> =
        cards.iter().filter(|card| !card.is_stunned).collect();
    actionable.sort_by_key(|card| card.slot);
    let chain_priority = normalized_critical_chain_priority(&strategy.chain_priority);

    if actionable.len() < 3 {
        let picks = actionable
            .into_iter()
            .map(|card| Pick::Card {
                slot: card.slot,
                point: Point::new(card.x, card.y),
                servant_id: card.servant_id,
                suit: card.suit.clone(),
                from_priority: Some("暴击模式补位".into()),
            })
            .collect();
        return (
            picks,
            CriticalPickSummary {
                chain: None,
                relaxed_alternation: true,
                relaxed_reason: Some("可行动卡不足"),
                owner_labels: Vec::new(),
            },
        );
    }

    #[derive(Clone)]
    struct Candidate<'a> {
        cards: [&'a CommandCardMatch; 3],
        owners: [Option<String>; 3],
        chain: Option<CriticalChainType>,
        score: (usize, [usize; 3], std::cmp::Reverse<u32>, [u32; 3]),
    }

    let mut candidates = Vec::new();
    for first in 0..actionable.len() {
        for second in 0..actionable.len() {
            if second == first {
                continue;
            }
            for third in 0..actionable.len() {
                if third == first || third == second {
                    continue;
                }
                let ordered = [actionable[first], actionable[second], actionable[third]];
                let owners = ordered.map(|card| command_card_owner_key(card, members));
                let (chain_rank, chain) = critical_chain_rank(ordered, &chain_priority);
                let member_ranks = ordered.map(|card| {
                    member_priority_rank(
                        card,
                        command_card_member(card, members),
                        &strategy.member_priority,
                    )
                });
                let crit_total = ordered
                    .iter()
                    .map(|card| card.crit_chance.unwrap_or(0))
                    .sum::<u32>();
                let slots = ordered.map(|card| card.slot);
                candidates.push(Candidate {
                    cards: ordered,
                    owners,
                    chain,
                    score: (
                        chain_rank,
                        member_ranks,
                        std::cmp::Reverse(crit_total),
                        slots,
                    ),
                });
            }
        }
    }

    let alternating = |candidate: &Candidate<'_>| {
        candidate.owners.iter().all(Option::is_some)
            && candidate.owners[0] != candidate.owners[1]
            && candidate.owners[1] != candidate.owners[2]
    };
    let has_alternating = candidates.iter().any(alternating);
    let relaxed_reason = if has_alternating {
        None
    } else if actionable
        .iter()
        .any(|card| command_card_owner_key(card, members).is_none())
    {
        Some("成员识别失败")
    } else {
        Some("手牌只有一个成员")
    };
    let best = candidates
        .into_iter()
        .filter(|candidate| !has_alternating || alternating(candidate))
        .min_by_key(|candidate| candidate.score.clone())
        .expect("three actionable cards always produce a permutation");
    let owner_labels = best
        .cards
        .iter()
        .map(|card| {
            let support = if card.is_support { "助战" } else { "自有" };
            card.servant_id
                .map(|id| format!("{support}#{id}"))
                .unwrap_or_else(|| "未识别成员".into())
        })
        .collect();
    let picks = best
        .cards
        .into_iter()
        .map(|card| Pick::Card {
            slot: card.slot,
            point: Point::new(card.x, card.y),
            servant_id: card.servant_id,
            suit: card.suit.clone(),
            from_priority: Some("暴击模式".into()),
        })
        .collect();
    (
        picks,
        CriticalPickSummary {
            chain: best.chain,
            relaxed_alternation: !has_alternating,
            relaxed_reason,
            owner_labels,
        },
    )
}

/// Center of a normalized rectangle (used to derive a tap point from a
/// detected NP slot's `card_region`).
pub(crate) fn rect_center(r: &NormRect) -> Point {
    Point::new(r.x + r.w / 2.0, r.y + r.h / 2.0)
}

impl Runner {
    pub(crate) fn execute_next_grand_turn_skills(&mut self, scene: &AdvancedBattleScene) -> bool {
        let scene_index = self.battle.current_scene_index;
        if !self.battle.advanced_startup_done.contains(&scene_index) {
            return true;
        }
        let completed_turn_count = *self
            .battle
            .advanced_turn_indices
            .get(&scene_index)
            .unwrap_or(&0);
        let Some(turn_actions) = advanced_turn_actions(scene, completed_turn_count) else {
            return true;
        };
        let executed_control_count = *self
            .battle
            .advanced_control_indices
            .get(&scene_index)
            .unwrap_or(&0);
        let startup_control_count = *self
            .battle
            .advanced_startup_control_indices
            .get(&scene_index)
            .unwrap_or(&executed_control_count);
        let active_actions = advanced_startup_flow_actions(
            scene,
            executed_control_count,
            startup_control_count,
            completed_turn_count,
            self.battle.advanced_auto_order_changes.get(&scene_index),
        );
        let (resolved_actions, _) = self.advanced_actions_and_members_after(
            active_actions.into_iter(),
            turn_actions.iter().cloned(),
        );
        let turn_number = completed_turn_count + 1;

        if !resolved_actions.is_empty() {
            self.emit("Battle", &format!("执行 Turn {turn_number} 技能"));
            let turn = BattleTurn {
                id: format!("{}_turn_{turn_number}", scene.id),
                preparation_actions: resolved_actions,
                servant_actions: Vec::new(),
                equipment_actions: Vec::new(),
                command_spell_actions: Vec::new(),
                enemy_target: None,
                attack_priority: Vec::new(),
                attack_mode: AttackMode::Normal,
                critical_strategy: CriticalAttackStrategy::default(),
                advanced_card_strategy: AdvancedCardStrategy::default(),
            };
            if !self.execute_turn_skills(&turn) {
                return false;
            }
        }

        self.battle
            .advanced_turn_indices
            .insert(scene_index, turn_number);
        true
    }

    pub(crate) fn prepare_grand_startup_before_attack(
        &mut self,
        scene: &AdvancedBattleScene,
    ) -> bool {
        let scene_index = self.battle.current_scene_index;
        if self.battle.advanced_startup_done.contains(&scene_index) {
            return true;
        }
        let executed_control_count = *self
            .battle
            .advanced_control_indices
            .get(&scene_index)
            .unwrap_or(&0);
        let next_control_count = if executed_control_count < scene.control_actions.len() {
            executed_control_count + 1
        } else {
            executed_control_count
        };
        if executed_control_count < scene.control_actions.len() {
            self.emit(
                "Battle",
                &format!("无启动条件，执行本回合控制行动 {next_control_count}"),
            );
        } else {
            self.emit("Battle", "无启动条件，直接执行 Turn 1 技能");
        }
        let first_turn_actions = advanced_turn_actions(scene, 0).unwrap_or_default();
        let pending_actions = scene
            .control_actions
            .iter()
            .skip(executed_control_count)
            .take(next_control_count.saturating_sub(executed_control_count))
            .chain(first_turn_actions.iter())
            .cloned();
        let (startup_actions, _) = self.advanced_actions_and_members_after(
            scene
                .control_actions
                .iter()
                .take(executed_control_count)
                .cloned(),
            pending_actions,
        );
        self.battle
            .advanced_control_indices
            .insert(scene_index, next_control_count);
        self.battle
            .advanced_startup_control_indices
            .insert(scene_index, next_control_count);
        self.battle.advanced_startup_done.insert(scene_index);
        self.battle.advanced_turn_indices.insert(scene_index, 1);

        if startup_actions.is_empty() {
            return true;
        }
        let prep_turn = BattleTurn {
            id: scene.id.clone(),
            preparation_actions: startup_actions,
            servant_actions: Vec::new(),
            equipment_actions: Vec::new(),
            command_spell_actions: Vec::new(),
            enemy_target: None,
            attack_priority: Vec::new(),
            attack_mode: AttackMode::Normal,
            critical_strategy: CriticalAttackStrategy::default(),
            advanced_card_strategy: AdvancedCardStrategy::default(),
        };
        if !self.execute_turn_skills(&prep_turn) {
            return false;
        }
        self.emit("Battle", "Turn 1 技能完成，进入自动战斗");
        true
    }

    pub(crate) fn handle_attack(&mut self) {
        if !self.ensure_battle_speed_level_two() {
            return;
        }

        if self.advanced_mode {
            let party_ids = self
                .advanced_scenes
                .get(self.battle.current_scene_index)
                .filter(|scene| scene.rules.is_empty() || uses_advanced_strategy_flow(scene))
                .map(|scene| self.advanced_current_party_ids(scene))
                .unwrap_or_else(|| self.build_party_ids());
            let party_supports = self
                .advanced_scenes
                .get(self.battle.current_scene_index)
                .filter(|scene| scene.rules.is_empty() || uses_advanced_strategy_flow(scene))
                .map(|scene| {
                    let scene_index = self.battle.current_scene_index;
                    let executed_control_count = *self
                        .battle
                        .advanced_control_indices
                        .get(&scene_index)
                        .unwrap_or(&0);
                    if self.battle.advanced_startup_done.contains(&scene_index) {
                        let startup_control_count = *self
                            .battle
                            .advanced_startup_control_indices
                            .get(&scene_index)
                            .unwrap_or(&executed_control_count);
                        self.advanced_party_supports_after_startup_flow(
                            scene,
                            executed_control_count,
                            startup_control_count,
                        )
                    } else {
                        self.advanced_party_supports_after_control(scene, executed_control_count)
                    }
                })
                .unwrap_or_else(|| {
                    let (_, supports) = frontline_party_ids_and_supports(&frontline_party_members(
                        &self.build_full_party_members(),
                    ));
                    supports
                });
            let Some((cards, nps)) = self.read_attack_state(&party_ids, true, false) else {
                return;
            };
            self.handle_advanced_attack(cards, nps, party_ids, party_supports);
            return;
        }

        let party_members = self.normal_current_party_members();
        let party_ids = std::array::from_fn(|index| {
            party_members[index]
                .as_ref()
                .map(|member| member.servant_id)
        });
        let party_supports = std::array::from_fn(|index| {
            party_members[index]
                .as_ref()
                .map(|member| member.is_support)
                .unwrap_or(false)
        });
        let current_turn = normal_turn_for_current_scene(
            &self.scenes,
            self.battle.current_scene_index,
            self.battle.current_turn_index,
        )
        .cloned();
        let recognize_command_cards = normal_scenes_need_command_card_recognition(&self.scenes);
        let tolerate_missing_owners = current_turn
            .as_ref()
            .is_some_and(|turn| turn.attack_mode == AttackMode::Critical);
        let Some((cards, nps)) =
            self.read_attack_state(&party_ids, recognize_command_cards, tolerate_missing_owners)
        else {
            return;
        };
        match current_turn
            .as_ref()
            .map(|turn| turn.attack_mode)
            .unwrap_or_default()
        {
            AttackMode::Normal => {
                self.emit("Attack", "普通模式：按配置的攻击优先级选卡");
                self.pick_and_tap_attack_cards(&cards, &nps, &party_ids, &party_supports, None);
            }
            AttackMode::Critical => {
                let strategy = current_turn
                    .as_ref()
                    .map(|turn| &turn.critical_strategy)
                    .cloned()
                    .unwrap_or_default();
                let (picks, summary) = choose_critical_picks(&cards, &party_members, &strategy);
                let chain = match summary.chain {
                    Some(CriticalChainType::Mighty) => "精湛连携",
                    Some(CriticalChainType::Buster) => "力击连携",
                    Some(CriticalChainType::Arts) => "技击连携",
                    Some(CriticalChainType::Quick) => "迅击连携",
                    None => "无连携",
                };
                self.emit(
                    "Attack",
                    &format!(
                        "暴击模式：{chain}，成员顺序 {}",
                        if summary.owner_labels.is_empty() {
                            "待补位".into()
                        } else {
                            summary.owner_labels.join(" → ")
                        }
                    ),
                );
                if summary.relaxed_alternation {
                    self.emit_warn(
                        "Attack",
                        &format!(
                            "暴击模式因{}，放宽相邻成员限制并补足选卡",
                            summary.relaxed_reason.unwrap_or("无法确认可交错的三张卡")
                        ),
                    );
                }
                self.tap_picks("Attack", &picks, &cards, &party_ids);
            }
            AttackMode::Advanced => {
                let strategy = current_turn
                    .as_ref()
                    .map(|turn| &turn.advanced_card_strategy)
                    .cloned()
                    .unwrap_or_default();
                let (picks, matched_rule) =
                    choose_ordinary_advanced_picks(&cards, &nps, &party_members, &strategy);
                if let Some(rule) = matched_rule {
                    self.emit("Attack", &format!("高级模式：命中 {rule}"));
                } else {
                    self.emit("Attack", "高级模式：无规则命中，按可行动卡从左至右补位");
                }
                self.tap_picks("Attack", &picks, &cards, &party_ids);
            }
        }
    }

    fn ensure_battle_speed_level_two(&mut self) -> bool {
        let mut switch_requested = false;
        loop {
            if self.is_cancelled() {
                return false;
            }
            let speed_two = match self.sidecar().find_element_by_name(
                None,
                ATTACK_SCREEN,
                ATTACK_SCREEN_SPEED_2_ELEMENT,
            ) {
                Ok(matched) => matched.found,
                Err(err) => {
                    self.fail_action("Attack", "检测战斗速度", err);
                    return false;
                }
            };
            if speed_two {
                return true;
            }
            let speed_one = match self.sidecar().find_element_by_name(
                None,
                ATTACK_SCREEN,
                ATTACK_SCREEN_SPEED_1_ELEMENT,
            ) {
                Ok(matched) => matched.found,
                Err(err) => {
                    self.fail_action("Attack", "检测战斗速度", err);
                    return false;
                }
            };
            if speed_one && !switch_requested {
                self.emit("Attack", "检测到战斗速度为一级，现在切换为二级。");
                if !self.tap_at("Attack", ATTACK_SCREEN_SPEED_BUTTON) {
                    return false;
                }
                switch_requested = true;
            }
            thread::sleep(SKILL_POLL_INTERVAL);
        }
    }

    pub(crate) fn advanced_current_party_ids(
        &self,
        scene: &AdvancedBattleScene,
    ) -> [Option<u32>; 3] {
        let scene_index = self.battle.current_scene_index;
        let executed_control_count = *self
            .battle
            .advanced_control_indices
            .get(&scene_index)
            .unwrap_or(&0);
        if self.battle.advanced_startup_done.contains(&scene_index) {
            let startup_control_count = *self
                .battle
                .advanced_startup_control_indices
                .get(&scene_index)
                .unwrap_or(&executed_control_count);
            self.advanced_party_ids_after_startup_flow(
                scene,
                executed_control_count,
                startup_control_count,
            )
        } else {
            self.advanced_party_ids_after_control(scene, executed_control_count)
        }
    }

    pub(crate) fn read_attack_state(
        &mut self,
        party_ids: &[Option<u32>; 3],
        recognize_command_cards: bool,
        tolerate_missing_owners: bool,
    ) -> Option<(Vec<CommandCardMatch>, Vec<NoblePhantasmMatch>)> {
        // Start with the expected front line for speed. If owner recognition
        // repeatedly fails, a servant probably died and a back-line member
        // moved forward, so broaden the template candidates to the full team.
        let full_candidate_ids = command_card_candidate_ids(&self.build_full_party_ids());
        let configured_frontline_count = party_ids.iter().flatten().count();
        let mut candidate_ids = if self.battle.command_card_owner_fallback_to_full_party {
            full_candidate_ids.clone()
        } else {
            command_card_candidate_ids(party_ids)
        };

        let cards = if recognize_command_cards {
            self.emit_attack(
                &format!("指令卡候选从者: {:?}", candidate_ids),
                AttackLogMeta {
                    front_servant_ids: *party_ids,
                    candidate_servant_ids: Some(candidate_ids.clone()),
                    command_cards: None,
                    ready_np_slots: None,
                    selected_pick: None,
                },
            );

            if self.assets_dir.is_none() {
                self.emit("Attack", "未找到从者资源目录，将无法按从者匹配指令卡");
            }

            let assets_dir = self.assets_dir.clone();
            loop {
                let used_full_party_candidates =
                    self.battle.command_card_owner_fallback_to_full_party;
                let cards = match self.sidecar().find_command_cards(
                    None,
                    None,
                    &candidate_ids,
                    assets_dir.as_deref(),
                ) {
                    Ok(c) => c,
                    Err(err) => {
                        self.fail_action("Attack", "识别指令卡", err);
                        return None;
                    }
                };
                if !command_cards_visible(&cards) {
                    if self.is_cancelled() {
                        return None;
                    }
                    self.emit("Attack", "指令卡尚未完全出现，等待卡面稳定后重试");
                    thread::sleep(ACTION_DELAY);
                    continue;
                }
                if !should_retry_command_card_owner_detection(
                    &cards,
                    &candidate_ids,
                    configured_frontline_count,
                ) {
                    if configured_frontline_count < 3
                        && cards
                            .iter()
                            .any(|card| !card.is_stunned && card.servant_id.is_none())
                    {
                        self.emit_warn(
                            "Attack",
                            "当前从者配置不满三位，跳过强制识别所有指令卡归属。",
                        );
                    }
                    if !self.battle.command_card_owner_fallback_to_full_party {
                        self.battle.command_card_owner_failure_count = 0;
                    }
                    break cards;
                }
                if tolerate_missing_owners && used_full_party_candidates {
                    self.emit_warn(
                        "Attack",
                        "警告：全队候选仍有指令卡成员未识别，暴击模式将放宽成员交错限制",
                    );
                    break cards;
                }
                if !self.battle.command_card_owner_fallback_to_full_party {
                    self.battle.command_card_owner_failure_count += 1;
                }
                if self.battle.command_card_owner_failure_count
                    >= COMMAND_CARD_FRONTLINE_OWNER_FAILURE_LIMIT
                    && !self.battle.command_card_owner_fallback_to_full_party
                {
                    self.battle.command_card_owner_fallback_to_full_party = true;
                    candidate_ids = full_candidate_ids.clone();
                    self.emit_warn(
                        "Attack",
                        "警告：指令卡归属已连续 3 次识别失败，本场战斗后续将改为检测全队六人",
                    );
                    self.emit_attack(
                        "指令卡归属连续识别失败，改用全队六人候选",
                        AttackLogMeta {
                            front_servant_ids: *party_ids,
                            candidate_servant_ids: Some(candidate_ids.clone()),
                            command_cards: None,
                            ready_np_slots: None,
                            selected_pick: None,
                        },
                    );
                }
                if self.is_cancelled() {
                    return None;
                }
                self.emit("Attack", "指令卡从者未识别，等待卡面稳定后重试");
                thread::sleep(ACTION_DELAY);
            }
        } else {
            let cards = loop {
                let cards = match self.sidecar().find_command_cards(None, None, &[], None) {
                    Ok(c) => c,
                    Err(err) => {
                        self.fail_action("Attack", "等待指令卡出现", err);
                        return None;
                    }
                };
                if command_cards_visible(&cards) {
                    break cards;
                }
                if self.is_cancelled() {
                    return None;
                }
                self.emit("Attack", "指令卡尚未完全出现，等待卡面稳定后重试");
                thread::sleep(ACTION_DELAY);
            };
            self.emit("Attack", "未配置普通指令卡，跳过指令卡归属识别");
            cards
        };
        let np_detection_mode = self.config.noble_phantasm_detection_mode;
        let nps = match np_detection_mode {
            NoblePhantasmDetectionMode::Card => loop {
                let mut nps = match self.sidecar().find_noble_phantasms(None, None) {
                    Ok(n) => n,
                    Err(err) => {
                        self.fail_action("Attack", "识别宝具卡", err);
                        return None;
                    }
                };
                apply_np_detection_mode(&mut nps, np_detection_mode);

                if np_card_read_complete(&nps) {
                    break nps;
                }
                if self.is_cancelled() {
                    return None;
                }
                self.emit("Attack", "宝具卡尚未识别完整，等待卡面稳定后重试");
                thread::sleep(ACTION_DELAY);
            },
            NoblePhantasmDetectionMode::Gauge => loop {
                let Some(nps) = self.read_noble_phantasm_gauges("Attack") else {
                    return None;
                };
                break nps;
            },
            NoblePhantasmDetectionMode::GaugeBeforeAttack => {
                let Some(nps) = self.battle.pre_attack_nps.clone() else {
                    self.fail_action(
                        "Attack",
                        "读取攻击前宝具条",
                        "未找到攻击前缓存的宝具条识别结果".into(),
                    );
                    return None;
                };
                nps
            }
        };

        if recognize_command_cards {
            let card_summary: Vec<String> = cards
                .iter()
                .map(|c| {
                    let owner = c
                        .servant_id
                        .and_then(|id| {
                            Some(format!("{id}{}", if c.is_support { "[支]" } else { "" }))
                        })
                        .unwrap_or_else(|| "未识别".into());
                    format!(
                        "C{}={}/{}",
                        c.slot + 1,
                        c.suit.as_deref().unwrap_or("?"),
                        owner,
                    )
                })
                .collect();
            self.emit_attack(
                &format!("指令卡: {}", card_summary.join(" ")),
                AttackLogMeta {
                    front_servant_ids: *party_ids,
                    candidate_servant_ids: None,
                    command_cards: Some(command_cards_log_meta(&cards)),
                    ready_np_slots: None,
                    selected_pick: None,
                },
            );
        }
        let ready: Vec<String> = nps
            .iter()
            .filter(|n| n.ready)
            .map(|n| format!("NP{}", n.slot + 1))
            .collect();
        let ready_np_slots: Vec<u32> = nps.iter().filter(|n| n.ready).map(|n| n.slot).collect();
        if ready.is_empty() {
            self.emit_attack(
                "宝具就绪: 无",
                AttackLogMeta {
                    front_servant_ids: *party_ids,
                    candidate_servant_ids: None,
                    command_cards: None,
                    ready_np_slots: Some(ready_np_slots),
                    selected_pick: None,
                },
            );
        } else {
            self.emit_attack(
                &format!("宝具就绪: {}", ready.join(" ")),
                AttackLogMeta {
                    front_servant_ids: *party_ids,
                    candidate_servant_ids: None,
                    command_cards: None,
                    ready_np_slots: Some(ready_np_slots),
                    selected_pick: None,
                },
            );
        }

        if reads_np_gauge_before_attack(np_detection_mode) {
            self.battle.pre_attack_nps = None;
        }

        Some((cards, nps))
    }

    /// Read the bottom NP gauges while the Battle screen is still visible.
    /// A one-second median-sample window avoids treating a transient dialogue
    /// overlay or visual effect as the final readiness state.
    pub(crate) fn read_noble_phantasm_gauges(
        &mut self,
        screen: &str,
    ) -> Option<Vec<NoblePhantasmMatch>> {
        loop {
            let started = Instant::now();
            let sample_window = Duration::from_secs(1);
            let sample_interval = Duration::from_millis(200);
            let mut samples: Vec<Vec<NoblePhantasmMatch>> = Vec::new();

            loop {
                let sample = match self.sidecar().find_noble_phantasms(None, None) {
                    Ok(n) => n,
                    Err(err) => {
                        self.fail_action(screen, "读取宝具数字", err);
                        return None;
                    }
                };

                samples.push(sample);

                if self.is_cancelled() {
                    return None;
                }
                if started.elapsed() >= sample_window {
                    break;
                }
                thread::sleep(sample_interval);
            }

            let nps = aggregate_np_gauge_samples(&samples);
            if np_gauge_read_complete(&nps) {
                return Some(nps);
            }
            if self.is_cancelled() {
                return None;
            }
            self.emit(screen, "宝具数字未识别完整，等待遮挡消失后重试");
            thread::sleep(ACTION_DELAY);
        }
    }

    /// Capture NP readiness before the attack button changes the screen to
    /// the command-card view. The result is consumed by `read_attack_state`.
    pub(crate) fn prepare_noble_phantasm_gauge_before_attack(&mut self) -> bool {
        if !reads_np_gauge_before_attack(self.config.noble_phantasm_detection_mode) {
            return true;
        }

        self.battle.pre_attack_nps = None;
        if !self.battle.pre_attack_np_warning_emitted {
            self.emit_warn(
                "Battle",
                "当前使用攻击前宝具条识别，从者台词可能遮挡识别结果，请关闭台词",
            );
            self.battle.pre_attack_np_warning_emitted = true;
        }
        let Some(nps) = self.read_noble_phantasm_gauges("Battle") else {
            return false;
        };
        self.battle.pre_attack_nps = Some(nps);
        true
    }

    pub(crate) fn pick_and_tap_attack_cards(
        &mut self,
        cards: &[CommandCardMatch],
        nps: &[NoblePhantasmMatch],
        party_ids: &[Option<u32>; 3],
        party_supports: &[bool; 3],
        attack_priority_override: Option<&[AttackCard]>,
    ) {
        let mut used_card_slots: HashSet<u32> = HashSet::new();
        let mut used_np_slots: HashSet<u32> = HashSet::new();
        let mut picks: Vec<Pick> = Vec::with_capacity(3);
        let actionable_cards = actionable_command_cards(cards);

        if let Some(priority) = attack_priority_override {
            picks = pick_by_priority(
                priority,
                &actionable_cards,
                nps,
                party_ids,
                party_supports,
                &mut used_card_slots,
                &mut used_np_slots,
            );
        } else if let Some(priority) = attack_priority_for_current_scene(
            self.advanced_mode,
            self.battle.scene_config_used,
            &self.scenes,
            self.battle.current_scene_index,
            self.battle.current_turn_index,
        ) {
            picks = pick_by_priority(
                priority,
                &actionable_cards,
                nps,
                party_ids,
                party_supports,
                &mut used_card_slots,
                &mut used_np_slots,
            );
        } else {
            self.emit("Attack", "场景未变更，按默认顺序补位");
        }

        if picks.len() < 3 {
            fill_remaining(&mut picks, &actionable_cards, &mut used_card_slots);
        }
        if picks.len() < 3 {
            fill_remaining(&mut picks, cards, &mut used_card_slots);
        }

        if picks.is_empty() {
            self.emit("Attack", "未能选出任何卡，跳过");
            self.battle.scene_config_used = false;
            return;
        }

        self.tap_picks("Attack", &picks, cards, party_ids);
    }

    pub(crate) fn tap_picks(
        &mut self,
        screen: &str,
        picks: &[Pick],
        cards: &[CommandCardMatch],
        party_ids: &[Option<u32>; 3],
    ) {
        if picks.is_empty() {
            self.emit(screen, "未能选出任何卡，跳过");
            self.battle.scene_config_used = false;
            return;
        }

        let mut pending: VecDeque<Pick> = picks.iter().cloned().collect();
        let mut replacements = replacement_command_card_picks(picks, cards);
        while pending.len() < 3 {
            let Some(replacement) = replacements.pop_front() else {
                self.fail_action(screen, "补足选卡", "没有其他可用指令卡".into());
                return;
            };
            pending.push_back(replacement);
        }

        let mut selected_count = 0usize;
        let mut submitted_picks = Vec::with_capacity(3);
        while selected_count < 3 {
            let Some(pick) = pending.pop_front() else {
                self.fail_action(screen, "补足选卡", "没有其他可用指令卡".into());
                return;
            };
            let is_np = matches!(&pick, Pick::Np { .. });
            let submitted_pick = pick.clone();
            let (msg, point, selected_pick) = match pick {
                Pick::Card {
                    slot,
                    point,
                    servant_id,
                    suit,
                    from_priority,
                } => {
                    let label = match from_priority.as_deref() {
                        Some(p) => format!("选择 {p} → C{}", slot + 1),
                        None => format!("补位: C{}", slot + 1),
                    };
                    let detail = format!(
                        " ({}{})",
                        suit.as_deref().unwrap_or("?"),
                        servant_id.map(|id| format!("/{id}")).unwrap_or_default(),
                    );
                    (
                        format!("{}/3 {}{}", selected_count + 1, label, detail),
                        point,
                        AttackLogSelectedPick {
                            step: selected_count + 1,
                            total: 3,
                            from_priority,
                            kind: AttackLogPickKind::Card,
                            slot,
                            suit,
                            servant_id,
                        },
                    )
                }
                Pick::Np {
                    slot,
                    point,
                    from_priority,
                } => (
                    format!(
                        "{}/3 选择 {} → NP{}",
                        selected_count + 1,
                        from_priority,
                        slot + 1,
                    ),
                    point,
                    AttackLogSelectedPick {
                        step: selected_count + 1,
                        total: 3,
                        from_priority: Some(from_priority),
                        kind: AttackLogPickKind::Np,
                        slot,
                        suit: None,
                        servant_id: party_ids.get(slot as usize).copied().flatten(),
                    },
                ),
            };
            if screen == "Attack" {
                self.emit_attack(
                    &msg,
                    AttackLogMeta {
                        front_servant_ids: *party_ids,
                        candidate_servant_ids: None,
                        command_cards: None,
                        ready_np_slots: None,
                        selected_pick: Some(selected_pick),
                    },
                );
            } else {
                self.emit(screen, &msg);
            }
            let simulate_missed_tap = screen == "Attack"
                && selected_count == 2
                && match consume_simulate_stuck_attack_selection(&self.app_handle) {
                    Ok(enabled) => enabled,
                    Err(err) => {
                        self.emit_warn(
                            screen,
                            &format!("读取选卡卡住测试开关失败，本次正常选卡: {err}"),
                        );
                        false
                    }
                };
            if simulate_missed_tap {
                self.emit_warn(
                    screen,
                    "调试测试：已故意跳过第 3 张卡的点击，等待触发选卡恢复",
                );
            } else if !self.tap_at(screen, point) {
                return;
            }
            thread::sleep(ACTION_DELAY);

            if is_np {
                let cannot_use_np = match self.sidecar().find_element_by_name(
                    None,
                    ATTACK_SCREEN,
                    CANNOT_USE_NP_CLOSE_BUTTON_ELEMENT,
                ) {
                    Ok(matched) => matched.found,
                    Err(err) => {
                        self.fail_action(screen, "检测宝具是否可用", err);
                        return;
                    }
                };
                if cannot_use_np {
                    self.emit_warn(screen, "宝具无法使用，关闭提示并改用其他指令卡");
                    if !self.tap_at(screen, CANNOT_USE_NP_CLOSE_POINT) {
                        return;
                    }
                    thread::sleep(ACTION_DELAY);
                    let Some(replacement) = replacements.pop_front() else {
                        self.fail_action(screen, "替换无法使用的宝具", "没有其他可用指令卡".into());
                        return;
                    };
                    pending.push_back(replacement);
                    continue;
                }
            }

            submitted_picks.push(submitted_pick);
            selected_count += 1;
        }

        self.last_attack_plan = Some(AttackRetryPlan {
            picks: submitted_picks,
            cards: cards.to_vec(),
            party_ids: *party_ids,
        });
        self.battle
            .transition(BattleFlowEvent::AttackCardsSubmitted { at: Instant::now() });

        // Reset for next cycle
        self.battle.scene_config_used = false;
    }

    fn read_noble_phantasms_for_retry(&mut self) -> Option<Vec<NoblePhantasmMatch>> {
        match self.config.noble_phantasm_detection_mode {
            NoblePhantasmDetectionMode::Card => loop {
                let mut nps = match self.sidecar().find_noble_phantasms(None, None) {
                    Ok(n) => n,
                    Err(err) => {
                        self.fail_action("Attack", "重试时识别宝具卡", err);
                        return None;
                    }
                };
                apply_np_detection_mode(&mut nps, NoblePhantasmDetectionMode::Card);
                if np_card_read_complete(&nps) {
                    return Some(nps);
                }
                if self.is_cancelled() {
                    return None;
                }
                self.emit("Attack", "重试时宝具卡尚未识别完整，等待卡面稳定后重试");
                thread::sleep(ACTION_DELAY);
            },
            NoblePhantasmDetectionMode::Gauge => self.read_noble_phantasm_gauges("Attack"),
            NoblePhantasmDetectionMode::GaugeBeforeAttack => {
                let nps = self.battle.pre_attack_nps.clone();
                if nps.is_none() {
                    self.fail_action(
                        "Attack",
                        "重试时读取攻击前宝具条",
                        "未找到重新识别的宝具条结果".into(),
                    );
                }
                nps
            }
        }
    }

    /// Recover a chain whose three issued taps left the game on the Attack
    /// screen. The command-card hand is reused after returning to Battle, but
    /// NP readiness is re-read because the original result may be stale.
    pub(crate) fn recover_stuck_attack_selection(&mut self) {
        let Some(plan) = self.last_attack_plan.clone() else {
            self.emit_warn("Attack", "选卡未完成，但没有可复用的选卡记录");
            return;
        };

        self.emit_warn(
            "Attack",
            "选卡提交后仍停留在指令卡画面，返回并复用本轮选卡重试",
        );
        if !self.tap_at("Attack", ATTACK_SCREEN_RETURN) {
            return;
        }
        thread::sleep(ACTION_DELAY);
        if !self.wait_for_attack_button("Battle", ATTACK_RETRY_NAVIGATION_TIMEOUT) {
            self.emit_warn("Attack", "选卡重试返回 Battle 超时，稍后继续恢复");
            return;
        }

        self.battle.transition(BattleFlowEvent::BattleActionable);
        if !self.tap_attack_button() {
            return;
        }
        if !self.wait_for_retry_attack_screen_stable(ATTACK_RETRY_NAVIGATION_TIMEOUT) {
            self.emit_warn("Attack", "选卡重试等待指令卡画面稳定超时");
            return;
        }
        self.battle
            .transition(BattleFlowEvent::AttackScreenDetected);
        thread::sleep(ATTACK_RETRY_SCREEN_SETTLE);

        let Some(nps) = self.read_noble_phantasms_for_retry() else {
            return;
        };
        let retry_picks = refresh_retry_picks_for_np_state(&plan.picks, &plan.cards, &nps);
        if retry_picks.len() < 3 {
            self.fail_action("Attack", "重试选卡", "宝具不可用且没有足够的指令卡补位".into());
            return;
        }
        self.emit("Attack", "指令卡画面已稳定，已重新识别宝具状态并重试选卡");
        self.tap_picks("Attack", &retry_picks, &plan.cards, &plan.party_ids);
        if !self.battle.awaiting_attack_resolution() {
            return;
        }

        if self.confirm_retried_attack_submission(ATTACK_SUBMISSION_STUCK_TIMEOUT) {
            self.emit("Attack", "已确认重试选卡成功");
        } else {
            self.emit_warn("Attack", "重试选卡后仍未离开指令卡画面，将再次恢复");
        }
    }

    fn wait_for_retry_attack_screen_stable(&mut self, timeout: Duration) -> bool {
        let started_at = Instant::now();
        let mut consecutive_attack_polls = 0u32;
        loop {
            if self.is_cancelled() {
                return false;
            }
            match self.sidecar().detect(None) {
                Ok(Screen::Attack) => {
                    consecutive_attack_polls += 1;
                    if consecutive_attack_polls >= ATTACK_RETRY_SCREEN_STABLE_POLLS {
                        return true;
                    }
                }
                Ok(_) | Err(_) => consecutive_attack_polls = 0,
            }
            if started_at.elapsed() >= timeout {
                return false;
            }
            thread::sleep(SKILL_POLL_INTERVAL);
        }
    }

    fn confirm_retried_attack_submission(&mut self, timeout: Duration) -> bool {
        let started_at = Instant::now();
        let mut consecutive_non_attack_polls = 0u32;
        loop {
            if self.is_cancelled() {
                return false;
            }
            match self.sidecar().detect(None) {
                Ok(Screen::Attack) => consecutive_non_attack_polls = 0,
                Ok(_) => {
                    consecutive_non_attack_polls += 1;
                    if consecutive_non_attack_polls >= 2 {
                        return true;
                    }
                }
                Err(_) => consecutive_non_attack_polls = 0,
            }
            if started_at.elapsed() >= timeout {
                return false;
            }
            thread::sleep(SKILL_POLL_INTERVAL);
        }
    }

    pub(crate) fn handle_advanced_attack(
        &mut self,
        cards: Vec<CommandCardMatch>,
        nps: Vec<NoblePhantasmMatch>,
        party_ids: [Option<u32>; 3],
        party_supports: [bool; 3],
    ) {
        let grand_servants = self.grand_servant_runtime_configs();
        let Some(scene) = self
            .advanced_scenes
            .get(self.battle.current_scene_index)
            .cloned()
        else {
            if !grand_servants.is_empty() {
                self.emit("Attack", "无高级指令配置，按冠位自动策略攻击");
                let scene = AdvancedBattleScene {
                    id: "__grand_auto_default__".into(),
                    enemy_target: None,
                    main_output: None,
                    grand_auto_order_change: None,
                    command_conditions: Vec::new(),
                    control_actions: Vec::new(),
                    turns: Vec::new(),
                    startup_actions: Vec::new(),
                    rules: Vec::new(),
                };
                let picks = choose_advanced_auto_picks_with_grand_class(
                    &scene,
                    &cards,
                    &nps,
                    &party_ids,
                    &party_supports,
                    &grand_servants,
                    &self.config.grand_card_strategy,
                    self.config.grand_class,
                );
                self.tap_picks("Attack", &picks, &cards, &party_ids);
                return;
            }
            self.emit("Attack", "无高级指令配置，按默认顺序补位");
            self.pick_and_tap_attack_cards(&cards, &nps, &party_ids, &party_supports, None);
            return;
        };

        if scene.rules.is_empty() || uses_advanced_strategy_flow(&scene) {
            let scene_index = self.battle.current_scene_index;
            let executed_control_count = *self
                .battle
                .advanced_control_indices
                .get(&scene_index)
                .unwrap_or(&0);
            let (current_party_ids, current_party_supports) =
                if self.battle.advanced_startup_done.contains(&scene_index) {
                    let startup_control_count = *self
                        .battle
                        .advanced_startup_control_indices
                        .get(&scene_index)
                        .unwrap_or(&executed_control_count);
                    (
                        self.advanced_party_ids_after_startup_flow(
                            &scene,
                            executed_control_count,
                            startup_control_count,
                        ),
                        self.advanced_party_supports_after_startup_flow(
                            &scene,
                            executed_control_count,
                            startup_control_count,
                        ),
                    )
                } else {
                    (
                        self.advanced_party_ids_after_control(&scene, executed_control_count),
                        self.advanced_party_supports_after_control(&scene, executed_control_count),
                    )
                };
            let (cards, nps, party_ids, party_supports) = if current_party_ids != party_ids
                || current_party_supports != party_supports
            {
                let Some((cards, nps)) = self.read_attack_state(&current_party_ids, true, false)
                else {
                    return;
                };
                (cards, nps, current_party_ids, current_party_supports)
            } else {
                (cards, nps, party_ids, party_supports)
            };

            if !self.battle.advanced_startup_done.contains(&scene_index) {
                if scene.grand_auto_order_change == Some(true) {
                    let auto_order_change = grand_auto_order_change_action(
                        &cards,
                        &party_ids,
                        &party_supports,
                        &grand_servants,
                        self.config.grand_class,
                    );
                    let original_members = self.build_full_party_members();
                    let mut members = original_members.clone();
                    let mut startup_actions = Vec::new();
                    if let Some(action) = auto_order_change.clone() {
                        self.emit("Attack", "启动条件：自动将后排冠位从者换至前排");
                        let ids = party_member_ids(&members);
                        if action_frontline_available(&ids, &action) {
                            apply_party_member_lineup_change(&mut members, &action);
                            startup_actions.push(action.clone());
                            self.battle
                                .advanced_auto_order_changes
                                .insert(scene_index, action);
                        } else {
                            self.emit("Battle", "跳过自动换位：目标不在可交换位置");
                        }
                    } else {
                        self.emit(
                            "Attack",
                            "启动条件：主冠位不需要或无法自动换位，直接执行 Turn 1 技能",
                        );
                    }
                    let next_control_count = if executed_control_count < scene.control_actions.len()
                    {
                        executed_control_count + 1
                    } else {
                        executed_control_count
                    };
                    if executed_control_count < scene.control_actions.len() {
                        self.emit(
                            "Attack",
                            &format!("Turn 1 执行本回合控制行动 {next_control_count}"),
                        );
                    }
                    let first_turn_actions = advanced_turn_actions(&scene, 0).unwrap_or_default();
                    let pending_actions = scene
                        .control_actions
                        .iter()
                        .skip(executed_control_count)
                        .take(next_control_count.saturating_sub(executed_control_count))
                        .chain(first_turn_actions.iter())
                        .cloned();
                    for action in pending_actions {
                        let Some(resolved_action) = resolve_action_to_current_member_positions(
                            &members,
                            &original_members,
                            &action,
                        ) else {
                            self.emit_action(
                                "跳过行动：目标不在当前可用位置",
                                skipped_action_log_meta(&action),
                            );
                            continue;
                        };
                        let ids = party_member_ids(&members);
                        if !action_frontline_available(&ids, &resolved_action) {
                            self.emit_action(
                                "跳过行动：目标不在前排",
                                skipped_action_log_meta(&action),
                            );
                            continue;
                        }
                        apply_party_member_lineup_change(&mut members, &resolved_action);
                        startup_actions.push(resolved_action);
                    }
                    let startup_party_members = frontline_party_members(&members);
                    let (startup_party_ids, startup_party_supports) =
                        frontline_party_ids_and_supports(&startup_party_members);
                    self.battle
                        .advanced_control_indices
                        .insert(scene_index, next_control_count);
                    self.battle
                        .advanced_startup_control_indices
                        .insert(scene_index, next_control_count);
                    self.battle.advanced_startup_done.insert(scene_index);
                    self.battle.advanced_turn_indices.insert(scene_index, 1);
                    if !startup_actions.is_empty() {
                        if !self.tap_at("Attack", ATTACK_SCREEN_RETURN) {
                            return;
                        }
                        thread::sleep(ACTION_DELAY);
                        if !self.wait_for_attack_button("Battle", SKILL_WAIT_TIMEOUT) {
                            self.fail_action(
                                "Battle",
                                "返回 Battle 执行启动行动",
                                "等待攻击按钮超时".into(),
                            );
                            return;
                        }
                        let prep_turn = BattleTurn {
                            id: scene.id.clone(),
                            preparation_actions: startup_actions,
                            servant_actions: Vec::new(),
                            equipment_actions: Vec::new(),
                            command_spell_actions: Vec::new(),
                            enemy_target: None,
                            attack_priority: Vec::new(),
                            attack_mode: AttackMode::Normal,
                            critical_strategy: CriticalAttackStrategy::default(),
                            advanced_card_strategy: AdvancedCardStrategy::default(),
                        };
                        if !self.execute_turn_skills(&prep_turn) {
                            return;
                        }

                        self.emit("Battle", "Turn 1 技能完成，进入自动战斗");
                        if !self.tap_attack_button() {
                            return;
                        }
                        thread::sleep(ACTION_DELAY);
                        let Some((next_cards, next_nps)) =
                            self.read_attack_state(&startup_party_ids, true, false)
                        else {
                            return;
                        };
                        let picks = choose_advanced_auto_picks_with_grand_class(
                            &scene,
                            &next_cards,
                            &next_nps,
                            &startup_party_ids,
                            &startup_party_supports,
                            &grand_servants,
                            &self.config.grand_card_strategy,
                            self.config.grand_class,
                        );
                        self.tap_picks("Attack", &picks, &next_cards, &startup_party_ids);
                        return;
                    }

                    let picks = choose_advanced_auto_picks_with_grand_class(
                        &scene,
                        &cards,
                        &nps,
                        &startup_party_ids,
                        &startup_party_supports,
                        &grand_servants,
                        &self.config.grand_card_strategy,
                        self.config.grand_class,
                    );
                    self.tap_picks("Attack", &picks, &cards, &startup_party_ids);
                    return;
                }

                if !advanced_startup_conditions_match(&scene, &cards, &party_ids, &party_supports) {
                    let control_index = executed_control_count;
                    if let Some(control_action) = scene.control_actions.get(control_index).cloned()
                    {
                        self.emit(
                            "Attack",
                            &format!("启动条件未满足，执行控制行动 {}", control_index + 1),
                        );
                        if !self.tap_at("Attack", ATTACK_SCREEN_RETURN) {
                            return;
                        }
                        thread::sleep(ACTION_DELAY);
                        if !self.wait_for_attack_button("Battle", SKILL_WAIT_TIMEOUT) {
                            self.fail_action(
                                "Battle",
                                "返回 Battle 执行控制行动",
                                "等待攻击按钮超时".into(),
                            );
                            return;
                        }
                        let control_turn = BattleTurn {
                            id: format!("{}_control_{}", scene.id, control_index + 1),
                            preparation_actions: vec![control_action],
                            servant_actions: Vec::new(),
                            equipment_actions: Vec::new(),
                            command_spell_actions: Vec::new(),
                            enemy_target: None,
                            attack_priority: Vec::new(),
                            attack_mode: AttackMode::Normal,
                            critical_strategy: CriticalAttackStrategy::default(),
                            advanced_card_strategy: AdvancedCardStrategy::default(),
                        };
                        if !self.execute_turn_skills(&control_turn) {
                            return;
                        }
                        self.battle
                            .advanced_control_indices
                            .insert(scene_index, control_index + 1);

                        self.emit("Battle", "控制行动完成，返回指令卡攻击");
                        if !self.tap_attack_button() {
                            return;
                        }
                        thread::sleep(ACTION_DELAY);
                        let control_party_ids =
                            self.advanced_party_ids_after_control(&scene, control_index + 1);
                        let control_party_supports =
                            self.advanced_party_supports_after_control(&scene, control_index + 1);
                        let Some((next_cards, _next_nps)) =
                            self.read_attack_state(&control_party_ids, true, false)
                        else {
                            return;
                        };
                        let picks = choose_advanced_auto_picks_with_grand_class(
                            &scene,
                            &next_cards,
                            &[],
                            &control_party_ids,
                            &control_party_supports,
                            &grand_servants,
                            &self.config.grand_card_strategy,
                            self.config.grand_class,
                        );
                        self.tap_picks("Attack", &picks, &next_cards, &control_party_ids);
                        return;
                    }

                    self.emit("Attack", "启动条件未满足，按自动优先级攻击且不释放宝具");
                    let picks = choose_advanced_auto_picks_with_grand_class(
                        &scene,
                        &cards,
                        &[],
                        &party_ids,
                        &party_supports,
                        &grand_servants,
                        &self.config.grand_card_strategy,
                        self.config.grand_class,
                    );
                    self.tap_picks("Attack", &picks, &cards, &party_ids);
                    return;
                }

                self.emit("Attack", "启动条件满足，执行 Turn 1 技能");
                let next_control_count = if executed_control_count < scene.control_actions.len() {
                    executed_control_count + 1
                } else {
                    executed_control_count
                };
                if executed_control_count < scene.control_actions.len() {
                    self.emit(
                        "Attack",
                        &format!("Turn 1 执行本回合控制行动 {next_control_count}"),
                    );
                }
                let first_turn_actions = advanced_turn_actions(&scene, 0).unwrap_or_default();
                let pending_actions = scene
                    .control_actions
                    .iter()
                    .skip(executed_control_count)
                    .take(next_control_count.saturating_sub(executed_control_count))
                    .chain(first_turn_actions.iter())
                    .cloned();
                let (startup_actions, startup_party_members) = self
                    .advanced_actions_and_members_after(
                        scene
                            .control_actions
                            .iter()
                            .take(executed_control_count)
                            .cloned(),
                        pending_actions,
                    );
                let (startup_party_ids, startup_party_supports) =
                    frontline_party_ids_and_supports(&startup_party_members);
                self.battle
                    .advanced_control_indices
                    .insert(scene_index, next_control_count);
                self.battle
                    .advanced_startup_control_indices
                    .insert(scene_index, next_control_count);
                self.battle.advanced_startup_done.insert(scene_index);
                self.battle.advanced_turn_indices.insert(scene_index, 1);
                if !startup_actions.is_empty() {
                    if !self.tap_at("Attack", ATTACK_SCREEN_RETURN) {
                        return;
                    }
                    thread::sleep(ACTION_DELAY);
                    if !self.wait_for_attack_button("Battle", SKILL_WAIT_TIMEOUT) {
                        self.fail_action(
                            "Battle",
                            "返回 Battle 执行启动行动",
                            "等待攻击按钮超时".into(),
                        );
                        return;
                    }
                    let prep_turn = BattleTurn {
                        id: scene.id.clone(),
                        preparation_actions: startup_actions,
                        servant_actions: Vec::new(),
                        equipment_actions: Vec::new(),
                        command_spell_actions: Vec::new(),
                        enemy_target: None,
                        attack_priority: Vec::new(),
                        attack_mode: AttackMode::Normal,
                        critical_strategy: CriticalAttackStrategy::default(),
                        advanced_card_strategy: AdvancedCardStrategy::default(),
                    };
                    if !self.execute_turn_skills(&prep_turn) {
                        return;
                    }

                    self.emit("Battle", "Turn 1 技能完成，进入自动战斗");
                    if !self.tap_attack_button() {
                        return;
                    }
                    thread::sleep(ACTION_DELAY);
                    let Some((next_cards, next_nps)) =
                        self.read_attack_state(&startup_party_ids, true, false)
                    else {
                        return;
                    };
                    let picks = choose_advanced_auto_picks_with_grand_class(
                        &scene,
                        &next_cards,
                        &next_nps,
                        &startup_party_ids,
                        &startup_party_supports,
                        &grand_servants,
                        &self.config.grand_card_strategy,
                        self.config.grand_class,
                    );
                    self.tap_picks("Attack", &picks, &next_cards, &startup_party_ids);
                    return;
                }

                let picks = choose_advanced_auto_picks_with_grand_class(
                    &scene,
                    &cards,
                    &nps,
                    &startup_party_ids,
                    &startup_party_supports,
                    &grand_servants,
                    &self.config.grand_card_strategy,
                    self.config.grand_class,
                );
                self.tap_picks("Attack", &picks, &cards, &startup_party_ids);
                return;
            }

            let executed_control_count = *self
                .battle
                .advanced_control_indices
                .get(&self.battle.current_scene_index)
                .unwrap_or(&0);
            let startup_control_count = *self
                .battle
                .advanced_startup_control_indices
                .get(&self.battle.current_scene_index)
                .unwrap_or(&executed_control_count);
            if executed_control_count < scene.control_actions.len() {
                let active_actions = advanced_startup_flow_actions(
                    &scene,
                    executed_control_count,
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
                let (control_actions, control_party_members) = self
                    .advanced_actions_and_members_after(
                        active_actions.into_iter(),
                        scene
                            .control_actions
                            .iter()
                            .skip(executed_control_count)
                            .take(1)
                            .cloned(),
                    );
                let (control_party_ids, control_party_supports) =
                    frontline_party_ids_and_supports(&control_party_members);
                let next_control_count = executed_control_count + 1;
                self.battle
                    .advanced_control_indices
                    .insert(self.battle.current_scene_index, next_control_count);

                if !control_actions.is_empty() {
                    self.emit(
                        "Attack",
                        &format!("自动战斗执行本回合控制行动 {next_control_count}"),
                    );
                    if !self.tap_at("Attack", ATTACK_SCREEN_RETURN) {
                        return;
                    }
                    thread::sleep(ACTION_DELAY);
                    if !self.wait_for_attack_button("Battle", SKILL_WAIT_TIMEOUT) {
                        self.fail_action(
                            "Battle",
                            "返回 Battle 执行控制行动",
                            "等待攻击按钮超时".into(),
                        );
                        return;
                    }
                    let control_turn = BattleTurn {
                        id: format!("{}_auto_control_{}", scene.id, next_control_count),
                        preparation_actions: control_actions,
                        servant_actions: Vec::new(),
                        equipment_actions: Vec::new(),
                        command_spell_actions: Vec::new(),
                        enemy_target: None,
                        attack_priority: Vec::new(),
                        attack_mode: AttackMode::Normal,
                        critical_strategy: CriticalAttackStrategy::default(),
                        advanced_card_strategy: AdvancedCardStrategy::default(),
                    };
                    if !self.execute_turn_skills(&control_turn) {
                        return;
                    }

                    self.emit("Battle", "控制行动完成，返回指令卡攻击");
                    if !self.tap_attack_button() {
                        return;
                    }
                    thread::sleep(ACTION_DELAY);
                    let Some((next_cards, next_nps)) =
                        self.read_attack_state(&control_party_ids, true, false)
                    else {
                        return;
                    };
                    let picks = choose_advanced_auto_picks_with_grand_class(
                        &scene,
                        &next_cards,
                        &next_nps,
                        &control_party_ids,
                        &control_party_supports,
                        &grand_servants,
                        &self.config.grand_card_strategy,
                        self.config.grand_class,
                    );
                    self.tap_picks("Attack", &picks, &next_cards, &control_party_ids);
                    return;
                }

                let picks = choose_advanced_auto_picks_with_grand_class(
                    &scene,
                    &cards,
                    &nps,
                    &control_party_ids,
                    &control_party_supports,
                    &grand_servants,
                    &self.config.grand_card_strategy,
                    self.config.grand_class,
                );
                self.tap_picks("Attack", &picks, &cards, &control_party_ids);
                return;
            }
            let active_party_ids = self.advanced_party_ids_after_startup_flow(
                &scene,
                executed_control_count,
                startup_control_count,
            );
            let active_party_supports = self.advanced_party_supports_after_startup_flow(
                &scene,
                executed_control_count,
                startup_control_count,
            );
            let picks = choose_advanced_auto_picks_with_grand_class(
                &scene,
                &cards,
                &nps,
                &active_party_ids,
                &active_party_supports,
                &grand_servants,
                &self.config.grand_card_strategy,
                self.config.grand_class,
            );
            self.tap_picks("Attack", &picks, &cards, &active_party_ids);
            return;
        }

        let mut cards = cards;
        let mut nps = nps;
        let mut next_rule_index = 0usize;
        let mut guard = 0usize;
        while next_rule_index < scene.rules.len() && guard <= scene.rules.len() {
            guard += 1;
            let Some((rule_index, rule)) = scene
                .rules
                .iter()
                .enumerate()
                .skip(next_rule_index)
                .find(|(_, rule)| {
                    advanced_rule_matches(rule, &cards, &nps, &party_ids, &party_supports)
                })
                .map(|(index, rule)| (index, rule.clone()))
            else {
                break;
            };

            let prep_actions: Vec<Action> = rule
                .actions
                .iter()
                .filter_map(|action| action.as_preparation_action())
                .collect();
            let attack_priority: Vec<AttackCard> = rule
                .actions
                .iter()
                .filter_map(|action| action.as_attack_card())
                .collect();

            if !prep_actions.is_empty() {
                self.emit(
                    "Attack",
                    &format!("高级规则 {} 命中，返回执行准备行动", rule_index + 1),
                );
                if !self.tap_at("Attack", ATTACK_SCREEN_RETURN) {
                    return;
                }
                thread::sleep(ACTION_DELAY);
                if !self.wait_for_attack_button("Battle", SKILL_WAIT_TIMEOUT) {
                    self.fail_action(
                        "Battle",
                        "返回 Battle 执行高级规则准备行动",
                        "等待攻击按钮超时".into(),
                    );
                    return;
                }
                let prep_turn = BattleTurn {
                    id: rule.id.clone(),
                    preparation_actions: prep_actions,
                    servant_actions: Vec::new(),
                    equipment_actions: Vec::new(),
                    command_spell_actions: Vec::new(),
                    enemy_target: None,
                    attack_priority: Vec::new(),
                    attack_mode: AttackMode::Normal,
                    critical_strategy: CriticalAttackStrategy::default(),
                    advanced_card_strategy: AdvancedCardStrategy::default(),
                };
                if !self.execute_turn_skills(&prep_turn) {
                    return;
                }

                self.emit("Battle", "高级规则准备行动完成，重新进入指令卡");
                if !self.tap_attack_button() {
                    return;
                }
                thread::sleep(ACTION_DELAY);
                let Some((next_cards, next_nps)) = self.read_attack_state(&party_ids, true, false)
                else {
                    return;
                };
                cards = next_cards;
                nps = next_nps;
                if !attack_priority.is_empty() {
                    self.emit(
                        "Attack",
                        &format!("高级规则 {} 准备后执行攻击", rule_index + 1),
                    );
                    self.pick_and_tap_attack_cards(
                        &cards,
                        &nps,
                        &party_ids,
                        &party_supports,
                        Some(&attack_priority),
                    );
                    return;
                }
                next_rule_index = rule_index + 1;
                continue;
            }

            if !attack_priority.is_empty() {
                self.emit(
                    "Attack",
                    &format!("高级规则 {} 命中，执行攻击", rule_index + 1),
                );
                self.pick_and_tap_attack_cards(
                    &cards,
                    &nps,
                    &party_ids,
                    &party_supports,
                    Some(&attack_priority),
                );
                return;
            }

            next_rule_index = rule_index + 1;
        }

        self.emit("Attack", "无高级规则命中攻击，按默认顺序补位");
        self.pick_and_tap_attack_cards(&cards, &nps, &party_ids, &party_supports, None);
    }
}
