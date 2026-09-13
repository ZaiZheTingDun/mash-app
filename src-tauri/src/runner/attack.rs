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

pub(crate) fn np_card_read_complete(nps: &[NoblePhantasmMatch]) -> bool {
    nps.len() == 3 && nps.iter().all(|np| np.card_ready.is_some())
}

pub(crate) fn np_gauge_read_complete(nps: &[NoblePhantasmMatch]) -> bool {
    nps.len() == 3 && nps.iter().all(|np| np.np_glow_score.is_some())
}

pub(crate) fn np_gauge_digit_read_complete(nps: &[NoblePhantasmMatch]) -> bool {
    nps.len() == 3 && nps.iter().all(|np| np.gauge_hundreds_visible.is_some())
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

/// Aggregate one-second NP-gauge digit samples by majority vote of the
/// fixed hundreds slot. A visible hundreds digit means the gauge is at least
/// 100%, which is the only distinction the runner needs for NP readiness.
pub(crate) fn aggregate_np_gauge_digit_samples(
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
            let readings: Vec<(bool, NoblePhantasmMatch)> = samples
                .iter()
                .filter_map(|sample| {
                    sample
                        .iter()
                        .find(|slot| slot.slot == slot_id)
                        .and_then(|slot| {
                            slot.gauge_hundreds_visible
                                .map(|visible| (visible, slot.clone()))
                        })
                })
                .collect();
            if readings.len() < NP_GAUGE_MIN_VALID_SAMPLES {
                return None;
            }

            let visible_count = readings.iter().filter(|(visible, _)| *visible).count();
            let ready = visible_count * 2 > readings.len();
            let mut representative = readings[readings.len() / 2].1.clone();
            representative.ready = ready;
            representative.ready_source = Some("gaugeDigits".into());
            representative.gauge_hundreds_visible = Some(ready);
            representative.gauge_digit_count = Some(if ready { 3 } else { 2 });
            Some(representative)
        })
        .collect()
}

pub(crate) fn np_gauge_sample_window_complete(elapsed: Duration, sample_count: usize) -> bool {
    elapsed >= NP_GAUGE_SAMPLE_WINDOW && sample_count >= NP_GAUGE_MIN_VALID_SAMPLES
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

/// Center of a normalized rectangle (used to derive a tap point from a
/// detected NP slot's `card_region`).
pub(crate) fn rect_center(r: &NormRect) -> Point {
    Point::new(r.x + r.w / 2.0, r.y + r.h / 2.0)
}

mod advanced_runtime;
mod critical;
mod runtime;
mod selection_runtime;
pub(crate) use critical::*;
