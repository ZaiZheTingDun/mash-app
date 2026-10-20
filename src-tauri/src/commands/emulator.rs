//! User choice of a Windows emulator installation. Discovery and native
//! inspection stay in `platform`; this module owns persistence and IPC.

use crate::automation_coordinator::AutomationCoordinator;
use crate::commands::settings::AdbDeviceSettings;
use crate::paths::app_data_dir;
use crate::platform;
use crate::storage::{read_json_or_default, write_json_atomic};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum EmulatorMode {
    #[default]
    Auto,
    Mumu,
    Other,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EmulatorSettings {
    #[serde(default)]
    pub(crate) mode: EmulatorMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) selected_path: Option<PathBuf>,
    #[serde(default)]
    pub(crate) manual_paths: Vec<PathBuf>,
    #[serde(skip)]
    pub(crate) skip_other_once_for: Option<String>,
}

#[derive(Clone, Default)]
pub(crate) struct EmulatorSelectionState(pub(crate) Arc<Mutex<EmulatorSettings>>);

impl EmulatorSelectionState {
    pub(crate) fn new(settings: EmulatorSettings) -> Self {
        Self(Arc::new(Mutex::new(settings)))
    }
}

pub(crate) fn clear_other_override(app: &tauri::AppHandle) {
    app.state::<EmulatorSelectionState>()
        .0
        .lock()
        .unwrap()
        .skip_other_once_for = None;
}

pub(crate) fn selected_mumu_installation(app: &tauri::AppHandle) -> Option<PathBuf> {
    let settings = app.state::<EmulatorSelectionState>();
    let settings = settings.0.lock().unwrap();
    if settings.mode == EmulatorMode::Mumu {
        settings.selected_path.clone()
    } else {
        None
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EmulatorOptions {
    supported: bool,
    mode: EmulatorMode,
    selected_path: Option<String>,
    installations: Vec<String>,
    scan_error: Option<String>,
    other_override_ready: bool,
}

fn settings_path(app: &tauri::AppHandle) -> PathBuf {
    app_data_dir(app).join("emulator_settings.json")
}

pub(crate) fn load_emulator_settings(app: &tauri::AppHandle) -> Result<EmulatorSettings, String> {
    read_json_or_default(&settings_path(app), "模拟器设置")
}

fn save_emulator_settings(app: &tauri::AppHandle, value: &EmulatorSettings) -> Result<(), String> {
    write_json_atomic(&settings_path(app), value, "模拟器设置")
}

fn same_path(left: &Path, right: &Path) -> bool {
    left.to_string_lossy()
        .trim_end_matches(['/', '\\'])
        .eq_ignore_ascii_case(right.to_string_lossy().trim_end_matches(['/', '\\']))
}

fn add_unique(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|existing| same_path(existing, &path)) {
        paths.push(path);
    }
}

fn options(
    settings: &EmulatorSettings,
    discovered: Vec<PathBuf>,
    scan_error: Option<String>,
) -> EmulatorOptions {
    let mut paths = Vec::new();
    if let Some(path) = &settings.selected_path {
        add_unique(&mut paths, path.clone());
    }
    for path in discovered {
        add_unique(&mut paths, path);
    }
    for path in &settings.manual_paths {
        add_unique(&mut paths, path.clone());
    }
    paths.sort_by_key(|path| path.to_string_lossy().to_ascii_lowercase());
    EmulatorOptions {
        supported: platform::supports_emulator_selection(),
        mode: settings.mode,
        selected_path: settings
            .selected_path
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned()),
        installations: paths
            .into_iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect(),
        scan_error,
        other_override_ready: settings.skip_other_once_for.is_some(),
    }
}

#[tauri::command]
pub(crate) fn supports_emulator_selection() -> bool {
    platform::supports_emulator_selection()
}

#[tauri::command]
pub(crate) async fn get_emulator_options(
    state: tauri::State<'_, EmulatorSelectionState>,
) -> Result<EmulatorOptions, String> {
    let snapshot = state.0.lock().unwrap().clone();
    let found = tauri::async_runtime::spawn_blocking(platform::discover_mumu_installations)
        .await
        .map_err(|error| format!("扫描 MuMu 安装目录失败: {error}"))?;
    let (paths, error) = match found {
        Ok(paths) => (paths, None),
        Err(error) => (Vec::new(), Some(error)),
    };
    Ok(options(&snapshot, paths, error))
}

