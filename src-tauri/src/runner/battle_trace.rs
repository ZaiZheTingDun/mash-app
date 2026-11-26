//! Development telemetry for the Runner's accepted battle transitions.

use super::{
    is_battle_result_screen, BattleFlowEvent, BattleFlowState, BattleFlowTransition,
    BattleLoadGate, Pick, Screen,
};
use crate::telemetry::BATTLE_TARGET;
use tracing::{field, info_span, Span};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Load,
    AttackScreen,
    PostAttack,
    Hud,
}

pub(super) struct BattleTrace {
    run_id: uuid::Uuid,
    quest_index: u32,
    reached_result: bool,
    battle: Option<Span>,
    turn: Option<TurnTrace>,
    phase: Option<(Phase, Span)>,
    turn_index: u32,
    start_taps: u32,
}

struct TurnTrace {
    span: Span,
    scene: Option<(u32, u32)>,
    submitted_cards: Option<String>,
    attack_retries: u32,
}

impl BattleTrace {
    pub(super) fn new(run_id: uuid::Uuid) -> Self {
        Self {
            run_id,
            quest_index: 0,
            reached_result: false,
            battle: None,
            turn: None,
            phase: None,
            turn_index: 0,
            start_taps: 0,
        }
    }

    pub(super) fn run_id(&self) -> uuid::Uuid {
        self.run_id
    }

    pub(super) fn is_active(&self) -> bool {
        self.battle.is_some()
    }

    pub(super) fn begin(&mut self, quest_index: u32, entry: &'static str) {
        if self.is_active()
            || (entry == "resumed" && self.quest_index == quest_index && self.reached_result)
        {
            return;
        }
        let span = info_span!(
            target: BATTLE_TARGET, parent: None, "battle.run",
            run_id = %self.run_id, quest_index, entry,
            outcome = field::Empty, reason = field::Empty, otel.status_code = field::Empty,
        );
        if !span.is_disabled() {
            self.quest_index = quest_index;
            self.reached_result = false;
            self.battle = Some(span);
            self.turn_index = 0;
            self.start_taps = 0;
        }
    }

    pub(super) fn parent_span(&self) -> Option<Span> {
        let battle = self.battle.as_ref()?;
        let current = Span::current();
        if current
            .metadata()
            .is_some_and(|meta| meta.target() == BATTLE_TARGET)
        {
            return Some(current);
        }
        Some(
            self.phase
                .as_ref()
                .map(|(_, span)| span)
                .or(self.turn.as_ref().map(|turn| &turn.span))
                .unwrap_or(battle)
                .clone(),
        )
    }

    pub(super) fn observe_exit(&mut self, screen: Screen, cancelled: bool) {
        // The load gate decides which observations can end the loading phase.
        if !self.is_active()
            || self
                .phase
                .as_ref()
                .is_some_and(|(phase, _)| *phase == Phase::Load)
        {
            return;
        }
        if matches!(screen, Screen::Battle | Screen::Attack | Screen::Unknown) {
            return;
        }
        let outcome = if cancelled {
            "cancelled"
        } else if is_battle_result_screen(screen) {
            self.reached_result = true;
            "result_reached"
        } else {
            "left_screen"
        };
        if let Some(span) = &self.battle {
            span.record("reason", field::display(screen));
        }
        self.finish(outcome);
    }

    pub(super) fn on_transition(&mut self, transition: BattleFlowTransition, quest_index: u32) {
        if !transition.accepted {
            return;
        }
        if matches!(transition.event, BattleFlowEvent::QuestStartTapped { .. }) {
            self.begin(quest_index, "quest_start");
        }
        if !self.is_active() {
            return;
        }
        match transition.event {
            BattleFlowEvent::QuestStartTapped { .. } => {
                self.start_taps += 1;
                self.start_phase(Phase::Load);
            }
            BattleFlowEvent::BattleLoadObserved { .. } => match transition.battle_load_gate {
                Some(BattleLoadGate::TimedOut) => {
                    self.finish_with_reason("timed_out", "battle_load")
                }
                Some(BattleLoadGate::StartRetriesExhausted) => {
                    self.finish_with_reason("failed", "start_retries_exhausted");
                }
                Some(BattleLoadGate::NotWaiting)
                    if transition.next == BattleFlowState::PreBattle =>
                {
                    self.finish_with_reason("left_screen", "ap_recovery");
                }
                Some(BattleLoadGate::NotWaiting) => self.end_phase("ready"),
                _ => {}
            },
            BattleFlowEvent::BattleActionable => self.begin_turn(),
            BattleFlowEvent::AttackButtonTapped { .. } => self.start_phase(Phase::AttackScreen),
            BattleFlowEvent::AttackScreenDetected => {
                if self
                    .phase
                    .as_ref()
                    .is_some_and(|(phase, _)| *phase == Phase::AttackScreen)
                {
                    self.end_phase("ready");
                }
                self.begin_turn();
            }
            BattleFlowEvent::AttackScreenWaitTimedOut => self.end_phase("timed_out"),
            BattleFlowEvent::AttackCardsSubmitted { .. } => self.start_phase(Phase::PostAttack),
            BattleFlowEvent::PostAttackHudResolved => {
                self.end_turn("resolved");
                self.begin_turn();
            }
            BattleFlowEvent::PostAttackHudWaitStarted { .. }
            | BattleFlowEvent::BattleLoadRetryDeferred => {}
        }
    }

