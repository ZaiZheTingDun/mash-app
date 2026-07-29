use super::*;

/// Helper: assert two `f64` are approximately equal. The CE search
/// region math is just adds + multiplies on small constants, so the
/// epsilon is tight.
fn approx(a: f64, b: f64) {
    assert!(
        (a - b).abs() < 1e-9,
        "expected ≈{b}, got {a} (diff {})",
        (a - b).abs()
    );
}

fn rect() -> NormRect {
    NormRect {
        x: 0.0,
        y: 0.0,
        w: 0.1,
        h: 0.1,
    }
}

fn command_card(
    slot: u32,
    servant_id: Option<u32>,
    suit: Option<&str>,
    crit: Option<u32>,
) -> CommandCardMatch {
    command_card_with_state(slot, servant_id, false, false, suit, crit)
}

fn command_card_with_support(
    slot: u32,
    servant_id: Option<u32>,
    is_support: bool,
    suit: Option<&str>,
    crit: Option<u32>,
) -> CommandCardMatch {
    command_card_with_state(slot, servant_id, is_support, false, suit, crit)
}

fn command_card_with_state(
    slot: u32,
    servant_id: Option<u32>,
    is_support: bool,
    is_stunned: bool,
    suit: Option<&str>,
    crit: Option<u32>,
) -> CommandCardMatch {
    CommandCardMatch {
        slot,
        x: 0.0,
        y: 0.0,
        card_region: rect(),
        face_region: rect(),
        crit_digit_regions: None,
        crit_digit_reads: None,
        suit: suit.map(str::to_string),
        icon_score: None,
        icon_region: None,
        servant_id,
        is_support,
        is_stunned,
        ascension: None,
        face_score: None,
        crit_chance: crit,
        support_icon_score: None,
        support_icon_region: None,
    }
}

fn np_slot(slot: u32, ready: bool) -> NoblePhantasmMatch {
    NoblePhantasmMatch {
        slot,
        card_region: rect(),
        ready,
        edge_frac: 0.0,
        std_bgr: 0.0,
        edge_threshold: 0.0,
        card_ready: None,
        ready_source: None,
        gauge_digit_count: None,
        gauge_region: None,
        np_glow_region: None,
        np_glow_score: None,
        np_glow_ready: None,
    }
}

fn np_slot_with_detectors(
    slot: u32,
    ready: bool,
    card_ready: Option<bool>,
    glow_ready: Option<bool>,
) -> NoblePhantasmMatch {
    NoblePhantasmMatch {
        ready_source: Some("glow".into()),
        card_ready,
        np_glow_score: glow_ready.map(|ready| if ready { 0.6 } else { 0.4 }),
        np_glow_ready: glow_ready,
        ..np_slot(slot, ready)
    }
}

fn bond_level_read(level: Option<u32>) -> BondLevelUpReadResult {
    BondLevelUpReadResult {
        ok: level.is_some(),
        bond_level_after: level,
        servant_name: None,
        servant_name_matched: None,
        reason: None,
        servant_match_score: None,
        confidence: None,
        diagnostics: None,
    }
}

#[test]
fn bond_result_stop_action_ignores_normal_bond_page() {
    assert_eq!(
        bond_result_stop_action(true, None, false, false),
        BondResultStopAction::Continue
    );
    assert_eq!(
        bond_result_stop_action(false, None, true, true),
        BondResultStopAction::Continue
    );
}

#[test]
fn bond_result_stop_action_stops_on_any_level_up_when_enabled() {
    assert_eq!(
        bond_result_stop_action(true, None, true, false),
        BondResultStopAction::StopOnLevelUp
    );
}

#[test]
fn bond_result_stop_action_stops_on_max_level_threshold() {
    let below = bond_level_read(Some(9));
    let max = bond_level_read(Some(10));
    let above = bond_level_read(Some(11));

    assert_eq!(
        bond_result_stop_action(true, Some(&below), false, true),
        BondResultStopAction::Continue
    );
    assert_eq!(
        bond_result_stop_action(true, Some(&max), false, true),
        BondResultStopAction::StopOnMaxLevel
    );
    assert_eq!(
        bond_result_stop_action(true, Some(&above), false, true),
        BondResultStopAction::StopOnMaxLevel
    );
}

#[test]
fn bond_result_stop_action_continues_when_max_level_read_fails() {
    let failed = bond_level_read(None);

    assert_eq!(
        bond_result_stop_action(true, Some(&failed), false, true),
        BondResultStopAction::Continue
    );
    assert_eq!(
        bond_result_stop_action(true, None, false, true),
        BondResultStopAction::Continue
    );
}

#[test]
fn card_np_detection_mode_uses_legacy_card_ready_signal() {
    let mut nps = vec![
        np_slot_with_detectors(0, false, Some(true), Some(false)),
        np_slot_with_detectors(1, true, Some(false), Some(true)),
    ];

    apply_np_detection_mode(
        &mut nps,
        crate::commands::settings::NoblePhantasmDetectionMode::Card,
    );

    assert!(nps[0].ready);
    assert!(!nps[1].ready);
    assert_eq!(nps[0].ready_source.as_deref(), Some("card"));
    assert_eq!(nps[1].ready_source.as_deref(), Some("card"));
}

#[test]
fn gauge_np_detection_mode_preserves_current_glow_signal() {
    let mut nps = vec![np_slot_with_detectors(0, false, Some(true), Some(false))];

    apply_np_detection_mode(
        &mut nps,
        crate::commands::settings::NoblePhantasmDetectionMode::Gauge,
    );

    assert!(!nps[0].ready);
    assert_eq!(nps[0].ready_source.as_deref(), Some("glow"));
}

#[test]
fn attack_log_command_cards_keep_slot_suit_and_servant_id() {
    let mut cards = vec![
        command_card(0, Some(309), Some("q"), None),
        command_card(1, None, Some("b"), None),
        command_card(2, Some(16), None, None),
    ];
    cards[1].is_stunned = true;

    assert_eq!(
        command_cards_log_meta(&cards),
        vec![
            AttackLogCommandCard {
                slot: 0,
                suit: Some("q".into()),
                servant_id: Some(309),
                is_support: false,
                is_stunned: false,
            },
            AttackLogCommandCard {
                slot: 1,
                suit: Some("b".into()),
                servant_id: None,
                is_support: false,
                is_stunned: true,
            },
            AttackLogCommandCard {
                slot: 2,
                suit: None,
                servant_id: Some(16),
                is_support: false,
                is_stunned: false,
            },
        ]
    );
}

#[test]
fn attack_log_meta_serializes_selected_pick_in_camel_case() {
    let meta = AttackLogMeta {
        front_servant_ids: [Some(284), Some(16), Some(309)],
        candidate_servant_ids: Some(vec![284, 16, 309]),
        command_cards: None,
        ready_np_slots: Some(vec![2]),
        selected_pick: Some(AttackLogSelectedPick {
            step: 1,
            total: 3,
            from_priority: Some("servant_3_np".into()),
            kind: AttackLogPickKind::Np,
            slot: 2,
            suit: None,
            servant_id: Some(309),
        }),
    };

    let json = serde_json::to_value(meta).unwrap();

    assert_eq!(
        json,
        serde_json::json!({
            "frontServantIds": [284, 16, 309],
            "candidateServantIds": [284, 16, 309],
            "readyNpSlots": [2],
            "selectedPick": {
                "step": 1,
                "total": 3,
                "fromPriority": "servant_3_np",
                "kind": "np",
                "slot": 2,
                "servantId": 309
            }
        })
    );
}

#[test]
fn action_log_meta_serializes_skill_icons_in_camel_case() {
    let meta = ActionLogMeta::ServantSkill {
        servant_id: Some(309),
        skill_index: 2,
        target_servant_id: Some(16),
    };

    assert_eq!(
        serde_json::to_value(meta).unwrap(),
        serde_json::json!({
            "kind": "servantSkill",
            "servantId": 309,
            "skillIndex": 2,
            "targetServantId": 16,
        })
    );
}

#[test]
fn skill_failure_labels_identify_actor_and_skill() {
    assert_eq!(
        servant_skill_failure_label(Some("servant_2"), Some(309), 0),
        "从者 servant_2 (#309) 技能 1"
    );
    assert_eq!(
        equipment_skill_failure_label(2, true),
        "御主技能 3 / Order Change"
    );
    assert_eq!(command_spell_failure_label("restore"), "令咒 回复");
}

fn empty_advanced_scene() -> AdvancedBattleScene {
    AdvancedBattleScene {
        id: "advanced_scene_1".into(),
        enemy_target: None,
        main_output: None,
        grand_auto_order_change: None,
        command_conditions: Vec::new(),
        control_actions: Vec::new(),
        turns: Vec::new(),
        startup_actions: Vec::new(),
        rules: Vec::new(),
    }
}

fn normal_turn(preparation_actions: Vec<Action>, attack_priority: Vec<AttackCard>) -> BattleTurn {
    BattleTurn {
        id: "turn_1".into(),
        preparation_actions,
        servant_actions: Vec::new(),
        equipment_actions: Vec::new(),
        command_spell_actions: Vec::new(),
        enemy_target: None,
        attack_priority,
    }
}

fn normal_scene(id: &str, turns: Vec<BattleTurn>) -> BattleScene {
    BattleScene {
        id: id.into(),
        turns,
        preparation_actions: Vec::new(),
        servant_actions: Vec::new(),
        equipment_actions: Vec::new(),
        command_spell_actions: Vec::new(),
        enemy_target: None,
        attack_priority: Vec::new(),
    }
}

fn grand_config(servant_id: u32, np_card: &str, priority: &str) -> GrandServantRuntimeConfig {
    grand_config_at(0, servant_id, np_card, priority)
}

fn grand_config_at(
    slot_index: usize,
    servant_id: u32,
    np_card: &str,
    priority: &str,
) -> GrandServantRuntimeConfig {
    GrandServantRuntimeConfig {
        slot_index,
        servant_id,
        is_support: false,
        np_card: np_card.into(),
        priority: priority.into(),
        role: if servant_id == 20 { "deputy" } else { "main" }.into(),
    }
}

fn pick_labels(picks: &[Pick]) -> Vec<String> {
    picks
        .iter()
        .map(|pick| match pick {
            Pick::Card { slot, .. } => format!("C{slot}"),
            Pick::Np { slot, .. } => format!("NP{slot}"),
        })
        .collect()
}

#[test]
fn battle_result_popup_skip_only_applies_to_result_screens() {
    assert!(is_battle_result_screen(Screen::BattleResultBond));
    assert!(is_battle_result_screen(Screen::BattleResultExp));
    assert!(is_battle_result_screen(Screen::BattleResultLoot));
    assert!(is_battle_result_screen(Screen::BattleResultFriendRequest));
    assert!(is_battle_result_screen(Screen::BattleResultContinue));

    assert!(!is_battle_result_screen(Screen::Battle));
    assert!(!is_battle_result_screen(Screen::Attack));
    assert!(!is_battle_result_screen(Screen::SupportSelect));
    assert!(!is_battle_result_screen(Screen::APRecovery));
    assert!(!is_battle_result_screen(Screen::Unknown));
}

#[test]
fn unknown_screen_timeout_screenshot_path_uses_debug_directory() {
    let root = PathBuf::from("/tmp/mash-app-test");

    assert_eq!(
        unknown_screen_timeout_screenshot_dir_in_root(&root),
        root.join("debug").join("unknown-screen-timeouts")
    );
}

#[test]
fn unknown_screen_timeout_screenshot_filename_includes_timestamp_and_run() {
    let timestamp = std::time::UNIX_EPOCH + Duration::from_millis(12_345);

    assert_eq!(
        unknown_screen_timeout_screenshot_filename(timestamp, 2),
        "unknown-0000000012345-run0003.jpg"
    );
}

#[test]
fn ce_search_region_identity_row_returns_offset() {
    // A unit row at the origin → the absolute window equals the
    // raw `SUPPORT_CE_OFFSET_IN_ROW` (it's already in unit-row coords).
    let row = NormRect {
        x: 0.0,
        y: 0.0,
        w: 1.0,
        h: 1.0,
    };
    let out = ce_search_region(row);
    approx(out.x, SUPPORT_CE_OFFSET_IN_ROW.x);
    approx(out.y, SUPPORT_CE_OFFSET_IN_ROW.y);
    approx(out.w, SUPPORT_CE_OFFSET_IN_ROW.w);
    approx(out.h, SUPPORT_CE_OFFSET_IN_ROW.h);
}

#[test]
fn ce_search_region_scales_and_translates_offset_row() {
    // Row at (0.10, 0.20) sized (0.50, 0.10): the CE icon search
    // window is the row-local offset, scaled by row size, then
    // translated by row origin.
    let row = NormRect {
        x: 0.10,
        y: 0.20,
        w: 0.50,
        h: 0.10,
    };
    let out = ce_search_region(row);
    approx(out.x, 0.10 + SUPPORT_CE_OFFSET_IN_ROW.x * 0.50);
    approx(out.y, 0.20 + SUPPORT_CE_OFFSET_IN_ROW.y * 0.10);
    approx(out.w, SUPPORT_CE_OFFSET_IN_ROW.w * 0.50);
    approx(out.h, SUPPORT_CE_OFFSET_IN_ROW.h * 0.10);
}

#[test]
fn ce_search_region_handles_negative_offset() {
    // SUPPORT_CE_OFFSET_IN_ROW.x is negative on purpose (the CE icon
    // sits to the *left* of the OCR-anchored row strip). For a row
    // that starts at x=0.20 with w=0.40, the search window should
    // start to the *left* of the row origin.
    let row = NormRect {
        x: 0.20,
        y: 0.30,
        w: 0.40,
        h: 0.10,
    };
    let out = ce_search_region(row);
    assert!(
        out.x < row.x,
        "search window should be left of row origin: got x={} vs row x={}",
        out.x,
        row.x,
    );
}

#[test]
fn grand_ce_search_region_anchors_to_confirm_button() {
    let region = NormRect {
        x: 0.30,
        y: 0.40,
        w: 0.50,
        h: 0.10,
    };
    let mut row = support_row(None, vec![], vec![]);
    row.row_region = region;
    row.score_anchor = Some(NormRect {
        x: 0.80,
        y: 0.60,
        w: 0.04,
        h: 0.06,
    });
    let first = grand_ce_search_region(&row, 0).unwrap();
    let second = grand_ce_search_region(&row, 1).unwrap();
    let third = grand_ce_search_region(&row, 2).unwrap();

    assert!(first.y < second.y);
    assert!(second.y < third.y);
    approx(first.x, SUPPORT_GRAND_CE_X);
    approx(first.w, SUPPORT_GRAND_CE_W);
    approx(
        third.y + third.h / 2.0,
        0.60 + SUPPORT_GRAND_CE_THIRD_CENTER_FROM_BUTTON_TOP_Y,
    );
    assert!(grand_ce_search_region(&row, 3).is_none());
}

// --- RunConfig serde -----------------------------------------------

/// Build the smallest valid RunConfig JSON (omitting all
/// `#[serde(default)]` fields so the test exercises the defaults).
fn minimal_run_config_json() -> serde_json::Value {
    serde_json::json!({
        "projectId": "p1",
        "partyOrder": null,
        "supportClassFilter": null,
        "supportServantName": null,
        "servantSelections": [],
        "maxSupportScrolls": 5,
    })
}

#[test]
fn run_config_defaults_support_ce_to_none_when_field_missing() {
    let cfg: RunConfig = serde_json::from_value(minimal_run_config_json()).unwrap();
    assert!(cfg.support_craft_essence_id.is_none());
    assert_eq!(
        cfg.support_ce_threshold,
        crate::commands::settings::SUPPORT_CE_THRESHOLD_DEFAULT
    );
    assert_eq!(
        cfg.support_ce_full_gate_threshold,
        crate::commands::settings::SUPPORT_CE_FULL_GATE_THRESHOLD_DEFAULT
    );
    assert_eq!(
        cfg.support_mlb_icon_threshold,
        crate::commands::settings::SUPPORT_ICON_THRESHOLD_DEFAULT
    );
    assert_eq!(
        cfg.support_bond_icon_threshold,
        crate::commands::settings::SUPPORT_ICON_THRESHOLD_DEFAULT
    );
    assert!(!cfg.stop_on_bond_level_up);
    assert!(!cfg.stop_on_bond_max_level);
    assert_eq!(cfg.support_craft_essence_mlb_required, true);
    assert_eq!(cfg.support_grand_mode, false);
    assert_eq!(cfg.support_grand_craft_essence_ids, [None; 3]);
    assert_eq!(cfg.support_grand_craft_essence_mlb_required, [true; 3]);
    assert_eq!(cfg.support_grand_bond_ce_mode, SupportGrandBondCeMode::Any);
    assert!(cfg.grand_servants.is_empty());
    assert_eq!(
        cfg.grand_card_strategy.chain_priority,
        default_grand_chain_priority()
    );
    assert!(cfg.grand_card_strategy.custom_rules.is_empty());
    // Other defaults travel through the same path; sanity-check
    // them so legacy `projects.json` rows keep deserializing.
    assert!(cfg.support_servant_id.is_none());
    assert!(cfg.support_slot_index.is_none());
    assert!(cfg.support_noble_phantasm_level_min.is_none());
    assert_eq!(cfg.support_skill_level_mins, [None; 3]);
    assert_eq!(cfg.support_append_skill_level_mins, [None; 5]);
    assert_eq!(cfg.repeat_mission, false);
    assert_eq!(cfg.max_mission_runs, None);
    assert!(cfg.ap_recovery_items.is_empty());
    assert!(!cfg.verify_skill_activation);
    assert_eq!(
        cfg.unknown_screen_timeout_count,
        crate::commands::settings::UNKNOWN_SCREEN_TIMEOUT_COUNT_DEFAULT
    );
    assert!(!cfg.auto_capture_battle_result_loot);
    assert!(!cfg.auto_capture_unknown_screen_timeout);
    assert!(!cfg.auto_capture_skill_use_probe);
}

#[test]
fn run_config_round_trips_support_craft_essence_id() {
    let mut payload = minimal_run_config_json();
    payload["supportCraftEssenceId"] = serde_json::json!(1485);
    payload["supportSlotIndex"] = serde_json::json!(5);
    payload["supportGrandMode"] = serde_json::json!(true);
    payload["supportGrandCraftEssenceIds"] = serde_json::json!([1001, null, 1003]);
    payload["supportCraftEssenceMlbRequired"] = serde_json::json!(false);
    payload["supportCeFullGateThreshold"] = serde_json::json!(0.55);
    payload["supportMlbIconThreshold"] = serde_json::json!(0.76);
    payload["supportBondIconThreshold"] = serde_json::json!(0.78);
    payload["verifySkillActivation"] = serde_json::json!(true);
    payload["supportGrandCraftEssenceMlbRequired"] = serde_json::json!([true, false, true]);
    payload["supportGrandBondCeMode"] = serde_json::json!("bondNp");
    payload["grandServants"] = serde_json::json!([
        { "slotIndex": 0, "npCard": "buster", "priority": "damage" },
        { "slotIndex": 2, "npCard": "auto", "priority": "np" }
    ]);
    let cfg: RunConfig = serde_json::from_value(payload).unwrap();
    assert_eq!(cfg.support_craft_essence_id, Some(1485));
    assert_eq!(cfg.support_craft_essence_mlb_required, false);
    assert_eq!(cfg.support_ce_full_gate_threshold, 0.55);
    assert_eq!(cfg.support_mlb_icon_threshold, 0.76);
    assert_eq!(cfg.support_bond_icon_threshold, 0.78);
    assert!(cfg.verify_skill_activation);
    assert_eq!(cfg.support_slot_index, Some(5));
    assert_eq!(cfg.support_grand_mode, true);
    assert_eq!(
        cfg.support_grand_craft_essence_ids,
        [Some(1001), None, Some(1003)]
    );
    assert_eq!(
        cfg.support_grand_craft_essence_mlb_required,
        [true, false, true]
    );
    assert_eq!(
        cfg.support_grand_bond_ce_mode,
        SupportGrandBondCeMode::BondNp
    );
    assert_eq!(cfg.grand_servants.len(), 2);
    assert_eq!(cfg.grand_servants[0].slot_index, 0);
    assert_eq!(cfg.grand_servants[0].np_card, "buster");
    assert_eq!(cfg.grand_servants[1].priority, "np");

    // Re-serialize and confirm the field round-trips under the
    // camelCase rename rule applied to the whole struct.
    let json = serde_json::to_value(&cfg).unwrap();
    assert_eq!(json["supportCraftEssenceId"], serde_json::json!(1485));
    assert_eq!(json["supportSlotIndex"], serde_json::json!(5));
    assert_eq!(json["supportGrandMode"], serde_json::json!(true));
    assert_eq!(
        json["supportGrandCraftEssenceIds"],
        serde_json::json!([1001, null, 1003])
    );
    assert_eq!(
        json["supportCraftEssenceMlbRequired"],
        serde_json::json!(false)
    );
    assert_eq!(
        json["supportGrandCraftEssenceMlbRequired"],
        serde_json::json!([true, false, true])
    );
    assert_eq!(json["supportGrandBondCeMode"], serde_json::json!("bondNp"));
    assert_eq!(
        json["grandServants"],
        serde_json::json!([
            { "memberId": null, "slotIndex": 0, "servantId": null, "isSupport": false, "npCard": "buster", "priority": "damage" },
            { "memberId": null, "slotIndex": 2, "servantId": null, "isSupport": false, "npCard": "auto", "priority": "np" }
        ])
    );
}

