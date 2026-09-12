use super::*;
use crate::screen::ReadCraftEssenceMainTargetResult;

fn ce_cell(
    level: u32,
    cap: u32,
    rarity: u8,
    limit_breaks: u8,
    locked: bool,
    fingerprint: &str,
) -> CraftEssenceGridCell {
    CraftEssenceGridCell {
        row: 0,
        col: 0,
        region: NormRect {
            x: 0.1,
            y: 0.2,
            w: 0.09,
            h: 0.18,
        },
        level: Some(level),
        level_cap: Some(cap),
        rarity: Some(rarity),
        limit_breaks: Some(limit_breaks),
        locked,
        lock_score: if locked { 0.95 } else { 0.25 },
        level_text: format!("{level}/{cap}"),
        level_confidence: 0.8,
        art_fingerprint: fingerprint.into(),
        same_as_target: false,
        same_as_target_score: 0.0,
        valid: true,
    }
}

#[test]
fn classifies_main_status_combinations() {
    for (extra, selected, ready) in [
        (vec!["element_enhancement_new"], false, false),
        (vec![], true, false),
        (
            vec!["element_enhancement_new", "button_enhancement_ready"],
            false,
            true,
        ),
        (vec!["button_enhancement_ready"], true, true),
    ] {
        let mut keys = vec!["icon_enhancement_result", "element_enhancement_ce_stripe"];
        keys.extend(extra);
        assert_eq!(
            classify_screen(&ProbeSnapshot::from_keys(&keys)),
            Screen::Main {
                target_selected: selected,
                ready,
            }
        );
    }
}

#[test]
fn reconciles_material_counter_before_selecting_or_committing() {
    assert_eq!(
        material_counter_decision(0, 0, false),
        MaterialCounterDecision::Confirmed
    );
    assert_eq!(
        material_counter_decision(0, 17, false),
        MaterialCounterDecision::ClearAutomaticSelection
    );
    assert_eq!(
        material_counter_decision(5, 5, false),
        MaterialCounterDecision::Confirmed
    );
    assert_eq!(
        material_counter_decision(7, 6, true),
        MaterialCounterDecision::AcceptLevelMax
    );
    assert_eq!(
        material_counter_decision(7, 6, false),
        MaterialCounterDecision::Mismatch
    );
    assert_eq!(
        material_counter_decision(5, 17, true),
        MaterialCounterDecision::Mismatch
    );
}

#[test]
fn level_max_requires_both_ocr_fragments_and_an_allowed_stage() {
    assert!(material_level_max_text_detected(
        "等级达到\n等级达到\n最大值\n最大值"
    ));
    assert!(!material_level_max_text_detected("等级1/15\n等级提升"));
    assert!(!material_accepts_level_max(StrategyStage::PacketSelected));
    assert!(material_accepts_level_max(
        StrategyStage::BombSelectedForFeed
    ));
    assert!(!material_accepts_level_max(StrategyStage::BombBaseSelected));
}

