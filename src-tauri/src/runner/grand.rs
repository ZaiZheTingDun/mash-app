//! Grand and advanced automatic card strategy helpers.
//!
//! This module owns advanced auto-pick candidate scoring, Grand servant chain
//! rules, custom Grand card rules, and Grand-class-specific card ordering.

use super::*;

mod berserker;
mod extra;
mod lancer;
mod rider;
mod saber;

use berserker::BerserkerStrategy;
use extra::ExtraStrategy;
use lancer::LancerStrategy;
use rider::RiderStrategy;
use saber::SaberStrategy;

static SABER_STRATEGY: SaberStrategy = SaberStrategy;
static LANCER_STRATEGY: LancerStrategy = LancerStrategy;
static RIDER_STRATEGY: RiderStrategy = RiderStrategy;
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

pub(crate) fn grand_class_strategies() -> [&'static dyn GrandClassStrategy; 8] {
    [
        &SABER_STRATEGY,
        &LANCER_STRATEGY,
        &RIDER_STRATEGY,
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
    pub(crate) is_support: bool,
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

pub(crate) fn grand_role_for_candidate(
    candidate: &AdvancedPickCandidate,
    grand_servants: &[GrandServantRuntimeConfig],
) -> GrandRole {
    if let Some((index, _)) = candidate.servant_id.and_then(|servant_id| {
        grand_servants.iter().enumerate().find(|(_, config)| {
            config.servant_id == servant_id && config.is_support == candidate.is_support
        })
    }) {
        return match index {
            0 => GrandRole::Main,
            1 => GrandRole::Deputy,
            _ => GrandRole::Other,
        };
    }

    // A recognized servant id plus support flag is authoritative. Falling
    // back to the id alone would make an owned and support copy of the same
    // servant share one Grand role.
    if candidate.servant_id.is_some() {
        return GrandRole::Other;
    }

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
    GrandRole::Other
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

pub(crate) fn grand_config_for_candidate(
    servant_index: Option<usize>,
    servant_id: Option<u32>,
    is_support: bool,
    grand_servants: &[GrandServantRuntimeConfig],
) -> Option<&GrandServantRuntimeConfig> {
    if let Some(servant_id) = servant_id {
        return grand_servants
            .iter()
            .find(|config| config.servant_id == servant_id && config.is_support == is_support);
    }
    servant_index.and_then(|index| {
        grand_servants
            .iter()
            .find(|config| config.slot_index == index)
    })
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

mod rules;
pub(crate) use rules::*;

#[cfg(test)]
pub(crate) fn choose_ordinary_advanced_picks(
    cards: &[CommandCardMatch],
    nps: &[NoblePhantasmMatch],
    members: &[Option<PartyMemberRuntime>; 3],
    strategy: &AdvancedCardStrategy,
) -> (Vec<Pick>, Option<String>) {
    choose_ordinary_advanced_picks_with_crit(cards, nps, members, strategy, false)
}

pub(crate) fn choose_ordinary_advanced_picks_with_crit(
    cards: &[CommandCardMatch],
    nps: &[NoblePhantasmMatch],
    members: &[Option<PartyMemberRuntime>; 3],
    strategy: &AdvancedCardStrategy,
    prefer_higher_critical_chance: bool,
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
            is_support: servant_index
                .and_then(|index| members[index].as_ref())
                .is_some_and(|member| member.is_support),
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
            is_support: card.is_support,
            color: card.suit.clone(),
            original_order: 10
                + command_card_preference_order(card, cards, prefer_higher_critical_chance),
            is_np: false,
        });
    }
    candidates.sort_by_key(|candidate| candidate.original_order);

    for (name, rule) in ordinary_advanced_rules(strategy, members) {
        let picks = choose_with_grand_rules(&candidates, &[], vec![rule]);
        if !picks.is_empty() {
            return (picks, Some(name));
        }
    }

    let mut fallback: Vec<&CommandCardMatch> =
        cards.iter().filter(|card| !card.is_stunned).collect();
    fallback.sort_by_key(|card| {
        command_card_preference_order(card, cards, prefer_higher_critical_chance)
    });
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

#[cfg(test)]
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
    choose_advanced_auto_picks_with_crit(
        scene,
        cards,
        nps,
        party_ids,
        party_supports,
        grand_servants,
        grand_card_strategy,
        grand_class,
        false,
    )
}

pub(crate) fn choose_advanced_auto_picks_with_crit(
    scene: &AdvancedBattleScene,
    cards: &[CommandCardMatch],
    nps: &[NoblePhantasmMatch],
    party_ids: &[Option<u32>; 3],
    party_supports: &[bool; 3],
    grand_servants: &[GrandServantRuntimeConfig],
    grand_card_strategy: &GrandCardStrategy,
    grand_class: GrandClass,
    prefer_higher_critical_chance: bool,
) -> Vec<Pick> {
    let main_index = main_output_index(scene);
    let main_np_color = main_np_color(scene, party_ids);
    let mut candidates: Vec<AdvancedPickCandidate> = Vec::new();

    for np in nps.iter().filter(|np| np.ready) {
        let servant_index = Some(np.slot as usize).filter(|index| *index < 3);
        let servant_id = servant_index.and_then(|index| party_ids.get(index).copied().flatten());
        let color = if let Some(config) = grand_config_for_candidate(
            servant_index,
            servant_id,
            servant_index
                .and_then(|index| party_supports.get(index).copied())
                .unwrap_or(false),
            grand_servants,
        ) {
            grand_np_color(config).map(str::to_string)
        } else if grand_servants.is_empty() && servant_index == main_index {
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
            is_support: servant_index
                .and_then(|index| party_supports.get(index).copied())
                .unwrap_or(false),
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
            is_support: card.is_support,
            color: card.suit.clone(),
            original_order: 10
                + command_card_preference_order(card, cards, prefer_higher_critical_chance),
            is_np: false,
        });
    }
    candidates.sort_by_key(|candidate| candidate.original_order);

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
        assert!(grand_strategy(GrandClass::Rider)
            .validate_servants(&[servant(0, "main")])
            .is_ok());
        assert!(grand_strategy(GrandClass::Extra1Earth)
            .validate_servants(&[servant(0, "aoe")])
            .is_ok());
        assert!(grand_strategy(GrandClass::Extra1Earth)
            .validate_servants(&[servant(0, "single")])
            .is_err());
    }

    #[test]
    fn rider_definition_uses_extra_like_default_rules() {
        let rider = grand_class_definitions()
            .into_iter()
            .find(|definition| definition.id == GrandClass::Rider)
            .expect("Rider definition");
        assert_eq!(rider.label, "骑阶冠位");
        assert_eq!(rider.servant_class, "Rider");
        assert!(rider.selection_group.is_none());
        assert!(!rider.card_priority_enabled);
        assert_eq!(rider.roles[0].role, "main");
        assert!(rider.roles[0].required);
        assert_eq!(rider.roles[1].role, "deputy");
        assert!(!rider.roles[1].required);
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
