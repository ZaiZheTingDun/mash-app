use super::*;

pub(super) struct SaberStrategy;

impl GrandClassStrategy for SaberStrategy {
    fn class(&self) -> GrandClass {
        GrandClass::Saber
    }

    fn definition(&self) -> GrandClassDefinition {
        standard_definition(
            GrandClass::Saber,
            "剑阶冠位",
            "Saber",
            true,
            "戴冠战需要选择 1 到 2 名冠位从者",
        )
    }

    fn built_in_rules(&self, strategy: &GrandCardStrategy) -> Vec<GrandCardRule> {
        normalized_grand_chain_priority(strategy)
            .into_iter()
            .flat_map(rules_for_priority_item)
            .collect()
    }
}

fn rules_for_priority_item(item: GrandChainPriorityItem) -> Vec<GrandCardRule> {
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
