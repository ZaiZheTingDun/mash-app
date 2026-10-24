use super::*;

fn project() -> Project {
    serde_json::from_value(serde_json::json!({"id": "project", "name": "测试"})).unwrap()
}

fn issue() -> TeamCheckIssue {
    TeamCheckIssue {
        slot_index: Some(2),
        kind: TeamCheckKind::Servant,
        expected_name: "测试从者".into(),
        reason: "未匹配到预期对象".into(),
    }
}

#[test]
fn empty_and_support_slots_skip_the_entire_slot() {
    let mut project = project();
    project.slots[0].craft_essence_id = Some(1);
    project.slots[2].servant_id = Some(1);
    project.slots[2].craft_essence_id = Some(1);
    assert!(build_team_check_targets(&project, None, None)
        .unwrap()
        .is_empty());
    project.slots[4].servant_id = Some(1);
    let targets = build_team_check_targets(&project, None, None).unwrap();
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].slot_index, 4);
    assert_eq!(targets[0].kind, TeamCheckKind::Servant);
    assert!(targets[0].template_paths.is_empty());
}

#[test]
fn only_grand_members_ce_is_excluded() {
    let mut project = project();
    project.advanced_mode = true;
    for index in [0, 4] {
        project.slots[index].servant_id = Some(1);
        project.slots[index].craft_essence_id = Some(1);
    }
    project.grand_servants = serde_json::from_value(serde_json::json!([{
        "memberId": project.slots[4].id, "slotIndex": 0, "servantId": 1, "isSupport": false
    }]))
    .unwrap();
    // Stable member identity wins over stale position, never servant id alone.
    let targets = build_team_check_targets(&project, None, None).unwrap();
    assert_eq!(
        targets
            .iter()
            .map(|t| (t.slot_index, t.kind.clone()))
            .collect::<Vec<_>>(),
        vec![
            (0, TeamCheckKind::Servant),
            (0, TeamCheckKind::CraftEssence),
            (4, TeamCheckKind::Servant)
        ]
    );
    project.grand_servants[0].member_id = None;
    project.grand_servants[0].slot_index = 4;
    assert_eq!(
        build_team_check_targets(&project, None, None)
            .unwrap()
            .len(),
        3
    );
    project.advanced_mode = false;
    assert_eq!(
        build_team_check_targets(&project, None, None)
            .unwrap()
            .len(),
        4
    );
}

#[test]
fn accepts_all_ascensions_and_costumes_and_resolves_configured_ce() {
    let dir = tempfile::tempdir().unwrap();
    let servant_dir = dir.path().join("1");
    std::fs::create_dir(&servant_dir).unwrap();
    for name in [
        "narrow_servant_1.png",
        "narrow_servant_2.png",
        "narrow_servant_800150.png",
        "face_servant_1.png",
        "card_ce.png",
    ] {
        std::fs::write(servant_dir.join(name), []).unwrap();
    }
    let mut project = project();
    project.slots[0].servant_id = Some(1);
    project.slots[0].craft_essence_id = Some(1);
    let targets = build_team_check_targets(&project, Some(dir.path()), Some(dir.path())).unwrap();
    assert_eq!(targets[0].template_paths.len(), 3);
    assert_eq!(
        targets[1].template_paths,
        vec![servant_dir.join("card_ce.png")]
    );
}

#[test]
fn decisions_are_scoped_to_run_and_check_and_consumed_once() {
    let mut control = TeamVerificationControl::default();
    let report = control.block(vec![issue(), issue()]);
    assert_eq!(report.issues.len(), 2);
    assert!(control
        .resolve("old-run", report.check_id, TeamVerificationDecision::Skip)
        .is_err());
    assert!(control
        .resolve(
            &report.run_id,
            report.check_id + 1,
            TeamVerificationDecision::Retry
        )
        .is_err());
    control
        .resolve(
            &report.run_id,
            report.check_id,
            TeamVerificationDecision::Retry,
        )
        .unwrap();
    assert!(control
        .resolve(
            &report.run_id,
            report.check_id,
            TeamVerificationDecision::Skip
        )
        .is_err());
    assert!(matches!(
        control.take_decision(),
        Some(TeamVerificationDecision::Retry)
    ));
    assert!(!control.skipped);
    assert!(control.report.is_none());
    assert!(control.take_decision().is_none());
    let next = control.block(vec![issue()]);
    assert!(control
        .resolve(
            &report.run_id,
            report.check_id,
            TeamVerificationDecision::Skip
        )
        .is_err());
    control
        .resolve(&next.run_id, next.check_id, TeamVerificationDecision::Skip)
        .unwrap();
    control.take_decision();
    assert!(control.skipped); // The same control survives repeated quests.
    assert!(!TeamVerificationControl::default().skipped);
}

#[test]
fn pause_retains_live_state_until_retry_skip_or_stop() {
    let paused = runner_lifecycle_transition(
        RunnerState::Running,
        RunnerLifecycleEvent::VerificationBlocked,
    );
    assert_eq!(paused.next, RunnerState::Paused);
    assert_eq!(paused.next.status(), "paused");
    assert_eq!(
        runner_lifecycle_transition(
            paused.next.clone(),
            RunnerLifecycleEvent::VerificationResolved
        )
        .next,
        RunnerState::Running
    );
    assert_eq!(
        runner_lifecycle_transition(paused.next, RunnerLifecycleEvent::StopRequested).next,
        RunnerState::Idle
    );
}

