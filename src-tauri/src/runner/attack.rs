//! Normal and advanced attack pick helpers.
//!
//! This module owns command-card priority parsing, fallback command-card
//! selection, and non-Grand advanced startup conditions.

use super::*;
use crate::commands::settings::{
    consume_simulate_stuck_attack_selection, NoblePhantasmDetectionMode,
};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

const COMMAND_CARD_FRONTLINE_OWNER_FAILURE_LIMIT: u32 = 3;
const CRITICAL_CHANCE_RECOGNITION_SETTLE_DELAY: Duration = Duration::from_secs(1);
const NP_GAUGE_GLOW_READY_THRESHOLD: f64 = 0.5;
const NP_GAUGE_MIN_VALID_SAMPLES: usize = 3;
const NP_GAUGE_SAMPLE_WINDOW: Duration = Duration::from_secs(1);
const NP_GAUGE_SAMPLE_INTERVAL: Duration = Duration::from_millis(200);

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

pub(crate) fn format_command_card_crit_chances(cards: &[CommandCardMatch]) -> String {
    let mut cards = cards.iter().collect::<Vec<_>>();
    cards.sort_by_key(|card| card.slot);
    cards
        .into_iter()
        .map(|card| {
            format!(
                "C{}={}",
                card.slot + 1,
                card.crit_chance
                    .map(|chance| format!("{chance}%"))
                    .unwrap_or_else(|| "未识别".into())
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn should_log_command_card_crit_chances(
    prefer_higher_critical_chance: bool,
    critical_mode: bool,
) -> bool {
    prefer_higher_critical_chance || critical_mode
}

pub(crate) fn should_capture_unrecognized_critical_chance(
    enabled: bool,
    critical_mode: bool,
    cards: &[CommandCardMatch],
) -> bool {
    enabled && critical_mode && cards.iter().any(|card| card.crit_chance.is_none())
}

pub(crate) fn command_card_recognition_settle_delay(critical_mode: bool) -> Duration {
    if critical_mode {
        CRITICAL_CHANCE_RECOGNITION_SETTLE_DELAY
    } else {
        Duration::ZERO
    }
}

pub(crate) fn unrecognized_critical_chance_screenshot_dir_in_root(root: &Path) -> PathBuf {
    root.join("debug").join("unrecognized-critical-chances")
}

pub(crate) fn unrecognized_critical_chance_screenshot_filename(
    timestamp: std::time::SystemTime,
    completed_mission_runs: u32,
) -> String {
    let millis = timestamp
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    format!(
        "critical-chance-{millis:013}-run{:04}.jpg",
        completed_mission_runs + 1,
    )
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
#[cfg(test)]
pub(crate) fn pick_by_priority(
    priority: &[AttackCard],
    cards: &[CommandCardMatch],
    nps: &[NoblePhantasmMatch],
    party_ids: &[Option<u32>; 3],
    party_supports: &[bool; 3],
    used_card_slots: &mut HashSet<u32>,
    used_np_slots: &mut HashSet<u32>,
) -> Vec<Pick> {
    pick_by_priority_with_crit(
        priority,
        cards,
        nps,
        party_ids,
        party_supports,
        used_card_slots,
        used_np_slots,
        false,
    )
}

pub(crate) fn pick_by_priority_with_crit(
    priority: &[AttackCard],
    cards: &[CommandCardMatch],
    nps: &[NoblePhantasmMatch],
    party_ids: &[Option<u32>; 3],
    party_supports: &[bool; 3],
    used_card_slots: &mut HashSet<u32>,
    used_np_slots: &mut HashSet<u32>,
    prefer_higher_critical_chance: bool,
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
                prefer_higher_critical_chance,
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
                prefer_higher_critical_chance,
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
    fill_empty_pick_slots(
        &mut picks[..fixed_len],
        cards,
        used_card_slots,
        prefer_higher_critical_chance,
    );
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

pub(crate) fn battle_turn_requires_np_recognition(turn: &BattleTurn) -> bool {
    match turn.attack_mode {
        AttackMode::Normal => turn
            .attack_priority
            .iter()
            .filter_map(|entry| entry.card.as_deref())
            .any(priority_card_is_np),
        AttackMode::Critical => false,
        AttackMode::Advanced => turn
            .advanced_card_strategy
            .custom_rules
            .iter()
            .flat_map(|rule| &rule.slots)
            .any(|slot| slot.kind == "np"),
    }
}

fn advanced_rule_requires_np_recognition(rule: &AdvancedRule) -> bool {
    !rule.np_condition_groups.is_empty()
        || rule.actions.iter().any(|action| {
            action
                .as_attack_card()
                .and_then(|entry| entry.card)
                .as_deref()
                .is_some_and(priority_card_is_np)
        })
}

pub(crate) fn advanced_scene_requires_np_recognition(
    scene: &AdvancedBattleScene,
    grand_servants_configured: bool,
    grand_card_strategy: &GrandCardStrategy,
) -> bool {
    if !scene.rules.is_empty() && !uses_advanced_strategy_flow(scene) {
        return scene
            .rules
            .iter()
            .any(advanced_rule_requires_np_recognition);
    }

    let main_output_is_np = scene
        .main_output
        .as_ref()
        .and_then(|output| output.output_type.as_ref())
        .is_some_and(|output_type| matches!(output_type, AdvancedOutputType::Np));
    let custom_rule_uses_np = grand_card_strategy
        .custom_rules
        .iter()
        .flat_map(|rule| &rule.slots)
        .any(|slot| slot.kind == "np");
    main_output_is_np || grand_servants_configured || custom_rule_uses_np
}

pub(crate) fn pick_one_priority(
    card_str: &str,
    cards: &[CommandCardMatch],
    nps: &[NoblePhantasmMatch],
    party_ids: &[Option<u32>; 3],
    party_supports: &[bool; 3],
    used_card_slots: &mut HashSet<u32>,
    used_np_slots: &mut HashSet<u32>,
    prefer_higher_critical_chance: bool,
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
        if best.is_none_or(|b| {
            command_card_preference_order(c, cards, prefer_higher_critical_chance)
                < command_card_preference_order(b, cards, prefer_higher_critical_chance)
        }) {
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

/// Return the card's virtual left-to-right order. When the preference is
/// enabled, cards from the same member with the same color exchange only
/// their order ranks according to critical chance. This keeps every other
/// configured owner/color priority intact.
pub(crate) fn command_card_preference_order(
    card: &CommandCardMatch,
    cards: &[CommandCardMatch],
    prefer_higher_critical_chance: bool,
) -> u32 {
    let (Some(servant_id), Some(suit)) = (card.servant_id, card.suit.as_deref()) else {
        return card.slot;
    };
    if !prefer_higher_critical_chance {
        return card.slot;
    }

    let mut equivalent = cards
        .iter()
        .filter(|candidate| {
            candidate.servant_id == Some(servant_id)
                && candidate.is_support == card.is_support
                && candidate.suit.as_deref() == Some(suit)
        })
        .collect::<Vec<_>>();
    if equivalent.len() < 2 {
        return card.slot;
    }
    if equivalent
        .iter()
        .any(|candidate| candidate.crit_chance.is_none())
    {
        return card.slot;
    }

    let mut available_orders = equivalent
        .iter()
        .map(|candidate| candidate.slot)
        .collect::<Vec<_>>();
    available_orders.sort_unstable();
    equivalent.sort_by_key(|candidate| {
        (
            std::cmp::Reverse(candidate.crit_chance.expect("checked above")),
            candidate.slot,
        )
    });
    equivalent
        .iter()
        .position(|candidate| candidate.slot == card.slot)
        .and_then(|rank| available_orders.get(rank).copied())
        .unwrap_or(card.slot)
}

/// Fill still-empty fixed-chain positions with the leftmost remaining command
/// cards, preserving the user's configured three-card order. Actionable cards
/// are considered before unavailable cards.
pub(crate) fn fill_empty_pick_slots(
    picks: &mut [Option<Pick>],
    cards: &[CommandCardMatch],
    used_card_slots: &mut HashSet<u32>,
    prefer_higher_critical_chance: bool,
) {
    let mut sorted: Vec<&CommandCardMatch> = cards.iter().collect();
    sorted.sort_by_key(|card| {
        (
            card.is_stunned,
            command_card_preference_order(card, cards, prefer_higher_critical_chance),
        )
    });

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
    prefer_higher_critical_chance: bool,
) {
    let mut sorted: Vec<&CommandCardMatch> = cards.iter().collect();
    sorted.sort_by_key(|card| {
        (
            card.is_stunned,
            command_card_preference_order(card, cards, prefer_higher_critical_chance),
        )
    });
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
#[cfg(test)]
pub(crate) fn replacement_command_card_picks(
    picks: &[Pick],
    cards: &[CommandCardMatch],
) -> VecDeque<Pick> {
    replacement_command_card_picks_with_crit(picks, cards, false)
}

pub(crate) fn replacement_command_card_picks_with_crit(
    picks: &[Pick],
    cards: &[CommandCardMatch],
    prefer_higher_critical_chance: bool,
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
    candidates.sort_by_key(|card| {
        (
            card.is_stunned,
            command_card_preference_order(card, cards, prefer_higher_critical_chance),
        )
    });
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
    prefer_higher_critical_chance: bool,
) -> Vec<Pick> {
    let mut replacements =
        replacement_command_card_picks_with_crit(picks, cards, prefer_higher_critical_chance);
    let mut refreshed = Vec::with_capacity(picks.len());

    for pick in picks {
        match pick {
            Pick::Np { slot, .. } if !nps.iter().any(|np| np.slot == *slot && np.ready) => {
                if let Some(replacement) = replacements.pop_front() {
                    refreshed.push(replacement);
                }
            }
            _ => refreshed.push(pick.clone()),
        }
    }

    refreshed
}

/// Center of a normalized rectangle (used to derive a tap point from a
/// detected NP slot's `card_region`).
pub(crate) fn rect_center(r: &NormRect) -> Point {
    Point::new(r.x + r.w / 2.0, r.y + r.h / 2.0)
}

mod advanced_runtime;
mod conditions;
mod critical;
mod noble_phantasm;
mod runtime;
mod selection_runtime;
pub(crate) use conditions::*;
pub(crate) use critical::*;
pub(crate) use noble_phantasm::*;
