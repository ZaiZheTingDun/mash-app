//! Persisted application settings and related IPC commands.
//!
//! Settings in this module are backend-owned state: commands update the shared
//! Tauri state and persist the matching JSON file under app_data_dir(). Server
//! switches also invalidate idle sidecars so future CV calls load the selected
//! server resources.

use crate::adb::BLUESTACKS_SERIAL;
use crate::automation_coordinator::AutomationCoordinator;
use crate::commands::debug;
use crate::paths::{migrate_legacy_app_data, StartupMigrationStatus};
use crate::runner::bond_level_up_screenshot_dir;
use crate::storage::{read_json_or_default, write_json_atomic};
use crate::Server;
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Mutex;
use tauri::Manager;
use tauri_plugin_opener::OpenerExt;

pub(crate) const SUPPORT_CE_THRESHOLD_DEFAULT: f64 = 0.70;
pub(crate) const SUPPORT_CE_THRESHOLD_MIN: f64 = 0.60;
pub(crate) const SUPPORT_CE_THRESHOLD_MAX: f64 = 0.85;
pub(crate) const SUPPORT_ICON_THRESHOLD_DEFAULT: f64 = 0.70;
pub(crate) const SUPPORT_ICON_THRESHOLD_MIN: f64 = 0.60;
pub(crate) const SUPPORT_ICON_THRESHOLD_MAX: f64 = 0.85;
pub(crate) const SUPPORT_CE_FULL_GATE_THRESHOLD_DEFAULT: f64 = 0.60;
pub(crate) const SUPPORT_CE_FULL_GATE_THRESHOLD_MIN: f64 = 0.40;
pub(crate) const SUPPORT_CE_FULL_GATE_THRESHOLD_MAX: f64 = 0.70;
pub(crate) const UNKNOWN_SCREEN_TIMEOUT_COUNT_DEFAULT: u32 = 100;
pub(crate) const UNKNOWN_SCREEN_TIMEOUT_COUNT_MIN: u32 = 50;
pub(crate) const UNKNOWN_SCREEN_TIMEOUT_COUNT_MAX: u32 = 1_000;
pub(crate) const UNKNOWN_SCREEN_TIMEOUT_COUNT_UNLIMITED: u32 = 9_999;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NoblePhantasmDetectionMode {
    Card,
    Gauge,
    GaugeBeforeAttack,
}

impl Default for NoblePhantasmDetectionMode {
    fn default() -> Self {
        Self::Card
    }
}

fn default_noble_phantasm_detection_mode() -> NoblePhantasmDetectionMode {
    NoblePhantasmDetectionMode::Card
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecognitionSettings {
    #[serde(default = "default_noble_phantasm_detection_mode")]
    pub noble_phantasm_detection_mode: NoblePhantasmDetectionMode,
    #[serde(default = "default_support_ce_threshold")]
    pub support_ce_threshold: f64,
    #[serde(default = "default_support_ce_full_gate_threshold")]
    pub support_ce_full_gate_threshold: f64,
    #[serde(default = "default_support_icon_threshold")]
    pub support_mlb_icon_threshold: f64,
    #[serde(default = "default_support_icon_threshold")]
    pub support_bond_icon_threshold: f64,
    #[serde(default)]
    pub stop_on_bond_level_up: bool,
    #[serde(default)]
    pub stop_on_bond_max_level: bool,
    #[serde(default)]
    pub auto_capture_bond_level_up: bool,
    #[serde(default)]
    pub verify_skill_activation: bool,
    #[serde(default = "default_true")]
    pub enable_extra_class_filter: bool,
    /// When anchor-row support OCR cannot find the target, retry with the
    /// slower detector over the full support list.
    #[serde(default)]
    pub support_full_list_ocr_fallback: bool,
    #[serde(default = "default_unknown_screen_timeout_count")]
    pub unknown_screen_timeout_count: u32,
    #[serde(default)]
    pub auto_friend_request: bool,
}

#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugSettings {
    #[serde(default)]
    pub auto_capture_battle_result_loot: bool,
    #[serde(default)]
    pub auto_capture_unknown_screen_timeout: bool,
    #[serde(default)]
    pub auto_capture_skill_use_probe: bool,
    #[serde(default)]
    pub auto_capture_unrecognized_critical_chance: bool,
    #[serde(default)]
    pub simulate_stuck_attack_selection: bool,
}

