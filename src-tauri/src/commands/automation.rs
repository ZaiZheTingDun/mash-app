//! Automation IPC commands and startup plumbing.
//!
//! Battle and enhancement automation share the same startup shape: connect ADB,
//! start or reuse the CV sidecar, validate stream resolution, normalize touch
//! coordinates, and then hand control to the appropriate runner. This module
//! keeps that orchestration out of the Tauri builder module while preserving the
//! existing command names and event payloads.

use crate::adb;
use crate::commands::catalog::load_enhancement_target;
use crate::commands::debug;
use crate::commands::projects::{load_advanced_battle_scenes, load_battle_scenes, read_projects};
use crate::commands::runtime::{
    resolve_ce_assets_dir, resolve_scrcpy_jar, resolve_servant_assets_dir,
};
use crate::commands::settings::RecognitionSettings;
use crate::enhancement_runner::{
    server_supported as enhancement_server_supported, EnhancementAutomationEvent,
    EnhancementConfig, EnhancementRunner, EnhancementRunnerHandle, EnhancementRunnerState,
};
use crate::models::ProjectRecognitionSettings;
use crate::runner::{AutomationEvent, LogLevel, RunConfig, Runner, RunnerHandle, RunnerState};
use crate::screen;
use crate::server::{
    stream_meets_minimum_resolution, stream_resolution_error, Server, STREAM_BIT_RATE,
    STREAM_MAX_SIZE,
};
use crate::{resolve_cv_config_paths, resolve_template_dirs};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use tauri::Emitter;

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

pub(crate) fn effective_recognition_settings(
    global: RecognitionSettings,
    project: Option<ProjectRecognitionSettings>,
) -> RecognitionSettings {
    RecognitionSettings {
        noble_phantasm_detection_mode: global.noble_phantasm_detection_mode,
        support_ce_threshold: project
            .and_then(|settings| settings.support_ce_threshold)
            .unwrap_or(global.support_ce_threshold),
        support_ce_full_gate_threshold: project
            .and_then(|settings| settings.support_ce_full_gate_threshold)
            .unwrap_or(global.support_ce_full_gate_threshold),
        support_mlb_icon_threshold: project
            .and_then(|settings| settings.support_mlb_icon_threshold)
            .unwrap_or(global.support_mlb_icon_threshold),
        support_bond_icon_threshold: project
            .and_then(|settings| settings.support_bond_icon_threshold)
            .unwrap_or(global.support_bond_icon_threshold),
    }
}

#[tauri::command]
pub(crate) fn start_automation(
    app: tauri::AppHandle,
    mut config: RunConfig,
    bluestack_state: tauri::State<'_, Mutex<bool>>,
    server_state: tauri::State<'_, Mutex<Server>>,
    recognition_settings_state: tauri::State<'_, Mutex<RecognitionSettings>>,
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
    let recognition_settings = effective_recognition_settings(
        *recognition_settings_state.lock().unwrap(),
        project
            .as_ref()
            .and_then(|project| project.recognition_settings),
    );
    config.support_ce_threshold = recognition_settings.support_ce_threshold;
    config.support_ce_full_gate_threshold = recognition_settings.support_ce_full_gate_threshold;
    config.support_mlb_icon_threshold = recognition_settings.support_mlb_icon_threshold;
    config.support_bond_icon_threshold = recognition_settings.support_bond_icon_threshold;
    config.noble_phantasm_detection_mode = recognition_settings.noble_phantasm_detection_mode;

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
        let runner = Runner::new(
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
pub(crate) fn stop_automation(
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
) -> Result<(), String> {
    let handle = handle_state.lock().unwrap();
    handle.cancel.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
pub(crate) fn stop_automation_after_current(
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
) -> Result<(), String> {
    let handle = handle_state.lock().unwrap();
    handle.stop_after_current.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
pub(crate) fn get_automation_status(
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
) -> RunnerState {
    let handle = handle_state.lock().unwrap();
    let state = handle.state.lock().unwrap().clone();
    state
}

#[tauri::command]
pub(crate) fn start_enhancement_automation(
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
pub(crate) fn stop_enhancement_automation(
    handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
) -> Result<(), String> {
    let handle = handle_state.lock().unwrap();
    handle.cancel.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
pub(crate) fn get_enhancement_automation_status(
    handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
) -> EnhancementRunnerState {
    let handle = handle_state.lock().unwrap();
    let state = handle.state.lock().unwrap().clone();
    state
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