    pub(super) fn begin_hud_wait(&mut self) {
        self.start_phase(Phase::Hud);
    }

    pub(super) fn hud_timed_out(&mut self) {
        self.end_phase("timed_out");
    }

    pub(super) fn record_scene(&mut self, scene: Option<(u32, u32)>) {
        if let (Some(turn), Some(scene)) = (&mut self.turn, scene) {
            turn.scene = Some(scene);
        }
    }

    pub(super) fn record_submission(&mut self, picks: &[Pick]) {
        let Some((Phase::PostAttack, phase)) = &self.phase else {
            return;
        };
        let cards: Vec<_> = picks
            .iter()
            .map(|pick| match pick {
                Pick::Card { slot, .. } => format!("C{}", slot + 1),
                Pick::Np { slot, .. } => format!("NP{}", slot + 1),
            })
            .collect();
        let cards = cards.join(",");
        phase.record("submitted_cards", cards.as_str());
        if let Some(turn) = &mut self.turn {
            turn.submitted_cards = Some(cards);
        }
    }

    pub(super) fn begin_retry(&mut self) {
        if let Some(turn) = &mut self.turn {
            turn.attack_retries += 1;
            self.end_phase("retry");
        }
    }

    pub(super) fn finish_with_reason(&mut self, outcome: &'static str, reason: &'static str) {
        if let Some(span) = &self.battle {
            span.record("reason", reason);
        }
        self.finish(outcome);
    }

    pub(super) fn finish(&mut self, outcome: &'static str) {
        self.end_turn(outcome);
        if let Some(span) = self.battle.take() {
            span.record("outcome", outcome);
            if matches!(outcome, "failed" | "timed_out") {
                span.record("otel.status_code", "ERROR");
            }
        }
    }

    fn begin_turn(&mut self) {
        if self.turn.is_some() {
            return;
        }
        if let Some(parent) = &self.battle {
            self.turn_index += 1;
            self.turn = Some(TurnTrace {
                span: info_span!(
                    target: BATTLE_TARGET, parent: parent, "battle.turn",
                    turn_index = self.turn_index, scene = field::Empty, scene_total = field::Empty,
                    submitted_cards = field::Empty, attack_retries = field::Empty, outcome = field::Empty,
                ),
                scene: None,
                submitted_cards: None,
                attack_retries: 0,
            });
        }
    }

    fn end_turn(&mut self, outcome: &'static str) {
        self.end_phase(outcome);
        if let Some(turn) = self.turn.take() {
            if let Some((scene, total)) = turn.scene {
                turn.span.record("scene", scene);
                turn.span.record("scene_total", total);
            }
            if let Some(cards) = turn.submitted_cards {
                turn.span.record("submitted_cards", cards.as_str());
            }
            turn.span.record("attack_retries", turn.attack_retries);
            turn.span.record("outcome", outcome);
        }
    }

    fn start_phase(&mut self, phase: Phase) {
        if !self.is_active()
            || self
                .phase
                .as_ref()
                .is_some_and(|(current, _)| phase == *current)
        {
            return;
        }
        self.end_phase("ready");
        let Some(parent) = self
            .turn
            .as_ref()
            .map(|turn| &turn.span)
            .or(self.battle.as_ref())
        else {
            return;
        };
        let span = match phase {
            Phase::Load => info_span!(target: BATTLE_TARGET, parent: parent, "battle.load",
                outcome = field::Empty, tap_attempts = field::Empty),
            Phase::AttackScreen => {
                info_span!(target: BATTLE_TARGET, parent: parent, "battle.attack_screen_wait",
                outcome = field::Empty)
            }
            Phase::PostAttack => {
                info_span!(target: BATTLE_TARGET, parent: parent, "battle.post_attack_wait",
                outcome = field::Empty, submitted_cards = field::Empty)
            }
            Phase::Hud => info_span!(target: BATTLE_TARGET, parent: parent, "battle.hud_wait",
                outcome = field::Empty),
        };
        self.phase = Some((phase, span));
    }

    fn end_phase(&mut self, outcome: &'static str) {
        if let Some((phase, span)) = self.phase.take() {
            if phase == Phase::Load {
                span.record("tap_attempts", self.start_taps);
            }
            span.record("outcome", outcome);
        }
    }
}

impl Drop for BattleTrace {
    fn drop(&mut self) {
        self.finish(if std::thread::panicking() {
            "failed"
        } else {
            "interrupted"
        });
    }
}

#[cfg(test)]
mod tests;
