//! Persisted application settings and related IPC commands.
//!
//! Settings in this module are backend-owned state: commands update the shared
//! Tauri state and persist the matching JSON file under app_data_dir(). Server
//! switches also invalidate idle sidecars so future CV calls load the selected
//! server resources.

use crate::adb::BLUESTACKS_SERIAL;
use crate::commands::debug;
use crate::craft_essence_enhancement_runner::{
    CraftEssenceEnhancementRunnerHandle, CraftEssenceEnhancementRunnerState,
};
use crate::enhancement_runner::{EnhancementRunnerHandle, EnhancementRunnerState};
use crate::friend_point_summon_runner::{
    FriendPointSummonRunnerHandle, FriendPointSummonRunnerState,
};
use crate::paths::{migrate_legacy_app_data, StartupMigrationStatus};
use crate::runner::{RunnerHandle, RunnerState};
use crate::Server;
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Mutex;
use tauri::Manager;

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
    pub verify_skill_activation: bool,
    #[serde(default = "default_true")]
    pub enable_extra_class_filter: bool,
    #[serde(default = "default_unknown_screen_timeout_count")]
    pub unknown_screen_timeout_count: u32,
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
            verify_skill_activation: false,
            enable_extra_class_filter: true,
            unknown_screen_timeout_count: UNKNOWN_SCREEN_TIMEOUT_COUNT_DEFAULT,
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

pub(crate) fn load_adb_device_settings(app: &tauri::AppHandle) -> AdbDeviceSettings {
    let path = adb_settings_path(app);
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .map(|v| adb_device_settings_from_value(&v))
        .unwrap_or_default()
}

pub(crate) fn save_adb_device_settings(
    app: &tauri::AppHandle,
    settings: &AdbDeviceSettings,
) -> Result<(), String> {
    let path = adb_settings_path(app);
    fs::write(
        &path,
        serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
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

pub(crate) fn load_server_setting(app: &tauri::AppHandle) -> Server {
    let path = server_settings_path(app);
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| {
            v.get("server")
                .and_then(|s| s.as_str())
                .map(|s| s.to_string())
        })
        .and_then(|s| Server::from_str(&s).ok())
        .unwrap_or_default()
}

pub(crate) fn load_recognition_settings(app: &tauri::AppHandle) -> RecognitionSettings {
    let path = recognition_settings_path(app);
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<RecognitionSettings>(&s).ok())
        .and_then(|settings| {
            Some(RecognitionSettings {
                noble_phantasm_detection_mode: settings.noble_phantasm_detection_mode,
                support_ce_threshold: normalize_support_ce_threshold(settings.support_ce_threshold)
                    .ok()?,
                support_ce_full_gate_threshold: normalize_support_ce_full_gate_threshold(
                    settings.support_ce_full_gate_threshold,
                )
                .ok()?,
                support_mlb_icon_threshold: normalize_support_icon_threshold(
                    settings.support_mlb_icon_threshold,
                    "满破图标阈值",
                )
                .ok()?,
                support_bond_icon_threshold: normalize_support_icon_threshold(
                    settings.support_bond_icon_threshold,
                    "牵绊图标阈值",
                )
                .ok()?,
                stop_on_bond_level_up: settings.stop_on_bond_level_up
                    && !settings.stop_on_bond_max_level,
                stop_on_bond_max_level: settings.stop_on_bond_max_level,
                verify_skill_activation: settings.verify_skill_activation,
                enable_extra_class_filter: settings.enable_extra_class_filter,
                unknown_screen_timeout_count: normalize_unknown_screen_timeout_count(
                    settings.unknown_screen_timeout_count,
                    "识别超时次数",
                )
                .ok()?,
            })
        })
        .unwrap_or_default()
}

pub(crate) fn load_debug_settings(app: &tauri::AppHandle) -> DebugSettings {
    let path = debug_settings_path(app);
    let settings = fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<DebugSettings>(&s).ok())
        .unwrap_or_default();
    debug_settings_for_current_build(settings)
}

