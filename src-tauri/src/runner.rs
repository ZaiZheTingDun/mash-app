use crate::adb::Adb;
use crate::screen::{NormRect, Point, Screen, SidecarClient};
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
// Runner
// ---------------------------------------------------------------------------

const POLL_INTERVAL: Duration = Duration::from_millis(800);
const ACTION_DELAY: Duration = Duration::from_millis(500);
const UNKNOWN_TIMEOUT: u32 = 10;

/// Screen dimensions read from the first screenshot.
/// Used to convert normalized coords to physical pixels for tap/swipe.
/// Falls back to 1080x1920 if detection fails.
const DEFAULT_W: u32 = 1080;
const DEFAULT_H: u32 = 1920;

pub struct Runner {
    adb: Adb,
    sidecar: SidecarClient,
    config: RunConfig,
    state: Arc<Mutex<RunnerState>>,
    cancel: Arc<AtomicBool>,
    app_handle: tauri::AppHandle,
    screen_w: u32,
    screen_h: u32,
    // Internal progress tracking
    team_changed: bool,
    support_selected: bool,
    support_scroll_count: u32,
    servants_placed: Vec<u32>,
}

impl Runner {
    pub fn new(
        adb: Adb,
        sidecar: SidecarClient,
        config: RunConfig,
        app_handle: tauri::AppHandle,
        state: Arc<Mutex<RunnerState>>,
        cancel: Arc<AtomicBool>,
    ) -> Self {
        Self {
            adb,
            sidecar,
            config,
            state,
            cancel,
            app_handle,
            screen_w: DEFAULT_W,
            screen_h: DEFAULT_H,
            team_changed: false,
            support_selected: false,
            support_scroll_count: 0,
            servants_placed: Vec::new(),
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

            // Take screenshot → temp file
            let img_path = match self.adb.screenshot_to_file() {
                Ok(p) => p,
                Err(e) => {
                    self.set_state(RunnerState::Error {
                        message: e.clone(),
                    });
                    self.emit("", &format!("截图失败: {e}"));
                    return;
                }
            };

            // Detect screen via sidecar
            let screen = match self.sidecar.detect(&img_path) {
                Ok(s) => s,
                Err(e) => {
                    let _ = std::fs::remove_file(&img_path);
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
                    self.handle_team_confirm(&img_path);
                }
                Screen::TeamChange => {
                    unknown_count = 0;
                    self.handle_team_change();
                }
                Screen::SupportSelect => {
                    unknown_count = 0;
                    self.handle_support_select(&img_path);
                }
                Screen::ServantSelect => {
                    unknown_count = 0;
                    self.handle_servant_select(&img_path);
                }
                Screen::Unknown => {
                    unknown_count += 1;
                    if unknown_count >= UNKNOWN_TIMEOUT {
                        let _ = std::fs::remove_file(&img_path);
                        self.set_state(RunnerState::Error {
                            message: "无法识别当前画面".into(),
                        });
                        self.emit("Unknown", "无法识别当前画面，已超时停止");
                        return;
                    }
                    self.emit(
                        "Unknown",
                        &format!("等待识别画面… ({unknown_count}/{UNKNOWN_TIMEOUT})"),
                    );
                }
            }

            let _ = std::fs::remove_file(&img_path);

            if matches!(*self.state.lock().unwrap(), RunnerState::Finished) {
                self.emit("", "自动化已完成");
                return;
            }

            thread::sleep(POLL_INTERVAL);
        }
    }

    // -- per-screen handlers -------------------------------------------------

    fn handle_team_confirm(&mut self, _img_path: &std::path::Path) {
        let (w, h) = (self.screen_w, self.screen_h);

        if self.config.party_order.is_some() && !self.team_changed {
            self.emit("TeamConfirm", "需要调整队伍顺序，进入编成变更");
            let (px, py) = Point::new(0.83, 0.90).to_physical(w, h);
            let _ = self.adb.tap(px, py);
            thread::sleep(ACTION_DELAY);
            return;
        }

        if let Some(slot_cfg) = self.next_unfilled_slot() {
            self.emit(
                "TeamConfirm",
                &format!("选择从者到槽位 {}", slot_cfg.slot_index),
            );
            let slot_x = slot_x_position(slot_cfg.slot_index);
            let (px, py) = Point::new(slot_x, 0.45).to_physical(w, h);
            let _ = self.adb.tap(px, py);
            thread::sleep(ACTION_DELAY);
            return;
        }

        self.emit("TeamConfirm", "队伍就绪，点击开始任务");
        let (px, py) = Point::new(0.90, 0.93).to_physical(w, h);
        let _ = self.adb.tap(px, py);
        thread::sleep(ACTION_DELAY);
    }