impl Default for RecognitionSettings {
    fn default() -> Self {
        Self {
            noble_phantasm_detection_mode: NoblePhantasmDetectionMode::Card,
            support_ce_threshold: SUPPORT_CE_THRESHOLD_DEFAULT,
            support_ce_full_gate_threshold: SUPPORT_CE_FULL_GATE_THRESHOLD_DEFAULT,
            support_mlb_icon_threshold: SUPPORT_ICON_THRESHOLD_DEFAULT,
            support_bond_icon_threshold: SUPPORT_ICON_THRESHOLD_DEFAULT,
            stop_on_bond_level_up: false,
            stop_on_bond_max_level: false,
            auto_capture_bond_level_up: false,
            verify_skill_activation: false,
            enable_extra_class_filter: true,
            support_full_list_ocr_fallback: false,
            unknown_screen_timeout_count: UNKNOWN_SCREEN_TIMEOUT_COUNT_DEFAULT,
            auto_friend_request: false,
        }
    }
}

fn default_support_ce_threshold() -> f64 {
    SUPPORT_CE_THRESHOLD_DEFAULT
}

fn default_support_icon_threshold() -> f64 {
    SUPPORT_ICON_THRESHOLD_DEFAULT
}

fn default_support_ce_full_gate_threshold() -> f64 {
    SUPPORT_CE_FULL_GATE_THRESHOLD_DEFAULT
}

fn default_unknown_screen_timeout_count() -> u32 {
    UNKNOWN_SCREEN_TIMEOUT_COUNT_DEFAULT
}

fn default_true() -> bool {
    true
}

fn normalize_unknown_screen_timeout_count(value: u32, label: &str) -> Result<u32, String> {
    if value == UNKNOWN_SCREEN_TIMEOUT_COUNT_UNLIMITED {
        return Ok(value);
    }
    if !(UNKNOWN_SCREEN_TIMEOUT_COUNT_MIN..=UNKNOWN_SCREEN_TIMEOUT_COUNT_MAX).contains(&value) {
        return Err(format!(
            "{label}必须在 {UNKNOWN_SCREEN_TIMEOUT_COUNT_MIN} 到 {UNKNOWN_SCREEN_TIMEOUT_COUNT_MAX} 次之间，或关闭限制"
        ));
    }
    Ok(value)
}

fn update_unknown_screen_timeout_count(
    state: &Mutex<RecognitionSettings>,
    value: u32,
    persist: impl FnOnce(&RecognitionSettings) -> Result<(), String>,
) -> Result<RecognitionSettings, String> {
    let mut guard = state.lock().unwrap();
    let mut next = *guard;
    next.unknown_screen_timeout_count = value;
    persist(&next)?;
    *guard = next;
    Ok(next)
}

/// Serialize a read-modify-write sequence under one lock and publish the new
/// in-memory value only after durable persistence succeeds.
fn update_persisted_state<T: Copy>(
    state: &Mutex<T>,
    update: impl FnOnce(&mut T),
    persist: impl FnOnce(&T) -> Result<(), String>,
) -> Result<T, String> {
    let mut guard = state.lock().unwrap();
    let mut next = *guard;
    update(&mut next);
    persist(&next)?;
    *guard = next;
    Ok(next)
}

fn update_recognition_settings(
    app: &tauri::AppHandle,
    state: &Mutex<RecognitionSettings>,
    update: impl FnOnce(&mut RecognitionSettings),
) -> Result<RecognitionSettings, String> {
    update_persisted_state(state, update, |settings| {
        save_recognition_settings(app, settings)
    })
}

