//! Persisted application UI preferences and their Tauri commands.

use super::*;

pub(crate) fn read_app_ui_settings_from_path(path: &Path) -> Result<AppUiSettings, String> {
    read_json_or_default(path, "界面设置")
}

pub(crate) fn write_app_ui_settings_to_path(
    path: &Path,
    settings: &AppUiSettings,
) -> Result<(), String> {
    write_json_atomic(path, settings, "界面设置")
}

pub(crate) fn update_app_ui_settings(
    app: &tauri::AppHandle,
    state: &Mutex<AppUiSettings>,
    update: impl FnOnce(&mut AppUiSettings),
) -> Result<AppUiSettings, String> {
    let mut guard = state.lock().unwrap();
    let mut next = guard.clone();
    update(&mut next);
    write_app_ui_settings_to_path(&app_ui_settings_path(app), &next)?;
    *guard = next.clone();
    Ok(next)
}

#[tauri::command]
pub(crate) fn get_active_project_id(
    state: tauri::State<'_, Mutex<AppUiSettings>>,
) -> Option<String> {
    state.lock().unwrap().active_project_id.clone()
}

#[tauri::command]
pub(crate) fn set_active_project_id(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<AppUiSettings>>,
    active_project_id: Option<String>,
) -> Result<(), String> {
    update_app_ui_settings(&app, state.inner(), |settings| {
        settings.active_project_id = active_project_id;
    })?;
    Ok(())
}

#[tauri::command]
pub(crate) fn get_app_theme(state: tauri::State<'_, Mutex<AppUiSettings>>) -> Option<String> {
    state
        .lock()
        .unwrap()
        .theme
        .clone()
        .filter(|theme| theme == "light" || theme == "dark" || theme == "system")
}

#[tauri::command]
pub(crate) fn set_app_theme(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<AppUiSettings>>,
    theme: String,
) -> Result<(), String> {
    if theme != "light" && theme != "dark" && theme != "system" {
        return Err(format!("invalid app theme: {theme}"));
    }
    update_app_ui_settings(&app, state.inner(), |settings| {
        settings.theme = Some(theme);
    })?;
    Ok(())
}

#[tauri::command]
pub(crate) fn get_battle_start_panel(
    state: tauri::State<'_, Mutex<AppUiSettings>>,
) -> crate::paths::BattleStartPanel {
    state.lock().unwrap().battle_start_panel
}

#[tauri::command]
pub(crate) fn set_battle_start_panel(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<AppUiSettings>>,
    value: crate::paths::BattleStartPanel,
) -> Result<crate::paths::BattleStartPanel, String> {
    update_app_ui_settings(&app, state.inner(), |settings| {
        settings.battle_start_panel = value;
    })?;
    Ok(value)
}

#[tauri::command]
pub(crate) fn get_mystic_code_gender(
    state: tauri::State<'_, Mutex<AppUiSettings>>,
) -> crate::paths::MysticCodeGender {
    state.lock().unwrap().mystic_code_gender
}

#[tauri::command]
pub(crate) fn set_mystic_code_gender(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<AppUiSettings>>,
    value: crate::paths::MysticCodeGender,
) -> Result<crate::paths::MysticCodeGender, String> {
    update_app_ui_settings(&app, state.inner(), |settings| {
        settings.mystic_code_gender = value;
    })?;
    Ok(value)
}
