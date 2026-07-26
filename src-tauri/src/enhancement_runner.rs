use crate::adb::Adb;
use crate::runner::LogLevel;
use crate::screen::{ElementMatch, NormRect, OcrFragment, OcrRegionResult, Point, SidecarClient};
use crate::Server;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tauri::Emitter;
use unicode_normalization::UnicodeNormalization;

const ENHANCEMENT_EVENT_NAME: &str = "enhancement-automation-status";

const LEVEL_REGION: NormRect = NormRect {
    x: 0.313,
    y: 0.611,
    w: 0.332,
    h: 0.208,
};
const LEVEL_DIGIT_REGION: NormRect = NormRect {
    x: 0.345,
    y: 0.626,
    w: 0.120,
    h: 0.075,
};
const FILTER_DIALOG_REGION: NormRect = NormRect {
    x: 0.14,
    y: 0.10,
    w: 0.74,
    h: 0.80,
};
const DIALOG_CLASSIFIER_REGION: NormRect = NormRect {
    x: 0.18,
    y: 0.14,
    w: 0.64,
    h: 0.70,
};
const ASCENSION_ENTRY_OCR_REGION: NormRect = NormRect {
    x: 0.66,
    y: 0.71,
    w: 0.30,
    h: 0.14,
};
const MATERIAL_LIST_REGION: NormRect = NormRect {
    x: 0.00,
    y: 0.16,
    w: 0.82,
    h: 0.72,
};
const MATERIAL_COUNTER_REGION: NormRect = NormRect {
    x: 0.33,
    y: 0.13,
    w: 0.22,
    h: 0.10,
};
const ASCENSION_RESULT_REGION: NormRect = NormRect {
    x: 0.54,
    y: 0.24,
    w: 0.36,
    h: 0.28,
};

const SCREEN_MAIN: &str = "Main";
const SCREEN_ENHANCEMENT: &str = "Enhancement";
const SCREEN_SERVANT_ENHANCEMENT: &str = "ServantEnhancement";
const SCREEN_ASCENSION: &str = "Ascension";

const PROBE_BUTTON_NOTIFICATION: TemplateProbe = TemplateProbe {
    key: "button_notification",
    screen: SCREEN_MAIN,
    element: "button_notification",
};
const PROBE_TEXT_ENHANCEMENT: TemplateProbe = TemplateProbe {
    key: "text_enhancement",
    screen: SCREEN_ENHANCEMENT,
    element: "text_enhancement",
};
const PROBE_TEXT_ENHANCEMENT_SERVANT: TemplateProbe = TemplateProbe {
    key: "text_enhancement_servant",
    screen: SCREEN_SERVANT_ENHANCEMENT,
    element: "text_enhancement_servant",
};
const PROBE_SCREEN_ASCENSION: TemplateProbe = TemplateProbe {
    key: "screen_enhancement_ascension",
    screen: SCREEN_ASCENSION,
    element: "screen_enhancement_ascension",
};
const PROBE_BUTTON_MENU: TemplateProbe = TemplateProbe {
    key: "button_menu",
    screen: SCREEN_MAIN,
    element: "button_menu",
};
const PROBE_BUTTON_ENHANCEMENT: TemplateProbe = TemplateProbe {
    key: "button_enhancement",
    screen: SCREEN_MAIN,
    element: "button_enhancement",
};
const PROBE_TEXT_ENHANCEMENT_RESULT: TemplateProbe = TemplateProbe {
    key: "text_enhancement_result",
    screen: SCREEN_SERVANT_ENHANCEMENT,
    element: "text_enhancement_result",
};
const PROBE_TEXT_ENHANCEMENT_SERVANT_SELECT: TemplateProbe = TemplateProbe {
    key: "text_enhancement_servant_select",
    screen: SCREEN_SERVANT_ENHANCEMENT,
    element: "text_enhancement_servant_select",
};
const PROBE_TEXT_ENHANCEMENT_MATERIAL: TemplateProbe = TemplateProbe {
    key: "text_enhancement_material",
    screen: SCREEN_SERVANT_ENHANCEMENT,
    element: "text_enhancement_material",
};
const PROBE_DIALOG_FILTER_SETTING: TemplateProbe = TemplateProbe {
    key: "dialog_filter_setting",
    screen: SCREEN_SERVANT_ENHANCEMENT,
    element: "dialog_filter_setting",
};
const PROBE_TEXT_FILTER_SETTING_TYPE: TemplateProbe = TemplateProbe {
    key: "text_filter_setting_type",
    screen: SCREEN_SERVANT_ENHANCEMENT,
    element: "text_filter_setting_type",
};
const PROBE_BUTTON_SCALE_LEVEL_3: TemplateProbe = TemplateProbe {
    key: "button_scale_level_3",
    screen: SCREEN_SERVANT_ENHANCEMENT,
    element: "button_scale_level_3",
};
const PROBE_TEXT_ASCENSION_MAIN_VARIANT: TemplateProbe = TemplateProbe {
    key: "text_ascension_main_variant",
    screen: SCREEN_ASCENSION,
    element: "text_ascension_main_variant",
};
const PROBE_TEXT_ASCENSION_SERVANT_SELECT: TemplateProbe = TemplateProbe {
    key: "text_enhancement_ascension_servant_select",
    screen: SCREEN_ASCENSION,
    element: "text_enhancement_ascension_servant_select",
};
const PROBE_ASCENSION_NOT_READY: TemplateProbe = TemplateProbe {
    key: "enhancement_ascension_not_ready",
    screen: SCREEN_ASCENSION,
    element: "enhancement_ascension_not_ready",
};
const PROBE_BUTTON_ASCENSION_TO_SERVANT: TemplateProbe = TemplateProbe {
    key: "button_enhancement_ascension_to_servant",
    screen: SCREEN_ASCENSION,
    element: "button_enhancement_ascension_to_servant",
};

const UNKNOWN_DIAGNOSTIC_PROBES: [TemplateProbe; 15] = [
    PROBE_BUTTON_NOTIFICATION,
    PROBE_BUTTON_ENHANCEMENT,
    PROBE_TEXT_ENHANCEMENT,
    PROBE_TEXT_ENHANCEMENT_SERVANT,
    PROBE_TEXT_ENHANCEMENT_RESULT,
    PROBE_TEXT_ENHANCEMENT_SERVANT_SELECT,
    PROBE_TEXT_ENHANCEMENT_MATERIAL,
    PROBE_DIALOG_FILTER_SETTING,
    PROBE_TEXT_FILTER_SETTING_TYPE,
    PROBE_SCREEN_ASCENSION,
    PROBE_TEXT_ASCENSION_MAIN_VARIANT,
    PROBE_TEXT_ASCENSION_SERVANT_SELECT,
    PROBE_ASCENSION_NOT_READY,
    PROBE_BUTTON_SCALE_LEVEL_3,
    PROBE_BUTTON_ASCENSION_TO_SERVANT,
];

