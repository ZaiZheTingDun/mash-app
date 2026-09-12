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
mod material_selection;
mod policy;
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

    fn handle_strategy_main(&mut self, template_ready: bool) -> bool {
        match self.strategy_stage {
            StrategyStage::BombSelected => {
                self.strategy_stage = StrategyStage::SelectPacketBase;
                self.open_target_select("准备制作 1 破 1 星经验包")
            }
            StrategyStage::FindBombBaseToLock => {
                self.open_target_select("重新找到刚制作的 1 星满破底卡并安全上锁")
            }
            StrategyStage::SelectBombForTransfer => {
                self.open_target_select("经验包制作完成，重新选择丸子")
            }
            StrategyStage::InspectBomb => self.open_target_select("检查丸子当前等级"),
            StrategyStage::SelectBomb
            | StrategyStage::SelectBombBase
            | StrategyStage::SelectPacketBase => self.open_target_select("重新识别目标概念礼装"),
            StrategyStage::PacketAutoFeedPending => {
                self.handle_packet_auto_feed_main(template_ready)
            }
            StrategyStage::FastAutoFeedPending => self.handle_fast_auto_feed_main(template_ready),
            StrategyStage::BombBaseSelected
            | StrategyStage::PacketSelected
            | StrategyStage::BombSelectedForFeed => {
                if !self.materials_committed {
                    self.reset_material_selection();
                    self.emit(
                        "CraftEssenceEnhancement",
                        match self.strategy_stage {
                            StrategyStage::BombBaseSelected => {
                                "打开素材列表：只选择 4 张同名未锁定 1 星，制作满破底卡"
                            }
                            StrategyStage::PacketSelected => {
                                "打开素材列表：只选择 1 张同名未锁定 1 星，先完成 1 破"
                            }
                            StrategyStage::BombSelectedForFeed => {
                                if self.feed_inventory_packets {
                                    "打开素材列表：选择库存中未锁定、已升级的 1 星礼装喂给丸子"
                                } else {
                                    "打开素材列表：丸子只吃本批刚制作的至少 1 破 1 星经验包"
                                }
                            }
                            _ => unreachable!(),
                        },
                    );
                    if !self.tap_at("CraftEssenceEnhancement", MATERIAL_SELECT_BUTTON) {
                        return false;
                    }
                    thread::sleep(Duration::from_millis(850));
                    return true;
                }

                let ready = if template_ready {
                    true
                } else {
                    let button_match = match self.sidecar().find_element_by_name(
                        None,
                        SCREEN_NAME,
                        "button_enhancement_ready",
                    ) {
                        Ok(result) => result,
                        Err(err) => {
                            self.fail(
                                "CraftEssenceEnhancement",
                                format!("识别强化按钮失败: {err}"),
                            );
                            return false;
                        }
                    };
                    let mean_luma =
                        match self.sidecar().read_region_luma(None, ENHANCE_BUTTON_REGION) {
                            Ok(value) => value,
                            Err(err) => {
                                self.fail(
                                    "CraftEssenceEnhancement",
                                    format!("读取强化按钮亮度失败: {err}"),
                                );
                                return false;
                            }
                        };
                    matches!(
                        classify_enhancement_button(button_match.score, mean_luma),
                        EnhancementReadyState::Ready
                    )
                };

                if !ready {
                    self.recommend_ready_waits = self.recommend_ready_waits.saturating_add(1);
                    if self.recommend_ready_waits >= RECOMMEND_READY_MAX_WAITS {
                        self.fail(
                            "CraftEssenceEnhancement",
                            "素材已决定，但强化按钮连续多次未就绪".into(),
                        );
                        return false;
                    }
                    thread::sleep(Duration::from_millis(600));
                    return true;
                }

                self.recommend_ready_waits = 0;
                self.emit(
                    "CraftEssenceEnhancement",
                    match self.strategy_stage {
                        StrategyStage::BombBaseSelected => "制作 1 星满破丸子底卡",
                        StrategyStage::PacketSelected => "只喂 1 张同名礼装，完成经验包 1 破",
                        StrategyStage::BombSelectedForFeed => {
                            "将未锁定、已升级的 1 星经验包喂给丸子"
                        }
                        _ => unreachable!(),
                    },
                );
                if self.enhance_open_attempts >= ENHANCE_OPEN_MAX_ATTEMPTS {
                    self.fail(
                        "CraftEssenceEnhancement",
                        "多次点击强化按钮后，确认对话框仍未打开".into(),
                    );
                    return false;
                }
                if !self.tap_probe_or_point(
                    "CraftEssenceEnhancement",
                    "button_enhancement_ready",
                    ENHANCE_BUTTON,
                ) {
                    return false;
                }
                self.enhance_open_attempts = self.enhance_open_attempts.saturating_add(1);
                thread::sleep(Duration::from_millis(800));
                true
            }
            StrategyStage::LockBombBaseActive
            | StrategyStage::VerifyBombBaseLock
            | StrategyStage::ExitBombBaseLockMode
            | StrategyStage::QpEfficientComplete => false,
        }
    }

    fn handle_packet_auto_feed_main(&mut self, template_ready: bool) -> bool {
        self.handle_auto_feed_main(template_ready, AutoFeedStrategy::QpEfficientPacket)
    }

    fn handle_fast_auto_feed_main(&mut self, template_ready: bool) -> bool {
        self.handle_auto_feed_main(template_ready, AutoFeedStrategy::FastBomb)
    }

    fn handle_auto_feed_main(&mut self, template_ready: bool, strategy: AutoFeedStrategy) -> bool {
        if recommendation_needs_execution(
            self.recommend_executed_for_target,
            self.recommend_configured_profile,
            self.recommend_profile,
        ) {
            if self.recommend_open_attempts >= RECOMMEND_OPEN_MAX_ATTEMPTS {
                self.fail(
                    "CraftEssenceEnhancement",
                    "多次点击推荐选择后，对话框仍未打开".into(),
                );
                return false;
            }
            self.recommend_reset_done =
                self.recommend_configured_profile == Some(self.recommend_profile);
            self.recommend_execute_tapped = false;
            self.emit(
                "CraftEssenceEnhancement",
                &format!("打开推荐选择，设置为{}", self.recommend_profile.label()),
            );
            if !self.tap_at("CraftEssenceEnhancement", RECOMMEND_MATERIAL_BUTTON) {
                return false;
            }
            self.recommend_open_attempts = self.recommend_open_attempts.saturating_add(1);
            thread::sleep(Duration::from_millis(800));
            return true;
        }

        let ready = if template_ready {
            true
        } else {
            let button_match = match self.sidecar().find_element_by_name(
                None,
                SCREEN_NAME,
                "button_enhancement_ready",
            ) {
                Ok(result) => result,
                Err(err) => {
                    self.fail(
                        "CraftEssenceEnhancement",
                        format!("识别自动配置后的强化按钮失败: {err}"),
                    );
                    return false;
                }
            };
            let mean_luma = match self.sidecar().read_region_luma(None, ENHANCE_BUTTON_REGION) {
                Ok(value) => value,
                Err(err) => {
                    self.fail(
                        "CraftEssenceEnhancement",
                        format!("读取自动配置后的强化按钮亮度失败: {err}"),
                    );
                    return false;
                }
            };
            matches!(
                classify_enhancement_button(button_match.score, mean_luma),
                EnhancementReadyState::Ready
            )
        };

        if !ready {
            self.recommend_ready_waits = self.recommend_ready_waits.saturating_add(1);
            if self.recommend_ready_waits < RECOMMEND_READY_MAX_WAITS {
                self.emit(
                    "CraftEssenceEnhancement",
                    &format!(
                        "等待游戏用{}自动配置强化素材（{}/{RECOMMEND_READY_MAX_WAITS}）",
                        self.recommend_profile.label(),
                        self.recommend_ready_waits
                    ),
                );
                thread::sleep(Duration::from_millis(650));
                return true;
            }
            self.recommend_ready_waits = 0;
            if strategy == AutoFeedStrategy::QpEfficientPacket
                && self.recommend_profile == RecommendMaterialProfile::TwoStarOnly
            {
                self.recommend_profile = RecommendMaterialProfile::OneStarOnly;
                self.recommend_executed_for_target = false;
                self.recommend_open_attempts = 0;
                self.emit(
                    "CraftEssenceEnhancement",
                    "仅二星推荐配置没有可用素材，切换为仅一星",
                );
                return true;
            }
            self.transition(LifecycleEvent::Finished);
            self.emit(
                "CraftEssenceEnhancement",
                if strategy == AutoFeedStrategy::FastBomb {
                    "没有可用的 1 星、2 星未强化素材，快速策略结束"
                } else {
                    "一星和二星推荐素材均已耗尽，丸子制作结束"
                },
            );
            return false;
        }

        self.recommend_ready_waits = 0;
        self.materials_committed = true;
        self.emit(
            "CraftEssenceEnhancement",
            if strategy == AutoFeedStrategy::FastBomb {
                "游戏已为当前丸子自动配置 1 星、2 星未强化素材，继续强化"
            } else {
                "游戏已自动配置当前经验包素材，本经验包只执行这一次自动配置强化"
            },
        );
        if self.enhance_open_attempts >= ENHANCE_OPEN_MAX_ATTEMPTS {
            self.fail(
                "CraftEssenceEnhancement",
                "多次点击自动配置强化按钮后，确认对话框仍未打开".into(),
            );
            return false;
        }
        if !self.tap_probe_or_point(
            "CraftEssenceEnhancement",
            "button_enhancement_ready",
            ENHANCE_BUTTON,
        ) {
            return false;
        }
        self.enhance_open_attempts = self.enhance_open_attempts.saturating_add(1);
        thread::sleep(Duration::from_millis(800));
        true
    }

    fn open_target_select(&mut self, message: &str) -> bool {
        self.emit("CraftEssenceEnhancement", message);
        self.target_tapped = false;
        self.target_return_waits = 0;
        self.target_scrolls = 0;
        self.target_scroll_reset_needed = true;
        self.target_scroll_reset_attempts = 0;
        self.target_grid_read_failures = 0;
        self.filter_two_star_enabled = None;
        self.filter_configured = false;
        if !self.tap_at("CraftEssenceEnhancement", TARGET_RESELECT_BUTTON) {
            return false;
        }
        thread::sleep(Duration::from_millis(850));
        true
    }

    fn reset_material_selection(&mut self) {
        self.material_selected_count = 0;
        self.material_same_copy_selected = false;
        self.material_same_copy_pending = false;
        self.material_pending_counter_waits = 0;
        self.filter_two_star_enabled = None;
        self.filter_configured = false;
        self.material_seen_cells.clear();
        self.material_scrolls = 0;
        self.material_scroll_reset_needed = true;
        self.material_scroll_reset_attempts = 0;
        self.material_grid_read_failures = 0;
        self.materials_committed = false;
        self.packet_feed_remaining = if self.strategy_stage == StrategyStage::BombSelectedForFeed {
            self.packet_fingerprints.clone()
        } else {
            Vec::new()
        };
    }

    fn handle_craft_essence_lock_mode(&mut self) -> bool {
        match self.strategy_stage {
            StrategyStage::LockBombBaseActive => {
                let Some(point) = self.lock_candidate_point else {
                    self.fail(
                        "CraftEssenceLockMode",
                        "缺少刚确认未锁定的满破底卡坐标，拒绝执行锁定".into(),
                    );
                    return false;
                };
                self.emit(
                    "CraftEssenceLockMode",
                    "只锁定刚制作且进入模式前已确认未锁定的 1 星满破底卡",
                );
                if !self.tap_at("CraftEssenceLockMode", point) {
                    return false;
                }
                self.strategy_stage = StrategyStage::VerifyBombBaseLock;
                self.target_return_waits = 0;
                thread::sleep(Duration::from_millis(650));
                true
            }
            StrategyStage::VerifyBombBaseLock => {
                let grid = match self.sidecar().read_craft_essence_grid(
                    None,
                    "enhancement_ce/item_ce_bar_bronze",
                    1920.0,
                    ITEM_GRID_REGION,
                    1.2,
                ) {
                    Ok(grid) => grid,
                    Err(err) => {
                        self.fail(
                            "CraftEssenceLockMode",
                            format!("锁定后读取礼装网格失败: {err}"),
                        );
                        return false;
                    }
                };
                if !grid.found || grid.cells.iter().any(|cell| !cell.valid) {
                    self.fail(
                        "CraftEssenceLockMode",
                        "锁定后存在无法安全识别的礼装，已停止且不会再次点击卡片".into(),
                    );
                    return false;
                }
                let candidate = grid.cells.iter().find(|cell| {
                    Some(cell.row) == self.lock_candidate_row
                        && Some(cell.col) == self.lock_candidate_col
                });
                if candidate.is_some_and(|cell| {
                    is_verified_locked_bomb_base(
                        cell,
                        self.lock_candidate_row,
                        self.lock_candidate_col,
                    )
                }) {
                    self.emit(
                        "CraftEssenceLockMode",
                        "已确认新满破底卡出现锁图标，切回选择对象模式",
                    );
                    if !self.tap_at("CraftEssenceLockMode", SELECT_OBJECT_BUTTON) {
                        return false;
                    }
                    self.strategy_stage = StrategyStage::ExitBombBaseLockMode;
                    self.target_return_waits = 0;
                    thread::sleep(Duration::from_millis(700));
                    return true;
                }
                self.target_return_waits = self.target_return_waits.saturating_add(1);
                if self.target_return_waits >= 5 {
                    self.fail(
                        "CraftEssenceLockMode",
                        "未确认新满破底卡出现锁图标；为避免反向解锁，不会再次点击".into(),
                    );
                    return false;
                }
                thread::sleep(Duration::from_millis(450));
                true
            }
            _ => {
                self.fail(
                    "CraftEssenceLockMode",
                    "检测到非预期的“统一锁定/锁定解除”操作模式。为避免改变既有锁定状态，自动化已停止；请先切回“选择对象”模式"
                        .into(),
                );
                false
            }
        }
    }

    #[cfg(any())]
    fn handle_selected_main(&mut self, template_ready: bool) -> bool {
        let ready = if template_ready {
            self.enhance_button_absent_recovery_taps = 0;
            true
        } else {
            let button_match = match self.sidecar().find_element_by_name(
                None,
                SCREEN_NAME,
                "button_enhancement_ready",
            ) {
                Ok(button_match) => button_match,
                Err(err) => {
                    self.fail(
                        "CraftEssenceEnhancement",
                        format!("识别强化按钮失败: {err}"),
                    );
                    return false;
                }
            };
            let mean_luma = match self.sidecar().read_region_luma(None, ENHANCE_BUTTON_REGION) {
                Ok(mean_luma) => mean_luma,
                Err(err) => {
                    self.fail(
                        "CraftEssenceEnhancement",
                        format!("读取强化按钮亮度失败: {err}"),
                    );
                    return false;
                }
            };
            match classify_enhancement_button(button_match.score, mean_luma) {
                EnhancementReadyState::Absent => {
                    self.enhance_button_absent_recovery_taps += 1;
                    if self.enhance_button_absent_recovery_taps
                        >= ENHANCE_BUTTON_ABSENT_MAX_RECOVERY_TAPS
                    {
                        self.fail(
                            "CraftEssenceEnhancement",
                            format!(
                                "主页面状态中未检测到强化按钮（形状分数 {:.3}）",
                                button_match.score
                            ),
                        );
                        return false;
                    }
                    self.emit(
                        "EnhancementResultRecovery",
                        &format!(
                            "未检测到强化按钮，尝试点击顶部返回（形状分数 {:.3}）",
                            button_match.score
                        ),
                    );
                    if !self.tap_at("EnhancementResultRecovery", ENHANCEMENT_SKIP_BUTTON) {
                        return false;
                    }
                    thread::sleep(Duration::from_millis(700));
                    return true;
                }
                EnhancementReadyState::Ready => {
                    self.enhance_button_absent_recovery_taps = 0;
                    self.emit(
                        "CraftEssenceEnhancement",
                        &format!(
                            "通过按钮形状和亮度确认强化已就绪（分数 {:.3}，亮度 {mean_luma:.1}）",
                            button_match.score
                        ),
                    );
                    true
                }
                EnhancementReadyState::NotReady => {
                    self.enhance_button_absent_recovery_taps = 0;
                    false
                }
                EnhancementReadyState::Transitioning => {
                    self.enhance_button_absent_recovery_taps = 0;
                    self.emit(
                        "CraftEssenceEnhancement",
                        &format!(
                            "强化按钮状态正在变化，继续等待（分数 {:.3}，亮度 {mean_luma:.1}）",
                            button_match.score
                        ),
                    );
                    thread::sleep(Duration::from_millis(500));
                    return true;
                }
            }
        };

        if ready {
            self.post_enhancement_not_ready_checks = 0;
        } else {
            self.post_enhancement_not_ready_checks =
                self.post_enhancement_not_ready_checks.saturating_add(1);
        }

        match selected_main_action(
            ready,
            self.recommend_execute_tapped,
            self.completed_enhancements,
            self.post_enhancement_not_ready_checks,
        ) {
            SelectedMainAction::OpenRecommendation => {
                if self.recommend_open_attempts >= RECOMMEND_OPEN_MAX_ATTEMPTS {
                    self.fail(
                        "CraftEssenceEnhancement",
                        "多次点击推荐选择后，对话框仍未打开".into(),
                    );
                    return false;
                }
                self.emit("CraftEssenceEnhancement", "打开推荐强化素材设置");
                if !self.tap_at("CraftEssenceEnhancement", RECOMMEND_MATERIAL_BUTTON) {
                    return false;
                }
                self.recommend_open_attempts += 1;
                thread::sleep(Duration::from_millis(800));
                true
            }
            SelectedMainAction::WaitForAutoSelection => {
                self.emit("CraftEssenceEnhancement", "等待自动配置强化素材");
                thread::sleep(Duration::from_millis(700));
                true
            }
            SelectedMainAction::Enhance => {
                if self.enhance_open_attempts >= ENHANCE_OPEN_MAX_ATTEMPTS {
                    self.fail(
                        "CraftEssenceEnhancement",
                        "多次点击强化按钮后，确认对话框仍未打开".into(),
                    );
                    return false;
                }
                self.emit(
                    "CraftEssenceEnhancement",
                    &format!("开始第 {} 次强化", self.completed_enhancements + 1),
                );
                if !self.tap_probe_or_point(
                    "CraftEssenceEnhancement",
                    "button_enhancement_ready",
                    ENHANCE_BUTTON,
                ) {
                    return false;
                }
                self.enhance_open_attempts += 1;
                thread::sleep(Duration::from_millis(800));
                true
            }
            SelectedMainAction::Finished => {
                self.transition(LifecycleEvent::Finished);
                self.emit(
                    "CraftEssenceEnhancement",
                    &format!(
                        "自动强化结束：概念礼装已满级或没有可用强化素材（共完成 {} 次强化）",
                        self.completed_enhancements
                    ),
                );
                false
            }
        }
    }

    fn handle_enhancement_confirm_dialog(&mut self) -> bool {
        if self.pending_enhancement_stage.is_some() {
            self.awaiting_enhancement_return = true;
            self.enhancement_return_waits = 0;
            self.enhancement_main_return_checks = 0;
            thread::sleep(Duration::from_millis(500));
            return true;
        }
        if !enhancement_confirm_can_arm(self.strategy_stage, self.materials_committed) {
            self.fail(
                "EnhancementConfirmDialog",
                format!(
                    "当前策略阶段或素材提交状态不允许执行强化: {:?}",
                    self.strategy_stage
                ),
            );
            return false;
        }
        self.emit("EnhancementConfirmDialog", "确认执行概念礼装强化");
        if !self.tap_at("EnhancementConfirmDialog", ENHANCE_CONFIRM_BUTTON) {
            return false;
        }
        self.pending_enhancement_stage = Some(self.strategy_stage);
        self.awaiting_enhancement_return = true;
        self.enhancement_left_main = false;
        self.enhancement_return_waits = 0;
        self.enhancement_main_return_checks = 0;
        self.post_enhancement_target_read_failures = 0;
        thread::sleep(Duration::from_millis(900));
        true
    }

    fn handle_enhanced_material_warning_dialog(&mut self) -> bool {
        if self.pending_enhancement_stage.is_some()
            || !enhancement_confirm_can_arm(self.strategy_stage, self.materials_committed)
        {
            self.fail(
                "EnhancedMaterialWarningDialog",
                format!(
                    "当前策略阶段或素材提交状态不允许确认已强化素材: {:?}",
                    self.strategy_stage
                ),
            );
            return false;
        }
        self.emit(
            "EnhancedMaterialWarningDialog",
            "确认本批只包含已登记的经验包，滑动解锁本次决定按钮",
        );
        if !self.swipe_at(
            "EnhancedMaterialWarningDialog",
            ENHANCED_MATERIAL_WARNING_SLIDER_FROM,
            ENHANCED_MATERIAL_WARNING_SLIDER_TO,
            900,
        ) {
            return false;
        }
        thread::sleep(Duration::from_millis(450));
        if !self.tap_at(
            "EnhancedMaterialWarningDialog",
            ENHANCED_MATERIAL_WARNING_DECIDE_BUTTON,
        ) {
            return false;
        }
        thread::sleep(Duration::from_millis(800));
        true
    }

    fn handle_enhancement_return(&mut self, screen: Screen) -> bool {
        match enhancement_return_action(screen, self.enhancement_left_main) {
            EnhancementReturnAction::ObserveReturnedMain => {
                self.enhancement_main_return_checks =
                    self.enhancement_main_return_checks.saturating_add(1);
                self.emit("EnhancementAnimation", "确认已返回概念礼装强化页面");
                if !self.tap_at("EnhancementAnimation", ENHANCEMENT_SKIP_BUTTON) {
                    return false;
                }
                thread::sleep(Duration::from_millis(700));
                if !enhancement_main_return_confirmed(self.enhancement_main_return_checks) {
                    return true;
                }
                let target = match self.sidecar().read_craft_essence_main_target(None) {
                    Ok(target) => target,
                    Err(err) => {
                        self.post_enhancement_target_read_failures =
                            self.post_enhancement_target_read_failures.saturating_add(1);
                        if self.post_enhancement_target_read_failures >= GRID_READ_MAX_FAILURES {
                            self.fail(
                                "CraftEssenceEnhancement",
                                format!("强化后读取目标等级上限失败: {err}"),
                            );
                            return false;
                        }
                        self.emit(
                            "CraftEssenceEnhancement",
                            "强化后目标等级上限读取不稳定，原地重试",
                        );
                        return true;
                    }
                };
                let completed_stage = match complete_pending_enhancement(
                    &mut self.pending_enhancement_stage,
                    &target,
                ) {
                    Ok(stage) => stage,
                    Err(EnhancementCompletionError::TargetUnreadable) => {
                        self.post_enhancement_target_read_failures =
                            self.post_enhancement_target_read_failures.saturating_add(1);
                        if self.post_enhancement_target_read_failures >= GRID_READ_MAX_FAILURES {
                            self.fail(
                                "CraftEssenceEnhancement",
                                format!("强化后无法确认目标等级上限（OCR：{}）", target.text),
                            );
                            return false;
                        }
                        self.emit(
                            "CraftEssenceEnhancement",
                            &format!(
                                "强化后暂未识别到目标等级上限，原地重试（{}/{GRID_READ_MAX_FAILURES}，OCR：{}）",
                                self.post_enhancement_target_read_failures,
                                target.text
                            ),
                        );
                        return true;
                    }
                    Err(EnhancementCompletionError::CapMismatch { expected, actual }) => {
                        let expected_label = if self.pending_enhancement_stage
                            == Some(StrategyStage::PacketAutoFeedPending)
                        {
                            "20/30/40/50".to_string()
                        } else {
                            expected.to_string()
                        };
                        self.fail(
                            "CraftEssenceEnhancement",
                            format!(
                                "强化后目标等级上限校验失败：识别为 {}/{}，当前阶段要求上限 {expected_label}；不会登记本次产物",
                                target.level.unwrap_or(0),
                                actual.unwrap_or(0)
                            ),
                        );
                        return false;
                    }
                    Err(EnhancementCompletionError::MissingPending) => {
                        self.fail(
                            "CraftEssenceEnhancement",
                            "强化返回时没有待结算记录，拒绝重复结算".into(),
                        );
                        return false;
                    }
                    Err(EnhancementCompletionError::IllegalStage(stage)) => {
                        self.fail(
                            "CraftEssenceEnhancement",
                            format!("强化返回记录包含非法策略阶段: {stage:?}"),
                        );
                        return false;
                    }
                };
                if !target.found {
                    self.emit(
                        "CraftEssenceEnhancement",
                        &format!(
                            "强化后当前等级读取不完整，已由独立上限证据确认上限 {}",
                            target.level_cap.unwrap_or(0)
                        ),
                    );
                }
                self.awaiting_enhancement_return = false;
                self.enhancement_left_main = false;
                self.enhancement_return_waits = 0;
                self.enhancement_main_return_checks = 0;
                self.post_enhancement_target_read_failures = 0;
                self.enhance_open_attempts = 0;
                self.completed_enhancements += 1;
                self.materials_committed = false;
                self.material_seen_cells.clear();
                match completed_stage {
                    StrategyStage::BombBaseSelected => {
                        self.strategy_stage = StrategyStage::FindBombBaseToLock;
                    }
                    StrategyStage::PacketSelected => {
                        self.strategy_stage = next_packet_stage_after_enhancement(
                            completed_stage,
                            self.packet_fingerprints.len(),
                        )
                        .expect("packet break stage must advance to automatic feed");
                        self.recommend_ready_waits = 0;
                        self.recommend_executed_for_target = false;
                        self.enhance_open_attempts = 0;
                        self.emit(
                            "CraftEssenceEnhancement",
                            "同名礼装强化完成，经验包已 1 破；等待游戏自动配置下一次素材",
                        );
                    }
                    StrategyStage::PacketAutoFeedPending => {
                        self.packet_fingerprints
                            .push(self.packet_fingerprint.clone());
                        self.packet_fingerprint.clear();
                        self.strategy_stage = next_packet_stage_after_enhancement(
                            completed_stage,
                            self.packet_fingerprints.len(),
                        )
                        .expect("packet automatic feed stage must advance after one enhancement");
                    }
                    StrategyStage::FastAutoFeedPending => {
                        self.current_bomb_level = target
                            .level
                            .expect("fast strategy requires a readable level");
                        if fast_bomb_is_complete(&target) {
                            self.transition(LifecycleEvent::Finished);
                            self.emit(
                                "CraftEssenceEnhancement",
                                &format!(
                                    "当前丸子已强化至 50 级（共完成 {} 次强化）",
                                    self.completed_enhancements
                                ),
                            );
                            return false;
                        }
                        self.recommend_ready_waits = 0;
                        self.enhance_open_attempts = 0;
                        self.emit(
                            "CraftEssenceEnhancement",
                            &format!(
                                "当前丸子已强化至 {}/50，等待自动配置下一批素材",
                                self.current_bomb_level
                            ),
                        );
                    }
                    StrategyStage::BombSelectedForFeed => {
                        self.packet_fingerprints.clear();
                        self.packet_feed_remaining.clear();
                        self.feed_inventory_packets = false;
                        self.strategy_stage = StrategyStage::InspectBomb;
                    }
                    _ => unreachable!("completed stage was validated before state transition"),
                }
                self.emit(
                    "CraftEssenceEnhancement",
                    &format!(
                        "第 {} 次强化完成，检查下一轮素材",
                        self.completed_enhancements
                    ),
                );
                true
            }
            EnhancementReturnAction::WaitForConfirmationClose => {
                self.enhancement_main_return_checks = 0;
                self.enhancement_return_waits = self.enhancement_return_waits.saturating_add(1);
                if self.enhancement_return_waits >= ENHANCEMENT_RETURN_MAX_WAITS {
                    self.fail(
                        "EnhancementConfirmDialog",
                        "点击决定后，强化确认对话框仍未关闭".into(),
                    );
                    return false;
                }
                thread::sleep(Duration::from_millis(500));
                true
            }
            EnhancementReturnAction::CloseExpOverflow => {
                self.enhancement_main_return_checks = 0;
                self.enhancement_left_main = true;
                self.enhancement_return_waits = self.enhancement_return_waits.saturating_add(1);
                if self.enhancement_return_waits >= ENHANCEMENT_RETURN_MAX_WAITS {
                    self.fail(
                        "EnhancementAnimation",
                        "经验值溢出提示持续未关闭，等待概念礼装强化结束超时".into(),
                    );
                    return false;
                }
                self.emit(
                    "EnhancementAnimation",
                    "检测到大成功或极大成功导致经验值溢出，关闭未使用素材提示",
                );
                if !self.tap_at("EnhancementAnimation", EXP_OVERFLOW_CLOSE_BUTTON) {
                    return false;
                }
                thread::sleep(Duration::from_millis(700));
                true
            }
            EnhancementReturnAction::TapSkip { mark_left_main } => {
                self.enhancement_main_return_checks = 0;
                self.enhancement_left_main |= mark_left_main;
                self.enhancement_return_waits = self.enhancement_return_waits.saturating_add(1);
                if self.enhancement_return_waits >= ENHANCEMENT_RETURN_MAX_WAITS {
                    self.fail(
                        "EnhancementAnimation",
                        if self.enhancement_left_main {
                            "等待概念礼装强化结束超时"
                        } else {
                            "点击决定后未能确认进入强化动画"
                        }
                        .into(),
                    );
                    return false;
                }
                self.emit(
                    "EnhancementAnimation",
                    if self.enhancement_left_main {
                        "点击页面顶部跳过强化动画"
                    } else {
                        "等待进入强化动画"
                    },
                );
                if !self.tap_at("EnhancementAnimation", ENHANCEMENT_SKIP_BUTTON) {
                    return false;
                }
                thread::sleep(Duration::from_millis(700));
                true
            }
            EnhancementReturnAction::Unexpected => {
                self.fail(
                    "EnhancementAnimation",
                    format!("强化动画期间进入了意外页面: {screen:?}"),
                );
                false
            }
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