fn update_debug_settings(
    app: &tauri::AppHandle,
    state: &Mutex<DebugSettings>,
    update: impl FnOnce(&mut DebugSettings),
) -> Result<DebugSettings, String> {
    update_persisted_state(
        state,
        |settings| {
            update(settings);
            *settings = debug_settings_for_current_build(*settings);
        },
        |settings| save_debug_settings(app, settings),
    )
}

fn normalize_threshold(value: f64, label: &str, min: f64, max: f64) -> Result<f64, String> {
    if !value.is_finite() {
        return Err(format!("{label}必须是有效数字"));
    }
    if !(min..=max).contains(&value) {
        return Err(format!("{label}必须在 {min:.2} 到 {max:.2} 之间"));
    }
    Ok((value * 100.0).round() / 100.0)
}

fn normalize_support_ce_threshold(value: f64) -> Result<f64, String> {
    normalize_threshold(
        value,
        "助战礼装阈值",
        SUPPORT_CE_THRESHOLD_MIN,
        SUPPORT_CE_THRESHOLD_MAX,
    )
}

fn normalize_support_icon_threshold(value: f64, label: &str) -> Result<f64, String> {
    normalize_threshold(
        value,
        label,
        SUPPORT_ICON_THRESHOLD_MIN,
        SUPPORT_ICON_THRESHOLD_MAX,
    )
}

fn normalize_support_ce_full_gate_threshold(value: f64) -> Result<f64, String> {
    normalize_threshold(
        value,
        "完整匹配兜底阈值",
        SUPPORT_CE_FULL_GATE_THRESHOLD_MIN,
        SUPPORT_CE_FULL_GATE_THRESHOLD_MAX,
    )
}

fn apply_stop_on_bond_level_up(settings: &mut RecognitionSettings, value: bool) {
    settings.stop_on_bond_level_up = value;
}

fn apply_stop_on_bond_max_level(settings: &mut RecognitionSettings, value: bool) {
    settings.stop_on_bond_max_level = value;
    if value {
        settings.stop_on_bond_level_up = false;
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AdbDeviceSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) selected_adb_serial: Option<String>,
}

fn normalize_adb_serial(value: Option<String>) -> Option<String> {
    value
        .map(|serial| serial.trim().to_string())
        .filter(|serial| !serial.is_empty())
}

fn adb_device_settings_from_value(v: &serde_json::Value) -> AdbDeviceSettings {
    let selected_adb_serial = normalize_adb_serial(
        v.get("selectedAdbSerial")
            .and_then(|value| value.as_str())
            .map(str::to_string),
    )
    .or_else(|| {
        if v.get("useBluestack").and_then(|value| value.as_bool()) == Some(true) {
            Some(BLUESTACKS_SERIAL.to_string())
        } else {
            None
        }
    });
    AdbDeviceSettings {
        selected_adb_serial,
    }
}

fn adb_settings_path(app: &tauri::AppHandle) -> PathBuf {
    let dir = app
        .path()
        .app_data_dir()
        .expect("failed to resolve app data dir");
    fs::create_dir_all(&dir).ok();
    dir.join("adb_settings.json")
}

pub(crate) fn load_adb_device_settings(
    app: &tauri::AppHandle,
) -> Result<AdbDeviceSettings, String> {
    let path = adb_settings_path(app);
    let value: serde_json::Value = read_json_or_default(&path, "ADB 设置")?;
    Ok(adb_device_settings_from_value(&value))
}

pub(crate) fn save_adb_device_settings(
    app: &tauri::AppHandle,
    settings: &AdbDeviceSettings,
) -> Result<(), String> {
    let path = adb_settings_path(app);
    write_json_atomic(&path, settings, "ADB 设置")
}

fn server_settings_path(app: &tauri::AppHandle) -> PathBuf {
    let dir = app
        .path()
        .app_data_dir()
        .expect("failed to resolve app data dir");
    fs::create_dir_all(&dir).ok();
    dir.join("server_settings.json")
}

