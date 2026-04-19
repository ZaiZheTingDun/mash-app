use crate::adb::Adb;
use crate::screen::{
    CommandCardMatch, NoblePhantasmMatch, NormRect, Point, Screen, SidecarClient,
};
use crate::{load_servant_metadata, Action, AttackCard, ServantMetadata, Turn};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::Emitter;

// ---------------------------------------------------------------------------
// Configuration (received from frontend)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServantSlotConfig {
    pub slot_index: u32,
    pub servant_id: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunConfig {
    pub project_id: String,
    /// Desired party slot order, e.g. [2,0,1,3,4,5]. None = don't reorder.
    pub party_order: Option<Vec<u32>>,
    /// Class filter to tap in the support list (e.g. "Caster").
    pub support_class_filter: Option<String>,
    /// Legacy support servant template key. Only consulted by the no-pin
    /// fallback path in `handle_support_select`; the OCR detector reads
    /// `support_servant_id` instead.
    pub support_servant_name: Option<String>,
    /// Servant id pinned via the team-builder support slot. Drives the
    /// OCR-based `find_supports` lookup. `None` falls back to legacy
    /// behaviour (tap top of the list).
    #[serde(default)]
    pub support_servant_id: Option<u32>,
    /// Servants to place into specific party slots.
    pub servant_selections: Vec<ServantSlotConfig>,
    /// Max scrolls before refreshing the support list.
    pub max_support_scrolls: u32,
}

// ---------------------------------------------------------------------------
// Runner state (shared with Tauri commands)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum RunnerState {
    Idle,
    Running,
    Finished,
    Error { message: String },
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationEvent {
    pub state: String,
    pub current_screen: String,
    pub message: String,
}

/// Handle stored in Tauri managed state to control / observe the runner.
pub struct RunnerHandle {
    pub state: Arc<Mutex<RunnerState>>,
    pub cancel: Arc<AtomicBool>,
}

