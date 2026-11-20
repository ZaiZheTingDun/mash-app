use super::*;

pub(super) struct ArcherStrategy;

impl GrandClassStrategy for ArcherStrategy {
    fn class(&self) -> GrandClass {
        GrandClass::Archer
    }

    fn definition(&self) -> GrandClassDefinition {
        standard_definition(
            GrandClass::Archer,
            "弓阶冠位",
            "Archer",
            false,
            "弓阶戴冠战需要选择主冠位，副冠位可选",
        )
    }

    fn built_in_rules(&self, _strategy: &GrandCardStrategy) -> Vec<GrandCardRule> {
        extra::default_built_in_rules()
    }

    fn incomplete_candidate_priority(&self) -> Option<RuleCandidatePriority> {
        Some(RuleCandidatePriority::MainDeputyOtherThenBusterArtsQuick)
    }
}
