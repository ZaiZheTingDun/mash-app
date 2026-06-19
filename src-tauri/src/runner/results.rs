//! Battle-result screen constants and handlers.
//!
//! This module owns the post-battle settlement flow: dismissing bond/EXP/loot
//! pages, skipping friend requests, and deciding whether to repeat a quest.

use super::*;

// ---------------------------------------------------------------------------
// Battle-result tap targets. Each post-battle page has a single forward
// button; constants are kept here so the debug page (and future overlay)
// can introspect them without crawling the match arm.
// ---------------------------------------------------------------------------

/// "Next" arrow on the bond-points result page.
pub(crate) const BATTLE_RESULT_BOND_NEXT: Point = Point::new(0.041, 0.945);
/// "Next" arrow on the EXP-gain result page (same physical button as bond).
pub(crate) const BATTLE_RESULT_EXP_NEXT: Point = Point::new(0.041, 0.945);
/// "Next" button on the loot/drops summary page.
pub(crate) const BATTLE_RESULT_LOOT_NEXT: Point = Point::new(0.874, 0.890);
/// "Skip / Close" on the optional friend-request prompt that appears
/// after using a non-friend support.
pub(crate) const BATTLE_RESULT_FRIEND_SKIP: Point = Point::new(0.254, 0.854);
/// "Continue / Repeat" button on the final continue page — taps this when
/// `RunConfig::repeat_mission` is true.
pub(crate) const BATTLE_RESULT_CONTINUE_REPEAT: Point = Point::new(0.657, 0.809);
/// "Close / Stop" button on the final continue page — taps this when
/// `RunConfig::repeat_mission` is false. The runner finishes after.
pub(crate) const BATTLE_RESULT_CONTINUE_STOP: Point = Point::new(0.348, 0.809);
/// Generic skip/close target for transient battle-result popups that can
/// obscure the settlement page and make screen detection return Unknown.
/// Same physical position as the battle animation-skip button, but kept as
/// a separate semantic constant so result-popup behavior can be tuned alone.
pub(crate) const BATTLE_RESULT_POPUP_SKIP: Point = Point::new(0.685, 0.095);

/// Cadence used by `tap_until_screen_changes` when dismissing post-battle
/// result pages. Slow enough for the device to register each tap and for
/// `detect()` to read a fresh frame, fast enough that a 3–5 s bond /
/// EXP animation only absorbs a few wasted taps before the page actually
/// transitions.
pub(crate) const BATTLE_RESULT_TAP_INTERVAL: Duration = Duration::from_millis(300);
/// Hard ceiling for how long any single result page is allowed to absorb
/// taps before the runner emits a timeout warning. Comfortably above the
/// longest measured bond / EXP animation (~5 s for a multi-servant
/// level-up cascade).
pub(crate) const BATTLE_RESULT_TAP_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) fn is_battle_result_screen(screen: Screen) -> bool {
    matches!(
        screen,
        Screen::BattleResultBond
            | Screen::BattleResultExp
            | Screen::BattleResultLoot
            | Screen::BattleResultFriendRequest
            | Screen::BattleResultContinue
    )
}

impl Runner {
    pub(crate) fn handle_battle_result_bond(&mut self) {
        self.emit("BattleResultBond", "羁绊点数结算，前往下一画面");
        self.tap_until_screen_changes(
            "BattleResultBond",
            Screen::BattleResultBond,
            BATTLE_RESULT_BOND_NEXT,
            BATTLE_RESULT_TAP_INTERVAL,
            BATTLE_RESULT_TAP_TIMEOUT,
        );
    }

    pub(crate) fn handle_battle_result_exp(&mut self) {
        self.emit("BattleResultExp", "经验结算，前往下一画面");
        self.tap_until_screen_changes(
            "BattleResultExp",
            Screen::BattleResultExp,
            BATTLE_RESULT_EXP_NEXT,
            BATTLE_RESULT_TAP_INTERVAL,
            BATTLE_RESULT_TAP_TIMEOUT,
        );
    }

    pub(crate) fn handle_battle_result_loot(&mut self) {
        self.emit("BattleResultLoot", "掉落结算，前往下一画面");
        if self.tap_at("BattleResultLoot", BATTLE_RESULT_LOOT_NEXT) {
            thread::sleep(ACTION_DELAY);
        }
    }

    pub(crate) fn handle_battle_result_friend_request(&mut self) {
        self.emit("BattleResultFriendRequest", "跳过好友申请");
        if self.tap_at("BattleResultFriendRequest", BATTLE_RESULT_FRIEND_SKIP) {
            thread::sleep(ACTION_DELAY);
        }
    }

    /// Final continue page. Branches on `RunConfig::repeat_mission`:
    ///
    /// * `true` — tap "Next" so FGO re-queues the same quest. Per-run
    ///   bookkeeping (team-change flag, battle turn counters, placed-
    ///   servant set) is reset so the next loop reuses pre-battle handlers
    ///   from a clean slate. The pinned support metadata is intentionally
    ///   kept since the same servant is still desired.
    /// * `false` — tap "Close" and transition to `Finished`. The main
    ///   loop's post-handler check exits cleanly so the user sees the
    ///   "已完成" toast.
    pub(crate) fn handle_battle_result_continue(&mut self) {
        if self.battle_result_continue_handled {
            self.emit("BattleResultContinue", "等待结算页切换…");
            thread::sleep(ACTION_DELAY);
            return;
        }
        self.battle_result_continue_handled = true;

        self.completed_mission_runs += 1;
        let reached_run_cap = self
            .config
            .max_mission_runs
            .is_some_and(|max| self.completed_mission_runs >= max);
        let should_repeat = self.config.max_mission_runs.is_some() || self.config.repeat_mission;

        if should_repeat && !reached_run_cap && !self.should_stop_after_current() {
            self.emit("BattleResultContinue", "继续重复任务");
            if !self.tap_at("BattleResultContinue", BATTLE_RESULT_CONTINUE_REPEAT) {
                return;
            }
            self.team_changed = false;
            self.servants_placed.clear();
            self.battle = BattleState::new();
            self.battle.waiting_for_battle = true;
            thread::sleep(ACTION_DELAY);
        } else {
            self.emit(
                "BattleResultContinue",
                &format!("结束任务（已完成 {} 轮）", self.completed_mission_runs),
            );
            if !self.tap_at("BattleResultContinue", BATTLE_RESULT_CONTINUE_STOP) {
                return;
            }
            self.set_state(RunnerState::Finished);
        }
    }
}