#[tauri::command]
pub(crate) async fn add_mumu_installation(
    app: tauri::AppHandle,
    state: tauri::State<'_, EmulatorSelectionState>,
    coordinator: tauri::State<'_, AutomationCoordinator>,
) -> Result<Option<EmulatorOptions>, String> {
    coordinator.require_idle()?;
    if !platform::supports_emulator_selection() {
        return Err("仅 Windows 支持选择 MuMu 安装目录".into());
    }
    let picked = app
        .dialog()
        .file()
        .set_title("选择 MuMu 安装根目录")
        .blocking_pick_folder();
    let Some(picked) = picked else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|error| error.to_string())?;
    platform::validate_mumu_installation(&path)?;
    coordinator.require_idle()?;
    let mut guard = state.0.lock().unwrap();
    let mut next = guard.clone();
    add_unique(&mut next.manual_paths, path.clone());
    next.mode = EmulatorMode::Mumu;
    next.selected_path = Some(path);
    next.skip_other_once_for = None;
    save_emulator_settings(&app, &next)?;
    *guard = next.clone();
    drop(guard);
    let (paths, error) = match platform::discover_mumu_installations() {
        Ok(paths) => (paths, None),
        Err(error) => (Vec::new(), Some(error)),
    };
    Ok(Some(options(&next, paths, error)))
}

#[tauri::command]
pub(crate) fn select_emulator(
    app: tauri::AppHandle,
    state: tauri::State<'_, EmulatorSelectionState>,
    coordinator: tauri::State<'_, AutomationCoordinator>,
    mode: EmulatorMode,
    path: Option<String>,
) -> Result<(), String> {
    coordinator.require_idle()?;
    if !platform::supports_emulator_selection() {
        return Err("仅 Windows 支持选择模拟器".into());
    }
    let path = match mode {
        EmulatorMode::Mumu => {
            let path = PathBuf::from(path.ok_or("请选择 MuMu 安装目录")?);
            platform::validate_mumu_installation(&path)?;
            Some(path)
        }
        _ => None,
    };
    let mut guard = state.0.lock().unwrap();
    let mut next = guard.clone();
    next.mode = mode;
    next.selected_path = path;
    next.skip_other_once_for = None;
    save_emulator_settings(&app, &next)?;
    *guard = next;
    Ok(())
}

#[tauri::command]
pub(crate) fn allow_other_emulator_once(
    state: tauri::State<'_, EmulatorSelectionState>,
    adb_settings: tauri::State<'_, Mutex<AdbDeviceSettings>>,
    coordinator: tauri::State<'_, AutomationCoordinator>,
) -> Result<(), String> {
    coordinator.require_idle()?;
    if !platform::supports_emulator_selection() {
        return Err("仅 Windows 支持选择模拟器".into());
    }
    let serial = adb_settings
        .lock()
        .unwrap()
        .selected_adb_serial
        .clone()
        .ok_or("请先选择 ADB 设备")?;
    let mut settings = state.0.lock().unwrap();
    if settings.mode != EmulatorMode::Other {
        return Err("请先选择“其他模拟器”".into());
    }
    settings.skip_other_once_for = Some(serial);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_options_deduplicate_case_and_keep_manual_selection() {
        let settings = EmulatorSettings {
            mode: EmulatorMode::Mumu,
            selected_path: Some(PathBuf::from("C:\\MuMu")),
            manual_paths: vec![PathBuf::from("c:\\mumu\\"), PathBuf::from("D:\\MuMu")],
            ..Default::default()
        };
        let result = options(&settings, vec![PathBuf::from("C:\\MuMu")], None);
        assert_eq!(result.installations.len(), 2);
        assert_eq!(result.selected_path.as_deref(), Some("C:\\MuMu"));
    }

    #[test]
    fn old_or_empty_settings_default_to_auto_and_never_persist_one_time_skip() {
        let settings: EmulatorSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings.mode, EmulatorMode::Auto);
        let mut settings = settings;
        settings.skip_other_once_for = Some("emulator-5554".into());
        assert!(options(&settings, Vec::new(), None).other_override_ready);
        let saved = serde_json::to_value(&settings).unwrap();
        assert!(saved.get("skipOtherOnceFor").is_none());
    }
}