#[test]
fn distinguishes_target_material_and_dialog_states() {
    assert_eq!(
        classify_screen(&ProbeSnapshot::from_keys(&[
            "button_enhancement_ce_select_ce_mark",
            "button_enhancement_ce_lock_mode_active",
        ])),
        Screen::CraftEssenceLockMode
    );
    assert_eq!(
        classify_screen(&ProbeSnapshot::from_keys(&[
            "button_enhancement_ce_select_ce_mark"
        ])),
        Screen::CraftEssenceSelect { descending: false }
    );
    assert_eq!(
        classify_screen(&ProbeSnapshot::from_keys(&[
            "button_enhancement_ce_select_ce_mark",
            "button_enhancement_ce_clean_all_select"
        ])),
        Screen::MaterialSelect
    );
    assert_eq!(
        classify_screen(&ProbeSnapshot::from_keys(&[
            "button_enhancement_ce_select_ce_mark",
            "dialog_enhancement_ce_filter",
            "button_enhancement_ce_filter_init"
        ])),
        Screen::FilterDialog
    );
    assert_eq!(
        classify_screen(&ProbeSnapshot::from_keys(&[
            "button_enhancement_ce_select_ce_mark",
            "dialog_enhancement_ce_order"
        ])),
        Screen::OrderDialog
    );
    assert_eq!(
        classify_screen(&ProbeSnapshot::from_keys(&[
            "icon_enhancement_result",
            "element_enhancement_ce_stripe",
            "dialog_enhancement_ce_recommend_material",
            "dialog_enhancement_ce_recommend_empty"
        ])),
        Screen::RecommendMaterialEmptyDialog
    );
    assert_eq!(
        classify_screen(&ProbeSnapshot::from_keys(&[
            "icon_enhancement_result",
            "element_enhancement_ce_stripe",
            "dialog_enhancement_ce_recommend_material"
        ])),
        Screen::RecommendMaterialDialog
    );
    assert_eq!(
        classify_screen(&ProbeSnapshot::from_keys(&[
            "icon_enhancement_result",
            "element_enhancement_ce_stripe",
            "button_enhancement_ready",
            "dialog_enhancement_ce_confirm",
            "dialog_enhancement_ce_enhanced_material_warning"
        ])),
        Screen::EnhancedMaterialWarningDialog
    );
    assert_eq!(
        classify_screen(&ProbeSnapshot::from_keys(&[
            "icon_enhancement_result",
            "element_enhancement_ce_stripe",
            "button_enhancement_ready",
            "dialog_enhancement_ce_confirm"
        ])),
        Screen::EnhancementConfirmDialog
    );
    assert_eq!(
        classify_screen(&ProbeSnapshot::from_keys(&[
            "icon_enhancement_result",
            "element_enhancement_ce_stripe",
            "dialog_enhancement_ce_confirm_compact"
        ])),
        Screen::EnhancementConfirmDialog
    );
    assert_eq!(
        classify_screen(&ProbeSnapshot::from_keys(&[
            "icon_enhancement_result",
            "element_enhancement_ce_stripe",
            "text_exp_overflow",
            "element_enhancement_ce_success"
        ])),
        Screen::ExpOverflowDialog
    );
    assert_eq!(
        classify_screen(&ProbeSnapshot::from_keys(&[
            "icon_enhancement_result",
            "element_enhancement_ce_stripe",
            "element_enhancement_ce_success"
        ])),
        Screen::EnhancementSuccess
    );
}

#[test]
fn lifecycle_covers_start_finish_stop_and_failure() {
    let running = lifecycle_transition(
        CraftEssenceEnhancementRunnerState::Starting,
        LifecycleEvent::WorkerStarted,
    );
    assert_eq!(running, CraftEssenceEnhancementRunnerState::Running);
    assert_eq!(
        lifecycle_transition(running.clone(), LifecycleEvent::Finished),
        CraftEssenceEnhancementRunnerState::Finished
    );
    assert_eq!(
        lifecycle_transition(running, LifecycleEvent::StopRequested),
        CraftEssenceEnhancementRunnerState::Idle
    );
    assert_eq!(
        lifecycle_transition(
            CraftEssenceEnhancementRunnerState::Idle,
            LifecycleEvent::Failed {
                message: "boom".into()
            }
        ),
        CraftEssenceEnhancementRunnerState::Error {
            message: "boom".into()
        }
    );
}

#[test]
fn only_cn_server_is_supported() {
    assert!(server_supported(Server::Cn));
    assert!(!server_supported(Server::Jp));
}

#[test]
fn automation_modes_deserialize_to_the_two_bomb_strategies() {
    assert_eq!(
        serde_json::from_str::<CraftEssenceEnhancementMode>("\"qpEfficient\"").unwrap(),
        CraftEssenceEnhancementMode::QpEfficient
    );
    assert_eq!(
        serde_json::from_str::<CraftEssenceEnhancementMode>("\"fast\"").unwrap(),
        CraftEssenceEnhancementMode::Fast
    );
    assert!(serde_json::from_str::<CraftEssenceEnhancementMode>("\"feedBombs\"").is_err());
    assert_eq!(
        stage_after_bomb_selection(CraftEssenceEnhancementMode::QpEfficient),
        StrategyStage::BombSelected
    );
    assert_eq!(
        stage_after_bomb_selection(CraftEssenceEnhancementMode::Fast),
        StrategyStage::FastAutoFeedPending
    );
    assert_eq!(
        stage_after_missing_bomb(),
        StrategyStage::SelectBombBase,
        "both strategies must create a new max-limit-break base when no bomb exists"
    );
    assert_eq!(
        recommend_profile_for_mode(CraftEssenceEnhancementMode::Fast),
        RecommendMaterialProfile::OneAndTwoStar
    );
}

