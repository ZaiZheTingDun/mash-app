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
fn parse_servant_skill_maps_prefers_cn_names_and_extracts_icon_filenames() {
    let jp = json!({
        "skills": [
            { "id": 11, "name": "JP 一技", "icon": "https://cdn.example.com/icons/skill_11.png" },
            { "id": 22, "name": "JP 二技", "icon": "skill_22.png" }
        ]
    });
    let cn = json!({
        "skills": [
            { "id": 11, "name": "CN 一技" }
        ]
    });

    let maps = parse_servant_skill_maps(&jp, Some(&cn));

    assert_eq!(
        maps.icon_map.get(&11).map(String::as_str),
        Some("skill_11.png")
    );
    assert_eq!(
        maps.icon_map.get(&22).map(String::as_str),
        Some("skill_22.png")
    );
    assert_eq!(maps.name_map.get(&11).map(String::as_str), Some("CN 一技"));
    assert_eq!(maps.name_map.get(&22).map(String::as_str), Some("JP 二技"));
}

#[test]
fn parse_servant_skill_maps_extracts_ally_target_skill_types() {
    let jp = json!({
        "skills": [
            {
                "id": 11,
                "name": "target ally",
                "functions": [
                    { "funcTargetType": "ptOneOther" },
                    { "funcTargetType": "enemy" },
                    { "funcTargetType": "ptOne" },
                    { "funcTargetType": "ptOne" }
                ]
            },
            {
                "id": 22,
                "name": "no ally target",
                "functions": [
                    { "funcTargetType": "enemy" },
                    { "funcTargetType": "ptAll" }
                ]
            },
            { "id": 33, "name": "no functions" }
        ]
    });

    let maps = parse_servant_skill_maps(&jp, None);

    assert_eq!(
        maps.target_type_map.get(&11),
        Some(&vec!["ptOne".to_string(), "ptOneOther".to_string()])
    );
    assert!(maps.target_type_map.get(&22).is_some_and(Vec::is_empty));
    assert!(maps.target_type_map.get(&33).is_some_and(Vec::is_empty));
}

#[test]
fn parse_servant_skill_maps_extracts_select_add_info_and_prefers_cn_labels() {
    let jp = json!({
        "skills": [
            {
                "id": 11,
                "name": "JP skill",
                "script": {
                    "SelectAddInfo": [
                        { "btn": [{ "name": "JP A" }, { "name": "JP B" }] },
                        { "btn": [{ "name": "JP ignored" }] }
                    ]
                }
            }
        ]
    });
    let cn = json!({
        "skills": [
            {
                "id": 11,
                "script": {
                    "SelectAddInfo": [
                        { "btn": [{ "name": "CN A" }, { "name": "CN B" }] }
                    ]
                }
            }
        ]
    });

    let maps = parse_servant_skill_maps(&jp, Some(&cn));
    let entry = maps.selection_map.get(&11).unwrap();

    assert_eq!(entry.selection_type, "SelectAddInfo");
    assert_eq!(
        entry.options,
        vec![
            SkillSelectionOption {
                index: 0,
                label: "CN A".into(),
            },
            SkillSelectionOption {
                index: 1,
                label: "CN B".into(),
            },
        ]
    );
}

#[test]
fn parse_servant_skill_maps_prefers_treasure_device_ui_over_command_type() {
    let jp = json!({
        "skills": [
            {
                "id": 22,
                "name": "NP selector",
                "script": {
                    "selectTreasureDeviceInfo": [
                        {
                            "treasureDevices": [
                                { "message": "攻击" },
                                { "message": "防御" }
                            ]
                        }
                    ]
                },
                "functions": [
                    {
                        "funcTargetType": "commandTypeSelfTreasureDevice",
                        "funcPopupText": "Arts",
                        "svals": [{ "ActSet": 1 }, { "ActSet": 2 }]
                    }
                ]
            }
        ]
    });

    let maps = parse_servant_skill_maps(&jp, None);
    let entry = maps.selection_map.get(&22).unwrap();

    assert_eq!(entry.selection_type, "selectTreasureDeviceInfo");
    assert_eq!(
        entry.supplementary_types,
        vec!["commandTypeSelfTreasureDevice"]
    );
    assert_eq!(entry.options[0].label, "攻击");
    assert_eq!(entry.options[1].label, "防御");
}