fn update_check_settings_path(app: &tauri::AppHandle) -> PathBuf {
    let dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."));
    dir.join("update_check_settings.json")
}

fn recognition_settings_path(app: &tauri::AppHandle) -> PathBuf {
    let dir = app
        .path()
        .app_data_dir()
        .expect("failed to resolve app data dir");
    fs::create_dir_all(&dir).ok();
    dir.join("recognition_settings.json")
}

fn debug_settings_path(app: &tauri::AppHandle) -> PathBuf {
    let dir = app
        .path()
        .app_data_dir()
        .expect("failed to resolve app data dir");
    fs::create_dir_all(&dir).ok();
    dir.join("debug_settings.json")
}

pub(crate) fn load_server_setting(app: &tauri::AppHandle) -> Result<Server, String> {
    let path = server_settings_path(app);
    let value: serde_json::Value = read_json_or_default(&path, "服务器设置")?;
    if value.is_null() {
        return Ok(Server::default());
    }
    let server = value
        .get("server")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("服务器设置缺少 server 字段（{}）", path.display()))?;
    Server::from_str(server)
}

pub(crate) fn load_recognition_settings(
    app: &tauri::AppHandle,
) -> Result<RecognitionSettings, String> {
    let path = recognition_settings_path(app);
    let settings: RecognitionSettings = read_json_or_default(&path, "识别设置")?;
    Ok(RecognitionSettings {
        noble_phantasm_detection_mode: settings.noble_phantasm_detection_mode,
        support_ce_threshold: normalize_support_ce_threshold(settings.support_ce_threshold)?,
        support_ce_full_gate_threshold: normalize_support_ce_full_gate_threshold(
            settings.support_ce_full_gate_threshold,
        )?,
        support_mlb_icon_threshold: normalize_support_icon_threshold(
            settings.support_mlb_icon_threshold,
            "满破图标阈值",
        )?,
        support_bond_icon_threshold: normalize_support_icon_threshold(
            settings.support_bond_icon_threshold,
            "牵绊图标阈值",
        )?,
        stop_on_bond_level_up: settings.stop_on_bond_level_up && !settings.stop_on_bond_max_level,
        stop_on_bond_max_level: settings.stop_on_bond_max_level,
        auto_capture_bond_level_up: settings.auto_capture_bond_level_up,
        verify_skill_activation: settings.verify_skill_activation,
        enable_extra_class_filter: settings.enable_extra_class_filter,
        support_full_list_ocr_fallback: settings.support_full_list_ocr_fallback,
        unknown_screen_timeout_count: normalize_unknown_screen_timeout_count(
            settings.unknown_screen_timeout_count,
            "识别超时次数",
        )?,
        auto_friend_request: settings.auto_friend_request,
    })
}

pub(crate) fn load_debug_settings(app: &tauri::AppHandle) -> Result<DebugSettings, String> {
    let path = debug_settings_path(app);
    let settings = read_json_or_default(&path, "调试设置")?;
    Ok(debug_settings_for_current_build(settings))
}

fn save_recognition_settings(
    app: &tauri::AppHandle,
    settings: &RecognitionSettings,
) -> Result<(), String> {
    let path = recognition_settings_path(app);
    write_json_atomic(&path, settings, "识别设置")
}

fn save_debug_settings(app: &tauri::AppHandle, settings: &DebugSettings) -> Result<(), String> {
    let path = debug_settings_path(app);
    write_json_atomic(&path, settings, "调试设置")
}

fn debug_settings_for_current_build(settings: DebugSettings) -> DebugSettings {
    debug_settings_for_runtime(settings, cfg!(debug_assertions))
}

fn debug_settings_for_runtime(
    settings: DebugSettings,
    allow_debug_settings: bool,
) -> DebugSettings {
    if allow_debug_settings {
        settings
    } else {
        DebugSettings::default()
    }
}

fn load_last_update_check_date(app: &tauri::AppHandle) -> Result<Option<String>, String> {
    let path = update_check_settings_path(app);
    if !path.exists() {
        return Ok(None);
    }
    read_json_or_default(&path, "更新检查设置").map(Some)
}

