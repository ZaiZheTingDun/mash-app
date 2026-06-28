//! Battle progress state and scene-transition helpers.
//!
//! This module owns the pure state transitions that decide when scene config
//! should run and when post-attack HUD reads can advance normal-mode turns.

use super::*;

// ---------------------------------------------------------------------------
// Battle state
// ---------------------------------------------------------------------------

pub(crate) struct BattleState {
    /// Which battle-scene config index we're executing (0-based into the
    /// `scenes` vec). One config block = one battle scene as labeled in
    /// the HUD's `BATTLE m/n`.
    pub(crate) current_scene_index: usize,
    /// 0-based turn counter inside the current normal-mode battle scene.
    /// It resets when the HUD moves to a new `BATTLE m/n` scene and
    /// increments when a submitted attack resolves back to an actionable
    /// Battle screen.
    pub(crate) current_turn_index: usize,
    /// `m` value last successfully detected from the BATTLE m/n HUD strip.
    /// Stays `Some(prev)` across transient failed reads (e.g. NP overlay
    /// briefly covers the strip) so we don't double-trigger skill
    /// execution when the strip reappears.
    pub(crate) last_screen_scene: Option<u32>,
    /// `current_scene_index` value we last executed skills for. We
    /// re-run the configured skills exactly once per index value, so
    /// transient failed reads (`scene_m == None`) never cause duplicate
    /// execution — only an actual `Some(prev) → Some(curr != prev)`
    /// transition advances the index and triggers a re-execution.
    pub(crate) executed_scene_index: Option<usize>,
    /// Normal-mode turn config that has already had preparation actions
    /// executed. Advanced mode continues to use `executed_scene_index`.
    pub(crate) executed_turn_key: Option<(usize, usize)>,
    /// Whether we used the scene config (vs fallback) — drives card selection
    pub(crate) scene_config_used: bool,
    /// Set after clicking start on TeamConfirm; tolerates longer Unknown streaks
    pub(crate) waiting_for_battle: bool,
    /// Set after tapping the selected command cards. The attack-card screen
    /// can remain detectable for a short moment before the animation takes
    /// over; this prevents submitting another set of picks in that window.
    pub(crate) attack_submitted: bool,
    /// Start time for the normal-mode post-attack HUD-read grace period.
    /// While this is set, the runner has returned to an actionable Battle
    /// screen but is waiting for `BATTLE m/n` before advancing a turn.
    pub(crate) post_attack_hud_wait_started: Option<Instant>,
    /// Consecutive command-card owner recognition failures across the current
    /// battle. A successful front-line-only read resets this before we
    /// permanently fall back to scanning the full six-member party.
    pub(crate) command_card_owner_failure_count: u32,
    /// Once owner recognition has failed three times in a row, keep scanning
    /// against the full party for the rest of the current battle because a
    /// back-line servant has likely rotated into the front line.
    pub(crate) command_card_owner_fallback_to_full_party: bool,
    pub(crate) advanced_startup_done: HashSet<usize>,
    pub(crate) advanced_control_indices: HashMap<usize, usize>,
    pub(crate) advanced_startup_control_indices: HashMap<usize, usize>,
    pub(crate) advanced_auto_order_changes: HashMap<usize, Action>,
}

impl BattleState {
    pub(crate) fn new() -> Self {
        Self {
            current_scene_index: 0,
            current_turn_index: 0,
            last_screen_scene: None,
            executed_scene_index: None,
            executed_turn_key: None,
            scene_config_used: false,
            waiting_for_battle: false,
            attack_submitted: false,
            post_attack_hud_wait_started: None,
            command_card_owner_failure_count: 0,
            command_card_owner_fallback_to_full_party: false,
            advanced_startup_done: HashSet::new(),
            advanced_control_indices: HashMap::new(),
            advanced_startup_control_indices: HashMap::new(),
            advanced_auto_order_changes: HashMap::new(),
        }
    }
}

/// Outcome of merging a fresh `BATTLE m/n` reading into the prior scene
/// state. Returned by [`tick_scene_state`] so the decision logic
/// (advance? lock in? re-execute?) is testable independently of the
/// runner's I/O side effects (taps, sidecar IPC, emitted events).
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) struct SceneTick {
    /// New value for `BattleState::last_screen_scene`. Preserves the
    /// prior `Some(prev)` across transient `None` reads.
    pub(crate) last_screen_scene: Option<u32>,
    /// New value for `BattleState::current_scene_index`. Increments
    /// only on an actual `Some(prev) → Some(curr != prev)` transition.
    pub(crate) current_scene_index: usize,
    /// True iff the configured skills for `current_scene_index` should
    /// be executed this iteration. False on every iteration where the
    /// caller has already executed for the same index value, including
    /// when the latest CV read failed and we're sitting on a previously
    /// locked-in scene.
    pub(crate) needs_exec: bool,
}

pub(crate) fn tick_scene_state(
    last_screen_scene: Option<u32>,
    current_scene_index: usize,
    executed_scene_index: Option<usize>,
    scene_m: Option<u32>,
) -> SceneTick {
    // Three update paths for `current_scene_index`:
    //
    // 1. First successful read (no prior `last_screen_scene`): snap the
    //    index to `scene_m - 1` so the runner aligns with whatever
    //    scene the screen is actually on. This handles the user
    //    starting the runner mid-quest (e.g. screen already shows 2/3
    //    on the first poll) — without the snap we would execute
    //    config block 0 for the actual scene 2 and only advance on the
    //    *next* observed transition.
    // 2. Subsequent transition (`prev → curr` with both Some and
    //    different): increment the index by 1, mirroring the on-screen
    //    advance.
    // 3. Anything else (failed read, same `m` re-read, no read yet):
    //    leave the index alone.
    let next_index = match (last_screen_scene, scene_m) {
        (None, Some(curr)) => curr.saturating_sub(1) as usize,
        (Some(prev), Some(curr)) if prev != curr => current_scene_index + 1,
        _ => current_scene_index,
    };
    let next_last = if scene_m.is_some() {
        scene_m
    } else {
        last_screen_scene
    };
    SceneTick {
        last_screen_scene: next_last,
        current_scene_index: next_index,
        needs_exec: executed_scene_index != Some(next_index),
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PostAttackHudReadGate {
    Ready,
    Waiting { started_at: Instant },
    TimedOut,
}

pub(crate) fn post_attack_hud_read_gate(
    advanced_mode: bool,
    attack_returned_after_submit: bool,
    screen_scene: Option<(u32, u32)>,
    wait_started: Option<Instant>,
    now: Instant,
    timeout: Duration,
) -> PostAttackHudReadGate {
    if advanced_mode || !attack_returned_after_submit || screen_scene.is_some() {
        return PostAttackHudReadGate::Ready;
    }

    let started_at = wait_started.unwrap_or(now);
    if now.duration_since(started_at) >= timeout {
        PostAttackHudReadGate::TimedOut
    } else {
        PostAttackHudReadGate::Waiting { started_at }
    }
}
