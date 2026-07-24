use crate::adb::Adb;
use crate::runner::LogLevel;
use crate::screen::{NormRect, Point, SidecarClient};
use crate::touch::{self, TouchBackend};
use crate::Server;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::Emitter;

pub(crate) const EVENT_NAME: &str = "craft-essence-enhancement-automation-status";
const SCREEN_NAME: &str = "CraftEssenceEnhancement";
const TAP_JITTER_PX: i32 = 6;
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
const POST_ENHANCEMENT_NOT_READY_CONFIRMATIONS: u8 = 3;
const ENHANCEMENT_RETURN_MAX_WAITS: u8 = 40;
const ENHANCEMENT_MAIN_RETURN_CONFIRMATIONS: u8 = 2;
const ENHANCE_BUTTON_ABSENT_MAX_RECOVERY_TAPS: u8 = 8;

const TARGET_SELECT_BUTTON: Point = Point::new(0.153, 0.555);
const RECOMMEND_MATERIAL_BUTTON: Point = Point::new(0.846, 0.233);
const ENHANCE_BUTTON: Point = Point::new(0.896, 0.931);
const ENHANCE_CONFIRM_BUTTON: Point = Point::new(0.656, 0.819);
const ENHANCEMENT_SKIP_BUTTON: Point = Point::new(0.5, 0.055);
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

const RARITY_FILTERS: [RarityFilter; 5] = [
    RarityFilter::new(5, false, 0.231, 0.305, 0.030, 0.041),
    RarityFilter::new(4, false, 0.380, 0.305, 0.030, 0.041),
    RarityFilter::new(3, false, 0.525, 0.305, 0.030, 0.041),
    RarityFilter::new(2, true, 0.675, 0.305, 0.030, 0.041),
    RarityFilter::new(1, true, 0.820, 0.305, 0.030, 0.041),
];

const RECOMMEND_FILTERS: [RecommendFilter; 7] = [
    RecommendFilter::new("1 星", true, 0.225, 0.472, 0.025, 0.040),
    RecommendFilter::new("2 星", true, 0.363, 0.472, 0.025, 0.040),
    RecommendFilter::new("3 星", false, 0.505, 0.472, 0.025, 0.040),
    RecommendFilter::new("4 星", false, 0.637, 0.472, 0.025, 0.040),
    RecommendFilter::new("5 星", false, 0.775, 0.472, 0.025, 0.040),
    RecommendFilter::new("未强化", true, 0.225, 0.580, 0.025, 0.040),
    RecommendFilter::new("已强化", false, 0.363, 0.580, 0.025, 0.040),
];

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
    MaterialSelect,
    FilterDialog,
    OrderDialog,
    RecommendMaterialDialog,
    EnhancementConfirmDialog,
    EnhancementSuccess,
    Unknown,
}

#[derive(Debug, Default)]
pub(crate) struct ProbeSnapshot {
    found: Vec<&'static str>,
}