#[test]
fn advanced_rule_matches_np_and_command_groups_with_and_between_types() {
    let rule = AdvancedRule {
        id: "rule".into(),
        np_condition_groups: vec![
            crate::AdvancedNpConditionGroup {
                id: "np1".into(),
                slots: vec![crate::AdvancedNpSlotCondition {
                    servant: "servant_1".into(),
                    ready: false,
                    member_id: None,
                    servant_id: None,
                    is_support: false,
                }],
            },
            crate::AdvancedNpConditionGroup {
                id: "np2".into(),
                slots: vec![crate::AdvancedNpSlotCondition {
                    servant: "servant_2".into(),
                    ready: true,
                    member_id: None,
                    servant_id: None,
                    is_support: false,
                }],
            },
        ],
        command_condition_groups: vec![crate::AdvancedCommandConditionGroup {
            id: "cmd".into(),
            cards: vec![AdvancedCommandCardCondition {
                slot: 0,
                servant: "servant_1".into(),
                suit: "buster".into(),
                min_crit_chance: Some(80),
                member_id: None,
                servant_id: None,
                is_support: false,
            }],
        }],
        actions: vec![],
    };
    let cards = vec![command_card(0, Some(11), Some("b"), Some(80))];
    let nps = vec![np_slot(0, true), np_slot(1, true), np_slot(2, false)];
    let party_ids = [Some(11), Some(22), Some(33)];

    assert!(advanced_rule_matches(
        &rule,
        &cards,
        &nps,
        &party_ids,
        &[false, false, false],
    ));
}

#[test]
fn advanced_rule_rejects_when_command_group_misses_even_if_np_matches() {
    let rule = AdvancedRule {
        id: "rule".into(),
        np_condition_groups: vec![crate::AdvancedNpConditionGroup {
            id: "np".into(),
            slots: vec![crate::AdvancedNpSlotCondition {
                servant: "servant_1".into(),
                ready: true,
                member_id: None,
                servant_id: None,
                is_support: false,
            }],
        }],
        command_condition_groups: vec![crate::AdvancedCommandConditionGroup {
            id: "cmd".into(),
            cards: vec![AdvancedCommandCardCondition {
                slot: 0,
                servant: "servant_1".into(),
                suit: "arts".into(),
                min_crit_chance: Some(90),
                member_id: None,
                servant_id: None,
                is_support: false,
            }],
        }],
        actions: vec![],
    };
    let cards = vec![command_card(0, Some(11), Some("a"), Some(80))];
    let nps = vec![np_slot(0, true)];
    let party_ids = [Some(11), None, None];

    assert!(!advanced_rule_matches(
        &rule,
        &cards,
        &nps,
        &party_ids,
        &[false, false, false],
    ));
}

#[test]
fn advanced_startup_conditions_match_only_configured_command_cards() {
    let scene = AdvancedBattleScene {
        id: "advanced_scene_1".into(),
        enemy_target: None,
        main_output: None,
        grand_auto_order_change: None,
        command_conditions: vec![
            AdvancedCommandCardCondition {
                slot: 0,
                servant: "servant_1".into(),
                suit: "buster".into(),
                min_crit_chance: None,
                member_id: None,
                servant_id: None,
                is_support: false,
            },
            AdvancedCommandCardCondition {
                slot: 1,
                servant: "any".into(),
                suit: "any".into(),
                min_crit_chance: None,
                member_id: None,
                servant_id: None,
                is_support: false,
            },
        ],
        control_actions: Vec::new(),
        turns: Vec::new(),
        startup_actions: Vec::new(),
        rules: Vec::new(),
    };
    let cards = vec![
        command_card(0, Some(10), Some("b"), None),
        command_card(1, Some(20), Some("a"), None),
    ];
    let party_ids = [Some(10), Some(20), Some(30)];

    assert!(advanced_startup_conditions_match(
        &scene,
        &cards,
        &party_ids,
        &[false, false, false],
    ));
}

#[test]
fn grand_startup_skips_precheck_without_conditions_or_backline_swap() {
    let scene = empty_advanced_scene();
    let front_main = grand_config_at(1, 10, "buster", "damage");

    assert!(grand_startup_can_run_before_attack(&scene, &[front_main]));
}

#[test]
fn grand_startup_keeps_attack_precheck_for_backline_auto_order_change() {
    let mut scene = empty_advanced_scene();
    scene.grand_auto_order_change = Some(true);
    let back_main = grand_config_at(4, 10, "buster", "damage");

    assert!(!grand_startup_can_run_before_attack(&scene, &[back_main]));
}

#[test]
fn grand_startup_keeps_attack_precheck_for_command_card_conditions() {
    let mut scene = empty_advanced_scene();
    scene.command_conditions = vec![AdvancedCommandCardCondition {
        slot: 0,
        servant: "servant_1".into(),
        member_id: None,
        servant_id: Some(10),
        is_support: false,
        suit: "buster".into(),
        min_crit_chance: None,
    }];
    let front_main = grand_config_at(1, 10, "buster", "damage");

    assert!(!grand_startup_can_run_before_attack(&scene, &[front_main]));
}

#[test]
fn advanced_startup_conditions_match_duplicate_servant_cards_in_any_slots() {
    let scene = AdvancedBattleScene {
        id: "advanced_scene_1".into(),
        enemy_target: None,
        main_output: None,
        grand_auto_order_change: None,
        command_conditions: vec![
            AdvancedCommandCardCondition {
                slot: 0,
                servant: "servant_1".into(),
                suit: "any".into(),
                min_crit_chance: None,
                member_id: None,
                servant_id: None,
                is_support: false,
            },
            AdvancedCommandCardCondition {
                slot: 1,
                servant: "servant_1".into(),
                suit: "any".into(),
                min_crit_chance: None,
                member_id: None,
                servant_id: None,
                is_support: false,
            },
        ],
        control_actions: Vec::new(),
        turns: Vec::new(),
        startup_actions: Vec::new(),
        rules: Vec::new(),
    };
    let cards = vec![
        command_card(0, Some(20), Some("a"), None),
        command_card(1, Some(10), Some("a"), None),
        command_card(2, Some(30), Some("b"), None),
        command_card(3, Some(10), Some("q"), None),
        command_card(4, Some(20), Some("q"), None),
    ];
    let party_ids = [Some(10), Some(20), Some(30)];

    assert!(advanced_startup_conditions_match(
        &scene,
        &cards,
        &party_ids,
        &[false, false, false],
    ));
}

#[test]
fn command_card_owner_detection_retries_until_all_five_cards_have_owners() {
    let cards = vec![
        command_card(0, None, Some("a"), None),
        command_card(1, None, Some("q"), None),
    ];
    assert!(should_retry_command_card_owner_detection(&cards, &[10, 20]));

    let partial_owner = vec![
        command_card(0, None, Some("a"), None),
        command_card(1, Some(10), Some("q"), None),
        command_card(2, Some(20), Some("b"), None),
        command_card(3, Some(10), Some("a"), None),
        command_card(4, Some(20), Some("q"), None),
    ];
    assert!(should_retry_command_card_owner_detection(
        &partial_owner,
        &[10, 20]
    ));

    let complete = vec![
        command_card(0, Some(10), Some("a"), None),
        command_card(1, Some(10), Some("q"), None),
        command_card(2, Some(20), Some("b"), None),
        command_card(3, Some(10), Some("a"), None),
        command_card(4, Some(20), Some("q"), None),
    ];
    assert!(!should_retry_command_card_owner_detection(
        &complete,
        &[10, 20]
    ));
    assert!(!should_retry_command_card_owner_detection(&cards, &[]));
    assert!(should_retry_command_card_owner_detection(&[], &[10]));
}

#[test]
fn command_card_candidates_expand_from_frontline_to_all_six_members() {
    let front = [Some(10), Some(20), Some(30)];
    let full = [Some(10), Some(20), Some(30), Some(40), Some(20), Some(50)];

    assert_eq!(command_card_candidate_ids(&front), vec![10, 20, 30]);
    assert_eq!(command_card_candidate_ids(&full), vec![10, 20, 30, 40, 50]);
}

#[test]
fn battle_state_starts_with_frontline_owner_detection_only() {
    let battle = BattleState::new();

    assert_eq!(battle.flow, BattleFlowState::PreBattle);
    assert_eq!(battle.command_card_owner_failure_count, 0);
    assert!(!battle.command_card_owner_fallback_to_full_party);
}

#[test]
fn battle_flow_runs_full_attack_cycle_through_explicit_states() {
    let mut battle = BattleState::new();
    let started_at = Instant::now();

    let loading = battle.transition(BattleFlowEvent::QuestStartTapped(
        BattleLoadSource::TeamConfirm,
    ));
    assert!(loading.accepted);
    assert_eq!(
        battle.flow,
        BattleFlowState::AwaitingBattleLoad {
            source: BattleLoadSource::TeamConfirm
        }
    );

    let ready = battle.transition(BattleFlowEvent::BattleActionable);
    assert!(ready.accepted);
    assert_eq!(battle.flow, BattleFlowState::BattleReady);

    let waiting = battle.transition(BattleFlowEvent::AttackButtonTapped { at: started_at });
    assert!(waiting.accepted);
    assert_eq!(
        battle.flow,
        BattleFlowState::AwaitingAttackScreen { started_at }
    );
    assert_eq!(battle.attack_screen_wait_started_at(), Some(started_at));

    let attack = battle.transition(BattleFlowEvent::AttackScreenDetected);
    assert!(attack.accepted);
    assert_eq!(battle.flow, BattleFlowState::AttackScreen);

    let submitted = battle.transition(BattleFlowEvent::AttackCardsSubmitted);
    assert!(submitted.accepted);
    assert_eq!(battle.flow, BattleFlowState::AwaitingAttackResolution);
    assert!(battle.awaiting_attack_resolution());

    let hud_wait_started_at = started_at + Duration::from_secs(1);
    let hud_wait = battle.transition(BattleFlowEvent::PostAttackHudWaitStarted {
        at: hud_wait_started_at,
    });
    assert!(hud_wait.accepted);
    assert_eq!(
        battle.flow,
        BattleFlowState::AwaitingPostAttackHud {
            started_at: hud_wait_started_at
        }
    );
    assert!(battle.awaiting_attack_resolution());

    let resolved = battle.transition(BattleFlowEvent::PostAttackHudResolved);
    assert!(resolved.accepted);
    assert_eq!(battle.flow, BattleFlowState::BattleReady);
    assert!(!battle.awaiting_attack_resolution());
}

#[test]
fn battle_flow_rejects_attack_submission_outside_attack_screen() {
    let mut battle = BattleState::new();

    let rejected = battle.transition(BattleFlowEvent::AttackCardsSubmitted);

    assert!(!rejected.accepted);
    assert_eq!(rejected.previous, BattleFlowState::PreBattle);
    assert_eq!(rejected.next, BattleFlowState::PreBattle);
    assert_eq!(battle.flow, BattleFlowState::PreBattle);
}

#[test]
fn battle_flow_attack_wait_timeout_returns_to_battle_ready() {
    let mut battle = BattleState::new();
    let started_at = Instant::now();

    battle.transition(BattleFlowEvent::BattleActionable);
    battle.transition(BattleFlowEvent::AttackButtonTapped { at: started_at });
    let timed_out = battle.transition(BattleFlowEvent::AttackScreenWaitTimedOut);

    assert!(timed_out.accepted);
    assert_eq!(battle.flow, BattleFlowState::BattleReady);
}

#[test]
fn battle_flow_accepts_mid_quest_attack_screen_start() {
    let mut battle = BattleState::new();

    let detected = battle.transition(BattleFlowEvent::AttackScreenDetected);

    assert!(detected.accepted);
    assert_eq!(battle.flow, BattleFlowState::AttackScreen);
}

#[test]
fn runner_lifecycle_accepts_normal_start_finish_path() {
    let started =
        runner_lifecycle_transition(RunnerState::Starting, RunnerLifecycleEvent::WorkerStarted);
    assert!(started.accepted);
    assert_eq!(started.next, RunnerState::Running);

    let finished = runner_lifecycle_transition(started.next, RunnerLifecycleEvent::Finished);
    assert!(finished.accepted);
    assert_eq!(finished.next, RunnerState::Finished);
}

#[test]
fn runner_lifecycle_rejects_finish_before_running() {
    let transition =
        runner_lifecycle_transition(RunnerState::Starting, RunnerLifecycleEvent::Finished);

    assert!(!transition.accepted);
    assert_eq!(transition.next, RunnerState::Starting);
}

#[test]
fn runner_lifecycle_allows_failure_from_any_state() {
    let transition = runner_lifecycle_transition(
        RunnerState::Idle,
        RunnerLifecycleEvent::Failed {
            message: "boom".into(),
        },
    );

    assert!(transition.accepted);
    assert_eq!(
        transition.next,
        RunnerState::Error {
            message: "boom".into()
        }
    );
}

#[test]
fn command_card_visibility_waits_for_all_five_suits_without_requiring_owner() {
    let partial = vec![
        command_card(0, None, Some("a"), None),
        command_card(1, None, Some("q"), None),
    ];
    assert!(!command_cards_visible(&partial));

    let mut missing_suit = vec![
        command_card(0, None, Some("a"), None),
        command_card(1, None, Some("q"), None),
        command_card(2, None, Some("b"), None),
        command_card(3, None, Some("a"), None),
        command_card(4, None, None, None),
    ];
    for card in &mut missing_suit {
        card.icon_region = Some(rect());
    }
    assert!(!command_cards_visible(&missing_suit));

    let mut visible_without_owners = vec![
        command_card(0, None, Some("a"), None),
        command_card(1, None, Some("q"), None),
        command_card(2, None, Some("b"), None),
        command_card(3, None, Some("a"), None),
        command_card(4, None, Some("q"), None),
    ];
    for card in &mut visible_without_owners {
        card.icon_region = Some(rect());
    }
    assert!(command_cards_visible(&visible_without_owners));
    assert!(should_retry_command_card_owner_detection(
        &visible_without_owners,
        &[10, 20]
    ));
}

#[test]
fn np_gauge_read_retries_until_all_three_slots_have_glow_scores() {
    let mut complete = vec![np_slot(0, true), np_slot(1, false), np_slot(2, true)];
    complete[0].np_glow_score = Some(0.8);
    complete[1].np_glow_score = Some(0.3);
    complete[2].np_glow_score = Some(0.7);
    assert!(np_gauge_read_complete(&complete));

    let mut obscured = complete.clone();
    obscured[1].np_glow_score = None;
    assert!(!np_gauge_read_complete(&obscured));

    assert!(!np_gauge_read_complete(&complete[..2]));
    assert!(!np_gauge_read_complete(&[]));
}

#[test]
fn normal_priority_skips_missing_chain_card_then_uses_fallbacks() {
    let priority = vec![
        AttackCard {
            id: "chain_1".into(),
            card: Some("servant_1_buster".into()),
            member_id: None,
            servant_id: None,
            is_support: false,
        },
        AttackCard {
            id: "chain_2".into(),
            card: Some("servant_1_np".into()),
            member_id: None,
            servant_id: None,
            is_support: false,
        },
        AttackCard {
            id: "chain_3".into(),
            card: Some("servant_1_arts".into()),
            member_id: None,
            servant_id: None,
            is_support: false,
        },
        AttackCard {
            id: "fallback_1".into(),
            card: Some("servant_2_quick".into()),
            member_id: None,
            servant_id: None,
            is_support: false,
        },
    ];
    let cards = vec![
        command_card(0, Some(10), Some("a"), None),
        command_card(1, Some(20), Some("q"), None),
    ];
    let nps = vec![np_slot(0, true)];
    let mut used_cards = HashSet::new();
    let mut used_nps = HashSet::new();

    let picks = pick_by_priority(
        &priority,
        &cards,
        &nps,
        &[Some(10), Some(20), Some(30)],
        &[false, false, false],
        &mut used_cards,
        &mut used_nps,
    );

    assert_eq!(pick_labels(&picks), vec!["C1", "NP0", "C0"]);
}

#[test]
fn normal_priority_preserves_chain_order_between_duplicate_card_colors_and_np() {
    let priority = vec![
        AttackCard {
            id: "chain_1".into(),
            card: Some("servant_1_buster".into()),
            member_id: None,
            servant_id: None,
            is_support: false,
        },
        AttackCard {
            id: "chain_2".into(),
            card: Some("servant_1_np".into()),
            member_id: None,
            servant_id: None,
            is_support: false,
        },
        AttackCard {
            id: "chain_3".into(),
            card: Some("servant_1_buster".into()),
            member_id: None,
            servant_id: None,
            is_support: false,
        },
    ];
    let cards = vec![
        command_card(0, Some(10), Some("b"), None),
        command_card(1, Some(10), Some("b"), None),
    ];
    let nps = vec![np_slot(0, true)];
    let mut used_cards = HashSet::new();
    let mut used_nps = HashSet::new();

    let picks = pick_by_priority(
        &priority,
        &cards,
        &nps,
        &[Some(10), Some(20), Some(30)],
        &[false, false, false],
        &mut used_cards,
        &mut used_nps,
    );

    assert_eq!(pick_labels(&picks), vec!["C0", "NP0", "C1"]);
}

#[test]
fn normal_priority_all_matches_any_suit_for_servant() {
    let priority = vec![AttackCard {
        id: "chain_1".into(),
        card: Some("servant_1_all".into()),
        member_id: None,
        servant_id: None,
        is_support: false,
    }];
    let cards = vec![
        command_card(0, Some(20), Some("b"), None),
        command_card(1, Some(10), Some("q"), None),
        command_card(2, Some(10), Some("a"), None),
    ];
    let mut used_cards = HashSet::new();
    let mut used_nps = HashSet::new();

    let picks = pick_by_priority(
        &priority,
        &cards,
        &[],
        &[Some(10), Some(20), Some(30)],
        &[false, false, false],
        &mut used_cards,
        &mut used_nps,
    );

    assert_eq!(pick_labels(&picks), vec!["C1"]);
}

#[test]
fn normal_priority_empty_fixed_slot_inherits_previous_non_np_rule() {
    let priority = vec![
        AttackCard {
            id: "chain_1".into(),
            card: Some("servant_1_np".into()),
            member_id: None,
            servant_id: None,
            is_support: false,
        },
        AttackCard {
            id: "chain_2".into(),
            card: Some("servant_1_all".into()),
            member_id: None,
            servant_id: None,
            is_support: false,
        },
        AttackCard {
            id: "chain_3".into(),
            card: None,
            member_id: None,
            servant_id: None,
            is_support: false,
        },
    ];
    let cards = vec![
        command_card(0, Some(10), Some("b"), None),
        command_card(1, Some(20), Some("b"), None),
        command_card(2, Some(10), Some("a"), None),
        command_card(3, Some(20), Some("a"), None),
        command_card(4, Some(10), Some("q"), None),
    ];
    let nps = vec![np_slot(0, true)];
    let mut used_cards = HashSet::new();
    let mut used_nps = HashSet::new();

    let picks = pick_by_priority(
        &priority,
        &cards,
        &nps,
        &[Some(10), Some(20), Some(30)],
        &[false, false, false],
        &mut used_cards,
        &mut used_nps,
    );

    assert_eq!(pick_labels(&picks), vec!["NP0", "C0", "C2"]);
}

#[test]
fn normal_priority_fills_missed_first_fixed_slot_in_place() {
    let priority = vec![
        AttackCard {
            id: "chain_1".into(),
            card: Some("servant_3_np".into()),
            member_id: None,
            servant_id: None,
            is_support: false,
        },
        AttackCard {
            id: "chain_2".into(),
            card: Some("servant_3_all".into()),
            member_id: None,
            servant_id: None,
            is_support: false,
        },
        AttackCard {
            id: "chain_3".into(),
            card: Some("servant_3_all".into()),
            member_id: None,
            servant_id: None,
            is_support: false,
        },
    ];
    let cards = vec![
        command_card(0, Some(284), Some("a"), None),
        command_card(1, Some(309), Some("b"), None),
        command_card(2, Some(37), Some("a"), None),
        command_card(3, Some(309), Some("q"), None),
        command_card(4, Some(37), Some("a"), None),
    ];
    let nps = vec![np_slot(0, true)];
    let mut used_cards = HashSet::new();
    let mut used_nps = HashSet::new();

    let picks = pick_by_priority(
        &priority,
        &cards,
        &nps,
        &[Some(284), Some(37), Some(309)],
        &[false, false, false],
        &mut used_cards,
        &mut used_nps,
    );

    assert_eq!(pick_labels(&picks), vec!["C0", "C1", "C3"]);
}