fn save_recognition_settings(
    app: &tauri::AppHandle,
    settings: &RecognitionSettings,
) -> Result<(), String> {
    let path = recognition_settings_path(app);
    fs::write(
        &path,
        serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

fn save_debug_settings(app: &tauri::AppHandle, settings: &DebugSettings) -> Result<(), String> {
    let path = debug_settings_path(app);
    fs::write(
        &path,
        serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
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

fn load_last_update_check_date(app: &tauri::AppHandle) -> Option<String> {
    let path = update_check_settings_path(app);
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<String>(&text).ok())
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
    let mut next = *state.lock().unwrap();
    next.noble_phantasm_detection_mode = value;
    *state.lock().unwrap() = next;
    save_recognition_settings(&app, &next)?;
    Ok(next)
}

#[tauri::command]
pub(crate) fn set_support_ce_threshold(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<RecognitionSettings>>,
    value: f64,
) -> Result<RecognitionSettings, String> {
    let value = normalize_support_ce_threshold(value)?;
    let mut next = *state.lock().unwrap();
    next.support_ce_threshold = value;
    *state.lock().unwrap() = next;
    save_recognition_settings(&app, &next)?;
    Ok(next)
}

#[tauri::command]
pub(crate) fn set_support_ce_full_gate_threshold(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<RecognitionSettings>>,
    value: f64,
) -> Result<RecognitionSettings, String> {
    let value = normalize_support_ce_full_gate_threshold(value)?;
    let mut next = *state.lock().unwrap();
    next.support_ce_full_gate_threshold = value;
    *state.lock().unwrap() = next;
    save_recognition_settings(&app, &next)?;
    Ok(next)
}

#[tauri::command]
pub(crate) fn set_support_mlb_icon_threshold(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<RecognitionSettings>>,
    value: f64,
) -> Result<RecognitionSettings, String> {
    let value = normalize_support_icon_threshold(value, "满破图标阈值")?;
    let mut next = *state.lock().unwrap();
    next.support_mlb_icon_threshold = value;
    *state.lock().unwrap() = next;
    save_recognition_settings(&app, &next)?;
    Ok(next)
}

#[tauri::command]
pub(crate) fn set_support_bond_icon_threshold(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<RecognitionSettings>>,
    value: f64,
) -> Result<RecognitionSettings, String> {
    let value = normalize_support_icon_threshold(value, "牵绊图标阈值")?;
    let mut next = *state.lock().unwrap();
    next.support_bond_icon_threshold = value;
    *state.lock().unwrap() = next;
    save_recognition_settings(&app, &next)?;
    Ok(next)
}

#[tauri::command]
pub(crate) fn set_stop_on_bond_level_up(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<RecognitionSettings>>,
    value: bool,
) -> Result<RecognitionSettings, String> {
    let mut next = *state.lock().unwrap();
    apply_stop_on_bond_level_up(&mut next, value);
    *state.lock().unwrap() = next;
    save_recognition_settings(&app, &next)?;
    Ok(next)
}

#[tauri::command]
pub(crate) fn set_stop_on_bond_max_level(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<RecognitionSettings>>,
    value: bool,
) -> Result<RecognitionSettings, String> {
    let mut next = *state.lock().unwrap();
    apply_stop_on_bond_max_level(&mut next, value);
    *state.lock().unwrap() = next;
    save_recognition_settings(&app, &next)?;
    Ok(next)
}

#[tauri::command]
pub(crate) fn set_verify_skill_activation(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<RecognitionSettings>>,
    value: bool,
) -> Result<RecognitionSettings, String> {
    let mut next = *state.lock().unwrap();
    next.verify_skill_activation = value;
    *state.lock().unwrap() = next;
    save_recognition_settings(&app, &next)?;
    Ok(next)
}

#[tauri::command]
pub(crate) fn set_enable_extra_class_filter(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<RecognitionSettings>>,
    value: bool,
) -> Result<RecognitionSettings, String> {
    let mut next = *state.lock().unwrap();
    next.enable_extra_class_filter = value;
    *state.lock().unwrap() = next;
    save_recognition_settings(&app, &next)?;
    Ok(next)
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
    let next = debug_settings_for_current_build(
        debug_settings_with_auto_capture_battle_result_loot(*state.lock().unwrap(), value),
    );
    *state.lock().unwrap() = next;
    save_debug_settings(&app, &next)?;
    Ok(next)
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

#[tauri::command]
pub(crate) fn set_auto_capture_unknown_screen_timeout(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<DebugSettings>>,
    value: bool,
) -> Result<DebugSettings, String> {
    let next = debug_settings_for_current_build(
        debug_settings_with_auto_capture_unknown_screen_timeout(*state.lock().unwrap(), value),
    );
    *state.lock().unwrap() = next;
    save_debug_settings(&app, &next)?;
    Ok(next)
}

#[tauri::command]
pub(crate) fn set_auto_capture_skill_use_probe(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<DebugSettings>>,
    value: bool,
) -> Result<DebugSettings, String> {
    let next = debug_settings_for_current_build(debug_settings_with_auto_capture_skill_use_probe(
        *state.lock().unwrap(),
        value,
    ));
    *state.lock().unwrap() = next;
    save_debug_settings(&app, &next)?;
    Ok(next)
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
        *adb_settings_state.lock().unwrap() = load_adb_device_settings(&app);
        *server_state.lock().unwrap() = load_server_setting(&app);
    }
    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn support_ce_threshold_accepts_configured_range() {
        assert_eq!(normalize_support_ce_threshold(0.60).unwrap(), 0.60);
        assert_eq!(normalize_support_ce_threshold(0.70).unwrap(), 0.70);
        assert_eq!(normalize_support_ce_threshold(0.85).unwrap(), 0.85);
    }

    #[test]
    fn support_ce_threshold_rejects_invalid_values() {
        assert!(normalize_support_ce_threshold(0.59).is_err());
        assert!(normalize_support_ce_threshold(0.86).is_err());
        assert!(normalize_support_ce_threshold(f64::NAN).is_err());
    }

    #[test]
    fn support_icon_threshold_accepts_configured_range() {
        assert_eq!(
            normalize_support_icon_threshold(0.60, "满破图标阈值").unwrap(),
            0.60
        );
        assert_eq!(
            normalize_support_icon_threshold(0.70, "满破图标阈值").unwrap(),
            0.70
        );
        assert_eq!(
            normalize_support_icon_threshold(0.85, "满破图标阈值").unwrap(),
            0.85
        );
    }

    #[test]
    fn support_icon_threshold_rejects_invalid_values() {
        assert!(normalize_support_icon_threshold(0.59, "牵绊图标阈值").is_err());
        assert!(normalize_support_icon_threshold(0.86, "牵绊图标阈值").is_err());
        assert!(normalize_support_icon_threshold(f64::NAN, "牵绊图标阈值").is_err());
    }

    #[test]
    fn support_ce_full_gate_threshold_accepts_configured_range() {
        assert_eq!(
            normalize_support_ce_full_gate_threshold(0.40).unwrap(),
            0.40
        );
        assert_eq!(
            normalize_support_ce_full_gate_threshold(0.60).unwrap(),
            0.60
        );
        assert_eq!(
            normalize_support_ce_full_gate_threshold(0.70).unwrap(),
            0.70
        );
    }

    #[test]
    fn support_ce_full_gate_threshold_rejects_invalid_values() {
        assert!(normalize_support_ce_full_gate_threshold(0.39).is_err());
        assert!(normalize_support_ce_full_gate_threshold(0.71).is_err());
        assert!(normalize_support_ce_full_gate_threshold(f64::NAN).is_err());
    }

    #[test]
    fn recognition_settings_default_disables_bond_auto_stop() {
        let settings = RecognitionSettings::default();

        assert!(!settings.stop_on_bond_level_up);
        assert!(!settings.stop_on_bond_max_level);
        assert!(!settings.verify_skill_activation);
        assert!(settings.enable_extra_class_filter);
        assert_eq!(
            settings.unknown_screen_timeout_count,
            UNKNOWN_SCREEN_TIMEOUT_COUNT_DEFAULT
        );
    }

    #[test]
    fn recognition_settings_deserializes_legacy_json_with_bond_auto_stop_defaults() {
        let settings: RecognitionSettings = serde_json::from_value(serde_json::json!({
            "noblePhantasmDetectionMode": "card",
            "supportCeThreshold": 0.7,
            "supportCeFullGateThreshold": 0.6,
            "supportMlbIconThreshold": 0.7,
            "supportBondIconThreshold": 0.7
        }))
        .unwrap();

        assert!(!settings.stop_on_bond_level_up);
        assert!(!settings.stop_on_bond_max_level);
        assert!(!settings.verify_skill_activation);
        assert!(settings.enable_extra_class_filter);
        assert_eq!(
            settings.unknown_screen_timeout_count,
            UNKNOWN_SCREEN_TIMEOUT_COUNT_DEFAULT
        );
    }

    #[test]
    fn recognition_settings_round_trips_disabled_extra_class_filter() {
        let settings = RecognitionSettings {
            enable_extra_class_filter: false,
            ..RecognitionSettings::default()
        };

        let serialized = serde_json::to_value(settings).unwrap();
        assert_eq!(
            serialized["enableExtraClassFilter"],
            serde_json::json!(false)
        );

        let restored: RecognitionSettings = serde_json::from_value(serialized).unwrap();
        assert!(!restored.enable_extra_class_filter);
    }

    #[test]
    fn unknown_screen_timeout_count_accepts_configured_range() {
        assert_eq!(
            normalize_unknown_screen_timeout_count(
                UNKNOWN_SCREEN_TIMEOUT_COUNT_MIN,
                "识别超时次数"
            )
            .unwrap(),
            UNKNOWN_SCREEN_TIMEOUT_COUNT_MIN
        );
        assert_eq!(
            normalize_unknown_screen_timeout_count(
                UNKNOWN_SCREEN_TIMEOUT_COUNT_MAX,
                "识别超时次数"
            )
            .unwrap(),
            UNKNOWN_SCREEN_TIMEOUT_COUNT_MAX
        );
        assert_eq!(
            normalize_unknown_screen_timeout_count(
                UNKNOWN_SCREEN_TIMEOUT_COUNT_UNLIMITED,
                "识别超时次数"
            )
            .unwrap(),
            UNKNOWN_SCREEN_TIMEOUT_COUNT_UNLIMITED
        );
    }

    #[test]
    fn unknown_screen_timeout_count_rejects_values_outside_range() {
        assert!(normalize_unknown_screen_timeout_count(0, "识别超时次数").is_err());
        assert!(normalize_unknown_screen_timeout_count(
            UNKNOWN_SCREEN_TIMEOUT_COUNT_MAX + 1,
            "识别超时次数"
        )
        .is_err());
    }

    #[test]
    fn failed_timeout_persistence_keeps_shared_state_unchanged() {
        let state = Mutex::new(RecognitionSettings::default());

        let result =
            update_unknown_screen_timeout_count(&state, 200, |_| Err("write failed".to_string()));

        assert_eq!(result.unwrap_err(), "write failed");
        assert_eq!(
            state.lock().unwrap().unknown_screen_timeout_count,
            UNKNOWN_SCREEN_TIMEOUT_COUNT_DEFAULT
        );
    }

    #[test]
    fn successful_timeout_persistence_updates_shared_state() {
        let state = Mutex::new(RecognitionSettings::default());

        let result = update_unknown_screen_timeout_count(&state, 200, |_| Ok(())).unwrap();

        assert_eq!(result.unknown_screen_timeout_count, 200);
        assert_eq!(state.lock().unwrap().unknown_screen_timeout_count, 200);
    }

    #[test]
    fn debug_settings_default_disables_debug_captures() {
        let settings = DebugSettings::default();

        assert!(!settings.auto_capture_battle_result_loot);
        assert!(!settings.auto_capture_unknown_screen_timeout);
        assert!(!settings.auto_capture_skill_use_probe);
    }

    #[test]
    fn debug_settings_deserializes_legacy_json_with_debug_capture_defaults() {
        let settings: DebugSettings = serde_json::from_value(serde_json::json!({})).unwrap();

        assert!(!settings.auto_capture_battle_result_loot);
        assert!(!settings.auto_capture_unknown_screen_timeout);
        assert!(!settings.auto_capture_skill_use_probe);
    }

    #[test]
    fn debug_settings_round_trips_debug_capture_settings() {
        let settings: DebugSettings = serde_json::from_value(serde_json::json!({
            "autoCaptureBattleResultLoot": true,
            "autoCaptureUnknownScreenTimeout": true,
            "autoCaptureSkillUseProbe": true,
        }))
        .unwrap();

        assert!(settings.auto_capture_battle_result_loot);
        assert!(settings.auto_capture_unknown_screen_timeout);
        assert!(settings.auto_capture_skill_use_probe);
        assert_eq!(
            serde_json::to_value(settings).unwrap()["autoCaptureBattleResultLoot"],
            serde_json::json!(true)
        );
        assert_eq!(
            serde_json::to_value(settings).unwrap()["autoCaptureUnknownScreenTimeout"],
            serde_json::json!(true)
        );
        assert_eq!(
            serde_json::to_value(settings).unwrap()["autoCaptureSkillUseProbe"],
            serde_json::json!(true)
        );
    }

    #[test]
    fn debug_settings_runtime_filter_forces_debug_captures_off_when_disallowed() {
        let settings = DebugSettings {
            auto_capture_battle_result_loot: true,
            auto_capture_unknown_screen_timeout: true,
            auto_capture_skill_use_probe: true,
        };

        let filtered = debug_settings_for_runtime(settings, false);

        assert!(!filtered.auto_capture_battle_result_loot);
        assert!(!filtered.auto_capture_unknown_screen_timeout);
        assert!(!filtered.auto_capture_skill_use_probe);
    }

    #[test]
    fn debug_settings_runtime_filter_preserves_debug_captures_when_allowed() {
        let settings = DebugSettings {
            auto_capture_battle_result_loot: true,
            auto_capture_unknown_screen_timeout: true,
            auto_capture_skill_use_probe: true,
        };

        let filtered = debug_settings_for_runtime(settings, true);

        assert!(filtered.auto_capture_battle_result_loot);
        assert!(filtered.auto_capture_unknown_screen_timeout);
        assert!(filtered.auto_capture_skill_use_probe);
    }

    #[test]
    fn debug_settings_auto_loot_update_preserves_unknown_timeout_capture() {
        let settings = DebugSettings {
            auto_capture_battle_result_loot: false,
            auto_capture_unknown_screen_timeout: true,
            auto_capture_skill_use_probe: false,
        };

        let next = debug_settings_with_auto_capture_battle_result_loot(settings, true);

        assert!(next.auto_capture_battle_result_loot);
        assert!(next.auto_capture_unknown_screen_timeout);
        assert!(!next.auto_capture_skill_use_probe);
    }

    #[test]
    fn debug_settings_unknown_timeout_update_preserves_auto_loot_capture() {
        let settings = DebugSettings {
            auto_capture_battle_result_loot: true,
            auto_capture_unknown_screen_timeout: false,
            auto_capture_skill_use_probe: false,
        };

        let next = debug_settings_with_auto_capture_unknown_screen_timeout(settings, true);

        assert!(next.auto_capture_battle_result_loot);
        assert!(next.auto_capture_unknown_screen_timeout);
        assert!(!next.auto_capture_skill_use_probe);
    }

    #[test]
    fn debug_settings_skill_use_probe_update_preserves_other_captures() {
        let settings = DebugSettings {
            auto_capture_battle_result_loot: true,
            auto_capture_unknown_screen_timeout: true,
            auto_capture_skill_use_probe: false,
        };

        let next = debug_settings_with_auto_capture_skill_use_probe(settings, true);

        assert!(next.auto_capture_battle_result_loot);
        assert!(next.auto_capture_unknown_screen_timeout);
        assert!(next.auto_capture_skill_use_probe);
    }

    #[test]
    fn adb_device_settings_migrates_legacy_bluestack_setting() {
        let settings = adb_device_settings_from_value(&serde_json::json!({
            "useBluestack": true,
        }));

        assert_eq!(
            settings.selected_adb_serial.as_deref(),
            Some(BLUESTACKS_SERIAL)
        );
    }

    #[test]
    fn adb_device_settings_prefers_selected_serial_over_legacy_flag() {
        let settings = adb_device_settings_from_value(&serde_json::json!({
            "useBluestack": true,
            "selectedAdbSerial": " emulator-5554 ",
        }));

        assert_eq!(
            settings.selected_adb_serial.as_deref(),
            Some("emulator-5554")
        );
    }

    #[test]
    fn enabling_bond_max_auto_stop_disables_level_up_auto_stop() {
        let mut settings = RecognitionSettings {
            stop_on_bond_level_up: true,
            ..RecognitionSettings::default()
        };

        apply_stop_on_bond_max_level(&mut settings, true);

        assert!(!settings.stop_on_bond_level_up);
        assert!(settings.stop_on_bond_max_level);
    }

    #[test]
    fn disabling_bond_level_up_auto_stop_does_not_affect_max_auto_stop() {
        let mut settings = RecognitionSettings {
            stop_on_bond_level_up: true,
            stop_on_bond_max_level: true,
            ..RecognitionSettings::default()
        };

        apply_stop_on_bond_level_up(&mut settings, false);

        assert!(!settings.stop_on_bond_level_up);
        assert!(settings.stop_on_bond_max_level);
    }

    #[test]
    fn loading_normalizes_bond_max_auto_stop_to_disable_level_up() {
        let settings = RecognitionSettings {
            stop_on_bond_level_up: true,
            stop_on_bond_max_level: true,
            ..RecognitionSettings::default()
        };
        let normalized = RecognitionSettings {
            stop_on_bond_level_up: settings.stop_on_bond_level_up
                && !settings.stop_on_bond_max_level,
            ..settings
        };

        assert!(!normalized.stop_on_bond_level_up);
        assert!(normalized.stop_on_bond_max_level);
    }
}

#[tauri::command]
pub(crate) fn set_server(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<Server>>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    ce_enhancement_handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    friend_point_summon_handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
    debug_state: tauri::State<'_, debug::DebugSidecar>,
    value: Server,
) -> Result<(), String> {
    // Refuse to flip mid-run: the runner cached templates / OCR model /
    // localized servant metadata for the *previous* server when it spawned;
    // changing the global setting now would silently desync those caches.
    {
        let handle = handle_state.lock().unwrap();
        let running = matches!(*handle.state.lock().unwrap(), RunnerState::Running);
        if running {
            return Err("自动化正在运行中，请先停止后再切换服务器".into());
        }
    }
    {
        let handle = enhancement_handle_state.lock().unwrap();
        let running = matches!(
            *handle.state.lock().unwrap(),
            EnhancementRunnerState::Running
        );
        if running {
            return Err("强化自动化正在运行中，请先停止后再切换服务器".into());
        }
    }
    {
        let handle = ce_enhancement_handle_state.lock().unwrap();
        let running = matches!(
            *handle.state.lock().unwrap(),
            CraftEssenceEnhancementRunnerState::Starting
                | CraftEssenceEnhancementRunnerState::Running
        );
        if running {
            return Err("概念礼装强化自动化正在运行中，请先停止后再切换服务器".into());
        }
    }
    {
        let handle = friend_point_summon_handle_state.lock().unwrap();
        let running = matches!(
            *handle.state.lock().unwrap(),
            FriendPointSummonRunnerState::Starting | FriendPointSummonRunnerState::Running
        );
        if running {
            return Err("友情点抽取自动化正在运行中，请先停止后再切换服务器".into());
        }
    }

    *state.lock().unwrap() = value;
    let path = server_settings_path(&app);
    let json = serde_json::json!({ "server": value.to_string() });
    fs::write(
        &path,
        serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

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
    Ok(load_last_update_check_date(&app).as_deref() != Some(today.as_str()))
}

#[tauri::command]
pub(crate) fn mark_update_checked_today(app: tauri::AppHandle, date: String) -> Result<(), String> {
    let path = update_check_settings_path(&app);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let body = serde_json::to_string(&date).map_err(|e| e.to_string())?;
    fs::write(path, body).map_err(|e| e.to_string())
}
