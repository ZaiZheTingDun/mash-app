mod adb;
mod asset_resources;
mod catalog;
mod debug;
mod enhancement_runner;
mod models;
mod paths;
mod projects;
mod runner;
mod runtime_resources;
mod screen;
mod server;
mod touch;

use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
#[cfg(desktop)]
use tauri::menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
use zip::ZipArchive;

use enhancement_runner::{
    server_supported as enhancement_server_supported, EnhancementAutomationEvent,
    EnhancementConfig, EnhancementRunner, EnhancementRunnerHandle, EnhancementRunnerState,
    EnhancementTarget,
};
use runner::{AutomationEvent, LogLevel, RunConfig, RunnerHandle, RunnerState};

pub use models::*;
pub use server::{
    stream_meets_minimum_resolution, stream_resolution_error, Server, STREAM_BIT_RATE,
    STREAM_MAX_SIZE,
};

pub(crate) use asset_resources::*;
pub(crate) use catalog::*;
pub(crate) use paths::*;
pub(crate) use projects::*;
pub(crate) use runtime_resources::*;

#[cfg(desktop)]
const CHECK_FOR_UPDATE_MENU_ID: &str = "check-for-update";
#[cfg(desktop)]
const CHECK_FOR_UPDATE_EVENT: &str = "updater-check-requested";
#[cfg(desktop)]
const SELF_CHECK_MENU_ID: &str = "self-check";
#[cfg(desktop)]
const SELF_CHECK_EVENT: &str = "self-check-requested";
#[cfg(desktop)]
const RESOURCE_MANAGER_MENU_ID: &str = "resource-manager";
#[cfg(desktop)]
const RESOURCE_MANAGER_EVENT: &str = "resource-manager-requested";
#[cfg(desktop)]
const SAVE_ADB_SCREENSHOT_MENU_ID: &str = "save-adb-screenshot";
#[cfg(desktop)]
const SAVE_ADB_SCREENSHOT_EVENT: &str = "save-adb-screenshot-requested";

fn adb_settings_path(app: &tauri::AppHandle) -> PathBuf {
    let dir = app
        .path()
        .app_data_dir()
        .expect("failed to resolve app data dir");
    fs::create_dir_all(&dir).ok();
    dir.join("adb_settings.json")
}

fn load_bluestack_setting(app: &tauri::AppHandle) -> bool {
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

fn load_server_setting(app: &tauri::AppHandle) -> Server {
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

fn parse_first_ready_device(output: &str) -> Option<String> {
    output.lines().skip(1).find_map(|line| {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }
        let mut parts = trimmed.split('\t');
        let serial = parts.next()?.trim();
        let status = parts.next()?.trim();
        if status == "device" {
            Some(serial.to_string())
        } else {
            None
        }
    })
}

#[tauri::command]
fn get_use_bluestack(state: tauri::State<'_, Mutex<bool>>) -> bool {
    *state.lock().unwrap()
}

#[tauri::command]
fn set_use_bluestack(
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
fn get_server(state: tauri::State<'_, Mutex<Server>>) -> Server {
    *state.lock().unwrap()
}

#[tauri::command]
async fn run_startup_migration(
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
fn set_server(
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
fn should_check_updates_today(app: tauri::AppHandle, today: String) -> Result<bool, String> {
    Ok(load_last_update_check_date(&app).as_deref() != Some(today.as_str()))
}

#[tauri::command]
fn mark_update_checked_today(app: tauri::AppHandle, date: String) -> Result<(), String> {
    let path = update_check_settings_path(&app);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let body = serde_json::to_string(&date).map_err(|e| e.to_string())?;
    fs::write(path, body).map_err(|e| e.to_string())
}

#[cfg(desktop)]
fn configure_app_menu<R: tauri::Runtime>(app: &tauri::App<R>) -> tauri::Result<()> {
    let handle = app.handle();
    let pkg_info = handle.package_info();
    let config = handle.config();
    let about_metadata = AboutMetadata {
        name: Some(pkg_info.name.clone()),
        version: Some(pkg_info.version.to_string()),
        copyright: config.bundle.copyright.clone(),
        authors: config.bundle.publisher.clone().map(|p| vec![p]),
        ..Default::default()
    };

    let window_menu = Submenu::with_id_and_items(
        handle,
        "window",
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(handle, None)?,
            &PredefinedMenuItem::maximize(handle, None)?,
            #[cfg(target_os = "macos")]
            &PredefinedMenuItem::separator(handle)?,
            &PredefinedMenuItem::close_window(handle, None)?,
        ],
    )?;

    let help_menu = Submenu::with_id_and_items(
        handle,
        "help",
        "Help",
        true,
        &[
            #[cfg(not(target_os = "macos"))]
            &PredefinedMenuItem::about(handle, None, Some(about_metadata.clone()))?,
            #[cfg(not(target_os = "macos"))]
            &PredefinedMenuItem::separator(handle)?,
            #[cfg(not(target_os = "macos"))]
            &MenuItem::with_id(
                handle,
                CHECK_FOR_UPDATE_MENU_ID,
                "Check for Update...",
                true,
                None::<&str>,
            )?,
            &MenuItem::with_id(handle, SELF_CHECK_MENU_ID, "自检...", true, None::<&str>)?,
            &MenuItem::with_id(
                handle,
                RESOURCE_MANAGER_MENU_ID,
                "资源管理...",
                true,
                None::<&str>,
            )?,
        ],
    )?;

    let tools_menu = Submenu::with_id_and_items(
        handle,
        "tools",
        "工具",
        true,
        &[&MenuItem::with_id(
            handle,
            SAVE_ADB_SCREENSHOT_MENU_ID,
            "截图...",
            true,
            None::<&str>,
        )?],
    )?;

    let menu = Menu::with_items(
        handle,
        &[
            #[cfg(target_os = "macos")]
            &Submenu::with_items(
                handle,
                pkg_info.name.clone(),
                true,
                &[
                    &PredefinedMenuItem::about(handle, None, Some(about_metadata))?,
                    &MenuItem::with_id(
                        handle,
                        CHECK_FOR_UPDATE_MENU_ID,
                        "Check for Update...",
                        true,
                        None::<&str>,
                    )?,
                    &MenuItem::with_id(handle, SELF_CHECK_MENU_ID, "自检...", true, None::<&str>)?,
                    &MenuItem::with_id(
                        handle,
                        RESOURCE_MANAGER_MENU_ID,
                        "资源管理...",
                        true,
                        None::<&str>,
                    )?,
                    &PredefinedMenuItem::separator(handle)?,
                    &PredefinedMenuItem::services(handle, None)?,
                    &PredefinedMenuItem::separator(handle)?,
                    &PredefinedMenuItem::hide(handle, None)?,
                    &PredefinedMenuItem::hide_others(handle, None)?,
                    &PredefinedMenuItem::separator(handle)?,
                    &PredefinedMenuItem::quit(handle, None)?,
                ],
            )?,
            #[cfg(not(any(
                target_os = "linux",
                target_os = "dragonfly",
                target_os = "freebsd",
                target_os = "netbsd",
                target_os = "openbsd"
            )))]
            &Submenu::with_items(
                handle,
                "File",
                true,
                &[
                    &PredefinedMenuItem::close_window(handle, None)?,
                    #[cfg(not(target_os = "macos"))]
                    &PredefinedMenuItem::quit(handle, None)?,
                ],
            )?,
            &Submenu::with_items(
                handle,
                "Edit",
                true,
                &[
                    &PredefinedMenuItem::undo(handle, None)?,
                    &PredefinedMenuItem::redo(handle, None)?,
                    &PredefinedMenuItem::separator(handle)?,
                    &PredefinedMenuItem::cut(handle, None)?,
                    &PredefinedMenuItem::copy(handle, None)?,
                    &PredefinedMenuItem::paste(handle, None)?,
                    &PredefinedMenuItem::select_all(handle, None)?,
                ],
            )?,
            #[cfg(target_os = "macos")]
            &Submenu::with_items(
                handle,
                "View",
                true,
                &[&PredefinedMenuItem::fullscreen(handle, None)?],
            )?,
            &tools_menu,
            &window_menu,
            &help_menu,
        ],
    )?;

    app.set_menu(menu)?;
    app.on_menu_event(|app, event| {
        if event.id() == CHECK_FOR_UPDATE_MENU_ID {
            let _ = app.emit(CHECK_FOR_UPDATE_EVENT, ());
        } else if event.id() == SELF_CHECK_MENU_ID {
            let _ = app.emit(SELF_CHECK_EVENT, ());
        } else if event.id() == RESOURCE_MANAGER_MENU_ID {
            let _ = app.emit(RESOURCE_MANAGER_EVENT, ());
        } else if event.id() == SAVE_ADB_SCREENSHOT_MENU_ID {
            let _ = app.emit(SAVE_ADB_SCREENSHOT_EVENT, ());
        }
    });
    Ok(())
}

fn with_png_extension(path: PathBuf) -> PathBuf {
    if path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
    {
        path
    } else {
        path.with_extension("png")
    }
}

#[tauri::command]
async fn save_adb_screenshot(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<bool>>,
) -> Result<Option<String>, String> {
    let use_bluestack = *state.lock().unwrap();
    let mut adb_dev = adb::Adb::new(&app, use_bluestack);
    adb_dev.connect()?;
    let screenshot_path = adb_dev.screenshot_to_file()?;

    let target = app
        .dialog()
        .file()
        .add_filter("PNG", &["png"])
        .set_title("保存原始截图")
        .set_file_name("mash-screenshot.png")
        .blocking_save_file();

    let Some(target) = target else {
        let _ = fs::remove_file(&screenshot_path);
        return Ok(None);
    };
    let target = with_png_extension(target.into_path().map_err(|e| e.to_string())?);
    if let Err(err) = fs::copy(&screenshot_path, &target) {
        let _ = fs::remove_file(&screenshot_path);
        return Err(format!("failed to save screenshot: {err}"));
    }
    let _ = fs::remove_file(&screenshot_path);

    Ok(Some(target.to_string_lossy().into_owned()))
}

#[tauri::command]
fn check_adb(app: tauri::AppHandle, state: tauri::State<'_, Mutex<bool>>) -> AdbStatus {
    let use_bluestack = *state.lock().unwrap();
    let adb_path = adb::resolve_adb_path(&app);
    if use_bluestack {
        std::process::Command::new(&adb_path)
            .args(["connect", "127.0.0.1:5555"])
            .output()
            .ok();
    }

    let device_name = std::process::Command::new(&adb_path)
        .arg("devices")
        .output()
        .ok()
        .and_then(|out| parse_first_ready_device(&String::from_utf8_lossy(&out.stdout)));

    AdbStatus {
        connected: device_name.is_some(),
        device_name,
    }
}

#[tauri::command]
fn reset_bluestacks_adb_connection(app: tauri::AppHandle) -> AdbResetResult {
    let adb_path = adb::resolve_adb_path(&app);
    let commands: Vec<Vec<String>> = vec![
        vec!["disconnect".into(), "127.0.0.1:5555".into()],
        vec!["kill-server".into()],
        vec!["start-server".into()],
        vec!["connect".into(), "127.0.0.1:5555".into()],
        vec!["devices".into(), "-l".into()],
        vec![
            "-s".into(),
            "127.0.0.1:5555".into(),
            "shell".into(),
            "echo".into(),
            "ok".into(),
        ],
    ];

    std::thread::spawn(move || {
        let mut steps = Vec::with_capacity(commands.len());
        let _ = app.emit(
            "adb-reset-status",
            AdbResetStatusEvent {
                message: "开始重置 BlueStacks ADB 链接…".into(),
                step: None,
                done: false,
                ok: None,
            },
        );
        eprintln!("[adb-reset] begin (adb={})", adb_path.display());
        for args in commands {
            let step = run_adb_reset_step(&adb_path, &args);
            let message = format_adb_reset_step_message(&step);
            let _ = app.emit(
                "adb-reset-status",
                AdbResetStatusEvent {
                    message,
                    step: Some(step.clone()),
                    done: false,
                    ok: None,
                },
            );
            steps.push(step);
        }
        let ok = steps.iter().all(|step| step.success);
        eprintln!("[adb-reset] done ok={ok}");
        let _ = app.emit(
            "adb-reset-status",
            AdbResetStatusEvent {
                message: if ok {
                    "ADB 链接重置完成".into()
                } else {
                    "ADB 链接重置完成，但存在失败命令".into()
                },
                step: None,
                done: true,
                ok: Some(ok),
            },
        );
    });

    AdbResetResult {
        ok: true,
        steps: Vec::new(),
    }
}

fn run_adb_reset_step(adb_path: &Path, args: &[String]) -> AdbResetStep {
    let command = format!("{} {}", adb_path.display(), args.join(" "));
    eprintln!("[adb-reset] running: {command}");
    match std::process::Command::new(adb_path).args(args).output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let success = output.status.success();
            let status = output.status.code();
            eprintln!(
                "[adb-reset] finished: success={success} status={status:?} command={command}"
            );
            if !stdout.is_empty() {
                eprintln!("[adb-reset] stdout: {stdout}");
            }
            if !stderr.is_empty() {
                eprintln!("[adb-reset] stderr: {stderr}");
            }
            AdbResetStep {
                command,
                success,
                status,
                stdout,
                stderr,
            }
        }
        Err(err) => {
            let stderr = err.to_string();
            eprintln!("[adb-reset] spawn failed: command={command} error={stderr}");
            AdbResetStep {
                command,
                success: false,
                status: None,
                stdout: String::new(),
                stderr,
            }
        }
    }
}

