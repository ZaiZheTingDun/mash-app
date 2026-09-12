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
mod policy;
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

    fn handle_strategy_material_select(&mut self) -> bool {
        if !matches!(
            self.strategy_stage,
            StrategyStage::BombBaseSelected
                | StrategyStage::PacketSelected
                | StrategyStage::BombSelectedForFeed
        ) {
            self.unexpected_material_checks = self.unexpected_material_checks.saturating_add(1);
            if self.unexpected_material_checks < GRID_READ_MAX_FAILURES {
                self.emit(
                    "MaterialSelect",
                    "页面信号与当前策略冲突，等待下一帧复核且不点击卡片",
                );
                thread::sleep(Duration::from_millis(500));
                return true;
            }
            self.fail(
                "MaterialSelect",
                format!("当前策略阶段不应进入素材列表: {:?}", self.strategy_stage),
            );
            return false;
        }
        self.unexpected_material_checks = 0;

        if !self.density_checked {
            if !self.ensure_max_density() {
                return false;
            }
            self.density_checked = true;
        }
        let displayed_selected_count = match self.read_material_selected_count() {
            Ok(count) => count,
            Err(message) => {
                self.fail("MaterialSelect", message);
                return false;
            }
        };
        if same_copy_counter_update_pending(
            self.material_same_copy_pending,
            self.material_selected_count,
            displayed_selected_count,
        ) {
            self.material_pending_counter_waits =
                self.material_pending_counter_waits.saturating_add(1);
            if self.material_pending_counter_waits < MATERIAL_PENDING_COUNTER_MAX_WAITS {
                self.emit(
                    "MaterialSelect",
                    &format!(
                        "同名副本点击后的页面计数仍为 {displayed_selected_count}/20，等待游戏更新（{}/{MATERIAL_PENDING_COUNTER_MAX_WAITS}）",
                        self.material_pending_counter_waits
                    ),
                );
                thread::sleep(Duration::from_millis(500));
                return true;
            }
            self.fail(
                "MaterialSelect",
                format!(
                    "同名副本点击后页面计数连续未更新：页面 {displayed_selected_count}/20，程序 {}/20",
                    self.material_selected_count
                ),
            );
            return false;
        }
        self.material_pending_counter_waits = 0;
        let possible_level_max = material_accepts_level_max(self.strategy_stage)
            && self.material_selected_count > displayed_selected_count
            && displayed_selected_count > 0;
        let level_max_reached = if possible_level_max {
            match self.read_material_level_max_reached() {
                Ok(reached) => reached,
                Err(message) => {
                    self.fail("MaterialSelect", message);
                    return false;
                }
            }
        } else {
            false
        };
        let counter_decision = material_counter_decision(
            self.material_selected_count,
            displayed_selected_count,
            level_max_reached,
        );
        if confirm_pending_same_copy(
            counter_decision,
            &mut self.material_same_copy_pending,
            &mut self.material_same_copy_selected,
        ) {
            self.emit("MaterialSelect", "页面计数已确认同名 1 星副本选择成功");
        }
        match counter_decision {
            MaterialCounterDecision::Confirmed => {}
            MaterialCounterDecision::ClearAutomaticSelection => {
                self.emit(
                    "MaterialSelect",
                    &format!(
                        "检测到自动配置已预选 {displayed_selected_count}/20 张素材，先清除后按丸子策略重新选择"
                    ),
                );
                if !self.tap_probe_or_point(
                    "MaterialSelect",
                    "button_enhancement_ce_clean_all_select_ready",
                    MATERIAL_CLEAR_ALL_BUTTON,
                ) {
                    return false;
                }
                thread::sleep(Duration::from_millis(700));
                return true;
            }
            MaterialCounterDecision::AcceptLevelMax => {
                self.material_selected_count = displayed_selected_count;
                if self.strategy_stage == StrategyStage::PacketSelected
                    && !self.material_same_copy_selected
                {
                    self.fail(
                        "MaterialSelect",
                        "目标虽已达到等级上限，但没有选到同名 1 星礼装，拒绝制作经验包".into(),
                    );
                    return false;
                }
                self.emit(
                    "MaterialSelect",
                    &format!(
                        "选择第 {} 张素材时目标已达到等级上限；以页面实际 {displayed_selected_count}/20 为准，点击决定",
                        self.material_selected_count.saturating_add(1)
                    ),
                );
                if !self.tap_probe_or_point(
                    "MaterialSelect",
                    "button_enhancement_ce_select_exp_decide",
                    MATERIAL_DECIDE_BUTTON,
                ) {
                    return false;
                }
                self.materials_committed = true;
                thread::sleep(Duration::from_millis(850));
                return true;
            }
            MaterialCounterDecision::Mismatch => {
                self.fail(
                    "MaterialSelect",
                    format!(
                        "页面已选 {displayed_selected_count}/20，与程序记录 {}/20 不一致，拒绝继续",
                        self.material_selected_count
                    ),
                );
                return false;
            }
        }
        let desired_two_star = false;
        if self.filter_two_star_enabled != Some(desired_two_star) {
            self.filter_two_star_desired = desired_two_star;
            self.filter_configured = false;
            self.material_scroll_reset_needed = true;
            self.material_scroll_reset_attempts = 0;
            self.emit(
                "MaterialSelect",
                "打开素材筛选，只显示 1 星以选择唯一同名副本",
            );
            if !self.tap_at("MaterialSelect", FILTER_BUTTON) {
                return false;
            }
            thread::sleep(Duration::from_millis(800));
            return true;
        }
        if !self.order_configured {
            self.emit("MaterialSelect", "打开素材排序设置");
            if !self.tap_at("MaterialSelect", ORDER_BUTTON) {
                return false;
            }
            thread::sleep(Duration::from_millis(800));
            return true;
        }
        let desired_descending = material_selection_descending(self.strategy_stage);
        let descending = self.probe("button_enhancement_ce_select_ce_desc");
        if descending != desired_descending {
            self.emit("MaterialSelect", "切换为等级升序，从顶部开始选择强化素材");
            if !self.tap_at("MaterialSelect", ORDER_DIRECTION_BUTTON) {
                return false;
            }
            self.material_scroll_reset_needed = true;
            self.material_scroll_reset_attempts = 0;
            thread::sleep(Duration::from_millis(650));
            return true;
        }

        let grid = match self.sidecar().read_craft_essence_grid(
            None,
            "enhancement_ce/item_ce_bar_bronze",
            1920.0,
            ITEM_GRID_REGION,
            1.2,
        ) {
            Ok(grid) => grid,
            Err(err) => {
                self.fail("MaterialSelect", format!("读取礼装素材列表失败: {err}"));
                return false;
            }
        };
        if self.material_scroll_reset_needed {
            match scrollbar_reset_drag_y(
                grid.diagnostics.scrollbar_thumb_y,
                grid.diagnostics.scrollbar_thumb_top_y,
                grid.diagnostics.visible_cell_count,
                grid.diagnostics.grid_cell_count,
                self.material_scroll_reset_attempts,
            ) {
                Ok(None) => {
                    self.material_scroll_reset_needed = false;
                    self.material_scroll_reset_attempts = 0;
                    self.material_scrolls = 0;
                }
                Ok(Some(thumb_y)) => {
                    self.material_scroll_reset_attempts =
                        self.material_scroll_reset_attempts.saturating_add(1);
                    self.emit("MaterialSelect", "将素材列表复位到顶部");
                    if !self.swipe_at(
                        "MaterialSelect",
                        Point::new(LIST_SCROLLBAR_X, thumb_y),
                        Point::new(LIST_SCROLLBAR_X, LIST_SCROLLBAR_OVERSHOOT_Y),
                        520,
                    ) {
                        return false;
                    }
                    thread::sleep(Duration::from_millis(700));
                    return true;
                }
                Err(message) => {
                    self.fail("MaterialSelect", format!("{message}，拒绝继续选择素材"));
                    return false;
                }
            }
        }

        if !grid.found || grid.cells.iter().any(|cell| !cell.valid) {
            self.material_grid_read_failures = self.material_grid_read_failures.saturating_add(1);
            if self.material_grid_read_failures < GRID_READ_MAX_FAILURES {
                self.emit(
                    "MaterialSelect",
                    &format!(
                        "素材列表识别不稳定（无效 {} 张），原地重试",
                        grid.diagnostics.invalid_cell_count
                    ),
                );
                thread::sleep(Duration::from_millis(450));
                return true;
            }
            self.fail(
                "MaterialSelect",
                format!(
                    "素材列表存在无法安全识别的礼装（无效 {} 张）",
                    grid.diagnostics.invalid_cell_count
                ),
            );
            return false;
        }
        self.material_grid_read_failures = 0;

        if self.mode == CraftEssenceEnhancementMode::QpEfficient
            && self.strategy_stage == StrategyStage::SelectBomb
            && !self.initial_bombs_counted
        {
            self.completed_bombs = u8::try_from(
                grid.cells
                    .iter()
                    .filter(|cell| is_complete_locked_bomb(cell))
                    .count(),
            )
            .unwrap_or(TARGET_BOMB_COUNT)
            .min(TARGET_BOMB_COUNT);
            self.initial_bombs_counted = true;
            if self.completed_bombs >= TARGET_BOMB_COUNT {
                self.strategy_stage = StrategyStage::QpEfficientComplete;
                self.transition(LifecycleEvent::Finished);
                self.emit(
                    "QpEfficientComplete",
                    "已识别到 8 个锁定的 50 级丸子，节省 QP 策略完成；程序不会解锁任何礼装",
                );
                return false;
            }
            if self.completed_bombs > 0 {
                self.emit(
                    "CraftEssenceSelect",
                    &format!(
                        "已识别到 {} 个现成的 50 级丸子，继续补足到 {} 个",
                        self.completed_bombs, TARGET_BOMB_COUNT
                    ),
                );
            }
        }

        let signature = grid_signature(&grid.cells);
        let mut choices: Vec<&CraftEssenceGridCell> = Vec::new();
        if self.strategy_stage == StrategyStage::BombBaseSelected {
            for cell in &grid.cells {
                if self.material_selected_count + u8::try_from(choices.len()).unwrap_or(4) >= 4 {
                    break;
                }
                if !is_raw_food(cell) || !cell.same_as_target {
                    continue;
                }
                let key = format!("{signature}:{}:{}", cell.row, cell.col);
                if !self.material_seen_cells.contains(&key) {
                    choices.push(cell);
                }
            }
        } else if self.strategy_stage == StrategyStage::PacketSelected {
            let available = grid
                .cells
                .iter()
                .filter(|cell| {
                    let key = format!("{signature}:{}:{}", cell.row, cell.col);
                    !self.material_seen_cells.contains(&key)
                })
                .collect::<Vec<_>>();
            choices = plan_packet_materials(&available, self.material_same_copy_selected);
        } else if self.strategy_stage == StrategyStage::BombSelectedForFeed {
            let available = grid
                .cells
                .iter()
                .filter(|cell| {
                    let key = format!("{signature}:{}:{}", cell.row, cell.col);
                    !self.material_seen_cells.contains(&key)
                })
                .collect::<Vec<_>>();
            choices = if self.feed_inventory_packets {
                plan_inventory_packet_feed(&available)
            } else {
                plan_packet_feed(&available, &self.packet_feed_remaining)
            };
        }

        let required = match self.strategy_stage {
            StrategyStage::BombBaseSelected => 4,
            StrategyStage::PacketSelected => 1,
            StrategyStage::BombSelectedForFeed => {
                if self.feed_inventory_packets {
                    PACKET_BATCH_SIZE
                } else {
                    u8::try_from(self.packet_fingerprints.len()).unwrap_or(PACKET_BATCH_SIZE)
                }
            }
            _ => unreachable!(),
        };
        // Use one CV/grid read to click the current page's whole safe batch, then
        // reconcile against the game's displayed N/20 counter on the next tick.
        // The single-copy packet break stays capped at exactly one material.
        choices.truncate(material_batch_click_limit(
            self.strategy_stage,
            self.material_selected_count,
            required,
        ));
        let material_tapped = !choices.is_empty();
        for cell in &choices {
            let key = format!("{signature}:{}:{}", cell.row, cell.col);
            self.material_seen_cells.insert(key);
            let point = Point::new(
                cell.region.x + cell.region.w / 2.0,
                cell.region.y + cell.region.h / 2.0,
            );
            if !self.tap_at("MaterialSelect", point) {
                return false;
            }
            if self.strategy_stage == StrategyStage::PacketSelected
                && !self.material_same_copy_selected
                && cell.same_as_target
            {
                self.material_same_copy_pending = true;
            }
            if self.strategy_stage == StrategyStage::BombSelectedForFeed {
                if !self.feed_inventory_packets {
                    let Some(index) = self.packet_feed_remaining.iter().position(|fingerprint| {
                        same_art_fingerprint(fingerprint, &cell.art_fingerprint)
                    }) else {
                        self.fail(
                            "MaterialSelect",
                            "经验包批次记录与待选素材不一致，已停止".into(),
                        );
                        return false;
                    };
                    self.packet_feed_remaining.remove(index);
                }
            }
            self.material_selected_count = self.material_selected_count.saturating_add(1);
            thread::sleep(Duration::from_millis(180));
        }

        if self.material_same_copy_pending {
            self.emit(
                "MaterialSelect",
                "已点击同名 1 星副本，等待下一帧页面计数确认",
            );
            return true;
        }

        if material_selection_ready_to_commit(
            self.material_selected_count,
            required,
            self.material_same_copy_pending,
        ) {
            let displayed_selected_count = match self.read_material_selected_count() {
                Ok(count) => count,
                Err(message) => {
                    self.fail("MaterialSelect", message);
                    return false;
                }
            };
            if displayed_selected_count != required {
                self.fail(
                    "MaterialSelect",
                    format!(
                        "点击决定前页面显示已选 {displayed_selected_count}/20，预期 {required}/20，拒绝继续"
                    ),
                );
                return false;
            }
            if self.strategy_stage == StrategyStage::PacketSelected
                && !self.material_same_copy_selected
            {
                self.fail(
                    "MaterialSelect",
                    "没有选到同名 1 星礼装，拒绝制作经验包".into(),
                );
                return false;
            }
            self.emit(
                "MaterialSelect",
                &format!("已安全选择 {required} 张素材，点击决定"),
            );
            if !self.tap_probe_or_point(
                "MaterialSelect",
                "button_enhancement_ce_select_exp_decide",
                MATERIAL_DECIDE_BUTTON,
            ) {
                return false;
            }
            self.materials_committed = true;
            thread::sleep(Duration::from_millis(850));
            return true;
        }
        if material_tapped {
            return true;
        }

        let material_at_bottom = self.probe("element_enhancement_ce_scroll_end");
        if inventory_feed_ready_at_bottom(
            self.feed_inventory_packets,
            self.material_selected_count,
            material_at_bottom,
        ) {
            let displayed_selected_count = match self.read_material_selected_count() {
                Ok(count) => count,
                Err(message) => {
                    self.fail("MaterialSelect", message);
                    return false;
                }
            };
            if displayed_selected_count != self.material_selected_count {
                self.fail(
                    "MaterialSelect",
                    format!(
                        "库存经验包选择结束时页面显示已选 {displayed_selected_count}/20，程序记录 {}/20，拒绝继续",
                        self.material_selected_count
                    ),
                );
                return false;
            }
            self.emit(
                "MaterialSelect",
                &format!(
                    "已到列表底部，安全选择了 {displayed_selected_count} 张未锁定、已升级的 1 星礼装，点击决定"
                ),
            );
            if !self.tap_probe_or_point(
                "MaterialSelect",
                "button_enhancement_ce_select_exp_decide",
                MATERIAL_DECIDE_BUTTON,
            ) {
                return false;
            }
            self.materials_committed = true;
            thread::sleep(Duration::from_millis(850));
            return true;
        }
        self.material_scrolls = next_page_scan_count(
            self.material_scrolls,
            MATERIAL_PAGE_MAX_SCROLLS,
            material_at_bottom,
        );
        if self.material_scrolls > MATERIAL_PAGE_MAX_SCROLLS {
            if material_at_bottom {
                self.emit("MaterialSelect", "已识别到礼装列表底部，不再继续滚动");
            }
            if self.strategy_stage == StrategyStage::BombSelectedForFeed
                && self.feed_inventory_packets
                && self.material_selected_count == 0
            {
                self.transition(LifecycleEvent::Finished);
                self.emit(
                    "MaterialSelect",
                    "没有可用的未锁定、已升级 1 星礼装；一星和二星素材均已消耗完",
                );
                return false;
            }
            self.fail(
                "MaterialSelect",
                match self.strategy_stage {
                    StrategyStage::BombBaseSelected => format!(
                        "只找到 {} / 4 张同名未锁定 1 星礼装，无法制作满破底卡",
                        self.material_selected_count
                    ),
                    StrategyStage::PacketSelected => format!(
                        "找不到同名未锁定 1 星副本：已选 {} / 1",
                        self.material_selected_count
                    ),
                    StrategyStage::BombSelectedForFeed => {
                        if self.feed_inventory_packets {
                            format!(
                                "只找到 {} 个未锁定、已升级的 1 星经验包",
                                self.material_selected_count
                            )
                        } else {
                            format!(
                                "只找到 {} / {} 个本批刚制作的至少 1 破 1 星经验包",
                                self.material_selected_count,
                                self.packet_fingerprints.len()
                            )
                        }
                    }
                    _ => unreachable!(),
                },
            );
            return false;
        }
        self.emit(
            "MaterialSelect",
            &format!(
                "当前已选 {} / {required}，向下查找更多素材",
                self.material_selected_count
            ),
        );
        if !self.swipe_at("MaterialSelect", LIST_SWIPE_FROM, LIST_SWIPE_TO, 520) {
            return false;
        }
        thread::sleep(Duration::from_millis(700));
        true
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

    fn handle_craft_essence_select(&mut self, descending: bool) -> bool {
        if self.target_tapped {
            self.target_return_waits += 1;
            if self.target_return_waits >= 6 {
                self.fail(
                    "CraftEssenceSelect",
                    "点击第一张概念礼装后未返回强化页面".into(),
                );
                return false;
            }
            thread::sleep(Duration::from_millis(700));
            return true;
        }
        if !self.density_checked {
            if !self.ensure_max_density() {
                return false;
            }
            self.density_checked = true;
        }
        if self.filter_two_star_enabled != Some(false) {
            self.filter_two_star_desired = false;
            self.filter_configured = false;
            self.target_scroll_reset_needed = true;
            self.target_scroll_reset_attempts = 0;
            self.emit("CraftEssenceSelect", "打开概念礼装筛选，只显示 1 星礼装");
            if self.tap_at("CraftEssenceSelect", FILTER_BUTTON) {
                thread::sleep(Duration::from_millis(800));
                return true;
            }
            return false;
        }
        if !self.order_configured {
            self.emit("CraftEssenceSelect", "打开概念礼装排序设置");
            if self.tap_at("CraftEssenceSelect", ORDER_BUTTON) {
                thread::sleep(Duration::from_millis(800));
                return true;
            }
            return false;
        }
        let desired_descending = target_selection_descending(self.strategy_stage);
        if descending != desired_descending {
            self.emit(
                "CraftEssenceSelect",
                if desired_descending {
                    "切换为等级降序，查找丸子"
                } else {
                    "切换为等级升序，查找 1 星经验包底卡"
                },
            );
            if self.tap_at("CraftEssenceSelect", ORDER_DIRECTION_BUTTON) {
                self.target_scroll_reset_needed = true;
                self.target_scroll_reset_attempts = 0;
                thread::sleep(Duration::from_millis(650));
                return true;
            }
            return false;
        }
        self.descending_checked = desired_descending;
        self.emit("CraftEssenceSelect", "读取礼装等级、上限、突破和锁定状态");
        let grid = match self.sidecar().read_craft_essence_grid(
            None,
            "enhancement_ce/item_ce_bar_bronze",
            1920.0,
            ITEM_GRID_REGION,
            1.2,
        ) {
            Ok(grid) => grid,
            Err(err) => {
                self.fail("CraftEssenceSelect", format!("概念礼装网格识别失败: {err}"));
                return false;
            }
        };
        if self.target_scroll_reset_needed {
            match scrollbar_reset_drag_y(
                grid.diagnostics.scrollbar_thumb_y,
                grid.diagnostics.scrollbar_thumb_top_y,
                grid.diagnostics.visible_cell_count,
                grid.diagnostics.grid_cell_count,
                self.target_scroll_reset_attempts,
            ) {
                Ok(None) => {
                    self.target_scroll_reset_needed = false;
                    self.target_scroll_reset_attempts = 0;
                    self.target_scrolls = 0;
                }
                Ok(Some(thumb_y)) => {
                    self.target_scroll_reset_attempts =
                        self.target_scroll_reset_attempts.saturating_add(1);
                    self.emit("CraftEssenceSelect", "将礼装列表复位到顶部");
                    if !self.swipe_at(
                        "CraftEssenceSelect",
                        Point::new(LIST_SCROLLBAR_X, thumb_y),
                        Point::new(LIST_SCROLLBAR_X, LIST_SCROLLBAR_OVERSHOOT_Y),
                        520,
                    ) {
                        return false;
                    }
                    thread::sleep(Duration::from_millis(700));
                    return true;
                }
                Err(message) => {
                    self.fail("CraftEssenceSelect", format!("{message}，拒绝继续选择礼装"));
                    return false;
                }
            }
        }

        if !grid.found || grid.cells.iter().any(|cell| !cell.valid) {
            self.target_grid_read_failures = self.target_grid_read_failures.saturating_add(1);
            if self.target_grid_read_failures < GRID_READ_MAX_FAILURES {
                self.emit(
                    "CraftEssenceSelect",
                    &format!(
                        "礼装列表识别不稳定（无效 {} 张），原地重试",
                        grid.diagnostics.invalid_cell_count
                    ),
                );
                thread::sleep(Duration::from_millis(450));
                return true;
            }
            self.fail(
                "CraftEssenceSelect",
                format!(
                    "礼装列表存在无法安全识别的卡片（无效 {} 张）",
                    grid.diagnostics.invalid_cell_count
                ),
            );
            return false;
        }
        self.target_grid_read_failures = 0;

        if self.strategy_stage == StrategyStage::InspectBomb {
            let inspected = grid
                .cells
                .iter()
                .filter(|cell| {
                    cell.valid
                        && cell.locked
                        && cell.rarity == Some(1)
                        && cell.level_cap == Some(50)
                        && same_art_fingerprint(
                            &cell.art_fingerprint,
                            &self.current_bomb_fingerprint,
                        )
                        && cell
                            .level
                            .is_some_and(|level| level >= self.current_bomb_level)
                })
                .max_by_key(|cell| cell.level.unwrap_or(0));
            if let Some(cell) = inspected {
                self.current_bomb_level = cell.level.unwrap_or(0);
                self.target_scrolls = 0;
                if self.current_bomb_level >= 50 {
                    self.completed_bombs = self.completed_bombs.saturating_add(1);
                    self.emit(
                        "CraftEssenceSelect",
                        &format!(
                            "第 {} / {} 个丸子已达到 50 级",
                            self.completed_bombs, TARGET_BOMB_COUNT
                        ),
                    );
                    if self.completed_bombs >= TARGET_BOMB_COUNT {
                        self.strategy_stage = StrategyStage::QpEfficientComplete;
                        self.transition(LifecycleEvent::Finished);
                        self.emit(
                            "QpEfficientComplete",
                            "8 个丸子已完成，节省 QP 策略结束；程序不会解锁任何礼装",
                        );
                        return false;
                    }
                    self.current_bomb_fingerprint.clear();
                    self.current_bomb_level = 0;
                    self.strategy_stage = StrategyStage::SelectBomb;
                } else {
                    self.strategy_stage = StrategyStage::SelectPacketBase;
                }
                thread::sleep(Duration::from_millis(250));
                return true;
            }
        }

        let candidate = match self.strategy_stage {
            StrategyStage::SelectBomb => choose_incomplete_bomb(&grid.cells),
            StrategyStage::SelectBombBase => choose_bomb_base(&grid.cells),
            StrategyStage::FindBombBaseToLock => grid.cells.iter().find(|cell| {
                cell.valid
                    && !cell.locked
                    && cell.rarity == Some(1)
                    && cell.level_cap == Some(50)
                    && cell.limit_breaks == Some(4)
                    && same_art_fingerprint(&cell.art_fingerprint, &self.current_bomb_fingerprint)
            }),
            StrategyStage::ExitBombBaseLockMode => grid.cells.iter().find(|cell| {
                is_incomplete_locked_bomb(cell)
                    && same_art_fingerprint(&cell.art_fingerprint, &self.current_bomb_fingerprint)
            }),
            StrategyStage::SelectPacketBase => choose_packet_base(&grid.cells),
            StrategyStage::SelectBombForTransfer => grid
                .cells
                .iter()
                .filter(|cell| {
                    is_incomplete_locked_bomb(cell)
                        && same_art_fingerprint(
                            &cell.art_fingerprint,
                            &self.current_bomb_fingerprint,
                        )
                        && cell
                            .level
                            .is_some_and(|level| level >= self.current_bomb_level)
                })
                .max_by_key(|cell| cell.level.unwrap_or(0)),
            _ => None,
        };

        let Some(candidate) = candidate else {
            let target_at_bottom = self.probe("element_enhancement_ce_scroll_end");
            self.target_scrolls = next_page_scan_count(
                self.target_scrolls,
                TARGET_PAGE_MAX_SCROLLS,
                target_at_bottom,
            );
            if self.target_scrolls > TARGET_PAGE_MAX_SCROLLS {
                if target_at_bottom {
                    self.emit("CraftEssenceSelect", "已识别到礼装列表底部，不再继续滚动");
                }
                if self.strategy_stage == StrategyStage::SelectBomb {
                    self.emit(
                        "CraftEssenceSelect",
                        "没有更多已锁定的未满级丸子底卡，开始制作新的 1 星满破底卡",
                    );
                    self.strategy_stage = stage_after_missing_bomb();
                    self.target_scrolls = 0;
                    self.target_scroll_reset_needed = true;
                    self.target_scroll_reset_attempts = 0;
                    return true;
                }
                if self.strategy_stage == StrategyStage::SelectPacketBase {
                    self.feed_inventory_packets = self.packet_fingerprints.is_empty();
                    let message = if self.feed_inventory_packets {
                        "没有更多同名 1/10 底卡，改用库存中未锁定、已升级的 1 星礼装强化丸子"
                            .to_string()
                    } else {
                        format!(
                            "本批已制作 {} 个经验包，未找到更多底卡，开始向丸子转移",
                            self.packet_fingerprints.len()
                        )
                    };
                    self.emit("CraftEssenceSelect", &message);
                    if packet_base_exhaustion_action(self.packet_fingerprints.len())
                        == PacketBaseExhaustionAction::ReturnToCurrentBomb
                    {
                        self.strategy_stage = StrategyStage::BombSelectedForFeed;
                        if !self.tap_at("CraftEssenceSelect", TARGET_LIST_CLOSE_BUTTON) {
                            return false;
                        }
                        thread::sleep(Duration::from_millis(850));
                        return true;
                    }
                    self.strategy_stage = StrategyStage::SelectBombForTransfer;
                    self.target_scrolls = 0;
                    self.target_scroll_reset_needed = true;
                    self.target_scroll_reset_attempts = 0;
                    return true;
                }
                self.fail(
                    "CraftEssenceSelect",
                    match self.strategy_stage {
                        StrategyStage::SelectBomb => {
                            "没有找到锁定、满破、未满 50 级的 1 星丸子底卡".into()
                        }
                        StrategyStage::SelectBombBase => {
                            "没有找到至少 5 张同名的未锁定 1 星 1 级礼装，无法制作新的满破底卡"
                                .into()
                        }
                        StrategyStage::FindBombBaseToLock => {
                            "无法重新找到刚制作的未锁定 1 星满破底卡".into()
                        }
                        StrategyStage::ExitBombBaseLockMode => {
                            "退出锁定模式后未确认刚制作的底卡已上锁".into()
                        }
                        StrategyStage::SelectBombForTransfer | StrategyStage::InspectBomb => {
                            "无法重新找到当前锁定的 1 星丸子，已停止以避免选错".into()
                        }
                        _ => "当前策略阶段无法选择礼装".into(),
                    },
                );
                return false;
            }
            self.emit("CraftEssenceSelect", "当前页没有合适目标，继续向下查找");
            if !self.swipe_at("CraftEssenceSelect", LIST_SWIPE_FROM, LIST_SWIPE_TO, 520) {
                return false;
            }
            thread::sleep(Duration::from_millis(700));
            return true;
        };

        let point = Point::new(
            candidate.region.x + candidate.region.w / 2.0,
            candidate.region.y + candidate.region.h / 2.0,
        );
        match self.strategy_stage {
            StrategyStage::SelectBomb => {
                self.current_bomb_fingerprint = candidate.art_fingerprint.clone();
                self.current_bomb_level = candidate.level.unwrap_or(0);
                self.enter_bomb_enhancement_strategy();
                self.emit(
                    "CraftEssenceSelect",
                    &format!(
                        "选择锁定的 1 星满破丸子（{}/50）{}",
                        self.current_bomb_level,
                        if self.mode == CraftEssenceEnhancementMode::Fast {
                            "，准备使用快速策略"
                        } else {
                            ""
                        }
                    ),
                );
            }
            StrategyStage::SelectBombBase => {
                self.current_bomb_fingerprint = candidate.art_fingerprint.clone();
                self.current_bomb_level = candidate.level.unwrap_or(1);
                self.strategy_stage = StrategyStage::BombBaseSelected;
                self.emit(
                    "CraftEssenceSelect",
                    "选择有 4 张以上同名副本的未锁定 1 星底卡",
                );
            }
            StrategyStage::FindBombBaseToLock => {
                self.lock_candidate_row = Some(candidate.row);
                self.lock_candidate_col = Some(candidate.col);
                self.lock_candidate_point = Some(point);
                self.strategy_stage = StrategyStage::LockBombBaseActive;
                self.target_return_waits = 0;
                self.emit(
                    "CraftEssenceSelect",
                    "已确认刚制作的满破底卡仍未锁定，进入统一锁定模式",
                );
                if !self.tap_at("CraftEssenceSelect", UNIFIED_LOCK_BUTTON) {
                    return false;
                }
                thread::sleep(Duration::from_millis(700));
                return true;
            }
            StrategyStage::ExitBombBaseLockMode => {
                self.current_bomb_level = candidate.level.unwrap_or(0);
                self.enter_bomb_enhancement_strategy();
                self.emit(
                    "CraftEssenceSelect",
                    &format!(
                        "已确认新底卡上锁，选择为当前丸子（{}/50）",
                        self.current_bomb_level
                    ),
                );
            }
            StrategyStage::SelectPacketBase => {
                self.packet_fingerprint = candidate.art_fingerprint.clone();
                self.strategy_stage = StrategyStage::PacketSelected;
                self.emit("CraftEssenceSelect", "选择未锁定的 1 星 1 级经验包底卡");
            }
            StrategyStage::SelectBombForTransfer => {
                self.current_bomb_level = candidate.level.unwrap_or(self.current_bomb_level);
                self.strategy_stage = StrategyStage::BombSelectedForFeed;
                self.emit(
                    "CraftEssenceSelect",
                    &format!("重新选择当前丸子（{}/50）", self.current_bomb_level),
                );
            }
            _ => unreachable!(),
        }
        self.target_scrolls = 0;
        if self.tap_at("CraftEssenceSelect", point) {
            self.target_tapped = true;
            thread::sleep(Duration::from_millis(900));
            return true;
        }
        false
    }

    fn enter_bomb_enhancement_strategy(&mut self) {
        if self.mode == CraftEssenceEnhancementMode::Fast {
            self.recommend_profile = RecommendMaterialProfile::OneAndTwoStar;
            self.recommend_configured_profile = None;
            self.recommend_reset_done = false;
            self.recommend_executed_for_target = false;
            self.recommend_execute_tapped = false;
            self.recommend_ready_waits = 0;
            self.recommend_open_attempts = 0;
        }
        self.strategy_stage = stage_after_bomb_selection(self.mode);
    }

    fn handle_filter_dialog(&mut self) -> bool {
        if !self.filter_scroll_reset_done {
            let thumb = match self.sidecar().find_element_by_name(
                None,
                SCREEN_NAME,
                "scroll_bar_enhancement_filter",
            ) {
                Ok(result) if result.found => Some(Point::new(result.x, result.y)),
                Ok(_) => None,
                Err(err) => {
                    self.fail("FilterDialog", format!("识别礼装筛选列表滚动条失败: {err}"));
                    return false;
                }
            }
            .filter(|point| {
                point.x.is_finite()
                    && point.y.is_finite()
                    && (0.0..=1.0).contains(&point.x)
                    && (0.0..=1.0).contains(&point.y)
            });
            let Some(from) = thumb else {
                self.fail("FilterDialog", "未识别到礼装筛选列表滚动条位置".into());
                return false;
            };
            let needs_drag = match filter_scrollbar_reset_needs_drag(
                from.y,
                self.filter_scroll_reset_attempts,
            ) {
                Ok(needs_drag) => needs_drag,
                Err(message) => {
                    self.fail("FilterDialog", message.into());
                    return false;
                }
            };
            if !needs_drag {
                self.filter_scroll_reset_done = true;
                self.emit("FilterDialog", "筛选列表滚动条已在顶部");
            } else {
                let message = if self.filter_scroll_reset_attempts == 0 {
                    "先将筛选列表滚动条拖到最顶端"
                } else {
                    "拖动后仍未到顶，再拖动一次"
                };
                self.emit("FilterDialog", message);
                self.filter_scroll_reset_attempts += 1;
                if !self.swipe_at("FilterDialog", from, FILTER_SCROLLBAR_TOP, 250) {
                    return false;
                }
                thread::sleep(Duration::from_millis(350));
                return true;
            }
        }

        if self.filter_configured {
            if self.tap_at("FilterDialog", FILTER_CONFIRM_BUTTON) {
                thread::sleep(Duration::from_millis(700));
                return true;
            }
            return false;
        }
        if !self.filter_reset_done {
            self.emit("FilterDialog", "恢复筛选初始设置");
            if self.tap_probe_or_point(
                "FilterDialog",
                "button_enhancement_ce_filter_init",
                Point::new(0.175, 0.881),
            ) {
                self.filter_reset_done = true;
                thread::sleep(Duration::from_millis(600));
                return true;
            }
            return false;
        }

        for filter in RARITY_FILTERS {
            let target_on = if filter.rarity == 2 {
                self.filter_two_star_desired
            } else {
                filter.target_on
            };
            let mean_luma = match self.sidecar().read_region_luma(None, filter.region) {
                Ok(mean_luma) => mean_luma,
                Err(err) => {
                    self.fail(
                        "FilterDialog",
                        format!("读取 {} 星筛选按钮颜色失败: {err}", filter.rarity),
                    );
                    return false;
                }
            };
            let current_state = classify_filter_toggle_luma(mean_luma);
            match current_state {
                FilterToggleState::On if target_on => continue,
                FilterToggleState::Off if !target_on => continue,
                FilterToggleState::On | FilterToggleState::Off => {
                    self.emit(
                        "FilterDialog",
                        &format!("调整 {} 星筛选状态", filter.rarity),
                    );
                    if self.tap_at("FilterDialog", filter.center()) {
                        thread::sleep(Duration::from_millis(450));
                        return true;
                    }
                    return false;
                }
                FilterToggleState::Ambiguous => {
                    self.fail(
                        "FilterDialog",
                        format!(
                            "无法明确识别 {} 星筛选开关颜色（平均亮度 {:.1}）",
                            filter.rarity, mean_luma
                        ),
                    );
                    return false;
                }
            }
        }

        self.filter_configured = true;
        self.filter_two_star_enabled = Some(self.filter_two_star_desired);
        self.emit("FilterDialog", "筛选状态已确认，保存设置");
        if self.tap_at("FilterDialog", FILTER_CONFIRM_BUTTON) {
            thread::sleep(Duration::from_millis(800));
            return true;
        }
        false
    }

    fn handle_recommend_material_empty_dialog(&mut self) -> bool {
        if !matches!(
            self.strategy_stage,
            StrategyStage::PacketAutoFeedPending | StrategyStage::FastAutoFeedPending
        ) {
            self.fail(
                "RecommendMaterialEmptyDialog",
                format!(
                    "当前策略阶段不允许处理推荐素材耗尽提示: {:?}",
                    self.strategy_stage
                ),
            );
            return false;
        }
        let exhausted_profile = self.recommend_profile;
        self.emit(
            "RecommendMaterialEmptyDialog",
            &format!("游戏确认没有可用的{}，关闭提示", exhausted_profile.label()),
        );
        if !self.tap_at("RecommendMaterialEmptyDialog", RECOMMEND_EMPTY_CLOSE_BUTTON) {
            return false;
        }
        self.recommend_execute_tapped = false;
        self.recommend_executed_for_target = false;
        self.recommend_ready_waits = 0;
        self.recommend_open_attempts = 0;
        thread::sleep(Duration::from_millis(800));

        if self.strategy_stage == StrategyStage::PacketAutoFeedPending
            && exhausted_profile == RecommendMaterialProfile::TwoStarOnly
        {
            self.recommend_profile = RecommendMaterialProfile::OneStarOnly;
            self.recommend_reset_done = false;
            self.emit(
                "RecommendMaterialEmptyDialog",
                "二星素材已耗尽，改为仅使用一星未强化礼装",
            );
            return true;
        }

        self.transition(LifecycleEvent::Finished);
        self.emit(
            "RecommendMaterialEmptyDialog",
            if self.strategy_stage == StrategyStage::FastAutoFeedPending {
                "没有可用的 1 星、2 星未强化素材，快速策略结束"
            } else {
                "一星和二星推荐素材均已耗尽，丸子制作结束"
            },
        );
        false
    }

    fn handle_recommend_material_dialog(&mut self) -> bool {
        if !matches!(
            self.strategy_stage,
            StrategyStage::PacketAutoFeedPending | StrategyStage::FastAutoFeedPending
        ) {
            self.fail(
                "RecommendMaterialDialog",
                format!("当前策略阶段不允许使用推荐选择: {:?}", self.strategy_stage),
            );
            return false;
        }
        if self.recommend_execute_tapped {
            self.recommend_ready_waits = self.recommend_ready_waits.saturating_add(1);
            if self.recommend_ready_waits >= RECOMMEND_READY_MAX_WAITS {
                self.fail(
                    "RecommendMaterialDialog",
                    "点击执行后推荐素材对话框仍未关闭".into(),
                );
                return false;
            }
            thread::sleep(Duration::from_millis(700));
            return true;
        }

        if !self.recommend_reset_done {
            self.emit("RecommendMaterialDialog", "初始化推荐素材筛选");
            if self.tap_at("RecommendMaterialDialog", RECOMMEND_INIT_BUTTON) {
                self.recommend_reset_done = true;
                thread::sleep(Duration::from_millis(600));
                return true;
            }
            return false;
        }

        for filter in RECOMMEND_FILTERS {
            let target_on = filter.target_on(self.recommend_profile);
            let mean_luma = match self.sidecar().read_region_luma(None, filter.region) {
                Ok(mean_luma) => mean_luma,
                Err(err) => {
                    self.fail(
                        "RecommendMaterialDialog",
                        format!("读取推荐素材“{}”颜色失败: {err}", filter.label),
                    );
                    return false;
                }
            };
            let current_state = classify_filter_toggle_luma(mean_luma);
            match current_state {
                FilterToggleState::On if target_on => continue,
                FilterToggleState::Off if !target_on => continue,
                FilterToggleState::On | FilterToggleState::Off => {
                    self.emit(
                        "RecommendMaterialDialog",
                        &format!("调整推荐素材“{}”筛选状态", filter.label),
                    );
                    if self.tap_at("RecommendMaterialDialog", filter.center()) {
                        thread::sleep(Duration::from_millis(450));
                        return true;
                    }
                    return false;
                }
                FilterToggleState::Ambiguous => {
                    self.fail(
                        "RecommendMaterialDialog",
                        format!(
                            "无法明确识别推荐素材“{}”开关颜色（平均亮度 {:.1}）",
                            filter.label, mean_luma
                        ),
                    );
                    return false;
                }
            }
        }

        let auto_color = match self
            .sidecar()
            .read_region_color(None, RECOMMEND_AUTO_CONFIG_REGION)
        {
            Ok(color) => color,
            Err(err) => {
                self.fail(
                    "RecommendMaterialDialog",
                    format!("读取自动配置开关颜色失败: {err}"),
                );
                return false;
            }
        };
        match classify_auto_config_saturation(auto_color.mean_saturation) {
            AutoConfigState::Off => {
                self.emit("RecommendMaterialDialog", "开启自动配置");
                if self.tap_at("RecommendMaterialDialog", RECOMMEND_AUTO_CONFIG_BUTTON) {
                    thread::sleep(Duration::from_millis(500));
                    return true;
                }
                false
            }
            AutoConfigState::On => {
                self.emit(
                    "RecommendMaterialDialog",
                    &format!("执行推荐素材选择：{}", self.recommend_profile.label()),
                );
                if self.tap_at("RecommendMaterialDialog", RECOMMEND_EXECUTE_BUTTON) {
                    self.recommend_execute_tapped = true;
                    self.recommend_executed_for_target = true;
                    self.recommend_configured_profile = Some(self.recommend_profile);
                    self.recommend_open_attempts = 0;
                    self.recommend_ready_waits = 0;
                    thread::sleep(Duration::from_millis(900));
                    return true;
                }
                false
            }
            AutoConfigState::Ambiguous => {
                self.fail(
                    "RecommendMaterialDialog",
                    format!(
                        "无法明确识别自动配置开关颜色（平均饱和度 {:.1}）",
                        auto_color.mean_saturation
                    ),
                );
                false
            }
        }
    }

    fn handle_order_dialog(&mut self) -> bool {
        if self.order_configured {
            if self.tap_at("OrderDialog", ORDER_CONFIRM_BUTTON) {
                thread::sleep(Duration::from_millis(700));
                return true;
            }
            return false;
        }
        if !self.order_level_selected {
            self.emit("OrderDialog", "设置为等级顺序");
            if self.tap_at("OrderDialog", ORDER_LEVEL_BUTTON) {
                self.order_level_selected = true;
                thread::sleep(Duration::from_millis(450));
                return true;
            }
            return false;
        }

        let on = self.probe_score("toggle_enhancement_ce_intelligent_order_on");
        let off = self.probe_score("toggle_enhancement_ce_intelligent_order_off");
        match decide_binary_control(on, off) {
            BinaryControlDecision::TargetConfirmed => {
                self.order_configured = true;
                self.emit("OrderDialog", "智能排序已开启，保存设置");
                if self.tap_at("OrderDialog", ORDER_CONFIRM_BUTTON) {
                    thread::sleep(Duration::from_millis(800));
                    return true;
                }
                false
            }
            BinaryControlDecision::Toggle => {
                self.emit("OrderDialog", "开启智能排序");
                if self.tap_probe_or_point(
                    "OrderDialog",
                    "toggle_enhancement_ce_intelligent_order_off",
                    Point::new(0.451, 0.658),
                ) {
                    thread::sleep(Duration::from_millis(500));
                    return true;
                }
                false
            }
            BinaryControlDecision::Ambiguous => {
                self.fail(
                    "OrderDialog",
                    format!("无法明确识别智能排序开关状态（开启分数 {on:.3}，关闭分数 {off:.3}）"),
                );
                false
            }
        }
    }

    fn ensure_max_density(&mut self) -> bool {
        self.emit("CraftEssenceSelect", "确认一屏最多显示模式");
        for taps_done in 0..=3 {
            match density_decision(self.probe("button_scale_level_3"), taps_done) {
                DensityDecision::Confirmed => return true,
                DensityDecision::Toggle => {
                    if !self.tap_at("CraftEssenceSelect", GRID_DENSITY_BUTTON) {
                        return false;
                    }
                    thread::sleep(Duration::from_millis(500));
                }
                DensityDecision::Failed => {
                    self.fail(
                        "CraftEssenceSelect",
                        "无法切换到一屏最多显示模式，button_scale_level_3 未命中".into(),
                    );
                    return false;
                }
            }
        }
        false
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