#[test]
fn normal_fallback_priority_repeats_before_next_fallback() {
    let priority = vec![
        AttackCard {
            id: "chain_1".into(),
            card: Some("servant_1_buster".into()),
            member_id: None,
            servant_id: None,
            is_support: false,
        },
        AttackCard {
            id: "chain_2".into(),
            card: Some("servant_1_np".into()),
            member_id: None,
            servant_id: None,
            is_support: false,
        },
        AttackCard {
            id: "chain_3".into(),
            card: Some("servant_1_arts".into()),
            member_id: None,
            servant_id: None,
            is_support: false,
        },
        AttackCard {
            id: "fallback_1".into(),
            card: Some("servant_1_all".into()),
            member_id: None,
            servant_id: None,
            is_support: false,
        },
        AttackCard {
            id: "fallback_2".into(),
            card: Some("servant_2_all".into()),
            member_id: None,
            servant_id: None,
            is_support: false,
        },
    ];
    let cards = vec![
        command_card(0, Some(20), Some("b"), None),
        command_card(1, Some(10), Some("a"), None),
        command_card(2, Some(10), Some("q"), None),
        command_card(3, Some(20), Some("b"), None),
        command_card(4, Some(10), Some("q"), None),
    ];
    let nps = vec![np_slot(0, false)];
    let mut used_cards = HashSet::new();
    let mut used_nps = HashSet::new();

    let picks = pick_by_priority(
        &priority,
        &cards,
        &nps,
        &[Some(10), Some(20), Some(30)],
        &[false, false, false],
        &mut used_cards,
        &mut used_nps,
    );

    assert_eq!(pick_labels(&picks), vec!["C2", "C4", "C1"]);
}

#[test]
fn normal_fallback_fills_missing_fixed_chain_slot_before_ready_nps() {
    let priority = vec![
        AttackCard {
            id: "chain_1".into(),
            card: Some("servant_1_arts".into()),
            member_id: None,
            servant_id: None,
            is_support: false,
        },
        AttackCard {
            id: "chain_2".into(),
            card: Some("servant_1_np".into()),
            member_id: None,
            servant_id: None,
            is_support: false,
        },
        AttackCard {
            id: "chain_3".into(),
            card: Some("servant_2_np".into()),
            member_id: None,
            servant_id: None,
            is_support: false,
        },
        AttackCard {
            id: "fallback_1".into(),
            card: Some("servant_2_arts".into()),
            member_id: None,
            servant_id: None,
            is_support: false,
        },
        AttackCard {
            id: "fallback_2".into(),
            card: Some("servant_3_arts".into()),
            member_id: None,
            servant_id: None,
            is_support: false,
        },
    ];
    let cards = vec![command_card(0, Some(30), Some("a"), None)];
    let nps = vec![np_slot(0, true), np_slot(1, true)];
    let mut used_cards = HashSet::new();
    let mut used_nps = HashSet::new();

    let picks = pick_by_priority(
        &priority,
        &cards,
        &nps,
        &[Some(10), Some(20), Some(30)],
        &[false, false, false],
        &mut used_cards,
        &mut used_nps,
    );

    assert_eq!(pick_labels(&picks), vec!["C0", "NP0", "NP1"]);
}

#[test]
fn normal_attack_priority_is_used_even_when_scene_was_not_reexecuted() {
    let scene = normal_scene(
        "scene_1",
        vec![normal_turn(
            vec![],
            vec![AttackCard {
                id: "chain_1".into(),
                card: Some("servant_1_all".into()),
                member_id: None,
                servant_id: None,
                is_support: false,
            }],
        )],
    );

    let scenes = [scene];
    let priority = attack_priority_for_current_scene(false, false, &scenes, 0, 0).unwrap();

    assert_eq!(priority[0].card.as_deref(), Some("servant_1_all"));
}

#[test]
fn normal_attack_priority_reuses_last_turn_after_configured_turns() {
    let scene = normal_scene(
        "scene_1",
        vec![
            normal_turn(
                vec![],
                vec![AttackCard {
                    id: "turn_1_attack".into(),
                    card: Some("servant_1_buster".into()),
                    member_id: None,
                    servant_id: None,
                    is_support: false,
                }],
            ),
            normal_turn(
                vec![],
                vec![AttackCard {
                    id: "turn_2_attack".into(),
                    card: Some("servant_2_arts".into()),
                    member_id: None,
                    servant_id: None,
                    is_support: false,
                }],
            ),
        ],
    );

    let scenes = [scene];
    let priority = attack_priority_for_current_scene(false, false, &scenes, 0, 2).unwrap();

    assert_eq!(priority[0].card.as_deref(), Some("servant_2_arts"));
}

#[test]
fn normal_scenes_skip_command_card_recognition_when_no_regular_cards_are_configured() {
    let scenes = [
        normal_scene(
            "scene_1",
            vec![normal_turn(
                vec![],
                vec![AttackCard {
                    id: "np_1".into(),
                    card: Some("servant_1_np".into()),
                    member_id: None,
                    servant_id: None,
                    is_support: false,
                }],
            )],
        ),
        normal_scene("scene_2", vec![normal_turn(vec![], vec![])]),
    ];

    assert!(!normal_scenes_need_command_card_recognition(&scenes));
}

#[test]
fn normal_scenes_need_command_card_recognition_when_regular_card_is_configured() {
    let scenes = [normal_scene(
        "scene_1",
        vec![normal_turn(
            vec![],
            vec![AttackCard {
                id: "card_1".into(),
                card: Some("servant_1_all".into()),
                member_id: None,
                servant_id: None,
                is_support: false,
            }],
        )],
    )];

    assert!(normal_scenes_need_command_card_recognition(&scenes));
}

#[test]
fn normal_turn_for_current_state_marks_over_configured_turns() {
    let mut first = normal_turn(vec![], vec![]);
    first.id = "turn_1".into();
    let mut second = normal_turn(vec![], vec![]);
    second.id = "turn_2".into();
    let scene = normal_scene("scene_1", vec![first, second]);

    let (turn, over_configured_turns) = normal_turn_for_current_state(&[scene], 0, 2).unwrap();

    assert_eq!(turn.id, "turn_2");
    assert!(over_configured_turns);
}

#[test]
fn advanced_enemy_target_is_selected_only_when_scene_first_executes() {
    let mut first = empty_advanced_scene();
    first.enemy_target = Some("enemy_5".into());
    let scenes = [first, empty_advanced_scene()];

    assert_eq!(
        advanced_enemy_target_for_current_scene(&scenes, 0, true),
        Some("enemy_5")
    );
    assert_eq!(
        advanced_enemy_target_for_current_scene(&scenes, 0, false),
        None
    );
    assert_eq!(
        advanced_enemy_target_for_current_scene(&scenes, 1, true),
        None
    );
    assert_eq!(
        advanced_enemy_target_for_current_scene(&scenes, 2, true),
        None
    );
}

#[test]
fn advanced_attack_priority_still_requires_scene_config_used() {
    let scene = normal_scene(
        "scene_1",
        vec![normal_turn(
            vec![],
            vec![AttackCard {
                id: "chain_1".into(),
                card: Some("servant_1_all".into()),
                member_id: None,
                servant_id: None,
                is_support: false,
            }],
        )],
    );

    assert!(attack_priority_for_current_scene(true, false, &[scene], 0, 0).is_none());
}

#[test]
fn normal_current_party_ids_apply_executed_order_change_before_attack() {
    let scene = normal_scene(
        "scene_1",
        vec![normal_turn(
            vec![Action::Equipment {
                id: "eq_1".into(),
                skill: Some("skill_3".into()),
                target: None,
                target_member_id: None,
                target_servant_id: None,
                target_is_support: false,
                order_change: Some(crate::OrderChangeSelection {
                    front: Some("servant_1".into()),
                    front_member_id: None,
                    front_servant_id: None,
                    front_is_support: false,
                    back: Some("servant_4".into()),
                    back_member_id: None,
                    back_servant_id: None,
                    back_is_support: false,
                }),
            }],
            vec![],
        )],
    );

    let party_ids = normal_current_party_ids_from(
        [Some(10), Some(20), Some(30), Some(40), None, None],
        &[scene],
        0,
        0,
        Some((0, 0)),
    );

    assert_eq!(party_ids, [Some(40), Some(20), Some(30)]);
}

#[test]
fn normal_current_party_ids_apply_previous_scene_np_retreat_before_attack() {
    let scene_1 = normal_scene(
        "scene_1",
        vec![normal_turn(
            vec![],
            vec![AttackCard {
                id: "atk_1".into(),
                card: Some("servant_2_np".into()),
                member_id: None,
                servant_id: None,
                is_support: false,
            }],
        )],
    );
    let scene_2 = normal_scene("scene_2", vec![normal_turn(vec![], vec![])]);

    let party_ids = normal_current_party_ids_from(
        [Some(284), Some(16), Some(315), Some(211), None, None],
        &[scene_1, scene_2],
        1,
        0,
        Some((1, 0)),
    );

    assert_eq!(party_ids, [Some(284), Some(211), Some(315)]);
}

#[test]
fn normal_current_party_ids_do_not_apply_current_scene_np_before_attack() {
    let scene = normal_scene(
        "scene_1",
        vec![normal_turn(
            vec![],
            vec![AttackCard {
                id: "atk_1".into(),
                card: Some("servant_2_np".into()),
                member_id: None,
                servant_id: None,
                is_support: false,
            }],
        )],
    );

    let party_ids = normal_current_party_ids_from(
        [Some(284), Some(16), Some(315), Some(211), None, None],
        &[scene],
        0,
        0,
        Some((0, 0)),
    );

    assert_eq!(party_ids, [Some(284), Some(16), Some(315)]);
}

#[test]
fn normal_current_party_ids_do_not_repeat_last_turn_prep_after_configured_turns() {
    let scene = normal_scene(
        "scene_1",
        vec![
            normal_turn(vec![], vec![]),
            normal_turn(
                vec![Action::Equipment {
                    id: "eq_1".into(),
                    skill: Some("skill_3".into()),
                    target: None,
                    target_member_id: None,
                    target_servant_id: None,
                    target_is_support: false,
                    order_change: Some(crate::OrderChangeSelection {
                        front: Some("servant_1".into()),
                        front_member_id: None,
                        front_servant_id: None,
                        front_is_support: false,
                        back: Some("servant_4".into()),
                        back_member_id: None,
                        back_servant_id: None,
                        back_is_support: false,
                    }),
                }],
                vec![],
            ),
        ],
    );

    let party_ids = normal_current_party_ids_from(
        [Some(10), Some(20), Some(30), Some(40), None, None],
        &[scene],
        0,
        2,
        None,
    );

    assert_eq!(party_ids, [Some(40), Some(20), Some(30)]);
}

#[test]
fn normal_current_party_ids_apply_previous_scene_end_of_turn_skill_exit() {
    let scene_1 = normal_scene(
        "scene_1",
        vec![normal_turn(
            vec![Action::Servant {
                id: "sa_1".into(),
                servant: Some("servant_1".into()),
                servant_member_id: None,
                servant_id: None,
                servant_is_support: false,
                skill: Some("skill_3".into()),
                skill_selection: None,
                target: None,
                target_member_id: None,
                target_servant_id: None,
                target_is_support: false,
            }],
            vec![],
        )],
    );
    let scene_2 = normal_scene("scene_2", vec![normal_turn(vec![], vec![])]);

    let party_ids = normal_current_party_ids_from(
        [Some(315), Some(434), Some(384), Some(11), Some(22), None],
        &[scene_1, scene_2],
        1,
        0,
        Some((1, 0)),
    );

    assert_eq!(party_ids, [Some(11), Some(434), Some(384)]);
}

#[test]
fn grand_auto_order_change_targets_front_servant_with_most_cards() {
    let cards = vec![
        command_card(0, Some(20), Some("a"), None),
        command_card(1, Some(10), Some("q"), None),
        command_card(2, Some(20), Some("b"), None),
        command_card(3, Some(30), Some("a"), None),
        command_card(4, Some(10), Some("b"), None),
    ];
    let action = grand_auto_order_change_action(
        &cards,
        &[Some(10), Some(20), Some(30)],
        &[false, false, false],
        &[grand_config_at(4, 99, "buster", "damage")],
        GrandClass::Saber,
    )
    .unwrap();

    match action {
        Action::Equipment {
            skill,
            order_change: Some(order_change),
            ..
        } => {
            assert_eq!(skill.as_deref(), Some("skill_3"));
            assert_eq!(order_change.front.as_deref(), Some("servant_1"));
            assert_eq!(order_change.back.as_deref(), Some("servant_5"));
        }
        _ => panic!("expected auto Order Change action"),
    }
}

#[test]
fn grand_auto_order_change_skips_when_main_grand_is_frontline() {
    let cards = vec![command_card(0, Some(99), Some("a"), None)];
    assert!(grand_auto_order_change_action(
        &cards,
        &[Some(99), Some(20), Some(30)],
        &[false, false, false],
        &[grand_config_at(0, 99, "buster", "damage")],
        GrandClass::Saber,
    )
    .is_none());
}

#[test]
fn lancer_auto_order_change_brings_aoe_forward_without_removing_front_single() {
    let cards = vec![
        command_card(0, Some(10), Some("a"), None),
        command_card(1, Some(10), Some("q"), None),
        command_card(2, Some(10), Some("b"), None),
        command_card(3, Some(30), Some("a"), None),
        command_card(4, Some(40), Some("a"), None),
    ];
    let action = grand_auto_order_change_action(
        &cards,
        &[Some(10), Some(30), Some(40)],
        &[false, false, false],
        &[
            GrandServantRuntimeConfig {
                role: "single".into(),
                ..grand_config_at(0, 10, "buster", "damage")
            },
            GrandServantRuntimeConfig {
                role: "aoe".into(),
                ..grand_config_at(4, 20, "arts", "damage")
            },
        ],
        GrandClass::Lancer,
    )
    .unwrap();

    match action {
        Action::Equipment {
            order_change: Some(order_change),
            ..
        } => {
            assert_eq!(order_change.front.as_deref(), Some("servant_2"));
            assert_eq!(order_change.back.as_deref(), Some("servant_5"));
        }
        _ => panic!("expected Lancer auto Order Change action"),
    }
}

#[test]
fn pick_by_priority_distinguishes_owned_and_support_cards_with_same_servant_id() {
    let cards = vec![
        command_card_with_support(0, Some(309), false, Some("a"), None),
        command_card_with_support(1, Some(309), true, Some("a"), None),
    ];
    let priority = vec![AttackCard {
        id: "atk_1".into(),
        card: Some("servant_1_arts".into()),
        member_id: None,
        servant_id: Some(309),
        is_support: true,
    }];
    let mut used_cards = HashSet::new();
    let mut used_nps = HashSet::new();

    let picks = pick_by_priority(
        &priority,
        &cards,
        &[],
        &[Some(309), None, None],
        &[true, false, false],
        &mut used_cards,
        &mut used_nps,
    );

    assert_eq!(picks.len(), 1);
    match &picks[0] {
        Pick::Card {
            slot, servant_id, ..
        } => {
            assert_eq!(*slot, 1);
            assert_eq!(*servant_id, Some(309));
        }
        _ => panic!("expected command card pick"),
    }
}

#[test]
fn command_card_picks_postpone_stunned_cards_until_needed() {
    let cards = vec![
        command_card_with_state(0, Some(309), false, true, Some("a"), None),
        command_card_with_state(1, Some(309), false, false, Some("a"), None),
        command_card_with_state(2, Some(309), false, false, Some("b"), None),
    ];
    let priority = vec![AttackCard {
        id: "atk_1".into(),
        card: Some("servant_1_arts".into()),
        member_id: None,
        servant_id: Some(309),
        is_support: false,
    }];
    let mut used_cards = HashSet::new();
    let mut used_nps = HashSet::new();

    let picks = pick_by_priority(
        &priority,
        &cards,
        &[],
        &[Some(309), None, None],
        &[false, false, false],
        &mut used_cards,
        &mut used_nps,
    );

    assert!(matches!(picks.first(), Some(Pick::Card { slot: 1, .. })));

    let mut used_cards = HashSet::new();
    let mut used_nps = HashSet::new();
    let only_stunned = vec![command_card_with_state(
        0,
        Some(309),
        false,
        true,
        Some("a"),
        None,
    )];
    let picks = pick_by_priority(
        &priority,
        &only_stunned,
        &[],
        &[Some(309), None, None],
        &[false, false, false],
        &mut used_cards,
        &mut used_nps,
    );
    assert!(matches!(picks.first(), Some(Pick::Card { slot: 0, .. })));
}

#[test]
fn command_card_fillers_postpone_stunned_cards_until_needed() {
    let cards = vec![
        command_card_with_state(0, Some(1), false, true, Some("a"), None),
        command_card_with_state(1, Some(1), false, false, Some("a"), None),
        command_card_with_state(2, Some(1), false, false, Some("a"), None),
    ];
    let mut used_cards = HashSet::new();
    let mut fixed = [None, None, None];

    fill_empty_pick_slots(&mut fixed, &cards, &mut used_cards);
    assert!(matches!(fixed[0], Some(Pick::Card { slot: 1, .. })));
    assert!(matches!(fixed[1], Some(Pick::Card { slot: 2, .. })));
    assert!(matches!(fixed[2], Some(Pick::Card { slot: 0, .. })));

    let mut remaining = Vec::new();
    let mut used_cards = HashSet::new();
    fill_remaining(&mut remaining, &cards, &mut used_cards);
    assert!(matches!(remaining[0], Pick::Card { slot: 1, .. }));
    assert!(matches!(remaining[1], Pick::Card { slot: 2, .. }));
    assert!(matches!(remaining[2], Pick::Card { slot: 0, .. }));
}

#[test]
fn normal_mode_uses_other_actionable_cards_before_a_stunned_priority_target() {
    let all_cards = vec![
        // C1 Oberon Arts; C2/C3 Morgan Arts/Quick (stunned); C4 Oberon
        // Buster; C5 Nocnaree Quick. With a "Morgan, any card" rule, the
        // normal cards must fill left-to-right as C1, C4, C5.
        command_card_with_state(0, Some(304), false, false, Some("a"), None),
        command_card_with_state(1, Some(309), false, true, Some("a"), None),
        command_card_with_state(2, Some(309), false, true, Some("q"), None),
        command_card_with_state(3, Some(304), false, false, Some("b"), None),
        command_card_with_state(4, Some(383), false, false, Some("q"), None),
    ];
    let actionable = actionable_command_cards(&all_cards);
    let priority = vec![AttackCard {
        id: "atk_1".into(),
        card: Some("servant_1_all".into()),
        member_id: None,
        servant_id: Some(309),
        is_support: false,
    }];
    let mut used_cards = HashSet::new();
    let mut used_nps = HashSet::new();
    let mut picks = pick_by_priority(
        &priority,
        &actionable,
        &[],
        &[Some(309), Some(304), Some(383)],
        &[false, false, false],
        &mut used_cards,
        &mut used_nps,
    );
    fill_remaining(&mut picks, &actionable, &mut used_cards);

    assert_eq!(pick_labels(&picks), vec!["C0", "C3", "C4"]);
}

#[test]
fn advanced_auto_picks_exclude_stunned_command_cards() {
    let scene = empty_advanced_scene();
    let cards = vec![
        command_card_with_state(0, Some(10), false, true, Some("b"), None),
        command_card_with_state(1, Some(10), false, false, Some("a"), None),
        command_card_with_state(2, Some(20), false, false, Some("q"), None),
        command_card_with_state(3, Some(20), false, false, Some("b"), None),
    ];
    let picks = choose_advanced_auto_picks(
        &scene,
        &cards,
        &[],
        &[Some(10), Some(20), None],
        &[false, false, false],
        &[],
        &GrandCardStrategy::default(),
    );

    assert_eq!(pick_labels(&picks), vec!["C2", "C3", "C1"]);
}

