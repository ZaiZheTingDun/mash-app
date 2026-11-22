use super::*;

#[test]
fn one_matching_unlock_label_is_enough_to_keep_a_candidate_frame() {
    assert_eq!(
        UnlockSample::from_matches(true, true),
        Some(UnlockSample::Confirmed)
    );
    for (notice, button) in [(true, false), (false, true)] {
        assert_eq!(
            UnlockSample::from_matches(notice, button),
            Some(UnlockSample::Candidate)
        );
    }
    assert_eq!(UnlockSample::from_matches(false, false), None);
}

#[test]
fn unlock_samples_keep_the_original_frame_and_avoid_repeated_candidate_copies() {
    let root = tempfile::tempdir().unwrap();
    let frame = root.path().join("interlude.png");
    let original = b"original frame";
    std::fs::write(&frame, original).unwrap();
    let mut samples = UnlockSamples::default();

    for (sample, prefix) in [
        (UnlockSample::Candidate, "candidate-"),
        (UnlockSample::Confirmed, "confirmed-"),
        (UnlockSample::Unrecognized, "unrecognized-"),
    ] {
        assert!(samples.should_save(sample));
        let saved = sample.save(root.path(), &frame).unwrap();
        assert_eq!(
            saved.parent().unwrap(),
            root.path().join("debug/interlude/cn-unlock-notices")
        );
        assert!(saved
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with(prefix));
        assert_eq!(std::fs::read(saved).unwrap(), original);
    }
    assert!(!samples.should_save(UnlockSample::Candidate));
    assert!(!samples.should_save(UnlockSample::Confirmed));
}

fn map_row(identity: InterludeIdentity) -> InterludeRow {
    InterludeRow {
        candidate_id: "map-row".into(),
        region: NormRect {
            x: 0.5,
            y: 0.2,
            w: 0.4,
            h: 0.2,
        },
        kind: InterludeRowKind::Quest,
        identity,
        restricted: false,
        actionable: true,
        progress: Some(0),
    }
}

#[test]
fn opening_a_shortcut_waits_for_its_map_rows_before_acting_on_the_scroll_edge() {
    let target = InterludeIdentity {
        portrait: [1; 4],
        title: [2; 4],
    };
    let mut runtime = InterludeRuntime::new(InterludeWorkflowConfig::All);
    runtime.active = Some(target);
    runtime.transition(InterludePhase::LoadingMapRows);
    let mut view = InterludeRows {
        rows: Vec::new(),
        at_top: false,
        at_bottom: true,
    };

    assert!(runtime.wait_for_map_rows(&view));
    view.rows.push(map_row(target));
    assert!(!runtime.wait_for_map_rows(&view));
    view.rows.clear();
    runtime.waited = MAP_ROWS_LOAD_POLLS;
    assert!(!runtime.wait_for_map_rows(&view));
}

#[test]
fn a_late_map_target_cancels_the_fallback_return_after_two_stable_frames() {
    let target = InterludeIdentity {
        portrait: [1; 4],
        title: [2; 4],
    };
    let mut runtime = InterludeRuntime::new(InterludeWorkflowConfig::All);
    runtime.active = Some(target);
    runtime.empty_map_checked = true;
    runtime.transition(InterludePhase::ReturningToList);
    let mut view = InterludeRows {
        rows: Vec::new(),
        at_top: true,
        at_bottom: false,
    };

    assert_eq!(runtime.check_map_return(&view), MapReturnAction::Wait);
    runtime.waited = LIST_RETURN_RETRY_POLLS;
    assert_eq!(
        runtime.check_map_return(&view),
        MapReturnAction::RetryReturn
    );

    view.rows.push(map_row(target));
    assert_eq!(runtime.check_map_return(&view), MapReturnAction::Wait);
    assert_eq!(
        runtime.check_map_return(&view),
        MapReturnAction::ContinueQuest
    );
    assert!(matches!(runtime.phase, InterludePhase::Map));
    assert_eq!(runtime.active, Some(target));
    assert!(!runtime.empty_map_checked);
}

