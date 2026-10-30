//! Persisted input preference, shared by stream startup and the settings UI.

use super::update_persisted_state;
use crate::paths::app_data_dir;
use crate::storage::{read_json_or_default, write_json_atomic};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TouchControlMode {
    #[default]
    Scrcpy,
    Adb,
}

#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TouchControlSettings {
    #[serde(default)]
    pub mode: TouchControlMode,
}

#[derive(Default)]
pub(crate) struct TouchControlSettingsState(pub Arc<Mutex<TouchControlSettings>>);

fn read_settings(path: &Path) -> Result<TouchControlSettings, String> {
    read_json_or_default(path, "触控设置")
}

fn write_settings(path: &Path, settings: &TouchControlSettings) -> Result<(), String> {
    write_json_atomic(path, settings, "触控设置")
}

pub(crate) fn load_touch_control_settings(
    app: &tauri::AppHandle,
) -> Result<TouchControlSettings, String> {
    read_settings(&app_data_dir(app).join("touch_control_settings.json"))
}

#[tauri::command]
pub(crate) fn get_touch_control_mode(
    state: tauri::State<'_, TouchControlSettingsState>,
) -> TouchControlMode {
    state.0.lock().unwrap().mode
}

#[tauri::command]
pub(crate) fn set_touch_control_mode(
    app: tauri::AppHandle,
    state: tauri::State<'_, TouchControlSettingsState>,
    value: TouchControlMode,
) -> Result<TouchControlMode, String> {
    let saved = update_persisted_state(
        &state.0,
        |settings| settings.mode = value,
        |settings| {
            write_settings(
                &app_data_dir(&app).join("touch_control_settings.json"),
                settings,
            )
        },
    )?;
    Ok(saved.mode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preference_defaults_and_round_trips_both_control_modes() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("touch.json");
        assert_eq!(read_settings(&path).unwrap().mode, TouchControlMode::Scrcpy);
        assert_eq!(
            serde_json::from_str::<TouchControlSettings>("{}")
                .unwrap()
                .mode,
            TouchControlMode::Scrcpy
        );
        for (mode, value) in [
            (TouchControlMode::Adb, "adb"),
            (TouchControlMode::Scrcpy, "scrcpy"),
        ] {
            write_settings(&path, &TouchControlSettings { mode }).unwrap();
            assert_eq!(read_settings(&path).unwrap().mode, mode);
            assert_eq!(serde_json::to_value(mode).unwrap(), value);
        }
        assert!(serde_json::from_str::<TouchControlMode>(r#""unknown""#).is_err());
    }

    #[test]
    fn failed_save_retains_the_previous_backend_preference() {
        let temp = tempfile::tempdir().unwrap();
        let state = Mutex::new(TouchControlSettings::default());
        let result = update_persisted_state(
            &state,
            |settings| settings.mode = TouchControlMode::Adb,
            |settings| write_settings(temp.path(), settings),
        );
        assert!(result.is_err());
        assert_eq!(state.lock().unwrap().mode, TouchControlMode::Scrcpy);
    }
}