#[test]
fn advanced_startup_conditions_match_respects_support_flag() {
    let scene = AdvancedBattleScene {
        id: "scene".into(),
        enemy_target: None,
        main_output: None,
        grand_auto_order_change: None,
        command_conditions: vec![AdvancedCommandCardCondition {
            slot: 0,
            servant: "servant_1".into(),
            member_id: None,
            servant_id: Some(309),
            is_support: true,
            suit: "arts".into(),
            min_crit_chance: None,
        }],
        control_actions: vec![],
        turns: Vec::new(),
        startup_actions: vec![],
        rules: vec![],
    };
    let support_cards = vec![command_card_with_support(
        0,
        Some(309),
        true,
        Some("a"),
        None,
    )];

    assert!(advanced_startup_conditions_match(
        &scene,
        &support_cards,
        &[Some(309), None, None],
        &[true, false, false],
    ));
    assert!(!advanced_startup_conditions_match(
        &scene,
        &support_cards,
        &[Some(309), None, None],
        &[false, false, false],
    ));
}

#[test]
fn grand_auto_order_change_uses_support_ownership_when_counting_cards() {
    let cards = vec![
        command_card_with_support(0, Some(20), false, Some("a"), None),
        command_card_with_support(1, Some(10), true, Some("b"), None),
        command_card_with_support(2, Some(10), true, Some("q"), None),
    ];

    let action = grand_auto_order_change_action(
        &cards,
        &[Some(20), Some(10), Some(30)],
        &[false, true, false],
        &[grand_config_at(4, 99, "buster", "damage")],
        GrandClass::Saber,
    )
    .unwrap();

    match action {
        Action::Equipment {
            order_change: Some(order_change),
            ..
        } => {
            assert_eq!(order_change.front.as_deref(), Some("servant_2"));
            assert!(order_change.front_is_support);
        }
        _ => panic!("expected auto Order Change action"),
    }
}

#[test]
fn startup_action_selected_slot_resolves_to_current_position_after_auto_order_change() {
    let original_ids = [Some(10), Some(20), Some(30), Some(99), None, None];
    let changed_ids = [Some(99), Some(20), Some(30), Some(10), None, None];
    let swapped_out_source = Action::Servant {
        id: "a1".into(),
        servant: Some("servant_1".into()),
        servant_member_id: None,
        servant_id: None,
        servant_is_support: false,
        skill: Some("skill_1".into()),
        skill_selection: None,
        target: None,
        target_member_id: None,
        target_servant_id: None,
        target_is_support: false,
    };
    let main_grand_source = Action::Servant {
        id: "a4".into(),
        servant: Some("servant_4".into()),
        servant_member_id: None,
        servant_id: None,
        servant_is_support: false,
        skill: Some("skill_1".into()),
        skill_selection: None,
        target: Some("servant_2".into()),
        target_member_id: None,
        target_servant_id: None,
        target_is_support: false,
    };
    let unchanged_source = Action::Servant {
        id: "a2".into(),
        servant: Some("servant_2".into()),
        servant_member_id: None,
        servant_id: None,
        servant_is_support: false,
        skill: Some("skill_1".into()),
        skill_selection: None,
        target: None,
        target_member_id: None,
        target_servant_id: None,
        target_is_support: false,
    };
    let swapped_target = Action::Equipment {
        id: "a3".into(),
        skill: Some("skill_1".into()),
        target: Some("servant_1".into()),
        target_member_id: None,
        target_servant_id: None,
        target_is_support: false,
        order_change: None,
    };

    assert!(
        resolve_action_to_current_positions(&changed_ids, &original_ids, &swapped_out_source)
            .is_none()
    );
    let resolved =
        resolve_action_to_current_positions(&changed_ids, &original_ids, &main_grand_source)
            .unwrap();
    match resolved {
        Action::Servant {
            servant, target, ..
        } => {
            assert_eq!(servant.as_deref(), Some("servant_1"));
            assert_eq!(target.as_deref(), Some("servant_2"));
        }
        _ => panic!("expected servant action"),
    }
    assert!(
        resolve_action_to_current_positions(&changed_ids, &original_ids, &unchanged_source)
            .is_some()
    );
    assert!(
        resolve_action_to_current_positions(&changed_ids, &original_ids, &swapped_target).is_none()
    );
}

#[test]
fn startup_action_member_id_resolves_after_team_reorder() {
    let original_members = [
        Some(PartyMemberRuntime {
            member_id: Some("slot-a".into()),
            slot_index: 0,
            servant_id: 10,
            is_support: false,
        }),
        Some(PartyMemberRuntime {
            member_id: Some("slot-b".into()),
            slot_index: 1,
            servant_id: 20,
            is_support: false,
        }),
        None,
        None,
        None,
        None,
    ];
    let changed_members = [
        Some(PartyMemberRuntime {
            member_id: Some("slot-b".into()),
            slot_index: 1,
            servant_id: 20,
            is_support: false,
        }),
        Some(PartyMemberRuntime {
            member_id: Some("slot-a".into()),
            slot_index: 0,
            servant_id: 10,
            is_support: false,
        }),
        None,
        None,
        None,
        None,
    ];
    let action = Action::Servant {
        id: "startup_member".into(),
        servant: Some("servant_2".into()),
        servant_member_id: Some("slot-b".into()),
        servant_id: Some(20),
        servant_is_support: false,
        skill: Some("skill_1".into()),
        skill_selection: Some(crate::SkillSelection {
            selection_type: "SelectAddInfo".into(),
            index: 1,
            option_count: Some(2),
            label: Some("测试选项".into()),
        }),
        target: Some("servant_1".into()),
        target_member_id: Some("slot-a".into()),
        target_servant_id: Some(10),
        target_is_support: false,
    };

    let resolved =
        resolve_action_to_current_member_positions(&changed_members, &original_members, &action)
            .unwrap();

    match resolved {
        Action::Servant {
            servant,
            target,
            skill_selection,
            ..
        } => {
            assert_eq!(servant.as_deref(), Some("servant_1"));
            assert_eq!(target.as_deref(), Some("servant_2"));
            assert_eq!(
                skill_selection
                    .as_ref()
                    .and_then(|selection| selection.label.as_deref()),
                Some("测试选项")
            );
        }
        _ => panic!("expected servant action"),
    }
}

fn runtime_member(
    member_id: &str,
    slot_index: usize,
    servant_id: u32,
    is_support: bool,
) -> PartyMemberRuntime {
    PartyMemberRuntime {
        member_id: Some(member_id.into()),
        slot_index,
        servant_id,
        is_support,
    }
}

#[test]
fn normal_battle_order_change_keeps_waver_actions_on_waver_member() {
    const TYPHON: u32 = 441;
    const MERLIN: u32 = 150;
    const PHANTASMOON: u32 = 431;
    const WAVER: u32 = 37;

    let original_members = [
        Some(runtime_member("slot-typhon", 0, TYPHON, true)),
        Some(runtime_member("slot-merlin", 1, MERLIN, false)),
        Some(runtime_member("slot-phantasmoon", 2, PHANTASMOON, false)),
        Some(runtime_member("slot-waver", 3, WAVER, false)),
        None,
        None,
    ];
    let mut current_members = original_members.clone();
    let order_change = Action::Equipment {
        id: "order_change_waver_merlin".into(),
        skill: Some("skill_3".into()),
        target: None,
        target_member_id: None,
        target_servant_id: None,
        target_is_support: false,
        order_change: Some(crate::OrderChangeSelection {
            front: Some("servant_2".into()),
            front_member_id: Some("slot-merlin".into()),
            front_servant_id: Some(MERLIN),
            front_is_support: false,
            back: Some("servant_4".into()),
            back_member_id: Some("slot-waver".into()),
            back_servant_id: Some(WAVER),
            back_is_support: false,
        }),
    };
    let waver_skill_1 = Action::Servant {
        id: "waver_skill_1".into(),
        servant: Some("servant_2".into()),
        servant_member_id: Some("slot-waver".into()),
        servant_id: Some(WAVER),
        servant_is_support: false,
        skill: Some("skill_1".into()),
        skill_selection: None,
        target: Some("servant_1".into()),
        target_member_id: Some("slot-typhon".into()),
        target_servant_id: Some(TYPHON),
        target_is_support: true,
    };
    let waver_skill_2 = Action::Servant {
        id: "waver_skill_2".into(),
        servant: Some("servant_2".into()),
        servant_member_id: Some("slot-waver".into()),
        servant_id: Some(WAVER),
        servant_is_support: false,
        skill: Some("skill_2".into()),
        skill_selection: None,
        target: None,
        target_member_id: None,
        target_servant_id: None,
        target_is_support: false,
    };
    let waver_skill_3 = Action::Servant {
        id: "waver_skill_3".into(),
        servant: Some("servant_2".into()),
        servant_member_id: Some("slot-waver".into()),
        servant_id: Some(WAVER),
        servant_is_support: false,
        skill: Some("skill_3".into()),
        skill_selection: None,
        target: None,
        target_member_id: None,
        target_servant_id: None,
        target_is_support: false,
    };

    let resolved_order_change =
        resolve_available_member_action(&current_members, &original_members, &order_change)
            .expect("Order Change should be executable");
    apply_party_member_lineup_change(&mut current_members, &resolved_order_change);

    assert_eq!(
        party_member_ids(&current_members),
        [
            Some(TYPHON),
            Some(WAVER),
            Some(PHANTASMOON),
            Some(MERLIN),
            None,
            None
        ]
    );

    let resolved_skill_1 =
        resolve_available_member_action(&current_members, &original_members, &waver_skill_1)
            .expect("Waver skill 1 should resolve after Order Change");
    let resolved_skill_2 =
        resolve_available_member_action(&current_members, &original_members, &waver_skill_2)
            .expect("Waver skill 2 should resolve after Order Change");
    let resolved_skill_3 =
        resolve_available_member_action(&current_members, &original_members, &waver_skill_3)
            .expect("Waver skill 3 should resolve after Order Change");

    match resolved_skill_1 {
        Action::Servant {
            servant, target, ..
        } => {
            assert_eq!(servant.as_deref(), Some("servant_2"));
            assert_eq!(target.as_deref(), Some("servant_1"));
        }
        _ => panic!("expected servant action"),
    }
    for resolved in [resolved_skill_2, resolved_skill_3] {
        match resolved {
            Action::Servant {
                servant, target, ..
            } => {
                assert_eq!(servant.as_deref(), Some("servant_2"));
                assert_eq!(target, None);
            }
            _ => panic!("expected servant action"),
        }
    }
}

#[test]
fn member_action_does_not_fall_back_to_same_position_when_member_is_backline() {
    const TYPHON: u32 = 441;
    const MERLIN: u32 = 150;
    const PHANTASMOON: u32 = 431;
    const WAVER: u32 = 37;

    let members = [
        Some(runtime_member("slot-typhon", 0, TYPHON, true)),
        Some(runtime_member("slot-merlin", 1, MERLIN, false)),
        Some(runtime_member("slot-phantasmoon", 2, PHANTASMOON, false)),
        Some(runtime_member("slot-waver", 3, WAVER, false)),
        None,
        None,
    ];
    let action = Action::Servant {
        id: "waver_backline_skill".into(),
        servant: Some("servant_2".into()),
        servant_member_id: Some("slot-waver".into()),
        servant_id: Some(WAVER),
        servant_is_support: false,
        skill: Some("skill_1".into()),
        skill_selection: None,
        target: None,
        target_member_id: None,
        target_servant_id: None,
        target_is_support: false,
    };

    assert!(
        resolve_available_member_action(&members, &members, &action).is_none(),
        "Waver is in the back line, so this must not execute as Merlin in servant_2"
    );
}

#[test]
fn auto_order_change_startup_flow_replays_control_after_swap() {
    let auto_order_change = Action::Equipment {
        id: "auto_oc".into(),
        skill: Some("skill_3".into()),
        target: None,
        target_member_id: None,
        target_servant_id: None,
        target_is_support: false,
        order_change: Some(crate::OrderChangeSelection {
            front: Some("servant_1".into()),
            front_member_id: None,
            front_servant_id: None,
            front_is_support: false,
            back: Some("servant_4".into()),
            back_member_id: None,
            back_servant_id: None,
            back_is_support: false,
        }),
    };
    let first_control = Action::Equipment {
        id: "control_1".into(),
        skill: Some("skill_1".into()),
        target: Some("servant_1".into()),
        target_member_id: None,
        target_servant_id: None,
        target_is_support: false,
        order_change: None,
    };
    let second_control = Action::Equipment {
        id: "control_2".into(),
        skill: Some("skill_2".into()),
        target: Some("servant_2".into()),
        target_member_id: None,
        target_servant_id: None,
        target_is_support: false,
        order_change: None,
    };
    let startup = Action::Servant {
        id: "startup_1".into(),
        servant: Some("servant_1".into()),
        servant_member_id: None,
        servant_id: None,
        servant_is_support: false,
        skill: Some("skill_1".into()),
        skill_selection: None,
        target: None,
        target_member_id: None,
        target_servant_id: None,
        target_is_support: false,
    };
    let scene = AdvancedBattleScene {
        id: "advanced_scene_1".into(),
        enemy_target: None,
        main_output: None,
        grand_auto_order_change: Some(true),
        command_conditions: Vec::new(),
        control_actions: vec![first_control, second_control],
        turns: vec![crate::AdvancedBattleTurn {
            id: "turn_1".into(),
            actions: vec![startup],
        }],
        startup_actions: Vec::new(),
        rules: Vec::new(),
    };

    let actions = advanced_startup_flow_actions(&scene, 2, 1, 1, Some(&auto_order_change));
    let ids: Vec<&str> = actions
        .iter()
        .map(|action| match action {
            Action::Servant { id, .. }
            | Action::Equipment { id, .. }
            | Action::CommandSpell { id, .. }
            | Action::EnemyTarget { id, .. } => id.as_str(),
        })
        .collect();

    assert_eq!(ids, vec!["auto_oc", "control_1", "startup_1", "control_2"]);
}

#[test]
fn advanced_startup_flow_interleaves_multi_turn_actions_and_later_controls() {
    let turn_action = |id: &str| Action::EnemyTarget {
        id: id.into(),
        target: None,
    };
    let control_action = |id: &str, order_change| Action::Equipment {
        id: id.into(),
        skill: Some("skill_3".into()),
        target: None,
        target_member_id: None,
        target_servant_id: None,
        target_is_support: false,
        order_change,
    };
    let scene = AdvancedBattleScene {
        id: "advanced_scene_1".into(),
        enemy_target: None,
        main_output: None,
        grand_auto_order_change: None,
        command_conditions: Vec::new(),
        control_actions: vec![
            control_action("control_1", None),
            control_action(
                "order_change_control_2",
                Some(crate::OrderChangeSelection {
                    front: Some("servant_1".into()),
                    front_member_id: None,
                    front_servant_id: None,
                    front_is_support: false,
                    back: Some("servant_4".into()),
                    back_member_id: None,
                    back_servant_id: None,
                    back_is_support: false,
                }),
            ),
            control_action("control_3", None),
        ],
        turns: vec![
            crate::AdvancedBattleTurn {
                id: "turn_1".into(),
                actions: vec![turn_action("turn_1")],
            },
            crate::AdvancedBattleTurn {
                id: "turn_2".into(),
                actions: vec![turn_action("turn_2")],
            },
            crate::AdvancedBattleTurn {
                id: "turn_3".into(),
                actions: vec![turn_action("turn_3")],
            },
        ],
        startup_actions: Vec::new(),
        rules: Vec::new(),
    };

    let actions = advanced_startup_flow_actions(&scene, 3, 1, 3, None);
    let ids: Vec<&str> = actions
        .iter()
        .map(|action| match action {
            Action::Servant { id, .. }
            | Action::Equipment { id, .. }
            | Action::CommandSpell { id, .. }
            | Action::EnemyTarget { id, .. } => id.as_str(),
        })
        .collect();

    assert_eq!(
        ids,
        vec![
            "control_1",
            "turn_1",
            "order_change_control_2",
            "turn_2",
            "control_3",
            "turn_3",
        ]
    );
}

#[test]
fn advanced_turn_actions_supports_legacy_and_multi_turn_scenes() {
    let legacy_action = Action::EnemyTarget {
        id: "legacy".into(),
        target: Some("enemy_1".into()),
    };
    let mut scene = empty_advanced_scene();
    scene.startup_actions = vec![legacy_action];

    assert_eq!(advanced_turn_actions(&scene, 0).unwrap().len(), 1);
    assert!(advanced_turn_actions(&scene, 1).is_none());

    scene.turns = vec![
        crate::AdvancedBattleTurn {
            id: "turn_1".into(),
            actions: Vec::new(),
        },
        crate::AdvancedBattleTurn {
            id: "turn_2".into(),
            actions: vec![Action::EnemyTarget {
                id: "second".into(),
                target: Some("enemy_2".into()),
            }],
        },
    ];

    assert!(advanced_turn_actions(&scene, 0).unwrap().is_empty());
    assert_eq!(advanced_turn_actions(&scene, 1).unwrap().len(), 1);
    assert!(advanced_turn_actions(&scene, 2).is_none());
}

#[test]
fn party_lineup_change_swaps_support_from_configured_back_slot() {
    let mut ids = [Some(8), Some(434), Some(384), Some(11), Some(22), Some(999)];
    let action = Action::Equipment {
        id: "a1".into(),
        skill: Some("skill_3".into()),
        target: None,
        target_member_id: None,
        target_servant_id: None,
        target_is_support: false,
        order_change: Some(crate::OrderChangeSelection {
            front: Some("servant_2".into()),
            front_member_id: None,
            front_servant_id: None,
            front_is_support: false,
            back: Some("servant_6".into()),
            back_member_id: None,
            back_servant_id: None,
            back_is_support: false,
        }),
    };

    apply_party_lineup_change(&mut ids, &action);

    assert_eq!(
        ids,
        [Some(8), Some(999), Some(384), Some(11), Some(22), Some(434)]
    );
}

#[test]
fn party_lineup_change_does_not_apply_end_of_turn_skill_by_default() {
    let mut ids = [Some(388), Some(434), Some(384), Some(11), Some(22), None];
    let action = Action::Servant {
        id: "a1".into(),
        servant: Some("servant_1".into()),
        servant_member_id: None,
        servant_id: None,
        servant_is_support: false,
        skill: Some("skill_2".into()),
        skill_selection: None,
        target: None,
        target_member_id: None,
        target_servant_id: None,
        target_is_support: false,
    };

    apply_party_lineup_change(&mut ids, &action);

    assert_eq!(
        ids,
        [Some(388), Some(434), Some(384), Some(11), Some(22), None]
    );
}

#[test]
fn party_lineup_change_applies_end_of_turn_servant_skill_withdraw_rule() {
    let mut ids = [Some(388), Some(434), Some(384), Some(11), Some(22), None];
    let action = Action::Servant {
        id: "a1".into(),
        servant: Some("servant_1".into()),
        servant_member_id: None,
        servant_id: None,
        servant_is_support: false,
        skill: Some("skill_2".into()),
        skill_selection: None,
        target: None,
        target_member_id: None,
        target_servant_id: None,
        target_is_support: false,
    };

    apply_party_lineup_change_at(&mut ids, &action, ChangeOrderTiming::EndOfTurn);

    assert_eq!(
        ids,
        [Some(11), Some(434), Some(384), Some(388), Some(22), None]
    );
}

#[test]
fn party_lineup_change_removes_habetrot_at_end_of_turn_after_third_skill() {
    let mut ids = [Some(315), Some(434), Some(384), Some(11), Some(22), None];
    let action = Action::Servant {
        id: "a1".into(),
        servant: Some("servant_1".into()),
        servant_member_id: None,
        servant_id: None,
        servant_is_support: false,
        skill: Some("skill_3".into()),
        skill_selection: None,
        target: None,
        target_member_id: None,
        target_servant_id: None,
        target_is_support: false,
    };

    apply_party_lineup_change_at(&mut ids, &action, ChangeOrderTiming::EndOfTurn);

    assert_eq!(ids, [Some(11), Some(434), Some(384), Some(22), None, None]);
}

#[test]
fn party_lineup_change_removes_ultimate_elisabeth_at_end_of_turn_after_third_skill() {
    let mut ids = [Some(8), Some(458), Some(384), Some(11), Some(22), None];
    let action = Action::Servant {
        id: "a1".into(),
        servant: Some("servant_2".into()),
        servant_member_id: None,
        servant_id: None,
        servant_is_support: false,
        skill: Some("skill_3".into()),
        skill_selection: None,
        target: None,
        target_member_id: None,
        target_servant_id: None,
        target_is_support: false,
    };

    apply_party_lineup_change_at(&mut ids, &action, ChangeOrderTiming::EndOfTurn);

    assert_eq!(ids, [Some(8), Some(11), Some(384), Some(22), None, None]);
}

#[test]
fn action_frontline_available_rejects_source_out_of_frontline() {
    let ids = [Some(8), Some(434), Some(384), Some(11), Some(22), None];
    let action = Action::Servant {
        id: "a1".into(),
        servant: Some("servant_4".into()),
        servant_member_id: None,
        servant_id: None,
        servant_is_support: false,
        skill: Some("skill_1".into()),
        skill_selection: None,
        target: None,
        target_member_id: None,
        target_servant_id: None,
        target_is_support: false,
    };

    assert!(!action_frontline_available(&ids, &action));
}