#[test]
fn parse_servant_skill_maps_extracts_command_type_act_sets() {
    let jp = json!({
        "skills": [
            {
                "id": 33,
                "name": "Card switch",
                "functions": [
                    {
                        "funcTargetType": "commandTypeSelfTreasureDevice",
                        "funcPopupText": "Arts",
                        "svals": [{ "ActSet": "arts" }]
                    },
                    {
                        "funcTargetType": "commandTypeSelfTreasureDevice",
                        "funcPopupText": "Quick",
                        "svals": [{ "ActSet": "quick" }]
                    },
                    {
                        "funcTargetType": "commandTypeSelfTreasureDevice",
                        "buffs": [{ "name": "Buster" }],
                        "svals": [{ "ActSet": "buster" }, { "ActSet": "buster" }]
                    }
                ]
            }
        ]
    });

    let maps = parse_servant_skill_maps(&jp, None);
    let entry = maps.selection_map.get(&33).unwrap();

    assert_eq!(entry.selection_type, "commandTypeSelfTreasureDevice");
    assert_eq!(
        entry.options,
        vec![
            SkillSelectionOption {
                index: 0,
                label: "Quick".into(),
            },
            SkillSelectionOption {
                index: 1,
                label: "Arts".into(),
            },
            SkillSelectionOption {
                index: 2,
                label: "Buster".into(),
            },
        ]
    );
}

#[test]
fn variant_skill_ids_uses_last_static_entry_for_each_slot() {
    let variants = HashMap::from([(
        42,
        vec![json!({
            "skills": {
                "1": [
                    { "id": 1001 },
                    { "id": 1002 },
                    { "id": 1003, "runtime": true }
                ],
                "2": [{ "id": 2001 }],
                "3": [{ "id": 3001, "runtime": true }]
            }
        })],
    )]);

    let ids = variant_skill_ids(&variants, 42, "42:1").unwrap();

    assert_eq!(ids, [Some(1002), Some(2001), None]);
}

#[test]
fn variant_skill_ids_returns_none_for_missing_variant() {
    let variants = HashMap::from([(42, vec![json!({ "skills": {} })])]);

    assert!(variant_skill_ids(&variants, 42, "42:2").is_none());
    assert!(variant_skill_ids(&variants, 7, "7:1").is_none());
}

#[test]
fn mash_runtime_skill_does_not_replace_the_latest_static_icon_form() {
    let static_ids = variant_skill_ids(variants_raw_data(), 1, "1:3").unwrap();
    let form_ids = variant_skill_form_ids(variants_raw_data(), 1, "1:3").unwrap();

    // Asset v9 adds a later non-runtime skill form (2550) after 970660.
    // Keep choosing that latest static form while retaining the runtime
    // form for targeting metadata.
    assert_eq!(static_ids[1], Some(2550));
    assert!(form_ids[1].contains(&2477450));
}

#[test]
fn variant_skill_form_ids_returns_every_form_in_each_slot() {
    let variants = HashMap::from([(
        42,
        vec![json!({
            "skills": {
                "1": [{ "id": 1001 }, { "id": 1002 }, { "id": 1001 }],
                "2": [{ "id": 2001, "runtime": true }],
                "3": []
            }
        })],
    )]);

    let ids = variant_skill_form_ids(&variants, 42, "42:1").unwrap();

    assert_eq!(ids, [vec![1001, 1002], vec![2001], vec![]]);
}

#[test]
fn resolve_skill_targeting_entry_reads_runtime_skill_and_marks_mixed() {
    let maps = parse_servant_skill_maps(
        &json!({
            "skills": [{
                "id": 2550,
                "functions": [{ "funcTargetType": "ptOne" }]
            }]
        }),
        None,
    );
    let temp = tempfile::tempdir().unwrap();
    let runtime_dir = temp.path().join("2477450");
    fs::create_dir_all(&runtime_dir).unwrap();
    fs::write(
        runtime_dir.join("skill.json"),
        serde_json::to_vec(&json!({
            "id": 2477450,
            "functions": [{ "funcTargetType": "self" }]
        }))
        .unwrap(),
    )
    .unwrap();

    let entry = resolve_skill_targeting_entry(1, 2, &[2550, 2477450], &maps, temp.path()).unwrap();

    assert_eq!(entry.skill_id, 2477450);
    assert_eq!(entry.skill_num, 2);
    assert_eq!(entry.func_target_types, vec!["ptOne"]);
    assert_eq!(entry.targeting_mode, SkillTargetingMode::Mixed);
}

#[test]
fn resolve_skill_targeting_entry_stays_unknown_when_a_form_is_missing() {
    let maps = parse_servant_skill_maps(
        &json!({
            "skills": [{
                "id": 2550,
                "functions": [{ "funcTargetType": "ptOne" }]
            }]
        }),
        None,
    );
    let temp = tempfile::tempdir().unwrap();

    let entry = resolve_skill_targeting_entry(1, 2, &[2550, 2477450], &maps, temp.path()).unwrap();

    assert_eq!(entry.targeting_mode, SkillTargetingMode::Unknown);
}