#[test]
fn a_confirmed_return_is_retried_even_when_the_previous_quest_row_is_visible() {
    let target = InterludeIdentity {
        portrait: [1; 4],
        title: [2; 4],
    };
    let mut runtime = InterludeRuntime::new(InterludeWorkflowConfig::All);
    runtime.active = Some(target);
    runtime.transition(InterludePhase::ReturningToList);
    runtime.waited = LIST_RETURN_RETRY_POLLS;
    let view = InterludeRows {
        rows: vec![map_row(target)],
        at_top: true,
        at_bottom: false,
    };
    assert_eq!(
        runtime.check_map_return(&view),
        MapReturnAction::RetryReturn
    );
}

#[test]
fn ordered_mode_takes_over_only_the_current_battle_from_battle_screens() {
    let identity = InterludeIdentity {
        portrait: [1; 4],
        title: [2; 4],
    };
    for screen in [
        Screen::SupportSelect,
        Screen::TeamConfirm,
        Screen::TeamChange,
        Screen::Battle,
        Screen::BattleResultLoot,
        Screen::BattleResultContinue,
    ] {
        let mut runtime = InterludeRuntime::new(InterludeWorkflowConfig::All);
        assert!(runtime.start_at(screen), "{screen:?}");
        assert_eq!(
            runtime.observe_battle_takeover(screen),
            Some(BattleTakeoverAction::Continue),
        );

        let mut single = InterludeRuntime::new(InterludeWorkflowConfig::Single(identity));
        assert!(!single.start_at(screen), "{screen:?}");
    }
}

#[test]
fn ordered_mode_starts_batch_selection_only_at_the_interlude_list() {
    let mut runtime = InterludeRuntime::new(InterludeWorkflowConfig::All);
    for screen in [
        Screen::InterludeMap,
        Screen::InterludeStory,
        Screen::InterludeCompletion,
        Screen::APRecovery,
        Screen::RankUpQuest,
    ] {
        assert!(!runtime.start_at(screen), "{screen:?}");
        assert_eq!(runtime.pending_start_guidance(), Some(BATCH_START_GUIDANCE));
    }

    assert!(runtime.start_at(Screen::InterludeList));
    assert!(runtime.seek_top);
    assert_eq!(runtime.observe_battle_takeover(Screen::InterludeList), None);
}

#[test]
fn takeover_ends_when_settlement_leaves_the_battle_flow() {
    for destination in [
        Screen::InterludeList,
        Screen::InterludeMap,
        Screen::InterludeStory,
        Screen::InterludeCompletion,
        Screen::SupportSelect,
    ] {
        let mut runtime = InterludeRuntime::new(InterludeWorkflowConfig::All);
        assert!(runtime.start_at(Screen::Battle));
        for screen in [Screen::Battle, Screen::BattleResultLoot, Screen::Unknown] {
            assert_eq!(
                runtime.observe_battle_takeover(screen),
                Some(BattleTakeoverAction::Continue)
            );
        }
        assert_eq!(
            runtime.observe_battle_takeover(destination),
            Some(BattleTakeoverAction::Finish)
        );
    }
}

#[test]
fn takeover_leaves_interlude_pages_for_the_player() {
    let mut runtime = InterludeRuntime::new(InterludeWorkflowConfig::All);
    assert!(runtime.start_at(Screen::TeamConfirm));
    assert_eq!(
        runtime.observe_battle_takeover(Screen::InterludeStory),
        Some(BattleTakeoverAction::GuideToList)
    );
    assert_eq!(
        runtime.observe_battle_takeover(Screen::Battle),
        Some(BattleTakeoverAction::Continue)
    );
    assert_eq!(
        runtime.observe_battle_takeover(Screen::InterludeCompletion),
        Some(BattleTakeoverAction::Finish)
    );
}

#[test]
fn starting_a_selected_quest_from_the_map_keeps_its_identity() {
    let identity = InterludeIdentity {
        portrait: [1; 4],
        title: [2; 4],
    };
    let mut runtime = InterludeRuntime::new(InterludeWorkflowConfig::Single(identity));
    assert!(runtime.start_at(Screen::InterludeMap));
    assert!(matches!(runtime.phase, InterludePhase::Map));
    assert_eq!(runtime.active, Some(identity));
    assert!(runtime.attempt.is_none());
}

#[test]
fn verifies_progress_after_battle_results_or_a_story_only_phase() {
    let mut attempt = Attempt::new(1);
    assert!(!attempt.progressed(Some(2)));
    attempt.story_seen = true;
    assert!(attempt.progressed(Some(2)));
    assert!(!attempt.progressed(Some(1)));
    attempt.battle_seen = true;
    assert!(!attempt.progressed(Some(2)));
    attempt.result_seen = true;
    assert!(attempt.progressed(Some(2)));
    assert!(!attempt.progressed(None));
}

