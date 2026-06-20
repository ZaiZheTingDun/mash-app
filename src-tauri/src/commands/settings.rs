//! Persisted application settings and related IPC commands.
//!
//! Settings in this module are backend-owned state: commands update the shared
//! Tauri state and persist the matching JSON file under app_data_dir(). Server
//! switches also invalidate idle sidecars so future CV calls load the selected
//! server resources.

use crate::commands::debug;
use crate::enhancement_runner::{EnhancementRunnerHandle, EnhancementRunnerState};
use crate::paths::{migrate_legacy_app_data, StartupMigrationStatus};
use crate::runner::{RunnerHandle, RunnerState};
use crate::Server;
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Mutex;
use tauri::Manager;

fn adb_settings_path(app: &tauri::AppHandle) -> PathBuf {
    let dir = app
        .path()
        .app_data_dir()
        .expect("failed to resolve app data dir");
    fs::create_dir_all(&dir).ok();
    dir.join("adb_settings.json")
}

pub(crate) fn load_bluestack_setting(app: &tauri::AppHandle) -> bool {
    let path = adb_settings_path(app);
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("useBluestack")?.as_bool())
        .unwrap_or(false)
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

fn load_last_update_check_date(app: &tauri::AppHandle) -> Option<String> {
    let path = update_check_settings_path(app);
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<String>(&text).ok())
}

#[tauri::command]
pub(crate) fn get_use_bluestack(state: tauri::State<'_, Mutex<bool>>) -> bool {
    *state.lock().unwrap()
}

#[tauri::command]
pub(crate) fn set_use_bluestack(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<bool>>,
    value: bool,
) -> Result<(), String> {
    *state.lock().unwrap() = value;
    let path = adb_settings_path(&app);
    let json = serde_json::json!({ "useBluestack": value });
    fs::write(
        &path,
        serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) fn get_server(state: tauri::State<'_, Mutex<Server>>) -> Server {
    *state.lock().unwrap()
}

#[tauri::command]
pub(crate) async fn run_startup_migration(
    app: tauri::AppHandle,
    bluestack_state: tauri::State<'_, Mutex<bool>>,
    server_state: tauri::State<'_, Mutex<Server>>,
) -> Result<StartupMigrationStatus, String> {
    let migration_app = app.clone();
    let status =
        tauri::async_runtime::spawn_blocking(move || migrate_legacy_app_data(&migration_app))
            .await
            .map_err(|e| format!("startup migration task failed: {e}"))??;
    if status.migrated {
        *bluestack_state.lock().unwrap() = load_bluestack_setting(&app);
        *server_state.lock().unwrap() = load_server_setting(&app);
    }
    Ok(status)
}

#[tauri::command]
pub(crate) fn set_server(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<Server>>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
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
