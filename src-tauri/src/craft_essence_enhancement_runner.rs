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

const TARGET_SELECT_BUTTON: Point = Point::new(0.153, 0.555);
const GRID_DENSITY_BUTTON: Point = Point::new(0.023, 0.938);
const FILTER_BUTTON: Point = Point::new(0.7635, 0.180);
const FILTER_CONFIRM_BUTTON: Point = Point::new(0.8235, 0.8855);
const ORDER_BUTTON: Point = Point::new(0.8795, 0.176);
const ORDER_LEVEL_BUTTON: Point = Point::new(0.255, 0.323);
const ORDER_CONFIRM_BUTTON: Point = Point::new(0.6735, 0.884);
const ORDER_DIRECTION_BUTTON: Point = Point::new(0.9748, 0.1833);
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
    Finished,
    MaterialUnsupported,
}

fn terminal_outcome(screen: Screen) -> Option<TerminalOutcome> {
    match screen {
        Screen::Main {
            target_selected: true,
            ..
        } => Some(TerminalOutcome::Finished),
        Screen::MaterialSelect => Some(TerminalOutcome::MaterialUnsupported),
        _ => None,
    }
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

const PROBES: [Probe; 10] = [
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
                Some(TerminalOutcome::Finished) => {
                    let Screen::Main { ready, .. } = screen else {
                        unreachable!()
                    };
                    self.transition(LifecycleEvent::Finished);
                    self.emit(
                        "CraftEssenceEnhancement",
                        if ready {
                            "已选择目标概念礼装并返回强化页面（强化按钮已就绪），本阶段完成"
                        } else {
                            "已选择目标概念礼装并返回强化页面，本阶段完成"
                        },
                    );
                    return;
                }
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
                Screen::Main {
                    target_selected: true,
                    ..
                }
                | Screen::MaterialSelect => unreachable!(),
                Screen::Unknown => unreachable!(),
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
    fn entry_terminal_outcomes_finish_selected_target_and_reject_materials() {
        assert_eq!(
            terminal_outcome(Screen::Main {
                target_selected: true,
                ready: false,
            }),
            Some(TerminalOutcome::Finished)
        );
        assert_eq!(
            terminal_outcome(Screen::MaterialSelect),
            Some(TerminalOutcome::MaterialUnsupported)
        );
        assert_eq!(
            terminal_outcome(Screen::Main {
                target_selected: false,
                ready: false,
            }),
            None
        );
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
}
