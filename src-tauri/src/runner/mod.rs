//! Battle automation engine and runtime helpers.
//! Public runner DTOs live in submodules; this module keeps the high-level loop.

mod actions;
mod ap_recovery;
mod attack;
mod config;
mod coords;
mod grand;
mod party;
mod prebattle;
mod results;
mod state;
mod support;

pub(crate) use actions::*;
#[cfg(test)]
pub(crate) use ap_recovery::*;
pub(crate) use attack::*;
pub use config::*;
use config::{default_grand_chain_priority, GrandServantRuntimeConfig};
pub(crate) use coords::*;
pub(crate) use grand::*;
pub(crate) use party::*;
pub(crate) use results::*;
pub(crate) use state::*;
pub(crate) use support::*;

use crate::adb::Adb;
use crate::screen::{
    CommandCardMatch, NoblePhantasmMatch, NormRect, Point, Screen, SidecarClient,
    SupportCeArtworkCheck, SupportCeVerificationOptions, SupportRowMatch,
};
use crate::touch::{self, TouchBackend};
use crate::{
    load_servant_metadata, servant_np_card, Action, AdvancedBattleScene,
    AdvancedCommandCardCondition, AdvancedOutputType, AdvancedRule, AttackCard, BattleScene,
    BattleTurn, ServantMetadata, Server,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};
use tauri::Emitter;

const UNKNOWN_TIMEOUT: u32 = 10;
/// Tolerated streak of `Unknown` screens while a long animation / loading
/// transition is playing -- raised from the default so a stacked NP chain
/// (which can run 30s+ of cut-ins before the battle screen reappears)
/// doesn't trip the "无法识别当前画面" error. 75 * 800ms = 60s.
const UNKNOWN_TIMEOUT_LOADING: u32 = 75;

/// Poll cadence for `wait_for_attack_button` while a skill animation
/// (cut-in, NP charge effect, etc.) is hiding the attack button.
const SKILL_POLL_INTERVAL: Duration = Duration::from_millis(300);
/// Hard cap on how long we'll wait for the attack button to come back
/// after a skill. Some skills trigger long buff cut-ins or animations
/// (and a few skills push an NP-charge cut-in on top), so this needs
/// to cover NP-length animations without hanging forever if something
/// genuinely went wrong.
const SKILL_WAIT_TIMEOUT: Duration = Duration::from_secs(45);
/// After a submitted attack resolves back to Battle, wait briefly for a
/// reliable `BATTLE m/n` HUD read before advancing the normal-mode turn
/// counter. If CV keeps failing, fall back to the legacy turn-advance logic
/// so automation does not stall forever.
const POST_ATTACK_HUD_READ_TIMEOUT: Duration = Duration::from_secs(3);
const COMMAND_CARD_COUNT: usize = 5;
/// Maximum per-axis jitter (in physical pixels) added to every tap so
/// repeated runs don't land on identical coordinates. Small enough to
/// stay well inside button hit-boxes; large enough that the noise is
/// distinguishable from a deterministic script.
const TAP_JITTER_PX: i32 = 6;

const DEFAULT_W: u32 = 1080;
const DEFAULT_H: u32 = 1920;

pub struct Runner {
    sidecar: Option<SidecarClient>,
    sidecar_cache: Option<Arc<Mutex<Option<SidecarClient>>>>,
    config: RunConfig,
    scenes: Vec<BattleScene>,
    advanced_mode: bool,
    advanced_scenes: Vec<AdvancedBattleScene>,
    state: Arc<Mutex<RunnerState>>,
    cancel: Arc<AtomicBool>,
    stop_after_current: Arc<AtomicBool>,
    app_handle: tauri::AppHandle,
    screen_w: u32,
    screen_h: u32,
    /// Per-servant face assets (`{id}/card_servant_*.png`). When ``None`` the
    /// sidecar can still report suit + slot but cannot identify which
    /// servant owns each command card, which means priority entries can't
    /// be matched and we fall through to the leftmost-fill path.
    assets_dir: Option<PathBuf>,
    /// Per-CE icon assets (`{ce_id}/card_ce.png`). Used by
    /// `handle_support_select` to verify a row's equipped CE matches the
    /// pinned support CE. `None` means we couldn't locate the dir on
    /// disk; CE verification is silently skipped in that case so the
    /// existing flow (pick first OCR match) still works.
    ce_assets_dir: Option<PathBuf>,
    /// Game server this run targets (JP/CN). Forwarded to
    /// `load_servant_metadata` so the cached `support_meta.name` /
    /// `np_names` come out in the right language for the OCR model the
    /// sidecar is using.
    server: Server,
    // Pre-battle progress tracking
    team_changed: bool,
    support_selected: bool,
    support_scroll_count: u32,
    /// How many times we've tapped the friend-list refresh button this run.
    /// Reset alongside `support_scroll_count` once a match is selected.
    support_refresh_count: u32,
    /// True once we've tapped the class-filter tab corresponding to the
    /// pinned servant's class. Reset on refresh (the refresh sometimes
    /// snaps the UI back to "all") so we always re-confirm the filter
    /// after a friend-list reload.
    support_class_tab_done: bool,
    /// Cached `(name, np_names, class_name)` for the pinned support
    /// servant. Loaded lazily on the first `handle_support_select` poll so
    /// we don't do disk I/O at 500ms cadence (and cleared between runs
    /// because each run owns its own `Runner`).
    support_meta: Option<ServantMetadata>,
    /// Cached resolved path to the pinned support CE template, computed
    /// once on the first poll where `support_craft_essence_id` is set.
    /// Outer `Option` is "have we tried to resolve yet"; inner `Option`
    /// is "did it succeed" (`None` = template missing / no CE pinned →
    /// skip verification).
    support_ce_template: Option<Option<PathBuf>>,
    support_grand_ce_templates: Option<[Option<PathBuf>; 3]>,
    /// Whether the current refreshed support list has ever shown the Grand
    /// avatar-frame probe. A missing probe before this flips true is not
    /// enough to conclude the Grand section is exhausted, because first-page
    /// template probes can be transiently stale while the list settles.
    support_grand_section_seen: bool,
    /// Consecutive Grand avatar-frame misses after the section was seen.
    support_grand_section_misses: u8,
    /// Tracks visible skill-panel validation for the current support row.
    /// Owned and append skills are shown on alternating panels, so a row can
    /// only satisfy both groups across multiple OCR polls.
    support_level_progress: SupportLevelPanelProgress,
    servants_placed: Vec<u32>,
    // Battle progress tracking
    battle: BattleState,
    completed_mission_runs: u32,
    battle_result_continue_handled: bool,
    /// Pluggable touch-injection backend (see `touch::TouchBackend`).
    /// Currently always `adb-input`; the trait indirection is kept so
    /// faster transports can be added later without changing the
    /// runner's call sites. The runner just calls `tap` / `swipe` /
    /// `swipe_with_settle` against the trait.
    touch: Box<dyn TouchBackend>,
}