#[test]
fn bomb_policy_uses_first_safe_candidate_from_game_sorted_order() {
    let cells = vec![
        ce_cell(44, 50, 1, 4, true, "bomb-a"),
        ce_cell(49, 50, 1, 4, false, "unlocked"),
        ce_cell(11, 55, 2, 4, true, "two-star"),
        ce_cell(46, 50, 1, 4, true, "bomb-b"),
        ce_cell(50, 50, 1, 4, true, "finished"),
    ];

    let selected = choose_incomplete_bomb(&cells).unwrap();
    assert_eq!(selected.level, Some(44));
    assert_eq!(selected.art_fingerprint, "bomb-a");
    assert!(!is_incomplete_locked_bomb(&cells[1]));
    assert!(!is_incomplete_locked_bomb(&cells[2]));
    assert!(!is_incomplete_locked_bomb(&cells[4]));
    assert!(is_complete_locked_bomb(&cells[4]));
}

#[test]
fn packet_policy_requires_two_unlocked_one_star_base_copies() {
    let cells = vec![
        ce_cell(1, 10, 1, 0, false, "single"),
        ce_cell(1, 10, 1, 0, false, "pair"),
        ce_cell(1, 10, 1, 0, false, "pair"),
        ce_cell(1, 15, 2, 0, false, "two-star"),
        ce_cell(1, 10, 1, 0, true, "locked-pair"),
        ce_cell(1, 10, 1, 0, false, "locked-pair"),
    ];

    let selected = choose_packet_base(&cells).unwrap();
    assert_eq!(selected.art_fingerprint, "pair");
    assert!(is_raw_food(&cells[0]));
    assert!(is_raw_food(&cells[3]));
    assert!(!is_raw_food(&cells[4]));
    assert!(is_packet(&ce_cell(8, 20, 1, 1, false, "packet")));
    assert!(is_packet(&ce_cell(13, 50, 1, 4, false, "packet-mlb")));
    assert!(is_packet(&ce_cell(11, 30, 1, 2, false, "packet-two-break")));
    assert!(!is_packet(&ce_cell(8, 20, 1, 1, true, "locked")));
    assert!(!is_packet(&ce_cell(10, 10, 1, 0, false, "raw")));
}

#[test]
fn bomb_base_policy_requires_five_unlocked_same_art_copies() {
    let near_fingerprints = [
        "353555d353535656",
        "3535555353535656",
        "353555535b535656",
        "3535555353535656",
        "353555d353535656",
    ];
    let mut cells = near_fingerprints[..4]
        .iter()
        .map(|fingerprint| ce_cell(1, 10, 1, 0, false, fingerprint))
        .collect::<Vec<_>>();
    assert!(choose_bomb_base(&cells).is_none());

    cells.push(ce_cell(1, 10, 1, 0, false, near_fingerprints[4]));
    let selected = choose_bomb_base(&cells).unwrap();
    assert!(same_art_fingerprint(
        &selected.art_fingerprint,
        near_fingerprints[2]
    ));
    assert!(!same_art_fingerprint(
        &selected.art_fingerprint,
        "ffffffffffffffff"
    ));

    let mut locked = ce_cell(1, 10, 1, 0, true, "base");
    locked.row = 2;
    locked.col = 3;
    assert!(!is_verified_locked_bomb_base(&locked, Some(2), Some(3)));
    let mut finished = ce_cell(6, 50, 1, 4, true, "base");
    finished.row = 2;
    finished.col = 3;
    assert!(is_verified_locked_bomb_base(&finished, Some(2), Some(3)));
    assert!(!is_verified_locked_bomb_base(&finished, Some(2), Some(4)));
}