impl ProbeSnapshot {
    #[cfg(test)]
    fn from_keys(keys: &[&'static str]) -> Self {
        Self {
            found: keys.to_vec(),
        }
    }

    fn has(&self, key: &str) -> bool {
        self.found.iter().any(|found| *found == key)
    }
}

pub(crate) fn classify_screen(snapshot: &ProbeSnapshot) -> Screen {
    if snapshot.has("dialog_enhancement_ce_confirm") {
        return Screen::EnhancementConfirmDialog;
    }
    if snapshot.has("element_enhancement_ce_success") {
        return Screen::EnhancementSuccess;
    }
    if snapshot.has("dialog_enhancement_ce_recommend_material") {
        return Screen::RecommendMaterialDialog;
    }
    let select_mark = snapshot.has("button_enhancement_ce_select_ce_mark");
    if select_mark
        && snapshot.has("dialog_enhancement_ce_filter")
        && snapshot.has("button_enhancement_ce_filter_init")
    {
        return Screen::FilterDialog;
    }
    if select_mark && snapshot.has("dialog_enhancement_ce_order") {
        return Screen::OrderDialog;
    }
    if select_mark {
        if snapshot.has("button_enhancement_ce_clean_all_select")
            || snapshot.has("button_enhancement_ce_clean_all_select_ready")
        {
            return Screen::MaterialSelect;
        }
        return Screen::CraftEssenceSelect {
            descending: snapshot.has("button_enhancement_ce_select_ce_desc"),
        };
    }
    if snapshot.has("icon_enhancement_result") && snapshot.has("element_enhancement_ce_stripe") {
        return Screen::Main {
            target_selected: !snapshot.has("element_enhancement_new"),
            ready: snapshot.has("button_enhancement_ready"),
        };
    }
    Screen::Unknown
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TerminalOutcome {
    MaterialUnsupported,
}

fn terminal_outcome(screen: Screen) -> Option<TerminalOutcome> {
    match screen {
        Screen::MaterialSelect => Some(TerminalOutcome::MaterialUnsupported),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelectedMainAction {
    OpenRecommendation,
    WaitForAutoSelection,
    Enhance,
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EnhancementReadyState {
    Absent,
    NotReady,
    Ready,
    Transitioning,
}

fn classify_enhancement_button(score: f64, mean_luma: f64) -> EnhancementReadyState {
    if score < ENHANCE_BUTTON_PRESENT_MIN_SCORE {
        EnhancementReadyState::Absent
    } else if mean_luma <= ENHANCE_BUTTON_NOT_READY_MAX_LUMA {
        EnhancementReadyState::NotReady
    } else if mean_luma >= ENHANCE_BUTTON_READY_MIN_LUMA {
        EnhancementReadyState::Ready
    } else {
        EnhancementReadyState::Transitioning
    }
}

fn selected_main_action(
    ready: bool,
    recommendation_executed: bool,
    completed_enhancements: u32,
    not_ready_observations: u8,
) -> SelectedMainAction {
    if ready {
        return SelectedMainAction::Enhance;
    }
    if completed_enhancements > 0 {
        return if not_ready_observations >= POST_ENHANCEMENT_NOT_READY_CONFIRMATIONS {
            SelectedMainAction::Finished
        } else {
            SelectedMainAction::WaitForAutoSelection
        };
    }
    if recommendation_executed {
        return if not_ready_observations >= RECOMMEND_READY_MAX_WAITS {
            SelectedMainAction::Finished
        } else {
            SelectedMainAction::WaitForAutoSelection
        };
    }
    SelectedMainAction::OpenRecommendation
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EnhancementReturnAction {
    ObserveReturnedMain,
    WaitForConfirmationClose,
    TapSkip { mark_left_main: bool },
    Unexpected,
}

fn enhancement_return_action(screen: Screen, left_main: bool) -> EnhancementReturnAction {
    match screen {
        Screen::Main { .. } if left_main => EnhancementReturnAction::ObserveReturnedMain,
        Screen::EnhancementConfirmDialog => EnhancementReturnAction::WaitForConfirmationClose,
        Screen::Unknown | Screen::EnhancementSuccess => EnhancementReturnAction::TapSkip {
            mark_left_main: true,
        },
        Screen::Main { .. } => EnhancementReturnAction::TapSkip {
            mark_left_main: false,
        },
        _ => EnhancementReturnAction::Unexpected,
    }
}

fn enhancement_main_return_confirmed(observations: u8) -> bool {
    observations >= ENHANCEMENT_MAIN_RETURN_CONFIRMATIONS
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BinaryControlDecision {
    TargetConfirmed,
    Toggle,
    Ambiguous,
}

fn decide_binary_control(target_score: f64, opposite_score: f64) -> BinaryControlDecision {
    if target_score >= BINARY_CONTROL_MIN_SCORE
        && target_score >= opposite_score + BINARY_CONTROL_SCORE_MARGIN
    {
        BinaryControlDecision::TargetConfirmed
    } else if opposite_score >= BINARY_CONTROL_MIN_SCORE
        && opposite_score >= target_score + BINARY_CONTROL_SCORE_MARGIN
    {
        BinaryControlDecision::Toggle
    } else {
        BinaryControlDecision::Ambiguous
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FilterToggleState {
    Off,
    On,
    Ambiguous,
}

fn classify_filter_toggle_luma(mean_luma: f64) -> FilterToggleState {
    if mean_luma <= FILTER_TOGGLE_OFF_MAX_LUMA {
        FilterToggleState::Off
    } else if mean_luma >= FILTER_TOGGLE_ON_MIN_LUMA {
        FilterToggleState::On
    } else {
        FilterToggleState::Ambiguous
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AutoConfigState {
    Off,
    On,
    Ambiguous,
}

fn classify_auto_config_saturation(mean_saturation: f64) -> AutoConfigState {
    if mean_saturation <= AUTO_CONFIG_OFF_MAX_SATURATION {
        AutoConfigState::Off
    } else if mean_saturation >= AUTO_CONFIG_ON_MIN_SATURATION {
        AutoConfigState::On
    } else {
        AutoConfigState::Ambiguous
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DensityDecision {
    Confirmed,
    Toggle,
    Failed,
}

fn density_decision(level_three_found: bool, taps_done: u8) -> DensityDecision {
    if level_three_found {
        DensityDecision::Confirmed
    } else if taps_done < 3 {
        DensityDecision::Toggle
    } else {
        DensityDecision::Failed
    }
}

#[derive(Clone, Copy)]
struct Probe {
    key: &'static str,
    element: &'static str,
}

const PROBES: [Probe; 13] = [
    Probe::new("icon_enhancement_result"),
    Probe::new("element_enhancement_ce_stripe"),
    Probe::new("element_enhancement_new"),
    Probe::new("button_enhancement_ready"),
    Probe::new("button_enhancement_ce_select_ce_mark"),
    Probe::new("button_enhancement_ce_clean_all_select"),
    Probe::new("button_enhancement_ce_clean_all_select_ready"),
    Probe::new("dialog_enhancement_ce_filter"),
    Probe::new("button_enhancement_ce_filter_init"),
    Probe::new("dialog_enhancement_ce_order"),
    Probe::new("dialog_enhancement_ce_recommend_material"),
    Probe::new("dialog_enhancement_ce_confirm"),
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
    target_on: bool,
    region: NormRect,
}

impl RecommendFilter {
    const fn new(label: &'static str, target_on: bool, x: f64, y: f64, w: f64, h: f64) -> Self {
        Self {
            label,
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
    order_level_selected: bool,
    order_configured: bool,
    descending_checked: bool,
    target_tapped: bool,
    target_return_waits: u8,
    recommend_reset_done: bool,
    recommend_open_attempts: u8,
    recommend_execute_tapped: bool,
    recommend_ready_waits: u8,
    enhance_open_attempts: u8,
    awaiting_enhancement_return: bool,
    enhancement_left_main: bool,
    enhancement_return_waits: u8,
    enhancement_main_return_checks: u8,
    completed_enhancements: u32,
    post_enhancement_not_ready_checks: u8,
    enhance_button_absent_recovery_taps: u8,
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
    ) -> Self {
        let touch = touch::build(&adb);
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
            order_level_selected: false,
            order_configured: false,
            descending_checked: false,
            target_tapped: false,
            target_return_waits: 0,
            recommend_reset_done: false,
            recommend_open_attempts: 0,
            recommend_execute_tapped: false,
            recommend_ready_waits: 0,
            enhance_open_attempts: 0,
            awaiting_enhancement_return: false,
            enhancement_left_main: false,
            enhancement_return_waits: 0,
            enhancement_main_return_checks: 0,
            completed_enhancements: 0,
            post_enhancement_not_ready_checks: 0,
            enhance_button_absent_recovery_taps: 0,
        }
    }

    pub fn run(mut self) {
        self.transition(LifecycleEvent::WorkerStarted);
        self.emit("", "概念礼装强化自动化已启动");
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
                    &format!("未能识别当前页面，继续观察… ({unknown_count}/6)"),
                );
                if unknown_count >= 6 {
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

            match terminal_outcome(screen) {
                Some(TerminalOutcome::MaterialUnsupported) => {
                    self.fail(
                        "MaterialSelect",
                        "当前处于强化素材选择页面；MVP 暂不处理该页面，请返回概念礼装强化页面后重试".into(),
                    );
                    return;
                }
                None => {}
            }

            match screen {
                Screen::Main {
                    target_selected: true,
                    ready,
                } => {
                    if !self.handle_selected_main(ready) {
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
                Screen::EnhancementConfirmDialog => {
                    if !self.handle_enhancement_confirm_dialog() {
                        return;
                    }
                }
                Screen::EnhancementSuccess => {
                    self.awaiting_enhancement_return = true;
                    self.enhancement_left_main = true;
                    self.enhancement_return_waits = 0;
                    self.enhancement_main_return_checks = 0;
                    if !self.handle_enhancement_return(screen) {
                        return;
                    }
                }
                Screen::MaterialSelect => unreachable!(),
                Screen::Unknown => unreachable!(),
            }
        }
    }

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
        self.emit("EnhancementConfirmDialog", "确认执行概念礼装强化");
        if !self.tap_at("EnhancementConfirmDialog", ENHANCE_CONFIRM_BUTTON) {
            return false;
        }
        self.awaiting_enhancement_return = true;
        self.enhancement_left_main = false;
        self.enhancement_return_waits = 0;
        self.enhancement_main_return_checks = 0;
        thread::sleep(Duration::from_millis(900));
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
                self.awaiting_enhancement_return = false;
                self.enhancement_left_main = false;
                self.enhancement_return_waits = 0;
                self.enhancement_main_return_checks = 0;
                self.enhance_open_attempts = 0;
                self.completed_enhancements += 1;
                self.post_enhancement_not_ready_checks = 0;
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
        if !self.filter_configured {
            self.emit("CraftEssenceSelect", "打开概念礼装筛选设置");
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
        if !self.descending_checked {
            if descending {
                self.descending_checked = true;
            } else {
                self.emit("CraftEssenceSelect", "切换为降序排列");
                if self.tap_at("CraftEssenceSelect", ORDER_DIRECTION_BUTTON) {
                    thread::sleep(Duration::from_millis(650));
                    return true;
                }
                return false;
            }
        }
        self.emit("CraftEssenceSelect", "识别并选择列表中的第一张概念礼装");
        let grid = match self.sidecar().find_item_grid(
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
        let Some(first) = grid.grid_cells.first() else {
            self.fail(
                "CraftEssenceSelect",
                format!(
                    "未找到概念礼装网格: {}",
                    grid.diagnostics.fail_reason.as_deref().unwrap_or("unknown")
                ),
            );
            return false;
        };
        let point = Point::new(
            first.region.x + first.region.w / 2.0,
            first.region.y + first.region.h / 2.0,
        );
        if self.tap_at("CraftEssenceSelect", point) {
            self.target_tapped = true;
            thread::sleep(Duration::from_millis(900));
            return true;
        }
        false
    }

    fn handle_filter_dialog(&mut self) -> bool {
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
                FilterToggleState::On if filter.target_on => continue,
                FilterToggleState::Off if !filter.target_on => continue,
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
        self.emit("FilterDialog", "筛选状态已确认，保存设置");
        if self.tap_at("FilterDialog", FILTER_CONFIRM_BUTTON) {
            thread::sleep(Duration::from_millis(800));
            return true;
        }
        false
    }

    fn handle_recommend_material_dialog(&mut self) -> bool {
        if self.recommend_execute_tapped {
            self.recommend_ready_waits += 1;
            if self.recommend_ready_waits >= 8 {
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
                FilterToggleState::On if filter.target_on => continue,
                FilterToggleState::Off if !filter.target_on => continue,
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
                self.emit("RecommendMaterialDialog", "执行推荐素材选择");
                if self.tap_at("RecommendMaterialDialog", RECOMMEND_EXECUTE_BUTTON) {
                    self.recommend_execute_tapped = true;
                    self.post_enhancement_not_ready_checks = 0;
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

    fn detect_screen(&mut self) -> Screen {
        let mut snapshot = ProbeSnapshot::default();
        for probe in PROBES {
            if self.probe(probe.element) {
                snapshot.found.push(probe.key);
            }
        }
        if self.probe("button_enhancement_ce_select_ce_desc") {
            snapshot.found.push("button_enhancement_ce_select_ce_desc");
        }
        classify_screen(&snapshot)
    }

    fn probe(&mut self, element: &str) -> bool {
        self.sidecar()
            .find_element_by_name(None, SCREEN_NAME, element)
            .map(|result| result.found)
            .unwrap_or(false)
    }

    fn probe_score(&mut self, element: &str) -> f64 {
        self.sidecar()
            .find_element_by_name(None, SCREEN_NAME, element)
            .map(|result| result.score)
            .unwrap_or(0.0)
    }

    fn tap_probe_or_point(&mut self, screen: &str, element: &str, fallback: Point) -> bool {
        let point = self
            .sidecar()
            .find_element_by_name(None, SCREEN_NAME, element)
            .ok()
            .and_then(|result| result.found.then_some(Point::new(result.x, result.y)))
            .unwrap_or(fallback);
        self.tap_at(screen, point)
    }

    fn tap_at(&mut self, screen: &str, point: Point) -> bool {
        let (px, py) = point.to_physical(self.screen_w, self.screen_h);
        let (jx, jy) = jitter_offset();
        let x = (px as i32 + jx).clamp(0, self.screen_w.saturating_sub(1) as i32) as u32;
        let y = (py as i32 + jy).clamp(0, self.screen_h.saturating_sub(1) as i32) as u32;
        match self.touch.tap(x, y) {
            Ok(()) => true,
            Err(err) => {
                self.fail(screen, format!("点击失败: {err}"));
                false
            }
        }
    }

    fn sidecar(&mut self) -> &mut SidecarClient {
        self.sidecar
            .as_mut()
            .expect("CE enhancement sidecar missing")
    }

    fn transition(&self, event: LifecycleEvent) {
        let mut state = self.state.lock().unwrap();
        *state = lifecycle_transition(state.clone(), event);
    }

    fn fail(&self, screen: &str, message: String) {
        self.transition(LifecycleEvent::Failed {
            message: message.clone(),
        });
        self.emit(screen, &message);
    }

    fn emit(&self, screen: &str, message: &str) {
        let (state, status) = {
            let state = self.state.lock().unwrap();
            (format!("{:?}", *state), state.status())
        };
        let _ = self.app_handle.emit(
            EVENT_NAME,
            CraftEssenceEnhancementAutomationEvent {
                state,
                status,
                current_screen: screen.into(),
                message: message.into(),
                level: LogLevel::Info,
            },
        );
    }
}

impl Drop for CraftEssenceEnhancementRunner {
    fn drop(&mut self) {
        let Some(mut sidecar) = self.sidecar.take() else {
            return;
        };
        if let Err(err) = sidecar.stop_stream() {
            eprintln!("[mash-cv] stop CE enhancement stream failed: {err}");
        }
        if let Some(cache) = &self.sidecar_cache {
            let mut guard = cache.lock().unwrap();
            if guard.is_none() {
                *guard = Some(sidecar);
            }
        }
    }
}

fn jitter_offset() -> (i32, i32) {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.subsec_nanos())
        .unwrap_or(0);
    let span = (TAP_JITTER_PX * 2 + 1) as u32;
    (
        (nanos % span) as i32 - TAP_JITTER_PX,
        ((nanos / span) % span) as i32 - TAP_JITTER_PX,
    )
}

pub(crate) fn server_supported(server: Server) -> bool {
    matches!(server, Server::Cn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_main_status_combinations() {
        for (extra, selected, ready) in [
            (vec!["element_enhancement_new"], false, false),
            (vec![], true, false),
            (
                vec!["element_enhancement_new", "button_enhancement_ready"],
                false,
                true,
            ),
            (vec!["button_enhancement_ready"], true, true),
        ] {
            let mut keys = vec!["icon_enhancement_result", "element_enhancement_ce_stripe"];
            keys.extend(extra);
            assert_eq!(
                classify_screen(&ProbeSnapshot::from_keys(&keys)),
                Screen::Main {
                    target_selected: selected,
                    ready,
                }
            );
        }
    }

    #[test]
    fn distinguishes_target_material_and_dialog_states() {
        assert_eq!(
            classify_screen(&ProbeSnapshot::from_keys(&[
                "button_enhancement_ce_select_ce_mark"
            ])),
            Screen::CraftEssenceSelect { descending: false }
        );
        assert_eq!(
            classify_screen(&ProbeSnapshot::from_keys(&[
                "button_enhancement_ce_select_ce_mark",
                "button_enhancement_ce_clean_all_select"
            ])),
            Screen::MaterialSelect
        );
        assert_eq!(
            classify_screen(&ProbeSnapshot::from_keys(&[
                "button_enhancement_ce_select_ce_mark",
                "dialog_enhancement_ce_filter",
                "button_enhancement_ce_filter_init"
            ])),
            Screen::FilterDialog
        );
        assert_eq!(
            classify_screen(&ProbeSnapshot::from_keys(&[
                "button_enhancement_ce_select_ce_mark",
                "dialog_enhancement_ce_order"
            ])),
            Screen::OrderDialog
        );
        assert_eq!(
            classify_screen(&ProbeSnapshot::from_keys(&[
                "icon_enhancement_result",
                "element_enhancement_ce_stripe",
                "dialog_enhancement_ce_recommend_material"
            ])),
            Screen::RecommendMaterialDialog
        );
        assert_eq!(
            classify_screen(&ProbeSnapshot::from_keys(&[
                "icon_enhancement_result",
                "element_enhancement_ce_stripe",
                "button_enhancement_ready",
                "dialog_enhancement_ce_confirm"
            ])),
            Screen::EnhancementConfirmDialog
        );
        assert_eq!(
            classify_screen(&ProbeSnapshot::from_keys(&[
                "icon_enhancement_result",
                "element_enhancement_ce_stripe",
                "element_enhancement_ce_success"
            ])),
            Screen::EnhancementSuccess
        );
    }

    #[test]
    fn lifecycle_covers_start_finish_stop_and_failure() {
        let running = lifecycle_transition(
            CraftEssenceEnhancementRunnerState::Starting,
            LifecycleEvent::WorkerStarted,
        );
        assert_eq!(running, CraftEssenceEnhancementRunnerState::Running);
        assert_eq!(
            lifecycle_transition(running.clone(), LifecycleEvent::Finished),
            CraftEssenceEnhancementRunnerState::Finished
        );
        assert_eq!(
            lifecycle_transition(running, LifecycleEvent::StopRequested),
            CraftEssenceEnhancementRunnerState::Idle
        );
        assert_eq!(
            lifecycle_transition(
                CraftEssenceEnhancementRunnerState::Idle,
                LifecycleEvent::Failed {
                    message: "boom".into()
                }
            ),
            CraftEssenceEnhancementRunnerState::Error {
                message: "boom".into()
            }
        );
    }

    #[test]
    fn only_cn_server_is_supported() {
        assert!(server_supported(Server::Cn));
        assert!(!server_supported(Server::Jp));
    }

    #[test]
    fn entry_terminal_outcome_only_rejects_manual_material_selection() {
        assert_eq!(
            terminal_outcome(Screen::MaterialSelect),
            Some(TerminalOutcome::MaterialUnsupported)
        );
        assert_eq!(
            terminal_outcome(Screen::Main {
                target_selected: true,
                ready: true,
            }),
            None
        );
        assert_eq!(
            terminal_outcome(Screen::Main {
                target_selected: true,
                ready: false,
            }),
            None
        );
        assert_eq!(
            terminal_outcome(Screen::Main {
                target_selected: false,
                ready: true,
            }),
            None
        );
    }

    #[test]
    fn selected_main_repeats_ready_enhancement_and_stops_after_stable_not_ready() {
        assert_eq!(
            selected_main_action(true, true, 2, 0),
            SelectedMainAction::Enhance
        );
        assert_eq!(
            selected_main_action(false, true, 2, 1),
            SelectedMainAction::WaitForAutoSelection
        );
        assert_eq!(
            selected_main_action(false, true, 2, POST_ENHANCEMENT_NOT_READY_CONFIRMATIONS),
            SelectedMainAction::Finished
        );
    }

    #[test]
    fn selected_main_configures_recommendation_once_before_first_enhancement() {
        assert_eq!(
            selected_main_action(false, false, 0, 1),
            SelectedMainAction::OpenRecommendation
        );
        assert_eq!(
            selected_main_action(false, true, 0, RECOMMEND_READY_MAX_WAITS - 1),
            SelectedMainAction::WaitForAutoSelection
        );
        assert_eq!(
            selected_main_action(false, true, 0, RECOMMEND_READY_MAX_WAITS),
            SelectedMainAction::Finished
        );
    }

    #[test]
    fn enhancement_return_requires_leaving_main_before_accepting_main_again() {
        let main = Screen::Main {
            target_selected: true,
            ready: true,
        };
        assert_eq!(
            enhancement_return_action(main, false),
            EnhancementReturnAction::TapSkip {
                mark_left_main: false
            }
        );
        assert_eq!(
            enhancement_return_action(Screen::Unknown, false),
            EnhancementReturnAction::TapSkip {
                mark_left_main: true
            }
        );
        assert_eq!(
            enhancement_return_action(Screen::EnhancementSuccess, true),
            EnhancementReturnAction::TapSkip {
                mark_left_main: true
            }
        );
        assert_eq!(
            enhancement_return_action(main, true),
            EnhancementReturnAction::ObserveReturnedMain
        );
        assert_eq!(
            enhancement_return_action(Screen::EnhancementConfirmDialog, false),
            EnhancementReturnAction::WaitForConfirmationClose
        );
        assert_eq!(
            enhancement_return_action(Screen::RecommendMaterialDialog, true),
            EnhancementReturnAction::Unexpected
        );
        assert!(!enhancement_main_return_confirmed(1));
        assert!(enhancement_main_return_confirmed(2));
    }

    #[test]
    fn binary_controls_retry_only_clear_opposite_states_and_fail_ambiguously() {
        assert_eq!(
            decide_binary_control(0.96, 0.82),
            BinaryControlDecision::TargetConfirmed
        );
        assert_eq!(
            decide_binary_control(0.81, 0.97),
            BinaryControlDecision::Toggle
        );
        assert_eq!(
            decide_binary_control(0.93, 0.92),
            BinaryControlDecision::Ambiguous
        );
        assert_eq!(
            decide_binary_control(0.70, 0.69),
            BinaryControlDecision::Ambiguous
        );
    }

    #[test]
    fn density_allows_three_toggles_before_failing() {
        assert_eq!(density_decision(true, 0), DensityDecision::Confirmed);
        for taps_done in 0..3 {
            assert_eq!(density_decision(false, taps_done), DensityDecision::Toggle);
        }
        assert_eq!(density_decision(false, 3), DensityDecision::Failed);
    }

    #[test]
    fn filter_toggle_color_separates_blue_off_from_white_on() {
        assert_eq!(classify_filter_toggle_luma(106.8), FilterToggleState::Off);
        assert_eq!(classify_filter_toggle_luma(216.0), FilterToggleState::On);
        assert_eq!(
            classify_filter_toggle_luma(160.0),
            FilterToggleState::Ambiguous
        );
    }

    #[test]
    fn auto_config_color_separates_gray_off_from_blue_on() {
        assert_eq!(classify_auto_config_saturation(37.5), AutoConfigState::Off);
        assert_eq!(classify_auto_config_saturation(150.3), AutoConfigState::On);
        assert_eq!(
            classify_auto_config_saturation(90.0),
            AutoConfigState::Ambiguous
        );
    }

    #[test]
    fn enhancement_button_requires_shape_before_using_luma_state() {
        assert_eq!(
            classify_enhancement_button(0.585, 88.0),
            EnhancementReadyState::Absent
        );
        assert_eq!(
            classify_enhancement_button(0.929, 102.0),
            EnhancementReadyState::NotReady
        );
        assert_eq!(
            classify_enhancement_button(0.978, 165.0),
            EnhancementReadyState::Ready
        );
        assert_eq!(
            classify_enhancement_button(0.94, 135.0),
            EnhancementReadyState::Transitioning
        );
        assert_eq!(
            classify_enhancement_button(0.85, 165.0),
            EnhancementReadyState::Absent
        );
    }

    #[test]
    fn recommended_material_targets_only_low_rarity_and_unenhanced() {
        let selected = RECOMMEND_FILTERS
            .iter()
            .filter(|filter| filter.target_on)
            .map(|filter| filter.label)
            .collect::<Vec<_>>();
        assert_eq!(selected, vec!["1 星", "2 星", "未强化"]);
    }
}