impl Runner {
    pub fn new(
        adb: Adb,
        sidecar: SidecarClient,
        config: RunConfig,
        scenes: Vec<BattleScene>,
        advanced_mode: bool,
        advanced_scenes: Vec<AdvancedBattleScene>,
        app_handle: tauri::AppHandle,
        state: Arc<Mutex<RunnerState>>,
        cancel: Arc<AtomicBool>,
        stop_after_current: Arc<AtomicBool>,
        screen_size: Option<(u32, u32)>,
        assets_dir: Option<PathBuf>,
        ce_assets_dir: Option<PathBuf>,
        server: Server,
        sidecar_cache: Option<Arc<Mutex<Option<SidecarClient>>>>,
    ) -> Self {
        let (screen_w, screen_h) = screen_size.unwrap_or((DEFAULT_W, DEFAULT_H));
        let touch = build_touch_backend(&adb);
        Self {
            touch,
            sidecar: Some(sidecar),
            sidecar_cache,
            config,
            scenes,
            advanced_mode,
            advanced_scenes,
            state,
            cancel,
            stop_after_current,
            app_handle,
            screen_w,
            screen_h,
            assets_dir,
            ce_assets_dir,
            server,
            team_changed: false,
            support_selected: false,
            support_scroll_count: 0,
            support_refresh_count: 0,
            support_class_tab_done: false,
            support_meta: None,
            support_ce_template: None,
            support_grand_ce_templates: None,
            support_grand_section_seen: false,
            support_grand_section_misses: 0,
            support_level_progress: SupportLevelPanelProgress::default(),
            servants_placed: Vec::new(),
            battle: BattleState::new(),
            completed_mission_runs: 0,
            battle_result_continue_handled: false,
        }
    }

    // -- helpers -------------------------------------------------------------

    fn set_state(&self, s: RunnerState) {
        *self.state.lock().unwrap() = s;
    }

    fn emit(&self, screen: &str, message: &str) {
        self.emit_with_level(screen, message, LogLevel::Info);
    }

    /// Like [`emit`] but at `LogLevel::Debug`. Use for technical
    /// diagnostics that the user doesn't normally want to see — they
    /// stay hidden behind the operation-log "显示调试" toggle in the
    /// status bar. Keep these messages compact (a single line) since
    /// the panel doesn't wrap long entries gracefully.
    fn emit_debug(&self, screen: &str, message: &str) {
        self.emit_with_level(screen, message, LogLevel::Debug);
    }

    fn emit_with_level(&self, screen: &str, message: &str, level: LogLevel) {
        self.emit_with_level_and_attack(screen, message, level, None);
    }

    fn emit_attack(&self, message: &str, attack: AttackLogMeta) {
        self.emit_with_level_and_attack("Attack", message, LogLevel::Info, Some(attack));
    }

    fn emit_with_level_and_attack(
        &self,
        screen: &str,
        message: &str,
        level: LogLevel,
        attack: Option<AttackLogMeta>,
    ) {
        let state_str = {
            let s = self.state.lock().unwrap();
            format!("{:?}", *s)
        };
        let _ = self.app_handle.emit(
            "automation-status",
            AutomationEvent {
                state: state_str,
                current_screen: screen.into(),
                message: message.into(),
                level,
                attack,
            },
        );
    }

    fn sidecar(&mut self) -> &mut SidecarClient {
        self.sidecar.as_mut().expect("runner sidecar missing")
    }

    fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    fn should_stop_after_current(&self) -> bool {
        self.stop_after_current.load(Ordering::Relaxed)
    }

    fn fail_action(&self, screen: &str, action: &str, err: String) {
        let message = format!("{action}失败: {err}");
        self.set_state(RunnerState::Error {
            message: message.clone(),
        });
        self.emit(screen, &message);
    }

    fn tap_at(&mut self, screen: &str, point: Point) -> bool {
        let (px, py) = point.to_physical(self.screen_w, self.screen_h);
        let (jx, jy) = jitter_offset();
        // Saturate at the screen edges so a near-edge button still
        // registers even if the jitter would push it off-screen.
        let tap_x = (px as i32 + jx).clamp(0, self.screen_w.saturating_sub(1) as i32) as u32;
        let tap_y = (py as i32 + jy).clamp(0, self.screen_h.saturating_sub(1) as i32) as u32;
        match self.touch.tap(tap_x, tap_y) {
            Ok(()) => true,
            Err(err) => {
                self.fail_action(screen, "点击", err);
                false
            }
        }
    }