    fn handle_team_change(&mut self) {
        let (w, h) = (self.screen_w, self.screen_h);

        if self.config.party_order.is_some() {
            // TODO: implement swap logic based on current vs desired order.
            self.emit("TeamChange", "完成顺序调整，确认返回");
            let (px, py) = Point::new(0.90, 0.93).to_physical(w, h);
            let _ = self.adb.tap(px, py);
            self.team_changed = true;
            thread::sleep(ACTION_DELAY);
        } else {
            let (px, py) = Point::new(0.05, 0.05).to_physical(w, h);
            let _ = self.adb.tap(px, py);
            thread::sleep(ACTION_DELAY);
        }
    }

    fn handle_support_select(&mut self, img_path: &std::path::Path) {
        let (w, h) = (self.screen_w, self.screen_h);

        if self.support_scroll_count == 0 {
            if let Some(ref _class) = self.config.support_class_filter {
                self.emit("SupportSelect", "选择职阶筛选");
                // TODO: map class name → tab position and tap
            }
        }

        if let Some(ref name) = self.config.support_servant_name.clone() {
            let region = NormRect {
                x: 0.0,
                y: 0.15,
                w: 1.0,
                h: 0.75,
            };
            if let Ok(Some(pos)) = self.sidecar.find_element(img_path, name, region, 0.8) {
                self.emit("SupportSelect", &format!("找到助战从者: {name}"));
                let (px, py) = pos.to_physical(w, h);
                let _ = self.adb.tap(px, py);
                self.support_selected = true;
                self.support_scroll_count = 0;
                thread::sleep(ACTION_DELAY);
                return;
            }
        } else {
            self.emit("SupportSelect", "选择第一个助战从者");
            let (px, py) = Point::new(0.50, 0.35).to_physical(w, h);
            let _ = self.adb.tap(px, py);
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
            let from = Point::new(0.50, 0.70).to_physical(w, h);
            let to = Point::new(0.50, 0.30).to_physical(w, h);
            let _ = self.adb.swipe(from, to, 300);
            self.support_scroll_count += 1;
            thread::sleep(ACTION_DELAY);
        } else {
            self.emit("SupportSelect", "刷新助战列表");
            let (px, py) = Point::new(0.92, 0.08).to_physical(w, h);
            let _ = self.adb.tap(px, py);
            self.support_scroll_count = 0;
            thread::sleep(Duration::from_secs(2));
        }
    }

    fn handle_servant_select(&mut self, img_path: &std::path::Path) {
        let (w, h) = (self.screen_w, self.screen_h);

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
                    .find_element(img_path, &servant_key, region, 0.8)
            {
                self.emit(
                    "ServantSelect",
                    &format!("找到从者 {}，点击选择", slot_cfg.servant_id),
                );
                let (px, py) = pos.to_physical(w, h);
                let _ = self.adb.tap(px, py);
                self.servants_placed.push(slot_cfg.slot_index);
                thread::sleep(ACTION_DELAY);
            } else {
                self.emit(
                    "ServantSelect",
                    &format!("搜索从者 {}…", slot_cfg.servant_id),
                );
                let from = Point::new(0.50, 0.70).to_physical(w, h);
                let to = Point::new(0.50, 0.30).to_physical(w, h);
                let _ = self.adb.swipe(from, to, 300);
                thread::sleep(ACTION_DELAY);
            }
        } else {
            self.emit("ServantSelect", "所有从者已选择，返回");
            let (px, py) = Point::new(0.05, 0.05).to_physical(w, h);
            let _ = self.adb.tap(px, py);
            thread::sleep(ACTION_DELAY);
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
