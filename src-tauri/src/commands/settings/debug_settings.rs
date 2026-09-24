//! Debug-only settings persistence and IPC commands.

use super::update_persisted_state;
use crate::paths::app_data_dir;
use crate::storage::{read_json_or_default, write_json_atomic};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::Manager;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImageRecognitionDebugMode {
    Enabled,
    Shadow,
    #[default]
    Disabled,
}

impl ImageRecognitionDebugMode {
    pub(crate) fn as_sidecar_value(self) -> &'static str {
        match self {
            Self::Enabled => "enabled",
            Self::Shadow => "shadow",
            Self::Disabled => "disabled",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugSettings {
    #[serde(default)]
    pub image_recognition_debug_mode: ImageRecognitionDebugMode,
    #[serde(default)]
    pub sequence_recognition_debug_mode: ImageRecognitionDebugMode,
    #[serde(default)]
    pub auto_capture_battle_before_attack: bool,
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

fn debug_settings_path(app: &tauri::AppHandle) -> PathBuf {
    app_data_dir(app).join("debug_settings.json")
}

pub(crate) fn load_debug_settings(app: &tauri::AppHandle) -> Result<DebugSettings, String> {
    let path = debug_settings_path(app);
    let settings = read_json_or_default(&path, "调试设置")?;
    Ok(debug_settings_for_current_build(settings))
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

#[tauri::command]
pub(crate) fn get_debug_settings(state: tauri::State<'_, Mutex<DebugSettings>>) -> DebugSettings {
    debug_settings_for_current_build(*state.lock().unwrap())
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

fn debug_settings_with_image_recognition_debug_mode(
    mut settings: DebugSettings,
    value: ImageRecognitionDebugMode,
) -> DebugSettings {
    settings.image_recognition_debug_mode = value;
    settings
}

fn debug_settings_with_sequence_recognition_debug_mode(
    mut settings: DebugSettings,
    value: ImageRecognitionDebugMode,
) -> DebugSettings {
    settings.sequence_recognition_debug_mode = value;
    settings
}

fn debug_settings_with_auto_capture_battle_before_attack(
    mut settings: DebugSettings,
    value: bool,
) -> DebugSettings {
    settings.auto_capture_battle_before_attack = value;
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
pub(crate) fn set_auto_capture_battle_before_attack(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<DebugSettings>>,
    value: bool,
) -> Result<DebugSettings, String> {
    update_debug_settings(&app, state.inner(), |settings| {
        *settings = debug_settings_with_auto_capture_battle_before_attack(*settings, value);
    })
}

#[tauri::command]
pub(crate) fn set_image_recognition_debug_mode(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<DebugSettings>>,
    value: ImageRecognitionDebugMode,
) -> Result<DebugSettings, String> {
    update_debug_settings(&app, state.inner(), |settings| {
        *settings = debug_settings_with_image_recognition_debug_mode(*settings, value);
    })
}

#[tauri::command]
pub(crate) fn set_sequence_recognition_debug_mode(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<DebugSettings>>,
    value: ImageRecognitionDebugMode,
) -> Result<DebugSettings, String> {
    update_debug_settings(&app, state.inner(), |settings| {
        *settings = debug_settings_with_sequence_recognition_debug_mode(*settings, value);
    })
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

#[cfg(test)]
mod tests;
