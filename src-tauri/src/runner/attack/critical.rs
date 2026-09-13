//! Critical-mode card scoring, chain selection, and owner alternation.

use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CriticalPickSummary {
    pub(crate) chain: Option<CriticalChainType>,
    pub(crate) bonus: Option<CriticalBonusType>,
    pub(crate) relaxed_alternation: bool,
    pub(crate) relaxed_reason: Option<&'static str>,
    pub(crate) owner_labels: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CriticalBonusType {
    MightyChain,
    QuickChain,
    QuickFirst,
}

fn critical_chain(cards: [&CommandCardMatch; 3]) -> Option<CriticalChainType> {
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
    match counts {
        (1, 1, 1) => Some(CriticalChainType::Mighty),
        (3, 0, 0) => Some(CriticalChainType::Buster),
        (0, 3, 0) => Some(CriticalChainType::Arts),
        (0, 0, 3) => Some(CriticalChainType::Quick),
        _ => None,
    }
}

fn critical_bonus(
    cards: [&CommandCardMatch; 3],
    chain: Option<CriticalChainType>,
) -> Option<CriticalBonusType> {
    match chain {
        Some(CriticalChainType::Mighty) => Some(CriticalBonusType::MightyChain),
        Some(CriticalChainType::Quick) => Some(CriticalBonusType::QuickChain),
        _ if cards[0].suit.as_deref() == Some("q") => Some(CriticalBonusType::QuickFirst),
        _ => None,
    }
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

pub(crate) fn choose_critical_picks(
    cards: &[CommandCardMatch],
    members: &[Option<PartyMemberRuntime>; 3],
    strategy: &CriticalAttackStrategy,
) -> (Vec<Pick>, CriticalPickSummary) {
    let mut actionable: Vec<&CommandCardMatch> =
        cards.iter().filter(|card| !card.is_stunned).collect();
    actionable.sort_by_key(|card| card.slot);

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
                bonus: None,
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
        bonus: Option<CriticalBonusType>,
        has_three_full_crit: bool,
        effective_crit_priority: [std::cmp::Reverse<u32>; 3],
        member_ranks: [usize; 3],
        slots: [u32; 3],
    }

    // Compare every ordered three-card choice from the full five-card hand so
    // that Mighty/Quick bonuses can change which cards have the best effective
    // critical rates instead of ranking only a preselected three-card subset.
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
                let chain = critical_chain(ordered);
                let bonus = critical_bonus(ordered, chain);
                let bonus_value = if bonus.is_some() { 20 } else { 0 };
                let effective_crit = ordered.map(|card| {
                    card.crit_chance
                        .unwrap_or(0)
                        .saturating_add(bonus_value)
                        .min(100)
                });
                let has_three_full_crit = effective_crit.iter().all(|chance| *chance == 100);
                let mut effective_crit_priority = effective_crit;
                effective_crit_priority.sort_unstable_by(|left, right| right.cmp(left));
                let effective_crit_priority = effective_crit_priority.map(std::cmp::Reverse);
                let member_ranks = ordered.map(|card| {
                    member_priority_rank(
                        card,
                        command_card_member(card, members),
                        &strategy.member_priority,
                    )
                });
                let slots = ordered.map(|card| card.slot);
                candidates.push(Candidate {
                    cards: ordered,
                    owners,
                    chain,
                    bonus,
                    has_three_full_crit,
                    effective_crit_priority,
                    member_ranks,
                    slots,
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
    let candidates = candidates
        .into_iter()
        .filter(|candidate| !has_alternating || alternating(candidate))
        .collect::<Vec<_>>();
    let has_three_full_crit = candidates
        .iter()
        .any(|candidate| candidate.has_three_full_crit);
    let has_prioritized_chain = !has_three_full_crit
        && candidates.iter().any(|candidate| {
            matches!(
                candidate.chain,
                Some(CriticalChainType::Quick | CriticalChainType::Mighty)
            )
        });
    let has_quick_first = !has_three_full_crit
        && !has_prioritized_chain
        && candidates
            .iter()
            .any(|candidate| candidate.bonus == Some(CriticalBonusType::QuickFirst));
    let chain_priority = normalized_critical_chain_priority(&strategy.chain_priority);
    let best = candidates
        .into_iter()
        .filter(|candidate| {
            if has_three_full_crit {
                candidate.has_three_full_crit
            } else if has_prioritized_chain {
                matches!(
                    candidate.chain,
                    Some(CriticalChainType::Quick | CriticalChainType::Mighty)
                )
            } else if has_quick_first {
                candidate.bonus == Some(CriticalBonusType::QuickFirst)
            } else {
                true
            }
        })
        .min_by_key(|candidate| {
            let chain_rank = if has_three_full_crit || has_prioritized_chain {
                candidate
                    .chain
                    .and_then(|chain| chain_priority.iter().position(|item| *item == chain))
                    .unwrap_or(chain_priority.len())
            } else {
                0
            };
            (
                chain_rank,
                candidate.effective_crit_priority,
                candidate.member_ranks,
                candidate.slots,
            )
        })
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
            bonus: best.bonus,
            relaxed_alternation: !has_alternating,
            relaxed_reason,
            owner_labels,
        },
    )
}