#[tauri::command]
pub(crate) fn get_use_bluestack(_state: tauri::State<'_, Mutex<AdbDeviceSettings>>) -> bool {
    false
}

#[tauri::command]
pub(crate) fn set_use_bluestack(
    _app: tauri::AppHandle,
    _state: tauri::State<'_, Mutex<AdbDeviceSettings>>,
    _value: bool,
) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub(crate) fn get_server(state: tauri::State<'_, Mutex<Server>>) -> Server {
    *state.lock().unwrap()
}

#[tauri::command]
pub(crate) fn get_recognition_settings(
    state: tauri::State<'_, Mutex<RecognitionSettings>>,
) -> RecognitionSettings {
    *state.lock().unwrap()
}

#[tauri::command]
pub(crate) fn get_debug_settings(state: tauri::State<'_, Mutex<DebugSettings>>) -> DebugSettings {
    debug_settings_for_current_build(*state.lock().unwrap())
}

#[tauri::command]
pub(crate) fn set_noble_phantasm_detection_mode(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<RecognitionSettings>>,
    value: NoblePhantasmDetectionMode,
) -> Result<RecognitionSettings, String> {
    update_recognition_settings(&app, state.inner(), |settings| {
        settings.noble_phantasm_detection_mode = value;
    })
}

#[tauri::command]
pub(crate) fn set_support_ce_threshold(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<RecognitionSettings>>,
    value: f64,
) -> Result<RecognitionSettings, String> {
    let value = normalize_support_ce_threshold(value)?;
    update_recognition_settings(&app, state.inner(), |settings| {
        settings.support_ce_threshold = value;
    })
}

#[tauri::command]
pub(crate) fn set_support_ce_full_gate_threshold(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<RecognitionSettings>>,
    value: f64,
) -> Result<RecognitionSettings, String> {
    let value = normalize_support_ce_full_gate_threshold(value)?;
    update_recognition_settings(&app, state.inner(), |settings| {
        settings.support_ce_full_gate_threshold = value;
    })
}

#[tauri::command]
pub(crate) fn set_support_mlb_icon_threshold(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<RecognitionSettings>>,
    value: f64,
) -> Result<RecognitionSettings, String> {
    let value = normalize_support_icon_threshold(value, "满破图标阈值")?;
    update_recognition_settings(&app, state.inner(), |settings| {
        settings.support_mlb_icon_threshold = value;
    })
}

#[tauri::command]
pub(crate) fn set_support_bond_icon_threshold(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<RecognitionSettings>>,
    value: f64,
) -> Result<RecognitionSettings, String> {
    let value = normalize_support_icon_threshold(value, "牵绊图标阈值")?;
    update_recognition_settings(&app, state.inner(), |settings| {
        settings.support_bond_icon_threshold = value;
    })
}

#[tauri::command]
pub(crate) fn set_stop_on_bond_level_up(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<RecognitionSettings>>,
    value: bool,
) -> Result<RecognitionSettings, String> {
    update_recognition_settings(&app, state.inner(), |settings| {
        apply_stop_on_bond_level_up(settings, value);
    })
}

#[tauri::command]
pub(crate) fn set_stop_on_bond_max_level(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<RecognitionSettings>>,
    value: bool,
) -> Result<RecognitionSettings, String> {
    update_recognition_settings(&app, state.inner(), |settings| {
        apply_stop_on_bond_max_level(settings, value);
    })
}

#[tauri::command]
pub(crate) fn set_auto_capture_bond_level_up(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<RecognitionSettings>>,
    value: bool,
) -> Result<RecognitionSettings, String> {
    update_recognition_settings(&app, state.inner(), |settings| {
        settings.auto_capture_bond_level_up = value;
    })
}