    fn swipe_at(&mut self, screen: &str, from: Point, to: Point, duration_ms: u32) -> bool {
        let from_px = from.to_physical(self.screen_w, self.screen_h);
        let to_px = to.to_physical(self.screen_w, self.screen_h);
        match self.touch.swipe(from_px, to_px, duration_ms) {
            Ok(()) => true,
            Err(err) => {
                self.fail_action(screen, "滑动", err);
                false
            }
        }
    }

    /// "Press, drag, hold, release" swipe via the active `TouchBackend`.
    /// Use this for any swipe whose precise stopping position matters
    /// — the settle window prevents Android's fling momentum from
    /// continuing the scroll past the lift-off coordinate.
    fn swipe_with_settle_at(
        &mut self,
        screen: &str,
        from: Point,
        to: Point,
        swipe_ms: u32,
        settle_ms: u32,
    ) -> bool {
        let from_px = from.to_physical(self.screen_w, self.screen_h);
        let to_px = to.to_physical(self.screen_w, self.screen_h);
        match self
            .touch
            .swipe_with_settle(from_px, to_px, swipe_ms, settle_ms)
        {
            Ok(()) => true,
            Err(err) => {
                self.fail_action(screen, "滑动", err);
                false
            }
        }
    }

    /// Tap `point` repeatedly, every `interval`, until either
    /// `sidecar.detect()` returns a screen different from `from_screen`,
    /// `timeout` elapses, or the user cancels the run.
    ///
    /// The bond and EXP result pages in particular have a ~3–5 s
    /// per-servant level-up / bond animation that absorbs the very first
    /// tap silently — the page stays mounted until the animation
    /// completes, so the legacy "one tap per main-loop poll" cadence
    /// (~one tap every 800 ms) wastes most of those taps and leaves the
    /// runner sitting on the result page much longer than necessary.
    /// Tapping inside the handler at a tighter cadence and bailing the
    /// instant the screen actually changes drops the average bond-screen
    /// dwell from ~6 s to under 2 s on common quests.
    ///
    /// Returns ``true`` when the screen changed away from `from_screen`,
    /// ``false`` on timeout, cancellation, or ADB tap failure (which is
    /// already reported via `fail_action` from `tap_at`).
    fn tap_until_screen_changes(
        &mut self,
        screen_label: &str,
        from_screen: Screen,
        point: Point,
        interval: Duration,
        timeout: Duration,
    ) -> bool {
        let start = std::time::Instant::now();
        let mut taps: u32 = 0;
        loop {
            if self.is_cancelled() {
                return false;
            }
            if !self.tap_at(screen_label, point) {
                return false;
            }
            taps += 1;
            thread::sleep(interval);
            // detect() failures are best-effort here — treat them as
            // "screen unchanged" so we keep tapping rather than bailing.
            // A persistent CV error will surface on the main loop's
            // next iteration via the same call path.
            let detected = self.sidecar().detect(None).unwrap_or(from_screen);
            if detected != from_screen {
                return true;
            }
            if start.elapsed() >= timeout {
                self.emit(
                    screen_label,
                    &format!("等待画面切换超时 (已点击 {taps} 次)"),
                );
                return false;
            }
        }
    }

    fn wait_for_element_visible(
        &mut self,
        screen: &str,
        element: &str,
        timeout: Duration,
        status_text: &str,
        timeout_text: &str,
    ) -> bool {
        self.wait_for_element_state(screen, element, true, timeout, status_text, timeout_text)
    }

    fn wait_for_element_hidden(
        &mut self,
        screen: &str,
        element: &str,
        timeout: Duration,
        status_text: &str,
        timeout_text: &str,
    ) -> bool {
        self.wait_for_element_state(screen, element, false, timeout, status_text, timeout_text)
    }

    fn wait_for_element_state(
        &mut self,
        screen: &str,
        element: &str,
        expected_found: bool,
        timeout: Duration,
        status_text: &str,
        timeout_text: &str,
    ) -> bool {
        let start = std::time::Instant::now();
        let mut tick: u32 = 0;
        loop {
            if self.is_cancelled() {
                return false;
            }
            let found = match self
                .sidecar()
                .find_element_by_name(None, BATTLE_SCREEN, element)
            {
                Ok(matched) => Some(matched.found),
                Err(err) => {
                    eprintln!(
                        "[runner] find_element_by_name({BATTLE_SCREEN}.{element}) failed: {err}"
                    );
                    None
                }
            };
            if found == Some(expected_found) {
                return true;
            }
            if start.elapsed() >= timeout {
                self.emit(screen, timeout_text);
                return false;
            }
            tick += 1;
            if tick % 4 == 1 {
                self.emit(screen, status_text);
            }
            thread::sleep(SKILL_POLL_INTERVAL);
        }
    }

    /// Block until the attack-button probe in `cv.json` matches, polling
    /// every `SKILL_POLL_INTERVAL`. Used after firing a skill so
    /// the next tap doesn't land during the cut-in / animation while the
    /// button is hidden.
    ///
    /// Returns ``true`` when the button is detected, ``false`` on timeout
    /// or cancellation. Emits status updates so the user can see the wait.
    fn wait_for_attack_button(&mut self, screen: &str, timeout: Duration) -> bool {
        self.wait_for_element_visible(
            screen,
            ATTACK_BUTTON_ELEMENT,
            timeout,
            "等待技能动画结束…",
            "等待攻击按钮超时",
        )
    }

