use super::*;

pub(super) struct BerserkerStrategy;

impl GrandClassStrategy for BerserkerStrategy {
    fn class(&self) -> GrandClass {
        GrandClass::Berserker
    }

    fn definition(&self) -> GrandClassDefinition {
        standard_definition(
            GrandClass::Berserker,
            "狂阶冠位",
            "Berserker",
            true,
            "戴冠战需要选择 1 到 2 名冠位从者",
        )
    }

    fn built_in_rules(&self, _strategy: &GrandCardStrategy) -> Vec<GrandCardRule> {
        built_in_rules()
    }
}

fn built_in_rules() -> Vec<GrandCardRule> {
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
