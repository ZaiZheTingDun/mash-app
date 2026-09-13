//! Party lineup mutation, Order Change, and frontline availability helpers.

use super::*;

// ---------------------------------------------------------------------------
// Position mapping helpers
// ---------------------------------------------------------------------------

pub(crate) fn parse_index(s: &str, prefix: &str) -> Option<usize> {
    s.strip_prefix(prefix)
        .and_then(|n| n.parse::<usize>().ok())
        .map(|n| n.saturating_sub(1))
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct PartyMemberRuntime {
    pub(crate) member_id: Option<String>,
    pub(crate) slot_index: usize,
    pub(crate) servant_id: u32,
    pub(crate) is_support: bool,
}

pub(crate) fn party_member_ids(members: &[Option<PartyMemberRuntime>; 6]) -> [Option<u32>; 6] {
    [
        members[0].as_ref().map(|member| member.servant_id),
        members[1].as_ref().map(|member| member.servant_id),
        members[2].as_ref().map(|member| member.servant_id),
        members[3].as_ref().map(|member| member.servant_id),
        members[4].as_ref().map(|member| member.servant_id),
        members[5].as_ref().map(|member| member.servant_id),
    ]
}

pub(crate) fn frontline_party_members(
    members: &[Option<PartyMemberRuntime>; 6],
) -> [Option<PartyMemberRuntime>; 3] {
    [members[0].clone(), members[1].clone(), members[2].clone()]
}

pub(crate) fn frontline_party_ids_and_supports(
    members: &[Option<PartyMemberRuntime>; 3],
) -> ([Option<u32>; 3], [bool; 3]) {
    (
        [
            members[0].as_ref().map(|member| member.servant_id),
            members[1].as_ref().map(|member| member.servant_id),
            members[2].as_ref().map(|member| member.servant_id),
        ],
        [
            members[0].as_ref().is_some_and(|member| member.is_support),
            members[1].as_ref().is_some_and(|member| member.is_support),
            members[2].as_ref().is_some_and(|member| member.is_support),
        ],
    )
}

fn front_slot_with_most_cards_excluding(
    cards: &[CommandCardMatch],
    party_ids: &[Option<u32>; 3],
    party_supports: &[bool; 3],
    excluded: &HashSet<(u32, bool)>,
) -> Option<usize> {
    let mut counts = [0usize; 3];
    for card in cards {
        let Some(servant_id) = card.servant_id else {
            continue;
        };
        if let Some(index) = party_ids
            .iter()
            .position(|party_id| *party_id == Some(servant_id))
            .filter(|index| party_supports[*index] == card.is_support)
        {
            counts[index] += 1;
        }
    }
    (0..3)
        .filter(|index| {
            party_ids[*index].is_some_and(|id| !excluded.contains(&(id, party_supports[*index])))
        })
        .max_by_key(|index| (counts[*index], std::cmp::Reverse(*index)))
}

pub(crate) fn grand_auto_order_change_action(
    cards: &[CommandCardMatch],
    party_ids: &[Option<u32>; 3],
    party_supports: &[bool; 3],
    grand_servants: &[GrandServantRuntimeConfig],
    grand_class: GrandClass,
) -> Option<Action> {
    let target = grand_strategy(grand_class).auto_order_change_target(grand_servants)?;
    let protected_front: HashSet<(u32, bool)> = grand_servants
        .iter()
        .filter(|config| config.slot_index < 3)
        .map(|config| (config.servant_id, config.is_support))
        .collect();
    let front_index =
        front_slot_with_most_cards_excluding(cards, party_ids, party_supports, &protected_front)?;
    Some(Action::Equipment {
        id: "auto_grand_order_change".into(),
        skill: Some("skill_3".into()),
        target: None,
        target_member_id: None,
        target_servant_id: None,
        target_is_support: false,
        order_change: Some(crate::OrderChangeSelection {
            front: Some(format!("servant_{}", front_index + 1)),
            front_member_id: None,
            front_servant_id: party_ids[front_index],
            front_is_support: party_supports[front_index],
            back: Some(format!("servant_{}", target.slot_index + 1)),
            back_member_id: None,
            back_servant_id: Some(target.servant_id),
            back_is_support: target.is_support,
        }),
    })
}

/// Parse a template-key style servant name like ``"servant_215"`` into its
/// numeric id. Returns ``None`` for any other string shape so callers can
/// gracefully drop unknown supports instead of erroring.
pub(crate) fn parse_servant_name_id(name: &str) -> Option<u32> {
    name.strip_prefix("servant_")?.parse::<u32>().ok()
}

mod lineup;
pub(crate) use lineup::*;
mod resolution;
pub(crate) use resolution::*;
mod replay;
pub(crate) use replay::*;
mod runtime;
