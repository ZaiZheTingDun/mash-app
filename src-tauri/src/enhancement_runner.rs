use crate::adb::Adb;
use crate::runner::LogLevel;
use crate::screen::{ElementMatch, NormRect, OcrFragment, OcrRegionResult, Point, SidecarClient};
use crate::touch::{self, TouchBackend};
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
    touch: Box<dyn TouchBackend>,
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

mod runtime;

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