const HOME_MENU_BUTTON: Point = Point::new(0.926, 0.903);
const HOME_STRENGTHEN_BUTTON: Point = Point::new(0.371, 0.840);
const ENHANCE_SERVANT_ENTRY_BUTTON: Point = Point::new(0.730, 0.203);
const SERVANT_SELECT_BUTTON: Point = Point::new(0.152, 0.542);
const MATERIAL_SELECT_BUTTON: Point = Point::new(0.340, 0.326);
const ENHANCE_CONFIRM_BUTTON: Point = Point::new(0.895, 0.934);
const ASCENSION_ENTRY_BUTTON: Point = Point::new(0.820, 0.778);
const FILTER_BUTTON: Point = Point::new(0.760, 0.177);
const FILTER_CONFIRM_BUTTON: Point = Point::new(0.820, 0.885);
const GRID_DENSITY_BUTTON: Point = Point::new(0.023, 0.938);
const MATERIAL_CONFIRM_BUTTON: Point = Point::new(0.898, 0.917);
const DIALOG_CONFIRM_BUTTON: Point = Point::new(0.652, 0.826);
const PAGE_CENTER_TAP: Point = Point::new(0.500, 0.500);
const ASCENSION_RESULT_RETURN_BUTTON: Point = Point::new(0.813, 0.396);
const MATERIAL_DRAG_FROM: Point = Point::new(0.104, 0.351);
const MATERIAL_DRAG_TO: Point = Point::new(0.725, 0.747);
const SERVANT_LIST_SCROLL_FROM: Point = Point::new(0.500, 0.780);
const SERVANT_LIST_SCROLL_TO: Point = Point::new(0.500, 0.300);
const FILTER_SCROLL_FROM: Point = Point::new(0.780, 0.780);
const FILTER_SCROLL_TO: Point = Point::new(0.780, 0.360);

pub(crate) const SERVANT_LIST_REGION: NormRect = NormRect {
    x: 0.055,
    y: 0.251,
    w: 0.755,
    h: 0.747,
};
pub(crate) const SERVANT_FACE_MATCH_CROP: NormRect = NormRect {
    x: 0.195,
    y: 0.265,
    w: 0.805,
    h: 0.438,
};
pub(crate) const SERVANT_FACE_TEMPLATE_SIZE: (u32, u32) = (223, 223);

const MATERIAL_GRID_POINTS: [Point; 20] = [
    Point::new(0.104, 0.351),
    Point::new(0.207, 0.351),
    Point::new(0.311, 0.351),
    Point::new(0.414, 0.351),
    Point::new(0.518, 0.351),
    Point::new(0.621, 0.351),
    Point::new(0.725, 0.351),
    Point::new(0.104, 0.549),
    Point::new(0.207, 0.549),
    Point::new(0.311, 0.549),
    Point::new(0.414, 0.549),
    Point::new(0.518, 0.549),
    Point::new(0.621, 0.549),
    Point::new(0.725, 0.549),
    Point::new(0.104, 0.747),
    Point::new(0.207, 0.747),
    Point::new(0.311, 0.747),
    Point::new(0.414, 0.747),
    Point::new(0.518, 0.747),
    Point::new(0.621, 0.747),
];

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnhancementConfig {
    pub target_servant_id: u32,
    pub target_servant_variant_key: String,
}

