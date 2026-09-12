use crate::adb::Adb;
use crate::enhancement_runner::parse_selected_count;
use crate::runner::LogLevel;
use crate::screen::{CraftEssenceGridCell, NormRect, Point, SidecarClient};
use crate::touch::{self, TouchBackend};
use crate::Server;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::Emitter;

pub(crate) const EVENT_NAME: &str = "craft-essence-enhancement-automation-status";
const SCREEN_NAME: &str = "CraftEssenceEnhancement";
const BINARY_CONTROL_MIN_SCORE: f64 = 0.9;
const BINARY_CONTROL_SCORE_MARGIN: f64 = 0.04;
const FILTER_TOGGLE_OFF_MAX_LUMA: f64 = 145.0;
const FILTER_TOGGLE_ON_MIN_LUMA: f64 = 180.0;
const AUTO_CONFIG_OFF_MAX_SATURATION: f64 = 70.0;
const AUTO_CONFIG_ON_MIN_SATURATION: f64 = 110.0;
const ENHANCE_BUTTON_PRESENT_MIN_SCORE: f64 = 0.9;
const ENHANCE_BUTTON_NOT_READY_MAX_LUMA: f64 = 125.0;
const ENHANCE_BUTTON_READY_MIN_LUMA: f64 = 145.0;
const RECOMMEND_OPEN_MAX_ATTEMPTS: u8 = 5;
const RECOMMEND_READY_MAX_WAITS: u8 = 8;
const ENHANCE_OPEN_MAX_ATTEMPTS: u8 = 5;
const ENHANCEMENT_RETURN_MAX_WAITS: u8 = 40;
const ENHANCEMENT_MAIN_RETURN_CONFIRMATIONS: u8 = 2;
const RESIDUAL_ENHANCEMENT_MAX_CHECKS: u8 = 8;
const UNKNOWN_SCREEN_MAX_CHECKS: u8 = 12;
const TARGET_BOMB_COUNT: u8 = 8;
const PACKET_BATCH_SIZE: u8 = 20;
const MATERIAL_PAGE_MAX_SCROLLS: u8 = 12;
const TARGET_PAGE_MAX_SCROLLS: u8 = 12;
const MATERIAL_PENDING_COUNTER_MAX_WAITS: u8 = 5;

