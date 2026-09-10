use super::*;

pub(super) struct RiderStrategy;

impl GrandClassStrategy for RiderStrategy {
    fn class(&self) -> GrandClass {
        GrandClass::Rider
    }

    fn definition(&self) -> GrandClassDefinition {
        standard_definition(
            GrandClass::Rider,
            "骑阶冠位",
            "Rider",
            false,
            "骑阶戴冠战需要选择 1 到 2 名冠位从者",
        )
    }

    fn built_in_rules(&self, _strategy: &GrandCardStrategy) -> Vec<GrandCardRule> {
        extra::default_built_in_rules()
    }

    fn incomplete_candidate_priority(&self) -> Option<RuleCandidatePriority> {
        Some(RuleCandidatePriority::MainDeputyOtherThenBusterArtsQuick)
    }
}
