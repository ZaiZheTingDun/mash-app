use crate::adb::Adb;
use crate::screen::{NormRect, Point, Screen, SidecarClient};
use crate::{Action, Turn};
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
    /// Support servant template key to search for.
    pub support_servant_name: Option<String>,
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
    [Point::new(0.0, 0.0), Point::new(0.0, 0.0), Point::new(0.0, 0.0)],
    [Point::new(0.0, 0.0), Point::new(0.0, 0.0), Point::new(0.0, 0.0)],
    [Point::new(0.0, 0.0), Point::new(0.0, 0.0), Point::new(0.0, 0.0)],
];

/// Master / equipment skill buttons
const EQUIPMENT_SKILLS: [Point; 3] = [
    Point::new(0.0, 0.0),
    Point::new(0.0, 0.0),
    Point::new(0.0, 0.0),
];

/// Attack button position on the battle screen
const ATTACK_BUTTON: Point = Point::new(0.0, 0.0);

/// Region to search for the attack button template
const ATTACK_BUTTON_REGION: NormRect = NormRect {
    x: 0.0,
    y: 0.0,
    w: 1.0,
    h: 1.0,
};

/// Region where the turn number is displayed
const TURN_REGION: NormRect = NormRect {
    x: 0.0,
    y: 0.0,
    w: 0.2,
    h: 0.1,
};

/// Ally target positions for skill targeting (servant_1, servant_2, servant_3)
const SKILL_TARGETS: [Point; 3] = [
    Point::new(0.0, 0.0),
    Point::new(0.0, 0.0),
    Point::new(0.0, 0.0),
];

/// Enemy target positions for attack targeting (enemy_1, enemy_2, enemy_3)
const ENEMY_TARGETS: [Point; 3] = [
    Point::new(0.0, 0.0),
    Point::new(0.0, 0.0),
    Point::new(0.0, 0.0),
];

/// Command card positions on the attack screen (5 cards left to right)
const COMMAND_CARDS: [Point; 5] = [
    Point::new(0.0, 0.0),
    Point::new(0.0, 0.0),
    Point::new(0.0, 0.0),
    Point::new(0.0, 0.0),
    Point::new(0.0, 0.0),
];

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

const POLL_INTERVAL: Duration = Duration::from_millis(800);
const ACTION_DELAY: Duration = Duration::from_millis(500);
const UNKNOWN_TIMEOUT: u32 = 10;
const UNKNOWN_TIMEOUT_LOADING: u32 = 30;

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
    // Pre-battle progress tracking
    team_changed: bool,
    support_selected: bool,
    support_scroll_count: u32,
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
            team_changed: false,
            support_selected: false,
            support_scroll_count: 0,
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
        match self.adb.tap(px, py) {
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
                    self.battle.waiting_for_battle = false;
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
        if self.support_scroll_count == 0 {
            if let Some(ref _class) = self.config.support_class_filter {
                self.emit("SupportSelect", "选择职阶筛选");
                // TODO: map class name → tab position and tap
            }
        }

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
        } else {
            self.emit("SupportSelect", "选择第一个助战从者");
            if !self.tap_at("SupportSelect", Point::new(0.50, 0.35)) {
                return;
            }
            self.support_selected = true;
            thread::sleep(ACTION_DELAY);
            return;
        }

        if self.support_scroll_count < self.config.max_support_scrolls {
            self.emit(
                "SupportSelect",
                &format!(
                    "未找到目标助战，滚动列表 ({}/{})",
                    self.support_scroll_count + 1,
                    self.config.max_support_scrolls,
                ),
            );
            if !self.swipe_at("SupportSelect", Point::new(0.50, 0.70), Point::new(0.50, 0.30), 300)
            {
                return;
            }
            self.support_scroll_count += 1;
            thread::sleep(ACTION_DELAY);
        } else {
            self.emit("SupportSelect", "刷新助战列表");
            if !self.tap_at("SupportSelect", Point::new(0.92, 0.08)) {
                return;
            }
            self.support_scroll_count = 0;
            thread::sleep(Duration::from_secs(2));
        }
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
            .find_element(None, "attack_button", ATTACK_BUTTON_REGION, 0.8)
            .unwrap_or(None)
            .is_some();

        if !attack_present {
            self.emit("Battle", "等待行动回合…");
            return;
        }

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

    fn handle_attack(&mut self) {
        if self.battle.turn_config_used {
            // TODO: use attackPriority to select optimal cards via CV.
            // For now, select the first 3 command cards.
            self.emit("Attack", "选择指令卡（前3张）");
        } else {
            self.emit("Attack", "回合未变更，选择默认指令卡（前3张）");
        }

        for i in 0..3 {
            if !self.tap_at("Attack", COMMAND_CARDS[i]) {
                return;
            }
            thread::sleep(ACTION_DELAY);
        }

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
            }
        }

        for action in &turn.equipment_actions {
            if let Action::Equipment { skill, .. } = action {
                let Some(pos) = equipment_skill_position(skill.as_deref()) else {
                    continue;
                };

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
            }
        }
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