#[test]
fn packet_material_plan_selects_exactly_one_same_copy_and_nothing_else() {
    let mut same = ce_cell(1, 10, 1, 0, false, "packet");
    same.same_as_target = true;
    same.same_as_target_score = 0.06;
    let other_one = ce_cell(1, 10, 1, 0, false, "other-one");
    let other_two = ce_cell(1, 15, 2, 0, false, "other-two");
    let locked = ce_cell(1, 10, 1, 0, true, "locked");

    let without_same = [&other_one, &other_two, &locked];
    let planned = plan_packet_materials(&without_same, false);
    assert!(planned.is_empty());

    let with_same = [&other_one, &same, &other_two, &locked];
    let planned = plan_packet_materials(&with_same, false);
    assert_eq!(
        planned
            .iter()
            .map(|cell| cell.art_fingerprint.as_str())
            .collect::<Vec<_>>(),
        vec!["packet"]
    );
    assert!(plan_packet_materials(&with_same, true).is_empty());
}

#[test]
fn same_copy_selection_is_confirmed_only_after_the_page_counter_matches() {
    let mut pending = true;
    let mut selected = false;
    assert!(!confirm_pending_same_copy(
        MaterialCounterDecision::Mismatch,
        &mut pending,
        &mut selected,
    ));
    assert!(pending);
    assert!(!selected);

    assert!(confirm_pending_same_copy(
        MaterialCounterDecision::Confirmed,
        &mut pending,
        &mut selected,
    ));
    assert!(!pending);
    assert!(selected);

    let mut automatic_pending = true;
    let mut automatic_selected = false;
    assert!(!confirm_pending_same_copy(
        MaterialCounterDecision::ClearAutomaticSelection,
        &mut automatic_pending,
        &mut automatic_selected,
    ));
    assert!(!automatic_pending);
    assert!(!automatic_selected);
}

#[test]
fn material_commit_waits_for_the_same_copy_counter_confirmation_frame() {
    assert!(!material_selection_ready_to_commit(1, 1, true));
    assert!(material_selection_ready_to_commit(1, 1, false));
    assert!(!material_selection_ready_to_commit(0, 1, false));
    assert!(same_copy_counter_update_pending(true, 1, 0));
    assert!(!same_copy_counter_update_pending(true, 1, 1));
    assert!(!same_copy_counter_update_pending(false, 1, 0));
}

#[test]
fn packet_feed_plan_matches_the_created_fingerprint_multiset() {
    let cells = vec![
        ce_cell(20, 20, 1, 1, false, "packet-a"),
        ce_cell(18, 20, 1, 1, false, "packet-a"),
        ce_cell(20, 20, 1, 1, false, "unrelated"),
        ce_cell(20, 20, 1, 1, true, "packet-b"),
        ce_cell(19, 20, 1, 1, false, "packet-b"),
    ];
    let wanted = vec![
        "packet-a".to_string(),
        "packet-a".to_string(),
        "packet-b".to_string(),
    ];

    let cell_refs = cells.iter().collect::<Vec<_>>();
    let planned = plan_packet_feed(&cell_refs, &wanted);
    assert_eq!(
        planned
            .iter()
            .map(|cell| cell.art_fingerprint.as_str())
            .collect::<Vec<_>>(),
        vec!["packet-a", "packet-a", "packet-b"]
    );
}