#[test]
fn action_frontline_available_rejects_missing_frontline_target() {
    let ids = [Some(8), None, Some(384), Some(11), Some(22), None];
    let action = Action::Servant {
        id: "a1".into(),
        servant: Some("servant_1".into()),
        servant_member_id: None,
        servant_id: None,
        servant_is_support: false,
        skill: Some("skill_1".into()),
        skill_selection: None,
        target: Some("servant_2".into()),
        target_member_id: None,
        target_servant_id: None,
        target_is_support: false,
    };

    assert!(!action_frontline_available(&ids, &action));
}

#[test]
fn advanced_auto_np_output_prefers_ready_np_and_arts_cards() {
    let scene = AdvancedBattleScene {
        id: "advanced_scene_1".into(),
        enemy_target: None,
        main_output: Some(crate::AdvancedMainOutput {
            member_id: None,
            servant: Some("servant_1".into()),
            servant_id: None,
            is_support: false,
            output_type: Some(AdvancedOutputType::Np),
            np_card: Some("arts".into()),
        }),
        grand_auto_order_change: None,
        command_conditions: Vec::new(),
        control_actions: Vec::new(),
        turns: Vec::new(),
        startup_actions: Vec::new(),
        rules: Vec::new(),
    };
    let cards = vec![
        command_card(0, Some(10), Some("b"), None),
        command_card(1, Some(10), Some("a"), None),
        command_card(2, Some(20), Some("a"), None),
        command_card(3, Some(30), Some("q"), None),
        command_card(4, Some(30), Some("b"), None),
    ];
    let nps = vec![np_slot(0, true), np_slot(1, false), np_slot(2, false)];
    let picks = choose_advanced_auto_picks(
        &scene,
        &cards,
        &nps,
        &[Some(10), Some(20), Some(30)],
        &[false, false, false],
        &[],
        &GrandCardStrategy::default(),
    );

    assert!(picks
        .iter()
        .any(|pick| matches!(pick, Pick::Np { slot: 0, .. })));
    assert!(picks
        .iter()
        .any(|pick| matches!(pick, Pick::Card { slot: 1, .. })));
    assert!(picks
        .iter()
        .any(|pick| matches!(pick, Pick::Card { slot: 2, .. })));
}

#[test]
fn grand_auto_main_exquisite_damage_puts_np_last() {
    let scene = empty_advanced_scene();
    let cards = vec![
        command_card(0, Some(10), Some("q"), None),
        command_card(1, Some(10), Some("a"), None),
        command_card(2, Some(20), Some("b"), None),
        command_card(3, Some(30), Some("q"), None),
        command_card(4, Some(30), Some("a"), None),
    ];
    let nps = vec![np_slot(0, true), np_slot(1, false), np_slot(2, false)];
    let grands = vec![grand_config(10, "buster", "damage")];
    let picks = choose_advanced_auto_picks(
        &scene,
        &cards,
        &nps,
        &[Some(10), Some(20), Some(30)],
        &[false, false, false],
        &grands,
        &GrandCardStrategy::default(),
    );

    assert_eq!(pick_labels(&picks), vec!["C0", "C1", "NP0"]);
}

#[test]
fn grand_auto_main_np_color_chain_places_np_last() {
    let scene = empty_advanced_scene();
    let cards = vec![
        command_card(0, Some(10), Some("b"), None),
        command_card(1, Some(10), Some("b"), None),
        command_card(2, Some(20), Some("a"), None),
        command_card(3, Some(30), Some("q"), None),
        command_card(4, Some(30), Some("a"), None),
    ];
    let nps = vec![np_slot(0, true), np_slot(1, false), np_slot(2, false)];
    let grands = vec![grand_config(10, "buster", "np")];
    let picks = choose_advanced_auto_picks(
        &scene,
        &cards,
        &nps,
        &[Some(10), Some(20), Some(30)],
        &[false, false, false],
        &grands,
        &GrandCardStrategy::default(),
    );

    assert_eq!(pick_labels(&picks), vec!["C0", "C1", "NP0"]);
}

#[test]
fn grand_auto_fallback_uses_deputy_np_as_overcharge_before_main_np() {
    let scene = empty_advanced_scene();
    let cards = vec![
        command_card(0, Some(10), Some("a"), None),
        command_card(1, Some(20), Some("q"), None),
        command_card(2, Some(30), Some("b"), None),
        command_card(3, Some(30), Some("b"), None),
        command_card(4, Some(30), Some("b"), None),
    ];
    let nps = vec![np_slot(0, true), np_slot(1, true), np_slot(2, false)];
    let grands = vec![
        grand_config(10, "arts", "np"),
        grand_config(20, "quick", "damage"),
    ];
    let picks = choose_advanced_auto_picks(
        &scene,
        &cards,
        &nps,
        &[Some(10), Some(20), Some(30)],
        &[false, false, false],
        &grands,
        &GrandCardStrategy::default(),
    );

    assert_eq!(pick_labels(&picks), vec!["NP1", "C0", "NP0"]);
}

/// Reproduces the real-world Iori (id 405, buster NP) hand:
/// front [405, 7, 8] with hand [a/8, a/405, a/8, b/7, a/7] and NP1
/// (slot 0) ready. The hand can form a tier-3 "same-color arts +
/// has main 405" combo, but main NP is buster and only 1 buster
/// card is available, so the NP cannot fit any same-color chain.
/// Pre-fix the picker chose the all-arts combo and let the ready
/// main NP rot. With the new "main NP ready" tier, the picker must
/// include NP0 in the final picks.
#[test]
fn grand_auto_fires_ready_main_np_when_color_does_not_fit_same_color_chain() {
    let scene = empty_advanced_scene();
    let cards = vec![
        command_card(0, Some(8), Some("a"), None),
        command_card(1, Some(405), Some("a"), None),
        command_card(2, Some(8), Some("a"), None),
        command_card(3, Some(7), Some("b"), None),
        command_card(4, Some(7), Some("a"), None),
    ];
    let nps = vec![np_slot(0, true), np_slot(1, false), np_slot(2, false)];
    let grands = vec![grand_config(405, "buster", "damage")];
    let picks = choose_advanced_auto_picks(
        &scene,
        &cards,
        &nps,
        &[Some(405), Some(7), Some(8)],
        &[false, false, false],
        &grands,
        &GrandCardStrategy::default(),
    );

    assert!(
        picks
            .iter()
            .any(|pick| matches!(pick, Pick::Np { slot: 0, .. })),
        "picker dropped the ready main NP: {:?}",
        pick_labels(&picks)
    );
}

/// A ready main NP must outrank a deputy three-card same-color
/// chain, even though tier-4 deputy chains used to score above any
/// non-main combo. Front party: [10 (main), 20 (deputy), 30].
/// Hand contains a clean three-arts deputy chain plus the main NP
/// alone with no support cards from main, so the ONLY way to fire
/// the main NP is to give up the deputy chain.
#[test]
fn grand_auto_main_np_outranks_deputy_three_card_chain() {
    let scene = empty_advanced_scene();
    let cards = vec![
        command_card(0, Some(20), Some("a"), None),
        command_card(1, Some(20), Some("a"), None),
        command_card(2, Some(20), Some("a"), None),
        command_card(3, Some(30), Some("b"), None),
        command_card(4, Some(30), Some("q"), None),
    ];
    let nps = vec![np_slot(0, true), np_slot(1, false), np_slot(2, false)];
    let grands = vec![
        grand_config(10, "buster", "damage"),
        grand_config_at(1, 20, "arts", "np"),
    ];
    let picks = choose_advanced_auto_picks(
        &scene,
        &cards,
        &nps,
        &[Some(10), Some(20), Some(30)],
        &[false, false, false],
        &grands,
        &GrandCardStrategy::default(),
    );

    assert!(
        picks
            .iter()
            .any(|pick| matches!(pick, Pick::Np { slot: 0, .. })),
        "main NP was not fired: {:?}",
        pick_labels(&picks)
    );
}

#[test]
fn grand_auto_respects_custom_exquisite_brave_chain_priority_order() {
    let scene = empty_advanced_scene();
    let cards = vec![
        command_card(0, Some(20), Some("a"), None),
        command_card(1, Some(20), Some("b"), None),
        command_card(2, Some(20), Some("q"), None),
        command_card(3, Some(30), Some("a"), None),
        command_card(4, Some(30), Some("q"), None),
    ];
    let nps = vec![np_slot(0, true), np_slot(1, false), np_slot(2, false)];
    let grands = vec![
        grand_config(10, "buster", "damage"),
        grand_config_at(1, 20, "arts", "np"),
    ];
    let strategy = GrandCardStrategy {
        chain_priority: vec![
            GrandChainPriorityItem::DeputyBraveChain,
            GrandChainPriorityItem::MainReadyNp,
            GrandChainPriorityItem::MainBraveChain,
            GrandChainPriorityItem::MainColorChain,
            GrandChainPriorityItem::DeputyColorChain,
            GrandChainPriorityItem::Fallback,
        ],
        custom_rules: Vec::new(),
    };
    let picks = choose_advanced_auto_picks(
        &scene,
        &cards,
        &nps,
        &[Some(10), Some(20), Some(30)],
        &[false, false, false],
        &grands,
        &strategy,
    );

    assert_eq!(pick_labels(&picks), vec!["C1", "C0", "C2"]);
}

#[test]
fn berserker_grand_auto_main_np_color_chain_clicks_np_last() {
    let scene = empty_advanced_scene();
    let cards = vec![
        command_card(0, Some(20), Some("b"), None),
        command_card(1, Some(30), Some("b"), None),
        command_card(2, Some(10), Some("a"), None),
        command_card(3, Some(30), Some("q"), None),
        command_card(4, Some(20), Some("a"), None),
    ];
    let nps = vec![np_slot(0, true), np_slot(1, false), np_slot(2, false)];
    let grands = vec![grand_config(10, "buster", "damage")];
    let picks = choose_advanced_auto_picks_with_grand_class(
        &scene,
        &cards,
        &nps,
        &[Some(10), Some(20), Some(30)],
        &[false, false, false],
        &grands,
        &GrandCardStrategy::default(),
        GrandClass::Berserker,
    );

    assert_eq!(pick_labels(&picks), vec!["C0", "C1", "NP0"]);
}

fn lancer_picks(
    cards: Vec<CommandCardMatch>,
    nps: Vec<NoblePhantasmMatch>,
    single_color: &str,
    aoe_color: &str,
) -> Vec<Pick> {
    choose_advanced_auto_picks_with_grand_class(
        &empty_advanced_scene(),
        &cards,
        &nps,
        &[Some(10), Some(20), Some(30)],
        &[false, false, false],
        &[
            grand_config_at(0, 10, single_color, "damage"),
            grand_config_at(1, 20, aoe_color, "damage"),
        ],
        &GrandCardStrategy::default(),
        GrandClass::Lancer,
    )
}

#[test]
fn lancer_grand_dual_np_prefers_exquisite_chain_and_fixed_np_order() {
    let picks = lancer_picks(
        vec![
            command_card(0, Some(30), Some("b"), None),
            command_card(1, Some(20), Some("q"), None),
            command_card(2, Some(10), Some("q"), None),
        ],
        vec![np_slot(0, true), np_slot(1, true), np_slot(2, false)],
        "buster",
        "arts",
    );

    assert_eq!(pick_labels(&picks), vec!["NP1", "NP0", "C2"]);
}

#[test]
fn lancer_grand_dual_np_prefers_same_color_then_owner_priority() {
    let same_color = lancer_picks(
        vec![
            command_card(0, Some(20), Some("a"), None),
            command_card(1, Some(10), Some("a"), None),
            command_card(2, Some(10), Some("b"), None),
        ],
        vec![np_slot(0, true), np_slot(1, true), np_slot(2, false)],
        "arts",
        "arts",
    );
    assert_eq!(pick_labels(&same_color), vec!["NP1", "NP0", "C1"]);

    let fallback = lancer_picks(
        vec![
            command_card(0, Some(20), Some("a"), None),
            command_card(1, Some(10), Some("b"), None),
            command_card(2, Some(30), Some("a"), None),
        ],
        vec![np_slot(0, true), np_slot(1, true), np_slot(2, false)],
        "buster",
        "arts",
    );
    assert_eq!(pick_labels(&fallback), vec!["NP1", "NP0", "C1"]);
}

#[test]
fn lancer_grand_single_ready_np_uses_single_aoe_other_and_a_q_b_filler_order() {
    let picks = lancer_picks(
        vec![
            command_card(0, Some(20), Some("a"), None),
            command_card(1, Some(10), Some("b"), None),
            command_card(2, Some(10), Some("q"), None),
            command_card(3, Some(10), Some("a"), None),
            command_card(4, Some(30), Some("a"), None),
        ],
        vec![np_slot(0, false), np_slot(1, true), np_slot(2, false)],
        "buster",
        "arts",
    );

    assert_eq!(pick_labels(&picks), vec!["NP1", "C3", "C2"]);
}

#[test]
fn lancer_grand_ignores_non_grand_np_and_uses_command_cards_when_grand_nps_unready() {
    let picks = lancer_picks(
        vec![
            command_card(0, Some(20), Some("a"), None),
            command_card(1, Some(10), Some("b"), None),
            command_card(2, Some(10), Some("q"), None),
            command_card(3, Some(10), Some("a"), None),
            command_card(4, Some(30), Some("a"), None),
        ],
        vec![np_slot(0, false), np_slot(1, false), np_slot(2, true)],
        "buster",
        "arts",
    );

    assert_eq!(pick_labels(&picks), vec!["C3", "C2", "C1"]);
}

#[test]
fn lancer_grand_incomplete_hand_keeps_owner_and_a_q_b_priority() {
    let picks = lancer_picks(
        vec![
            command_card(0, Some(10), Some("b"), None),
            command_card(1, Some(10), Some("a"), None),
        ],
        Vec::new(),
        "buster",
        "arts",
    );

    assert_eq!(pick_labels(&picks), vec!["C1", "C0"]);
}

#[test]
fn berserker_grand_auto_main_np_color_chain_prioritizes_grand_any_slots() {
    let scene = empty_advanced_scene();
    let cards = vec![
        command_card(0, Some(30), Some("b"), None),
        command_card(1, Some(10), Some("b"), None),
        command_card(2, Some(20), Some("b"), None),
        command_card(3, Some(30), Some("b"), None),
        command_card(4, Some(30), Some("a"), None),
    ];
    let nps = vec![np_slot(0, true), np_slot(1, false), np_slot(2, false)];
    let grands = vec![
        grand_config(10, "buster", "damage"),
        grand_config_at(1, 20, "arts", "damage"),
    ];
    let picks = choose_advanced_auto_picks_with_grand_class(
        &scene,
        &cards,
        &nps,
        &[Some(10), Some(20), Some(30)],
        &[false, false, false],
        &grands,
        &GrandCardStrategy::default(),
        GrandClass::Berserker,
    );

    assert_eq!(pick_labels(&picks), vec!["C1", "C2", "NP0"]);
}

#[test]
fn berserker_grand_auto_main_np_outranks_deputy_np_color_chain() {
    let scene = empty_advanced_scene();
    let cards = vec![
        command_card(0, Some(10), Some("a"), None),
        command_card(1, Some(10), Some("a"), None),
        command_card(2, Some(20), Some("b"), None),
        command_card(3, Some(30), Some("b"), None),
        command_card(4, Some(30), Some("a"), None),
    ];
    let nps = vec![np_slot(0, true), np_slot(1, true), np_slot(2, false)];
    let grands = vec![
        grand_config(10, "arts", "damage"),
        grand_config_at(1, 20, "buster", "damage"),
    ];
    let picks = choose_advanced_auto_picks_with_grand_class(
        &scene,
        &cards,
        &nps,
        &[Some(10), Some(20), Some(30)],
        &[false, false, false],
        &grands,
        &GrandCardStrategy::default(),
        GrandClass::Berserker,
    );

    assert_eq!(pick_labels(&picks), vec!["C0", "C1", "NP0"]);
}

#[test]
fn berserker_grand_auto_main_np_ready_prioritizes_grand_free_cards() {
    let scene = empty_advanced_scene();
    let cards = vec![
        command_card(0, Some(10), Some("b"), None),
        command_card(1, Some(20), Some("q"), None),
        command_card(2, Some(30), Some("b"), None),
        command_card(3, Some(30), Some("q"), None),
        command_card(4, Some(30), Some("b"), None),
    ];
    let nps = vec![np_slot(0, true), np_slot(1, false), np_slot(2, true)];
    let grands = vec![
        grand_config(10, "arts", "damage"),
        grand_config_at(1, 20, "buster", "damage"),
    ];
    let picks = choose_advanced_auto_picks_with_grand_class(
        &scene,
        &cards,
        &nps,
        &[Some(10), Some(20), Some(30)],
        &[false, false, false],
        &grands,
        &GrandCardStrategy::default(),
        GrandClass::Berserker,
    );

    assert_eq!(pick_labels(&picks), vec!["C0", "C1", "NP0"]);
}

#[test]
fn berserker_grand_auto_main_np_color_chain_places_ready_deputy_np_second() {
    let scene = empty_advanced_scene();
    let cards = vec![
        command_card(0, Some(10), Some("b"), None),
        command_card(1, Some(20), Some("q"), None),
        command_card(2, Some(30), Some("b"), None),
        command_card(3, Some(30), Some("q"), None),
        command_card(4, Some(30), Some("q"), None),
    ];
    let nps = vec![np_slot(0, true), np_slot(1, true), np_slot(2, false)];
    let grands = vec![
        grand_config(10, "buster", "damage"),
        grand_config_at(1, 20, "buster", "damage"),
    ];
    let picks = choose_advanced_auto_picks_with_grand_class(
        &scene,
        &cards,
        &nps,
        &[Some(10), Some(20), Some(30)],
        &[false, false, false],
        &grands,
        &GrandCardStrategy::default(),
        GrandClass::Berserker,
    );

    assert_eq!(pick_labels(&picks), vec!["C0", "NP1", "NP0"]);
}

#[test]
fn berserker_grand_auto_main_np_ready_places_ready_deputy_np_second() {
    let scene = empty_advanced_scene();
    let cards = vec![
        command_card(0, Some(10), Some("b"), None),
        command_card(1, Some(20), Some("q"), None),
        command_card(2, Some(30), Some("b"), None),
        command_card(3, Some(30), Some("q"), None),
        command_card(4, Some(30), Some("q"), None),
    ];
    let nps = vec![np_slot(0, true), np_slot(1, true), np_slot(2, false)];
    let grands = vec![
        grand_config(10, "arts", "damage"),
        grand_config_at(1, 20, "buster", "damage"),
    ];
    let picks = choose_advanced_auto_picks_with_grand_class(
        &scene,
        &cards,
        &nps,
        &[Some(10), Some(20), Some(30)],
        &[false, false, false],
        &grands,
        &GrandCardStrategy::default(),
        GrandClass::Berserker,
    );

    assert_eq!(pick_labels(&picks), vec!["C0", "NP1", "NP0"]);
}

#[test]
fn berserker_grand_auto_deputy_np_color_chain_outranks_main_other_same_color_without_main_np() {
    let scene = empty_advanced_scene();
    let cards = vec![
        command_card(0, Some(10), Some("a"), None),
        command_card(1, Some(10), Some("a"), None),
        command_card(2, Some(20), Some("b"), None),
        command_card(3, Some(30), Some("b"), None),
        command_card(4, Some(30), Some("a"), None),
    ];
    let nps = vec![np_slot(0, false), np_slot(1, true), np_slot(2, false)];
    let grands = vec![
        grand_config(10, "arts", "damage"),
        grand_config_at(1, 20, "buster", "damage"),
    ];
    let picks = choose_advanced_auto_picks_with_grand_class(
        &scene,
        &cards,
        &nps,
        &[Some(10), Some(20), Some(30)],
        &[false, false, false],
        &grands,
        &GrandCardStrategy::default(),
        GrandClass::Berserker,
    );

    assert_eq!(pick_labels(&picks), vec!["C3", "C2", "NP1"]);
}

#[test]
fn berserker_grand_auto_other_same_color_runs_when_main_np_is_unavailable() {
    let scene = empty_advanced_scene();
    let cards = vec![
        command_card(0, Some(10), Some("a"), None),
        command_card(1, Some(20), Some("a"), None),
        command_card(2, Some(30), Some("a"), None),
        command_card(3, Some(20), Some("b"), None),
        command_card(4, Some(30), Some("q"), None),
    ];
    let nps = vec![np_slot(0, false), np_slot(1, false), np_slot(2, false)];
    let grands = vec![grand_config(10, "quick", "damage")];
    let picks = choose_advanced_auto_picks_with_grand_class(
        &scene,
        &cards,
        &nps,
        &[Some(10), Some(20), Some(30)],
        &[false, false, false],
        &grands,
        &GrandCardStrategy::default(),
        GrandClass::Berserker,
    );

    assert_eq!(pick_labels(&picks), vec!["C1", "C2", "C0"]);
}

