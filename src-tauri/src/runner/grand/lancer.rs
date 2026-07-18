use super::*;

pub(super) struct LancerStrategy;

impl GrandClassStrategy for LancerStrategy {
    fn class(&self) -> GrandClass {
        GrandClass::Lancer
    }

    fn definition(&self) -> GrandClassDefinition {
        GrandClassDefinition {
            id: GrandClass::Lancer,
            label: "枪阶冠位".into(),
            servant_class: "Lancer".into(),
            roles: vec![
                GrandRoleDefinition {
                    role: "single".into(),
                    label: "单体".into(),
                    required: true,
                },
                GrandRoleDefinition {
                    role: "aoe".into(),
                    label: "光炮".into(),
                    required: true,
                },
            ],
            card_priority_enabled: false,
            auto_order_change_roles: vec!["single".into(), "aoe".into()],
            validation_message: "枪阶戴冠战需要分别选择单体和光炮从者".into(),
        }
    }

    fn built_in_rules(&self, _strategy: &GrandCardStrategy) -> Vec<GrandCardRule> {
        built_in_rules()
    }

    fn incomplete_candidate_priority(&self) -> Option<RuleCandidatePriority> {
        Some(RuleCandidatePriority::MainDeputyOtherThenArtsQuickBuster)
    }
}

fn filler_slot() -> RuleSlot {
    with_candidate_priority(
        slot(RuleOwner::Any, RuleKind::Command, RuleColor::Any),
        RuleCandidatePriority::MainDeputyOtherThenArtsQuickBuster,
    )
}

fn dual_np_rule(same_color: bool, color_set_baq: bool) -> GrandCardRule {
    GrandCardRule {
        slots: [
            slot(RuleOwner::DeputyGrand, RuleKind::Np, RuleColor::Any),
            slot(RuleOwner::MainGrand, RuleKind::Np, RuleColor::Any),
            filler_slot(),
        ],
        same_color,
        color_set_baq,
        include: Vec::new(),
        exclude: Vec::new(),
        target_role: Some(GrandRole::Main),
    }
}

fn single_np_rule(owner: RuleOwner, role: GrandRole) -> GrandCardRule {
    GrandCardRule {
        slots: [
            slot(owner, RuleKind::Np, RuleColor::Any),
            filler_slot(),
            filler_slot(),
        ],
        same_color: false,
        color_set_baq: false,
        include: Vec::new(),
        exclude: Vec::new(),
        target_role: Some(role),
    }
}

fn built_in_rules() -> Vec<GrandCardRule> {
    vec![
        dual_np_rule(false, true),
        dual_np_rule(true, false),
        dual_np_rule(false, false),
        single_np_rule(RuleOwner::MainGrand, GrandRole::Main),
        single_np_rule(RuleOwner::DeputyGrand, GrandRole::Deputy),
        GrandCardRule {
            slots: [filler_slot(), filler_slot(), filler_slot()],
            same_color: false,
            color_set_baq: false,
            include: Vec::new(),
            exclude: Vec::new(),
            target_role: None,
        },
    ]
}