impl RunnerHandle {
    pub fn new_idle() -> Self {
        Self {
            state: Arc::new(Mutex::new(RunnerState::Idle)),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
}

// ---------------------------------------------------------------------------
// Placeholder positions (normalized 0..1, to be configured later)
// ---------------------------------------------------------------------------

/// Servant skill buttons: [servant_index][skill_index]
/// servant_1 = index 0, servant_2 = index 1, servant_3 = index 2
/// skill_1 = index 0, skill_2 = index 1, skill_3 = index 2
const SERVANT_SKILLS: [[Point; 3]; 3] = [
    [Point::new(0.058, 0.807), Point::new(0.127, 0.807), Point::new(0.196, 0.807)],
    [Point::new(0.305, 0.807), Point::new(0.374, 0.807), Point::new(0.443, 0.807)],
    [Point::new(0.553, 0.807), Point::new(0.622, 0.807), Point::new(0.691, 0.807)],
];

const EQUIPMENT_BUTTON: Point = Point::new(0.933, 0.434);

/// Master / equipment skill buttons
const EQUIPMENT_SKILLS: [Point; 3] = [
    Point::new(0.708, 0.436),
    Point::new(0.777, 0.436),
    Point::new(0.848, 0.436),
];

/// Attack button position on the battle screen
const ATTACK_BUTTON: Point = Point::new(0.887, 0.844);

/// Tap target that, when pressed during a skill / NP animation, makes the
/// game skip ahead to the next actionable frame. Same physical button
/// works after every skill on the battle screen.
const SKIP_ANIMATION_BUTTON: Point = Point::new(0.685, 0.095);

/// Region to search for the attack button template
const ATTACK_BUTTON_REGION: NormRect = NormRect {
    x: 0.799,
    y: 0.746,
    w: 0.177,
    h: 0.195,
};

/// Region where the turn number is displayed
const TURN_REGION: NormRect = NormRect {
    x: 0.587,
    y: 0.090,
    w: 0.208,
    h: 0.087,
};

/// Ally target positions for skill targeting (servant_1, servant_2, servant_3)
const SKILL_TARGETS: [Point; 3] = [
    Point::new(0.254, 0.474),
    Point::new(0.499, 0.474),
    Point::new(0.744, 0.474),
];

/// Enemy target positions for attack targeting (enemy_1, enemy_2, enemy_3)
const ENEMY_TARGETS: [Point; 3] = [
    Point::new(0.035, 0.060),
    Point::new(0.230, 0.060),
    Point::new(0.425, 0.060),
];

/// Command card positions on the attack screen (5 cards left to right)
const COMMAND_CARDS: [Point; 5] = [
    Point::new(0.097, 0.678),
    Point::new(0.303, 0.678),
    Point::new(0.504, 0.678),
    Point::new(0.705, 0.678),
    Point::new(0.907, 0.678),
];

const NOBLE_PHANTASMS: [Point; 3] = [
    Point::new(0.319, 0.242),
    Point::new(0.497, 0.242),
    Point::new(0.680, 0.242),
];

// ---------------------------------------------------------------------------
// Debug: expose coordinate constants for visualization
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabeledPoint {
    pub label: String,
    pub point: Point,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabeledRegion {
    pub label: String,
    pub region: NormRect,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoordGroup {
    pub id: String,
    pub label: String,
    pub points: Vec<LabeledPoint>,
    pub regions: Vec<LabeledRegion>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DebugCoordinates {
    pub groups: Vec<CoordGroup>,
}

/// Snapshot of every `Point` / `NormRect` constant the runner uses, grouped
/// for display in the debug UI.
pub fn debug_coordinates() -> DebugCoordinates {
    let mut servant_skill_points = Vec::with_capacity(9);
    for (si, row) in SERVANT_SKILLS.iter().enumerate() {
        for (ki, p) in row.iter().enumerate() {
            servant_skill_points.push(LabeledPoint {
                label: format!("S{}.{}", si + 1, ki + 1),
                point: *p,
            });
        }
    }

    let mut equipment_points = Vec::with_capacity(4);
    equipment_points.push(LabeledPoint {
        label: "Menu".into(),
        point: EQUIPMENT_BUTTON,
    });
    for (i, p) in EQUIPMENT_SKILLS.iter().enumerate() {
        equipment_points.push(LabeledPoint {
            label: format!("E{}", i + 1),
            point: *p,
        });
    }

    let groups = vec![
        CoordGroup {
            id: "servantSkills".into(),
            label: "从者技能".into(),
            points: servant_skill_points,
            regions: Vec::new(),
        },
        CoordGroup {
            id: "equipment".into(),
            label: "御主技能 / 装备".into(),
            points: equipment_points,
            regions: Vec::new(),
        },
        CoordGroup {
            id: "attack".into(),
            label: "攻击".into(),
            points: vec![LabeledPoint {
                label: "Attack".into(),
                point: ATTACK_BUTTON,
            }],
            regions: vec![LabeledRegion {
                label: "AttackRegion".into(),
                region: ATTACK_BUTTON_REGION,
            }],
        },
        CoordGroup {
            id: "turn".into(),
            label: "回合".into(),
            points: Vec::new(),
            regions: vec![LabeledRegion {
                label: "TurnRegion".into(),
                region: TURN_REGION,
            }],
        },
        CoordGroup {
            id: "skillTargets".into(),
            label: "技能目标".into(),
            points: SKILL_TARGETS
                .iter()
                .enumerate()
                .map(|(i, p)| LabeledPoint {
                    label: format!("Ally{}", i + 1),
                    point: *p,
                })
                .collect(),
            regions: Vec::new(),
        },
        CoordGroup {
            id: "enemyTargets".into(),
            label: "敌人目标".into(),
            points: ENEMY_TARGETS
                .iter()
                .enumerate()
                .map(|(i, p)| LabeledPoint {
                    label: format!("Enemy{}", i + 1),
                    point: *p,
                })
                .collect(),
            regions: Vec::new(),
        },
        CoordGroup {
            id: "commandCards".into(),
            label: "指令卡".into(),
            points: COMMAND_CARDS
                .iter()
                .enumerate()
                .map(|(i, p)| LabeledPoint {
                    label: format!("C{}", i + 1),
                    point: *p,
                })
                .collect(),
            regions: Vec::new(),
        },
        CoordGroup {
            id: "noblePhantasms".into(),
            label: "宝具".into(),
            points: NOBLE_PHANTASMS
                .iter()
                .enumerate()
                .map(|(i, p)| LabeledPoint {
                    label: format!("NP{}", i + 1),
                    point: *p,
                })
                .collect(),
            regions: Vec::new(),
        },
    ];

    DebugCoordinates { groups }
}

// ---------------------------------------------------------------------------
// Battle state
// ---------------------------------------------------------------------------

struct BattleState {
    /// Which turn config index we're executing (0-based into the turns vec)
    current_turn: usize,
    /// Turn number last detected from the screen
    last_screen_turn: Option<u32>,
    /// Whether we've already executed skills for the current turn
    skills_executed: bool,
    /// Whether we used the turn config (vs fallback) — drives card selection
    turn_config_used: bool,
    /// Set after clicking start on TeamConfirm; tolerates longer Unknown streaks
    waiting_for_battle: bool,
}

impl BattleState {
    fn new() -> Self {
        Self {
            current_turn: 0,
            last_screen_turn: None,
            skills_executed: false,
            turn_config_used: false,
            waiting_for_battle: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Runner
// ---------------------------------------------------------------------------

const POLL_INTERVAL: Duration = Duration::from_millis(500);
const ACTION_DELAY: Duration = Duration::from_millis(300);

/// Hard cap on friend-list refreshes inside `handle_support_select`. After
/// this many refreshes (each preceded by a full scroll cycle) without
/// finding the pinned servant, the runner aborts with an error so the user
/// isn't stuck looping forever on a servant that simply isn't available.
const SUPPORT_MAX_REFRESHES: u32 = 3;
/// Settle time after the support-list scroll swipe completes; long enough
/// for momentum scrolling to come to rest before the next OCR pass.
const SUPPORT_SCROLL_SETTLE: Duration = Duration::from_millis(900);
/// Settle time after tapping the "refresh friend list" button. The friend
/// list refetch and re-render takes ~2.5s on slow devices; one extra second
/// of buffer keeps us from OCRing a half-loaded list.
const SUPPORT_REFRESH_SETTLE: Duration = Duration::from_secs(3);

/// Template that appears at the bottom of the support scroll bar once the
/// list is fully scrolled. Bundled under `resources/templates/` so it's
/// auto-registered by `_load_templates` under this stem-only key.
const SUPPORT_SCROLL_END_TEMPLATE: &str = "ui_scroll_bar_end";
/// Crop the scroll-bar tail so template matching only inspects the corner
/// where the indicator can appear. Keeps the match unambiguous and cheap.
const SUPPORT_SCROLL_END_REGION: NormRect = NormRect {
    x: 0.937,
    y: 0.904,
    w: 0.063,
    h: 0.096,
};
/// Match threshold for `SUPPORT_SCROLL_END_TEMPLATE`. The indicator is a
/// fixed-shape sprite so we can demand a tight match; lowering this risks
/// false positives that prematurely trigger refreshes mid-list.
const SUPPORT_SCROLL_END_THRESHOLD: f64 = 0.85;
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

/// Maximum per-axis jitter (in physical pixels) added to every tap so
/// repeated runs don't land on identical coordinates. Small enough to
/// stay well inside button hit-boxes; large enough that the noise is
/// distinguishable from a deterministic script.
const TAP_JITTER_PX: i32 = 6;

const DEFAULT_W: u32 = 1080;
const DEFAULT_H: u32 = 1920;

pub struct Runner {
    adb: Adb,
    sidecar: SidecarClient,
    config: RunConfig,
    turns: Vec<Turn>,
    state: Arc<Mutex<RunnerState>>,
    cancel: Arc<AtomicBool>,
    app_handle: tauri::AppHandle,
    screen_w: u32,
    screen_h: u32,
    /// Per-servant face assets (`{id}/card_servant_*.png`). When ``None`` the
    /// sidecar can still report suit + slot but cannot identify which
    /// servant owns each command card, which means priority entries can't
    /// be matched and we fall through to the leftmost-fill path.
    assets_dir: Option<PathBuf>,
    // Pre-battle progress tracking
    team_changed: bool,
    support_selected: bool,
    support_scroll_count: u32,
    /// How many times we've tapped the friend-list refresh button this run.
    /// Reset alongside `support_scroll_count` once a match is selected.
    support_refresh_count: u32,
    /// Cached `(name, np_names)` for the pinned support servant. Loaded
    /// lazily on the first `handle_support_select` poll so we don't do disk
    /// I/O at 500ms cadence (and cleared between runs because each run
    /// owns its own `Runner`).
    support_meta: Option<ServantMetadata>,
    servants_placed: Vec<u32>,
    // Battle progress tracking
    battle: BattleState,
}

impl Runner {
    pub fn new(
        adb: Adb,
        sidecar: SidecarClient,
        config: RunConfig,
        turns: Vec<Turn>,
        app_handle: tauri::AppHandle,
        state: Arc<Mutex<RunnerState>>,
        cancel: Arc<AtomicBool>,
        screen_size: Option<(u32, u32)>,
        assets_dir: Option<PathBuf>,
    ) -> Self {
        let (screen_w, screen_h) = screen_size.unwrap_or((DEFAULT_W, DEFAULT_H));
        Self {
            adb,
            sidecar,
            config,
            turns,
            state,
            cancel,
            app_handle,
            screen_w,
            screen_h,
            assets_dir,
            team_changed: false,
            support_selected: false,
            support_scroll_count: 0,
            support_refresh_count: 0,
            support_meta: None,
            servants_placed: Vec::new(),
            battle: BattleState::new(),
        }
    }

    // -- helpers -------------------------------------------------------------

    fn set_state(&self, s: RunnerState) {
        *self.state.lock().unwrap() = s;
    }

    fn emit(&self, screen: &str, message: &str) {
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
            },
        );
    }

    fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    fn fail_action(&self, screen: &str, action: &str, err: String) {
        let message = format!("{action}失败: {err}");
        self.set_state(RunnerState::Error {
            message: message.clone(),
        });
        self.emit(screen, &message);
    }

    fn tap_at(&self, screen: &str, point: Point) -> bool {
        let (px, py) = point.to_physical(self.screen_w, self.screen_h);
        let (jx, jy) = jitter_offset();
        // Saturate at the screen edges so a near-edge button still
        // registers even if the jitter would push it off-screen.
        let tap_x = (px as i32 + jx)
            .clamp(0, self.screen_w.saturating_sub(1) as i32) as u32;
        let tap_y = (py as i32 + jy)
            .clamp(0, self.screen_h.saturating_sub(1) as i32) as u32;
        match self.adb.tap(tap_x, tap_y) {
            Ok(()) => true,
            Err(err) => {
                self.fail_action(screen, "点击", err);
                false
            }
        }
    }

    fn swipe_at(&self, screen: &str, from: Point, to: Point, duration_ms: u32) -> bool {
        let from_px = from.to_physical(self.screen_w, self.screen_h);
        let to_px = to.to_physical(self.screen_w, self.screen_h);
        match self.adb.swipe(from_px, to_px, duration_ms) {
            Ok(()) => true,
            Err(err) => {
                self.fail_action(screen, "滑动", err);
                false
            }
        }
    }

    /// Block until the attack button reappears in `ATTACK_BUTTON_REGION`,
    /// polling every `SKILL_POLL_INTERVAL`. Used after firing a skill so
    /// the next tap doesn't land during the cut-in / animation while the
    /// button is hidden.
    ///
    /// Returns ``true`` when the button is detected, ``false`` on timeout
    /// or cancellation. Emits status updates so the user can see the wait.
    fn wait_for_attack_button(&mut self, screen: &str, timeout: Duration) -> bool {
        let start = std::time::Instant::now();
        let mut tick: u32 = 0;
        loop {
            if self.is_cancelled() {
                return false;
            }
            let found = self
                .sidecar
                .find_element(None, "button_attack", ATTACK_BUTTON_REGION, 0.8)
                .unwrap_or(None)
                .is_some();
            if found {
                return true;
            }
            if start.elapsed() >= timeout {
                self.emit(screen, "等待攻击按钮超时");
                return false;
            }
            tick += 1;
            if tick % 4 == 1 {
                self.emit(screen, "等待技能动画结束…");
            }
            thread::sleep(SKILL_POLL_INTERVAL);
        }
    }

    // -- main loop -----------------------------------------------------------

    pub fn run(mut self) {
        self.set_state(RunnerState::Running);
        self.emit("", "自动化已启动");

        let mut unknown_count: u32 = 0;

        loop {
            if self.is_cancelled() {
                self.set_state(RunnerState::Idle);
                self.emit("", "自动化已停止");
                return;
            }

            let screen = match self.sidecar.detect(None) {
                Ok(s) => s,
                Err(e) => {
                    self.set_state(RunnerState::Error {
                        message: e.clone(),
                    });
                    self.emit("", &format!("画面识别失败: {e}"));
                    return;
                }
            };

            match screen {
                Screen::TeamConfirm => {
                    unknown_count = 0;
                    self.handle_team_confirm();
                }
                Screen::TeamChange => {
                    unknown_count = 0;
                    self.handle_team_change();
                }
                Screen::SupportSelect => {
                    unknown_count = 0;
                    self.handle_support_select();
                }
                Screen::ServantSelect => {
                    unknown_count = 0;
                    self.handle_servant_select();
                }
                Screen::Battle => {
                    unknown_count = 0;
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
                    self.handle_attack();
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

    // -- pre-battle screen handlers ------------------------------------------

    fn handle_team_confirm(&mut self) {
        if self.config.party_order.is_some() && !self.team_changed {
            self.emit("TeamConfirm", "需要调整队伍顺序，进入编成变更");
            if !self.tap_at("TeamConfirm", Point::new(0.83, 0.90)) {
                return;
            }
            thread::sleep(ACTION_DELAY);
            return;
        }

        if let Some(slot_cfg) = self.next_unfilled_slot() {
            self.emit(
                "TeamConfirm",
                &format!("选择从者到槽位 {}", slot_cfg.slot_index),
            );
            let slot_x = slot_x_position(slot_cfg.slot_index);
            if !self.tap_at("TeamConfirm", Point::new(slot_x, 0.45)) {
                return;
            }
            thread::sleep(ACTION_DELAY);
            return;
        }

        self.emit("TeamConfirm", "队伍就绪，点击开始任务");
        if !self.tap_at("TeamConfirm", Point::new(0.90, 0.93)) {
            return;
        }
        self.battle.waiting_for_battle = true;
        thread::sleep(ACTION_DELAY);
    }

    fn handle_team_change(&mut self) {
        if self.config.party_order.is_some() {
            // TODO: implement swap logic based on current vs desired order.
            self.emit("TeamChange", "完成顺序调整，确认返回");
            if !self.tap_at("TeamChange", Point::new(0.90, 0.93)) {
                return;
            }
            self.team_changed = true;
            thread::sleep(ACTION_DELAY);
        } else {
            if !self.tap_at("TeamChange", Point::new(0.05, 0.05)) {
                return;
            }
            thread::sleep(ACTION_DELAY);
        }
    }

    fn handle_support_select(&mut self) {
        // Optional class-tab filter (still TODO: map name → tab coords).
        if self.support_scroll_count == 0 && self.support_refresh_count == 0 {
            if let Some(ref _class) = self.config.support_class_filter {
                self.emit("SupportSelect", "选择职阶筛选");
                // TODO: map class name → tab position and tap
            }
        }

        // No servant pinned → fall back to "tap the top of the list" so
        // existing setups that never picked a support still work.
        let Some(servant_id) = self.config.support_servant_id else {
            self.legacy_pick_first_support();
            return;
        };

        // Lazy-load (name, np_names) once per run. The shared static cache
        // in `lib.rs` makes this cheap, but caching on the runner avoids
        // even hashing it at every poll.
        if self.support_meta.is_none() {
            match load_servant_metadata(&self.app_handle, servant_id) {
                Ok(meta) => self.support_meta = Some(meta),
                Err(e) => {
                    self.fail_action(
                        "SupportSelect",
                        "加载助战元数据",
                        format!("servant {servant_id}: {e}"),
                    );
                    return;
                }
            }
        }
        let meta = self.support_meta.clone().unwrap();

        // OCR the current screen and look for a row whose name + NP both
        // fuzzy-match the pinned servant's (name, np_names) pair.
        let result = match self.sidecar.find_supports(None, &meta.name, &meta.np_names) {
            Ok(r) => r,
            Err(e) => {
                self.fail_action("SupportSelect", "OCR 助战识别", e);
                return;
            }
        };

        // `supports` is already sorted top-down by the sidecar; pick the
        // topmost match so a servant that occurs twice in the list (e.g.
        // friend + non-friend slot) yields a deterministic tap target.
        if let Some(row) = result.supports.first() {
            self.emit(
                "SupportSelect",
                &format!(
                    "找到助战 {} (name {:.2}, np {:.2})",
                    meta.name, row.name_score, row.np_score,
                ),
            );
            if !self.tap_at("SupportSelect", row.tap) {
                return;
            }
            self.support_selected = true;
            self.support_scroll_count = 0;
            self.support_refresh_count = 0;
            thread::sleep(ACTION_DELAY);
            return;
        }

        // No match in the visible viewport. The scroll-bar tail indicator
        // is the source of truth for "have we reached the bottom of the
        // friend list" — keep swiping until it shows up, then refresh.
        if !self.support_scroll_bar_at_end() {
            self.emit(
                "SupportSelect",
                &format!(
                    "未找到 {}，滚动列表 (第 {} 次)",
                    meta.name,
                    self.support_scroll_count + 1,
                ),
            );
            if !self.swipe_at(
                "SupportSelect",
                Point::new(0.50, 0.70),
                Point::new(0.50, 0.30),
                300,
            ) {
                return;
            }
            self.support_scroll_count += 1;
            thread::sleep(SUPPORT_SCROLL_SETTLE);
        } else if self.support_refresh_count < SUPPORT_MAX_REFRESHES {
            self.emit(
                "SupportSelect",
                &format!(
                    "已到底部，刷新助战列表 ({}/{})",
                    self.support_refresh_count + 1,
                    SUPPORT_MAX_REFRESHES,
                ),
            );
            if !self.tap_at("SupportSelect", Point::new(0.92, 0.08)) {
                return;
            }
            self.support_scroll_count = 0;
            self.support_refresh_count += 1;
            thread::sleep(SUPPORT_REFRESH_SETTLE);
        } else {
            self.fail_action(
                "SupportSelect",
                "查找助战",
                format!("刷新 {} 次仍未找到 {}", SUPPORT_MAX_REFRESHES, meta.name),
            );
        }
    }

    /// Return true when the scroll-bar-end indicator is visible in the
    /// bottom-right corner of the support list, meaning the user has
    /// scrolled all the way down. Logs the actual match score every poll
    /// so the threshold can be tuned from real numbers; transient sidecar
    /// errors degrade to `false` so a CV blip just means "keep scrolling"
    /// instead of triggering a refresh loop.
    fn support_scroll_bar_at_end(&mut self) -> bool {
        match self.sidecar.find_element_full(
            None,
            SUPPORT_SCROLL_END_TEMPLATE,
            SUPPORT_SCROLL_END_REGION,
            SUPPORT_SCROLL_END_THRESHOLD,
        ) {
            Ok(m) => {
                eprintln!(
                    "[runner] scroll-bar-end score={:.3} (threshold {:.2}) -> {}",
                    m.score,
                    SUPPORT_SCROLL_END_THRESHOLD,
                    if m.found { "AT-BOTTOM" } else { "scrolling" },
                );
                m.found
            }
            Err(e) => {
                eprintln!(
                    "[runner] scroll-bar-end check failed (treating as not-at-bottom): {e}"
                );
                false
            }
        }
    }

    /// Legacy "tap the top of the list" path used when the project hasn't
    /// pinned a support servant. Mirrors the pre-OCR behaviour so existing
    /// projects don't regress; new setups should pin a servant via the
    /// team-builder support slot to engage the OCR-based flow above.
    fn legacy_pick_first_support(&mut self) {
        if let Some(name) = self.config.support_servant_name.as_deref() {
            let region = NormRect {
                x: 0.0,
                y: 0.15,
                w: 1.0,
                h: 0.75,
            };
            if let Ok(Some(pos)) = self.sidecar.find_element(None, name, region, 0.8) {
                self.emit("SupportSelect", &format!("找到助战从者: {name}"));
                if !self.tap_at("SupportSelect", pos) {
                    return;
                }
                self.support_selected = true;
                self.support_scroll_count = 0;
                thread::sleep(ACTION_DELAY);
                return;
            }
            // Template miss → fall through to scroll/refresh below.
            if self.support_scroll_count < self.config.max_support_scrolls {
                if !self.swipe_at(
                    "SupportSelect",
                    Point::new(0.50, 0.70),
                    Point::new(0.50, 0.30),
                    300,
                ) {
                    return;
                }
                self.support_scroll_count += 1;
                thread::sleep(ACTION_DELAY);
            } else {
                if !self.tap_at("SupportSelect", Point::new(0.92, 0.08)) {
                    return;
                }
                self.support_scroll_count = 0;
                thread::sleep(Duration::from_secs(2));
            }
            return;
        }

        self.emit("SupportSelect", "选择第一个助战从者");
        if !self.tap_at("SupportSelect", Point::new(0.50, 0.35)) {
            return;
        }
        self.support_selected = true;
        thread::sleep(ACTION_DELAY);
    }

    fn handle_servant_select(&mut self) {
        if let Some(slot_cfg) = self.next_unfilled_slot() {
            let servant_key = format!("servant_{}", slot_cfg.servant_id);
            let region = NormRect {
                x: 0.0,
                y: 0.10,
                w: 1.0,
                h: 0.85,
            };

            if let Ok(Some(pos)) =
                self.sidecar
                    .find_element(None, &servant_key, region, 0.8)
            {
                self.emit(
                    "ServantSelect",
                    &format!("找到从者 {}，点击选择", slot_cfg.servant_id),
                );
                if !self.tap_at("ServantSelect", pos) {
                    return;
                }
                self.servants_placed.push(slot_cfg.slot_index);
                thread::sleep(ACTION_DELAY);
            } else {
                self.emit(
                    "ServantSelect",
                    &format!("搜索从者 {}…", slot_cfg.servant_id),
                );
                if !self.swipe_at("ServantSelect", Point::new(0.50, 0.70), Point::new(0.50, 0.30), 300)
                {
                    return;
                }
                thread::sleep(ACTION_DELAY);
            }
        } else {
            self.emit("ServantSelect", "所有从者已选择，返回");
            if !self.tap_at("ServantSelect", Point::new(0.05, 0.05)) {
                return;
            }
            thread::sleep(ACTION_DELAY);
        }
    }

    // -- battle screen handlers ----------------------------------------------

    fn handle_battle(&mut self) {
        // Check if the attack button is present (our turn to act)
        let attack_present = self
            .sidecar
            .find_element(None, "button_attack", ATTACK_BUTTON_REGION, 0.8)
            .unwrap_or(None)
            .is_some();

        if !attack_present {
            self.emit("Battle", "等待行动回合…");
            return;
        }

        // Attack button is back -- the prior NP / attack cinematic (if
        // any) has finished. Drop back to the short Unknown tolerance.
        self.battle.waiting_for_battle = false;

        // Read current turn number from the screen
        let screen_turn = self
            .sidecar
            .read_turn(None, TURN_REGION)
            .unwrap_or(None);

        let turn_changed = match (self.battle.last_screen_turn, screen_turn) {
            (None, _) => true,
            (Some(prev), Some(curr)) if curr != prev => true,
            _ => false,
        };

        if turn_changed {
            if self.battle.last_screen_turn.is_some() {
                self.battle.current_turn += 1;
            }
            self.battle.last_screen_turn = screen_turn;
            self.battle.skills_executed = false;
            self.battle.turn_config_used = false;

            self.emit(
                "Battle",
                &format!(
                    "回合变更 → 执行第 {} 组指令 (画面回合: {})",
                    self.battle.current_turn + 1,
                    screen_turn.map(|n| n.to_string()).unwrap_or_else(|| "?".into()),
                ),
            );
        } else {
            self.emit("Battle", "回合未变更，直接攻击");
        }

        // Execute skills if this is a new turn and we have config for it
        if !self.battle.skills_executed {
            if let Some(turn_cfg) = self.turns.get(self.battle.current_turn).cloned() {
                self.execute_turn_skills(&turn_cfg);
                self.battle.turn_config_used = true;
            } else {
                self.emit(
                    "Battle",
                    &format!(
                        "无第 {} 组指令配置，直接攻击",
                        self.battle.current_turn + 1
                    ),
                );
            }
            self.battle.skills_executed = true;
        }

        // Click the attack button
        self.emit("Battle", "点击攻击按钮");
        if !self.tap_at("Battle", ATTACK_BUTTON) {
            return;
        }
        thread::sleep(ACTION_DELAY);
    }

    /// Build the front-line ``[party_slot_0, party_slot_1, party_slot_2]``
    /// id map. Player-configured slots take precedence; any remaining
    /// front-line slot is assumed to be the support (its id parsed out of
    /// ``support_servant_name`` when the user pinned a specific servant).
    fn build_party_ids(&self) -> [Option<u32>; 3] {
        let mut ids: [Option<u32>; 3] = [None, None, None];
        for sel in &self.config.servant_selections {
            let slot = sel.slot_index as usize;
            if slot < 3 {
                ids[slot] = Some(sel.servant_id);
            }
        }

        if let Some(support_id) = self
            .config
            .support_servant_name
            .as_deref()
            .and_then(parse_servant_name_id)
        {
            for slot in ids.iter_mut() {
                if slot.is_none() {
                    *slot = Some(support_id);
                    break;
                }
            }
        }
        ids
    }

    fn handle_attack(&mut self) {
        let party_ids = self.build_party_ids();

        // Unique candidate set, stable by party position.
        let mut candidate_ids: Vec<u32> = Vec::with_capacity(3);
        for id in party_ids.iter().flatten() {
            if !candidate_ids.contains(id) {
                candidate_ids.push(*id);
            }
        }

        if self.assets_dir.is_none() {
            self.emit(
                "Attack",
                "未找到从者资源目录，将无法按从者匹配指令卡",
            );
        }

        let cards = match self.sidecar.find_command_cards(
            None,
            None,
            &candidate_ids,
            self.assets_dir.as_deref(),
        ) {
            Ok(c) => c,
            Err(err) => {
                self.fail_action("Attack", "识别指令卡", err);
                return;
            }
        };

        let nps = match self.sidecar.find_noble_phantasms(None, None) {
            Ok(n) => n,
            Err(err) => {
                self.fail_action("Attack", "识别宝具卡", err);
                return;
            }
        };

        let card_summary: Vec<String> = cards
            .iter()
            .map(|c| {
                format!(
                    "C{}={}{}",
                    c.slot + 1,
                    c.suit.as_deref().unwrap_or("?"),
                    c.servant_id
                        .map(|id| format!("/{id}"))
                        .unwrap_or_default(),
                )
            })
            .collect();
        self.emit(
            "Attack",
            &format!("指令卡: {}", card_summary.join(" ")),
        );
        let ready: Vec<String> = nps
            .iter()
            .filter(|n| n.ready)
            .map(|n| format!("NP{}", n.slot + 1))
            .collect();
        if ready.is_empty() {
            self.emit("Attack", "宝具就绪: 无");
        } else {
            self.emit("Attack", &format!("宝具就绪: {}", ready.join(" ")));
        }

        let mut used_card_slots: HashSet<u32> = HashSet::new();
        let mut used_np_slots: HashSet<u32> = HashSet::new();
        let mut picks: Vec<Pick> = Vec::with_capacity(3);

        if self.battle.turn_config_used {
            if let Some(turn_cfg) = self.turns.get(self.battle.current_turn) {
                picks = pick_by_priority(
                    &turn_cfg.attack_priority,
                    &cards,
                    &nps,
                    &party_ids,
                    &mut used_card_slots,
                    &mut used_np_slots,
                );
            }
        } else {
            self.emit("Attack", "回合未变更，按默认顺序补位");
        }

        if picks.len() < 3 {
            fill_remaining(&mut picks, &cards, &mut used_card_slots);
        }

        if picks.is_empty() {
            self.emit("Attack", "未能选出任何卡，跳过");
            self.battle.turn_config_used = false;
            return;
        }

        for (i, pick) in picks.iter().enumerate() {
            let (msg, point) = match pick {
                Pick::Card {
                    slot,
                    point,
                    servant_id,
                    suit,
                    from_priority,
                } => {
                    let label = match from_priority {
                        Some(p) => format!("选择 {p} → C{}", slot + 1),
                        None => format!("补位: C{}", slot + 1),
                    };
                    let detail = format!(
                        " ({}{})",
                        suit.as_deref().unwrap_or("?"),
                        servant_id
                            .map(|id| format!("/{id}"))
                            .unwrap_or_default(),
                    );
                    (format!("{}/{} {}{}", i + 1, picks.len(), label, detail), *point)
                }
                Pick::Np {
                    slot,
                    point,
                    from_priority,
                } => (
                    format!(
                        "{}/{} 选择 {} → NP{}",
                        i + 1,
                        picks.len(),
                        from_priority,
                        slot + 1,
                    ),
                    *point,
                ),
            };
            self.emit("Attack", &msg);
            if !self.tap_at("Attack", point) {
                return;
            }
            thread::sleep(ACTION_DELAY);
        }

        // The 3 cards (especially when an NP is included) trigger a long
        // attack cinematic before the battle screen comes back. While that
        // animation plays the screen classifier returns Unknown, so flag
        // the loop to use the longer Unknown tolerance until handle_battle
        // sees a real Battle frame again.
        self.battle.waiting_for_battle = true;

        // Reset for next cycle
        self.battle.turn_config_used = false;
    }

    // -- skill execution -----------------------------------------------------

    fn execute_turn_skills(&mut self, turn: &Turn) {
        for action in &turn.servant_actions {
            if let Action::Servant {
                servant,
                skill,
                target,
                ..
            } = action
            {
                let Some(pos) = skill_position(servant.as_deref(), skill.as_deref()) else {
                    continue;
                };

                self.emit(
                    "Battle",
                    &format!(
                        "从者技能: {} 使用 {}",
                        servant.as_deref().unwrap_or("?"),
                        skill.as_deref().unwrap_or("?"),
                    ),
                );
                if !self.tap_at("Battle", pos) {
                    return;
                }
                thread::sleep(ACTION_DELAY);

                if let Some(target_pos) = skill_target_position(target.as_deref()) {
                    self.emit(
                        "Battle",
                        &format!("选择目标: {}", target.as_deref().unwrap_or("?")),
                    );
                    if !self.tap_at("Battle", target_pos) {
                        return;
                    }
                    thread::sleep(ACTION_DELAY);
                }

                self.skip_after_skill();

                // Skill animation (cut-in, buff effect, etc.) hides the
                // attack button. Wait for it to reappear before firing
                // the next skill, otherwise rapid taps land on nothing
                // or, worse, on whatever overlay is currently shown.
                if !self.wait_for_attack_button("Battle", SKILL_WAIT_TIMEOUT) {
                    return;
                }
            }
        }

        for action in &turn.equipment_actions {
            if let Action::Equipment { skill, target, .. } = action {
                let Some(pos) = equipment_skill_position(skill.as_deref()) else {
                    continue;
                };

                self.emit("Battle", "打开御主技能面板");
                if !self.tap_at("Battle", EQUIPMENT_BUTTON) {
                    return;
                }
                thread::sleep(ACTION_DELAY);

                self.emit(
                    "Battle",
                    &format!(
                        "御主技能: {}",
                        skill.as_deref().unwrap_or("?"),
                    ),
                );
                if !self.tap_at("Battle", pos) {
                    return;
                }
                thread::sleep(ACTION_DELAY);

                if let Some(target_pos) = skill_target_position(target.as_deref()) {
                    self.emit(
                        "Battle",
                        &format!("选择目标: {}", target.as_deref().unwrap_or("?")),
                    );
                    if !self.tap_at("Battle", target_pos) {
                        return;
                    }
                    thread::sleep(ACTION_DELAY);
                }

                self.skip_after_skill();

                if !self.wait_for_attack_button("Battle", SKILL_WAIT_TIMEOUT) {
                    return;
                }
            }
        }
    }

    /// Tap the in-game "skip animation" button so the cut-in / buff
    /// effect plays at full speed. The button is in the top-right corner
    /// and is safe to tap whether or not an animation is currently
    /// playing -- on a normal Battle frame this region is the turn-counter
    /// pill which has no interactive effect.
    fn skip_after_skill(&self) {
        let _ = self.tap_at("Battle", SKIP_ANIMATION_BUTTON);
        thread::sleep(ACTION_DELAY);
    }

    // -- utilities -----------------------------------------------------------

    fn next_unfilled_slot(&self) -> Option<ServantSlotConfig> {
        self.config
            .servant_selections
            .iter()
            .find(|s| !self.servants_placed.contains(&s.slot_index))
            .cloned()
    }
}

// ---------------------------------------------------------------------------
// Position mapping helpers
// ---------------------------------------------------------------------------

fn parse_index(s: &str, prefix: &str) -> Option<usize> {
    s.strip_prefix(prefix)
        .and_then(|n| n.parse::<usize>().ok())
        .map(|n| n.saturating_sub(1))
}

fn skill_position(servant: Option<&str>, skill: Option<&str>) -> Option<Point> {
    let si = parse_index(servant?, "servant_")?;
    let ki = parse_index(skill?, "skill_")?;
    SERVANT_SKILLS.get(si).and_then(|row| row.get(ki)).copied()
}

fn equipment_skill_position(skill: Option<&str>) -> Option<Point> {
    let ki = parse_index(skill?, "skill_")?;
    EQUIPMENT_SKILLS.get(ki).copied()
}

/// Skill targets are always allies (servant_1, servant_2, servant_3).
fn skill_target_position(target: Option<&str>) -> Option<Point> {
    let t = target?;
    let si = parse_index(t, "servant_")?;
    SKILL_TARGETS.get(si).copied()
}

/// Enemy targets (enemy_1, enemy_2, enemy_3). Available for future use.
#[allow(dead_code)]
fn enemy_target_position(target: Option<&str>) -> Option<Point> {
    let t = target?;
    let ei = parse_index(t, "enemy_")?;
    ENEMY_TARGETS.get(ei).copied()
}

fn slot_x_position(index: u32) -> f64 {
    match index {
        0 => 0.12,
        1 => 0.28,
        2 => 0.44,
        3 => 0.56,
        4 => 0.72,
        5 => 0.88,
        _ => 0.50,
    }
}

/// Parse a template-key style servant name like ``"servant_215"`` into its
/// numeric id. Returns ``None`` for any other string shape so callers can
/// gracefully drop unknown supports instead of erroring.
fn parse_servant_name_id(name: &str) -> Option<u32> {
    name.strip_prefix("servant_")?.parse::<u32>().ok()
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

/// Center of a normalized rectangle (used to derive a tap point from a
/// detected NP slot's `card_region`).
fn rect_center(r: &NormRect) -> Point {
    Point::new(r.x + r.w / 2.0, r.y + r.h / 2.0)
}

// ---------------------------------------------------------------------------
// Attack pick logic
// ---------------------------------------------------------------------------

/// One card chosen by the priority walk / fill step. Carries enough info to
/// log + tap.
#[derive(Debug, Clone)]
enum Pick {
    Card {
        slot: u32,
        point: Point,
        servant_id: Option<u32>,
        suit: Option<String>,
        /// Source of the pick: priority entry text (e.g. ``"servant_2_arts"``)
        /// or ``None`` if it came from the leftmost-fill step.
        from_priority: Option<String>,
    },
    Np {
        slot: u32,
        point: Point,
        from_priority: String,
    },
}

/// Map ``"quick"|"arts"|"buster"`` to the suit code returned by the sidecar.
fn suit_code(suit: &str) -> Option<&'static str> {
    match suit {
        "quick" => Some("q"),
        "arts" => Some("a"),
        "buster" => Some("b"),
        _ => None,
    }
}

/// Walk `priority` in order, picking matching cards / NPs until 3 are chosen
/// or the list is exhausted. Cards / NPs already picked are tracked in
/// `used_card_slots` / `used_np_slots` and will not be re-selected.
fn pick_by_priority(
    priority: &[AttackCard],
    cards: &[CommandCardMatch],
    nps: &[NoblePhantasmMatch],
    party_ids: &[Option<u32>; 3],
    used_card_slots: &mut HashSet<u32>,
    used_np_slots: &mut HashSet<u32>,
) -> Vec<Pick> {
    let mut picks: Vec<Pick> = Vec::with_capacity(3);

    for entry in priority {
        if picks.len() >= 3 {
            break;
        }
        let Some(card_str) = entry.card.as_deref() else {
            continue;
        };

        // Expect ``servant_{i}_{suit|np}``; bail on anything else.
        let rest = match card_str.strip_prefix("servant_") {
            Some(r) => r,
            None => continue,
        };
        let (idx_str, kind) = match rest.split_once('_') {
            Some(p) => p,
            None => continue,
        };
        let Ok(field_pos) = idx_str.parse::<usize>() else {
            continue;
        };
        if !(1..=3).contains(&field_pos) {
            continue;
        }

        if kind == "np" {
            let np_slot = (field_pos - 1) as u32;
            if used_np_slots.contains(&np_slot) {
                continue;
            }
            if let Some(np) = nps.iter().find(|n| n.slot == np_slot) {
                if np.ready {
                    used_np_slots.insert(np_slot);
                    picks.push(Pick::Np {
                        slot: np_slot,
                        point: rect_center(&np.card_region),
                        from_priority: card_str.to_string(),
                    });
                }
            }
            continue;
        }

        let Some(wanted_suit) = suit_code(kind) else {
            continue;
        };
        let Some(wanted_id) = party_ids[field_pos - 1] else {
            // No known servant at that field position — can't match by face.
            continue;
        };

        // Leftmost-by-slot scan among unused cards owned by this servant
        // with the desired suit.
        let mut best: Option<&CommandCardMatch> = None;
        for c in cards {
            if used_card_slots.contains(&c.slot) {
                continue;
            }
            if c.servant_id != Some(wanted_id) {
                continue;
            }
            if c.suit.as_deref() != Some(wanted_suit) {
                continue;
            }
            if best.map_or(true, |b| c.slot < b.slot) {
                best = Some(c);
            }
        }
        if let Some(c) = best {
            used_card_slots.insert(c.slot);
            picks.push(Pick::Card {
                slot: c.slot,
                point: Point::new(c.x, c.y),
                servant_id: c.servant_id,
                suit: c.suit.clone(),
                from_priority: Some(card_str.to_string()),
            });
        }
    }

    picks
}

/// After the priority walk, top picks up to 3 by choosing the leftmost
/// command cards that have not yet been used.
fn fill_remaining(
    picks: &mut Vec<Pick>,
    cards: &[CommandCardMatch],
    used_card_slots: &mut HashSet<u32>,
) {
    let mut sorted: Vec<&CommandCardMatch> = cards.iter().collect();
    sorted.sort_by_key(|c| c.slot);
    for c in sorted {
        if picks.len() >= 3 {
            break;
        }
        if used_card_slots.contains(&c.slot) {
            continue;
        }
        used_card_slots.insert(c.slot);
        picks.push(Pick::Card {
            slot: c.slot,
            point: Point::new(c.x, c.y),
            servant_id: c.servant_id,
            suit: c.suit.clone(),
            from_priority: None,
        });
    }
}