#[test]
fn berserker_grand_auto_exquisite_chain_uses_buster_arts_quick_slot_order() {
    let scene = empty_advanced_scene();
    let cards = vec![
        command_card(0, Some(30), Some("q"), None),
        command_card(1, Some(20), Some("a"), None),
        command_card(2, Some(30), Some("b"), None),
        command_card(3, Some(10), Some("q"), None),
        command_card(4, Some(20), Some("a"), None),
    ];
    let nps = vec![np_slot(0, false), np_slot(1, false), np_slot(2, false)];
    let grands = vec![
        grand_config(10, "buster", "damage"),
        grand_config_at(1, 20, "arts", "damage"),
    ];
    let picks = choose_advanced_auto_picks_with_grand_class(
        &scene,
        &cards,
        &nps,
        &[Some(10), Some(20), Some(30)],
        &[false, false, false],
        &grands,
        &GrandCardStrategy::default(),
        GrandClass::Berserker,
    );

    assert_eq!(pick_labels(&picks), vec!["C2", "C1", "C3"]);
}

/// A ready DEPUTY NP, on its own, must NOT outrank a main same-
/// color chain (tier 3). Only the main NP gets the priority bump,
/// because the user expectation of "fire NP instead of charging
/// it" is specifically about the main output.
#[test]
fn grand_auto_deputy_ready_np_does_not_outrank_main_same_color_chain() {
    let scene = empty_advanced_scene();
    let cards = vec![
        command_card(0, Some(10), Some("a"), None),
        command_card(1, Some(10), Some("a"), None),
        command_card(2, Some(30), Some("a"), None),
        command_card(3, Some(30), Some("b"), None),
        command_card(4, Some(30), Some("q"), None),
    ];
    let nps = vec![np_slot(0, false), np_slot(1, true), np_slot(2, false)];
    let grands = vec![
        grand_config(10, "arts", "np"),
        grand_config_at(1, 20, "buster", "damage"),
    ];
    let picks = choose_advanced_auto_picks(
        &scene,
        &cards,
        &nps,
        &[Some(10), Some(20), Some(30)],
        &[false, false, false],
        &grands,
        &GrandCardStrategy::default(),
    );

    let labels = pick_labels(&picks);
    assert!(
        !labels.iter().any(|label| label == "NP1"),
        "deputy NP should not displace main same-color chain: {:?}",
        labels
    );
}

fn custom_rule_slot(servant_id: Option<u32>, kind: &str, color: &str) -> GrandCardRuleSlotConfig {
    GrandCardRuleSlotConfig {
        member_id: None,
        slot_index: None,
        servant_id,
        is_support: false,
        grand_servant: false,
        kind: kind.into(),
        color: color.into(),
    }
}

fn custom_rule_slot_at(
    slot_index: u32,
    servant_id: u32,
    kind: &str,
    color: &str,
) -> GrandCardRuleSlotConfig {
    GrandCardRuleSlotConfig {
        member_id: None,
        slot_index: Some(slot_index),
        servant_id: Some(servant_id),
        is_support: false,
        grand_servant: false,
        kind: kind.into(),
        color: color.into(),
    }
}

fn custom_grand_rule_slot(kind: &str, color: &str) -> GrandCardRuleSlotConfig {
    GrandCardRuleSlotConfig {
        member_id: None,
        slot_index: None,
        servant_id: None,
        is_support: false,
        grand_servant: true,
        kind: kind.into(),
        color: color.into(),
    }
}

fn custom_strategy(rule: GrandCardRuleConfig) -> GrandCardStrategy {
    GrandCardStrategy {
        chain_priority: default_grand_chain_priority(),
        custom_rules: vec![rule],
    }
}

#[test]
fn custom_grand_rule_config_maps_supported_constraints() {
    let rule = custom_rule_config_to_rule(&GrandCardRuleConfig {
        id: "custom_1".into(),
        name: "约束".into(),
        slots: vec![
            custom_rule_slot(None, "any", "buster"),
            custom_rule_slot(Some(20), "np", "quick"),
            custom_grand_rule_slot("command", "arts"),
        ],
    })
    .expect("supported custom rule should convert");

    assert!(!rule.same_color);
    assert!(!rule.color_set_baq);
    assert!(rule.include.is_empty());
    assert!(rule.exclude.is_empty());
    assert_eq!(rule.slots[0].owner, RuleOwner::Any);
    assert_eq!(rule.slots[0].kind, RuleKind::Any);
    assert_eq!(rule.slots[1].owner, RuleOwner::ExactServant(20));
    assert_eq!(rule.slots[1].kind, RuleKind::Np);
    assert_eq!(rule.slots[2].owner, RuleOwner::AnyGrand);
    assert_eq!(rule.slots[2].kind, RuleKind::Command);
    assert_eq!(rule.target_role, None);
}

#[test]
fn custom_grand_rule_accepts_any_servant_slots() {
    let scene = empty_advanced_scene();
    let cards = vec![
        command_card(0, Some(10), Some("q"), None),
        command_card(1, Some(20), Some("b"), None),
        command_card(2, Some(30), Some("a"), None),
        command_card(3, Some(20), Some("q"), None),
        command_card(4, Some(30), Some("b"), None),
    ];
    let nps = vec![np_slot(0, true), np_slot(1, false), np_slot(2, false)];
    let grands = vec![grand_config(10, "buster", "damage")];
    let strategy = custom_strategy(GrandCardRuleConfig {
        id: "custom_any".into(),
        name: "指定宝具加任意两张".into(),
        slots: vec![
            custom_rule_slot(Some(10), "np", "any"),
            custom_rule_slot(None, "any", "any"),
            custom_rule_slot(None, "any", "any"),
        ],
    });

    let picks = choose_advanced_auto_picks(
        &scene,
        &cards,
        &nps,
        &[Some(10), Some(20), Some(30)],
        &[false, false, false],
        &grands,
        &strategy,
    );

    assert_eq!(pick_labels(&picks), vec!["NP0", "C1", "C0"]);
}

#[test]
fn custom_grand_rule_takes_priority_before_builtin_rules() {
    let scene = empty_advanced_scene();
    let cards = vec![
        command_card(0, Some(10), Some("q"), None),
        command_card(1, Some(10), Some("a"), None),
        command_card(2, Some(20), Some("b"), None),
        command_card(3, Some(30), Some("q"), None),
        command_card(4, Some(30), Some("a"), None),
    ];
    let nps = vec![np_slot(0, true), np_slot(1, false), np_slot(2, false)];
    let grands = vec![
        grand_config(10, "buster", "damage"),
        grand_config_at(1, 20, "arts", "damage"),
    ];
    let strategy = custom_strategy(GrandCardRuleConfig {
        id: "custom_1".into(),
        name: "先打副手红卡".into(),
        slots: vec![
            custom_rule_slot(Some(20), "any", "buster"),
            custom_rule_slot(Some(10), "any", "quick"),
            custom_rule_slot(Some(10), "np", "any"),
        ],
    });

    let picks = choose_advanced_auto_picks(
        &scene,
        &cards,
        &nps,
        &[Some(10), Some(20), Some(30)],
        &[false, false, false],
        &grands,
        &strategy,
    );

    assert_eq!(pick_labels(&picks), vec!["C2", "C0", "NP0"]);
}

#[test]
fn custom_grand_rule_matches_duplicate_servant_by_slot_index() {
    let candidates = vec![
        AdvancedPickCandidate {
            pick: Pick::Np {
                slot: 0,
                point: Point::new(0.0, 0.0),
                from_priority: "test".into(),
            },
            servant_index: Some(0),
            servant_id: Some(10),
            color: None,
            original_order: 0,
            is_np: true,
        },
        AdvancedPickCandidate {
            pick: Pick::Np {
                slot: 1,
                point: Point::new(0.0, 0.0),
                from_priority: "test".into(),
            },
            servant_index: Some(1),
            servant_id: Some(10),
            color: None,
            original_order: 1,
            is_np: true,
        },
        AdvancedPickCandidate {
            pick: Pick::Card {
                slot: 0,
                point: Point::new(0.0, 0.0),
                servant_id: Some(30),
                suit: Some("b".into()),
                from_priority: Some("test".into()),
            },
            servant_index: Some(2),
            servant_id: Some(30),
            color: Some("b".into()),
            original_order: 10,
            is_np: false,
        },
    ];
    let rule = custom_rule_config_to_rule(&GrandCardRuleConfig {
        id: "custom_1".into(),
        name: "指定支援槽".into(),
        slots: vec![
            custom_rule_slot_at(1, 10, "np", "any"),
            custom_rule_slot(Some(30), "command", "buster"),
            custom_rule_slot_at(0, 10, "np", "any"),
        ],
    })
    .expect("slot-index rule should convert");
    let combo = vec![&candidates[0], &candidates[1], &candidates[2]];
    let matched = match_grand_rule(&combo, &rule, &[]).expect("rule should match by slot");
    let labels = pick_labels(
        &matched
            .ordered
            .into_iter()
            .map(|candidate| candidate.pick)
            .collect::<Vec<_>>(),
    );

    assert_eq!(labels, vec!["NP1", "C0", "NP0"]);
}

#[test]
fn grand_role_for_candidate_prefers_slot_over_duplicate_servant_id() {
    let grands = vec![
        GrandServantRuntimeConfig {
            slot_index: 0,
            servant_id: 10,
            is_support: false,
            np_card: "buster".into(),
            priority: "damage".into(),
            role: "main".into(),
        },
        GrandServantRuntimeConfig {
            slot_index: 1,
            servant_id: 10,
            is_support: true,
            np_card: "arts".into(),
            priority: "damage".into(),
            role: "deputy".into(),
        },
    ];
    let candidate = AdvancedPickCandidate {
        pick: Pick::Np {
            slot: 1,
            point: Point::new(0.0, 0.0),
            from_priority: "test".into(),
        },
        servant_index: Some(1),
        servant_id: Some(10),
        color: None,
        original_order: 1,
        is_np: true,
    };

    assert_eq!(
        grand_role_for_candidate(&candidate, &grands),
        GrandRole::Deputy
    );
}

#[test]
fn custom_grand_rule_prioritizes_main_then_deputy_for_grand_slots() {
    let scene = empty_advanced_scene();
    let cards = vec![
        command_card(0, Some(30), Some("a"), None),
        command_card(1, Some(10), Some("b"), None),
        command_card(2, Some(20), Some("q"), None),
        command_card(3, Some(30), Some("b"), None),
        command_card(4, Some(20), Some("a"), None),
    ];
    let nps = vec![np_slot(0, false), np_slot(1, false), np_slot(2, false)];
    let grands = vec![
        grand_config(10, "buster", "damage"),
        grand_config_at(1, 20, "arts", "damage"),
    ];
    let strategy = custom_strategy(GrandCardRuleConfig {
        id: "custom_1".into(),
        name: "冠位优先".into(),
        slots: vec![
            custom_grand_rule_slot("any", "any"),
            custom_grand_rule_slot("any", "any"),
            custom_grand_rule_slot("any", "any"),
        ],
    });

    let picks = choose_advanced_auto_picks(
        &scene,
        &cards,
        &nps,
        &[Some(10), Some(20), Some(30)],
        &[false, false, false],
        &grands,
        &strategy,
    );

    assert_eq!(pick_labels(&picks), vec!["C1", "C2", "C4"]);
}

#[test]
fn invalid_custom_grand_rule_falls_back_to_builtin_rules() {
    let scene = empty_advanced_scene();
    let cards = vec![
        command_card(0, Some(10), Some("q"), None),
        command_card(1, Some(10), Some("a"), None),
        command_card(2, Some(20), Some("b"), None),
        command_card(3, Some(30), Some("q"), None),
        command_card(4, Some(30), Some("a"), None),
    ];
    let nps = vec![np_slot(0, true), np_slot(1, false), np_slot(2, false)];
    let grands = vec![grand_config(10, "buster", "damage")];
    let strategy = custom_strategy(GrandCardRuleConfig {
        id: "invalid".into(),
        name: "无效".into(),
        slots: vec![
            custom_rule_slot(Some(20), "invalid", "buster"),
            custom_rule_slot(Some(10), "any", "quick"),
            custom_rule_slot(Some(10), "np", "any"),
        ],
    });

    let picks = choose_advanced_auto_picks(
        &scene,
        &cards,
        &nps,
        &[Some(10), Some(20), Some(30)],
        &[false, false, false],
        &grands,
        &strategy,
    );

    assert_eq!(pick_labels(&picks), vec!["C0", "C1", "NP0"]);
}

#[test]
fn run_config_round_trips_support_servant_id_and_repeat_flag() {
    let mut payload = minimal_run_config_json();
    payload["supportServantId"] = serde_json::json!(284);
    payload["supportNoblePhantasmLevelMin"] = serde_json::json!(2);
    payload["supportSkillLevelMins"] = serde_json::json!([10, null, 9]);
    payload["supportAppendSkillLevelMins"] = serde_json::json!([null, 10, null, null, 6]);
    payload["repeatMission"] = serde_json::json!(true);
    payload["maxMissionRuns"] = serde_json::json!(3);
    payload["apRecoveryItems"] = serde_json::json!(["gold", "bronze"]);
    let cfg: RunConfig = serde_json::from_value(payload).unwrap();
    assert_eq!(cfg.support_servant_id, Some(284));
    assert_eq!(cfg.grand_class, GrandClass::Saber);
    assert_eq!(cfg.support_noble_phantasm_level_min, Some(2));
    assert_eq!(cfg.support_skill_level_mins, [Some(10), None, Some(9)]);
    assert_eq!(
        cfg.support_append_skill_level_mins,
        [None, Some(10), None, None, Some(6)]
    );
    assert_eq!(cfg.repeat_mission, true);
    assert_eq!(cfg.max_mission_runs, Some(3));
    assert_eq!(
        cfg.ap_recovery_items,
        vec![ApRecoveryItem::Gold, ApRecoveryItem::Bronze]
    );
}

#[test]
fn run_config_round_trips_grand_class() {
    let mut payload = minimal_run_config_json();
    payload["grandClass"] = serde_json::json!("berserker");

    let cfg: RunConfig = serde_json::from_value(payload).unwrap();

    assert_eq!(cfg.grand_class, GrandClass::Berserker);
    let serialized = serde_json::to_value(&cfg).unwrap();
    assert_eq!(serialized["grandClass"], serde_json::json!("berserker"));
}

#[test]
fn run_config_round_trips_lancer_roles() {
    let mut payload = minimal_run_config_json();
    payload["grandClass"] = serde_json::json!("lancer");
    payload["grandServants"] = serde_json::json!([
        { "slotIndex": 0, "lancerRole": "single" },
        { "slotIndex": 1, "lancerRole": "aoe" }
    ]);

    let mut cfg: RunConfig = serde_json::from_value(payload).unwrap();
    assert_eq!(cfg.grand_class, GrandClass::Lancer);
    grand_strategy(cfg.grand_class).normalize_servants(&mut cfg.grand_servants);
    assert_eq!(cfg.grand_servants[0].role.as_deref(), Some("single"));
    assert_eq!(cfg.grand_servants[1].role.as_deref(), Some("aoe"));
    let serialized = serde_json::to_value(&cfg).unwrap();
    assert_eq!(serialized["grandServants"][0]["role"], "single");
    assert_eq!(serialized["grandServants"][1]["role"], "aoe");
    assert!(serialized["grandServants"][0].get("lancerRole").is_none());
}

#[test]
fn support_class_filter_keeps_single_tap_for_jp_extra_and_cn_standard_classes() {
    let jp_extra = support_class_filter_action(Server::Jp, "ruler", false).unwrap();
    let cn_standard = support_class_filter_action(Server::Cn, "caster", false).unwrap();

    for (action, expected) in [
        (jp_extra, SUPPORT_TAB_EXTRA),
        (cn_standard, SUPPORT_TAB_CASTER),
    ] {
        let SupportClassFilterAction::Tap(point) = action else {
            panic!("expected a single class-tab tap");
        };
        approx(point.x, expected.x);
        approx(point.y, expected.y);
    }
}

#[test]
fn support_class_filter_uses_cn_extra_dialog_slots() {
    let expected = [
        ("shielder", "盾兵", 0.260, 0.427),
        ("ruler", "裁定者", 0.420, 0.427),
        ("avenger", "复仇者", 0.580, 0.427),
        ("mooncancer", "月之癌", 0.740, 0.427),
        ("alterego", "他人格", 0.260, 0.649),
        ("foreigner", "降临者", 0.420, 0.649),
        ("pretender", "身披角色者", 0.580, 0.649),
        ("beasteresh", "兽", 0.740, 0.649),
    ];

    for (class_name, label, x, y) in expected {
        let action = support_class_filter_action(Server::Cn, class_name, false).unwrap();
        let SupportClassFilterAction::CnExtra(extra) = action else {
            panic!("expected CN EXTRA dialog action for {class_name}");
        };
        assert_eq!(extra.label, label);
        approx(extra.point.x, x);
        approx(extra.point.y, y);
    }
}

#[test]
fn support_class_filter_reuses_saved_cn_extra_choice_after_first_configuration() {
    let action = support_class_filter_action(Server::Cn, "foreigner", true).unwrap();
    let SupportClassFilterAction::Tap(point) = action else {
        panic!("expected saved CN EXTRA choice to reuse the ordinary tab");
    };
    approx(point.x, SUPPORT_TAB_EXTRA.x);
    approx(point.y, SUPPORT_TAB_EXTRA.y);
}

#[test]
fn support_class_filter_rejects_non_support_beast_variants() {
    assert!(support_class_filter_action(Server::Jp, "beasteresh", false).is_none());
    assert!(support_class_filter_action(Server::Cn, "beastii", false).is_none());
    assert!(support_class_filter_action(Server::Cn, "unbeastolgamarie", false).is_none());
}

#[test]
fn cn_extra_filter_uses_human_press_and_waits_for_result_overlay() {
    assert_eq!(SUPPORT_EXTRA_FILTER_LONG_PRESS_MS, 900);
    assert_eq!(SUPPORT_EXTRA_FILTER_CONFIRM_PRESS_MS, 100);
    assert_eq!(SUPPORT_EXTRA_FILTER_RESULT_SETTLE, Duration::from_secs(2));
}

#[test]
fn support_level_meets_treats_none_requirement_as_any() {
    assert!(support_level_meets(None, None));
    assert!(support_level_meets(Some(1), None));
    assert!(!support_level_meets(None, Some(1)));
    assert!(!support_level_meets(Some(4), Some(5)));
    assert!(support_level_meets(Some(5), Some(5)));
    assert!(support_level_meets(Some(10), Some(5)));
}

fn support_row(
    panel: Option<&str>,
    skills: Vec<Option<u32>>,
    append: Vec<Option<u32>>,
) -> SupportRowMatch {
    let region = NormRect {
        x: 0.1,
        y: 0.5,
        w: 0.4,
        h: 0.1,
    };
    SupportRowMatch {
        row_region: region,
        tap: Point::new(0.3, 0.55),
        name_text: "哈贝特洛特".into(),
        name_score: 1.0,
        name_matched_name: None,
        name_region: region,
        np_text: "为你纺织的时光之轮等级5".into(),
        np_score: 1.0,
        np_region: region,
        score_anchor: None,
        np_matched_name: "为你纺织的时光之轮".into(),
        np_level: Some(5),
        skill_panel: panel.map(str::to_string),
        skill_levels: skills,
        append_skill_levels: append,
        skill_level_diagnostics: Vec::new(),
    }
}

#[test]
fn support_row_tap_point_uses_left_side_of_confirm_button_anchor() {
    let mut row = support_row(None, vec![], vec![]);
    row.tap = Point::new(0.30, 0.55);
    row.score_anchor = Some(NormRect {
        x: 0.85,
        y: 0.86,
        w: 0.08,
        h: 0.06,
    });

    let point = support_row_tap_point(&row);
    assert!((point.x - 0.82).abs() < 1e-9);
    assert!((point.y - 0.89).abs() < 1e-9);
}

#[test]
fn support_row_tap_point_falls_back_to_ocr_row_tap() {
    let mut row = support_row(None, vec![], vec![]);
    row.tap = Point::new(0.30, 0.55);

    let point = support_row_tap_point(&row);
    assert!((point.x - 0.30).abs() < 1e-9);
    assert!((point.y - 0.55).abs() < 1e-9);
}