const TARGET_SELECT_BUTTON: Point = Point::new(0.153, 0.555);
const TARGET_RESELECT_BUTTON: Point = Point::new(0.195, 0.060);
const TARGET_LIST_CLOSE_BUTTON: Point = Point::new(0.040, 0.060);
const MATERIAL_SELECT_BUTTON: Point = Point::new(0.341, 0.328);
const RECOMMEND_MATERIAL_BUTTON: Point = Point::new(0.846, 0.233);
const MATERIAL_DECIDE_BUTTON: Point = Point::new(0.895, 0.933);
const MATERIAL_CLEAR_ALL_BUTTON: Point = Point::new(0.9, 0.292);
const UNIFIED_LOCK_BUTTON: Point = Point::new(0.024, 0.528);
const SELECT_OBJECT_BUTTON: Point = Point::new(0.024, 0.356);
const ENHANCE_BUTTON: Point = Point::new(0.896, 0.931);
const ENHANCE_CONFIRM_BUTTON: Point = Point::new(0.656, 0.819);
const ENHANCED_MATERIAL_WARNING_SLIDER_FROM: Point = Point::new(0.292, 0.727);
const ENHANCED_MATERIAL_WARNING_SLIDER_TO: Point = Point::new(0.704, 0.727);
const ENHANCED_MATERIAL_WARNING_DECIDE_BUTTON: Point = Point::new(0.650, 0.875);
const EXP_OVERFLOW_CLOSE_BUTTON: Point = Point::new(0.5, 0.78);
const ENHANCEMENT_SKIP_BUTTON: Point = Point::new(0.5, 0.055);
const LIST_SWIPE_FROM: Point = Point::new(0.70, 0.88);
const LIST_SWIPE_TO: Point = Point::new(0.70, 0.31);
const LIST_SCROLLBAR_X: f64 = 0.791;
const LIST_SCROLLBAR_OVERSHOOT_Y: f64 = 0.20;
const LIST_SCROLLBAR_TOP_MAX_TOP_Y: f64 = 0.28;
const LIST_SCROLLBAR_LEGACY_TOP_MAX_CENTER_Y: f64 = 0.36;
const LIST_RESET_MAX_ATTEMPTS: u8 = 3;
const FILTER_SCROLLBAR_TOP: Point = Point::new(0.888, 0.115);
const FILTER_SCROLLBAR_TOP_MAX_Y: f64 = 0.148;
const FILTER_SCROLLBAR_RESET_MAX_ATTEMPTS: u8 = 2;
const GRID_READ_MAX_FAILURES: u8 = 3;
const GRID_DENSITY_BUTTON: Point = Point::new(0.023, 0.938);
const FILTER_BUTTON: Point = Point::new(0.7635, 0.180);
const FILTER_CONFIRM_BUTTON: Point = Point::new(0.8235, 0.8855);
const ORDER_BUTTON: Point = Point::new(0.8795, 0.176);
const ORDER_LEVEL_BUTTON: Point = Point::new(0.255, 0.323);
const ORDER_CONFIRM_BUTTON: Point = Point::new(0.6735, 0.884);
const ORDER_DIRECTION_BUTTON: Point = Point::new(0.9748, 0.1833);
const RECOMMEND_INIT_BUTTON: Point = Point::new(0.1755, 0.8815);
const RECOMMEND_AUTO_CONFIG_BUTTON: Point = Point::new(0.6245, 0.733);
const RECOMMEND_EXECUTE_BUTTON: Point = Point::new(0.8295, 0.8815);
const RECOMMEND_EMPTY_CLOSE_BUTTON: Point = Point::new(0.482, 0.78);
const RECOMMEND_AUTO_CONFIG_REGION: NormRect = NormRect {
    x: 0.601,
    y: 0.685,
    w: 0.047,
    h: 0.09,
};
const ENHANCE_BUTTON_REGION: NormRect = NormRect {
    x: 0.8,
    y: 0.87,
    w: 0.19,
    h: 0.12,
};
const ITEM_GRID_REGION: NormRect = NormRect {
    x: 0.055,
    y: 0.251,
    w: 0.755,
    h: 0.747,
};
const MATERIAL_COUNTER_REGION: NormRect = NormRect {
    x: 0.33,
    y: 0.13,
    w: 0.22,
    h: 0.10,
};

const RARITY_FILTERS: [RarityFilter; 5] = [
    RarityFilter::new(5, false, 0.231, 0.305, 0.030, 0.041),
    RarityFilter::new(4, false, 0.380, 0.305, 0.030, 0.041),
    RarityFilter::new(3, false, 0.525, 0.305, 0.030, 0.041),
    RarityFilter::new(2, true, 0.675, 0.305, 0.030, 0.041),
    RarityFilter::new(1, true, 0.820, 0.305, 0.030, 0.041),
];

