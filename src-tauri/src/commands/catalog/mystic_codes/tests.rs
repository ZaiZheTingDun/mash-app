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

#[test]
fn mystic_code_catalog_exposes_master_figure_and_face_paths() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("470");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("mystic-code.json"),
        r#"{"name":"Test","skills":[]}"#,
    )
    .unwrap();
    for kind in ["figure", "face"] {
        for gender in ["female", "male"] {
            std::fs::write(dir.join(format!("master-{kind}-{gender}.png")), []).unwrap();
        }
    }

    let codes = load_mystic_codes_from_dir(temp.path());
    assert_eq!(codes.len(), 1);
    assert_eq!(
        codes[0].master_figure_female_path.as_deref(),
        Some(dir.join("master-figure-female.png").to_str().unwrap())
    );
    assert_eq!(
        codes[0].master_figure_male_path.as_deref(),
        Some(dir.join("master-figure-male.png").to_str().unwrap())
    );
    assert_eq!(
        codes[0].master_face_female_path.as_deref(),
        Some(dir.join("master-face-female.png").to_str().unwrap())
    );
    assert_eq!(
        codes[0].master_face_male_path.as_deref(),
        Some(dir.join("master-face-male.png").to_str().unwrap())
    );
}