#[tauri::command]
pub(crate) fn open_bond_level_up_screenshot_folder(app: tauri::AppHandle) -> Result<(), String> {
    let dir = bond_level_up_screenshot_dir(&app);
    fs::create_dir_all(&dir).map_err(|err| format!("创建牵绊升级截图目录失败: {err}"))?;
    app.opener()
        .open_path(dir.to_string_lossy().into_owned(), None::<String>)
        .map_err(|err| format!("打开牵绊升级截图目录失败: {err}"))
}

#[tauri::command]
pub(crate) fn set_verify_skill_activation(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<RecognitionSettings>>,
    value: bool,
) -> Result<RecognitionSettings, String> {
    update_recognition_settings(&app, state.inner(), |settings| {
        settings.verify_skill_activation = value;
    })
}

#[tauri::command]
pub(crate) fn set_enable_extra_class_filter(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<RecognitionSettings>>,
    value: bool,
) -> Result<RecognitionSettings, String> {
    update_recognition_settings(&app, state.inner(), |settings| {
        settings.enable_extra_class_filter = value;
    })
}

#[tauri::command]
pub(crate) fn set_support_full_list_ocr_fallback(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<RecognitionSettings>>,
    value: bool,
) -> Result<RecognitionSettings, String> {
    update_recognition_settings(&app, state.inner(), |settings| {
        settings.support_full_list_ocr_fallback = value;
    })
}

#[tauri::command]
pub(crate) fn set_auto_friend_request(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<RecognitionSettings>>,
    value: bool,
) -> Result<RecognitionSettings, String> {
    update_recognition_settings(&app, state.inner(), |settings| {
        settings.auto_friend_request = value;
    })
}

#[tauri::command]
pub(crate) fn set_unknown_screen_timeout_count(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<RecognitionSettings>>,
    value: u32,
) -> Result<RecognitionSettings, String> {
    let value = normalize_unknown_screen_timeout_count(value, "识别超时次数")?;
    update_unknown_screen_timeout_count(state.inner(), value, |next| {
        save_recognition_settings(&app, next)
    })
}

#[tauri::command]
pub(crate) fn set_auto_capture_battle_result_loot(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<DebugSettings>>,
    value: bool,
) -> Result<DebugSettings, String> {
    update_debug_settings(&app, state.inner(), |settings| {
        *settings = debug_settings_with_auto_capture_battle_result_loot(*settings, value);
    })
}

fn debug_settings_with_auto_capture_battle_result_loot(
    mut settings: DebugSettings,
    value: bool,
) -> DebugSettings {
    settings.auto_capture_battle_result_loot = value;
    settings
}

fn debug_settings_with_auto_capture_unknown_screen_timeout(
    mut settings: DebugSettings,
    value: bool,
) -> DebugSettings {
    settings.auto_capture_unknown_screen_timeout = value;
    settings
}

fn debug_settings_with_auto_capture_skill_use_probe(
    mut settings: DebugSettings,
    value: bool,
) -> DebugSettings {
    settings.auto_capture_skill_use_probe = value;
    settings
}

fn debug_settings_with_auto_capture_unrecognized_critical_chance(
    mut settings: DebugSettings,
    value: bool,
) -> DebugSettings {
    settings.auto_capture_unrecognized_critical_chance = value;
    settings
}

fn debug_settings_with_simulate_stuck_attack_selection(
    mut settings: DebugSettings,
    value: bool,
) -> DebugSettings {
    settings.simulate_stuck_attack_selection = value;
    settings
}

#[tauri::command]
pub(crate) fn set_auto_capture_unknown_screen_timeout(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<DebugSettings>>,
    value: bool,
) -> Result<DebugSettings, String> {
    update_debug_settings(&app, state.inner(), |settings| {
        *settings = debug_settings_with_auto_capture_unknown_screen_timeout(*settings, value);
    })
}

#[tauri::command]
pub(crate) fn set_auto_capture_skill_use_probe(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<DebugSettings>>,
    value: bool,
) -> Result<DebugSettings, String> {
    update_debug_settings(&app, state.inner(), |settings| {
        *settings = debug_settings_with_auto_capture_skill_use_probe(*settings, value);
    })
}

