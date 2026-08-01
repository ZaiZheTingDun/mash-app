use super::*;

pub(super) struct ExtraStrategy {
    class: GrandClass,
    label: &'static str,
    servant_class: &'static str,
    selection_group: &'static str,
    selection_group_label: &'static str,
    selection_option_label: &'static str,
    aoe_stage: bool,
}

impl ExtraStrategy {
    pub(super) const fn fire() -> Self {
        Self {
            class: GrandClass::Extra1Fire,
            label: "Extra1 · 火",
            servant_class: "Extra1",
            selection_group: "extra1",
            selection_group_label: "额外职阶 Ⅰ 冠位",
            selection_option_label: "火",
            aoe_stage: false,
        }
    }

    pub(super) const fn earth() -> Self {
        Self {
            class: GrandClass::Extra1Earth,
            label: "Extra1 · 地",
            servant_class: "Extra1",
            selection_group: "extra1",
            selection_group_label: "额外职阶 Ⅰ 冠位",
            selection_option_label: "地",
            aoe_stage: true,
        }
    }

    pub(super) const fn wind() -> Self {
        Self {
            class: GrandClass::Extra2Wind,
            label: "Extra2 · 风",
            servant_class: "Extra2",
            selection_group: "extra2",
            selection_group_label: "额外职阶 Ⅱ 冠位",
            selection_option_label: "风",
            aoe_stage: false,
        }
    }

    pub(super) const fn water() -> Self {
        Self {
            class: GrandClass::Extra2Water,
            label: "Extra2 · 水",
            servant_class: "Extra2",
            selection_group: "extra2",
            selection_group_label: "额外职阶 Ⅱ 冠位",
            selection_option_label: "水",
            aoe_stage: true,
        }
    }
}

impl GrandClassStrategy for ExtraStrategy {
    fn class(&self) -> GrandClass {
        self.class
    }

    fn definition(&self) -> GrandClassDefinition {
        let roles = if self.aoe_stage {
            vec![
                GrandRoleDefinition {
                    role: "aoe".into(),
                    label: "光炮".into(),
                    required: true,
                },
                GrandRoleDefinition {
                    role: "single".into(),
                    label: "单体".into(),
                    required: false,
                },
            ]
        } else {
            vec![
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
            ]
        };
        GrandClassDefinition {
            id: self.class,
            label: self.label.into(),
            servant_class: self.servant_class.into(),
            selection_group: Some(self.selection_group.into()),
            selection_group_label: Some(self.selection_group_label.into()),
            selection_option_label: Some(self.selection_option_label.into()),
            roles,
            card_priority_enabled: false,
            auto_order_change_roles: if self.aoe_stage {
                vec!["aoe".into(), "single".into()]
            } else {
                vec!["main".into()]
            },
            validation_message: if self.aoe_stage {
                format!("{}戴冠战需要选择光炮从者，单体从者可选", self.label)
            } else {
                format!("{}戴冠战需要选择主冠位，副冠位可选", self.label)
            },
        }
    }

    fn built_in_rules(&self, _strategy: &GrandCardStrategy) -> Vec<GrandCardRule> {
        built_in_rules()
    }

    fn incomplete_candidate_priority(&self) -> Option<RuleCandidatePriority> {
        Some(RuleCandidatePriority::MainDeputyOtherThenBusterArtsQuick)
    }
}

fn filler_slot() -> RuleSlot {
    with_candidate_priority(
        slot(RuleOwner::Any, RuleKind::Command, RuleColor::Any),
        RuleCandidatePriority::MainDeputyOtherThenBusterArtsQuick,
    )
}

fn dual_np_rule(same_color: bool) -> GrandCardRule {
    GrandCardRule {
        slots: [
            slot(RuleOwner::MainGrand, RuleKind::Np, RuleColor::Any),
            slot(RuleOwner::DeputyGrand, RuleKind::Np, RuleColor::Any),
            filler_slot(),
        ],
        same_color,
        color_set_baq: false,
        include: Vec::new(),
        exclude: Vec::new(),
        target_role: Some(GrandRole::Main),
    }
}

fn main_brave_rule(same_color: bool) -> GrandCardRule {
    GrandCardRule {
        slots: [
            slot(RuleOwner::MainGrand, RuleKind::Np, RuleColor::Any),
            slot(RuleOwner::MainGrand, RuleKind::Command, RuleColor::Any),
            slot(RuleOwner::MainGrand, RuleKind::Command, RuleColor::Any),
        ],
        same_color,
        color_set_baq: false,
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
        dual_np_rule(true),
        dual_np_rule(false),
        main_brave_rule(true),
        main_brave_rule(false),
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
