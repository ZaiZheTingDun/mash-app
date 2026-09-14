use super::*;
use serde_json::json;

#[test]
fn mystic_code_skill_mode_distinguishes_target_none_and_order_change() {
    assert_eq!(
        mystic_code_skill_mode(&json!({
            "functions": [{ "funcTargetType": "ptOne" }, { "funcTargetType": "enemy" }]
        })),
        MysticCodeSkillMode::NeedsTarget
    );
    assert_eq!(
        mystic_code_skill_mode(&json!({
            "functions": [{ "funcTargetType": "enemy" }, { "funcTargetType": "self" }]
        })),
        MysticCodeSkillMode::NoTarget
    );
    assert_eq!(
        mystic_code_skill_mode(&json!({
            "functions": [{ "funcTargetType": "ptselectOneSub" }]
        })),
        MysticCodeSkillMode::OrderChange
    );
}