fn format_adb_reset_step_message(step: &AdbResetStep) -> String {
    let status_text = step
        .status
        .map(|status| format!("exit {status}"))
        .unwrap_or_else(|| "spawn failed".into());
    let output = [&step.stdout, &step.stderr]
        .into_iter()
        .filter(|part| !part.is_empty())
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(" | ");
    format!(
        "{}: {} ({}){}",
        if step.success { "成功" } else { "失败" },
        step.command,
        status_text,
        if output.is_empty() {
            String::new()
        } else {
            format!(" - {output}")
        }
    )
}

pub(crate) fn spawn_configured_sidecar(
    app: &tauri::AppHandle,
    server: Server,
) -> Result<screen::SidecarClient, String> {
    let template_dirs = resolve_template_dirs(app, server);
    let cv_configs = resolve_cv_config_paths(app, server);
    screen::SidecarClient::spawn(app, &template_dirs, &cv_configs, server)
}

fn take_or_spawn_sidecar(
    app: &tauri::AppHandle,
    shared_sidecar: &debug::DebugSidecar,
    server: Server,
) -> Result<screen::SidecarClient, String> {
    if let Some(client) = shared_sidecar.0.lock().unwrap().take() {
        eprintln!("[mash-cv] reusing cached sidecar");
        return Ok(client);
    }
    spawn_configured_sidecar(app, server)
}

fn input_size_for_taps(adb_size: Option<(u32, u32)>, stream_size: (u32, u32)) -> (u32, u32) {
    let Some((adb_w, adb_h)) = adb_size else {
        return stream_size;
    };
    let (stream_w, stream_h) = stream_size;
    let adb_landscape = adb_w >= adb_h;
    let stream_landscape = stream_w >= stream_h;
    if adb_landscape == stream_landscape {
        (adb_w, adb_h)
    } else {
        (adb_h, adb_w)
    }
}

fn runner_is_busy(state: &RunnerState) -> bool {
    matches!(state, RunnerState::Starting | RunnerState::Running)
}

fn enhancement_runner_is_busy(state: &EnhancementRunnerState) -> bool {
    matches!(
        state,
        EnhancementRunnerState::Starting | EnhancementRunnerState::Running
    )
}

fn emit_automation_status(
    app: &tauri::AppHandle,
    state: &Arc<Mutex<RunnerState>>,
    screen: &str,
    message: &str,
) {
    let state_str = {
        let s = state.lock().unwrap();
        format!("{:?}", *s)
    };
    let _ = app.emit(
        "automation-status",
        AutomationEvent {
            state: state_str,
            current_screen: screen.into(),
            message: message.into(),
            level: LogLevel::Info,
            attack: None,
        },
    );
}

fn fail_automation_start(app: &tauri::AppHandle, state: &Arc<Mutex<RunnerState>>, message: String) {
    *state.lock().unwrap() = RunnerState::Error {
        message: message.clone(),
    };
    emit_automation_status(app, state, "", &format!("启动失败: {message}"));
}

fn stop_automation_start(app: &tauri::AppHandle, state: &Arc<Mutex<RunnerState>>) {
    *state.lock().unwrap() = RunnerState::Idle;
    emit_automation_status(app, state, "", "自动化已停止");
}

fn emit_enhancement_status(
    app: &tauri::AppHandle,
    state: &Arc<Mutex<EnhancementRunnerState>>,
    screen: &str,
    message: &str,
) {
    let state_str = {
        let s = state.lock().unwrap();
        format!("{:?}", *s)
    };
    let _ = app.emit(
        "enhancement-automation-status",
        EnhancementAutomationEvent {
            state: state_str,
            current_screen: screen.into(),
            message: message.into(),
            level: LogLevel::Info,
        },
    );
}

fn fail_enhancement_start(
    app: &tauri::AppHandle,
    state: &Arc<Mutex<EnhancementRunnerState>>,
    message: String,
) {
    *state.lock().unwrap() = EnhancementRunnerState::Error {
        message: message.clone(),
    };
    emit_enhancement_status(app, state, "", &format!("启动失败: {message}"));
}

fn stop_enhancement_start(app: &tauri::AppHandle, state: &Arc<Mutex<EnhancementRunnerState>>) {
    *state.lock().unwrap() = EnhancementRunnerState::Idle;
    emit_enhancement_status(app, state, "", "强化自动化已停止");
}

#[tauri::command]
fn start_automation(
    app: tauri::AppHandle,
    config: RunConfig,
    bluestack_state: tauri::State<'_, Mutex<bool>>,
    server_state: tauri::State<'_, Mutex<Server>>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    debug_state: tauri::State<'_, debug::DebugSidecar>,
) -> Result<(), String> {
    let is_running = {
        let state = handle_state.lock().unwrap().state.clone();
        let running = runner_is_busy(&state.lock().unwrap());
        running
    };
    if is_running {
        return Err("自动化正在运行中".into());
    }
    {
        let handle = enhancement_handle_state.lock().unwrap();
        let running = enhancement_runner_is_busy(&handle.state.lock().unwrap());
        if running {
            return Err("强化自动化正在运行中".into());
        }
    }

    let project = read_projects(&app)
        .into_iter()
        .find(|project| project.id == config.project_id);
    let advanced_mode = project
        .as_ref()
        .map(|project| project.advanced_mode)
        .unwrap_or(false);
    let scenes = if advanced_mode {
        Vec::new()
    } else {
        load_battle_scenes(app.clone(), config.project_id.clone())
    };
    let advanced_scenes = if advanced_mode {
        load_advanced_battle_scenes(app.clone(), config.project_id.clone())
    } else {
        Vec::new()
    };

    let use_bluestack = *bluestack_state.lock().unwrap();
    let server = *server_state.lock().unwrap();

    let state = Arc::new(Mutex::new(RunnerState::Starting));
    let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop_after_current = Arc::new(std::sync::atomic::AtomicBool::new(false));

    {
        let mut handle = handle_state.lock().unwrap();
        handle.state = state.clone();
        handle.cancel = cancel.clone();
        handle.stop_after_current = stop_after_current.clone();
    }

    let debug_sidecar = debug_state.0.clone();
    std::thread::spawn(move || {
        emit_automation_status(&app, &state, "", "正在连接 ADB…");
        let mut adb_dev = adb::Adb::new(&app, use_bluestack);
        if let Err(err) = adb_dev.connect() {
            fail_automation_start(&app, &state, err);
            return;
        }
        if cancel.load(Ordering::Relaxed) {
            stop_automation_start(&app, &state);
            return;
        }
        let serial = adb_dev.serial().map(|s| s.to_string());

        let Some(jar_path) = resolve_scrcpy_jar(&app) else {
            fail_automation_start(&app, &state, "找不到 scrcpy-server.jar 资源".into());
            return;
        };
        if !jar_path.exists() {
            fail_automation_start(
                &app,
                &state,
                format!("scrcpy-server.jar 不存在: {}", jar_path.display()),
            );
            return;
        }

        emit_automation_status(&app, &state, "", "正在启动视频流…");
        let debug_state = debug::DebugSidecar(debug_sidecar.clone());
        let mut sidecar = match take_or_spawn_sidecar(&app, &debug_state, server) {
            Ok(sidecar) => sidecar,
            Err(err) => {
                fail_automation_start(&app, &state, err);
                return;
            }
        };

        let (w, h) = match sidecar.start_stream(
            adb_dev.path(),
            &jar_path,
            serial.as_deref(),
            STREAM_MAX_SIZE,
            STREAM_BIT_RATE,
        ) {
            Ok(size) => size,
            Err(err) => {
                fail_automation_start(&app, &state, format!("启动 scrcpy 视频流失败: {err}"));
                return;
            }
        };
        if !stream_meets_minimum_resolution(w, h) {
            if let Err(err) = sidecar.stop_stream() {
                eprintln!("[runner] stop unsupported-resolution stream failed: {err}");
            }
            fail_automation_start(&app, &state, stream_resolution_error(w, h));
            return;
        }
        let input_size = input_size_for_taps(adb_dev.screen_size(), (w, h));
        if input_size != (w, h) {
            eprintln!(
                "[runner] using adb input size {}x{} with stream frame {}x{}",
                input_size.0, input_size.1, w, h
            );
        }
        let screen_size = Some(input_size);
        let assets_dir = resolve_servant_assets_dir(&app);
        let ce_assets_dir = resolve_ce_assets_dir(&app);
        let runner = runner::Runner::new(
            adb_dev,
            sidecar,
            config,
            scenes,
            advanced_mode,
            advanced_scenes,
            app,
            state,
            cancel,
            stop_after_current,
            screen_size,
            assets_dir,
            ce_assets_dir,
            server,
            Some(debug_sidecar),
        );
        runner.run();
    });

    Ok(())
}