#[test]
fn final_reward_counts_a_played_battle_even_when_the_standard_result_was_skipped() {
    let mut attempt = Attempt::new(2);
    attempt.quest_cleared = true;
    assert!(!attempt.completed_battle());
    attempt.battle_seen = true;
    assert!(attempt.completed_battle());
}

#[test]
fn unclassified_settlement_requires_confirmed_result_or_quest_clear() {
    let mut attempt = Attempt::new(0);
    assert!(!attempt.can_continue_unclassified_settlement());
    attempt.story_seen = true;
    assert!(!attempt.can_continue_unclassified_settlement());
    attempt.battle_seen = true;
    assert!(!attempt.can_continue_unclassified_settlement());
    attempt.result_seen = true;
    assert!(attempt.can_continue_unclassified_settlement());
    let mut cleared = Attempt::new(0);
    cleared.quest_cleared = true;
    assert!(cleared.can_continue_unclassified_settlement());
}

#[test]
fn cn_story_only_phase_can_handle_completion_notices_but_not_a_generic_prompt() {
    let mut attempt = Attempt::new(0);
    assert!(!attempt.can_probe_localized_completion(Server::Cn));
    attempt.story_seen = true;
    assert!(attempt.can_probe_localized_completion(Server::Cn));
    assert!(!attempt.can_continue_unclassified_settlement());
    attempt.battle_seen = true;
    assert!(!attempt.can_probe_localized_completion(Server::Cn));
    attempt.result_seen = true;
    assert!(attempt.can_probe_localized_completion(Server::Cn));
}

#[test]
fn map_match_requires_both_title_and_portrait_and_rejects_ambiguous_rows() {
    let target = InterludeIdentity {
        portrait: [30, 0, 1, 2],
        title: [235, 2, 3, 4],
    };
    let row = InterludeRow {
        candidate_id: "map-row".into(),
        region: NormRect {
            x: 0.5,
            y: 0.2,
            w: 0.4,
            h: 0.2,
        },
        kind: InterludeRowKind::Quest,
        identity: InterludeIdentity {
            title: [234, 2, 3, 4],
            ..target
        },
        restricted: false,
        actionable: true,
        progress: Some(1),
    };
    let same_servant = InterludeRow {
        identity: InterludeIdentity {
            title: [u32::MAX; 4],
            ..target
        },
        ..row.clone()
    };
    assert!(unique_target(&[same_servant], target).is_none());
    assert!(unique_target(std::slice::from_ref(&row), target).is_some());
    assert!(unique_target(&[row.clone(), row], target).is_none());
}

#[test]
fn batch_selects_the_next_available_quest_after_restricted_map_entries() {
    let mut runtime = InterludeRuntime::new(InterludeWorkflowConfig::All);
    let identity = InterludeIdentity {
        portrait: [0; 4],
        title: [0; 4],
    };
    let shortcut = InterludeRow {
        candidate_id: "visited-map".into(),
        region: NormRect {
            x: 0.5,
            y: 0.2,
            w: 0.4,
            h: 0.2,
        },
        kind: InterludeRowKind::QuickLink,
        identity,
        restricted: false,
        actionable: true,
        progress: None,
    };
    let available = InterludeRow {
        candidate_id: "next-quest".into(),
        kind: InterludeRowKind::Quest,
        identity: InterludeIdentity {
            portrait: [u32::MAX; 4],
            title: [u32::MAX; 4],
        },
        progress: Some(0),
        ..shortcut.clone()
    };
    let restricted = InterludeRow {
        candidate_id: "restricted-quest".into(),
        restricted: true,
        actionable: false,
        ..available.clone()
    };
    runtime.skipped.push(shortcut.identity);
    let rows = [shortcut, restricted, available];
    assert_eq!(
        runtime.list_candidate(&rows).unwrap().candidate_id,
        "next-quest"
    );
    assert!(runtime.start_at(Screen::InterludeList));
    runtime.start_list_scan(None);
    assert!(runtime.seek_top);
    assert!(runtime.attempt.is_none());
    assert_eq!(
        runtime.list_candidate(&rows).unwrap().candidate_id,
        "next-quest"
    );
}
