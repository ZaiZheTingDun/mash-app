use std::time::{Duration, Instant};

use opentelemetry::trace::{SpanId, Status};
use opentelemetry_sdk::trace::{SdkTracerProvider, SpanData};

use super::*;
use crate::{
    runner::{support::SupportSearchTrace, BattleLoadSource, BattleState, Point},
    telemetry::{self, test_support::CompletedSpans},
};

fn capture(run: impl FnOnce()) -> Vec<SpanData> {
    let completed = CompletedSpans::default();
    let provider = SdkTracerProvider::builder()
        .with_span_processor(completed.clone())
        .build();
    tracing::subscriber::with_default(telemetry::trace_subscriber(&provider), run);
    let spans = std::mem::take(&mut *completed.0.lock().unwrap());
    spans
}

fn attr(span: &SpanData, key: &str) -> Option<String> {
    span.attributes
        .iter()
        .find(|attr| attr.key.as_str() == key)
        .map(|attr| attr.value.to_string())
}

fn quest_start(at: Instant) -> BattleFlowEvent {
    BattleFlowEvent::QuestStartTapped {
        source: BattleLoadSource::TeamConfirm,
        at,
    }
}

fn loading(screen: Screen, at: Instant, timeout_count: u32) -> BattleFlowEvent {
    BattleFlowEvent::BattleLoadObserved {
        screen,
        at,
        timeout_count,
    }
}

#[test]
fn one_battle_links_support_load_retries_and_two_turns_to_the_observed_result() {
    let run_id = uuid::Uuid::new_v4();
    let at = Instant::now();
    let spans = capture(|| {
        assert!(telemetry::sidecar_span("detect").is_disabled());
        let mut support = SupportSearchTrace::default();
        support.begin();
        support.record_quest(run_id, 3);
        drop(support);

        let mut trace = BattleTrace::new(run_id);
        let mut battle = BattleState::new();
        let step = |trace: &mut BattleTrace, battle: &mut BattleState, event| {
            trace.on_transition(battle.transition(event), 3);
        };
        step(&mut trace, &mut battle, quest_start(at));
        step(&mut trace, &mut battle, loading(Screen::Unknown, at, 10));
        // A transient result-looking frame during load cannot end the root.
        trace.observe_exit(Screen::BattleResultLoot, false);
        step(
            &mut trace,
            &mut battle,
            quest_start(at + Duration::from_secs(4)),
        );
        step(&mut trace, &mut battle, loading(Screen::Battle, at, 10));
        step(&mut trace, &mut battle, BattleFlowEvent::BattleActionable);
        trace.record_scene(Some((1, 2)));
        step(
            &mut trace,
            &mut battle,
            BattleFlowEvent::AttackButtonTapped { at },
        );
        step(
            &mut trace,
            &mut battle,
            BattleFlowEvent::AttackScreenDetected,
        );
        step(
            &mut trace,
            &mut battle,
            BattleFlowEvent::AttackCardsSubmitted { at },
        );
        trace.record_submission(&[Pick::Np {
            slot: 0,
            point: Point::new(0.5, 0.5),
            from_priority: "test".into(),
        }]);
        trace.observe_exit(Screen::Unknown, false);
        trace
            .parent_span()
            .unwrap()
            .in_scope(|| telemetry::sidecar_span("detect").in_scope(|| {}));

        trace.begin_retry();
        step(&mut trace, &mut battle, BattleFlowEvent::BattleActionable);
        step(
            &mut trace,
            &mut battle,
            BattleFlowEvent::AttackButtonTapped { at },
        );
        step(
            &mut trace,
            &mut battle,
            BattleFlowEvent::AttackScreenDetected,
        );
        step(
            &mut trace,
            &mut battle,
            BattleFlowEvent::AttackCardsSubmitted { at },
        );
        trace.record_submission(&[Pick::Card {
            slot: 1,
            point: Point::new(0.5, 0.5),
            servant_id: None,
            suit: None,
            from_priority: None,
        }]);
        trace.begin_hud_wait();
        step(
            &mut trace,
            &mut battle,
            BattleFlowEvent::PostAttackHudWaitStarted { at },
        );
        trace.begin_hud_wait();
        step(
            &mut trace,
            &mut battle,
            BattleFlowEvent::PostAttackHudResolved,
        );
        trace.record_scene(Some((2, 2)));

        step(
            &mut trace,
            &mut battle,
            BattleFlowEvent::AttackButtonTapped { at },
        );
        step(
            &mut trace,
            &mut battle,
            BattleFlowEvent::AttackScreenDetected,
        );
        step(
            &mut trace,
            &mut battle,
            BattleFlowEvent::AttackCardsSubmitted { at },
        );
        trace.observe_exit(Screen::BattleResultBond, false);
        trace.observe_exit(Screen::BattleResultLoot, false);
        // A late Battle label over the same settlement must not open another trace.
        trace.begin(3, "resumed");
    });

    let roots: Vec<_> = spans
        .iter()
        .filter(|span| span.parent_span_id == SpanId::INVALID)
        .collect();
    assert_eq!(roots.len(), 2);
    let support = roots
        .iter()
        .find(|span| span.name == "support.search")
        .unwrap();
    let root = roots.iter().find(|span| span.name == "battle.run").unwrap();
    assert_eq!(attr(root, "run_id"), Some(run_id.to_string()));
    assert_eq!(attr(root, "run_id"), attr(support, "run_id"));
    assert_eq!(attr(root, "quest_index"), Some("3".into()));
    assert_eq!(attr(root, "quest_index"), attr(support, "quest_index"));
    assert_eq!(attr(root, "outcome"), Some("result_reached".into()));
    assert_eq!(attr(root, "reason"), Some("BattleResultBond".into()));

    let load: Vec<_> = spans
        .iter()
        .filter(|span| span.name == "battle.load")
        .collect();
    assert_eq!(load.len(), 1);
    assert_eq!(attr(load[0], "tap_attempts"), Some("2".into()));
    assert_eq!(load[0].parent_span_id, root.span_context.span_id());
    let turns: Vec<_> = spans
        .iter()
        .filter(|span| span.name == "battle.turn")
        .collect();
    assert_eq!(turns.len(), 2);
    assert!(turns
        .iter()
        .all(|turn| turn.parent_span_id == root.span_context.span_id()));
    assert_eq!(attr(turns[0], "scene"), Some("1".into()));
    assert_eq!(attr(turns[0], "attack_retries"), Some("1".into()));
    assert_eq!(attr(turns[0], "submitted_cards"), Some("C2".into()));
    assert_eq!(attr(turns[0], "outcome"), Some("resolved".into()));
    assert_eq!(attr(turns[1], "scene"), Some("2".into()));
    assert_eq!(attr(turns[1], "outcome"), Some("result_reached".into()));
    let hud: Vec<_> = spans
        .iter()
        .filter(|span| span.name == "battle.hud_wait")
        .collect();
    assert_eq!(hud.len(), 1);
    assert_eq!(hud[0].parent_span_id, turns[0].span_context.span_id());

    let submissions: Vec<_> = spans
        .iter()
        .filter(|span| span.name == "battle.post_attack_wait")
        .collect();
    assert_eq!(submissions.len(), 3);
    assert_eq!(attr(submissions[0], "outcome"), Some("retry".into()));
    assert_eq!(attr(submissions[0], "submitted_cards"), Some("NP1".into()));
    assert_eq!(attr(submissions[1], "submitted_cards"), Some("C2".into()));
    assert_eq!(
        attr(submissions[2], "outcome"),
        Some("result_reached".into())
    );
    let ipc = spans
        .iter()
        .find(|span| span.name == "battle.sidecar")
        .unwrap();
    assert_eq!(ipc.parent_span_id, submissions[0].span_context.span_id());
    assert_eq!(attr(ipc, "command"), Some("detect".into()));
}