    // -- main loop -----------------------------------------------------------

    pub fn run(mut self) {
        self.set_state(RunnerState::Running);
        self.emit("", "自动化已启动");

        let mut unknown_count: u32 = 0;
        let mut last_detected_screen = Screen::Unknown;

        loop {
            if self.is_cancelled() {
                self.set_state(RunnerState::Idle);
                self.emit("", "自动化已停止");
                return;
            }

            let screen = match self.sidecar().detect(None) {
                Ok(s) => s,
                Err(e) => {
                    self.set_state(RunnerState::Error { message: e.clone() });
                    self.emit("", &format!("画面识别失败: {e}"));
                    return;
                }
            };

            if screen != Screen::BattleResultContinue {
                self.battle_result_continue_handled = false;
            }

            match screen {
                Screen::TeamConfirm => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_team_confirm();
                }
                Screen::TeamChange => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_team_change();
                }
                Screen::SupportSelect => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_support_select();
                }
                Screen::ServantSelect => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_servant_select();
                }
                Screen::Battle => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    // NOTE: do NOT clear `waiting_for_battle` here. The
                    // screen classifier briefly returns Battle between
                    // taps and the NP cinematic, and clearing the flag
                    // too early would shrink the Unknown tolerance back
                    // to UNKNOWN_TIMEOUT mid-animation. handle_battle
                    // clears it itself once the attack button is
                    // actually visible (= we can really act).
                    self.handle_battle();
                }
                Screen::Attack => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    if self.battle.attack_submitted {
                        self.emit("Attack", "已提交本轮选卡，等待攻击动画");
                    } else {
                        self.handle_attack();
                    }
                }
                Screen::BattleResultBond => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_battle_result_bond();
                }
                Screen::BattleResultExp => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_battle_result_exp();
                }
                Screen::BattleResultLoot => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_battle_result_loot();
                }
                Screen::BattleResultFriendRequest => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_battle_result_friend_request();
                }
                Screen::BattleResultContinue => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_battle_result_continue();
                }
                Screen::APRecovery => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_ap_recovery();
                }
                Screen::Unknown => {
                    unknown_count += 1;
                    let timeout = if self.battle.waiting_for_battle {
                        UNKNOWN_TIMEOUT_LOADING
                    } else {
                        UNKNOWN_TIMEOUT
                    };
                    if unknown_count >= timeout {
                        self.set_state(RunnerState::Error {
                            message: "无法识别当前画面".into(),
                        });
                        self.emit("Unknown", "无法识别当前画面，已超时停止");
                        return;
                    }
                    self.emit(
                        "Unknown",
                        &format!("等待识别画面… ({unknown_count}/{timeout})"),
                    );
                    if is_battle_result_screen(last_detected_screen) {
                        self.emit_debug("Unknown", "结算页可能被弹窗遮挡，尝试点击跳过区域");
                        if self.tap_at("Unknown", BATTLE_RESULT_POPUP_SKIP) {
                            thread::sleep(ACTION_DELAY);
                        }
                    }
                }
            }

            if matches!(*self.state.lock().unwrap(), RunnerState::Error { .. }) {
                return;
            }

            if matches!(*self.state.lock().unwrap(), RunnerState::Finished) {
                self.emit("", "自动化已完成");
                return;
            }

            thread::sleep(POLL_INTERVAL);
        }
    }

    // -- battle screen handlers ----------------------------------------------

    fn handle_battle(&mut self) {
        // Check if the attack button is present (our turn to act)
        let attack_present = self
            .sidecar()
            .find_element_by_name(None, BATTLE_SCREEN, ATTACK_BUTTON_ELEMENT)
            .map(|m| m.found)
            .unwrap_or(false);

        if !attack_present {
            self.emit("Battle", "等待战斗动作…");
            return;
        }

        let attack_returned_after_submit = self.battle.attack_submitted;

        // Attack button is back -- the prior NP / attack cinematic (if
        // any) has finished. Drop back to the short Unknown tolerance.
        self.battle.waiting_for_battle = false;

        // Read the current battle scene (m of n) from the BATTLE label HUD.
        let screen_scene = self
            .sidecar()
            .read_battle_scene(None, BATTLE_SCENE_REGION)
            .unwrap_or(None);
        match post_attack_hud_read_gate(
            self.advanced_mode,
            attack_returned_after_submit,
            screen_scene,
            self.battle.post_attack_hud_wait_started,
            Instant::now(),
            POST_ATTACK_HUD_READ_TIMEOUT,
        ) {
            PostAttackHudReadGate::Ready => {
                self.battle.post_attack_hud_wait_started = None;
                self.battle.attack_submitted = false;
            }
            PostAttackHudReadGate::Waiting { started_at } => {
                self.battle.post_attack_hud_wait_started = Some(started_at);
                self.emit("Battle", "等待读取 Battle HUD…");
                return;
            }
            PostAttackHudReadGate::TimedOut => {
                self.battle.post_attack_hud_wait_started = None;
                self.battle.attack_submitted = false;
                self.emit("Battle", "Battle HUD 读取超时，沿用当前 Turn 推进逻辑");
            }
        }
        let scene_m = screen_scene.map(|(m, _)| m);

        // Decide what to do based on (prior scene, fresh read). See
        // `tick_scene_state` for the full state-transition rules; the
        // key invariant is that a failed CV read (`scene_m == None`)
        // never advances the index, never re-locks `last_screen_scene`,
        // and never re-fires skills for an already-executed scene.
        let tick = tick_scene_state(
            self.battle.last_screen_scene,
            self.battle.current_scene_index,
            self.battle.executed_scene_index,
            scene_m,
        );
        let previous_scene_index = self.battle.current_scene_index;
        self.battle.current_scene_index = tick.current_scene_index;
        self.battle.last_screen_scene = tick.last_screen_scene;
        let scene_changed = self.battle.current_scene_index != previous_scene_index;
        if !self.advanced_mode {
            if scene_changed {
                self.battle.current_turn_index = 0;
            } else if attack_returned_after_submit {
                self.battle.current_turn_index = self.battle.current_turn_index.saturating_add(1);
            }
        }

        if self.advanced_mode && tick.needs_exec {
            self.battle.scene_config_used = false;
            let scene_str = match screen_scene {
                Some((m, n)) => format!("{m}/{n}"),
                None => "?".into(),
            };
            self.emit(
                "Battle",
                &format!(
                    "执行第 {} 组指令 (画面场景: {})",
                    self.battle.current_scene_index + 1,
                    scene_str,
                ),
            );
            self.battle.scene_config_used = self
                .advanced_scenes
                .get(self.battle.current_scene_index)
                .is_some();
            self.battle.executed_scene_index = Some(self.battle.current_scene_index);
        } else if self.advanced_mode {
            self.emit("Battle", "场景未变更，直接攻击");
        } else {
            self.battle.scene_config_used = false;
            let turn_key = (
                self.battle.current_scene_index,
                self.battle.current_turn_index,
            );
            if let Some((turn_cfg, over_configured_turns)) = normal_turn_for_current_state(
                &self.scenes,
                self.battle.current_scene_index,
                self.battle.current_turn_index,
            ) {
                let scene_str = match screen_scene {
                    Some((m, n)) => format!("{m}/{n}"),
                    None => "?".into(),
                };
                if over_configured_turns {
                    self.emit(
                        "Battle",
                        &format!(
                            "Battle {} Turn {} 超出配置，沿用最后 Turn 的攻击配置 (画面场景: {})",
                            self.battle.current_scene_index + 1,
                            self.battle.current_turn_index + 1,
                            scene_str,
                        ),
                    );
                } else if self.battle.executed_turn_key != Some(turn_key) {
                    self.emit(
                        "Battle",
                        &format!(
                            "执行 Battle {} Turn {} 指令 (画面场景: {})",
                            self.battle.current_scene_index + 1,
                            self.battle.current_turn_index + 1,
                            scene_str,
                        ),
                    );
                    self.execute_turn_skills(&turn_cfg);
                    self.battle.executed_turn_key = Some(turn_key);
                    self.battle.scene_config_used = true;
                } else {
                    self.emit("Battle", "Turn 未变更，直接攻击");
                    self.battle.scene_config_used = true;
                }
            } else {
                self.emit(
                    "Battle",
                    &format!(
                        "无第 {} 组 Battle 配置，直接攻击",
                        self.battle.current_scene_index + 1
                    ),
                );
            }
        }

        if !self.advanced_mode {
            if let Some((turn_cfg, over_configured_turns)) = normal_turn_for_current_state(
                &self.scenes,
                self.battle.current_scene_index,
                self.battle.current_turn_index,
            ) {
                if !over_configured_turns {
                    self.select_enemy_target(turn_cfg.enemy_target.as_deref());
                }
            }
        }

        // Click the attack button
        self.emit("Battle", "点击攻击按钮");
        if !self.tap_at("Battle", ATTACK_BUTTON) {
            return;
        }
        thread::sleep(ACTION_DELAY);
    }
}

