use crate::adb::Adb;
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
const SERVANT_ENHANCE_REGION: NormRect = NormRect {
    x: 0.18,
    y: 0.16,
    w: 0.76,
    h: 0.72,
};
const FILTER_DIALOG_REGION: NormRect = NormRect {
    x: 0.14,
    y: 0.10,
    w: 0.74,
    h: 0.80,
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

const ENHANCEMENT_PROBE_SCREEN: &str = "EnhancementAutomation";

const PROBE_BUTTON_NOTIFICATION: TemplateProbe = TemplateProbe {
    key: "button_notification",
    screen: ENHANCEMENT_PROBE_SCREEN,
    element: "button_notification",
};
const PROBE_TEXT_ENHANCEMENT: TemplateProbe = TemplateProbe {
    key: "text_enhancement",
    screen: ENHANCEMENT_PROBE_SCREEN,
    element: "text_enhancement",
};
const PROBE_TEXT_ENHANCEMENT_SERVANT: TemplateProbe = TemplateProbe {
    key: "text_enhancement_servant",
    screen: ENHANCEMENT_PROBE_SCREEN,
    element: "text_enhancement_servant",
};
const PROBE_SCREEN_ASCENSION: TemplateProbe = TemplateProbe {
    key: "screen_enhancement_ascension",
    screen: ENHANCEMENT_PROBE_SCREEN,
    element: "screen_enhancement_ascension",
};
const PROBE_BUTTON_MENU: TemplateProbe = TemplateProbe {
    key: "button_menu",
    screen: ENHANCEMENT_PROBE_SCREEN,
    element: "button_menu",
};
const PROBE_BUTTON_ENHANCEMENT: TemplateProbe = TemplateProbe {
    key: "button_enhancement",
    screen: ENHANCEMENT_PROBE_SCREEN,
    element: "button_enhancement",
};
const PROBE_TEXT_ENHANCEMENT_RESULT: TemplateProbe = TemplateProbe {
    key: "text_enhancement_result",
    screen: ENHANCEMENT_PROBE_SCREEN,
    element: "text_enhancement_result",
};
const PROBE_TEXT_ENHANCEMENT_SERVANT_SELECT: TemplateProbe = TemplateProbe {
    key: "text_enhancement_servant_select",
    screen: ENHANCEMENT_PROBE_SCREEN,
    element: "text_enhancement_servant_select",
};
const PROBE_TEXT_ENHANCEMENT_MATERIAL: TemplateProbe = TemplateProbe {
    key: "text_enhancement_material",
    screen: ENHANCEMENT_PROBE_SCREEN,
    element: "text_enhancement_material",
};
const PROBE_DIALOG_FILTER_SETTING: TemplateProbe = TemplateProbe {
    key: "dialog_filter_setting",
    screen: ENHANCEMENT_PROBE_SCREEN,
    element: "dialog_filter_setting",
};
const PROBE_TEXT_FILTER_SETTING_TYPE: TemplateProbe = TemplateProbe {
    key: "text_filter_setting_type",
    screen: ENHANCEMENT_PROBE_SCREEN,
    element: "text_filter_setting_type",
};
const PROBE_BUTTON_SCALE_LEVEL_3: TemplateProbe = TemplateProbe {
    key: "button_scale_level_3",
    screen: ENHANCEMENT_PROBE_SCREEN,
    element: "button_scale_level_3",
};
const PROBE_TEXT_ASCENSION_MAIN_VARIANT: TemplateProbe = TemplateProbe {
    key: "text_ascension_main_variant",
    screen: ENHANCEMENT_PROBE_SCREEN,
    element: "text_ascension_main_variant",
};
const PROBE_TEXT_ASCENSION_SERVANT_SELECT: TemplateProbe = TemplateProbe {
    key: "text_enhancement_ascension_servant_select",
    screen: ENHANCEMENT_PROBE_SCREEN,
    element: "text_enhancement_ascension_servant_select",
};
const PROBE_ASCENSION_NOT_READY: TemplateProbe = TemplateProbe {
    key: "enhancement_ascension_not_ready",
    screen: ENHANCEMENT_PROBE_SCREEN,
    element: "enhancement_ascension_not_ready",
};
const PROBE_BUTTON_ASCENSION_TO_SERVANT: TemplateProbe = TemplateProbe {
    key: "button_enhancement_ascension_to_servant",
    screen: ENHANCEMENT_PROBE_SCREEN,
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
const ENHANCE_SERVANT_ENTRY_BUTTON: Point = Point::new(0.730, 0.347);
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
const SERVANT_LIST_REGION: NormRect = NormRect {
    x: 0.05,
    y: 0.16,
    w: 0.78,
    h: 0.74,
};

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
    pub face_template_path: PathBuf,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum EnhancementRunnerState {
    Idle,
    Running,
    Finished,
    Error { message: String },
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnhancementAutomationEvent {
    pub state: String,
    pub current_screen: String,
    pub message: String,
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
    MenuCollapsed,
    MenuOpen,
    Main,
    ServantSelect,
    MaterialSelect,
    FilterDialogOpen,
    NotReady,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EnhancementRoute {
    screen: EnhancementTopScreen,
    variant: EnhancementVariant,
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
    sidecar: SidecarClient,
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
    ) -> Self {
        Self {
            adb,
            sidecar,
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
        self.set_state(EnhancementRunnerState::Running);
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
                self.set_state(EnhancementRunnerState::Idle);
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

    fn set_state(&self, s: EnhancementRunnerState) {
        *self.state.lock().unwrap() = s;
    }

    fn emit(&self, screen: &str, message: &str) {
        let state_str = {
            let s = self.state.lock().unwrap();
            format!("{:?}", *s)
        };
        let _ = self.app_handle.emit(
            ENHANCEMENT_EVENT_NAME,
            EnhancementAutomationEvent {
                state: state_str,
                current_screen: screen.to_string(),
                message: message.to_string(),
            },
        );
    }

    fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    fn fail(&self, screen: &str, message: String) {
        self.set_state(EnhancementRunnerState::Error {
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
        self.sidecar.ocr_region(None, region)
    }

    fn probe_template(&mut self, probe: TemplateProbe) -> TemplateProbeResult {
        match self
            .sidecar
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
        let dialog = self.ocr_region(FILTER_DIALOG_REGION)?;
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
        let screen = match (route.screen, route.variant) {
            (EnhancementTopScreen::Main, EnhancementVariant::MenuCollapsed) => {
                EnhancementScreen::HomeMenuClosed
            }
            (EnhancementTopScreen::Main, EnhancementVariant::MenuOpen) => {
                EnhancementScreen::HomeMenuOpen
            }
            (EnhancementTopScreen::Enhancement, EnhancementVariant::Main) => {
                EnhancementScreen::EnhanceMenu
            }
            (EnhancementTopScreen::ServantEnhancement, EnhancementVariant::Main) => {
                EnhancementScreen::ServantEnhance
            }
            (EnhancementTopScreen::ServantEnhancement, EnhancementVariant::ServantSelect)
            | (EnhancementTopScreen::Ascension, EnhancementVariant::ServantSelect) => {
                EnhancementScreen::ServantSelect
            }
            (EnhancementTopScreen::ServantEnhancement, EnhancementVariant::MaterialSelect) => {
                EnhancementScreen::MaterialSelect
            }
            (EnhancementTopScreen::ServantEnhancement, EnhancementVariant::FilterDialogOpen) => {
                EnhancementScreen::FilterDialog
            }
            (EnhancementTopScreen::Ascension, EnhancementVariant::Main) => {
                EnhancementScreen::Ascension
            }
            (EnhancementTopScreen::Ascension, EnhancementVariant::NotReady) => {
                EnhancementScreen::AscensionNotReady
            }
            _ => EnhancementScreen::Unknown,
        };

        let ocr = match screen {
            EnhancementScreen::ServantEnhance => self.ocr_region(SERVANT_ENHANCE_REGION)?,
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
        let level_ocr = match self.ocr_region(LEVEL_REGION) {
            Ok(v) => v,
            Err(err) => {
                self.fail("ServantEnhance", format!("读取等级失败: {err}"));
                return;
            }
        };
        let level_text = if level_ocr.full_text.trim().is_empty() {
            ocr.full_text.clone()
        } else {
            level_ocr.full_text
        };
        if let Some((current, max)) = parse_level_pair(&level_text) {
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

            self.set_state(EnhancementRunnerState::Finished);
            self.emit(
                "ServantEnhance",
                &format!("强化完成，当前等级 {current}/{max}"),
            );
            return;
        }

        self.emit("ServantEnhance", "当前未选中目标从者，进入从者选择");
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
        match self.sidecar.find_region(
            None,
            &self.target.face_template_path,
            SERVANT_LIST_REGION,
            0.72,
        ) {
            Ok(Some(center)) => {
                if self.tap_at("ServantSelect", center) {
                    thread::sleep(Duration::from_millis(900));
                }
                return;
            }
            Ok(None) => {}
            Err(err) => {
                self.fail("ServantSelect", format!("头像匹配失败: {err}"));
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
        let variant = if snapshot.found("dialog_filter_setting") {
            EnhancementVariant::FilterDialogOpen
        } else if snapshot.found("text_enhancement_material") {
            EnhancementVariant::MaterialSelect
        } else if snapshot.found("text_enhancement_servant_select") {
            EnhancementVariant::ServantSelect
        } else if snapshot.found("text_enhancement_result") {
            EnhancementVariant::Main
        } else {
            EnhancementVariant::Main
        };
        return Some(EnhancementRoute {
            screen: EnhancementTopScreen::ServantEnhancement,
            variant,
        });
    }

    if snapshot.found("screen_enhancement_ascension") {
        let variant = if snapshot.found("enhancement_ascension_not_ready") {
            EnhancementVariant::NotReady
        } else if snapshot.found("text_enhancement_ascension_servant_select") {
            EnhancementVariant::ServantSelect
        } else if snapshot.found("text_ascension_main_variant") {
            EnhancementVariant::Main
        } else {
            EnhancementVariant::Main
        };
        return Some(EnhancementRoute {
            screen: EnhancementTopScreen::Ascension,
            variant,
        });
    }

    if snapshot.found("text_enhancement") {
        return Some(EnhancementRoute {
            screen: EnhancementTopScreen::Enhancement,
            variant: EnhancementVariant::Main,
        });
    }

    if snapshot.found("button_notification") {
        let variant = if snapshot.found("button_enhancement") {
            EnhancementVariant::MenuOpen
        } else {
            EnhancementVariant::MenuCollapsed
        };
        return Some(EnhancementRoute {
            screen: EnhancementTopScreen::Main,
            variant,
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

fn parse_level_pair(text: &str) -> Option<(u32, u32)> {
    let chars: Vec<char> = normalize_text(text).chars().collect();
    for idx in 0..chars.len() {
        if chars[idx] != 'l' {
            continue;
        }
        if idx + 1 >= chars.len() || chars[idx + 1] != 'v' {
            continue;
        }
        let mut j = idx + 2;
        let mut left = String::new();
        while j < chars.len() && chars[j].is_ascii_digit() {
            left.push(chars[j]);
            j += 1;
        }
        if left.is_empty() || j >= chars.len() || chars[j] != '/' {
            continue;
        }
        j += 1;
        let mut right = String::new();
        while j < chars.len() && chars[j].is_ascii_digit() {
            right.push(chars[j]);
            j += 1;
        }
        if right.is_empty() {
            continue;
        }
        let current = left.parse().ok()?;
        let max = right.parse().ok()?;
        return Some((current, max));
    }
    None
}

fn parse_selected_count(text: &str) -> Option<u32> {
    let normalized = normalize_text(text);
    for marker in ["選択済", "20/20", "/20"] {
        if let Some(start) = normalized.find(marker) {
            let slice = &normalized[start..];
            let bytes: Vec<char> = slice.chars().collect();
            for idx in 0..bytes.len() {
                if !bytes[idx].is_ascii_digit() {
                    continue;
                }
                let mut digits = String::new();
                let mut j = idx;
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    digits.push(bytes[j]);
                    j += 1;
                }
                if !digits.is_empty() && j < bytes.len() && bytes[j] == '/' {
                    return digits.parse().ok();
                }
            }
        }
    }
    if let Some(start) = normalized.find("選択済") {
        let slice = &normalized[start..];
        let chars: Vec<char> = slice.chars().collect();
        for idx in 0..chars.len() {
            if !chars[idx].is_ascii_digit() {
                continue;
            }
            let mut digits = String::new();
            let mut j = idx;
            while j < chars.len() && chars[j].is_ascii_digit() {
                digits.push(chars[j]);
                j += 1;
            }
            if digits.len() > 2 && digits.ends_with("20") {
                return digits[..digits.len() - 2].parse().ok();
            }
        }
    }
    None
}

pub(crate) fn server_supported(server: Server) -> bool {
    matches!(server, Server::Jp)
}

#[cfg(test)]
mod tests {
    use super::{
        classify_enhancement_route, normalize_text, parse_level_pair, parse_selected_count,
        scale_level_3_decision, EnhancementRoute, EnhancementTopScreen, EnhancementVariant,
        ProbeSnapshot, ScaleLevel3Decision,
    };

    #[test]
    fn parse_level_pair_reads_current_and_max() {
        assert_eq!(parse_level_pair("Lv. 70/80"), Some((70, 80)));
        assert_eq!(parse_level_pair("Lv.80/90"), Some((80, 90)));
        assert_eq!(parse_level_pair("Ｌｖ． 1 / 70"), Some((1, 70)));
    }

    #[test]
    fn parse_selected_count_reads_counter() {
        assert_eq!(parse_selected_count("選択済み: 20/20"), Some(20));
        assert_eq!(parse_selected_count("20/20"), Some(20));
        assert_eq!(parse_selected_count("選択済 7/20"), Some(7));
        assert_eq!(parse_selected_count("選択済み：\n020"), Some(0));
        assert_eq!(parse_selected_count("選択済み：\n720"), Some(7));
        assert_eq!(parse_selected_count("選択済み：\n2020"), Some(20));
    }

    #[test]
    fn normalize_text_drops_spacing_and_punctuation() {
        assert_eq!(normalize_text(" Exp. UP "), "expup");
        assert_eq!(normalize_text("Lv. 80/90"), "lv80/90");
    }

    #[test]
    fn template_route_classifies_main_menu_states() {
        assert_eq!(
            classify_enhancement_route(&ProbeSnapshot::from_keys(&["button_notification"])),
            Some(EnhancementRoute {
                screen: EnhancementTopScreen::Main,
                variant: EnhancementVariant::MenuCollapsed,
            })
        );
        assert_eq!(
            classify_enhancement_route(&ProbeSnapshot::from_keys(&[
                "button_notification",
                "button_enhancement"
            ])),
            Some(EnhancementRoute {
                screen: EnhancementTopScreen::Main,
                variant: EnhancementVariant::MenuOpen,
            })
        );
    }

    #[test]
    fn template_route_classifies_servant_enhancement_variants() {
        assert_eq!(
            classify_enhancement_route(&ProbeSnapshot::from_keys(&[
                "text_enhancement_servant",
                "text_enhancement_result"
            ])),
            Some(EnhancementRoute {
                screen: EnhancementTopScreen::ServantEnhancement,
                variant: EnhancementVariant::Main,
            })
        );
        assert_eq!(
            classify_enhancement_route(&ProbeSnapshot::from_keys(&[
                "text_enhancement_servant",
                "text_enhancement_servant_select"
            ])),
            Some(EnhancementRoute {
                screen: EnhancementTopScreen::ServantEnhancement,
                variant: EnhancementVariant::ServantSelect,
            })
        );
        assert_eq!(
            classify_enhancement_route(&ProbeSnapshot::from_keys(&[
                "text_enhancement_servant",
                "text_enhancement_material"
            ])),
            Some(EnhancementRoute {
                screen: EnhancementTopScreen::ServantEnhancement,
                variant: EnhancementVariant::MaterialSelect,
            })
        );
        assert_eq!(
            classify_enhancement_route(&ProbeSnapshot::from_keys(&[
                "text_enhancement_servant",
                "text_enhancement_material",
                "dialog_filter_setting"
            ])),
            Some(EnhancementRoute {
                screen: EnhancementTopScreen::ServantEnhancement,
                variant: EnhancementVariant::FilterDialogOpen,
            })
        );
    }

    #[test]
    fn template_route_classifies_ascension_variants() {
        assert_eq!(
            classify_enhancement_route(&ProbeSnapshot::from_keys(&[
                "screen_enhancement_ascension",
                "text_ascension_main_variant"
            ])),
            Some(EnhancementRoute {
                screen: EnhancementTopScreen::Ascension,
                variant: EnhancementVariant::Main,
            })
        );
        assert_eq!(
            classify_enhancement_route(&ProbeSnapshot::from_keys(&[
                "screen_enhancement_ascension",
                "text_enhancement_ascension_servant_select"
            ])),
            Some(EnhancementRoute {
                screen: EnhancementTopScreen::Ascension,
                variant: EnhancementVariant::ServantSelect,
            })
        );
        assert_eq!(
            classify_enhancement_route(&ProbeSnapshot::from_keys(&[
                "screen_enhancement_ascension",
                "enhancement_ascension_not_ready"
            ])),
            Some(EnhancementRoute {
                screen: EnhancementTopScreen::Ascension,
                variant: EnhancementVariant::NotReady,
            })
        );
    }

    #[test]
    fn template_route_prefers_specific_enhancement_screen_over_generic_title() {
        assert_eq!(
            classify_enhancement_route(&ProbeSnapshot::from_keys(&[
                "text_enhancement",
                "text_enhancement_servant",
                "text_enhancement_result"
            ])),
            Some(EnhancementRoute {
                screen: EnhancementTopScreen::ServantEnhancement,
                variant: EnhancementVariant::Main,
            })
        );
    }

    #[test]
    fn scale_level_3_retries_until_match_or_three_taps() {
        assert_eq!(scale_level_3_decision(0, true), ScaleLevel3Decision::Done);
        assert_eq!(
            scale_level_3_decision(0, false),
            ScaleLevel3Decision::TapAndRetry
        );
        assert_eq!(
            scale_level_3_decision(2, false),
            ScaleLevel3Decision::TapAndRetry
        );
        assert_eq!(scale_level_3_decision(3, false), ScaleLevel3Decision::Fail);
    }
}