#[test]
fn support_level_progress_resets_panel_toggle_taps_on_new_candidate() {
    // The runner increments `panel_toggle_taps` outside the matcher,
    // but the matcher owns the lifecycle: whenever it sees a new
    // `candidate_key` it must also reset the tap counter so the
    // budget only applies to the current row. Across calls that
    // keep the same key the counter must be left untouched.
    let mut payload = minimal_run_config_json();
    payload["supportSkillLevelMins"] = serde_json::json!([10, null, null]);
    payload["supportAppendSkillLevelMins"] = serde_json::json!([null, 10, null, null, null]);
    let cfg: RunConfig = serde_json::from_value(payload).unwrap();
    let mut progress = SupportLevelPanelProgress::default();

    // First call: row A on the owned panel — owned passes, still
    // need append → WaitingForPanel; tap counter untouched.
    support_row_matches_level_requirements_with_progress(
        Server::Cn,
        &cfg,
        &support_row(Some("owned"), vec![Some(10), None, None], vec![]),
        &mut progress,
    );
    assert!(progress.owned_met);
    progress.panel_toggle_taps = 2;

    // Second call: row A again, same panel — matcher must not
    // clobber the counter the runner just incremented.
    support_row_matches_level_requirements_with_progress(
        Server::Cn,
        &cfg,
        &support_row(Some("owned"), vec![Some(10), None, None], vec![]),
        &mut progress,
    );
    assert_eq!(progress.panel_toggle_taps, 2);

    // Third call: row B (different y → different candidate_key) —
    // matcher resets the whole accumulator, including tap counter.
    let mut row_b = support_row(Some("owned"), vec![Some(10), None, None], vec![]);
    row_b.row_region.y = 0.62;
    support_row_matches_level_requirements_with_progress(Server::Cn, &cfg, &row_b, &mut progress);
    assert_eq!(progress.panel_toggle_taps, 0);
}

#[test]
fn support_level_filter_accumulates_owned_and_append_panels() {
    let mut payload = minimal_run_config_json();
    payload["supportSkillLevelMins"] = serde_json::json!([10, null, 9]);
    payload["supportAppendSkillLevelMins"] = serde_json::json!([null, 10, null, null, null]);
    let cfg: RunConfig = serde_json::from_value(payload).unwrap();
    let mut progress = SupportLevelPanelProgress::default();

    assert_eq!(
        support_row_matches_level_requirements_with_progress(
            Server::Cn,
            &cfg,
            &support_row(Some("owned"), vec![Some(10), Some(1), Some(9)], vec![]),
            &mut progress,
        ),
        SupportLevelFilter::WaitingForPanel
    );
    assert!(progress.owned_met);
    assert!(!progress.append_met);

    assert_eq!(
        support_row_matches_level_requirements_with_progress(
            Server::Cn,
            &cfg,
            &support_row(
                Some("append"),
                vec![],
                vec![None, Some(10), None, None, None],
            ),
            &mut progress,
        ),
        SupportLevelFilter::Pass
    );
}

#[test]
fn support_level_filter_fails_visible_panel_before_waiting_for_other_panel() {
    let mut payload = minimal_run_config_json();
    payload["supportSkillLevelMins"] = serde_json::json!([10, null, null]);
    payload["supportAppendSkillLevelMins"] = serde_json::json!([null, 10, null, null, null]);
    let cfg: RunConfig = serde_json::from_value(payload).unwrap();
    let mut progress = SupportLevelPanelProgress::default();

    assert_eq!(
        support_row_matches_level_requirements_with_progress(
            Server::Cn,
            &cfg,
            &support_row(Some("owned"), vec![Some(9), Some(10), Some(10)], vec![]),
            &mut progress,
        ),
        SupportLevelFilter::Fail("持有技能 1 ≥ 10（实际 9）".into())
    );
    assert!(!progress.owned_met);
}

#[test]
fn support_level_filter_reports_first_mismatch_per_panel() {
    // The Fail payload is what the runner surfaces in the UI log,
    // so pin the format down: NP / 持有 / 追加 each get their own
    // labelled reason, with the offending slot index (1-based)
    // and the OCR'd actual value (or `-` when not detected).
    let mut payload = minimal_run_config_json();
    payload["supportNoblePhantasmLevelMin"] = serde_json::json!(5);
    payload["supportSkillLevelMins"] = serde_json::json!([10, 10, 10]);
    payload["supportAppendSkillLevelMins"] = serde_json::json!([null, 10, null, null, null]);
    let cfg: RunConfig = serde_json::from_value(payload).unwrap();

    // NP miss is checked before the panel branch is even consulted,
    // so a row whose owned panel would otherwise pass still fails
    // early with an NP reason.
    let mut progress = SupportLevelPanelProgress::default();
    let mut np_low = support_row(Some("owned"), vec![Some(10), Some(10), Some(10)], vec![]);
    np_low.np_level = Some(3);
    assert_eq!(
        support_row_matches_level_requirements_with_progress(
            Server::Cn,
            &cfg,
            &np_low,
            &mut progress,
        ),
        SupportLevelFilter::Fail("宝具 ≥ 5（实际 3）".into())
    );

    // Owned miss reports the first failing slot — slot 2 here —
    // even though slot 3 also fails downstream.
    let mut progress = SupportLevelPanelProgress::default();
    assert_eq!(
        support_row_matches_level_requirements_with_progress(
            Server::Cn,
            &cfg,
            &support_row(Some("owned"), vec![Some(10), Some(8), Some(7)], vec![]),
            &mut progress,
        ),
        SupportLevelFilter::Fail("持有技能 2 ≥ 10（实际 8）".into())
    );

    // Append miss with `None` value ⇒ formatted as `-` so
    // "skill icon never OCR'd" reads distinctly from "level 0".
    let mut progress = SupportLevelPanelProgress::default();
    assert_eq!(
        support_row_matches_level_requirements_with_progress(
            Server::Cn,
            &cfg,
            &support_row(Some("append"), vec![], vec![None, None, None, None, None],),
            &mut progress,
        ),
        SupportLevelFilter::Fail("追加技能 2 ≥ 10（实际 -）".into())
    );
}

#[test]
fn ap_recovery_template_maps_frontend_items_to_template_keys() {
    assert_eq!(
        ap_recovery_template(ApRecoveryItem::Rainbow),
        ApRecoveryTemplate {
            item: ApRecoveryItem::Rainbow,
            page: ApRecoveryPage::Top,
            label: "圣晶石",
            template_key: "items/item_saint_quartz",
        }
    );
    assert_eq!(
        ap_recovery_template(ApRecoveryItem::Bronze),
        ApRecoveryTemplate {
            item: ApRecoveryItem::Bronze,
            page: ApRecoveryPage::Bottom,
            label: "青铜苹果",
            template_key: "items/item_apple_bronzed_cobalt",
        }
    );
    assert_eq!(
        ap_recovery_template(ApRecoveryItem::Copper),
        ApRecoveryTemplate {
            item: ApRecoveryItem::Copper,
            page: ApRecoveryPage::Bottom,
            label: "赤铜苹果",
            template_key: "items/item_apple_bronze",
        }
    );
}

#[test]
fn ap_recovery_list_label_probes_keep_old_cn_template_and_add_new_one() {
    assert_eq!(
        ap_recovery_list_label_probes(Server::Cn),
        &[
            (AP_RECOVERY_LIST_LABEL_TEMPLATE, None),
            (
                AP_RECOVERY_LIST_LABEL_NEW_TEMPLATE,
                Some(AP_RECOVERY_LIST_LABEL_NEW_REFERENCE_WIDTH),
            ),
        ]
    );
    assert_eq!(
        ap_recovery_list_label_probes(Server::Jp),
        &[(AP_RECOVERY_LIST_LABEL_TEMPLATE, None)]
    );
}

#[test]
fn ap_recovery_candidates_for_page_preserves_priority_within_page() {
    let top = ap_recovery_candidates_for_page(
        &[
            ApRecoveryItem::Silver,
            ApRecoveryItem::Copper,
            ApRecoveryItem::Gold,
            ApRecoveryItem::Rainbow,
        ],
        ApRecoveryPage::Top,
    );
    assert_eq!(
        top.iter().map(|item| item.item).collect::<Vec<_>>(),
        vec![
            ApRecoveryItem::Gold,
            ApRecoveryItem::Silver,
            ApRecoveryItem::Rainbow,
        ]
    );

    let bottom = ap_recovery_candidates_for_page(
        &[
            ApRecoveryItem::Silver,
            ApRecoveryItem::Copper,
            ApRecoveryItem::Gold,
            ApRecoveryItem::Bronze,
        ],
        ApRecoveryPage::Bottom,
    );
    assert_eq!(
        bottom.iter().map(|item| item.item).collect::<Vec<_>>(),
        vec![ApRecoveryItem::Bronze, ApRecoveryItem::Copper]
    );
}

#[test]
fn ap_recovery_waits_half_a_second_between_item_and_confirmation_steps() {
    assert_eq!(AP_RECOVERY_TAP_SETTLE, Duration::from_millis(500));
}

#[test]
fn ap_recovery_confirm_region_uses_dialog_layout_for_each_item_group() {
    for item in [ApRecoveryItem::Rainbow, ApRecoveryItem::Gold] {
        let region = ap_recovery_confirm_region(item);
        approx(region.x, AP_RECOVERY_CONFIRM_UPPER_REGION.x);
        approx(region.y, AP_RECOVERY_CONFIRM_UPPER_REGION.y);
        approx(region.w, AP_RECOVERY_CONFIRM_UPPER_REGION.w);
        approx(region.h, AP_RECOVERY_CONFIRM_UPPER_REGION.h);
    }
    for item in [
        ApRecoveryItem::Silver,
        ApRecoveryItem::Bronze,
        ApRecoveryItem::Copper,
    ] {
        let region = ap_recovery_confirm_region(item);
        approx(region.x, AP_RECOVERY_CONFIRM_LOWER_REGION.x);
        approx(region.y, AP_RECOVERY_CONFIRM_LOWER_REGION.y);
        approx(region.w, AP_RECOVERY_CONFIRM_LOWER_REGION.w);
        approx(region.h, AP_RECOVERY_CONFIRM_LOWER_REGION.h);
    }
    assert_eq!(AP_RECOVERY_CONFIRM_TEMPLATE, "shared/button_dialog");
}

#[test]
fn ap_recovery_close_observation_yields_unknown_to_the_main_loop() {
    assert_eq!(
        classify_ap_recovery_close_observation(Screen::APRecovery),
        ApRecoveryCloseObservation::StillOpen
    );
    assert_eq!(
        classify_ap_recovery_close_observation(Screen::Unknown),
        ApRecoveryCloseObservation::Obscured
    );
    assert_eq!(
        classify_ap_recovery_close_observation(Screen::Battle),
        ApRecoveryCloseObservation::Closed
    );
}

#[test]
fn unknown_element_error_helper_matches_exact_lookup() {
    assert!(is_unknown_element_error(
        "unknown element: SupportSelect.dialog_refresh_support",
        "SupportSelect",
        "dialog_refresh_support",
    ));
    assert!(!is_unknown_element_error(
        "unknown element: SupportSelect.support_scroll_end",
        "SupportSelect",
        "dialog_refresh_support",
    ));
}

// --- tick_scene_state -----------------------------------------------
//
// These cover the full state-transition matrix for the BATTLE m/n
// reading. The previous implementation reset `skills_executed` (and
// therefore re-fired skills) on every iteration where the CV read
// returned `None`, which double-fired skills any time the NP overlay
// covered the strip. The helper now treats failed reads as "stay
// put" so the bug can't come back without breaking these tests.

#[test]
fn tick_scene_state_first_successful_read_snaps_index_to_screen_scene() {
    // First successful read of scene 1 from a fresh runner. The
    // snap takes m=1 → index 0, which matches the runner's default
    // starting index, so behaviour is identical to the legacy
    // "lock in but don't advance" semantics for the m=1 case.
    let tick = tick_scene_state(None, 0, None, Some(1));
    assert_eq!(
        tick,
        SceneTick {
            last_screen_scene: Some(1),
            current_scene_index: 0,
            needs_exec: true,
        }
    );
}

#[test]
fn tick_scene_state_first_successful_read_at_scene_two_snaps_to_index_one() {
    // Mid-quest start: the runner is launched while the screen is
    // already on scene 2/3. The first successful read must snap
    // current_scene_index to 1 so the user's second configured
    // command block runs — without the snap we would execute
    // block 0 for the actual scene 2 and only advance after the
    // *next* on-screen transition (i.e. when the screen moves to
    // 3/3), wasting block 1 entirely.
    let tick = tick_scene_state(None, 0, None, Some(2));
    assert_eq!(
        tick,
        SceneTick {
            last_screen_scene: Some(2),
            current_scene_index: 1,
            needs_exec: true,
        }
    );
}

#[test]
fn tick_scene_state_delayed_first_read_at_scene_two_replaces_default_exec() {
    // First poll's CV read failed (e.g. NP overlay covered the
    // strip), so the runner emitted the default block 0. The next
    // poll succeeds with m=2: we must snap the index to 1 and
    // re-execute, otherwise the user's scene-2 block never fires
    // for the entire scene.
    let tick = tick_scene_state(None, 0, Some(0), Some(2));
    assert_eq!(
        tick,
        SceneTick {
            last_screen_scene: Some(2),
            current_scene_index: 1,
            needs_exec: true,
        }
    );
}

#[test]
fn tick_scene_state_first_successful_read_at_scene_three_snaps_to_index_two() {
    // Same as the scene-2 case but proves the snap is general,
    // not a special-case of m=2.
    let tick = tick_scene_state(None, 0, None, Some(3));
    assert_eq!(
        tick,
        SceneTick {
            last_screen_scene: Some(3),
            current_scene_index: 2,
            needs_exec: true,
        }
    );
}

#[test]
fn tick_scene_state_failed_read_after_lockin_holds_state_and_skips_exec() {
    // We executed scene 0 last iteration; the CV now fails (NP
    // overlay). Index must not advance, last_screen_scene must
    // stay at the previously locked value, and needs_exec must be
    // false so we don't double-fire skills.
    let tick = tick_scene_state(Some(1), 0, Some(0), None);
    assert_eq!(
        tick,
        SceneTick {
            last_screen_scene: Some(1),
            current_scene_index: 0,
            needs_exec: false,
        }
    );
}

#[test]
fn tick_scene_state_same_scene_read_skips_exec() {
    let tick = tick_scene_state(Some(1), 0, Some(0), Some(1));
    assert_eq!(
        tick,
        SceneTick {
            last_screen_scene: Some(1),
            current_scene_index: 0,
            needs_exec: false,
        }
    );
}

#[test]
fn tick_scene_state_real_transition_advances_and_triggers_exec() {
    let tick = tick_scene_state(Some(1), 0, Some(0), Some(2));
    assert_eq!(
        tick,
        SceneTick {
            last_screen_scene: Some(2),
            current_scene_index: 1,
            needs_exec: true,
        }
    );
}

// --- command_spell_index -------------------------------------------

#[test]
fn command_spell_index_maps_known_names_to_dialog_rows() {
    // Dialog row order is documented next to `COMMAND_SPELL_OPTIONS`:
    // 0 = 宝具解放 (np_release), 1 = 灵基修复 (restore). If anyone
    // swaps the array entries without updating the helper the runner
    // would tap the wrong spell; this test pins the mapping.
    assert_eq!(command_spell_index(Some("np_release")), Some(0));
    assert_eq!(command_spell_index(Some("restore")), Some(1));
}

#[test]
fn battle_close_button_element_names_are_stable() {
    assert_eq!(
        SKILL_TARGET_CLOSE_BUTTON_ELEMENT,
        "skill_target_close_button"
    );
    assert_eq!(
        COMMAND_SPELL_CLOSE_BUTTON_ELEMENT,
        "command_spell_close_button"
    );
    assert_eq!(
        ORDER_CHANGE_CLOSE_BUTTON_ELEMENT,
        "order_change_close_button"
    );
    assert_eq!(ATTACK_BUTTON_ELEMENT, "attack_button");
    assert_eq!(BATTLE_ACTION_MENU_ELEMENT, "battle_action_menu");
}

#[test]
fn attack_speed_element_names_are_stable() {
    assert_eq!(ATTACK_SCREEN, "Attack");
    assert_eq!(ATTACK_SCREEN_SPEED_2_ELEMENT, "battle_speed_2");
    assert_eq!(ATTACK_SCREEN_SPEED_1_ELEMENT, "battle_speed_1");
}

#[test]
fn order_change_extra_settle_matches_expected_delay() {
    assert_eq!(ORDER_CHANGE_EXTRA_SETTLE, Duration::from_secs(1));
}

#[test]
fn command_spell_index_returns_none_for_unknown_or_missing() {
    // Unknown / missing spell names cause the runner's loop to
    // `continue` instead of tapping a phantom row; mirrors how
    // `skill_position` handles malformed inputs.
    assert_eq!(command_spell_index(None), None);
    assert_eq!(command_spell_index(Some("")), None);
    assert_eq!(command_spell_index(Some("self_destruct")), None);
}

#[test]
fn skill_selection_option_position_maps_supported_dialogs() {
    let selection = |selection_type: &str, index: u32, option_count: u32| crate::SkillSelection {
        selection_type: selection_type.into(),
        index,
        option_count: Some(option_count),
        label: None,
    };
    let point = |selection_type: &str, index: u32, option_count: u32| {
        skill_selection_option_position(&selection(selection_type, index, option_count))
            .map(|point| (point.x, point.y))
    };

    assert_eq!(
        point("SelectAddInfo", 0, 2),
        Some((
            SELECT_ADD_INFO_OPTIONS_2[0].x,
            SELECT_ADD_INFO_OPTIONS_2[0].y
        ))
    );
    assert_eq!(
        point("SelectAddInfo", 2, 3),
        Some((
            SELECT_ADD_INFO_OPTIONS_3[2].x,
            SELECT_ADD_INFO_OPTIONS_3[2].y
        ))
    );
    assert_eq!(
        point("selectTreasureDeviceInfo", 1, 2),
        Some((NP_SELECTION_OPTIONS_2[1].x, NP_SELECTION_OPTIONS_2[1].y))
    );
    assert_eq!(
        point("commandTypeSelfTreasureDevice", 2, 3),
        Some((NP_SELECTION_OPTIONS_3[2].x, NP_SELECTION_OPTIONS_3[2].y))
    );
    assert_eq!(point("SelectAddInfo", 2, 2), None);
    assert_eq!(point("SelectAddInfo", 0, 4), None);
    assert_eq!(point("unknown", 0, 2), None);
}

#[test]
fn classify_skill_use_dialog_luma_separates_confirm_and_already_used() {
    assert_eq!(
        classify_skill_use_dialog_luma(236.0, 210.0),
        SkillUseDialogState::Confirm
    );
    assert_eq!(
        classify_skill_use_dialog_luma(182.0, 210.0),
        SkillUseDialogState::AlreadyUsed
    );
}

#[test]
fn skill_selection_close_region_maps_supported_dialogs() {
    assert_eq!(
        skill_selection_close_region(SelectionDialogKind::AddInfo).x,
        SELECT_ADD_INFO_CLOSE.x
    );
    assert_eq!(
        skill_selection_close_region(SelectionDialogKind::TreasureDevice).y,
        SELECT_TREASURE_DEVICE_CLOSE.y
    );
    assert_eq!(
        skill_selection_close_region(SelectionDialogKind::SelfTreasureDevice).w,
        COMMAND_TYPE_SELF_TREASURE_DEVICE_CLOSE.w
    );
    assert_eq!(
        selection_dialog_kind("SelectAddInfo"),
        Some(SelectionDialogKind::AddInfo)
    );
    assert!(selection_dialog_kind("unknown").is_none());
}

#[test]
fn skill_post_tap_expectation_prefers_selection_before_target_or_activation() {
    let selection = crate::SkillSelection {
        selection_type: "SelectAddInfo".into(),
        index: 0,
        option_count: Some(2),
        label: None,
    };
    assert!(matches!(
        skill_post_tap_expectation(Some(&selection), Some(SKILL_TARGETS[0]), false),
        SkillPostTapExpectation::SelectionDialog(SelectionDialogKind::AddInfo)
    ));
    assert!(matches!(
        skill_post_tap_expectation(None, Some(SKILL_TARGETS[0]), false),
        SkillPostTapExpectation::TargetPicker
    ));
    assert!(matches!(
        skill_post_tap_expectation(None, None, true),
        SkillPostTapExpectation::OrderChange
    ));
    assert!(matches!(
        skill_post_tap_expectation(None, None, false),
        SkillPostTapExpectation::ActivationStart
    ));
}