#[tauri::command]
pub(crate) fn set_auto_capture_unrecognized_critical_chance(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<DebugSettings>>,
    value: bool,
) -> Result<DebugSettings, String> {
    update_debug_settings(&app, state.inner(), |settings| {
        *settings = debug_settings_with_auto_capture_unrecognized_critical_chance(*settings, value);
    })
}

#[tauri::command]
pub(crate) fn set_simulate_stuck_attack_selection(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<DebugSettings>>,
    value: bool,
) -> Result<DebugSettings, String> {
    update_debug_settings(&app, state.inner(), |settings| {
        *settings = debug_settings_with_simulate_stuck_attack_selection(*settings, value);
    })
}

fn take_simulate_stuck_attack_selection(
    state: &Mutex<DebugSettings>,
    persist: impl FnOnce(&DebugSettings) -> Result<(), String>,
) -> Result<bool, String> {
    let mut guard = state.lock().unwrap();
    let current = debug_settings_for_current_build(*guard);
    if !current.simulate_stuck_attack_selection {
        return Ok(false);
    }

    let next = debug_settings_with_simulate_stuck_attack_selection(current, false);
    persist(&next)?;
    *guard = next;
    Ok(true)
}

pub(crate) fn consume_simulate_stuck_attack_selection(
    app: &tauri::AppHandle,
) -> Result<bool, String> {
    let state = app
        .try_state::<Mutex<DebugSettings>>()
        .ok_or_else(|| "调试设置尚未初始化".to_string())?;
    take_simulate_stuck_attack_selection(state.inner(), |next| save_debug_settings(app, next))
}

#[tauri::command]
pub(crate) async fn run_startup_migration(
    app: tauri::AppHandle,
    adb_settings_state: tauri::State<'_, Mutex<AdbDeviceSettings>>,
    server_state: tauri::State<'_, Mutex<Server>>,
) -> Result<StartupMigrationStatus, String> {
    let migration_app = app.clone();
    let status =
        tauri::async_runtime::spawn_blocking(move || migrate_legacy_app_data(&migration_app))
            .await
            .map_err(|e| format!("startup migration task failed: {e}"))??;
    if status.migrated {
        *adb_settings_state.lock().unwrap() = load_adb_device_settings(&app)?;
        *server_state.lock().unwrap() = load_server_setting(&app)?;
    }
    crate::battle_statistics::initialize(&app)?;
    Ok(status)
}

#[cfg(test)]
mod tests;

#[tauri::command]
pub(crate) fn set_server(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<Server>>,
    coordinator: tauri::State<'_, AutomationCoordinator>,
    debug_state: tauri::State<'_, debug::DebugSidecar>,
    value: Server,
) -> Result<(), String> {
    // Refuse to flip mid-run: the runner cached templates / OCR model /
    // localized servant metadata for the *previous* server when it spawned;
    // changing the global setting now would silently desync those caches.
    coordinator
        .require_idle()
        .map_err(|_| "自动化正在运行中，请先停止后再切换服务器".to_string())?;

    let path = server_settings_path(&app);
    let json = serde_json::json!({ "server": value.to_string() });
    write_json_atomic(&path, &json, "服务器设置")?;
    *state.lock().unwrap() = value;

    // Tear down any idle sidecar so the next debug or automation call
    // respawns it with the new server's templates / OCR model.
    {
        let mut guard = debug_state.0.lock().unwrap();
        guard.take();
    }

    Ok(())
}

#[tauri::command]
pub(crate) fn should_check_updates_today(
    app: tauri::AppHandle,
    today: String,
) -> Result<bool, String> {
    Ok(load_last_update_check_date(&app)?.as_deref() != Some(today.as_str()))
}

#[tauri::command]
pub(crate) fn mark_update_checked_today(app: tauri::AppHandle, date: String) -> Result<(), String> {
    let path = update_check_settings_path(&app);
    write_json_atomic(&path, &date, "更新检查设置")
}
