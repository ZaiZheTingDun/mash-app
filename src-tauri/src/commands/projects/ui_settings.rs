//! Persisted application UI preferences and their Tauri commands.

use super::*;
use crate::commands::runtime::resolve_mystic_code_assets_dir;

pub(crate) const DEFAULT_HOME_MASTER_FIGURE_ID: u32 = 470;

fn home_master_figure_exists(root: &Path, id: u32) -> bool {
    let code_dir = root.join(id.to_string());
    code_dir.join("master-figure-female.png").is_file()
        && code_dir.join("master-figure-male.png").is_file()
}

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
pub(crate) fn get_app_color_theme(
    state: tauri::State<'_, Mutex<AppUiSettings>>,
) -> Result<crate::paths::AppColorTheme, String> {
    Ok(state.lock().map_err(|err| err.to_string())?.color_theme)
}

#[tauri::command]
pub(crate) fn set_app_color_theme(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<AppUiSettings>>,
    value: crate::paths::AppColorTheme,
) -> Result<crate::paths::AppColorTheme, String> {
    let saved = update_app_ui_settings(&app, state.inner(), |settings| {
        settings.color_theme = value;
    })?;
    Ok(saved.color_theme)
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
pub(crate) fn get_servant_picker_settings(
    state: tauri::State<'_, Mutex<AppUiSettings>>,
) -> Result<ServantPickerSettings, String> {
    Ok(state.lock().map_err(|err| err.to_string())?.servant_picker)
}

#[tauri::command]
pub(crate) fn update_servant_picker_settings(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<AppUiSettings>>,
    action: ServantPickerAction,
) -> Result<ServantPickerSettings, String> {
    let saved = update_app_ui_settings(&app, state.inner(), |settings| {
        settings.servant_picker.apply(action);
    })?;
    Ok(saved.servant_picker)
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

#[tauri::command]
pub(crate) fn get_home_master_figure_id(state: tauri::State<'_, Mutex<AppUiSettings>>) -> u32 {
    state
        .lock()
        .unwrap()
        .home_master_figure_id
        .unwrap_or(DEFAULT_HOME_MASTER_FIGURE_ID)
}

#[tauri::command]
pub(crate) fn set_home_master_figure_id(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<AppUiSettings>>,
    id: u32,
) -> Result<u32, String> {
    let root =
        resolve_mystic_code_assets_dir(&app).ok_or_else(|| "未找到御主立绘资源目录".to_string())?;
    if !home_master_figure_exists(&root, id) {
        return Err(format!("未找到御主立绘 {id}"));
    }
    update_app_ui_settings(&app, state.inner(), |settings| {
        settings.home_master_figure_id = Some(id);
    })?;
    Ok(id)
}

#[cfg(test)]
mod servant_picker_tests {
    use super::*;

    #[test]
    fn legacy_preferences_default_to_list_and_medium_without_changing_other_settings() {
        let settings: AppUiSettings =
            serde_json::from_str(r#"{"theme":"dark","activeProjectId":"existing-project"}"#)
                .unwrap();
        assert_eq!(settings.servant_picker.layout, ServantPickerLayout::List);
        assert_eq!(
            settings.servant_picker.grid_size,
            ServantPickerGridSize::Medium
        );
        assert_eq!(settings.theme.as_deref(), Some("dark"));
        assert_eq!(
            settings.active_project_id.as_deref(),
            Some("existing-project")
        );
        let partial: AppUiSettings =
            serde_json::from_str(r#"{"servantPicker":{"layout":"grid"}}"#).unwrap();
        assert_eq!(partial.servant_picker.layout, ServantPickerLayout::Grid);
        assert_eq!(
            partial.servant_picker.grid_size,
            ServantPickerGridSize::Medium
        );
    }

    #[test]
    fn picker_actions_round_trip_on_disk_and_preserve_size_when_switching_layout() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("app_ui_settings.json");
        let mut settings = AppUiSettings {
            theme: Some("dark".into()),
            ..Default::default()
        };
        for (action, layout, size) in [
            (ServantPickerAction::ToggleLayout, "grid", "medium"),
            (ServantPickerAction::CycleGridSize, "grid", "large"),
            (ServantPickerAction::ToggleLayout, "list", "large"),
            (ServantPickerAction::CycleGridSize, "list", "small"),
            (ServantPickerAction::CycleGridSize, "list", "medium"),
        ] {
            settings.servant_picker.apply(action);
            write_app_ui_settings_to_path(&path, &settings).unwrap();
            settings = read_app_ui_settings_from_path(&path).unwrap();
            let json = serde_json::to_value(&settings).unwrap();
            assert_eq!(
                json["servantPicker"],
                serde_json::json!({ "layout": layout, "gridSize": size })
            );
            assert_eq!(settings.theme.as_deref(), Some("dark"));
        }
        assert!(serde_json::from_str::<ServantPickerSettings>(r#"{"layout":"other"}"#).is_err());
        assert!(serde_json::from_str::<ServantPickerSettings>(r#"{"gridSize":"other"}"#).is_err());
        assert!(serde_json::from_str::<ServantPickerAction>(r#""other""#).is_err());
    }
}

#[cfg(test)]
mod home_master_figure_tests {
    use super::*;

    #[test]
    fn figure_selection_requires_both_gender_images() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("470");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("master-figure-female.png"), []).unwrap();
        assert!(!home_master_figure_exists(temp.path(), 470));
        std::fs::write(dir.join("master-figure-male.png"), []).unwrap();
        assert!(home_master_figure_exists(temp.path(), 470));
    }
}