#[test]
fn order_change_slot_position_requires_one_front_and_one_back_range() {
    let front = order_change_slot_position(Some("servant_1"), 0..3).unwrap();
    assert_eq!(
        (front.x, front.y),
        (ORDER_CHANGE_SLOTS[0].x, ORDER_CHANGE_SLOTS[0].y)
    );

    let back = order_change_slot_position(Some("servant_4"), 3..6).unwrap();
    assert_eq!(
        (back.x, back.y),
        (ORDER_CHANGE_SLOTS[3].x, ORDER_CHANGE_SLOTS[3].y)
    );

    assert!(order_change_slot_position(Some("servant_4"), 0..3).is_none());
    assert!(order_change_slot_position(Some("servant_2"), 3..6).is_none());
}

#[test]
fn enemy_target_position_maps_six_2k_reference_points() {
    let expected = [
        (282.0 / 2560.0, 66.0 / 1440.0),
        (682.0 / 2560.0, 66.0 / 1440.0),
        (1082.0 / 2560.0, 66.0 / 1440.0),
        (85.0 / 2560.0, 263.0 / 1440.0),
        (485.0 / 2560.0, 263.0 / 1440.0),
        (885.0 / 2560.0, 263.0 / 1440.0),
    ];

    for (index, (x, y)) in expected.into_iter().enumerate() {
        let point = enemy_target_position(Some(&format!("enemy_{}", index + 1))).unwrap();
        assert!((point.x - x).abs() < f64::EPSILON);
        assert!((point.y - y).abs() < f64::EPSILON);
    }

    assert!(enemy_target_position(None).is_none());
    assert!(enemy_target_position(Some("enemy_7")).is_none());
    assert!(enemy_target_position(Some("servant_1")).is_none());
}

#[test]
fn format_skill_tap_debug_includes_normalized_and_physical_position() {
    assert_eq!(
        format_skill_tap_debug("点击从者技能", Point::new(0.25, 0.5), 2560, 1440),
        "点击从者技能: x=0.250, y=0.500 (640, 720)"
    );
}

#[test]
fn debug_coordinates_include_all_enemy_targets() {
    let coords = debug_coordinates();
    let group = coords
        .groups
        .iter()
        .find(|group| group.id == "enemyTargets")
        .expect("enemy target coordinate group");

    let labels: Vec<&str> = group
        .points
        .iter()
        .map(|point| point.label.as_str())
        .collect();
    assert_eq!(
        labels,
        vec!["Enemy1", "Enemy2", "Enemy3", "Enemy4", "Enemy5", "Enemy6"]
    );
}

#[test]
fn turn_preparation_actions_preserves_configured_row_order() {
    let turn = BattleTurn {
        id: "turn_1".into(),
        preparation_actions: vec![
            Action::Equipment {
                id: "eq_1".into(),
                skill: Some("skill_2".into()),
                target: None,
                target_member_id: None,
                target_servant_id: None,
                target_is_support: false,
                order_change: None,
            },
            Action::Servant {
                id: "sa_1".into(),
                servant: Some("servant_1".into()),
                servant_member_id: None,
                servant_id: None,
                servant_is_support: false,
                skill: Some("skill_3".into()),
                skill_selection: None,
                target: Some("servant_2".into()),
                target_member_id: None,
                target_servant_id: None,
                target_is_support: false,
            },
            Action::CommandSpell {
                id: "cs_1".into(),
                spell: Some("restore".into()),
                target: Some("servant_1".into()),
                target_member_id: None,
                target_servant_id: None,
                target_is_support: false,
            },
        ],
        servant_actions: vec![],
        equipment_actions: vec![],
        command_spell_actions: vec![],
        enemy_target: None,
        attack_priority: vec![],
    };

    let kinds: Vec<&str> = turn_preparation_actions(&turn)
        .map(|action| match action {
            Action::Servant { .. } => "servant",
            Action::Equipment { .. } => "equipment",
            Action::CommandSpell { .. } => "commandSpell",
            Action::EnemyTarget { .. } => "enemyTarget",
        })
        .collect();

    assert_eq!(kinds, vec!["equipment", "servant", "commandSpell"]);
}

#[test]
fn tick_scene_state_failed_first_read_executes_default_index_zero() {
    // CV fails before we ever locked in a scene → we still want to
    // execute the configured first block so the runner doesn't
    // stall on a missing read. Subsequent successful reads must
    // not re-trigger execution for the same index.
    let first = tick_scene_state(None, 0, None, None);
    assert_eq!(
        first,
        SceneTick {
            last_screen_scene: None,
            current_scene_index: 0,
            needs_exec: true,
        }
    );
    // Caller would then mark executed_scene_index = Some(0). Next
    // iteration: still no successful read.
    let second = tick_scene_state(None, 0, Some(0), None);
    assert_eq!(second.needs_exec, false);
    // First successful read of scene 1: locks in but does NOT
    // advance the index (we never observed a transition), and
    // does NOT re-execute (executed already at index 0).
    let third = tick_scene_state(None, 0, Some(0), Some(1));
    assert_eq!(
        third,
        SceneTick {
            last_screen_scene: Some(1),
            current_scene_index: 0,
            needs_exec: false,
        }
    );
}

#[test]
fn post_attack_hud_read_gate_continues_when_hud_read_succeeds() {
    let now = Instant::now();
    let gate = post_attack_hud_read_gate(
        false,
        true,
        Some((2, 3)),
        None,
        now,
        POST_ATTACK_HUD_READ_TIMEOUT,
    );

    assert_eq!(gate, PostAttackHudReadGate::Ready);
}

#[test]
fn post_attack_hud_read_gate_waits_before_timeout() {
    let now = Instant::now();
    let gate =
        post_attack_hud_read_gate(false, true, None, None, now, POST_ATTACK_HUD_READ_TIMEOUT);

    assert_eq!(gate, PostAttackHudReadGate::Waiting { started_at: now });
}

#[test]
fn post_attack_hud_read_gate_times_out_to_legacy_progression() {
    let started_at = Instant::now();
    let now = started_at + POST_ATTACK_HUD_READ_TIMEOUT + Duration::from_millis(1);
    let gate = post_attack_hud_read_gate(
        false,
        true,
        None,
        Some(started_at),
        now,
        POST_ATTACK_HUD_READ_TIMEOUT,
    );

    assert_eq!(gate, PostAttackHudReadGate::TimedOut);
}

#[test]
fn post_attack_hud_read_gate_does_not_delay_advanced_mode() {
    let now = Instant::now();
    let gate = post_attack_hud_read_gate(true, true, None, None, now, POST_ATTACK_HUD_READ_TIMEOUT);

    assert_eq!(gate, PostAttackHudReadGate::Ready);
}

#[test]
fn attack_screen_wait_gate_allows_normal_battle_when_not_waiting() {
    let now = Instant::now();
    let gate = attack_screen_wait_gate(None, now, ATTACK_SCREEN_WAIT_TIMEOUT);

    assert_eq!(gate, AttackScreenWaitGate::NotWaiting);
}

#[test]
fn attack_screen_wait_gate_suppresses_duplicate_attack_tap_before_timeout() {
    let started_at = Instant::now();
    let now = started_at + ATTACK_SCREEN_WAIT_TIMEOUT - Duration::from_millis(1);
    let gate = attack_screen_wait_gate(Some(started_at), now, ATTACK_SCREEN_WAIT_TIMEOUT);

    assert_eq!(gate, AttackScreenWaitGate::Waiting);
}

#[test]
fn attack_screen_wait_gate_recovers_after_timeout() {
    let started_at = Instant::now();
    let now = started_at + ATTACK_SCREEN_WAIT_TIMEOUT;
    let gate = attack_screen_wait_gate(Some(started_at), now, ATTACK_SCREEN_WAIT_TIMEOUT);

    assert_eq!(gate, AttackScreenWaitGate::TimedOut);
}

#[test]
fn grand_support_section_requires_seen_then_two_misses_before_exhausted() {
    let mut seen = false;
    let mut misses = 0;

    assert!(!support_grand_section_exhausted_after_probe(
        Some(false),
        &mut seen,
        &mut misses
    ));
    assert!(!seen);
    assert_eq!(misses, 0);

    assert!(!support_grand_section_exhausted_after_probe(
        Some(true),
        &mut seen,
        &mut misses
    ));
    assert!(seen);
    assert_eq!(misses, 0);

    assert!(!support_grand_section_exhausted_after_probe(
        Some(false),
        &mut seen,
        &mut misses
    ));
    assert_eq!(misses, 1);
    assert!(support_grand_section_exhausted_after_probe(
        Some(false),
        &mut seen,
        &mut misses
    ));
}

#[test]
fn grand_support_section_ignores_unavailable_probe() {
    let mut seen = true;
    let mut misses = 1;

    assert!(!support_grand_section_exhausted_after_probe(
        None,
        &mut seen,
        &mut misses
    ));
    assert!(seen);
    assert_eq!(misses, 1);
}

fn anchor_at_y(y: f64) -> NormRect {
    NormRect {
        x: 0.846,
        y,
        w: 0.079,
        h: 0.05,
    }
}

#[test]
fn scroll_delta_falls_back_to_fixed_when_no_anchors() {
    assert!((scroll_support_list_delta(&[]) - SUPPORT_SCROLL_FALLBACK_DELTA).abs() < 1e-9);
}

#[test]
fn scroll_delta_moves_last_anchor_to_first_row_target() {
    let anchors = vec![anchor_at_y(0.364), anchor_at_y(0.642), anchor_at_y(0.919)];
    let delta = scroll_support_list_delta(&anchors);
    assert!((delta - (0.919 - SUPPORT_SCROLL_TARGET_TOP_ANCHOR_Y)).abs() < 1e-9);
    let new_position_of_last_button = 0.919 - delta;
    assert!((new_position_of_last_button - SUPPORT_SCROLL_TARGET_TOP_ANCHOR_Y).abs() < 1e-9);
}

#[test]
fn scroll_delta_uses_last_anchor_even_with_two_visible_buttons() {
    let anchors = vec![anchor_at_y(0.62), anchor_at_y(0.92)];
    let delta = scroll_support_list_delta(&anchors);
    assert!(
        (delta - (0.92 - SUPPORT_SCROLL_TARGET_TOP_ANCHOR_Y)).abs() < 1e-9,
        "got {delta}"
    );
}

#[test]
fn scroll_delta_does_not_extrapolate_clipped_offscreen_buttons() {
    let anchors = vec![anchor_at_y(0.462), anchor_at_y(0.740)];
    let delta = scroll_support_list_delta(&anchors);
    assert!(
        (delta - (0.740 - SUPPORT_SCROLL_TARGET_TOP_ANCHOR_Y)).abs() < 1e-9,
        "got {delta}"
    );
}

#[test]
fn scroll_delta_uses_last_anchor_near_screen_edge() {
    let anchors = vec![anchor_at_y(0.55), anchor_at_y(0.90)];
    let delta = scroll_support_list_delta(&anchors);
    assert!(
        (delta - (0.90 - SUPPORT_SCROLL_TARGET_TOP_ANCHOR_Y)).abs() < 1e-9,
        "got {delta}"
    );
}

#[test]
fn scroll_delta_uses_single_visible_button() {
    let anchors = vec![anchor_at_y(0.50)];
    let delta = scroll_support_list_delta(&anchors);
    assert!((delta - (0.50 - SUPPORT_SCROLL_TARGET_TOP_ANCHOR_Y)).abs() < 1e-9);
}

#[test]
fn scroll_delta_clamps_unusually_large_distance() {
    // Pathological case: 4+ visible cards with the bottom button
    // way down at y=0.99. Even though the raw delta would be 0.71,
    // the clamp keeps the swipe inside `SUPPORT_SCROLL_MAX_DELTA`
    // so a misdetection can't fling the list past the bottom.
    let anchors = vec![
        anchor_at_y(0.28),
        anchor_at_y(0.50),
        anchor_at_y(0.72),
        anchor_at_y(0.99),
    ];
    let delta = scroll_support_list_delta(&anchors);
    assert!((delta - SUPPORT_SCROLL_MAX_DELTA).abs() < 1e-9);
}

#[test]
fn scroll_delta_independent_of_anchor_input_order() {
    // Runner must not depend on sidecar returning anchors sorted.
    let sorted = vec![anchor_at_y(0.30), anchor_at_y(0.55), anchor_at_y(0.80)];
    let shuffled = vec![anchor_at_y(0.80), anchor_at_y(0.30), anchor_at_y(0.55)];
    let from_sorted = scroll_support_list_delta(&sorted);
    let from_shuffled = scroll_support_list_delta(&shuffled);
    assert!((from_sorted - from_shuffled).abs() < 1e-9);
}

#[test]
fn format_scroll_debug_lists_sorted_anchor_positions() {
    // The shuffled-input case is intentional: the message must be
    // human-readable regardless of detector order so an operator
    // reading the debug log can quickly eyeball whether the row
    // pitch looks right.
    let anchors = vec![anchor_at_y(0.80), anchor_at_y(0.30), anchor_at_y(0.55)];
    let msg = format_scroll_debug(&anchors, 0.500, 0.78, 0.28, 588, 400);
    assert!(
        msg.contains("0.300, 0.550, 0.800"),
        "expected sorted anchor list in message, got: {msg}"
    );
    assert!(msg.contains("Δ=0.500"));
    assert!(msg.contains("n=3"));
    assert!(msg.contains("swipe=0.78→0.28"));
    assert!(msg.contains("(588ms+400ms settle)"));
}

#[test]
fn format_scroll_debug_handles_empty_anchors() {
    let msg = format_scroll_debug(
        &[],
        SUPPORT_SCROLL_FALLBACK_DELTA,
        0.78,
        0.38,
        SUPPORT_SCROLL_MIN_DURATION_MS,
        SUPPORT_SCROLL_SETTLE_MS,
    );
    assert!(
        msg.contains("无"),
        "empty anchors should render as 无, got: {msg}"
    );
    assert!(msg.contains("n=0"));
}

#[test]
fn scroll_duration_scales_linearly_with_delta_within_bounds() {
    // Mid-range delta: duration should equal `delta / velocity * 1000`,
    // i.e. the velocity-matched value, neither clamped to the min
    // nor the max.
    let delta = 0.556;
    let expected = (delta / SUPPORT_SCROLL_VELOCITY * 1000.0).round() as u32;
    assert!(expected > SUPPORT_SCROLL_MIN_DURATION_MS);
    assert!(expected < SUPPORT_SCROLL_MAX_DURATION_MS);
    assert_eq!(scroll_support_list_duration_ms(delta), expected);
}

#[test]
fn scroll_duration_clamped_at_minimum_for_tiny_deltas() {
    // A near-zero delta would compute a duration of just a few ms,
    // which the OS touch dispatcher may reject as too fast. The min
    // clamp keeps every swipe a deliberate gesture.
    assert_eq!(
        scroll_support_list_duration_ms(0.01),
        SUPPORT_SCROLL_MIN_DURATION_MS
    );
    assert_eq!(
        scroll_support_list_duration_ms(0.0),
        SUPPORT_SCROLL_MIN_DURATION_MS
    );
}

#[test]
fn scroll_duration_clamped_at_maximum_for_pathological_deltas() {
    // A pathological delta (e.g. detector returning an anchor near
    // y=1.0 on a misaligned frame) must not stall the runner with a
    // multi-second swipe.
    assert_eq!(
        scroll_support_list_duration_ms(5.0),
        SUPPORT_SCROLL_MAX_DURATION_MS
    );
}

// ── CE mismatch reason / debug_summary separation ────────────────────────────

fn artwork_check(variant: &str, score: f64, threshold: f64, passed: bool) -> SupportCeArtworkCheck {
    SupportCeArtworkCheck {
        variant: variant.to_string(),
        region_kind: "ce".to_string(),
        score,
        threshold,
        passed,
        selected: passed,
        error: None,
    }
}

fn icon_check(kind: &str, score: f64, threshold: f64, passed: bool) -> SupportCeIconCheck {
    SupportCeIconCheck {
        kind: kind.to_string(),
        template_key: format!("{kind}_template"),
        region: NormRect {
            x: 0.0,
            y: 0.0,
            w: 0.1,
            h: 0.1,
        },
        score,
        threshold,
        passed,
        error: None,
    }
}

fn ce_result() -> SupportCeVerificationResult {
    SupportCeVerificationResult {
        score: 0.0,
        passed: false,
        threshold: 0.0,
        full_gate_score: 0.0,
        full_gate_threshold: 0.80,
        full_gate_passed: true,
        error: None,
        icon_checks: vec![],
        artwork_checks: vec![],
    }
}

fn mismatch(
    label: &str,
    effective_threshold: f64,
    result: SupportCeVerificationResult,
) -> SupportCeMismatch {
    mismatch_from_ce_result(label, &result, effective_threshold).expect("expected mismatch")
}

fn assert_reason_and_debug(
    mismatch: &SupportCeMismatch,
    expected_reason: &str,
    expected_debug_fragments: &[&str],
) {
    assert_eq!(mismatch.reason(), expected_reason);
    let debug = mismatch
        .debug_summary()
        .expect("debug_summary should be Some");
    for fragment in expected_debug_fragments {
        assert!(
            debug.contains(fragment),
            "debug summary should contain {fragment:?}, got {debug:?}"
        );
    }
}

#[test]
fn format_ce_verification_summary_empty_when_no_checks() {
    assert_eq!(format_ce_verification_summary(&[], &[]), "");
}

#[test]
fn format_ce_verification_summary_artwork_only() {
    let summary = format_ce_verification_summary(&[artwork_check("full", 0.85, 0.70, true)], &[]);
    assert!(summary.contains("完整匹配"), "should contain variant label");
    assert!(summary.contains("0.85"), "should contain score");
    assert!(summary.contains("0.70"), "should contain threshold");
    assert!(!summary.contains("满破"), "should not contain icon label");
}

#[test]
fn format_ce_verification_summary_includes_icon_check() {
    let summary = format_ce_verification_summary(&[], &[icon_check("mlb", 0.60, 0.75, false)]);
    assert!(summary.contains("满破图标"), "should contain mlb label");
    assert!(summary.contains("0.60"), "should contain score");
}

#[test]
fn mismatch_from_ce_result_none_when_passed() {
    let mut result = ce_result();
    result.score = 0.90;
    result.passed = true;
    assert!(mismatch_from_ce_result("礼装 1", &result, 0.70).is_none());
}

#[test]
fn mismatch_from_ce_result_plain_mismatch_reason_has_no_scores() {
    let mut result = ce_result();
    result.score = 0.55;
    result.artwork_checks = vec![artwork_check("full", 0.55, 0.70, false)];
    let mismatch = mismatch("礼装 1", 0.70, result);

    assert_reason_and_debug(&mismatch, "礼装 1 不匹配", &["完整匹配", "0.55"]);
}

#[test]
fn mismatch_from_ce_result_plain_mismatch_no_score_details_in_reason() {
    let mut result = ce_result();
    result.score = 0.55;
    result.artwork_checks = vec![artwork_check("full", 0.55, 0.70, false)];
    let mismatch = mismatch("礼装 1", 0.70, result);

    assert!(
        !mismatch.reason().contains("0.55"),
        "reason must not contain raw score"
    );
    assert!(
        !mismatch.reason().contains("完整匹配"),
        "reason must not contain variant label"
    );
}

#[test]
fn mismatch_from_ce_result_icon_mismatch_names_icon_kind() {
    let mut result = ce_result();
    result.score = 0.85;
    result.artwork_checks = vec![artwork_check("full", 0.85, 0.70, true)];
    result.icon_checks = vec![icon_check("mlb", 0.50, 0.75, false)];
    let mismatch = mismatch("礼装 2", 0.70, result);

    assert_reason_and_debug(&mismatch, "礼装 2 满破图标不匹配", &["满破图标"]);
    assert!(
        !mismatch.reason().contains("0.50"),
        "reason must not contain score"
    );
}

#[test]
fn mismatch_from_ce_result_full_gate_mismatch_reason_contains_scores_but_not_summary() {
    let mut result = ce_result();
    result.score = 0.90;
    result.artwork_checks = vec![
        artwork_check("full", 0.72, 0.80, false),
        artwork_check("center", 0.90, 0.70, true),
    ];
    result.full_gate_passed = false;
    result.full_gate_score = 0.72;
    result.full_gate_threshold = 0.80;

    let mismatch = mismatch("礼装 3", 0.70, result);

    assert!(
        mismatch.reason().contains("礼装 3 完整匹配不足"),
        "reason should start with label"
    );
    assert!(
        mismatch.reason().contains("0.72"),
        "reason should include gate score"
    );
    assert!(
        mismatch.reason().contains("0.80"),
        "reason should include gate threshold"
    );
    assert!(
        !mismatch.reason().contains("完整匹配（"),
        "reason must not contain variant summary"
    );
    assert_reason_and_debug(&mismatch, mismatch.reason(), &["完整匹配"]);
}