#[test]
fn inventory_packet_feed_accepts_only_unlocked_upgraded_one_star_cards() {
    let cells = vec![
        ce_cell(15, 20, 1, 1, false, "one-break"),
        ce_cell(12, 30, 1, 2, false, "two-break"),
        ce_cell(1, 10, 1, 0, false, "raw"),
        ce_cell(15, 20, 1, 1, true, "locked"),
        ce_cell(15, 25, 2, 1, false, "two-star"),
    ];
    let refs = cells.iter().collect::<Vec<_>>();
    let planned = plan_inventory_packet_feed(&refs);

    assert_eq!(
        planned
            .iter()
            .map(|cell| cell.art_fingerprint.as_str())
            .collect::<Vec<_>>(),
        vec!["one-break", "two-break"]
    );
    assert!(inventory_feed_ready_at_bottom(true, 2, true));
    assert!(!inventory_feed_ready_at_bottom(true, 0, true));
    assert!(!inventory_feed_ready_at_bottom(false, 2, true));
    assert_eq!(
        packet_base_exhaustion_action(0),
        PacketBaseExhaustionAction::ReturnToCurrentBomb
    );
    assert_eq!(
        packet_base_exhaustion_action(1),
        PacketBaseExhaustionAction::ReselectBomb
    );
    assert!(target_selection_descending(StrategyStage::SelectBomb));
    assert!(target_selection_descending(
        StrategyStage::SelectBombForTransfer
    ));
    assert!(!target_selection_descending(
        StrategyStage::SelectPacketBase
    ));
    assert!(!material_selection_descending(
        StrategyStage::BombSelectedForFeed
    ));
    assert_eq!(
        material_batch_click_limit(StrategyStage::PacketSelected, 0, 1),
        1
    );
    assert_eq!(
        material_batch_click_limit(StrategyStage::BombSelectedForFeed, 0, 20),
        20
    );
    assert_eq!(
        material_batch_click_limit(StrategyStage::BombSelectedForFeed, 14, 20),
        6
    );
}

#[test]
fn scrollbar_reset_requires_a_detected_top_position() {
    assert_eq!(
        scrollbar_reset_drag_y(Some(0.35), Some(0.276), 21, 21, 0),
        Ok(None)
    );
    assert_eq!(
        scrollbar_reset_drag_y(Some(0.43), Some(0.31), 21, 21, 0),
        Ok(Some(0.43))
    );
    assert_eq!(
        scrollbar_reset_drag_y(Some(0.35), None, 21, 21, 0),
        Ok(None)
    );
    assert_eq!(
        scrollbar_reset_drag_y(Some(0.93), Some(0.82), 15, 21, 0),
        Ok(Some(0.93))
    );
    assert_eq!(scrollbar_reset_drag_y(None, None, 12, 21, 0), Ok(None));
    assert!(scrollbar_reset_drag_y(None, Some(0.276), 21, 21, 0).is_err());
    assert!(scrollbar_reset_drag_y(Some(0.35), Some(1.2), 21, 21, 0).is_err());
    assert!(
        scrollbar_reset_drag_y(Some(0.93), Some(0.82), 15, 21, LIST_RESET_MAX_ATTEMPTS).is_err()
    );
}

#[test]
fn filter_scrollbar_top_threshold_is_strict() {
    assert!(filter_scrollbar_at_top(0.145));
    assert!(filter_scrollbar_at_top(FILTER_SCROLLBAR_TOP_MAX_Y));
    assert!(!filter_scrollbar_at_top(0.149));
    assert!(!filter_scrollbar_at_top(0.153));
    assert!(!filter_scrollbar_at_top(f64::NAN));
}

#[test]
fn filter_scrollbar_reset_allows_one_retry_after_the_first_drag() {
    assert_eq!(filter_scrollbar_reset_needs_drag(0.153, 0), Ok(true));
    assert_eq!(filter_scrollbar_reset_needs_drag(0.153, 1), Ok(true));
    assert!(filter_scrollbar_reset_needs_drag(0.153, 2).is_err());
    assert_eq!(filter_scrollbar_reset_needs_drag(0.148, 2), Ok(false));
}

#[test]
fn scroll_end_probe_exhausts_the_current_scan_immediately() {
    assert_eq!(next_page_scan_count(2, 12, false), 3);
    assert_eq!(next_page_scan_count(0, 12, true), 13);
    assert_eq!(next_page_scan_count(12, 12, true), 13);
}