const RECOMMEND_FILTERS: [RecommendFilter; 7] = [
    RecommendFilter::rarity("1 星", 1, 0.225, 0.472, 0.025, 0.040),
    RecommendFilter::rarity("2 星", 2, 0.363, 0.472, 0.025, 0.040),
    RecommendFilter::fixed("3 星", false, 0.505, 0.472, 0.025, 0.040),
    RecommendFilter::fixed("4 星", false, 0.637, 0.472, 0.025, 0.040),
    RecommendFilter::fixed("5 星", false, 0.775, 0.472, 0.025, 0.040),
    RecommendFilter::fixed("未强化", true, 0.225, 0.580, 0.025, 0.040),
    RecommendFilter::fixed("已强化", false, 0.363, 0.580, 0.025, 0.040),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum CraftEssenceEnhancementMode {
    QpEfficient,
    Fast,
}

impl Default for CraftEssenceEnhancementMode {
    fn default() -> Self {
        Self::QpEfficient
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum CraftEssenceEnhancementRunnerState {
    Idle,
    Starting,
    Running,
    Finished,
    Error { message: String },
}

impl CraftEssenceEnhancementRunnerState {
    pub(crate) fn status(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Finished => "finished",
            Self::Error { .. } => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LifecycleEvent {
    WorkerStarted,
    StopRequested,
    Finished,
    Failed { message: String },
}

pub(crate) fn lifecycle_transition(
    state: CraftEssenceEnhancementRunnerState,
    event: LifecycleEvent,
) -> CraftEssenceEnhancementRunnerState {
    match (&state, event) {
        (CraftEssenceEnhancementRunnerState::Starting, LifecycleEvent::WorkerStarted) => {
            CraftEssenceEnhancementRunnerState::Running
        }
        (
            CraftEssenceEnhancementRunnerState::Starting
            | CraftEssenceEnhancementRunnerState::Running,
            LifecycleEvent::StopRequested,
        ) => CraftEssenceEnhancementRunnerState::Idle,
        (CraftEssenceEnhancementRunnerState::Running, LifecycleEvent::Finished) => {
            CraftEssenceEnhancementRunnerState::Finished
        }
        (_, LifecycleEvent::Failed { message }) => {
            CraftEssenceEnhancementRunnerState::Error { message }
        }
        _ => state,
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CraftEssenceEnhancementAutomationEvent {
    pub state: String,
    pub status: &'static str,
    pub current_screen: String,
    pub message: String,
    pub level: LogLevel,
}

pub struct CraftEssenceEnhancementRunnerHandle {
    pub state: Arc<Mutex<CraftEssenceEnhancementRunnerState>>,
    pub cancel: Arc<AtomicBool>,
}

impl CraftEssenceEnhancementRunnerHandle {
    pub fn new_idle() -> Self {
        Self {
            state: Arc::new(Mutex::new(CraftEssenceEnhancementRunnerState::Idle)),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Screen {
    Main { target_selected: bool, ready: bool },
    CraftEssenceSelect { descending: bool },
    CraftEssenceLockMode,
    MaterialSelect,
    FilterDialog,
    OrderDialog,
    RecommendMaterialDialog,
    RecommendMaterialEmptyDialog,
    EnhancedMaterialWarningDialog,
    EnhancementConfirmDialog,
    ExpOverflowDialog,
    EnhancementSuccess,
    Unknown,
}

mod device;
mod dialogs;
mod enhancement_flow;
mod material_selection;
mod policy;
mod strategy;
mod target_selection;
use policy::*;

#[derive(Clone, Copy)]
struct Probe {
    key: &'static str,
    element: &'static str,
}

const PROBES: [Probe; 18] = [
    Probe::new("icon_enhancement_result"),
    Probe::new("element_enhancement_ce_stripe"),
    Probe::new("element_enhancement_new"),
    Probe::new("button_enhancement_ready"),
    Probe::new("button_enhancement_ce_select_ce_mark"),
    Probe::new("button_enhancement_ce_lock_mode_active"),
    Probe::new("button_enhancement_ce_clean_all_select"),
    Probe::new("button_enhancement_ce_clean_all_select_ready"),
    Probe::new("dialog_enhancement_ce_filter"),
    Probe::new("button_enhancement_ce_filter_init"),
    Probe::new("dialog_enhancement_ce_order"),
    Probe::new("dialog_enhancement_ce_recommend_empty"),
    Probe::new("dialog_enhancement_ce_recommend_material"),
    Probe::new("dialog_enhancement_ce_enhanced_material_warning"),
    Probe::new("dialog_enhancement_ce_confirm"),
    Probe::new("dialog_enhancement_ce_confirm_compact"),
    Probe::new("text_exp_overflow"),
    Probe::new("element_enhancement_ce_success"),
];

impl Probe {
    const fn new(key: &'static str) -> Self {
        Self { key, element: key }
    }
}

#[derive(Clone, Copy)]
struct RarityFilter {
    rarity: u8,
    target_on: bool,
    region: NormRect,
}

impl RarityFilter {
    const fn new(rarity: u8, target_on: bool, x: f64, y: f64, w: f64, h: f64) -> Self {
        Self {
            rarity,
            target_on,
            region: NormRect { x, y, w, h },
        }
    }

    fn center(self) -> Point {
        Point::new(
            self.region.x + self.region.w / 2.0,
            self.region.y + self.region.h / 2.0,
        )
    }
}

#[derive(Clone, Copy)]
struct RecommendFilter {
    label: &'static str,
    rarity: Option<u8>,
    fixed_target_on: bool,
    region: NormRect,
}

impl RecommendFilter {
    const fn rarity(label: &'static str, rarity: u8, x: f64, y: f64, w: f64, h: f64) -> Self {
        Self {
            label,
            rarity: Some(rarity),
            fixed_target_on: false,
            region: NormRect { x, y, w, h },
        }
    }

    const fn fixed(label: &'static str, target_on: bool, x: f64, y: f64, w: f64, h: f64) -> Self {
        Self {
            label,
            rarity: None,
            fixed_target_on: target_on,
            region: NormRect { x, y, w, h },
        }
    }

    fn target_on(self, profile: RecommendMaterialProfile) -> bool {
        self.rarity
            .is_some_and(|filter_rarity| profile.includes_rarity(filter_rarity))
            || (self.rarity.is_none() && self.fixed_target_on)
    }

    fn center(self) -> Point {
        Point::new(
            self.region.x + self.region.w / 2.0,
            self.region.y + self.region.h / 2.0,
        )
    }
}

pub struct CraftEssenceEnhancementRunner {
    sidecar: Option<SidecarClient>,
    sidecar_cache: Option<Arc<Mutex<Option<SidecarClient>>>>,
    touch: Box<dyn TouchBackend>,
    app_handle: tauri::AppHandle,
    state: Arc<Mutex<CraftEssenceEnhancementRunnerState>>,
    cancel: Arc<AtomicBool>,
    screen_w: u32,
    screen_h: u32,
    density_checked: bool,
    filter_reset_done: bool,
    filter_configured: bool,
    filter_scroll_reset_done: bool,
    filter_scroll_reset_attempts: u8,
    filter_two_star_enabled: Option<bool>,
    filter_two_star_desired: bool,
    order_level_selected: bool,
    order_configured: bool,
    descending_checked: bool,
    target_tapped: bool,
    target_return_waits: u8,
    recommend_reset_done: bool,
    recommend_open_attempts: u8,
    recommend_execute_tapped: bool,
    recommend_executed_for_target: bool,
    recommend_configured_profile: Option<RecommendMaterialProfile>,
    recommend_profile: RecommendMaterialProfile,
    recommend_ready_waits: u8,
    enhance_open_attempts: u8,
    awaiting_enhancement_return: bool,
    pending_enhancement_stage: Option<StrategyStage>,
    enhancement_left_main: bool,
    enhancement_return_waits: u8,
    enhancement_main_return_checks: u8,
    post_enhancement_target_read_failures: u8,
    residual_enhancement_checks: u8,
    completed_enhancements: u32,
    strategy_stage: StrategyStage,
    current_bomb_fingerprint: String,
    current_bomb_level: u32,
    packet_fingerprint: String,
    packet_fingerprints: Vec<String>,
    packet_feed_remaining: Vec<String>,
    feed_inventory_packets: bool,
    materials_committed: bool,
    material_selected_count: u8,
    material_same_copy_selected: bool,
    material_same_copy_pending: bool,
    material_pending_counter_waits: u8,
    material_seen_cells: HashSet<String>,
    material_scrolls: u8,
    material_scroll_reset_needed: bool,
    material_scroll_reset_attempts: u8,
    material_grid_read_failures: u8,
    target_scrolls: u8,
    target_scroll_reset_needed: bool,
    target_scroll_reset_attempts: u8,
    target_grid_read_failures: u8,
    unexpected_material_checks: u8,
    completed_bombs: u8,
    initial_bombs_counted: bool,
    lock_candidate_row: Option<u32>,
    lock_candidate_col: Option<u32>,
    lock_candidate_point: Option<Point>,
    mode: CraftEssenceEnhancementMode,
}

impl CraftEssenceEnhancementRunner {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        adb: Adb,
        sidecar: SidecarClient,
        app_handle: tauri::AppHandle,
        state: Arc<Mutex<CraftEssenceEnhancementRunnerState>>,
        cancel: Arc<AtomicBool>,
        screen_size: (u32, u32),
        sidecar_cache: Option<Arc<Mutex<Option<SidecarClient>>>>,
        mode: CraftEssenceEnhancementMode,
    ) -> Self {
        let touch = touch::build(&adb, &app_handle, screen_size);
        Self {
            sidecar: Some(sidecar),
            sidecar_cache,
            touch,
            app_handle,
            state,
            cancel,
            screen_w: screen_size.0,
            screen_h: screen_size.1,
            density_checked: false,
            filter_reset_done: false,
            filter_configured: false,
            filter_scroll_reset_done: false,
            filter_scroll_reset_attempts: 0,
            filter_two_star_enabled: None,
            filter_two_star_desired: false,
            order_level_selected: false,
            order_configured: false,
            descending_checked: false,
            target_tapped: false,
            target_return_waits: 0,
            recommend_reset_done: false,
            recommend_open_attempts: 0,
            recommend_execute_tapped: false,
            recommend_executed_for_target: false,
            recommend_configured_profile: None,
            recommend_profile: recommend_profile_for_mode(mode),
            recommend_ready_waits: 0,
            enhance_open_attempts: 0,
            awaiting_enhancement_return: false,
            pending_enhancement_stage: None,
            enhancement_left_main: false,
            enhancement_return_waits: 0,
            enhancement_main_return_checks: 0,
            post_enhancement_target_read_failures: 0,
            residual_enhancement_checks: 0,
            completed_enhancements: 0,
            strategy_stage: StrategyStage::SelectBomb,
            current_bomb_fingerprint: String::new(),
            current_bomb_level: 0,
            packet_fingerprint: String::new(),
            packet_fingerprints: Vec::new(),
            packet_feed_remaining: Vec::new(),
            feed_inventory_packets: false,
            materials_committed: false,
            material_selected_count: 0,
            material_same_copy_selected: false,
            material_same_copy_pending: false,
            material_pending_counter_waits: 0,
            material_seen_cells: HashSet::new(),
            material_scrolls: 0,
            material_scroll_reset_needed: true,
            material_scroll_reset_attempts: 0,
            material_grid_read_failures: 0,
            target_scrolls: 0,
            target_scroll_reset_needed: true,
            target_scroll_reset_attempts: 0,
            target_grid_read_failures: 0,
            unexpected_material_checks: 0,
            completed_bombs: 0,
            initial_bombs_counted: false,
            lock_candidate_row: None,
            lock_candidate_col: None,
            lock_candidate_point: None,
            mode,
        }
    }

    pub fn run(mut self) {
        self.transition(LifecycleEvent::WorkerStarted);
        self.emit(
            "",
            if self.mode == CraftEssenceEnhancementMode::QpEfficient {
                "丸子制作自动化已启动（节省 QP 策略）"
            } else {
                "丸子制作自动化已启动（快速策略）"
            },
        );
        let mut unknown_count = 0_u8;
        loop {
            if self.cancel.load(Ordering::Relaxed) {
                self.transition(LifecycleEvent::StopRequested);
                self.emit("", "概念礼装强化自动化已停止");
                return;
            }
            let screen = self.detect_screen();
            if self.awaiting_enhancement_return {
                if !self.handle_enhancement_return(screen) {
                    return;
                }
                continue;
            }
            if screen == Screen::Unknown {
                unknown_count += 1;
                self.emit(
                    "Unknown",
                    &format!(
                        "未能识别当前页面，继续观察… ({unknown_count}/{UNKNOWN_SCREEN_MAX_CHECKS})"
                    ),
                );
                if unknown_count >= UNKNOWN_SCREEN_MAX_CHECKS {
                    self.fail(
                        "Unknown",
                        "连续多次无法识别当前页面，请确认当前处于概念礼装强化流程".into(),
                    );
                    return;
                }
                thread::sleep(Duration::from_millis(700));
                continue;
            }
            unknown_count = 0;

            match screen {
                Screen::Main {
                    target_selected: true,
                    ready,
                } => {
                    if !self.handle_strategy_main(ready) {
                        return;
                    }
                }
                Screen::Main {
                    target_selected: false,
                    ready,
                } => {
                    self.emit(
                        "CraftEssenceEnhancement",
                        if ready {
                            "当前未选择目标概念礼装（强化按钮状态：就绪），进入选择页面"
                        } else {
                            "当前未选择目标概念礼装，进入选择页面"
                        },
                    );
                    if self.tap_probe_or_point(
                        "CraftEssenceEnhancement",
                        "element_enhancement_new",
                        TARGET_SELECT_BUTTON,
                    ) {
                        thread::sleep(Duration::from_millis(900));
                    }
                }
                Screen::CraftEssenceSelect { descending } => {
                    if !self.handle_craft_essence_select(descending) {
                        return;
                    }
                }
                Screen::CraftEssenceLockMode => {
                    if !self.handle_craft_essence_lock_mode() {
                        return;
                    }
                }
                Screen::FilterDialog => {
                    if !self.handle_filter_dialog() {
                        return;
                    }
                }
                Screen::OrderDialog => {
                    if !self.handle_order_dialog() {
                        return;
                    }
                }
                Screen::RecommendMaterialDialog => {
                    if !self.handle_recommend_material_dialog() {
                        return;
                    }
                }
                Screen::RecommendMaterialEmptyDialog => {
                    if !self.handle_recommend_material_empty_dialog() {
                        return;
                    }
                }
                Screen::EnhancedMaterialWarningDialog => {
                    if !self.handle_enhanced_material_warning_dialog() {
                        return;
                    }
                }
                Screen::EnhancementConfirmDialog => {
                    if !enhancement_confirm_can_arm(self.strategy_stage, self.materials_committed)
                        && self.pending_enhancement_stage.is_none()
                    {
                        self.residual_enhancement_checks =
                            self.residual_enhancement_checks.saturating_add(1);
                        if self.residual_enhancement_checks >= RESIDUAL_ENHANCEMENT_MAX_CHECKS {
                            self.fail(
                                "EnhancementConfirmDialog",
                                "强化已结算后确认框信号持续存在，页面未能稳定".into(),
                            );
                            return;
                        }
                        self.emit(
                            "EnhancementConfirmDialog",
                            "忽略已结算强化的残留确认框信号，等待页面稳定",
                        );
                        thread::sleep(Duration::from_millis(500));
                        continue;
                    }
                    if !self.handle_enhancement_confirm_dialog() {
                        return;
                    }
                }
                Screen::ExpOverflowDialog => {
                    self.emit(
                        "EnhancementAnimation",
                        "检测到大成功或极大成功导致经验值溢出，关闭未使用素材提示",
                    );
                    if !self.tap_at("EnhancementAnimation", EXP_OVERFLOW_CLOSE_BUTTON) {
                        return;
                    }
                    thread::sleep(Duration::from_millis(700));
                }
                Screen::EnhancementSuccess => {
                    if unawaited_success_action(self.pending_enhancement_stage)
                        == UnawaitedSuccessAction::IgnoreResidual
                    {
                        self.residual_enhancement_checks =
                            self.residual_enhancement_checks.saturating_add(1);
                        if self.residual_enhancement_checks >= RESIDUAL_ENHANCEMENT_MAX_CHECKS {
                            self.fail(
                                "EnhancementAnimation",
                                "强化已结算后成功画面信号持续存在，页面未能稳定".into(),
                            );
                            return;
                        }
                        self.emit(
                            "EnhancementAnimation",
                            "忽略已结算强化的残留成功画面信号，只点击顶部返回且不重复结算",
                        );
                        if !self.tap_at("EnhancementAnimation", ENHANCEMENT_SKIP_BUTTON) {
                            return;
                        }
                        thread::sleep(Duration::from_millis(500));
                        continue;
                    }
                    self.awaiting_enhancement_return = true;
                    self.enhancement_left_main = true;
                    self.enhancement_return_waits = 0;
                    self.enhancement_main_return_checks = 0;
                    if !self.handle_enhancement_return(screen) {
                        return;
                    }
                }
                Screen::MaterialSelect => {
                    if !self.handle_strategy_material_select() {
                        return;
                    }
                }
                Screen::Unknown => unreachable!(),
            }
            self.residual_enhancement_checks = 0;
        }
    }
}

impl Drop for CraftEssenceEnhancementRunner {
    fn drop(&mut self) {
        let Some(mut sidecar) = self.sidecar.take() else {
            return;
        };
        if let Err(err) = sidecar.prepare_for_cache() {
            eprintln!("[mash-cv] prepare CE enhancement sidecar for cache failed: {err}");
        }
        if let Some(cache) = &self.sidecar_cache {
            let mut guard = cache.lock().unwrap();
            if guard.is_none() {
                *guard = Some(sidecar);
            }
        }
    }
}

pub(crate) fn server_supported(server: Server) -> bool {
    matches!(server, Server::Cn)
}

#[cfg(test)]
mod tests;