#[derive(Debug, Clone)]
pub struct EnhancementTarget {
    pub id: u32,
    pub variant_key: String,
    pub name_jp: String,
    pub class_name: String,
    pub rarity: u32,
    pub face_template_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum EnhancementRunnerState {
    Idle,
    Starting,
    Running,
    Finished,
    Error { message: String },
}

impl EnhancementRunnerState {
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
pub(crate) enum EnhancementLifecycleEvent {
    WorkerStarted,
    StopRequested,
    Finished,
    Failed { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EnhancementLifecycleTransition {
    pub(crate) previous: EnhancementRunnerState,
    pub(crate) event: EnhancementLifecycleEvent,
    pub(crate) next: EnhancementRunnerState,
    pub(crate) accepted: bool,
}

pub(crate) fn enhancement_lifecycle_transition(
    state: EnhancementRunnerState,
    event: EnhancementLifecycleEvent,
) -> EnhancementLifecycleTransition {
    let next = match (&state, &event) {
        (EnhancementRunnerState::Starting, EnhancementLifecycleEvent::WorkerStarted) => {
            Some(EnhancementRunnerState::Running)
        }
        (
            EnhancementRunnerState::Starting | EnhancementRunnerState::Running,
            EnhancementLifecycleEvent::StopRequested,
        ) => Some(EnhancementRunnerState::Idle),
        (EnhancementRunnerState::Running, EnhancementLifecycleEvent::Finished) => {
            Some(EnhancementRunnerState::Finished)
        }
        (_, EnhancementLifecycleEvent::Failed { message }) => Some(EnhancementRunnerState::Error {
            message: message.clone(),
        }),
        _ => None,
    };

    let accepted = next.is_some();
    EnhancementLifecycleTransition {
        previous: state.clone(),
        event,
        next: next.unwrap_or(state),
        accepted,
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnhancementAutomationEvent {
    pub state: String,
    pub status: &'static str,
    pub current_screen: String,
    pub message: String,
    pub level: LogLevel,
}

pub struct EnhancementRunnerHandle {
    pub state: Arc<Mutex<EnhancementRunnerState>>,
    pub cancel: Arc<AtomicBool>,
}

impl EnhancementRunnerHandle {
    pub fn new_idle() -> Self {
        Self {
            state: Arc::new(Mutex::new(EnhancementRunnerState::Idle)),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EnhancementScreen {
    HomeMenuClosed,
    HomeMenuOpen,
    EnhanceMenu,
    ServantEnhance,
    ServantSelect,
    FilterDialog,
    MaterialSelect,
    ConfirmDialog,
    Ascension,
    AscensionNotReady,
    AscensionResult,
    ProfileUpdateDialog,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EnhancementTopScreen {
    Main,
    Enhancement,
    ServantEnhancement,
    Ascension,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EnhancementVariant {
    Main,
    ServantSelect,
    MaterialSelect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EnhancementStatus {
    None,
    MenuOpen,
    FilterDialogOpen,
    NotReady,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EnhancementRoute {
    screen: EnhancementTopScreen,
    variant: EnhancementVariant,
    status: EnhancementStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingConfirm {
    Enhancement,
    Ascension,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScaleLevel3Decision {
    Done,
    TapAndRetry,
    Fail,
}

#[derive(Debug, Clone, Copy)]
struct TemplateProbe {
    key: &'static str,
    screen: &'static str,
    element: &'static str,
}

#[derive(Debug, Clone)]
struct TemplateProbeResult {
    probe: TemplateProbe,
    found: bool,
    score: f64,
    center: Option<Point>,
    region: Option<NormRect>,
    error: Option<String>,
}

struct NamedOcr {
    name: &'static str,
    result: OcrRegionResult,
}

pub struct EnhancementRunner {
    adb: Adb,
    sidecar: Option<SidecarClient>,
    sidecar_cache: Option<Arc<Mutex<Option<SidecarClient>>>>,
    app_handle: tauri::AppHandle,
    state: Arc<Mutex<EnhancementRunnerState>>,
    cancel: Arc<AtomicBool>,
    screen_w: u32,
    screen_h: u32,
    target: EnhancementTarget,
    pending_confirm: Option<PendingConfirm>,
    exp_materials_verified: bool,
    servant_select_density_checked: bool,
}

impl EnhancementRunner {
    pub fn new(
        adb: Adb,
        sidecar: SidecarClient,
        app_handle: tauri::AppHandle,
        state: Arc<Mutex<EnhancementRunnerState>>,
        cancel: Arc<AtomicBool>,
        screen_size: (u32, u32),
        target: EnhancementTarget,
        sidecar_cache: Option<Arc<Mutex<Option<SidecarClient>>>>,
    ) -> Self {
        Self {
            adb,
            sidecar: Some(sidecar),
            sidecar_cache,
            app_handle,
            state,
            cancel,
            screen_w: screen_size.0,
            screen_h: screen_size.1,
            target,
            pending_confirm: None,
            exp_materials_verified: false,
            servant_select_density_checked: false,
        }
    }

    pub fn run(mut self) {
        self.transition_lifecycle(EnhancementLifecycleEvent::WorkerStarted);
        self.emit(
            "",
            &format!(
                "强化自动化已启动，目标: {} / {} / {}星 / {}",
                self.target.name_jp,
                self.target.class_name,
                self.target.rarity,
                self.target.variant_key
            ),
        );

        let mut unknown_count = 0_u32;
        loop {
            if self.is_cancelled() {
                self.transition_lifecycle(EnhancementLifecycleEvent::StopRequested);
                self.emit("", "强化自动化已停止");
                return;
            }

            let (screen, ocr) = match self.detect_screen() {
                Ok(result) => result,
                Err(err) => {
                    self.fail("", format!("画面识别失败: {err}"));
                    return;
                }
            };

            if screen != EnhancementScreen::ServantSelect {
                self.servant_select_density_checked = false;
            }

            match screen {
                EnhancementScreen::HomeMenuClosed => {
                    unknown_count = 0;
                    self.handle_home_menu_closed();
                }
                EnhancementScreen::HomeMenuOpen => {
                    unknown_count = 0;
                    self.handle_home_menu_open();
                }
                EnhancementScreen::EnhanceMenu => {
                    unknown_count = 0;
                    self.handle_enhance_menu();
                }
                EnhancementScreen::ServantEnhance => {
                    unknown_count = 0;
                    self.handle_servant_enhance(&ocr);
                }
                EnhancementScreen::ServantSelect => {
                    unknown_count = 0;
                    self.handle_servant_select(&ocr);
                }
                EnhancementScreen::FilterDialog => {
                    unknown_count = 0;
                    self.handle_filter_dialog(&ocr);
                }
                EnhancementScreen::MaterialSelect => {
                    unknown_count = 0;
                    self.handle_material_select(&ocr);
                }
                EnhancementScreen::ConfirmDialog => {
                    unknown_count = 0;
                    self.handle_confirm_dialog(&ocr);
                }
                EnhancementScreen::Ascension => {
                    unknown_count = 0;
                    self.handle_ascension();
                }
                EnhancementScreen::AscensionNotReady => {
                    unknown_count = 0;
                    self.handle_ascension_not_ready();
                }
                EnhancementScreen::AscensionResult => {
                    unknown_count = 0;
                    self.handle_ascension_result();
                }
                EnhancementScreen::ProfileUpdateDialog => {
                    unknown_count = 0;
                    self.handle_profile_update_dialog(&ocr);
                }
                EnhancementScreen::Unknown => {
                    unknown_count += 1;
                    self.emit(
                        "Unknown",
                        &format!(
                            "未能识别当前页面，继续观察… ({unknown_count}/6) OCR={}",
                            summarize_ocr(&ocr)
                        ),
                    );
                    if unknown_count >= 6 {
                        self.fail(
                            "Unknown",
                            format!(
                                "连续多次无法识别当前页面，请提供当前截图。最后一次 OCR={}",
                                summarize_ocr(&ocr)
                            ),
                        );
                        return;
                    }
                    thread::sleep(Duration::from_millis(800));
                }
            }
        }
    }

    fn transition_lifecycle(
        &self,
        event: EnhancementLifecycleEvent,
    ) -> EnhancementLifecycleTransition {
        let mut state = self.state.lock().unwrap();
        let transition = enhancement_lifecycle_transition(state.clone(), event);
        if transition.accepted {
            *state = transition.next.clone();
        }
        transition
    }

    fn emit(&self, screen: &str, message: &str) {
        self.emit_with_level(screen, message, LogLevel::Info);
    }

    /// Debug-level counterpart to [`emit`]; see `Runner::emit_debug` for
    /// the full rationale. Use sparingly for technical diagnostics that
    /// don't belong in the user-facing operation log.
    #[allow(dead_code)]
    fn emit_debug(&self, screen: &str, message: &str) {
        self.emit_with_level(screen, message, LogLevel::Debug);
    }

    fn emit_with_level(&self, screen: &str, message: &str, level: LogLevel) {
        let (state_str, status) = {
            let s = self.state.lock().unwrap();
            (format!("{:?}", *s), s.status())
        };
        let _ = self.app_handle.emit(
            ENHANCEMENT_EVENT_NAME,
            EnhancementAutomationEvent {
                state: state_str,
                status,
                current_screen: screen.to_string(),
                message: message.to_string(),
                level,
            },
        );
    }

    fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    fn sidecar(&mut self) -> &mut SidecarClient {
        self.sidecar.as_mut().expect("enhancement sidecar missing")
    }

    fn fail(&self, screen: &str, message: String) {
        self.transition_lifecycle(EnhancementLifecycleEvent::Failed {
            message: message.clone(),
        });
        self.emit(screen, &message);
    }

    fn tap_at(&self, screen: &str, point: Point) -> bool {
        let (px, py) = point.to_physical(self.screen_w, self.screen_h);
        match self.adb.tap(px, py) {
            Ok(()) => true,
            Err(err) => {
                self.fail(screen, format!("点击失败: {err}"));
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
                self.fail(screen, format!("滑动失败: {err}"));
                false
            }
        }
    }

    fn tap_template_or_point(
        &mut self,
        screen: &str,
        probe: TemplateProbe,
        fallback: Point,
    ) -> bool {
        let result = self.probe_template(probe);
        if let Some(center) = result.center {
            return self.tap_at(screen, center);
        }
        self.emit(
            screen,
            &format!(
                "模板 {} 未命中，使用兼容坐标点击。{}",
                probe.key,
                result.summary()
            ),
        );
        self.tap_at(screen, fallback)
    }

    fn ocr_region(&mut self, region: NormRect) -> Result<OcrRegionResult, String> {
        self.sidecar().ocr_region(None, region)
    }

    fn probe_template(&mut self, probe: TemplateProbe) -> TemplateProbeResult {
        match self
            .sidecar()
            .find_element_by_name(None, probe.screen, probe.element)
        {
            Ok(result) => TemplateProbeResult::from_match(probe, result),
            Err(err) => TemplateProbeResult {
                probe,
                found: false,
                score: 0.0,
                center: None,
                region: None,
                error: Some(err),
            },
        }
    }

    fn detect_screen(&mut self) -> Result<(EnhancementScreen, OcrRegionResult), String> {
        let dialog = self.ocr_region(DIALOG_CLASSIFIER_REGION)?;
        let dialog_text = normalize_text(&dialog.full_text);
        if dialog_text.contains("プロフィール") && dialog_text.contains("閉じる") {
            return Ok((EnhancementScreen::ProfileUpdateDialog, dialog));
        }
        if dialog_text.contains("★4以上")
            || (dialog_text.contains("決定")
                && (dialog_text.contains("キャンセル") || dialog_text.contains("取消")))
        {
            return Ok((EnhancementScreen::ConfirmDialog, dialog));
        }

        let template_results = self.collect_template_probe_results();
        if let Some(route) = detect_enhancement_screen(&template_results) {
            return self.route_to_screen(route, &template_results);
        }

        let result = self.ocr_region(ASCENSION_RESULT_REGION)?;
        let result_text = normalize_text(&result.full_text);
        if contains_servant_word(&result_text) && result_text.contains("強化へ") {
            return Ok((EnhancementScreen::AscensionResult, result));
        }

        let unknown_summary = self.collect_unknown_region_summaries(&template_results)?;
        Ok((
            EnhancementScreen::Unknown,
            merge_unknown_diagnostics(&template_results, &unknown_summary),
        ))
    }

    fn collect_template_probe_results(&mut self) -> Vec<TemplateProbeResult> {
        UNKNOWN_DIAGNOSTIC_PROBES
            .into_iter()
            .map(|probe| self.probe_template(probe))
            .collect()
    }

    fn route_to_screen(
        &mut self,
        route: EnhancementRoute,
        probes: &[TemplateProbeResult],
    ) -> Result<(EnhancementScreen, OcrRegionResult), String> {
        let screen = match (route.screen, route.variant, route.status) {
            (EnhancementTopScreen::Main, EnhancementVariant::Main, EnhancementStatus::MenuOpen) => {
                EnhancementScreen::HomeMenuOpen
            }
            (EnhancementTopScreen::Main, EnhancementVariant::Main, _) => {
                EnhancementScreen::HomeMenuClosed
            }
            (EnhancementTopScreen::Enhancement, EnhancementVariant::Main, _) => {
                EnhancementScreen::EnhanceMenu
            }
            (EnhancementTopScreen::ServantEnhancement, EnhancementVariant::Main, _) => {
                EnhancementScreen::ServantEnhance
            }
            (EnhancementTopScreen::ServantEnhancement, EnhancementVariant::ServantSelect, _)
            | (EnhancementTopScreen::Ascension, EnhancementVariant::ServantSelect, _) => {
                EnhancementScreen::ServantSelect
            }
            (
                EnhancementTopScreen::ServantEnhancement,
                EnhancementVariant::MaterialSelect,
                EnhancementStatus::FilterDialogOpen,
            ) => EnhancementScreen::FilterDialog,
            (EnhancementTopScreen::ServantEnhancement, EnhancementVariant::MaterialSelect, _) => {
                EnhancementScreen::MaterialSelect
            }
            (
                EnhancementTopScreen::Ascension,
                EnhancementVariant::Main,
                EnhancementStatus::NotReady,
            ) => EnhancementScreen::AscensionNotReady,
            (EnhancementTopScreen::Ascension, EnhancementVariant::Main, _) => {
                EnhancementScreen::Ascension
            }
            _ => EnhancementScreen::Unknown,
        };

        let ocr = match screen {
            EnhancementScreen::ServantEnhance => self.ocr_region(ASCENSION_ENTRY_OCR_REGION)?,
            EnhancementScreen::ServantSelect => self.ocr_region(SERVANT_LIST_REGION)?,
            EnhancementScreen::MaterialSelect => {
                let material_list = self.ocr_region(MATERIAL_LIST_REGION)?;
                let material_counter = self.ocr_region(MATERIAL_COUNTER_REGION)?;
                merge_ocr_results(&[&material_list, &material_counter])
            }
            EnhancementScreen::FilterDialog
            | EnhancementScreen::ConfirmDialog
            | EnhancementScreen::ProfileUpdateDialog => self.ocr_region(FILTER_DIALOG_REGION)?,
            _ => probe_diagnostics_as_ocr(probes),
        };
        Ok((screen, ocr))
    }

    fn collect_unknown_region_summaries(
        &mut self,
        _template_results: &[TemplateProbeResult],
    ) -> Result<Vec<NamedOcr>, String> {
        Ok(vec![
            NamedOcr {
                name: "dialog",
                result: self.ocr_region(FILTER_DIALOG_REGION)?,
            },
            NamedOcr {
                name: "servantList",
                result: self.ocr_region(SERVANT_LIST_REGION)?,
            },
            NamedOcr {
                name: "materialList",
                result: self.ocr_region(MATERIAL_LIST_REGION)?,
            },
            NamedOcr {
                name: "level",
                result: self.ocr_region(LEVEL_REGION)?,
            },
        ])
    }

    fn handle_home_menu_closed(&mut self) {
        self.emit("Home", "主页 MENU 未展开，点击 MENU");
        if self.tap_template_or_point("Home", PROBE_BUTTON_MENU, HOME_MENU_BUTTON) {
            thread::sleep(Duration::from_millis(700));
        }
    }

    fn handle_home_menu_open(&mut self) {
        self.emit("HomeMenu", "进入强化菜单");
        if self.tap_template_or_point("HomeMenu", PROBE_BUTTON_ENHANCEMENT, HOME_STRENGTHEN_BUTTON)
        {
            thread::sleep(Duration::from_millis(900));
        }
    }

    fn handle_enhance_menu(&self) {
        self.emit("EnhanceMenu", "进入从者强化");
        if self.tap_at("EnhanceMenu", ENHANCE_SERVANT_ENTRY_BUTTON) {
            thread::sleep(Duration::from_millis(900));
        }
    }

    fn handle_servant_enhance(&mut self, ocr: &OcrRegionResult) {
        let level = match self.sidecar().read_level_digits(None, LEVEL_DIGIT_REGION) {
            Ok(v) => v,
            Err(err) => {
                self.fail("ServantEnhance", format!("模板读取等级失败: {err}"));
                return;
            }
        };
        if let (true, Some(current), Some(max)) = (level.found, level.current, level.max_level) {
            self.emit("ServantEnhance", &format!("level digits: {}", level.text));
            if current < max {
                if self.exp_materials_verified {
                    self.emit(
                        "ServantEnhance",
                        &format!("当前等级 {current}/{max}，执行强化"),
                    );
                    self.pending_confirm = Some(PendingConfirm::Enhancement);
                    self.exp_materials_verified = false;
                    if self.tap_at("ServantEnhance", ENHANCE_CONFIRM_BUTTON) {
                        thread::sleep(Duration::from_millis(900));
                    }
                } else {
                    self.emit(
                        "ServantEnhance",
                        &format!("当前等级 {current}/{max}，进入素材页"),
                    );
                    if self.tap_at("ServantEnhance", MATERIAL_SELECT_BUTTON) {
                        thread::sleep(Duration::from_millis(900));
                    }
                }
                return;
            }

            if contains_keyword(&ocr.fragments, "霊基再臨") {
                self.exp_materials_verified = false;
                self.emit(
                    "ServantEnhance",
                    &format!("已满级 {current}/{max}，进入灵基再临"),
                );
                if self.tap_at("ServantEnhance", ASCENSION_ENTRY_BUTTON) {
                    thread::sleep(Duration::from_millis(900));
                }
                return;
            }

            self.transition_lifecycle(EnhancementLifecycleEvent::Finished);
            self.emit(
                "ServantEnhance",
                &format!("强化完成，当前等级 {current}/{max}"),
            );
            return;
        }

        self.emit(
            "ServantEnhance",
            &format!(
                "等级数字模板未命中({})，判定当前未选中目标从者，进入从者选择",
                level.fail_reason.as_deref().unwrap_or("unknown")
            ),
        );
        if self.tap_at("ServantEnhance", SERVANT_SELECT_BUTTON) {
            thread::sleep(Duration::from_millis(900));
        }
    }

    fn handle_servant_select(&mut self, ocr: &OcrRegionResult) {
        if !self.servant_select_density_checked {
            if !self.ensure_servant_select_max_density() {
                return;
            }
            self.servant_select_density_checked = true;
        }

        self.emit(
            "ServantSelect",
            &format!("查找从者头像 {}", self.target.name_jp),
        );
        let face_template_paths = self.target.face_template_paths.clone();
        match self.sidecar().find_enhancement_servant_grid(
            None,
            &face_template_paths,
            SERVANT_LIST_REGION,
            SERVANT_FACE_MATCH_CROP,
            Some(SERVANT_FACE_TEMPLATE_SIZE),
            0.85,
            1.2,
        ) {
            Ok(result) => {
                self.emit(
                    "ServantSelect",
                    &format!(
                        "网格 anchors={} cells={} best={:.3}",
                        result.anchors.len(),
                        result.grid_cells.len(),
                        result.score
                    ),
                );
                if result.found {
                    if self.tap_at("ServantSelect", Point::new(result.x, result.y)) {
                        thread::sleep(Duration::from_millis(900));
                    }
                    return;
                }
            }
            Err(err) => {
                self.fail("ServantSelect", format!("头像网格匹配失败: {err}"));
                return;
            }
        }

        let _ = self.tap_fragment_if_present("ServantSelect", &ocr.fragments, "Lv.順");
        thread::sleep(Duration::from_millis(250));
        let _ = self.tap_fragment_if_present("ServantSelect", &ocr.fragments, "降順");
        thread::sleep(Duration::from_millis(250));

        if self.swipe_at(
            "ServantSelect",
            SERVANT_LIST_SCROLL_FROM,
            SERVANT_LIST_SCROLL_TO,
            450,
        ) {
            thread::sleep(Duration::from_millis(800));
        }
    }

    fn ensure_servant_select_max_density(&mut self) -> bool {
        self.ensure_scale_level_3("ServantSelect")
    }

    fn handle_filter_dialog(&mut self, ocr: &OcrRegionResult) {
        self.emit("FilterDialog", "设置仅经验值素材筛选");
        let type_probe = self.probe_template(PROBE_TEXT_FILTER_SETTING_TYPE);
        if !type_probe.found {
            self.fail(
                "FilterDialog",
                format!(
                    "筛选弹窗中未能确认素材种类区域，停止以避免误消耗素材。{}",
                    type_probe.summary()
                ),
            );
            return;
        }
        for _ in 0..5 {
            if let Some(center) = find_exp_material_filter_center(&ocr.fragments) {
                if self.tap_at("FilterDialog", center) {
                    thread::sleep(Duration::from_millis(400));
                }
                let _ = self.tap_at("FilterDialog", FILTER_CONFIRM_BUTTON);
                thread::sleep(Duration::from_millis(900));
                return;
            }
            if !self.swipe_at("FilterDialog", FILTER_SCROLL_FROM, FILTER_SCROLL_TO, 500) {
                return;
            }
            thread::sleep(Duration::from_millis(700));
            let refreshed = match self.ocr_region(FILTER_DIALOG_REGION) {
                Ok(v) => v,
                Err(err) => {
                    self.fail("FilterDialog", format!("刷新筛选 OCR 失败: {err}"));
                    return;
                }
            };
            if let Some(center) = find_exp_material_filter_center(&refreshed.fragments) {
                if self.tap_at("FilterDialog", center) {
                    thread::sleep(Duration::from_millis(400));
                }
                let _ = self.tap_at("FilterDialog", FILTER_CONFIRM_BUTTON);
                thread::sleep(Duration::from_millis(900));
                return;
            }
        }

        self.fail(
            "FilterDialog",
            "未找到「サーヴァント(経験値)」筛选项，请确认素材筛选页面".to_string(),
        );
    }

    fn handle_material_select(&mut self, ocr: &OcrRegionResult) {
        if !self.exp_materials_look_safe(ocr) {
            self.emit("MaterialSelect", "当前素材页未确认是纯经验素材，打开筛选");
            if self.tap_at("MaterialSelect", FILTER_BUTTON) {
                thread::sleep(Duration::from_millis(900));
            }
            return;
        }

        self.exp_materials_verified = true;
        if !self.ensure_max_density() {
            return;
        }

        self.emit("MaterialSelect", "尝试拖选 20 个经验素材");
        let _ = self.swipe_at("MaterialSelect", MATERIAL_DRAG_FROM, MATERIAL_DRAG_TO, 1100);
        thread::sleep(Duration::from_millis(1000));

        let selected_after_drag = match self.read_selected_count() {
            Ok(v) => v,
            Err(err) => {
                self.fail("MaterialSelect", format!("读取已选数量失败: {err}"));
                return;
            }
        };

        if selected_after_drag < 20 {
            self.emit("MaterialSelect", "拖选未凑满 20，改用逐个点击回退");
            for point in MATERIAL_GRID_POINTS {
                if self.is_cancelled() {
                    return;
                }
                if !self.tap_at("MaterialSelect", point) {
                    return;
                }
                thread::sleep(Duration::from_millis(140));
                if let Ok(count) = self.read_selected_count() {
                    if count >= 20 {
                        break;
                    }
                }
            }
        }

        let final_count = match self.read_selected_count() {
            Ok(v) => v,
            Err(err) => {
                self.fail("MaterialSelect", format!("读取最终已选数量失败: {err}"));
                return;
            }
        };
        if final_count < 20 {
            self.fail(
                "MaterialSelect",
                format!("经验素材选择不足，当前仅选择了 {final_count}/20"),
            );
            return;
        }

        self.emit("MaterialSelect", "已选择 20 个经验素材，返回强化页");
        if self.tap_at("MaterialSelect", MATERIAL_CONFIRM_BUTTON) {
            thread::sleep(Duration::from_millis(900));
        }
    }

    fn handle_confirm_dialog(&mut self, ocr: &OcrRegionResult) {
        if normalize_text(&ocr.full_text).contains("★4以上") && !self.exp_materials_verified {
            self.fail(
                "ConfirmDialog",
                "出现高星素材警告，但当前未能确认素材仅为经验值".to_string(),
            );
            return;
        }

        self.emit("ConfirmDialog", "确认执行强化");
        if !self.tap_at("ConfirmDialog", DIALOG_CONFIRM_BUTTON) {
            return;
        }
        thread::sleep(Duration::from_millis(900));

        match self.pending_confirm.take() {
            Some(PendingConfirm::Enhancement) => {
                let _ = self.skip_until(
                    "EnhancementAnimation",
                    &[EnhancementScreen::ServantEnhance],
                    Duration::from_secs(30),
                );
            }
            Some(PendingConfirm::Ascension) => {
                let _ = self.skip_until(
                    "AscensionAnimation",
                    &[
                        EnhancementScreen::AscensionResult,
                        EnhancementScreen::Ascension,
                        EnhancementScreen::AscensionNotReady,
                    ],
                    Duration::from_secs(90),
                );
            }
            None => {}
        }
    }

    fn handle_ascension(&mut self) {
        self.emit("Ascension", "执行灵基再临");
        self.pending_confirm = Some(PendingConfirm::Ascension);
        if self.tap_at("Ascension", ENHANCE_CONFIRM_BUTTON) {
            thread::sleep(Duration::from_millis(900));
        }
    }

    fn handle_ascension_not_ready(&self) {
        self.fail(
            "Ascension",
            "检测到灵基再临 not_ready 状态，停止以避免无效点击；请确认材料或补充可执行状态模板"
                .to_string(),
        );
    }

    fn handle_ascension_result(&mut self) {
        self.emit("AscensionResult", "灵基再临完成，返回从者强化页");
        if self.tap_template_or_point(
            "AscensionResult",
            PROBE_BUTTON_ASCENSION_TO_SERVANT,
            ASCENSION_RESULT_RETURN_BUTTON,
        ) {
            thread::sleep(Duration::from_millis(900));
        }
    }

    fn handle_profile_update_dialog(&self, ocr: &OcrRegionResult) {
        self.emit("ProfileDialog", "关闭资料更新弹窗");
        if let Some(center) = find_best_fragment_center(&ocr.fragments, "閉じる") {
            if self.tap_at("ProfileDialog", center) {
                thread::sleep(Duration::from_millis(700));
            }
        } else if self.tap_at("ProfileDialog", PAGE_CENTER_TAP) {
            thread::sleep(Duration::from_millis(700));
        }
    }

    fn tap_fragment_if_present(
        &self,
        screen: &str,
        fragments: &[OcrFragment],
        needle: &str,
    ) -> bool {
        if let Some(center) = find_best_fragment_center(fragments, needle) {
            self.tap_at(screen, center)
        } else {
            false
        }
    }

    fn exp_materials_look_safe(&self, ocr: &OcrRegionResult) -> bool {
        let normalized = normalize_text(&ocr.full_text);
        let exp_count = normalized.matches("expup").count() + normalized.matches("exp").count();
        let banned = normalized.contains("atk")
            || normalized.contains("hp")
            || normalized.contains("フォウ")
            || normalized.contains("fou");
        exp_count >= 8 && !banned
    }

    fn ensure_max_density(&mut self) -> bool {
        self.ensure_scale_level_3("MaterialSelect")
    }

    fn ensure_scale_level_3(&mut self, screen: &str) -> bool {
        self.emit(screen, "确认一屏最多显示模式");
        let mut last = self.probe_template(PROBE_BUTTON_SCALE_LEVEL_3);
        for attempt in 0..=3 {
            match scale_level_3_decision(attempt, last.found) {
                ScaleLevel3Decision::Done => return true,
                ScaleLevel3Decision::Fail => {
                    self.fail(
                        screen,
                        format!(
                            "无法切换到一屏最多显示模式，button_scale_level_3 未命中。{}",
                            last.summary()
                        ),
                    );
                    return false;
                }
                ScaleLevel3Decision::TapAndRetry => {
                    if !self.tap_at(screen, GRID_DENSITY_BUTTON) {
                        return false;
                    }
                    thread::sleep(Duration::from_millis(500));
                    last = self.probe_template(PROBE_BUTTON_SCALE_LEVEL_3);
                }
            }
        }
        false
    }

    fn read_selected_count(&mut self) -> Result<u32, String> {
        let ocr = self.ocr_region(MATERIAL_COUNTER_REGION)?;
        Ok(parse_selected_count(&ocr.full_text).unwrap_or(0))
    }

    fn skip_until(
        &mut self,
        label: &str,
        targets: &[EnhancementScreen],
        timeout: Duration,
    ) -> Result<(), String> {
        let start = Instant::now();
        let mut last_tap = Instant::now() - Duration::from_secs(3);
        loop {
            if self.is_cancelled() {
                return Ok(());
            }
            let (screen, ocr) = self.detect_screen()?;
            if targets.contains(&screen) {
                return Ok(());
            }
            if screen == EnhancementScreen::ProfileUpdateDialog {
                self.handle_profile_update_dialog(&ocr);
                continue;
            }
            if start.elapsed() >= timeout {
                return Err(format!("{label} 超时"));
            }
            if last_tap.elapsed() >= Duration::from_millis(1200) {
                self.emit(label, "点击屏幕跳过动画…");
                if !self.tap_at(label, PAGE_CENTER_TAP) {
                    return Err("点击跳过动画失败".to_string());
                }
                last_tap = Instant::now();
            }
            thread::sleep(Duration::from_millis(400));
        }
    }
}

impl Drop for EnhancementRunner {
    fn drop(&mut self) {
        let Some(mut sidecar) = self.sidecar.take() else {
            return;
        };
        if let Err(err) = sidecar.stop_stream() {
            eprintln!("[mash-cv] stop_stream before caching enhancement sidecar failed: {err}");
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

impl TemplateProbeResult {
    fn from_match(probe: TemplateProbe, result: ElementMatch) -> Self {
        Self {
            probe,
            found: result.found,
            score: result.score,
            center: if result.found {
                Some(Point::new(result.x, result.y))
            } else {
                None
            },
            region: result.region,
            error: None,
        }
    }

    fn summary(&self) -> String {
        let region = self
            .region
            .map(|r| format!(" match={:.3},{:.3},{:.3},{:.3}", r.x, r.y, r.w, r.h))
            .unwrap_or_default();
        let error = self
            .error
            .as_ref()
            .map(|e| format!(" error={e}"))
            .unwrap_or_default();
        format!(
            "{}.{} found={} score={:.3}{}{}",
            self.probe.screen, self.probe.key, self.found, self.score, region, error
        )
    }
}

fn detect_enhancement_screen(probes: &[TemplateProbeResult]) -> Option<EnhancementRoute> {
    let snapshot = ProbeSnapshot::from_results(probes);
    classify_enhancement_route(&snapshot)
}

fn classify_enhancement_route(snapshot: &ProbeSnapshot) -> Option<EnhancementRoute> {
    if snapshot.found("text_enhancement_servant") {
        let variant = if snapshot.found("text_enhancement_material") {
            EnhancementVariant::MaterialSelect
        } else if snapshot.found("text_enhancement_servant_select") {
            EnhancementVariant::ServantSelect
        } else if snapshot.found("text_enhancement_result") {
            EnhancementVariant::Main
        } else {
            EnhancementVariant::Main
        };
        let status = if snapshot.found("dialog_filter_setting") {
            EnhancementStatus::FilterDialogOpen
        } else {
            EnhancementStatus::None
        };
        return Some(EnhancementRoute {
            screen: EnhancementTopScreen::ServantEnhancement,
            variant,
            status,
        });
    }

    if snapshot.found("screen_enhancement_ascension") {
        let variant = if snapshot.found("text_enhancement_ascension_servant_select") {
            EnhancementVariant::ServantSelect
        } else if snapshot.found("text_ascension_main_variant") {
            EnhancementVariant::Main
        } else {
            EnhancementVariant::Main
        };
        let status = if snapshot.found("enhancement_ascension_not_ready") {
            EnhancementStatus::NotReady
        } else {
            EnhancementStatus::None
        };
        return Some(EnhancementRoute {
            screen: EnhancementTopScreen::Ascension,
            variant,
            status,
        });
    }

    if snapshot.found("text_enhancement") {
        return Some(EnhancementRoute {
            screen: EnhancementTopScreen::Enhancement,
            variant: EnhancementVariant::Main,
            status: EnhancementStatus::None,
        });
    }

    if snapshot.found("button_notification") {
        let status = if snapshot.found("button_enhancement") {
            EnhancementStatus::MenuOpen
        } else {
            EnhancementStatus::None
        };
        return Some(EnhancementRoute {
            screen: EnhancementTopScreen::Main,
            variant: EnhancementVariant::Main,
            status,
        });
    }

    None
}

#[derive(Debug, Default)]
struct ProbeSnapshot {
    found_keys: Vec<&'static str>,
}

impl ProbeSnapshot {
    fn from_results(results: &[TemplateProbeResult]) -> Self {
        Self {
            found_keys: results
                .iter()
                .filter(|result| result.found)
                .map(|result| result.probe.key)
                .collect(),
        }
    }

    #[cfg(test)]
    fn from_keys(keys: &[&'static str]) -> Self {
        Self {
            found_keys: keys.to_vec(),
        }
    }

    fn found(&self, key: &str) -> bool {
        self.found_keys.iter().any(|found_key| *found_key == key)
    }
}

fn probe_diagnostics_as_ocr(probes: &[TemplateProbeResult]) -> OcrRegionResult {
    OcrRegionResult {
        fragments: Vec::new(),
        full_text: summarize_probe_results(probes),
    }
}

fn scale_level_3_decision(attempt: usize, found: bool) -> ScaleLevel3Decision {
    if found {
        ScaleLevel3Decision::Done
    } else if attempt >= 3 {
        ScaleLevel3Decision::Fail
    } else {
        ScaleLevel3Decision::TapAndRetry
    }
}

fn contains_servant_word(text: &str) -> bool {
    text.contains("サーヴァント") || text.contains("サーヴアント")
}

fn merge_ocr_results(parts: &[&OcrRegionResult]) -> OcrRegionResult {
    let mut fragments = Vec::new();
    let mut lines = Vec::new();
    for part in parts {
        fragments.extend(part.fragments.iter().cloned());
        let text = part.full_text.trim();
        if !text.is_empty() {
            lines.push(text.to_string());
        }
    }
    OcrRegionResult {
        fragments,
        full_text: lines.join("\n"),
    }
}

fn merge_unknown_diagnostics(
    template_results: &[TemplateProbeResult],
    ocr_parts: &[NamedOcr],
) -> OcrRegionResult {
    let mut result = merge_named_ocr_for_unknown(ocr_parts);
    let template_summary = summarize_probe_results(template_results);
    if result.full_text.trim().is_empty() {
        result.full_text = template_summary;
    } else {
        result.full_text = format!("templates={} || {}", template_summary, result.full_text);
    }
    result
}

fn summarize_probe_results(results: &[TemplateProbeResult]) -> String {
    if results.is_empty() {
        return "<empty>".to_string();
    }
    results
        .iter()
        .map(TemplateProbeResult::summary)
        .collect::<Vec<_>>()
        .join(" | ")
}

fn merge_named_ocr_for_unknown(parts: &[NamedOcr]) -> OcrRegionResult {
    let mut fragments = Vec::new();
    let mut lines = Vec::new();
    for part in parts {
        fragments.extend(part.result.fragments.iter().cloned());
        let text = summarize_ocr(&part.result);
        if text != "<empty>" {
            lines.push(format!("{}={}", part.name, text));
        }
    }
    OcrRegionResult {
        fragments,
        full_text: lines.join(" || "),
    }
}

fn summarize_ocr(ocr: &OcrRegionResult) -> String {
    let full_text = ocr.full_text.trim();
    if !full_text.is_empty() {
        return truncate_for_log(&normalize_whitespace(full_text), 120);
    }

    let fragments: Vec<String> = ocr
        .fragments
        .iter()
        .take(6)
        .filter_map(|fragment| {
            let text = fragment.text.trim();
            if text.is_empty() {
                return None;
            }
            Some(format!(
                "{}@{:.2},{:.2}",
                truncate_for_log(&normalize_whitespace(text), 24),
                fragment.region.x,
                fragment.region.y
            ))
        })
        .collect();

    if fragments.is_empty() {
        "<empty>".to_string()
    } else {
        fragments.join(" | ")
    }
}

#[cfg(test)]
fn norm_rect_area(rect: NormRect) -> f64 {
    rect.w * rect.h
}

fn normalize_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_for_log(s: &str, max_chars: usize) -> String {
    let mut chars = s.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

fn contains_keyword(fragments: &[OcrFragment], needle: &str) -> bool {
    find_best_fragment_center(fragments, needle).is_some()
}

fn find_exp_material_filter_center(fragments: &[OcrFragment]) -> Option<Point> {
    find_best_fragment_center(fragments, "サーヴァント(経験値)")
        .or_else(|| find_best_fragment_center(fragments, "経験値"))
}

fn find_best_fragment_center(fragments: &[OcrFragment], needle: &str) -> Option<Point> {
    let target = normalize_text(needle);
    let mut best: Option<(&OcrFragment, usize)> = None;
    for fragment in fragments {
        let text = normalize_text(&fragment.text);
        if text.is_empty() || target.is_empty() {
            continue;
        }
        let score = if text.contains(&target) {
            target.len()
        } else if target.contains(&text) {
            text.len()
        } else {
            0
        };
        if score == 0 {
            continue;
        }
        if best
            .map(|(_, best_score)| score > best_score)
            .unwrap_or(true)
        {
            best = Some((fragment, score));
        }
    }
    best.map(|(fragment, _)| {
        Point::new(
            fragment.region.x + fragment.region.w / 2.0,
            fragment.region.y + fragment.region.h / 2.0,
        )
    })
}

fn normalize_text(s: &str) -> String {
    s.nfkc()
        .collect::<String>()
        .chars()
        .filter(|ch| {
            !matches!(
                ch,
                ' ' | '\t'
                    | '\n'
                    | '\r'
                    | '\u{3000}'
                    | '・'
                    | '·'
                    | '.'
                    | ','
                    | '、'
                    | '。'
                    | ';'
                    | ':'
                    | '!'
                    | '?'
                    | '-'
                    | '_'
                    | '|'
            )
        })
        .collect::<String>()
        .to_lowercase()
}

pub(crate) fn parse_selected_count(text: &str) -> Option<u32> {
    let normalized = normalize_text(text);
    let chars = normalized.chars().collect::<Vec<_>>();
    for slash in 0..chars.len() {
        if chars[slash] != '/'
            || chars.get(slash + 1) != Some(&'2')
            || chars.get(slash + 2) != Some(&'0')
        {
            continue;
        }
        let start = (0..slash)
            .rev()
            .find(|&index| !chars[index].is_ascii_digit())
            .map_or(0, |index| index + 1);
        if start < slash {
            let selected = chars[start..slash]
                .iter()
                .collect::<String>()
                .parse()
                .ok()?;
            if selected <= 20 {
                return Some(selected);
            }
        }
    }

    for digits in normalized
        .split(|character: char| !character.is_ascii_digit())
        .filter(|digits| digits.len() > 2 && digits.ends_with("20"))
    {
        let numerator = &digits[..digits.len() - 2];
        // The OCR model frequently reads the slash in "17/20" as "1",
        // producing "17120". Remove that trailing false digit first.
        if let Some(without_false_slash) = numerator.strip_suffix('1') {
            if let Ok(selected) = without_false_slash.parse::<u32>() {
                if selected <= 20 {
                    return Some(selected);
                }
            }
        }
        if let Ok(selected) = numerator.parse::<u32>() {
            if selected <= 20 {
                return Some(selected);
            }
        }
    }
    None
}

pub(crate) fn server_supported(server: Server) -> bool {
    matches!(server, Server::Jp)
}

#[cfg(test)]
mod tests;
