//! Normal and advanced attack pick helpers.
//!
//! This module owns command-card priority parsing, fallback command-card
//! selection, and non-Grand advanced startup conditions.

use super::*;

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
        })
        .collect()
}

pub(crate) fn advanced_rule_matches(
    rule: &AdvancedRule,
    cards: &[CommandCardMatch],
    nps: &[NoblePhantasmMatch],
    party_ids: &[Option<u32>; 3],
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
            group
                .cards
                .iter()
                .all(|condition| command_condition_matches(condition, cards, party_ids))
        })
    };
    np_matches && command_matches
}

pub(crate) fn should_retry_command_card_owner_detection(
    cards: &[CommandCardMatch],
    candidate_ids: &[u32],
) -> bool {
    !candidate_ids.is_empty()
        && (cards.len() < COMMAND_CARD_COUNT || cards.iter().any(|card| card.servant_id.is_none()))
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
        if card.servant_id != Some(expected_id) {
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
) -> bool {
    if condition.servant != "any" {
        let Some(index) = parse_index(&condition.servant, "servant_") else {
            return false;
        };
        let Some(expected_id) = party_ids.get(index).copied().flatten() else {
            return false;
        };
        if card.servant_id != Some(expected_id) {
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
    let wanted_suit = if kind == "all" {
        None
    } else {
        Some(suit_code(kind)?)
    };

    let mut best: Option<&CommandCardMatch> = None;
    for c in cards {
        if used_card_slots.contains(&c.slot) {
            continue;
        }
        if c.servant_id != Some(wanted_id) {
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
/// cards, preserving the user's configured three-card order.
pub(crate) fn fill_empty_pick_slots(
    picks: &mut [Option<Pick>],
    cards: &[CommandCardMatch],
    used_card_slots: &mut HashSet<u32>,
) {
    let mut sorted: Vec<&CommandCardMatch> = cards.iter().collect();
    sorted.sort_by_key(|c| c.slot);

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
/// command cards that have not yet been used.
pub(crate) fn fill_remaining(
    picks: &mut Vec<Pick>,
    cards: &[CommandCardMatch],
    used_card_slots: &mut HashSet<u32>,
) {
    let mut sorted: Vec<&CommandCardMatch> = cards.iter().collect();
    sorted.sort_by_key(|c| c.slot);
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

pub(crate) fn advanced_startup_conditions_match(
    scene: &AdvancedBattleScene,
    cards: &[CommandCardMatch],
    party_ids: &[Option<u32>; 3],
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
    startup_conditions_match_from(0, &active, cards, party_ids, &mut used_slots)
}

pub(crate) fn startup_conditions_match_from(
    condition_index: usize,
    conditions: &[&AdvancedCommandCardCondition],
    cards: &[CommandCardMatch],
    party_ids: &[Option<u32>; 3],
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
        if !command_condition_matches_card(condition, card, party_ids) {
            continue;
        }
        used_slots.insert(card.slot);
        if startup_conditions_match_from(
            condition_index + 1,
            conditions,
            cards,
            party_ids,
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
        || !scene.startup_actions.is_empty()
}