#[test]
fn recommendation_filters_cover_qp_efficient_and_fast_profiles() {
    let two_star_targets = RECOMMEND_FILTERS
        .iter()
        .map(|filter| {
            (
                filter.label,
                filter.target_on(RecommendMaterialProfile::TwoStarOnly),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        two_star_targets,
        vec![
            ("1 星", false),
            ("2 星", true),
            ("3 星", false),
            ("4 星", false),
            ("5 星", false),
            ("未强化", true),
            ("已强化", false),
        ]
    );

    let one_star_targets = RECOMMEND_FILTERS
        .iter()
        .map(|filter| {
            (
                filter.label,
                filter.target_on(RecommendMaterialProfile::OneStarOnly),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        one_star_targets,
        vec![
            ("1 星", true),
            ("2 星", false),
            ("3 星", false),
            ("4 星", false),
            ("5 星", false),
            ("未强化", true),
            ("已强化", false),
        ]
    );

    let fast_targets = RECOMMEND_FILTERS
        .iter()
        .map(|filter| {
            (
                filter.label,
                filter.target_on(RecommendMaterialProfile::OneAndTwoStar),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        fast_targets,
        vec![
            ("1 星", true),
            ("2 星", true),
            ("3 星", false),
            ("4 星", false),
            ("5 星", false),
            ("未强化", true),
            ("已强化", false),
        ]
    );
}

#[test]
fn packet_requires_one_break_enhancement_then_exactly_one_auto_feed_enhancement() {
    assert_eq!(
        next_packet_stage_after_enhancement(StrategyStage::PacketSelected, 0),
        Some(StrategyStage::PacketAutoFeedPending)
    );
    assert_eq!(
        next_packet_stage_after_enhancement(StrategyStage::PacketAutoFeedPending, 1),
        Some(StrategyStage::SelectPacketBase)
    );
    assert_eq!(
        next_packet_stage_after_enhancement(
            StrategyStage::PacketAutoFeedPending,
            usize::from(PACKET_BATCH_SIZE)
        ),
        Some(StrategyStage::SelectBombForTransfer)
    );
    assert_eq!(
        next_packet_stage_after_enhancement(StrategyStage::SelectPacketBase, 0),
        None
    );
    assert!(recommendation_needs_execution(
        false,
        Some(RecommendMaterialProfile::TwoStarOnly),
        RecommendMaterialProfile::TwoStarOnly
    ));
    assert!(!recommendation_needs_execution(
        true,
        Some(RecommendMaterialProfile::TwoStarOnly),
        RecommendMaterialProfile::TwoStarOnly
    ));
    assert!(recommendation_needs_execution(
        true,
        Some(RecommendMaterialProfile::TwoStarOnly),
        RecommendMaterialProfile::OneStarOnly
    ));
}

#[test]
fn enhancement_return_requires_leaving_main_before_accepting_main_again() {
    let main = Screen::Main {
        target_selected: true,
        ready: true,
    };
    assert_eq!(
        enhancement_return_action(main, false),
        EnhancementReturnAction::TapSkip {
            mark_left_main: false
        }
    );
    assert_eq!(
        enhancement_return_action(Screen::Unknown, false),
        EnhancementReturnAction::TapSkip {
            mark_left_main: true
        }
    );
    assert_eq!(
        enhancement_return_action(Screen::EnhancementSuccess, true),
        EnhancementReturnAction::TapSkip {
            mark_left_main: true
        }
    );
    assert_eq!(
        enhancement_return_action(Screen::ExpOverflowDialog, false),
        EnhancementReturnAction::CloseExpOverflow
    );
    assert_eq!(
        enhancement_return_action(main, true),
        EnhancementReturnAction::ObserveReturnedMain
    );
    assert_eq!(
        enhancement_return_action(Screen::EnhancementConfirmDialog, false),
        EnhancementReturnAction::WaitForConfirmationClose
    );
    assert_eq!(
        enhancement_return_action(Screen::RecommendMaterialDialog, true),
        EnhancementReturnAction::Unexpected
    );
    assert!(!enhancement_main_return_confirmed(1));
    assert!(enhancement_main_return_confirmed(2));
}

#[test]
fn post_enhancement_caps_cover_only_committed_stages() {
    assert_eq!(
        expected_post_enhancement_cap(StrategyStage::BombBaseSelected),
        Some(50)
    );
    assert_eq!(
        expected_post_enhancement_cap(StrategyStage::PacketSelected),
        Some(20)
    );
    assert_eq!(
        expected_post_enhancement_cap(StrategyStage::PacketAutoFeedPending),
        Some(20)
    );
    assert_eq!(
        expected_post_enhancement_cap(StrategyStage::BombSelectedForFeed),
        Some(50)
    );
    assert_eq!(
        expected_post_enhancement_cap(StrategyStage::FastAutoFeedPending),
        Some(50)
    );
    assert_eq!(
        expected_post_enhancement_cap(StrategyStage::SelectPacketBase),
        None
    );
    assert!(enhancement_confirm_can_arm(
        StrategyStage::PacketSelected,
        true
    ));
    assert!(enhancement_confirm_can_arm(
        StrategyStage::PacketAutoFeedPending,
        true
    ));
    assert!(enhancement_confirm_can_arm(
        StrategyStage::FastAutoFeedPending,
        true
    ));
    assert!(!enhancement_confirm_can_arm(
        StrategyStage::PacketSelected,
        false
    ));
    assert!(!enhancement_confirm_can_arm(
        StrategyStage::SelectPacketBase,
        true
    ));
}

#[test]
fn enhancement_completion_consumes_pending_stage_exactly_once() {
    let target = ReadCraftEssenceMainTargetResult {
        found: true,
        level: Some(12),
        level_cap: Some(20),
        text: "等级12/20".into(),
        region: None,
    };
    let mut pending = Some(StrategyStage::PacketSelected);
    assert_eq!(
        complete_pending_enhancement(&mut pending, &target),
        Ok(StrategyStage::PacketSelected)
    );
    assert_eq!(pending, None);
    assert_eq!(
        complete_pending_enhancement(&mut pending, &target),
        Err(EnhancementCompletionError::MissingPending)
    );
}

#[test]
fn fast_strategy_requires_a_readable_level_and_finishes_only_at_fifty() {
    let unreadable = ReadCraftEssenceMainTargetResult {
        found: false,
        level: None,
        level_cap: Some(50),
        text: "/50".into(),
        region: None,
    };
    let mut pending = Some(StrategyStage::FastAutoFeedPending);
    assert_eq!(
        complete_pending_enhancement(&mut pending, &unreadable),
        Err(EnhancementCompletionError::TargetUnreadable)
    );
    assert_eq!(pending, Some(StrategyStage::FastAutoFeedPending));

    let level_49 = ReadCraftEssenceMainTargetResult {
        found: true,
        level: Some(49),
        level_cap: Some(50),
        text: "等级49/50".into(),
        region: None,
    };
    let mut pending = Some(StrategyStage::FastAutoFeedPending);
    assert_eq!(
        complete_pending_enhancement(&mut pending, &level_49),
        Ok(StrategyStage::FastAutoFeedPending)
    );
    assert!(!fast_bomb_is_complete(&level_49));

    let level_50 = ReadCraftEssenceMainTargetResult {
        found: true,
        level: Some(50),
        level_cap: Some(50),
        text: "等级50/50".into(),
        region: None,
    };
    assert!(fast_bomb_is_complete(&level_50));
}

#[test]
fn packet_enhancement_completion_requires_level_cap_twenty() {
    let cap_ten = ReadCraftEssenceMainTargetResult {
        found: true,
        level: Some(10),
        level_cap: Some(10),
        text: "等级10/10".into(),
        region: None,
    };
    let cap_twenty = ReadCraftEssenceMainTargetResult {
        found: false,
        level: None,
        level_cap: Some(20),
        text: "31 120".into(),
        region: None,
    };
    let mut pending = Some(StrategyStage::PacketSelected);
    assert_eq!(
        complete_pending_enhancement(&mut pending, &cap_ten),
        Err(EnhancementCompletionError::CapMismatch {
            expected: 20,
            actual: Some(10),
        })
    );
    assert_eq!(pending, Some(StrategyStage::PacketSelected));
    let unreadable = ReadCraftEssenceMainTargetResult {
        found: false,
        level: None,
        level_cap: None,
        text: "garbled".into(),
        region: None,
    };
    assert_eq!(
        complete_pending_enhancement(&mut pending, &unreadable),
        Err(EnhancementCompletionError::TargetUnreadable)
    );
    assert_eq!(pending, Some(StrategyStage::PacketSelected));
    assert_eq!(
        complete_pending_enhancement(&mut pending, &cap_twenty),
        Ok(StrategyStage::PacketSelected)
    );
    assert_eq!(pending, None);

    for cap in [20, 30, 40, 50] {
        let auto_result = ReadCraftEssenceMainTargetResult {
            found: true,
            level: Some(13),
            level_cap: Some(cap),
            text: format!("等级13/{cap}"),
            region: None,
        };
        let mut auto_pending = Some(StrategyStage::PacketAutoFeedPending);
        assert_eq!(
            complete_pending_enhancement(&mut auto_pending, &auto_result),
            Ok(StrategyStage::PacketAutoFeedPending)
        );
        assert_eq!(auto_pending, None);
    }

    let mut invalid_auto_pending = Some(StrategyStage::PacketAutoFeedPending);
    assert_eq!(
        complete_pending_enhancement(&mut invalid_auto_pending, &cap_ten),
        Err(EnhancementCompletionError::CapMismatch {
            expected: 20,
            actual: Some(10),
        })
    );
}

#[test]
fn residual_success_without_pending_commit_is_ignored() {
    assert_eq!(
        unawaited_success_action(None),
        UnawaitedSuccessAction::IgnoreResidual
    );
    assert_eq!(
        unawaited_success_action(Some(StrategyStage::PacketSelected)),
        UnawaitedSuccessAction::ResumePending
    );
}

#[test]
fn binary_controls_retry_only_clear_opposite_states_and_fail_ambiguously() {
    assert_eq!(
        decide_binary_control(0.96, 0.82),
        BinaryControlDecision::TargetConfirmed
    );
    assert_eq!(
        decide_binary_control(0.81, 0.97),
        BinaryControlDecision::Toggle
    );
    assert_eq!(
        decide_binary_control(0.93, 0.92),
        BinaryControlDecision::Ambiguous
    );
    assert_eq!(
        decide_binary_control(0.70, 0.69),
        BinaryControlDecision::Ambiguous
    );
}

#[test]
fn density_allows_three_toggles_before_failing() {
    assert_eq!(density_decision(true, 0), DensityDecision::Confirmed);
    for taps_done in 0..3 {
        assert_eq!(density_decision(false, taps_done), DensityDecision::Toggle);
    }
    assert_eq!(density_decision(false, 3), DensityDecision::Failed);
}

#[test]
fn filter_toggle_color_separates_blue_off_from_white_on() {
    assert_eq!(classify_filter_toggle_luma(106.8), FilterToggleState::Off);
    assert_eq!(classify_filter_toggle_luma(216.0), FilterToggleState::On);
    assert_eq!(
        classify_filter_toggle_luma(160.0),
        FilterToggleState::Ambiguous
    );
}

#[test]
fn auto_config_color_requires_a_clear_off_or_on_saturation() {
    assert_eq!(classify_auto_config_saturation(40.0), AutoConfigState::Off);
    assert_eq!(classify_auto_config_saturation(140.0), AutoConfigState::On);
    assert_eq!(
        classify_auto_config_saturation(90.0),
        AutoConfigState::Ambiguous
    );
}

#[test]
fn enhancement_button_requires_shape_before_using_luma_state() {
    assert_eq!(
        classify_enhancement_button(0.585, 88.0),
        EnhancementReadyState::Absent
    );
    assert_eq!(
        classify_enhancement_button(0.929, 102.0),
        EnhancementReadyState::NotReady
    );
    assert_eq!(
        classify_enhancement_button(0.978, 165.0),
        EnhancementReadyState::Ready
    );
    assert_eq!(
        classify_enhancement_button(0.94, 135.0),
        EnhancementReadyState::Transitioning
    );
    assert_eq!(
        classify_enhancement_button(0.85, 165.0),
        EnhancementReadyState::Absent
    );
}