#[tauri::command]
fn stop_automation(handle_state: tauri::State<'_, Mutex<RunnerHandle>>) -> Result<(), String> {
    let handle = handle_state.lock().unwrap();
    handle.cancel.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
fn stop_automation_after_current(
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
) -> Result<(), String> {
    let handle = handle_state.lock().unwrap();
    handle.stop_after_current.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
fn get_automation_status(handle_state: tauri::State<'_, Mutex<RunnerHandle>>) -> RunnerState {
    let handle = handle_state.lock().unwrap();
    let state = handle.state.lock().unwrap().clone();
    state
}

#[tauri::command]
fn start_enhancement_automation(
    app: tauri::AppHandle,
    config: EnhancementConfig,
    bluestack_state: tauri::State<'_, Mutex<bool>>,
    server_state: tauri::State<'_, Mutex<Server>>,
    battle_handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    debug_state: tauri::State<'_, debug::DebugSidecar>,
) -> Result<(), String> {
    {
        let handle = handle_state.lock().unwrap();
        let running = enhancement_runner_is_busy(&handle.state.lock().unwrap());
        if running {
            return Err("强化自动化正在运行中".into());
        }
    }
    {
        let handle = battle_handle_state.lock().unwrap();
        let running = runner_is_busy(&handle.state.lock().unwrap());
        if running {
            return Err("战斗自动化正在运行中，请先停止".into());
        }
    }

    let use_bluestack = *bluestack_state.lock().unwrap();
    let server = *server_state.lock().unwrap();
    if !enhancement_server_supported(server) {
        return Err("当前仅支持日服强化自动化".into());
    }

    let target = load_enhancement_target(&app, &config.target_servant_variant_key)?;
    if target.id != config.target_servant_id {
        return Err("目标从者 id 与 variantKey 不匹配".into());
    }

    let state = Arc::new(Mutex::new(EnhancementRunnerState::Starting));
    let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let mut handle = handle_state.lock().unwrap();
        handle.state = state.clone();
        handle.cancel = cancel.clone();
    }

    let debug_sidecar = debug_state.0.clone();
    std::thread::spawn(move || {
        emit_enhancement_status(&app, &state, "", "正在连接 ADB…");
        let mut adb_dev = adb::Adb::new(&app, use_bluestack);
        if let Err(err) = adb_dev.connect() {
            fail_enhancement_start(&app, &state, err);
            return;
        }
        if cancel.load(Ordering::Relaxed) {
            stop_enhancement_start(&app, &state);
            return;
        }
        let serial = adb_dev.serial().map(|s| s.to_string());

        let Some(jar_path) = resolve_scrcpy_jar(&app) else {
            fail_enhancement_start(&app, &state, "找不到 scrcpy-server.jar 资源".into());
            return;
        };
        if !jar_path.exists() {
            fail_enhancement_start(
                &app,
                &state,
                format!("scrcpy-server.jar 不存在: {}", jar_path.display()),
            );
            return;
        }

        emit_enhancement_status(&app, &state, "", "正在启动视频流…");
        let debug_state = debug::DebugSidecar(debug_sidecar.clone());
        let mut sidecar = match take_or_spawn_sidecar(&app, &debug_state, server) {
            Ok(sidecar) => sidecar,
            Err(err) => {
                fail_enhancement_start(&app, &state, err);
                return;
            }
        };
        let (w, h) = match sidecar.start_stream(
            adb_dev.path(),
            &jar_path,
            serial.as_deref(),
            STREAM_MAX_SIZE,
            STREAM_BIT_RATE,
        ) {
            Ok(size) => size,
            Err(err) => {
                fail_enhancement_start(&app, &state, format!("启动 scrcpy 视频流失败: {err}"));
                return;
            }
        };
        if !stream_meets_minimum_resolution(w, h) {
            if let Err(err) = sidecar.stop_stream() {
                eprintln!("[enhancement] stop unsupported-resolution stream failed: {err}");
            }
            fail_enhancement_start(&app, &state, stream_resolution_error(w, h));
            return;
        }
        let input_size = input_size_for_taps(adb_dev.screen_size(), (w, h));
        if input_size != (w, h) {
            eprintln!(
                "[enhancement] using adb input size {}x{} with stream frame {}x{}",
                input_size.0, input_size.1, w, h
            );
        }

        let runner = EnhancementRunner::new(
            adb_dev,
            sidecar,
            app,
            state,
            cancel,
            input_size,
            target,
            Some(debug_sidecar),
        );
        runner.run();
    });
    Ok(())
}

#[tauri::command]
fn stop_enhancement_automation(
    handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
) -> Result<(), String> {
    let handle = handle_state.lock().unwrap();
    handle.cancel.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
fn get_enhancement_automation_status(
    handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
) -> EnhancementRunnerState {
    let handle = handle_state.lock().unwrap();
    let state = handle.state.lock().unwrap().clone();
    state
}

// ---------------------------------------------------------------------------
// mash-cv runtime management
// ---------------------------------------------------------------------------

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let use_bluestack = load_bluestack_setting(&app.handle());
            let server = load_server_setting(&app.handle());
            #[cfg(desktop)]
            configure_app_menu(app)?;
            refresh_asset_protocol_scope(&app.handle())?;
            app.manage(Mutex::new(use_bluestack));
            app.manage(Mutex::new(server));
            app.manage(Mutex::new(RunnerHandle::new_idle()));
            app.manage(Mutex::new(EnhancementRunnerHandle::new_idle()));
            app.manage(Arc::new(ResourceDownloadCancelState::default()));
            app.manage(debug::DebugSidecar::new());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            catalog::get_servants,
            catalog::get_craft_essences,
            asset_resources::get_self_check_status,
            asset_resources::get_asset_bundle_status,
            asset_resources::pick_asset_bundle,
            asset_resources::import_asset_bundle,
            asset_resources::download_asset_bundles,
            asset_resources::cancel_resource_downloads,
            runtime_resources::get_runtime_status,
            runtime_resources::pick_runtime_bundle,
            runtime_resources::import_runtime_bundle,
            runtime_resources::download_runtime_bundles,
            catalog::get_servant_portrait_path,
            catalog::get_servant_face_path,
            catalog::get_craft_essence_card_path,
            catalog::get_template_asset_path,
            projects::save_battle_scenes,
            projects::load_battle_scenes,
            projects::save_advanced_battle_scenes,
            projects::load_advanced_battle_scenes,
            projects::list_exportable_configs,
            projects::export_configs,
            projects::pick_config_import_file,
            projects::preview_config_import,
            projects::import_configurations,
            projects::list_projects,
            projects::get_active_project_id,
            projects::set_active_project_id,
            projects::get_app_theme,
            projects::set_app_theme,
            projects::create_project,
            projects::duplicate_project,
            projects::update_project,
            projects::delete_project,
            check_adb,
            reset_bluestacks_adb_connection,
            save_adb_screenshot,
            run_startup_migration,
            get_use_bluestack,
            set_use_bluestack,
            get_server,
            set_server,
            should_check_updates_today,
            mark_update_checked_today,
            start_automation,
            stop_automation,
            stop_automation_after_current,
            get_automation_status,
            start_enhancement_automation,
            stop_enhancement_automation,
            get_enhancement_automation_status,
            debug::debug_capture,
            debug::debug_find_element,
            debug::debug_find_element_by_name,
            debug::debug_list_templates,
            debug::debug_get_cv_config,
            debug::debug_reload_sidecar,
            debug::debug_shutdown,
            debug::debug_get_runner_coordinates,
            debug::debug_find_command_cards,
            debug::debug_find_noble_phantasms,
            debug::debug_find_enhancement_servant,
            debug::debug_find_attack_button,
            debug::debug_read_battle_scene,
            debug::debug_find_supports,
            debug::debug_list_servant_assets,
            debug::warm_sidecar,
            catalog::get_servant_metadata,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::io::Cursor;
    use zip::write::SimpleFileOptions;

    #[test]
    fn tauri_bundle_resources_cover_template_subdirectories() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let config_path = manifest_dir.join("tauri.conf.json");
        let config: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
        let resources: HashSet<String> = config["bundle"]["resources"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item.as_str().unwrap().to_string())
            .collect();

        assert!(
            resources.contains("resources/servers/shared/cv.json"),
            "missing Tauri bundle resource for shared cv.json"
        );
        assert!(
            resources.contains("resources/servers/shared/templates/*"),
            "missing Tauri bundle resource glob for shared templates"
        );

        for server in ["jp", "cn"] {
            let templates_dir = manifest_dir
                .join("resources")
                .join("servers")
                .join(server)
                .join("templates");
            for entry in fs::read_dir(&templates_dir).unwrap() {
                let entry = entry.unwrap();
                if !entry.file_type().unwrap().is_dir() {
                    continue;
                }
                let dir_name = entry.file_name().to_string_lossy().into_owned();
                let glob = format!("resources/servers/{server}/templates/{dir_name}/*");
                assert!(
                    resources.contains(&glob),
                    "missing Tauri bundle resource glob for {glob}"
                );
            }
        }
    }

    #[test]
    fn shared_cv_defines_battle_close_button_elements() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let config_path = manifest_dir
            .join("resources")
            .join("servers")
            .join("shared")
            .join("cv.json");
        let config: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
        let elements = &config["screens"]["Battle"]["variants"]["main"]["elements"];

        assert_battle_close_button_element(
            &elements["skill_target_close_button"],
            0.822,
            0.161,
            0.074,
            0.1,
        );
        assert_battle_close_button_element(
            &elements["order_change_close_button"],
            0.918,
            0.138,
            0.074,
            0.1,
        );
        assert_battle_close_button_element(
            &elements["command_spell_close_button"],
            0.826,
            0.159,
            0.074,
            0.1,
        );
    }

    fn assert_battle_close_button_element(
        element: &serde_json::Value,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    ) {
        assert_eq!(
            element["template"].as_str(),
            Some("shared/battle_close_button")
        );
        assert_close(element["region"]["x"].as_f64().unwrap(), x);
        assert_close(element["region"]["y"].as_f64().unwrap(), y);
        assert_close(element["region"]["w"].as_f64().unwrap(), w);
        assert_close(element["region"]["h"].as_f64().unwrap(), h);
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-9,
            "expected {expected}, got {actual}"
        );
    }

    // --- input coordinate sizing --------------------------------------

    #[test]
    fn input_size_for_taps_uses_stream_when_adb_size_missing() {
        assert_eq!(input_size_for_taps(None, (1920, 1080)), (1920, 1080));
    }

    #[test]
    fn input_size_for_taps_prefers_adb_size_with_matching_orientation() {
        assert_eq!(
            input_size_for_taps(Some((2560, 1440)), (1920, 1080)),
            (2560, 1440)
        );
    }

    #[test]
    fn input_size_for_taps_swaps_adb_size_to_match_stream_orientation() {
        assert_eq!(
            input_size_for_taps(Some((1080, 1920)), (1920, 1080)),
            (1920, 1080)
        );
    }

    #[test]
    fn stream_minimum_resolution_requires_1080p_landscape_or_better() {
        assert!(stream_meets_minimum_resolution(1920, 1080));
        assert!(stream_meets_minimum_resolution(2560, 1440));
        assert!(!stream_meets_minimum_resolution(1280, 720));
        assert!(!stream_meets_minimum_resolution(1600, 900));
    }

    // --- app data identifier migration --------------------------------

    #[test]
    fn migrate_legacy_app_data_copies_old_identifier_when_current_is_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let legacy = tmp.path().join("com.mash.app");
        let current = tmp.path().join("com.xiaotongx.mash");
        fs::create_dir_all(legacy.join("runtime/mash-cv/code/code-v1")).unwrap();
        fs::write(legacy.join("projects.json"), b"[]").unwrap();
        fs::write(
            legacy.join("runtime/mash-cv/code/code-v1/code-version.json"),
            br#"{"version":"code-v1","platform":"darwin-aarch64"}"#,
        )
        .unwrap();

        let migrated = migrate_legacy_app_data_dir(&current, &[legacy.clone()]).unwrap();

        assert_eq!(migrated.as_deref(), Some(legacy.as_path()));
        assert!(current.join("projects.json").is_file());
        assert!(current
            .join("runtime/mash-cv/code/code-v1/code-version.json")
            .is_file());
        assert!(current.join("identifier-migration.json").is_file());
    }

    #[test]
    fn migrate_legacy_app_data_skips_when_current_has_data() {
        let tmp = tempfile::tempdir().unwrap();
        let legacy = tmp.path().join("com.mash.app");
        let current = tmp.path().join("com.xiaotongx.mash");
        fs::create_dir_all(&legacy).unwrap();
        fs::create_dir_all(&current).unwrap();
        fs::write(legacy.join("projects.json"), b"[]").unwrap();
        fs::write(current.join("projects.json"), br#"[{"id":"new"}]"#).unwrap();

        let migrated = migrate_legacy_app_data_dir(&current, &[legacy]).unwrap();

        assert_eq!(migrated, None);
        assert_eq!(
            fs::read_to_string(current.join("projects.json")).unwrap(),
            r#"[{"id":"new"}]"#
        );
        assert!(!current.join("identifier-migration.json").exists());
    }

    #[test]
    fn migrate_legacy_app_data_allows_empty_current_scaffolding() {
        let tmp = tempfile::tempdir().unwrap();
        let legacy = tmp.path().join("com.mash.app");
        let current = tmp.path().join("com.xiaotongx.mash");
        fs::create_dir_all(&current).unwrap();
        fs::create_dir_all(current.join("assets")).unwrap();
        fs::create_dir_all(legacy.join("runtime")).unwrap();
        fs::write(legacy.join("projects.json"), b"[]").unwrap();

        let migrated = migrate_legacy_app_data_dir(&current, &[legacy.clone()]).unwrap();

        assert_eq!(migrated.as_deref(), Some(legacy.as_path()));
        assert!(current.join("assets").is_dir());
        assert!(current.join("projects.json").is_file());
    }

    #[cfg(unix)]
    #[test]
    fn migrate_legacy_app_data_preserves_symlinks() {
        use std::os::unix::fs::symlink;

        let tmp = tempfile::tempdir().unwrap();
        let legacy = tmp.path().join("com.mash.app");
        let current = tmp.path().join("com.xiaotongx.mash");
        fs::create_dir_all(legacy.join("runtime/mash-cv/runtime/runtime-v1/lib")).unwrap();
        fs::write(
            legacy.join("runtime/mash-cv/runtime/runtime-v1/lib/real.dylib"),
            b"lib",
        )
        .unwrap();
        symlink(
            "real.dylib",
            legacy.join("runtime/mash-cv/runtime/runtime-v1/lib/link.dylib"),
        )
        .unwrap();

        migrate_legacy_app_data_dir(&current, &[legacy]).unwrap();

        let migrated_link = current.join("runtime/mash-cv/runtime/runtime-v1/lib/link.dylib");
        assert!(fs::symlink_metadata(&migrated_link)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(
            fs::read_link(migrated_link).unwrap(),
            PathBuf::from("real.dylib")
        );
    }

    // --- default_project_slots -----------------------------------------

    #[test]
    fn default_project_slots_yields_six_slots_with_support_at_index_two() {
        let slots = default_project_slots();
        assert_eq!(slots.len(), 6);
        for (i, slot) in slots.iter().enumerate() {
            assert_eq!(slot.id, format!("slot-{i}"));
            assert!(slot.servant_id.is_none());
            assert!(slot.craft_essence_id.is_none());
        }
        // Slot 2 is the support pin; everything else is a party slot.
        assert_eq!(slots[2].kind, "support");
        for i in [0, 1, 3, 4, 5] {
            assert_eq!(slots[i].kind, "servant");
        }
    }

    // --- app UI settings ----------------------------------------------

    #[test]
    fn app_ui_settings_persist_active_project_id() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("app_ui_settings.json");

        let initial = read_app_ui_settings_from_path(&path);
        assert!(initial.active_project_id.is_none());

        write_app_ui_settings_to_path(
            &path,
            &AppUiSettings {
                active_project_id: Some("project-2".into()),
                theme: Some("system".into()),
            },
        )
        .unwrap();

        let saved = read_app_ui_settings_from_path(&path);
        assert_eq!(saved.active_project_id.as_deref(), Some("project-2"));
        assert_eq!(saved.theme.as_deref(), Some("system"));
    }

    fn test_project(id: &str, name: &str, advanced_mode: bool) -> Project {
        Project {
            id: id.into(),
            name: name.into(),
            advanced_mode,
            support_servant_id: None,
            support_servant_variant_key: None,
            support_grand_mode: false,
            support_grand_craft_essence_ids: default_support_grand_craft_essence_ids(),
            support_grand_craft_essence_mlb_required:
                default_support_grand_craft_essence_mlb_required(),
            support_grand_bond_ce_mode: SupportGrandBondCeMode::Any,
            grand_class: GrandClass::Saber,
            grand_servants: Vec::new(),
            grand_card_strategy: GrandCardStrategy::default(),
            support_noble_phantasm_level_min: None,
            support_skill_level_mins: default_support_skill_level_mins(),
            support_append_skill_level_mins: default_support_append_skill_level_mins(),
            slots: default_project_slots(),
            repeat_mission: false,
            repeat_mode: Some(ProjectRepeatMode::Single),
            repeat_count: None,
            ap_recovery_items: Vec::new(),
        }
    }

    fn test_battle_scene(id: &str) -> BattleScene {
        BattleScene {
            id: id.into(),
            turns: Vec::new(),
            preparation_actions: Vec::new(),
            servant_actions: Vec::new(),
            equipment_actions: Vec::new(),
            command_spell_actions: Vec::new(),
            enemy_target: None,
            attack_priority: Vec::new(),
        }
    }

    fn test_advanced_scene(id: &str) -> AdvancedBattleScene {
        AdvancedBattleScene {
            id: id.into(),
            main_output: None,
            grand_auto_order_change: None,
            command_conditions: Vec::new(),
            control_actions: Vec::new(),
            startup_actions: Vec::new(),
            rules: Vec::new(),
        }
    }

    #[test]
    fn config_export_package_contains_selected_projects_and_scenes() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let projects = vec![
            test_project("project-1", "第一套", false),
            test_project("project-2", "第二套", true),
        ];
        write_projects_to_path(&root.join("projects.json"), &projects).unwrap();
        write_battle_scenes_to_root(root, "project-1", &[test_battle_scene("battle-1")]).unwrap();
        write_advanced_battle_scenes_to_root(
            root,
            "project-2",
            &[test_advanced_scene("advanced-1")],
        )
        .unwrap();

        let package = export_package_for_project_ids(root, &[String::from("project-2")]).unwrap();

        assert_eq!(package.schema_version, 1);
        assert_eq!(package.configs.len(), 1);
        assert_eq!(package.configs[0].project.id, "project-2");
        assert!(package.configs[0].battle_scenes.is_empty());
        assert_eq!(package.configs[0].advanced_battle_scenes.len(), 1);
    }

    #[test]
    fn config_export_timestamp_uses_filename_friendly_utc_format() {
        assert_eq!(format_unix_timestamp_utc(0), "19700101-000000");
        assert_eq!(format_unix_timestamp_utc(1_704_067_199), "20231231-235959");
    }

    #[test]
    fn config_import_preview_reports_valid_and_invalid_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_projects_to_path(
            &root.join("projects.json"),
            &[test_project("existing", "第一套", false)],
        )
        .unwrap();
        let path = root.join("import.mashconfig.json");
        let valid_project = serde_json::to_value(test_project("source", "第一套", false)).unwrap();
        fs::write(
            &path,
            serde_json::json!({
                "schemaVersion": 1,
                "exportedAt": "test",
                "configs": [
                    {
                        "project": valid_project,
                        "battleScenes": [test_battle_scene("battle-1")],
                        "advancedBattleScenes": []
                    },
                    {
                        "project": { "id": "broken", "name": "" },
                        "battleScenes": []
                    }
                ]
            })
            .to_string(),
        )
        .unwrap();

        let preview = preview_config_import_from_path(root, &path).unwrap();

        assert_eq!(preview.valid_configs.len(), 1);
        assert_eq!(preview.valid_configs[0].source_name, "第一套");
        assert_eq!(preview.valid_configs[0].target_name, "第一套（导入）");
        assert_eq!(preview.valid_configs[0].battle_scene_count, 1);
        assert_eq!(preview.invalid_items.len(), 1);
    }

    #[test]
    fn config_import_appends_copies_with_new_ids_and_unique_names() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_projects_to_path(
            &root.join("projects.json"),
            &[
                test_project("existing-1", "第一套", false),
                test_project("existing-2", "第一套（导入）", false),
            ],
        )
        .unwrap();
        let package = ConfigExportPackage {
            schema_version: 1,
            exported_at: "test".into(),
            app_version: None,
            configs: vec![ConfigExportEntry {
                project: test_project("source", "第一套", false),
                battle_scenes: vec![test_battle_scene("battle-1")],
                advanced_battle_scenes: vec![test_advanced_scene("advanced-1")],
            }],
        };
        let zip_path = root.join("import.mashconfig.zip");
        write_config_package_zip(&package, &zip_path).unwrap();

        let result =
            import_configurations_from_path(root, &zip_path, &[String::from("0")]).unwrap();

        assert_eq!(result.imported_projects.len(), 1);
        let imported = &result.imported_projects[0];
        assert_ne!(imported.id, "source");
        assert_eq!(imported.name, "第一套（导入 2）");
        let projects = read_projects_from_path(&root.join("projects.json"));
        assert_eq!(projects.len(), 3);
        assert_eq!(load_battle_scenes_from_root(root, &imported.id).len(), 1);
        assert_eq!(
            load_advanced_battle_scenes_from_root(root, &imported.id).len(),
            1
        );
    }

    #[test]
    fn config_import_only_imports_selected_keys() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_projects_to_path(&root.join("projects.json"), &[]).unwrap();
        let package = ConfigExportPackage {
            schema_version: 1,
            exported_at: "test".into(),
            app_version: None,
            configs: vec![
                ConfigExportEntry {
                    project: test_project("source-1", "第一套", false),
                    battle_scenes: vec![test_battle_scene("battle-1")],
                    advanced_battle_scenes: Vec::new(),
                },
                ConfigExportEntry {
                    project: test_project("source-2", "第二套", true),
                    battle_scenes: Vec::new(),
                    advanced_battle_scenes: vec![test_advanced_scene("advanced-1")],
                },
            ],
        };
        let zip_path = root.join("import.mashconfig.zip");
        write_config_package_zip(&package, &zip_path).unwrap();

        let result =
            import_configurations_from_path(root, &zip_path, &[String::from("1")]).unwrap();

        assert_eq!(result.imported_projects.len(), 1);
        assert_eq!(result.imported_projects[0].name, "第二套（导入）");
        let projects = read_projects_from_path(&root.join("projects.json"));
        assert_eq!(projects.len(), 1);
        assert_eq!(
            load_advanced_battle_scenes_from_root(root, &result.imported_projects[0].id).len(),
            1
        );
        assert!(load_battle_scenes_from_root(root, &result.imported_projects[0].id).is_empty());
    }

    #[test]
    fn config_import_rejects_malformed_json_and_missing_selection() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let bad_json = root.join("bad.mashconfig.json");
        fs::write(&bad_json, "{").unwrap();
        assert!(preview_config_import_from_path(root, &bad_json)
            .unwrap_err()
            .contains("有效 JSON"));
        assert!(export_package_for_project_ids(root, &[])
            .unwrap_err()
            .contains("请选择"));
        assert!(import_configurations_from_path(root, &bad_json, &[])
            .unwrap_err()
            .contains("请选择"));
        assert!(
            export_package_for_project_ids(root, &[String::from("missing")])
                .unwrap_err()
                .contains("未找到配置")
        );
    }

    // --- ProjectSlot serde --------------------------------------------

    #[test]
    fn project_slot_legacy_json_without_ce_field_deserializes_with_none() {
        // Mirrors a row from a pre-CE-picker `projects.json`. The
        // `#[serde(default)]` on `craft_essence_id` is what keeps these
        // legacy rows loading; this test guards against accidentally
        // dropping that attribute.
        let json = serde_json::json!({
            "id": "slot-0",
            "type": "servant",
            "servantId": 284,
        });
        let slot: ProjectSlot = serde_json::from_value(json).unwrap();
        assert_eq!(slot.id, "slot-0");
        assert_eq!(slot.kind, "servant");
        assert_eq!(slot.servant_id, Some(284));
        assert!(slot.craft_essence_id.is_none());
        assert_eq!(slot.craft_essence_mlb_required, true);
    }

    #[test]
    fn project_slot_round_trips_craft_essence_id() {
        let json = serde_json::json!({
            "id": "slot-2",
            "type": "support",
            "servantId": 284,
            "craftEssenceId": 1485,
        });
        let slot: ProjectSlot = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(slot.craft_essence_id, Some(1485));
        assert_eq!(slot.craft_essence_mlb_required, true);

        // Camel-case rename round-trips on serialize too.
        let serialized = serde_json::to_value(&slot).unwrap();
        assert_eq!(serialized["craftEssenceId"], serde_json::json!(1485));
        assert_eq!(
            serialized["craftEssenceMlbRequired"],
            serde_json::json!(true)
        );
        assert_eq!(serialized["servantId"], serde_json::json!(284));
        assert_eq!(serialized["type"], serde_json::json!("support"));
    }

    #[test]
    fn project_slot_servant_id_also_defaults_when_missing() {
        // Sanity-check the sibling `#[serde(default)]` on `servant_id`
        // so a slot row with neither id field still parses (legacy
        // empty slots).
        let json = serde_json::json!({
            "id": "slot-0",
            "type": "servant",
        });
        let slot: ProjectSlot = serde_json::from_value(json).unwrap();
        assert!(slot.servant_id.is_none());
        assert!(slot.craft_essence_id.is_none());
        assert_eq!(slot.craft_essence_mlb_required, true);
    }

    // --- Project (top-level legacy JSON) -------------------------------

    #[test]
    fn project_legacy_json_without_slots_falls_back_to_defaults() {
        // The `#[serde(default = "default_project_slots")]` attribute is
        // what makes pre-team-builder `projects.json` rows continue to
        // load; this test pins that contract.
        let json = serde_json::json!({
            "id": "abc",
            "name": "Legacy",
        });
        let project: Project = serde_json::from_value(json).unwrap();
        assert_eq!(project.slots.len(), 6);
        assert_eq!(project.advanced_mode, false);
        assert!(project.support_servant_id.is_none());
        assert_eq!(project.support_grand_mode, false);
        assert_eq!(project.support_grand_craft_essence_ids, [None; 3]);
        assert_eq!(project.support_grand_craft_essence_mlb_required, [true; 3]);
        assert_eq!(
            project.support_grand_bond_ce_mode,
            SupportGrandBondCeMode::Any
        );
        assert_eq!(project.grand_class, GrandClass::Saber);
        assert!(project.grand_servants.is_empty());
        assert_eq!(
            project.grand_card_strategy.chain_priority,
            default_grand_chain_priority()
        );
        assert!(project.support_noble_phantasm_level_min.is_none());
        assert_eq!(project.support_skill_level_mins, [None; 3]);
        assert_eq!(project.support_append_skill_level_mins, [None; 5]);
        assert_eq!(project.repeat_mission, false);
        assert!(project.repeat_mode.is_none());
        assert!(project.repeat_count.is_none());
        assert!(project.ap_recovery_items.is_empty());
    }

    #[test]
    fn project_grand_class_round_trips_as_camel_case() {
        let json = serde_json::json!({
            "id": "abc",
            "name": "Grand",
            "grandClass": "berserker",
        });
        let project: Project = serde_json::from_value(json).unwrap();
        assert_eq!(project.grand_class, GrandClass::Berserker);

        let serialized = serde_json::to_value(&project).unwrap();
        assert_eq!(serialized["grandClass"], serde_json::json!("berserker"));
    }

    #[test]
    fn normalize_project_migrates_legacy_repeat_flag_to_infinite_mode() {
        let project = normalize_project(Project {
            id: "abc".into(),
            name: "Legacy".into(),
            advanced_mode: false,
            support_servant_id: None,
            support_servant_variant_key: None,
            support_grand_mode: false,
            support_grand_craft_essence_ids: default_support_grand_craft_essence_ids(),
            support_grand_craft_essence_mlb_required:
                default_support_grand_craft_essence_mlb_required(),
            support_grand_bond_ce_mode: SupportGrandBondCeMode::Any,
            grand_class: GrandClass::Saber,
            grand_servants: Vec::new(),
            grand_card_strategy: GrandCardStrategy::default(),
            support_noble_phantasm_level_min: None,
            support_skill_level_mins: default_support_skill_level_mins(),
            support_append_skill_level_mins: default_support_append_skill_level_mins(),
            slots: default_project_slots(),
            repeat_mission: true,
            repeat_mode: None,
            repeat_count: Some(9),
            ap_recovery_items: Vec::new(),
        });

        assert_eq!(project.repeat_mission, true);
        assert!(matches!(
            project.repeat_mode,
            Some(ProjectRepeatMode::Infinite)
        ));
        assert_eq!(project.repeat_count, None);
    }

    #[test]
    fn copy_project_dir_recursively_copies_saved_project_files() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let nested = src.join("nested");
        let dst = tmp.path().join("dst");
        fs::create_dir_all(&nested).unwrap();
        fs::write(src.join("battle_scenes.json"), br#"[{"id":"scene-1"}]"#).unwrap();
        fs::write(nested.join("notes.json"), br#"{"ok":true}"#).unwrap();

        copy_project_dir(&src, &dst).unwrap();

        assert_eq!(
            fs::read_to_string(dst.join("battle_scenes.json")).unwrap(),
            r#"[{"id":"scene-1"}]"#
        );
        assert_eq!(
            fs::read_to_string(dst.join("nested").join("notes.json")).unwrap(),
            r#"{"ok":true}"#
        );
    }

    // --- pick_portrait_in ----------------------------------------------

    #[test]
    fn pick_portrait_in_returns_none_for_missing_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("does-not-exist");
        assert!(pick_portrait_in(&missing).is_none());
    }

    #[test]
    fn pick_portrait_in_returns_none_when_only_face_and_card_files_present() {
        // Mirrors the real `assets/servants/1/` layout for servants that
        // haven't had a `narrow_servant_*.png` portrait dropped in yet —
        // face and card art exist but they aren't full-body portraits
        // and must not be served as one.
        let tmp = tempfile::tempdir().unwrap();
        for name in ["face_servant_1.png", "card_servant_1.png", "servant.json"] {
            fs::write(tmp.path().join(name), b"").unwrap();
        }
        assert!(pick_portrait_in(tmp.path()).is_none());
    }

    #[test]
    fn pick_portrait_in_picks_highest_ascension_stage() {
        // With multiple `narrow_servant_<n>.png` siblings, the resolver
        // must hand back the lexicographically-largest filename — which
        // for the single-digit ascension scheme used by the Atlas dump
        // is also the highest stage (i.e. the final-ascension full art).
        let tmp = tempfile::tempdir().unwrap();
        for name in [
            "narrow_servant_3.png",
            "narrow_servant_4.png",
            "narrow_servant_1.png",
        ] {
            fs::write(tmp.path().join(name), b"").unwrap();
        }
        let picked = pick_portrait_in(tmp.path()).expect("expected a match");
        assert_eq!(
            picked.file_name().and_then(|n| n.to_str()),
            Some("narrow_servant_4.png")
        );
    }

    #[test]
    fn pick_portrait_by_id_in_prefers_exact_variant_asset() {
        let tmp = tempfile::tempdir().unwrap();
        for name in ["narrow_servant_4.png", "narrow_servant_800170.png"] {
            fs::write(tmp.path().join(name), b"").unwrap();
        }
        let picked = pick_portrait_by_id_in(tmp.path(), 800170).expect("expected a match");
        assert_eq!(
            picked.file_name().and_then(|n| n.to_str()),
            Some("narrow_servant_800170.png")
        );
        assert!(pick_portrait_by_id_in(tmp.path(), 800151).is_none());
    }

    #[test]
    fn pick_face_in_picks_highest_ascension_stage() {
        let tmp = tempfile::tempdir().unwrap();
        for name in [
            "face_servant_1.png",
            "face_servant_4.png",
            "narrow_servant_4.png",
        ] {
            fs::write(tmp.path().join(name), b"").unwrap();
        }
        let picked = pick_face_in(tmp.path()).expect("expected a face match");
        assert_eq!(
            picked.file_name().and_then(|n| n.to_str()),
            Some("face_servant_4.png")
        );
    }

    #[test]
    fn pick_faces_desc_in_returns_all_faces_high_to_low() {
        let tmp = tempfile::tempdir().unwrap();
        for name in [
            "face_servant_1.png",
            "face_servant_10.png",
            "face_servant_4.png",
            "narrow_servant_4.png",
        ] {
            fs::write(tmp.path().join(name), b"").unwrap();
        }
        let picked = pick_faces_desc_in(tmp.path());
        let names: Vec<_> = picked
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();
        assert_eq!(
            names,
            vec![
                "face_servant_10.png",
                "face_servant_4.png",
                "face_servant_1.png"
            ]
        );
    }

    fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        build_zip_with_options(
            &entries
                .iter()
                .map(|(name, contents)| (*name, *contents, None))
                .collect::<Vec<_>>(),
        )
    }

    fn build_zip_with_options(entries: &[(&str, &[u8], Option<u32>)]) -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut cursor);
            for (name, contents, unix_permissions) in entries {
                if unix_permissions.is_some_and(|mode| (mode & 0o170000) == 0o120000) {
                    writer
                        .add_symlink(
                            name,
                            String::from_utf8_lossy(contents),
                            SimpleFileOptions::default(),
                        )
                        .unwrap();
                    continue;
                }
                let options = unix_permissions.map_or(SimpleFileOptions::default(), |mode| {
                    SimpleFileOptions::default().unix_permissions(mode)
                });
                writer.start_file(name, options).unwrap();
                writer.write_all(contents).unwrap();
            }
            writer.finish().unwrap();
        }
        cursor.into_inner()
    }

    #[test]
    fn import_asset_bundle_from_zip_path_accepts_assets_wrapper() {
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("bundle.zip");
        fs::write(
            &zip_path,
            build_zip(&[
                ("assets/servants/1/narrow_servant_4.png", b"portrait"),
                ("assets/ces/2/card_ce.png", b"ce"),
            ]),
        )
        .unwrap();

        let result = import_asset_bundle_from_zip_path(&zip_path, &tmp.path().join("installed"))
            .expect("import should succeed");

        assert!(result.imported_servants);
        assert!(result.imported_craft_essences);
        assert_eq!(result.servant_files, 1);
        assert_eq!(result.craft_essence_files, 1);
        assert!(tmp
            .path()
            .join("installed")
            .join("servants")
            .join("1")
            .join("narrow_servant_4.png")
            .is_file());
        assert!(tmp
            .path()
            .join("installed")
            .join("ces")
            .join("2")
            .join("card_ce.png")
            .is_file());
    }

    #[test]
    fn import_asset_bundle_from_zip_path_writes_nested_version_record() {
        let tmp = tempfile::tempdir().unwrap();
        let install_root = tmp.path().join("installed");
        let zip_path = tmp.path().join("bundle.zip");
        fs::write(
            &zip_path,
            build_zip(&[
                ("assets/assets-version.json", br#"{"version":2}"#),
                ("assets/servants/1/narrow_servant_4.png", b"portrait"),
                ("assets/ces/2/card_ce.png", b"ce"),
            ]),
        )
        .unwrap();

        import_asset_bundle_from_zip_path(&zip_path, &install_root).unwrap();

        assert_eq!(read_asset_version(&install_root).unwrap().version, 2);
    }

    #[test]
    fn import_asset_bundle_from_zip_path_writes_root_version_record() {
        let tmp = tempfile::tempdir().unwrap();
        let install_root = tmp.path().join("installed");
        let zip_path = tmp.path().join("bundle.zip");
        fs::write(
            &zip_path,
            build_zip(&[
                ("assets-version.json", br#"{"version":2}"#),
                ("servants/1/narrow_servant_4.png", b"portrait"),
                ("ces/2/card_ce.png", b"ce"),
            ]),
        )
        .unwrap();

        import_asset_bundle_from_zip_path(&zip_path, &install_root).unwrap();

        assert_eq!(read_asset_version(&install_root).unwrap().version, 2);
    }

    #[test]
    fn import_asset_bundle_from_zip_path_defaults_missing_version_to_v1() {
        let tmp = tempfile::tempdir().unwrap();
        let install_root = tmp.path().join("installed");
        let zip_path = tmp.path().join("bundle.zip");
        fs::write(
            &zip_path,
            build_zip(&[
                ("servants/1/narrow_servant_4.png", b"portrait"),
                ("ces/2/card_ce.png", b"ce"),
            ]),
        )
        .unwrap();

        import_asset_bundle_from_zip_path(&zip_path, &install_root).unwrap();

        assert_eq!(read_asset_version(&install_root).unwrap().version, 1);
    }

    #[test]
    fn import_asset_bundle_from_zip_path_rejects_invalid_version_record() {
        let tmp = tempfile::tempdir().unwrap();
        let install_root = tmp.path().join("installed");
        let zip_path = tmp.path().join("bundle.zip");
        fs::write(
            &zip_path,
            build_zip(&[
                ("assets/assets-version.json", br#"{"version":"bad"}"#),
                ("assets/servants/1/narrow_servant_4.png", b"portrait"),
                ("assets/ces/2/card_ce.png", b"ce"),
            ]),
        )
        .unwrap();

        let err = import_asset_bundle_from_zip_path(&zip_path, &install_root).unwrap_err();

        assert!(err.contains("解析素材包版本记录失败"));
        assert!(!asset_version_path(&install_root).exists());
        assert!(!install_root.join("servants").exists());
        assert!(!install_root.join("ces").exists());
    }

    #[test]
    fn import_asset_bundle_from_zip_path_replaces_existing_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let install_root = tmp.path().join("installed");
        let existing = install_root.join("servants").join("1");
        fs::create_dir_all(&existing).unwrap();
        fs::write(existing.join("old.png"), b"old").unwrap();

        let zip_path = tmp.path().join("bundle.zip");
        fs::write(&zip_path, build_zip(&[("servants/1/new.png", b"new")])).unwrap();

        let result = import_asset_bundle_from_zip_path(&zip_path, &install_root)
            .expect("import should work");

        assert!(result.imported_servants);
        assert_eq!(result.servant_files, 1);
        assert!(!install_root
            .join("servants")
            .join("1")
            .join("old.png")
            .exists());
        assert!(install_root
            .join("servants")
            .join("1")
            .join("new.png")
            .is_file());
    }

    #[test]
    fn import_asset_bundle_from_zip_path_rejects_zip_without_asset_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("bundle.zip");
        fs::write(&zip_path, build_zip(&[("docs/readme.txt", b"no assets")])).unwrap();

        let err = import_asset_bundle_from_zip_path(&zip_path, &tmp.path().join("installed"))
            .expect_err("import should fail");
        assert!(err.contains("assets/servants") || err.contains("assets/ces"));
    }

    #[test]
    fn asset_bundle_status_requires_servants_and_craft_essences() {
        let tmp = tempfile::tempdir().unwrap();
        let assets_root = tmp.path().join("assets");
        let app_manifest = build_assets_app_manifest(2);
        let missing = asset_bundle_status_from_root(&assets_root, &app_manifest);
        assert!(!missing.installed);
        assert!(!missing.imported_servants);
        assert!(!missing.imported_craft_essences);
        assert_eq!(missing.current_version, None);

        fs::create_dir_all(assets_root.join("servants").join("1")).unwrap();
        fs::write(
            assets_root
                .join("servants")
                .join("1")
                .join("face_servant_1.png"),
            b"face",
        )
        .unwrap();
        let partial = asset_bundle_status_from_root(&assets_root, &app_manifest);
        assert!(!partial.installed);
        assert!(partial.imported_servants);
        assert!(!partial.imported_craft_essences);
        assert_eq!(partial.servant_files, 1);
        assert_eq!(partial.current_version, Some(1));

        fs::create_dir_all(assets_root.join("ces").join("2")).unwrap();
        fs::write(assets_root.join("ces").join("2").join("card_ce.png"), b"ce").unwrap();
        let stale = asset_bundle_status_from_root(&assets_root, &app_manifest);
        assert!(!stale.installed);
        assert_eq!(stale.servant_files, 1);
        assert_eq!(stale.craft_essence_files, 1);
        assert_eq!(stale.current_version, Some(1));
        assert_eq!(stale.target_version, Some(2));
        assert!(stale.update_available);
        assert_eq!(stale.update_download_size, 0);
        assert_eq!(stale.update_plan, "pending");
        assert_eq!(stale.remote_latest_version, None);

        write_asset_version(&assets_root, 2).unwrap();
        let installed = asset_bundle_status_from_root(&assets_root, &app_manifest);
        assert!(installed.installed);
        assert_eq!(installed.current_version, Some(2));
        assert_eq!(installed.target_version, None);
        assert!(!installed.update_available);
    }

    #[test]
    fn self_check_asset_group_reports_empty_missing_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let status = scan_self_check_asset_group(&tmp.path().join("servants"));

        assert_eq!(status.entries, 0);
        assert!(!status.has_image);
        assert!(!status.has_json);
    }

    #[test]
    fn self_check_asset_group_counts_only_top_level_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let servants = tmp.path().join("servants");
        fs::create_dir_all(servants.join("1").join("nested")).unwrap();
        fs::create_dir_all(servants.join("2")).unwrap();
        fs::write(servants.join("1").join("face_servant_1.png"), b"png").unwrap();
        fs::write(servants.join("1").join("nested").join("extra.json"), b"{}").unwrap();
        fs::write(servants.join("loose.json"), b"{}").unwrap();

        let status = scan_self_check_asset_group(&servants);

        assert_eq!(status.entries, 2);
        assert!(status.has_image);
        assert!(status.has_json);
    }

    #[test]
    fn self_check_asset_group_scans_servants_and_ces_independently() {
        let tmp = tempfile::tempdir().unwrap();
        let assets_root = tmp.path().join("assets");
        let servants = assets_root.join("servants");
        let ces = assets_root.join("ces");
        fs::create_dir_all(servants.join("1")).unwrap();
        fs::create_dir_all(ces.join("10")).unwrap();
        fs::write(servants.join("1").join("servant.json"), b"{}").unwrap();
        fs::write(ces.join("10").join("card_ce.webp"), b"webp").unwrap();

        let servant_status = scan_self_check_asset_group(&servants);
        let ce_status = scan_self_check_asset_group(&ces);

        assert_eq!(servant_status.entries, 1);
        assert!(!servant_status.has_image);
        assert!(servant_status.has_json);
        assert_eq!(ce_status.entries, 1);
        assert!(ce_status.has_image);
        assert!(!ce_status.has_json);
    }

    fn build_assets_app_manifest(version: u32) -> AssetsAppManifest {
        parse_assets_app_manifest(&format!(
            r#"{{
                "assetsVersion": {version},
                "latestUrl": "https://mash.xiaotongx.com/mash/assets/latest.json"
            }}"#
        ))
        .unwrap()
    }

    fn build_assets_remote_manifest(latest: u32, patches: &str) -> AssetsRemoteManifest {
        parse_assets_remote_manifest(&format!(
            r#"{{
                "latest": {latest},
                "latestBase": 1,
                "base": {{
                    "version": 1,
                    "packs": [
                        {{
                            "name": "assets-json",
                            "file": "base/v1/assets-json-v1.zip",
                            "sha256": "base-json-sha",
                            "size": 10
                        }},
                        {{
                            "name": "servant-images",
                            "file": "base/v1/servant-images-v1.zip",
                            "sha256": "base-servants-sha",
                            "size": 20
                        }}
                    ]
                }},
                "patches": {patches}
            }}"#
        ))
        .unwrap()
    }

    #[test]
    fn assets_app_manifest_parses_target_version_and_latest_url() {
        let manifest = parse_assets_app_manifest(ASSETS_MANIFEST_JSON).unwrap();
        assert_eq!(manifest.assets_version, 2);
        assert_eq!(
            manifest.latest_url,
            "https://mash.xiaotongx.com/mash/assets/latest.json"
        );
    }

    #[test]
    fn asset_update_plan_caps_target_to_app_configured_version() {
        let remote = build_assets_remote_manifest(
            3,
            r#"[
                {"from": 1, "to": 2, "file": "patches/v1-to-v2.zip", "sha256": "p12", "size": 7},
                {"from": 2, "to": 3, "file": "patches/v2-to-v3.zip", "sha256": "p23", "size": 9}
            ]"#,
        );

        let plan = asset_update_plan(Some(1), 2, &remote, false);

        assert_eq!(plan.target_version(), Some(2));
        assert_eq!(plan.plan_type(), "patch");
        assert_eq!(plan.download_size(), 7);
    }

    #[test]
    fn asset_update_plan_installs_base_then_patches_for_missing_assets() {
        let remote = build_assets_remote_manifest(
            2,
            r#"[{"from": 1, "to": 2, "file": "patches/v1-to-v2.zip", "sha256": "p12", "size": 7}]"#,
        );

        let plan = asset_update_plan(None, 2, &remote, false);

        assert_eq!(plan.target_version(), Some(2));
        assert_eq!(plan.plan_type(), "base");
        assert_eq!(plan.download_size(), 37);
    }

    #[test]
    fn asset_update_plan_chains_patches_to_target() {
        let remote = build_assets_remote_manifest(
            3,
            r#"[
                {"from": 1, "to": 2, "file": "patches/v1-to-v2.zip", "sha256": "p12", "size": 7},
                {"from": 2, "to": 3, "file": "patches/v2-to-v3.zip", "sha256": "p23", "size": 9}
            ]"#,
        );

        let plan = asset_update_plan(Some(1), 3, &remote, false);

        assert_eq!(plan.target_version(), Some(3));
        assert_eq!(plan.plan_type(), "patch");
        assert_eq!(plan.download_size(), 16);
    }

    #[test]
    fn asset_update_plan_falls_back_to_base_when_patch_chain_is_missing() {
        let remote = build_assets_remote_manifest(
            3,
            r#"[{"from": 2, "to": 3, "file": "patches/v2-to-v3.zip", "sha256": "p23", "size": 9}]"#,
        );

        let plan = asset_update_plan(Some(1), 3, &remote, false);

        assert_eq!(plan.target_version(), Some(1));
        assert_eq!(plan.plan_type(), "base");
        assert_eq!(plan.download_size(), 30);
    }

    #[test]
    fn asset_update_plan_force_base_reinstalls_even_when_current_is_latest() {
        let remote = build_assets_remote_manifest(
            2,
            r#"[{"from": 1, "to": 2, "file": "patches/v1-to-v2.zip", "sha256": "p12", "size": 7}]"#,
        );

        let plan = asset_update_plan(Some(2), 2, &remote, true);

        assert_eq!(plan.target_version(), Some(2));
        assert_eq!(plan.plan_type(), "force-base");
        assert_eq!(plan.download_size(), 37);
    }

    #[test]
    fn install_asset_zip_base_replace_combines_base_packs_before_replacing() {
        let tmp = tempfile::tempdir().unwrap();
        let assets_root = tmp.path().join("assets");
        fs::create_dir_all(assets_root.join("servants").join("1")).unwrap();
        fs::write(
            assets_root.join("servants").join("1").join("old.png"),
            b"old",
        )
        .unwrap();

        let json_zip = tmp.path().join("json.zip");
        fs::write(
            &json_zip,
            build_zip(&[("assets/servants/1/servant.json", br#"{"name":"test"}"#)]),
        )
        .unwrap();
        let image_zip = tmp.path().join("image.zip");
        fs::write(
            &image_zip,
            build_zip(&[("assets/servants/1/face_servant_1.png", b"face")]),
        )
        .unwrap();

        let stats = install_asset_zip_base_replace(&[json_zip, image_zip], &assets_root).unwrap();

        assert_eq!(stats.files, 2);
        assert!(!assets_root
            .join("servants")
            .join("1")
            .join("old.png")
            .exists());
        assert!(assets_root
            .join("servants")
            .join("1")
            .join("servant.json")
            .is_file());
        assert!(assets_root
            .join("servants")
            .join("1")
            .join("face_servant_1.png")
            .is_file());
    }

    #[test]
    fn cleanup_replaced_asset_trees_removes_only_asset_replaced_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let assets_root = tmp.path().join("assets");
        fs::create_dir_all(assets_root.join("servants.replaced-old")).unwrap();
        fs::create_dir_all(assets_root.join("ces.replaced-old")).unwrap();
        fs::create_dir_all(assets_root.join("servants")).unwrap();
        fs::create_dir_all(assets_root.join("other.replaced-old")).unwrap();
        fs::write(assets_root.join("servants.replaced-file"), b"file").unwrap();

        cleanup_replaced_asset_trees(&assets_root);

        assert!(!assets_root.join("servants.replaced-old").exists());
        assert!(!assets_root.join("ces.replaced-old").exists());
        assert!(assets_root.join("servants").is_dir());
        assert!(assets_root.join("other.replaced-old").is_dir());
        assert!(assets_root.join("servants.replaced-file").is_file());
    }

    #[test]
    fn install_asset_zip_merge_copies_json_and_png_without_deleting_old_files() {
        let tmp = tempfile::tempdir().unwrap();
        let assets_root = tmp.path().join("assets");
        fs::create_dir_all(assets_root.join("servants").join("1")).unwrap();
        fs::write(
            assets_root.join("servants").join("1").join("old.png"),
            b"old",
        )
        .unwrap();
        let zip_path = tmp.path().join("patch.zip");
        fs::write(
            &zip_path,
            build_zip(&[
                ("assets/servants/1/servant.json", br#"{"name":"test"}"#),
                ("assets/servants/1/new.png", b"new"),
                ("assets/ces/2/card_ce.png", b"ce"),
            ]),
        )
        .unwrap();

        let stats = install_asset_zip_merge(&zip_path, &assets_root).unwrap();

        assert_eq!(stats.files, 3);
        assert!(assets_root
            .join("servants")
            .join("1")
            .join("old.png")
            .is_file());
        assert!(assets_root
            .join("servants")
            .join("1")
            .join("servant.json")
            .is_file());
        assert!(assets_root
            .join("servants")
            .join("1")
            .join("new.png")
            .is_file());
        assert!(assets_root
            .join("ces")
            .join("2")
            .join("card_ce.png")
            .is_file());
    }

    #[test]
    fn verify_asset_artifact_rejects_size_and_sha_mismatches() {
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("asset.zip");
        fs::write(&zip_path, b"zip bytes").unwrap();

        let size_err = verify_asset_artifact(&zip_path, &"0".repeat(64), 1).unwrap_err();
        assert!(size_err.contains("大小不匹配"));

        let err = verify_asset_artifact(&zip_path, &"0".repeat(64), 9).unwrap_err();

        assert!(err.contains("sha256 不匹配"));
        assert!(!asset_version_path(tmp.path()).exists());
    }

    fn build_runtime_manifest(
        runtime_version: &str,
        code_version: &str,
        platform: &str,
        runtime_sha256: &str,
        code_sha256: &str,
    ) -> RuntimeManifest {
        parse_runtime_manifest(&format!(
            r#"{{
                "mashCvRuntimeVersion": "{runtime_version}",
                "mashCvCodeVersion": "{code_version}",
                "platforms": {{
                    "{platform}": {{
                        "runtimeUrl": "https://cdn.example.com/runtime.zip",
                        "runtimeSha256": "{runtime_sha256}",
                        "codeUrl": "https://cdn.example.com/code.zip",
                        "codeSha256": "{code_sha256}"
                    }}
                }}
            }}"#
        ))
        .unwrap()
    }

    fn sha256_bytes(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }

    #[test]
    fn runtime_manifest_selects_platform_artifact() {
        let manifest = build_runtime_manifest(
            "2026.05.08-runtime1",
            "2026.05.08-code1",
            "darwin-aarch64",
            "runtime-sha",
            "code-sha",
        );
        let artifact = manifest.platforms.get("darwin-aarch64").unwrap();
        assert_eq!(manifest.mash_cv_runtime_version, "2026.05.08-runtime1");
        assert_eq!(manifest.mash_cv_code_version, "2026.05.08-code1");
        assert_eq!(artifact.runtime_url, "https://cdn.example.com/runtime.zip");
        assert_eq!(artifact.runtime_sha256, "runtime-sha");
        assert_eq!(artifact.code_url, "https://cdn.example.com/code.zip");
        assert_eq!(artifact.code_sha256, "code-sha");
        assert!(manifest.platforms.get("darwin-x86_64").is_none());
    }

    #[test]
    fn runtime_platform_key_normalizes_macos_to_darwin() {
        assert_eq!(
            runtime_platform_key_from("macos", "aarch64"),
            "darwin-aarch64"
        );
        assert_eq!(
            runtime_platform_key_from("macos", "x86_64"),
            "darwin-x86_64"
        );
        assert_eq!(runtime_platform_key_from("linux", "x86_64"), "linux-x86_64");
    }

    #[test]
    fn runtime_status_reports_missing_matching_and_stale_versions() {
        let tmp = tempfile::tempdir().unwrap();
        let platform = "darwin-aarch64";
        let manifest = build_runtime_manifest("runtime-v2", "code-v2", platform, "abc", "def");

        let missing = runtime_status_from_manifest(&manifest, tmp.path(), platform);
        assert!(!missing.installed);
        assert!(!missing.runtime_installed);
        assert!(!missing.code_installed);
        assert_eq!(missing.installed_runtime_version, None);
        assert_eq!(missing.installed_code_version, None);

        fs::create_dir_all(runtime_version_path(tmp.path()).parent().unwrap()).unwrap();
        fs::write(
            runtime_version_path(tmp.path()),
            r#"{"version":"runtime-v1","platform":"darwin-aarch64"}"#,
        )
        .unwrap();
        fs::create_dir_all(code_version_path(tmp.path()).parent().unwrap()).unwrap();
        fs::write(
            code_version_path(tmp.path()),
            r#"{"version":"code-v1","platform":"darwin-aarch64"}"#,
        )
        .unwrap();
        let stale = runtime_status_from_manifest(&manifest, tmp.path(), platform);
        assert!(!stale.installed);
        assert_eq!(
            stale.installed_runtime_version.as_deref(),
            Some("runtime-v1")
        );
        assert_eq!(stale.installed_code_version.as_deref(), Some("code-v1"));

        let exe = runtime_executable_path(tmp.path(), "runtime-v2");
        fs::create_dir_all(exe.parent().unwrap()).unwrap();
        fs::write(&exe, b"exe").unwrap();
        let code_pkg = runtime_code_path(tmp.path(), "code-v2").join("mash_cv");
        fs::create_dir_all(&code_pkg).unwrap();
        fs::write(
            runtime_version_path(tmp.path()),
            r#"{"version":"runtime-v2","platform":"darwin-aarch64"}"#,
        )
        .unwrap();
        fs::write(
            code_version_path(tmp.path()),
            r#"{"version":"code-v2","platform":"darwin-aarch64"}"#,
        )
        .unwrap();
        let matching = runtime_status_from_manifest(&manifest, tmp.path(), platform);
        assert!(matching.installed);
        assert_eq!(
            matching.installed_runtime_version.as_deref(),
            Some("runtime-v2")
        );
        assert_eq!(matching.installed_code_version.as_deref(), Some("code-v2"));
        assert!(matching.executable_path.ends_with(runtime_exe_name()));
    }

    #[test]
    fn import_runtime_bundle_rejects_sha_mismatch() {
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("runtime.zip");
        fs::write(
            &zip_path,
            build_zip(&[("mash-cv-runtime/mash-cv", b"runtime executable")]),
        )
        .unwrap();
        let manifest = build_runtime_manifest(
            "runtime-v1",
            "code-v1",
            "darwin-aarch64",
            "bad-runtime-sha",
            "bad-code-sha",
        );

        let err = import_runtime_bundle_from_zip_path(
            &zip_path,
            &manifest,
            &tmp.path().join("runtime"),
            "darwin-aarch64",
        )
        .expect_err("import should fail");
        assert!(err.contains("sha256 不匹配"));
    }

    #[test]
    fn import_runtime_bundle_rejects_missing_executable() {
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("runtime.zip");
        let bytes = build_zip(&[("mash-cv-runtime/readme.txt", b"no exe")]);
        fs::write(&zip_path, &bytes).unwrap();
        let manifest = build_runtime_manifest(
            "runtime-v1",
            "code-v1",
            "darwin-aarch64",
            &sha256_bytes(&bytes),
            "code-sha",
        );

        let err = import_runtime_bundle_from_zip_path(
            &zip_path,
            &manifest,
            &tmp.path().join("runtime"),
            "darwin-aarch64",
        )
        .expect_err("import should fail");
        assert!(err.contains("未找到 mash-cv"));
    }

    #[test]
    fn import_runtime_bundle_rejects_path_traversal_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("runtime.zip");
        let bytes = build_zip(&[
            ("mash-cv-runtime/mash-cv", b"runtime executable"),
            ("../evil.txt", b"evil"),
        ]);
        fs::write(&zip_path, &bytes).unwrap();
        let manifest = build_runtime_manifest(
            "runtime-v1",
            "code-v1",
            "darwin-aarch64",
            &sha256_bytes(&bytes),
            "code-sha",
        );

        let err = import_runtime_bundle_from_zip_path(
            &zip_path,
            &manifest,
            &tmp.path().join("runtime"),
            "darwin-aarch64",
        )
        .expect_err("import should fail");
        assert!(err.contains("非法路径"));
        assert!(!tmp.path().join("evil.txt").exists());
    }

    #[test]
    fn import_runtime_bundle_installs_version_record_and_executable() {
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("runtime.zip");
        let exe_entry = format!("mash-cv-runtime/{}", runtime_exe_name());
        let bytes = build_zip(&[(exe_entry.as_str(), b"runtime executable")]);
        fs::write(&zip_path, &bytes).unwrap();
        let manifest = build_runtime_manifest(
            "runtime-v1",
            "code-v1",
            "darwin-aarch64",
            &sha256_bytes(&bytes),
            "code-sha",
        );
        let runtime_root = tmp.path().join("runtime");

        let result = import_runtime_bundle_from_zip_path(
            &zip_path,
            &manifest,
            &runtime_root,
            "darwin-aarch64",
        )
        .expect("import should work");

        assert_eq!(result.installed_kind, "runtime");
        assert_eq!(result.installed_version, "runtime-v1");
        let exe = runtime_executable_path(&runtime_root, "runtime-v1");
        assert!(exe.is_file());
        let record = read_runtime_version(&runtime_root).unwrap();
        assert_eq!(record.version, "runtime-v1");
        assert_eq!(record.platform, "darwin-aarch64");
    }

    #[cfg(unix)]
    #[test]
    fn import_runtime_bundle_preserves_unix_symlinked_dylibs() {
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("runtime.zip");
        let exe_entry = format!("mash-cv-runtime/{}", runtime_exe_name());
        let bytes = build_zip_with_options(&[
            (exe_entry.as_str(), b"runtime executable", Some(0o755)),
            (
                "mash-cv-runtime/_internal/cv2/.dylibs/libavif.16.3.0.dylib",
                b"real dylib bytes",
                Some(0o644),
            ),
            (
                "mash-cv-runtime/_internal/libavif.16.3.0.dylib",
                b"cv2/.dylibs/libavif.16.3.0.dylib",
                Some(0o120755),
            ),
        ]);
        fs::write(&zip_path, &bytes).unwrap();
        let manifest = build_runtime_manifest(
            "runtime-v1",
            "code-v1",
            "darwin-aarch64",
            &sha256_bytes(&bytes),
            "code-sha",
        );
        let runtime_root = tmp.path().join("runtime");

        import_runtime_bundle_from_zip_path(&zip_path, &manifest, &runtime_root, "darwin-aarch64")
            .expect("import should work");

        let link = runtime_version_dir(&runtime_root, "runtime-v1")
            .join("mash-cv-runtime")
            .join("_internal")
            .join("libavif.16.3.0.dylib");
        let meta = fs::symlink_metadata(&link).unwrap();
        assert!(
            meta.file_type().is_symlink(),
            "expected symlink, got {:?}",
            meta.file_type()
        );
        assert_eq!(
            fs::read_link(&link).unwrap(),
            PathBuf::from("cv2/.dylibs/libavif.16.3.0.dylib")
        );
        let target = link.parent().unwrap().join(fs::read_link(&link).unwrap());
        assert!(target.is_file());
    }

    #[test]
    fn import_runtime_bundle_installs_code_package() {
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("code.zip");
        let bytes = build_zip(&[
            ("mash-cv-code/mash_cv/__init__.py", b""),
            ("mash-cv-code/mash_cv/cv.py", b"code"),
        ]);
        fs::write(&zip_path, &bytes).unwrap();
        let manifest = build_runtime_manifest(
            "runtime-v1",
            "code-v1",
            "darwin-aarch64",
            "runtime-sha",
            &sha256_bytes(&bytes),
        );
        let runtime_root = tmp.path().join("runtime");

        let result = import_runtime_bundle_from_zip_path(
            &zip_path,
            &manifest,
            &runtime_root,
            "darwin-aarch64",
        )
        .expect("import should work");

        assert_eq!(result.installed_kind, "code");
        assert_eq!(result.installed_version, "code-v1");
        assert!(runtime_code_path(&runtime_root, "code-v1")
            .join("mash_cv")
            .join("cv.py")
            .is_file());
        let record = read_code_version(&runtime_root).unwrap();
        assert_eq!(record.version, "code-v1");
        assert_eq!(record.platform, "darwin-aarch64");
    }

    #[test]
    fn preferred_cn_name_prefers_server_alias() {
        let renamed = serde_json::json!({
            "name_cn": "美杜莎",
            "name_cn_server": "歌果",
        });
        assert_eq!(preferred_cn_name(&renamed).as_deref(), Some("歌果"));

        let blank_alias = serde_json::json!({
            "name_cn": "美杜莎",
            "name_cn_server": "   ",
        });
        assert_eq!(preferred_cn_name(&blank_alias).as_deref(), Some("美杜莎"));
    }

    #[test]
    fn first_np_name_prefers_server_cn_then_cn_then_legacy_name() {
        let server = serde_json::json!({
            "noble_phantasms": [{
                "name_cn_server": "国服宝具名",
                "name_cn": "普通中文宝具名",
                "name": "Legacy"
            }]
        });
        assert_eq!(first_np_name(&server).as_deref(), Some("国服宝具名"));

        let flat = serde_json::json!({
            "noble_phantasms": [{ "name_cn": "流星一条", "name": "Stella" }]
        });
        assert_eq!(first_np_name(&flat).as_deref(), Some("流星一条"));

        let fallback = serde_json::json!({
            "noble_phantasms": [{ "name": "Stella" }]
        });
        assert_eq!(first_np_name(&fallback).as_deref(), Some("Stella"));
    }

    #[test]
    fn servants_data_expands_variants_with_last_cn_np_and_face_id() {
        let mash_variants: Vec<&ServantInfo> =
            servants_data().iter().filter(|s| s.id == 1).collect();
        assert_eq!(mash_variants.len(), 3);
        assert_eq!(mash_variants[0].variant_key, "1:1");
        assert_eq!(mash_variants[0].face_id, Some(800170));
        assert_eq!(
            mash_variants[0].noble_phantasm_name.as_deref(),
            Some("已然遥远的理想之城")
        );
        assert_eq!(mash_variants[1].face_id, Some(800151));
        assert_eq!(
            mash_variants[1].noble_phantasm_name.as_deref(),
            Some("依然存在的梦想之城")
        );
    }

    // --- pick_ce_card_in -----------------------------------------------

    #[test]
    fn pick_ce_card_in_returns_none_when_directory_missing() {
        let tmp = tempfile::tempdir().unwrap();
        // No `123/` subdirectory ever created.
        assert!(pick_ce_card_in(tmp.path(), 123).is_none());
    }

    #[test]
    fn pick_ce_card_in_returns_none_when_only_metadata_present() {
        // CE directories may exist with only `craft-essence.json` but
        // no `card_ce.png` if the asset hasn't been pulled yet — the
        // resolver must report `None` in that case so the frontend
        // renders the gray placeholder rather than a broken `<img>`.
        let tmp = tempfile::tempdir().unwrap();
        let ce_dir = tmp.path().join("42");
        fs::create_dir_all(&ce_dir).unwrap();
        fs::write(ce_dir.join("craft-essence.json"), b"{}").unwrap();
        assert!(pick_ce_card_in(tmp.path(), 42).is_none());
    }

    #[test]
    fn pick_ce_card_in_returns_path_when_file_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let ce_dir = tmp.path().join("42");
        fs::create_dir_all(&ce_dir).unwrap();
        let card = ce_dir.join("card_ce.png");
        fs::write(&card, b"fake png bytes").unwrap();
        let picked = pick_ce_card_in(tmp.path(), 42).expect("expected a match");
        assert_eq!(picked, card);
    }

    // --- craft_essences_data -------------------------------------------

    #[test]
    fn craft_essences_data_parses_and_has_unique_ids() {
        let ces = craft_essences_data();
        assert!(
            !ces.is_empty(),
            "bundled craft_essences.json parsed to an empty list"
        );

        let mut seen: HashSet<u32> = HashSet::with_capacity(ces.len());
        for ce in ces {
            assert!(!ce.name.is_empty(), "CE id {} has an empty name", ce.id);
            assert!(
                seen.insert(ce.id),
                "duplicate CE id {} in craft_essences.json",
                ce.id
            );
        }
    }

    #[test]
    fn craft_essences_data_uses_collection_no_and_cn_names() {
        let ces = craft_essences_data();
        let ce_2234 = ces.iter().find(|ce| ce.id == 2234).unwrap();
        let ce_2235 = ces.iter().find(|ce| ce.id == 2235).unwrap();
        let ce_2236 = ces.iter().find(|ce| ce.id == 2236).unwrap();
        let ce_2237 = ces.iter().find(|ce| ce.id == 2237).unwrap();

        assert!(
            ces.iter().all(|ce| ce.id < 9000000),
            "CE ids exposed to the app must use collectionNo, not Atlas internal ids"
        );
        assert_eq!(ce_2234.name, "心愿之味");
        assert!(ce_2234.name_aliases.is_empty());
        assert_eq!(ce_2235.name, "龙之山地徒步");
        assert_eq!(ce_2236.name, "悠久的特洛伊");
        assert!(ce_2236.name_aliases.is_empty());
        assert_eq!(ce_2237.name, "去往大海");
        assert!(ce_2237.name_aliases.is_empty());
    }

    #[test]
    fn craft_essences_data_is_memoized_via_oncelock() {
        // OnceLock-backed `&'static [CraftEssenceInfo]` should hand back
        // the exact same slice on repeated calls (same pointer + len).
        // If somebody refactors away the cache, this catches it.
        let a = craft_essences_data();
        let b = craft_essences_data();
        assert_eq!(a.as_ptr(), b.as_ptr());
        assert_eq!(a.len(), b.len());
    }

    // --- Server enum --------------------------------------------------

    #[test]
    fn server_default_is_jp() {
        assert_eq!(Server::default(), Server::Jp);
    }

    #[test]
    fn server_display_and_dir_token_match_serde() {
        for s in [Server::Jp, Server::Cn] {
            // Display + dir_token must agree with the serde tag (the
            // value the frontend sees) so JSON, file paths, and stderr
            // logs all line up.
            let serialized = serde_json::to_string(&s).unwrap();
            assert_eq!(serialized, format!("\"{}\"", s));
            assert_eq!(serialized.to_lowercase().trim_matches('"'), s.dir_token());
        }
    }

    #[test]
    fn server_from_str_round_trips_both_cases() {
        // Persisted settings file stores `"JP"` / `"CN"`; defensive
        // lower / mixed-case parsing keeps a hand-edited file working.
        for (input, expected) in [
            ("JP", Server::Jp),
            ("jp", Server::Jp),
            ("Jp", Server::Jp),
            ("CN", Server::Cn),
            ("cn", Server::Cn),
        ] {
            assert_eq!(Server::from_str(input).unwrap(), expected, "input={input}");
        }
        assert!(Server::from_str("us").is_err());
    }

    #[test]
    fn server_serde_uses_uppercase_tag() {
        let value: Server = serde_json::from_str("\"CN\"").unwrap();
        assert_eq!(value, Server::Cn);
        assert!(serde_json::from_str::<Server>("\"cn\"").is_err());
    }

    // --- Localization indices -----------------------------------------

    #[test]
    fn normalize_jp_key_strips_whitespace_and_nfkc_folds() {
        // Full-width vs. half-width digits should collapse to the
        // same key so JP→CN lookups don't miss on cosmetic
        // differences between mooncell and Atlas dumps.
        assert_eq!(normalize_jp_key("Ｌｖ１"), normalize_jp_key("Lv1"));
        // Stray spaces in the source data must not split the key.
        assert_eq!(
            normalize_jp_key("アルトリア ペンドラゴン"),
            normalize_jp_key("アルトリアペンドラゴン")
        );
    }

    #[test]
    fn servant_jp_to_cn_index_maps_known_servant_name() {
        // Servant 2 = Altria Pendragon — the row exists in the
        // bundled mooncell `servants.json` with both `name_jp` and
        // `name_cn` populated, so the index must produce the CN
        // string.
        let idx = servant_jp_to_cn_index();
        assert_eq!(
            idx.get(&normalize_jp_key("アルトリア・ペンドラゴン"))
                .map(|s| s.as_str()),
            Some("阿尔托莉雅·潘德拉贡")
        );
    }

    #[test]
    fn servant_jp_to_cn_index_prefers_cn_server_alias() {
        let idx = servant_jp_to_cn_index();
        assert_eq!(
            idx.get(&normalize_jp_key("メドゥーサ")).map(|s| s.as_str()),
            Some("歌果")
        );
    }

    #[test]
    fn servants_data_exposes_overwrite_servant_names() {
        let jinako = servants_data().iter().find(|s| s.id == 244).unwrap();
        assert_eq!(jinako.name_cn, "吉娜可·加里吉利");
        assert!(jinako
            .over_write_servant_names
            .iter()
            .any(|alias| alias.name_cn.as_deref() == Some("伟大的石像神")
                && alias.name_jp.as_deref() == Some("大いなる石像神")));
    }

    #[test]
    fn localized_servant_names_include_overwrite_aliases_by_server() {
        let cn_names = localized_servant_names_by_id(244, Server::Cn, "吉娜可·加里吉利");
        assert_eq!(
            cn_names.first().map(|s| s.as_str()),
            Some("吉娜可·加里吉利")
        );
        assert!(cn_names.iter().any(|name| name == "伟大的石像神"));
        assert!(!cn_names.iter().any(|name| name == "大いなる石像神"));

        let jp_names = localized_servant_names_by_id(244, Server::Jp, "ジナコ＝カリギリ");
        assert_eq!(
            jp_names.first().map(|s| s.as_str()),
            Some("ジナコ＝カリギリ")
        );
        assert!(jp_names.iter().any(|name| name == "大いなる石像神"));
        assert!(!jp_names.iter().any(|name| name == "伟大的石像神"));
    }

    #[test]
    fn localized_servant_names_dedupe_primary_alias_overlap() {
        let names = localized_servant_names_by_id(244, Server::Cn, "伟大的石像神");
        assert_eq!(
            names
                .iter()
                .filter(|name| name.as_str() == "伟大的石像神")
                .count(),
            1
        );
    }

    #[test]
    fn servant_id_cn_index_keeps_same_jp_names_distinct() {
        let idx = servant_id_to_cn_index();
        assert_eq!(idx.get(&23).map(|s| s.as_str()), Some("歌果"));
        assert_eq!(idx.get(&384).map(|s| s.as_str()), Some("美杜莎"));
        assert_eq!(localize_servant_name_by_id(384, "メドゥーサ"), "美杜莎");
    }

    #[test]
    fn np_index_handles_both_shapes() {
        // Servant 1 uses the dict-of-variants `noble_phantasms`
        // shape; servant 2 uses the flat-list shape. Both must be
        // discoverable by the same builder.
        let idx = np_jp_to_cn_index();
        // Dict-of-variants entry from servant 1 (variant "初始").
        assert_eq!(
            idx.get(&normalize_jp_key("いまは遙か理想の城"))
                .map(|s| s.as_str()),
            Some("已然遥远的理想之城"),
        );
        // Flat-list entry from servant 2.
        assert_eq!(
            idx.get(&normalize_jp_key("約束された勝利の剣"))
                .map(|s| s.as_str()),
            Some("誓约胜利之剑"),
        );
        // Sanity: index contains a non-trivial number of entries —
        // catches an outright build error in the parser.
        assert!(idx.len() > 100, "NP index too small: {}", idx.len());
    }

    #[test]
    fn np_index_prefers_cn_server_alias_and_keeps_jp_equal_cn() {
        let idx = np_jp_to_cn_index();
        assert_eq!(
            idx.get(&normalize_jp_key("始皇帝")).map(|s| s.as_str()),
            Some("祖政")
        );
        assert_eq!(
            idx.get(&normalize_jp_key("流星一条")).map(|s| s.as_str()),
            Some("流星一条")
        );
    }

    #[test]
    fn np_index_dedupes_repeated_jp_names() {
        // Servant 2 lists the same `name_jp` twice (two NP variants
        // share the same wording). The OnceLock builder uses
        // `entry().or_insert` so the duplicate is discarded; this
        // test pins that contract — if a refactor flips to
        // `insert()`, the second translation would silently overwrite
        // the first, which is harmless here but a footgun for cases
        // where the two CN translations actually disagree.
        let idx = np_jp_to_cn_index();
        let key = normalize_jp_key("約束された勝利の剣");
        assert!(idx.contains_key(&key));
    }

    #[test]
    fn localize_servant_name_falls_back_to_jp_when_unmapped() {
        // A name we know isn't in the index must come back unchanged
        // so OCR has *some* target.
        let unknown = "存在しないサーヴァント";
        assert_eq!(localize_servant_name(unknown), unknown);
    }

    #[test]
    fn localize_np_names_drops_unmapped_and_dedupes() {
        // One known + one unknown -> only the known one survives.
        let mapped = localize_np_names(&[
            "約束された勝利の剣".to_string(),
            "存在しない宝具".to_string(),
        ]);
        assert_eq!(mapped, vec!["誓约胜利之剑".to_string()]);

        // Two inputs that translate to the same CN string -> single
        // output entry (preserves order, dedupes by post-translation
        // string).
        let mapped = localize_np_names(&[
            "約束された勝利の剣".to_string(),
            "約束された勝利の剣".to_string(),
        ]);
        assert_eq!(mapped, vec!["誓约胜利之剑".to_string()]);

        // All-unknown input collapses to empty list, which is the
        // signal `_find_supports` uses to switch into name-only mode.
        let mapped = localize_np_names(&["存在しない宝具".to_string()]);
        assert!(mapped.is_empty());
    }

    // --- Action::CommandSpell + BattleScene serde --------------------------

    #[test]
    fn action_command_spell_serializes_with_camel_case_tag() {
        // The frontend identifies the variant by `type:"commandSpell"`;
        // pin that wire shape so a refactor that drops the
        // `#[serde(rename = "commandSpell")]` attribute breaks loudly.
        let action = Action::CommandSpell {
            id: "cs_1".into(),
            spell: Some("np_release".into()),
            target: Some("servant_2".into()),
        };
        let json = serde_json::to_value(&action).unwrap();
        assert_eq!(json["type"], serde_json::json!("commandSpell"));
        assert_eq!(json["spell"], serde_json::json!("np_release"));
        assert_eq!(json["target"], serde_json::json!("servant_2"));
    }

    #[test]
    fn action_command_spell_target_defaults_to_none_when_missing() {
        // Mirror the equipment-action behaviour: an action stored
        // without a target field still loads (target left null).
        let json = serde_json::json!({
            "type": "commandSpell",
            "id": "cs_1",
            "spell": "restore",
        });
        let action: Action = serde_json::from_value(json).unwrap();
        match action {
            Action::CommandSpell { id, spell, target } => {
                assert_eq!(id, "cs_1");
                assert_eq!(spell.as_deref(), Some("restore"));
                assert!(target.is_none());
            }
            _ => panic!("expected Action::CommandSpell"),
        }
    }

    #[test]
    fn action_equipment_order_change_round_trips() {
        let json = serde_json::json!({
            "type": "equipment",
            "id": "eq_1",
            "skill": "skill_3",
            "target": null,
            "orderChange": {
                "front": "servant_1",
                "back": "servant_4"
            }
        });
        let action: Action = serde_json::from_value(json).unwrap();
        match action {
            Action::Equipment {
                skill,
                target,
                order_change,
                ..
            } => {
                assert_eq!(skill.as_deref(), Some("skill_3"));
                assert!(target.is_none());
                let order_change = order_change.unwrap();
                assert_eq!(order_change.front.as_deref(), Some("servant_1"));
                assert_eq!(order_change.back.as_deref(), Some("servant_4"));
            }
            _ => panic!("expected Action::Equipment"),
        }
    }

    #[test]
    fn battle_scene_legacy_json_merges_old_action_buckets_in_fixed_order() {
        // Pre-ordered-action configs stored three separate preparation
        // buckets. Normalization preserves their historical execution
        // order so old projects keep behaving the same after loading.
        let json = serde_json::json!({
            "id": "scene_1",
            "servantActions": [{
                "type": "servant",
                "id": "sa_1",
                "servant": "servant_1",
                "skill": "skill_1",
                "target": null
            }],
            "equipmentActions": [{
                "type": "equipment",
                "id": "eq_1",
                "skill": "skill_2",
                "target": null
            }],
            "commandSpellActions": [{
                "type": "commandSpell",
                "id": "cs_1",
                "spell": "restore",
                "target": "servant_2"
            }],
            "attackPriority": [],
        });
        let scene: BattleScene = serde_json::from_value::<BattleScene>(json)
            .unwrap()
            .normalize_turns();
        assert_eq!(scene.id, "scene_1");
        assert_eq!(scene.turns.len(), 1);
        let turn = &scene.turns[0];
        assert_eq!(turn.preparation_actions.len(), 3);
        assert!(matches!(
            turn.preparation_actions[0],
            Action::Servant { .. }
        ));
        assert!(matches!(
            turn.preparation_actions[1],
            Action::Equipment { .. }
        ));
        assert!(matches!(
            turn.preparation_actions[2],
            Action::CommandSpell { .. }
        ));
        assert!(scene.servant_actions.is_empty());
        assert!(scene.equipment_actions.is_empty());
        assert!(scene.command_spell_actions.is_empty());
    }

    #[test]
    fn battle_scene_round_trips_preparation_actions_under_camel_case_key() {
        let scene = BattleScene {
            id: "scene_1".into(),
            turns: vec![BattleTurn {
                id: "turn_1".into(),
                preparation_actions: vec![Action::CommandSpell {
                    id: "cs_1".into(),
                    spell: Some("np_release".into()),
                    target: Some("servant_1".into()),
                }],
                servant_actions: vec![],
                equipment_actions: vec![],
                command_spell_actions: vec![],
                enemy_target: None,
                attack_priority: vec![],
            }],
            preparation_actions: vec![],
            servant_actions: vec![],
            equipment_actions: vec![],
            command_spell_actions: vec![],
            enemy_target: None,
            attack_priority: vec![],
        };
        let json = serde_json::to_value(&scene).unwrap();
        assert!(json["turns"][0]["preparationActions"].is_array());
        assert!(json["preparationActions"].is_null());
        assert!(json["commandSpellActions"].is_null());
        assert_eq!(
            json["turns"][0]["preparationActions"][0]["type"],
            serde_json::json!("commandSpell")
        );

        // Deserialize back — symmetry guards against accidentally
        // exposing a field under one name and reading it under another.
        let parsed: BattleScene = serde_json::from_value::<BattleScene>(json)
            .unwrap()
            .normalize_turns();
        assert_eq!(parsed.turns[0].preparation_actions.len(), 1);
        match &parsed.turns[0].preparation_actions[0] {
            Action::CommandSpell { spell, target, .. } => {
                assert_eq!(spell.as_deref(), Some("np_release"));
                assert_eq!(target.as_deref(), Some("servant_1"));
            }
            _ => panic!("expected Action::CommandSpell"),
        }
    }

    #[test]
    fn battle_scene_defaults_missing_enemy_target_to_none() {
        let json = serde_json::json!({
            "id": "scene_1",
            "preparationActions": [],
            "attackPriority": []
        });

        let scene: BattleScene = serde_json::from_value(json).unwrap();

        assert_eq!(scene.id, "scene_1");
        let scene = scene.normalize_turns();
        assert!(scene.turns[0].enemy_target.is_none());
    }

    #[test]
    fn advanced_battle_scene_round_trips_rule_groups_and_actions() {
        let scene = AdvancedBattleScene {
            id: "advanced_scene_1".into(),
            main_output: Some(AdvancedMainOutput {
                servant: Some("servant_1".into()),
                output_type: Some(AdvancedOutputType::Np),
                np_card: Some("arts".into()),
            }),
            grand_auto_order_change: Some(true),
            command_conditions: vec![AdvancedCommandCardCondition {
                slot: 0,
                servant: "servant_1".into(),
                suit: "buster".into(),
                min_crit_chance: Some(80),
            }],
            control_actions: vec![Action::Servant {
                id: "sa_control".into(),
                servant: Some("servant_1".into()),
                skill: Some("skill_1".into()),
                target: None,
            }],
            startup_actions: vec![Action::Equipment {
                id: "eq_start".into(),
                skill: Some("skill_2".into()),
                target: None,
                order_change: None,
            }],
            rules: vec![AdvancedRule {
                id: "rule_1".into(),
                np_condition_groups: vec![AdvancedNpConditionGroup {
                    id: "np_group_1".into(),
                    slots: vec![AdvancedNpSlotCondition {
                        servant: "servant_1".into(),
                        ready: true,
                    }],
                }],
                command_condition_groups: vec![AdvancedCommandConditionGroup {
                    id: "cmd_group_1".into(),
                    cards: vec![AdvancedCommandCardCondition {
                        slot: 0,
                        servant: "servant_1".into(),
                        suit: "buster".into(),
                        min_crit_chance: Some(80),
                    }],
                }],
                actions: vec![
                    AdvancedAction::Servant {
                        id: "sa_1".into(),
                        servant: Some("servant_1".into()),
                        skill: Some("skill_1".into()),
                        target: None,
                    },
                    AdvancedAction::Attack {
                        id: "atk_1".into(),
                        card: Some("servant_1_buster".into()),
                    },
                ],
            }],
        };

        let json = serde_json::to_value(&scene).unwrap();
        assert_eq!(
            json["rules"][0]["npConditionGroups"][0]["slots"][0]["ready"],
            true
        );
        assert_eq!(
            json["rules"][0]["commandConditionGroups"][0]["cards"][0]["minCritChance"],
            serde_json::json!(80)
        );
        assert_eq!(
            json["rules"][0]["actions"][1]["type"],
            serde_json::json!("attack")
        );

        let parsed: AdvancedBattleScene = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.grand_auto_order_change, Some(true));
        assert_eq!(parsed.rules.len(), 1);
        assert_eq!(parsed.rules[0].actions.len(), 2);
    }

    #[test]
    fn screenshot_save_path_keeps_or_adds_png_extension() {
        assert_eq!(
            with_png_extension(PathBuf::from("/tmp/capture.png")),
            PathBuf::from("/tmp/capture.png")
        );
        assert_eq!(
            with_png_extension(PathBuf::from("/tmp/capture.PNG")),
            PathBuf::from("/tmp/capture.PNG")
        );
        assert_eq!(
            with_png_extension(PathBuf::from("/tmp/capture")),
            PathBuf::from("/tmp/capture.png")
        );
        assert_eq!(
            with_png_extension(PathBuf::from("/tmp/capture.jpg")),
            PathBuf::from("/tmp/capture.png")
        );
    }
}