/// Build the runner's `TouchBackend`. Today there's only one
/// implementation, but the indirection through the trait makes adding
/// a faster transport (minitouch / sendevent / native helper) a
/// localized change later.
fn build_touch_backend(adb: &Adb) -> Box<dyn TouchBackend> {
    let backend = touch::build(adb);
    eprintln!("[touch] backend selected: {}", backend.name());
    backend
}

impl Drop for Runner {
    fn drop(&mut self) {
        // The touch backend's own Drop handles its cleanup. Today the
        // adb-input backend has nothing to clean up, but the trait
        // contract still lets a future backend (e.g. minitouch /
        // sendevent / native helper) release resources here.

        let Some(mut sidecar) = self.sidecar.take() else {
            return;
        };
        if let Err(err) = sidecar.stop_stream() {
            eprintln!("[mash-cv] stop_stream before caching runner sidecar failed: {err}");
        }
        if let Some(cache) = &self.sidecar_cache {
            let mut guard = cache.lock().unwrap();
            if guard.is_none() {
                *guard = Some(sidecar);
                return;
            }
        }
    }
}

/// Cheap (dx, dy) pixel jitter in the range
/// `[-TAP_JITTER_PX, TAP_JITTER_PX]` derived from the current wall-clock
/// nanos. Avoids pulling in a `rand` dependency for what is essentially
/// "make our taps look slightly less robotic" -- the distribution does
/// not need to be cryptographically uniform.
fn jitter_offset() -> (i32, i32) {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    // Two independent low-bit slices of the same nanos value.
    let span = (TAP_JITTER_PX * 2 + 1) as u32;
    let dx = (nanos % span) as i32 - TAP_JITTER_PX;
    let dy = ((nanos / span) % span) as i32 - TAP_JITTER_PX;
    (dx, dy)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
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
            ascension: None,
            face_score: None,
            crit_chance: crit,
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
        }
    }

    #[test]
    fn attack_log_command_cards_keep_slot_suit_and_servant_id() {
        let cards = vec![
            command_card(0, Some(309), Some("q"), None),
            command_card(1, None, Some("b"), None),
            command_card(2, Some(16), None, None),
        ];

        assert_eq!(
            command_cards_log_meta(&cards),
            vec![
                AttackLogCommandCard {
                    slot: 0,
                    suit: Some("q".into()),
                    servant_id: Some(309),
                },
                AttackLogCommandCard {
                    slot: 1,
                    suit: Some("b".into()),
                    servant_id: None,
                },
                AttackLogCommandCard {
                    slot: 2,
                    suit: None,
                    servant_id: Some(16),
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

    fn empty_advanced_scene() -> AdvancedBattleScene {
        AdvancedBattleScene {
            id: "advanced_scene_1".into(),
            main_output: None,
            grand_auto_order_change: None,
            command_conditions: Vec::new(),
            control_actions: Vec::new(),
            startup_actions: Vec::new(),
            rules: Vec::new(),
        }
    }

    fn normal_turn(
        preparation_actions: Vec<Action>,
        attack_priority: Vec<AttackCard>,
    ) -> BattleTurn {
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
            np_card: np_card.into(),
            priority: priority.into(),
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
    }

    #[test]
    fn run_config_round_trips_support_craft_essence_id() {
        let mut payload = minimal_run_config_json();
        payload["supportCraftEssenceId"] = serde_json::json!(1485);
        payload["supportSlotIndex"] = serde_json::json!(5);
        payload["supportGrandMode"] = serde_json::json!(true);
        payload["supportGrandCraftEssenceIds"] = serde_json::json!([1001, null, 1003]);
        payload["supportCraftEssenceMlbRequired"] = serde_json::json!(false);
        payload["supportGrandCraftEssenceMlbRequired"] = serde_json::json!([true, false, true]);
        payload["supportGrandBondCeMode"] = serde_json::json!("bondNp");
        payload["grandServants"] = serde_json::json!([
            { "slotIndex": 0, "npCard": "buster", "priority": "damage" },
            { "slotIndex": 2, "npCard": "auto", "priority": "np" }
        ]);
        let cfg: RunConfig = serde_json::from_value(payload).unwrap();
        assert_eq!(cfg.support_craft_essence_id, Some(1485));
        assert_eq!(cfg.support_craft_essence_mlb_required, false);
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
                { "slotIndex": 0, "npCard": "buster", "priority": "damage" },
                { "slotIndex": 2, "npCard": "auto", "priority": "np" }
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
                    }],
                },
                crate::AdvancedNpConditionGroup {
                    id: "np2".into(),
                    slots: vec![crate::AdvancedNpSlotCondition {
                        servant: "servant_2".into(),
                        ready: true,
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
                }],
            }],
            actions: vec![],
        };
        let cards = vec![command_card(0, Some(11), Some("b"), Some(80))];
        let nps = vec![np_slot(0, true), np_slot(1, true), np_slot(2, false)];
        let party_ids = [Some(11), Some(22), Some(33)];

        assert!(advanced_rule_matches(&rule, &cards, &nps, &party_ids));
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
                }],
            }],
            command_condition_groups: vec![crate::AdvancedCommandConditionGroup {
                id: "cmd".into(),
                cards: vec![AdvancedCommandCardCondition {
                    slot: 0,
                    servant: "servant_1".into(),
                    suit: "arts".into(),
                    min_crit_chance: Some(90),
                }],
            }],
            actions: vec![],
        };
        let cards = vec![command_card(0, Some(11), Some("a"), Some(80))];
        let nps = vec![np_slot(0, true)];
        let party_ids = [Some(11), None, None];

        assert!(!advanced_rule_matches(&rule, &cards, &nps, &party_ids));
    }

    #[test]
    fn advanced_startup_conditions_match_only_configured_command_cards() {
        let scene = AdvancedBattleScene {
            id: "advanced_scene_1".into(),
            main_output: None,
            grand_auto_order_change: None,
            command_conditions: vec![
                AdvancedCommandCardCondition {
                    slot: 0,
                    servant: "servant_1".into(),
                    suit: "buster".into(),
                    min_crit_chance: None,
                },
                AdvancedCommandCardCondition {
                    slot: 1,
                    servant: "any".into(),
                    suit: "any".into(),
                    min_crit_chance: None,
                },
            ],
            control_actions: Vec::new(),
            startup_actions: Vec::new(),
            rules: Vec::new(),
        };
        let cards = vec![
            command_card(0, Some(10), Some("b"), None),
            command_card(1, Some(20), Some("a"), None),
        ];
        let party_ids = [Some(10), Some(20), Some(30)];

        assert!(advanced_startup_conditions_match(
            &scene, &cards, &party_ids
        ));
    }

    #[test]
    fn advanced_startup_conditions_match_duplicate_servant_cards_in_any_slots() {
        let scene = AdvancedBattleScene {
            id: "advanced_scene_1".into(),
            main_output: None,
            grand_auto_order_change: None,
            command_conditions: vec![
                AdvancedCommandCardCondition {
                    slot: 0,
                    servant: "servant_1".into(),
                    suit: "any".into(),
                    min_crit_chance: None,
                },
                AdvancedCommandCardCondition {
                    slot: 1,
                    servant: "servant_1".into(),
                    suit: "any".into(),
                    min_crit_chance: None,
                },
            ],
            control_actions: Vec::new(),
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
            &scene, &cards, &party_ids
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
    fn normal_priority_skips_missing_chain_card_then_uses_fallbacks() {
        let priority = vec![
            AttackCard {
                id: "chain_1".into(),
                card: Some("servant_1_buster".into()),
            },
            AttackCard {
                id: "chain_2".into(),
                card: Some("servant_1_np".into()),
            },
            AttackCard {
                id: "chain_3".into(),
                card: Some("servant_1_arts".into()),
            },
            AttackCard {
                id: "fallback_1".into(),
                card: Some("servant_2_quick".into()),
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
            },
            AttackCard {
                id: "chain_2".into(),
                card: Some("servant_1_np".into()),
            },
            AttackCard {
                id: "chain_3".into(),
                card: Some("servant_1_buster".into()),
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
            },
            AttackCard {
                id: "chain_2".into(),
                card: Some("servant_1_all".into()),
            },
            AttackCard {
                id: "chain_3".into(),
                card: None,
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
            },
            AttackCard {
                id: "chain_2".into(),
                card: Some("servant_3_all".into()),
            },
            AttackCard {
                id: "chain_3".into(),
                card: Some("servant_3_all".into()),
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
            },
            AttackCard {
                id: "chain_2".into(),
                card: Some("servant_1_np".into()),
            },
            AttackCard {
                id: "chain_3".into(),
                card: Some("servant_1_arts".into()),
            },
            AttackCard {
                id: "fallback_1".into(),
                card: Some("servant_1_all".into()),
            },
            AttackCard {
                id: "fallback_2".into(),
                card: Some("servant_2_all".into()),
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
            },
            AttackCard {
                id: "chain_2".into(),
                card: Some("servant_1_np".into()),
            },
            AttackCard {
                id: "chain_3".into(),
                card: Some("servant_2_np".into()),
            },
            AttackCard {
                id: "fallback_1".into(),
                card: Some("servant_2_arts".into()),
            },
            AttackCard {
                id: "fallback_2".into(),
                card: Some("servant_3_arts".into()),
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
                    }],
                ),
                normal_turn(
                    vec![],
                    vec![AttackCard {
                        id: "turn_2_attack".into(),
                        card: Some("servant_2_arts".into()),
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
                }],
            )],
        )];

        assert!(normal_scenes_need_command_card_recognition(&scenes));
    }

    #[test]
    fn fallback_command_cards_use_fixed_attack_screen_positions() {
        let cards = fallback_command_cards();

        assert_eq!(cards.len(), COMMAND_CARDS.len());
        for (card, point) in cards.iter().zip(COMMAND_CARDS.iter()) {
            assert_eq!(card.servant_id, None);
            assert_eq!(card.suit, None);
            approx(card.x, point.x);
            approx(card.y, point.y);
        }
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
    fn advanced_attack_priority_still_requires_scene_config_used() {
        let scene = normal_scene(
            "scene_1",
            vec![normal_turn(
                vec![],
                vec![AttackCard {
                    id: "chain_1".into(),
                    card: Some("servant_1_all".into()),
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
                    order_change: Some(crate::OrderChangeSelection {
                        front: Some("servant_1".into()),
                        back: Some("servant_4".into()),
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
                        order_change: Some(crate::OrderChangeSelection {
                            front: Some("servant_1".into()),
                            back: Some("servant_4".into()),
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
                    skill: Some("skill_3".into()),
                    target: None,
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
            &[grand_config_at(4, 99, "buster", "damage")],
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
            &[grand_config_at(0, 99, "buster", "damage")],
        )
        .is_none());
    }

    #[test]
    fn startup_action_selected_slot_resolves_to_current_position_after_auto_order_change() {
        let original_ids = [Some(10), Some(20), Some(30), Some(99), None, None];
        let changed_ids = [Some(99), Some(20), Some(30), Some(10), None, None];
        let swapped_out_source = Action::Servant {
            id: "a1".into(),
            servant: Some("servant_1".into()),
            skill: Some("skill_1".into()),
            target: None,
        };
        let main_grand_source = Action::Servant {
            id: "a4".into(),
            servant: Some("servant_4".into()),
            skill: Some("skill_1".into()),
            target: Some("servant_2".into()),
        };
        let unchanged_source = Action::Servant {
            id: "a2".into(),
            servant: Some("servant_2".into()),
            skill: Some("skill_1".into()),
            target: None,
        };
        let swapped_target = Action::Equipment {
            id: "a3".into(),
            skill: Some("skill_1".into()),
            target: Some("servant_1".into()),
            order_change: None,
        };

        assert!(resolve_action_to_current_positions(
            &changed_ids,
            &original_ids,
            &swapped_out_source
        )
        .is_none());
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
        assert!(resolve_action_to_current_positions(
            &changed_ids,
            &original_ids,
            &unchanged_source
        )
        .is_some());
        assert!(
            resolve_action_to_current_positions(&changed_ids, &original_ids, &swapped_target)
                .is_none()
        );
    }

    #[test]
    fn auto_order_change_startup_flow_replays_control_after_swap() {
        let auto_order_change = Action::Equipment {
            id: "auto_oc".into(),
            skill: Some("skill_3".into()),
            target: None,
            order_change: Some(crate::OrderChangeSelection {
                front: Some("servant_1".into()),
                back: Some("servant_4".into()),
            }),
        };
        let first_control = Action::Equipment {
            id: "control_1".into(),
            skill: Some("skill_1".into()),
            target: Some("servant_1".into()),
            order_change: None,
        };
        let second_control = Action::Equipment {
            id: "control_2".into(),
            skill: Some("skill_2".into()),
            target: Some("servant_2".into()),
            order_change: None,
        };
        let startup = Action::Servant {
            id: "startup_1".into(),
            servant: Some("servant_1".into()),
            skill: Some("skill_1".into()),
            target: None,
        };
        let scene = AdvancedBattleScene {
            id: "advanced_scene_1".into(),
            main_output: None,
            grand_auto_order_change: Some(true),
            command_conditions: Vec::new(),
            control_actions: vec![first_control, second_control],
            startup_actions: vec![startup],
            rules: Vec::new(),
        };

        let actions = advanced_startup_flow_actions(&scene, 2, 1, Some(&auto_order_change));
        let ids: Vec<&str> = actions
            .iter()
            .map(|action| match action {
                Action::Servant { id, .. }
                | Action::Equipment { id, .. }
                | Action::CommandSpell { id, .. } => id.as_str(),
            })
            .collect();

        assert_eq!(ids, vec!["auto_oc", "control_1", "startup_1", "control_2"]);
    }

    #[test]
    fn party_lineup_change_swaps_support_from_configured_back_slot() {
        let mut ids = [Some(8), Some(434), Some(384), Some(11), Some(22), Some(999)];
        let action = Action::Equipment {
            id: "a1".into(),
            skill: Some("skill_3".into()),
            target: None,
            order_change: Some(crate::OrderChangeSelection {
                front: Some("servant_2".into()),
                back: Some("servant_6".into()),
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
            skill: Some("skill_2".into()),
            target: None,
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
            skill: Some("skill_2".into()),
            target: None,
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
            skill: Some("skill_3".into()),
            target: None,
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
            skill: Some("skill_3".into()),
            target: None,
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
            skill: Some("skill_1".into()),
            target: None,
        };

        assert!(!action_frontline_available(&ids, &action));
    }

    #[test]
    fn action_frontline_available_rejects_missing_frontline_target() {
        let ids = [Some(8), None, Some(384), Some(11), Some(22), None];
        let action = Action::Servant {
            id: "a1".into(),
            servant: Some("servant_1".into()),
            skill: Some("skill_1".into()),
            target: Some("servant_2".into()),
        };

        assert!(!action_frontline_available(&ids, &action));
    }

    #[test]
    fn advanced_auto_np_output_prefers_ready_np_and_arts_cards() {
        let scene = AdvancedBattleScene {
            id: "advanced_scene_1".into(),
            main_output: Some(crate::AdvancedMainOutput {
                servant: Some("servant_1".into()),
                output_type: Some(AdvancedOutputType::Np),
                np_card: Some("arts".into()),
            }),
            grand_auto_order_change: None,
            command_conditions: Vec::new(),
            control_actions: Vec::new(),
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
            &grands,
            &GrandCardStrategy::default(),
            GrandClass::Berserker,
        );

        assert_eq!(pick_labels(&picks), vec!["C0", "C1", "NP0"]);
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

    fn custom_rule_slot(
        servant_id: Option<u32>,
        kind: &str,
        color: &str,
    ) -> GrandCardRuleSlotConfig {
        GrandCardRuleSlotConfig {
            servant_id,
            grand_servant: false,
            kind: kind.into(),
            color: color.into(),
        }
    }

    fn custom_grand_rule_slot(kind: &str, color: &str) -> GrandCardRuleSlotConfig {
        GrandCardRuleSlotConfig {
            servant_id: None,
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
                custom_rule_slot(Some(10), "any", "buster"),
                custom_rule_slot(Some(20), "np", "quick"),
                custom_grand_rule_slot("command", "arts"),
            ],
        })
        .expect("supported custom rule should convert");

        assert!(!rule.same_color);
        assert!(!rule.color_set_baq);
        assert!(rule.include.is_empty());
        assert!(rule.exclude.is_empty());
        assert_eq!(rule.slots[0].owner, RuleOwner::ExactServant(10));
        assert_eq!(rule.slots[0].kind, RuleKind::Any);
        assert_eq!(rule.slots[1].owner, RuleOwner::ExactServant(20));
        assert_eq!(rule.slots[1].kind, RuleKind::Np);
        assert_eq!(rule.slots[2].owner, RuleOwner::AnyGrand);
        assert_eq!(rule.slots[2].kind, RuleKind::Command);
        assert_eq!(rule.target_role, None);
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
            &grands,
            &strategy,
        );

        assert_eq!(pick_labels(&picks), vec!["C2", "C0", "NP0"]);
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
                custom_rule_slot(None, "any", "buster"),
                custom_rule_slot(Some(10), "any", "quick"),
                custom_rule_slot(Some(10), "np", "any"),
            ],
        });

        let picks = choose_advanced_auto_picks(
            &scene,
            &cards,
            &nps,
            &[Some(10), Some(20), Some(30)],
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
        support_row_matches_level_requirements_with_progress(
            Server::Cn,
            &cfg,
            &row_b,
            &mut progress,
        );
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
                    order_change: None,
                },
                Action::Servant {
                    id: "sa_1".into(),
                    servant: Some("servant_1".into()),
                    skill: Some("skill_3".into()),
                    target: Some("servant_2".into()),
                },
                Action::CommandSpell {
                    id: "cs_1".into(),
                    spell: Some("restore".into()),
                    target: Some("servant_1".into()),
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
        let gate =
            post_attack_hud_read_gate(true, true, None, None, now, POST_ATTACK_HUD_READ_TIMEOUT);

        assert_eq!(gate, PostAttackHudReadGate::Ready);
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
}