#[test]
fn battle_closes_on_load_timeout_cancel_failure_or_known_departure() {
    let spans = capture(|| {
        let mut trace = BattleTrace::new(uuid::Uuid::new_v4());
        let mut battle = BattleState::new();
        let at = Instant::now();
        trace.on_transition(battle.transition(quest_start(at)), 1);
        trace.on_transition(battle.transition(loading(Screen::Unknown, at, 1)), 1);
        trace.begin(2, "resumed");
        trace.observe_exit(Screen::BattleResultExp, true);
        trace.begin(3, "resumed");
        trace.observe_exit(Screen::SupportSelect, false);
        trace.begin(4, "resumed");
        trace.finish("failed");
    });
    let roots: Vec<_> = spans
        .iter()
        .filter(|span| span.name == "battle.run")
        .collect();
    let expected = ["timed_out", "cancelled", "left_screen", "failed"];
    assert_eq!(roots.len(), expected.len());
    for (root, outcome) in roots.iter().zip(expected) {
        assert_eq!(attr(root, "outcome"), Some(outcome.into()));
        assert_eq!(
            matches!(root.status, Status::Error { .. }),
            matches!(outcome, "timed_out" | "failed")
        );
    }
    assert_eq!(attr(roots[0], "reason"), Some("battle_load".into()));
    assert_eq!(attr(roots[1], "entry"), Some("resumed".into()));
    assert_eq!(attr(roots[2], "reason"), Some("SupportSelect".into()));
}