#[test]
fn resume_resets_load_wait_without_resetting_start_attempts() {
    let start = Instant::now();
    let mut battle = BattleState::new();
    battle.transition(BattleFlowEvent::QuestStartTapped {
        source: BattleLoadSource::TeamConfirm,
        at: start,
    });
    battle.observe_battle_load(Screen::Unknown, start, 3);
    battle.observe_battle_load(Screen::Unknown, start, 3);
    let resumed = start + Duration::from_secs(100);
    battle.resume_after_team_verification(resumed);
    assert_eq!(
        battle.observe_battle_load(Screen::TeamConfirm, resumed, 3),
        BattleLoadGate::Waiting
    );
    assert_eq!(
        battle.observe_battle_load(Screen::TeamConfirm, resumed + Duration::from_secs(3), 10),
        BattleLoadGate::Waiting
    );
    assert_eq!(
        battle.observe_battle_load(Screen::TeamConfirm, resumed + Duration::from_secs(4), 10),
        BattleLoadGate::RetryStart { attempt: 2 }
    );
}

#[test]
fn camel_case_report_aligns_frontend_and_sidecar() {
    let mut control = TeamVerificationControl::default();
    let report = control.block(vec![TeamCheckIssue {
        slot_index: None,
        kind: TeamCheckKind::MysticCode,
        expected_name: "迦勒底战斗服".into(),
        reason: "未匹配到预期御主礼装".into(),
    }]);
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["runId"], report.run_id);
    assert_eq!(json["checkId"], 1);
    assert_eq!(json["issues"][0]["slotIndex"], serde_json::Value::Null);
    assert_eq!(json["issues"][0]["kind"], "mysticCode");
    assert_eq!(json["issues"][0]["expectedName"], "迦勒底战斗服");
    let issue: TeamCheckIssue = serde_json::from_value(serde_json::json!({
        "slotIndex": 4, "kind": "craftEssence", "expectedName": "宝石翁", "reason": "未匹配到预期对象"
    })).unwrap();
    assert_eq!(issue.kind, TeamCheckKind::CraftEssence);
    assert_eq!(issue.slot_index, Some(4));
    assert!(matches!(
        serde_json::from_str::<TeamVerificationDecision>("\"skip\"").unwrap(),
        TeamVerificationDecision::Skip
    ));
}

#[test]
fn failure_evidence_keeps_exact_checked_frame_and_overwrites_previous_record() {
    let directory = tempfile::tempdir().unwrap();
    let frame = directory.path().join("checked.png");
    let output = directory.path().join("evidence");
    std::fs::write(&frame, b"the frame before the dialog").unwrap();
    let evidence = serde_json::json!({"recognition": {"checks": [{"score": 0.74, "threshold": 0.8, "bestTemplate": "narrow_servant_1.png"}]}});
    save_team_check_evidence(&output, &frame, &evidence).unwrap();
    assert_eq!(
        std::fs::read(output.join("last.png")).unwrap(),
        b"the frame before the dialog"
    );
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output.join("last.json")).unwrap()).unwrap();
    assert_eq!(saved, evidence);
    std::fs::write(&frame, b"the next failed check").unwrap();
    save_team_check_evidence(&output, &frame, &serde_json::json!({"issues": []})).unwrap();
    assert_eq!(
        std::fs::read(output.join("last.png")).unwrap(),
        b"the next failed check"
    );
    assert_eq!(std::fs::read_dir(output).unwrap().count(), 2);
}

#[test]
fn unloaded_artwork_gets_fresh_attempts_and_continues_as_soon_as_ready() {
    let start = Instant::now();
    let mut wait = TeamArtworkLoadWait::default();
    assert_eq!(wait.observe(true, start), TeamArtworkLoadGate::Waiting);
    assert_eq!(
        wait.observe(true, start + Duration::from_millis(300)),
        TeamArtworkLoadGate::Waiting
    );
    assert_eq!(
        wait.observe(false, start + Duration::from_millis(600)),
        TeamArtworkLoadGate::Ready
    );
    // A successful check resets the grace window for the next entry.
    assert_eq!(
        wait.observe(true, start + Duration::from_secs(20)),
        TeamArtworkLoadGate::Waiting
    );
}

#[test]
fn persistent_mismatch_pauses_after_a_bounded_wait_without_lowering_thresholds() {
    let start = Instant::now();
    let mut wait = TeamArtworkLoadWait::default();
    assert_eq!(wait.observe(true, start), TeamArtworkLoadGate::Waiting);
    assert_eq!(
        wait.observe(true, start + Duration::from_millis(2999)),
        TeamArtworkLoadGate::Waiting
    );
    assert_eq!(
        wait.observe(true, start + Duration::from_secs(3)),
        TeamArtworkLoadGate::Blocked
    );
    assert_eq!(
        wait.observe(true, start + Duration::from_secs(30)),
        TeamArtworkLoadGate::Blocked
    );
    // Matching at the deadline still passes; the timeout never overrides evidence.
    assert_eq!(
        wait.observe(false, start + Duration::from_secs(30)),
        TeamArtworkLoadGate::Ready
    );
    // The user's Retry gets a new bounded waiting window.
    let mut retried = TeamArtworkLoadWait::default();
    assert_eq!(
        retried.observe(true, start + Duration::from_secs(60)),
        TeamArtworkLoadGate::Waiting
    );
}
