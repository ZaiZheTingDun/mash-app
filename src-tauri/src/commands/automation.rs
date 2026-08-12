//! Automation IPC commands and startup plumbing.
//!
//! Battle and enhancement automation share the same startup shape: connect ADB,
//! start or reuse the CV sidecar, validate stream resolution, normalize touch
//! coordinates, and then hand control to the appropriate runner. This module
//! keeps that orchestration out of the Tauri builder module while preserving the
//! existing command names and event payloads.

use crate::adb;
use crate::battle_statistics::BattleRunRecorder;
use crate::commands::catalog::load_enhancement_target;
use crate::commands::debug;
use crate::commands::projects::{load_advanced_battle_scenes, load_battle_scenes, read_projects};
use crate::commands::runtime::{
    resolve_ce_assets_dir, resolve_scrcpy_jar, resolve_servant_assets_dir,
};
use crate::commands::settings::{AdbDeviceSettings, DebugSettings, RecognitionSettings};
use crate::craft_essence_enhancement_runner::{
    lifecycle_transition as ce_lifecycle_transition, server_supported as ce_server_supported,
    CraftEssenceEnhancementAutomationEvent, CraftEssenceEnhancementMode,
    CraftEssenceEnhancementRunner, CraftEssenceEnhancementRunnerHandle,
    CraftEssenceEnhancementRunnerState, LifecycleEvent as CeLifecycleEvent,
    EVENT_NAME as CE_EVENT_NAME,
};
use crate::enhancement_runner::{
    enhancement_lifecycle_transition, server_supported as enhancement_server_supported,
    EnhancementAutomationEvent, EnhancementConfig, EnhancementLifecycleEvent, EnhancementRunner,
    EnhancementRunnerHandle, EnhancementRunnerState,
};
use crate::friend_point_summon_runner::{
    lifecycle_transition as friend_point_summon_lifecycle_transition,
    server_supported as friend_point_summon_server_supported, FriendPointSummonAutomationEvent,
    FriendPointSummonRunner, FriendPointSummonRunnerHandle, FriendPointSummonRunnerState,
    LifecycleEvent as FriendPointSummonLifecycleEvent,
    EVENT_NAME as FRIEND_POINT_SUMMON_EVENT_NAME,
};
use crate::models::ProjectRecognitionSettings;
use crate::runner::{
    grand_strategy, runner_lifecycle_transition, AutomationEvent, LogLevel, RunConfig, Runner,
    RunnerHandle, RunnerLifecycleEvent, RunnerState,
};
use crate::screen;
use crate::server::{
    stream_meets_minimum_resolution, stream_resolution_error, Server, STREAM_BIT_RATE,
    STREAM_MAX_FPS, STREAM_MAX_SIZE,
};
use crate::{resolve_cv_config_paths, resolve_template_dirs};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use tauri::Emitter;

struct BattleRunFinishGuard {
    recorder: Option<BattleRunRecorder>,
    state: Arc<Mutex<RunnerState>>,
}

impl Drop for BattleRunFinishGuard {
    fn drop(&mut self) {
        let Some(recorder) = &self.recorder else {
            return;
        };
        recorder.finish(&self.state.lock().unwrap());
    }
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

fn stream_mapping_debug_message(
    scope: &str,
    input_size: (u32, u32),
    stream_size: (u32, u32),
) -> Option<String> {
    (input_size != stream_size).then(|| {
        format!(
            "[{scope}] using adb input size {}x{} with stream frame {}x{}",
            input_size.0, input_size.1, stream_size.0, stream_size.1
        )
    })
}

fn emit_stream_mapping_debug(
    app: &tauri::AppHandle,
    scope: &str,
    input_size: (u32, u32),
    stream_size: (u32, u32),
) {
    let Some(message) = stream_mapping_debug_message(scope, input_size, stream_size) else {
        return;
    };
    eprintln!("{message}");
    crate::operation_log::emit_debug(app, message);
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

fn ce_enhancement_runner_is_busy(state: &CraftEssenceEnhancementRunnerState) -> bool {
    matches!(
        state,
        CraftEssenceEnhancementRunnerState::Starting | CraftEssenceEnhancementRunnerState::Running
    )
}

fn friend_point_summon_runner_is_busy(state: &FriendPointSummonRunnerState) -> bool {
    matches!(
        state,
        FriendPointSummonRunnerState::Starting | FriendPointSummonRunnerState::Running
    )
}

const ADB_RESET_USER_MESSAGE: &str =
    "ADB 设备离线，请点击状态栏的游戏连接按钮，并选择「重置 ADB」后重试。";

fn scrcpy_stream_start_user_message(error: &str) -> Option<&'static str> {
    if error.contains("device offline") || error.contains("failed to get feature set") {
        Some(ADB_RESET_USER_MESSAGE)
    } else {
        None
    }
}

fn emit_automation_status(
    app: &tauri::AppHandle,
    state: &Arc<Mutex<RunnerState>>,
    screen: &str,
    message: &str,
) {
    emit_automation_status_with_level(app, state, screen, message, LogLevel::Info);
}

fn emit_automation_status_with_level(
    app: &tauri::AppHandle,
    state: &Arc<Mutex<RunnerState>>,
    screen: &str,
    message: &str,
    level: LogLevel,
) {
    let (state_str, status) = {
        let s = state.lock().unwrap();
        (format!("{:?}", *s), s.status())
    };
    let _ = app.emit(
        "automation-status",
        AutomationEvent {
            state: state_str,
            status,
            current_screen: screen.into(),
            message: message.into(),
            level,
            attack: None,
            action: None,
        },
    );
}

fn apply_runner_lifecycle_event(state: &Arc<Mutex<RunnerState>>, event: RunnerLifecycleEvent) {
    let mut guard = state.lock().unwrap();
    let transition = runner_lifecycle_transition(guard.clone(), event);
    if transition.accepted {
        *guard = transition.next;
    }
}

fn apply_enhancement_lifecycle_event(
    state: &Arc<Mutex<EnhancementRunnerState>>,
    event: EnhancementLifecycleEvent,
) {
    let mut guard = state.lock().unwrap();
    let transition = enhancement_lifecycle_transition(guard.clone(), event);
    if transition.accepted {
        *guard = transition.next;
    }
}

fn fail_automation_start(app: &tauri::AppHandle, state: &Arc<Mutex<RunnerState>>, message: String) {
    apply_runner_lifecycle_event(
        state,
        RunnerLifecycleEvent::Failed {
            message: message.clone(),
        },
    );
    emit_automation_status(app, state, "", &format!("启动失败: {message}"));
}

fn fail_automation_start_with_debug(
    app: &tauri::AppHandle,
    state: &Arc<Mutex<RunnerState>>,
    user_message: String,
    debug_message: String,
) {
    apply_runner_lifecycle_event(
        state,
        RunnerLifecycleEvent::Failed {
            message: user_message.clone(),
        },
    );
    emit_automation_status_with_level(app, state, "CV", &debug_message, LogLevel::Debug);
    emit_automation_status(app, state, "", &format!("启动失败: {user_message}"));
}

fn stop_automation_start(app: &tauri::AppHandle, state: &Arc<Mutex<RunnerState>>) {
    apply_runner_lifecycle_event(state, RunnerLifecycleEvent::StopRequested);
    emit_automation_status(app, state, "", "自动化已停止");
}

fn emit_enhancement_status(
    app: &tauri::AppHandle,
    state: &Arc<Mutex<EnhancementRunnerState>>,
    screen: &str,
    message: &str,
) {
    emit_enhancement_status_with_level(app, state, screen, message, LogLevel::Info);
}

fn emit_enhancement_status_with_level(
    app: &tauri::AppHandle,
    state: &Arc<Mutex<EnhancementRunnerState>>,
    screen: &str,
    message: &str,
    level: LogLevel,
) {
    let (state_str, status) = {
        let s = state.lock().unwrap();
        (format!("{:?}", *s), s.status())
    };
    let _ = app.emit(
        "enhancement-automation-status",
        EnhancementAutomationEvent {
            state: state_str,
            status,
            current_screen: screen.into(),
            message: message.into(),
            level,
        },
    );
}

fn fail_enhancement_start(
    app: &tauri::AppHandle,
    state: &Arc<Mutex<EnhancementRunnerState>>,
    message: String,
) {
    apply_enhancement_lifecycle_event(
        state,
        EnhancementLifecycleEvent::Failed {
            message: message.clone(),
        },
    );
    emit_enhancement_status(app, state, "", &format!("启动失败: {message}"));
}

fn fail_enhancement_start_with_debug(
    app: &tauri::AppHandle,
    state: &Arc<Mutex<EnhancementRunnerState>>,
    user_message: String,
    debug_message: String,
) {
    apply_enhancement_lifecycle_event(
        state,
        EnhancementLifecycleEvent::Failed {
            message: user_message.clone(),
        },
    );
    emit_enhancement_status_with_level(app, state, "CV", &debug_message, LogLevel::Debug);
    emit_enhancement_status(app, state, "", &format!("启动失败: {user_message}"));
}

fn stop_enhancement_start(app: &tauri::AppHandle, state: &Arc<Mutex<EnhancementRunnerState>>) {
    apply_enhancement_lifecycle_event(state, EnhancementLifecycleEvent::StopRequested);
    emit_enhancement_status(app, state, "", "强化自动化已停止");
}

fn apply_ce_lifecycle_event(
    state: &Arc<Mutex<CraftEssenceEnhancementRunnerState>>,
    event: CeLifecycleEvent,
) {
    let mut guard = state.lock().unwrap();
    *guard = ce_lifecycle_transition(guard.clone(), event);
}

fn emit_ce_enhancement_status(
    app: &tauri::AppHandle,
    state: &Arc<Mutex<CraftEssenceEnhancementRunnerState>>,
    screen: &str,
    message: &str,
) {
    let (state_str, status) = {
        let state = state.lock().unwrap();
        (format!("{:?}", *state), state.status())
    };
    let _ = app.emit(
        CE_EVENT_NAME,
        CraftEssenceEnhancementAutomationEvent {
            state: state_str,
            status,
            current_screen: screen.into(),
            message: message.into(),
            level: LogLevel::Info,
        },
    );
}

fn fail_ce_enhancement_start(
    app: &tauri::AppHandle,
    state: &Arc<Mutex<CraftEssenceEnhancementRunnerState>>,
    message: String,
) {
    apply_ce_lifecycle_event(
        state,
        CeLifecycleEvent::Failed {
            message: message.clone(),
        },
    );
    emit_ce_enhancement_status(app, state, "", &format!("启动失败: {message}"));
}

fn stop_ce_enhancement_start(
    app: &tauri::AppHandle,
    state: &Arc<Mutex<CraftEssenceEnhancementRunnerState>>,
) {
    apply_ce_lifecycle_event(state, CeLifecycleEvent::StopRequested);
    emit_ce_enhancement_status(app, state, "", "概念礼装强化自动化已停止");
}

fn apply_friend_point_summon_lifecycle_event(
    state: &Arc<Mutex<FriendPointSummonRunnerState>>,
    event: FriendPointSummonLifecycleEvent,
) {
    let mut guard = state.lock().unwrap();
    *guard = friend_point_summon_lifecycle_transition(guard.clone(), event);
}

fn emit_friend_point_summon_status(
    app: &tauri::AppHandle,
    state: &Arc<Mutex<FriendPointSummonRunnerState>>,
    screen: &str,
    message: &str,
) {
    let (state_str, status) = {
        let state = state.lock().unwrap();
        (format!("{:?}", *state), state.status())
    };
    let _ = app.emit(
        FRIEND_POINT_SUMMON_EVENT_NAME,
        FriendPointSummonAutomationEvent {
            state: state_str,
            status,
            current_screen: screen.into(),
            message: message.into(),
            level: LogLevel::Info,
            completed_batches: 0,
            summoned_count: 0,
        },
    );
}

fn fail_friend_point_summon_start(
    app: &tauri::AppHandle,
    state: &Arc<Mutex<FriendPointSummonRunnerState>>,
    message: String,
) {
    apply_friend_point_summon_lifecycle_event(
        state,
        FriendPointSummonLifecycleEvent::Failed {
            message: message.clone(),
        },
    );
    emit_friend_point_summon_status(app, state, "", &format!("启动失败: {message}"));
}

fn stop_friend_point_summon_start(
    app: &tauri::AppHandle,
    state: &Arc<Mutex<FriendPointSummonRunnerState>>,
) {
    apply_friend_point_summon_lifecycle_event(
        state,
        FriendPointSummonLifecycleEvent::StopRequested,
    );
    emit_friend_point_summon_status(app, state, "", "友情点抽取自动化已停止");
}

pub(crate) fn effective_recognition_settings(
    global: RecognitionSettings,
    project: Option<ProjectRecognitionSettings>,
) -> RecognitionSettings {
    let project_verify_skill_activation =
        project.and_then(|settings| settings.verify_skill_activation);
    let project_enable_extra_class_filter =
        project.and_then(|settings| settings.enable_extra_class_filter);
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
        stop_on_bond_level_up: global.stop_on_bond_level_up && !global.stop_on_bond_max_level,
        stop_on_bond_max_level: global.stop_on_bond_max_level,
        auto_capture_bond_level_up: global.auto_capture_bond_level_up,
        verify_skill_activation: project_verify_skill_activation
            .unwrap_or(global.verify_skill_activation),
        enable_extra_class_filter: project_enable_extra_class_filter
            .unwrap_or(global.enable_extra_class_filter),
        support_full_list_ocr_fallback: global.support_full_list_ocr_fallback,
        unknown_screen_timeout_count: global.unknown_screen_timeout_count,
    }
}

#[tauri::command]
pub(crate) fn start_automation(
    app: tauri::AppHandle,
    mut config: RunConfig,
    adb_settings_state: tauri::State<'_, Mutex<AdbDeviceSettings>>,
    server_state: tauri::State<'_, Mutex<Server>>,
    recognition_settings_state: tauri::State<'_, Mutex<RecognitionSettings>>,
    debug_settings_state: tauri::State<'_, Mutex<DebugSettings>>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    ce_enhancement_handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    friend_point_summon_handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
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
    {
        let handle = ce_enhancement_handle_state.lock().unwrap();
        if ce_enhancement_runner_is_busy(&handle.state.lock().unwrap()) {
            return Err("概念礼装强化自动化正在运行中".into());
        }
    }
    {
        let handle = friend_point_summon_handle_state.lock().unwrap();
        if friend_point_summon_runner_is_busy(&handle.state.lock().unwrap()) {
            return Err("友情点抽取自动化正在运行中".into());
        }
    }

    let project = read_projects(&app)
        .into_iter()
        .find(|project| project.id == config.project_id);
    let advanced_mode = project
        .as_ref()
        .map(|project| project.advanced_mode)
        .unwrap_or(false);
    if advanced_mode {
        let strategy = grand_strategy(config.grand_class);
        strategy.normalize_servants(&mut config.grand_servants);
        strategy.validate_servants(&config.grand_servants)?;
    }
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

    let selected_adb_serial = adb_settings_state
        .lock()
        .unwrap()
        .selected_adb_serial
        .clone();
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
    config.stop_on_bond_level_up = recognition_settings.stop_on_bond_level_up;
    config.stop_on_bond_max_level = recognition_settings.stop_on_bond_max_level;
    config.auto_capture_bond_level_up = recognition_settings.auto_capture_bond_level_up;
    config.verify_skill_activation = recognition_settings.verify_skill_activation;
    config.enable_extra_class_filter = recognition_settings.enable_extra_class_filter;
    config.support_full_list_ocr_fallback = recognition_settings.support_full_list_ocr_fallback;
    config.unknown_screen_timeout_count = recognition_settings.unknown_screen_timeout_count;
    let debug_settings = *debug_settings_state.lock().unwrap();
    config.auto_capture_battle_result_loot = debug_settings.auto_capture_battle_result_loot;
    config.auto_capture_unknown_screen_timeout = debug_settings.auto_capture_unknown_screen_timeout;
    config.auto_capture_skill_use_probe = debug_settings.auto_capture_skill_use_probe;

    let state = Arc::new(Mutex::new(RunnerState::Starting));
    let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop_after_current = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let run_recorder = match BattleRunRecorder::start(&app, config.max_mission_runs) {
        Ok(recorder) => Some(recorder),
        Err(err) => {
            eprintln!("[battle-statistics] {err}");
            None
        }
    };

    {
        let mut handle = handle_state.lock().unwrap();
        handle.state = state.clone();
        handle.cancel = cancel.clone();
        handle.stop_after_current = stop_after_current.clone();
    }

    let debug_sidecar = debug_state.0.clone();
    std::thread::spawn(move || {
        let _run_finish_guard = BattleRunFinishGuard {
            recorder: run_recorder.clone(),
            state: state.clone(),
        };
        emit_automation_status(&app, &state, "", "正在连接 ADB…");
        let mut adb_dev = adb::Adb::new(&app, selected_adb_serial);
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
                if let Some(user_message) = screen::sidecar_startup_user_message(&err) {
                    fail_automation_start_with_debug(
                        &app,
                        &state,
                        user_message.to_string(),
                        format!("CV 运行时启动失败详情: {err}"),
                    );
                } else {
                    fail_automation_start(&app, &state, err);
                }
                return;
            }
        };

        let (w, h) = match sidecar.start_stream(
            adb_dev.path(),
            &jar_path,
            serial.as_deref(),
            STREAM_MAX_SIZE,
            STREAM_BIT_RATE,
            STREAM_MAX_FPS,
        ) {
            Ok(size) => size,
            Err(err) => {
                if let Some(user_message) = scrcpy_stream_start_user_message(&err) {
                    fail_automation_start_with_debug(
                        &app,
                        &state,
                        user_message.to_string(),
                        format!("scrcpy 视频流启动失败详情: {err}"),
                    );
                } else {
                    fail_automation_start(&app, &state, format!("启动 scrcpy 视频流失败: {err}"));
                }
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
        emit_stream_mapping_debug(&app, "runner", input_size, (w, h));
        let screen_size = Some(input_size);
        let frame_size = Some((w, h));
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
            frame_size,
            assets_dir,
            ce_assets_dir,
            server,
            Some(debug_sidecar),
            run_recorder,
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
    adb_settings_state: tauri::State<'_, Mutex<AdbDeviceSettings>>,
    server_state: tauri::State<'_, Mutex<Server>>,
    battle_handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    ce_enhancement_handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    friend_point_summon_handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
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
    {
        let handle = ce_enhancement_handle_state.lock().unwrap();
        if ce_enhancement_runner_is_busy(&handle.state.lock().unwrap()) {
            return Err("概念礼装强化自动化正在运行中，请先停止".into());
        }
    }
    {
        let handle = friend_point_summon_handle_state.lock().unwrap();
        if friend_point_summon_runner_is_busy(&handle.state.lock().unwrap()) {
            return Err("友情点抽取自动化正在运行中，请先停止".into());
        }
    }

    let selected_adb_serial = adb_settings_state
        .lock()
        .unwrap()
        .selected_adb_serial
        .clone();
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
        let mut adb_dev = adb::Adb::new(&app, selected_adb_serial);
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
                if let Some(user_message) = screen::sidecar_startup_user_message(&err) {
                    fail_enhancement_start_with_debug(
                        &app,
                        &state,
                        user_message.to_string(),
                        format!("CV 运行时启动失败详情: {err}"),
                    );
                } else {
                    fail_enhancement_start(&app, &state, err);
                }
                return;
            }
        };
        let (w, h) = match sidecar.start_stream(
            adb_dev.path(),
            &jar_path,
            serial.as_deref(),
            STREAM_MAX_SIZE,
            STREAM_BIT_RATE,
            STREAM_MAX_FPS,
        ) {
            Ok(size) => size,
            Err(err) => {
                if let Some(user_message) = scrcpy_stream_start_user_message(&err) {
                    fail_enhancement_start_with_debug(
                        &app,
                        &state,
                        user_message.to_string(),
                        format!("scrcpy 视频流启动失败详情: {err}"),
                    );
                } else {
                    fail_enhancement_start(&app, &state, format!("启动 scrcpy 视频流失败: {err}"));
                }
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
        emit_stream_mapping_debug(&app, "enhancement", input_size, (w, h));

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

#[tauri::command]
pub(crate) fn start_craft_essence_enhancement_automation(
    app: tauri::AppHandle,
    adb_settings_state: tauri::State<'_, Mutex<AdbDeviceSettings>>,
    server_state: tauri::State<'_, Mutex<Server>>,
    battle_handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    servant_enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    friend_point_summon_handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
    debug_state: tauri::State<'_, debug::DebugSidecar>,
    mode: Option<CraftEssenceEnhancementMode>,
) -> Result<(), String> {
    {
        let handle = handle_state.lock().unwrap();
        if ce_enhancement_runner_is_busy(&handle.state.lock().unwrap()) {
            return Err("概念礼装强化自动化正在运行中".into());
        }
    }
    {
        let handle = battle_handle_state.lock().unwrap();
        if runner_is_busy(&handle.state.lock().unwrap()) {
            return Err("战斗自动化正在运行中，请先停止".into());
        }
    }
    {
        let handle = servant_enhancement_handle_state.lock().unwrap();
        if enhancement_runner_is_busy(&handle.state.lock().unwrap()) {
            return Err("从者强化自动化正在运行中，请先停止".into());
        }
    }
    {
        let handle = friend_point_summon_handle_state.lock().unwrap();
        if friend_point_summon_runner_is_busy(&handle.state.lock().unwrap()) {
            return Err("友情点抽取自动化正在运行中，请先停止".into());
        }
    }

    let server = *server_state.lock().unwrap();
    if !ce_server_supported(server) {
        return Err("当前仅支持国服概念礼装强化自动化".into());
    }
    let selected_adb_serial = adb_settings_state
        .lock()
        .unwrap()
        .selected_adb_serial
        .clone();
    let state = Arc::new(Mutex::new(CraftEssenceEnhancementRunnerState::Starting));
    let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let mut handle = handle_state.lock().unwrap();
        handle.state = state.clone();
        handle.cancel = cancel.clone();
    }

    let debug_sidecar = debug_state.0.clone();
    let mode = mode.unwrap_or_default();
    std::thread::spawn(move || {
        emit_ce_enhancement_status(&app, &state, "", "正在连接 ADB…");
        let mut adb_dev = adb::Adb::new(&app, selected_adb_serial);
        if let Err(err) = adb_dev.connect() {
            fail_ce_enhancement_start(&app, &state, err);
            return;
        }
        if cancel.load(Ordering::Relaxed) {
            stop_ce_enhancement_start(&app, &state);
            return;
        }
        let serial = adb_dev.serial().map(str::to_string);
        let Some(jar_path) = resolve_scrcpy_jar(&app) else {
            fail_ce_enhancement_start(&app, &state, "找不到 scrcpy-server.jar 资源".into());
            return;
        };
        if !jar_path.exists() {
            fail_ce_enhancement_start(
                &app,
                &state,
                format!("scrcpy-server.jar 不存在: {}", jar_path.display()),
            );
            return;
        }

        emit_ce_enhancement_status(&app, &state, "", "正在启动视频流…");
        let debug_state = debug::DebugSidecar(debug_sidecar.clone());
        let mut sidecar = match take_or_spawn_sidecar(&app, &debug_state, server) {
            Ok(sidecar) => sidecar,
            Err(err) => {
                fail_ce_enhancement_start(&app, &state, err);
                return;
            }
        };
        let (w, h) = match sidecar.start_stream(
            adb_dev.path(),
            &jar_path,
            serial.as_deref(),
            STREAM_MAX_SIZE,
            STREAM_BIT_RATE,
            STREAM_MAX_FPS,
        ) {
            Ok(size) => size,
            Err(err) => {
                fail_ce_enhancement_start(&app, &state, format!("启动 scrcpy 视频流失败: {err}"));
                return;
            }
        };
        if !stream_meets_minimum_resolution(w, h) {
            let _ = sidecar.stop_stream();
            fail_ce_enhancement_start(&app, &state, stream_resolution_error(w, h));
            return;
        }
        let input_size = input_size_for_taps(adb_dev.screen_size(), (w, h));
        emit_stream_mapping_debug(&app, "ce-enhancement", input_size, (w, h));
        let runner = CraftEssenceEnhancementRunner::new(
            adb_dev,
            sidecar,
            app,
            state,
            cancel,
            input_size,
            Some(debug_sidecar),
            mode,
        );
        runner.run();
    });
    Ok(())
}

#[tauri::command]
pub(crate) fn stop_craft_essence_enhancement_automation(
    handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
) -> Result<(), String> {
    let handle = handle_state.lock().unwrap();
    handle.cancel.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
pub(crate) fn get_craft_essence_enhancement_automation_status(
    handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
) -> CraftEssenceEnhancementRunnerState {
    let handle = handle_state.lock().unwrap();
    let state = handle.state.lock().unwrap().clone();
    state
}

#[tauri::command]
pub(crate) fn start_friend_point_summon_automation(
    app: tauri::AppHandle,
    adb_settings_state: tauri::State<'_, Mutex<AdbDeviceSettings>>,
    server_state: tauri::State<'_, Mutex<Server>>,
    battle_handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    servant_enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    ce_enhancement_handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
    debug_state: tauri::State<'_, debug::DebugSidecar>,
) -> Result<(), String> {
    {
        let handle = handle_state.lock().unwrap();
        if friend_point_summon_runner_is_busy(&handle.state.lock().unwrap()) {
            return Err("友情点抽取自动化正在运行中".into());
        }
    }
    {
        let handle = battle_handle_state.lock().unwrap();
        if runner_is_busy(&handle.state.lock().unwrap()) {
            return Err("战斗自动化正在运行中，请先停止".into());
        }
    }
    {
        let handle = servant_enhancement_handle_state.lock().unwrap();
        if enhancement_runner_is_busy(&handle.state.lock().unwrap()) {
            return Err("从者强化自动化正在运行中，请先停止".into());
        }
    }
    {
        let handle = ce_enhancement_handle_state.lock().unwrap();
        if ce_enhancement_runner_is_busy(&handle.state.lock().unwrap()) {
            return Err("概念礼装强化自动化正在运行中，请先停止".into());
        }
    }

    let server = *server_state.lock().unwrap();
    if !friend_point_summon_server_supported(server) {
        return Err("当前仅支持国服友情点抽取自动化".into());
    }
    let selected_adb_serial = adb_settings_state
        .lock()
        .unwrap()
        .selected_adb_serial
        .clone();
    let state = Arc::new(Mutex::new(FriendPointSummonRunnerState::Starting));
    let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let mut handle = handle_state.lock().unwrap();
        handle.state = state.clone();
        handle.cancel = cancel.clone();
    }

    let debug_sidecar = debug_state.0.clone();
    std::thread::spawn(move || {
        emit_friend_point_summon_status(&app, &state, "", "正在连接 ADB…");
        let mut adb_dev = adb::Adb::new(&app, selected_adb_serial);
        if let Err(error) = adb_dev.connect() {
            fail_friend_point_summon_start(&app, &state, error);
            return;
        }
        if cancel.load(Ordering::Relaxed) {
            stop_friend_point_summon_start(&app, &state);
            return;
        }
        let serial = adb_dev.serial().map(str::to_string);
        let Some(jar_path) = resolve_scrcpy_jar(&app) else {
            fail_friend_point_summon_start(&app, &state, "找不到 scrcpy-server.jar 资源".into());
            return;
        };
        if !jar_path.exists() {
            fail_friend_point_summon_start(
                &app,
                &state,
                format!("scrcpy-server.jar 不存在: {}", jar_path.display()),
            );
            return;
        }

        emit_friend_point_summon_status(&app, &state, "", "正在启动视频流…");
        let debug_state = debug::DebugSidecar(debug_sidecar.clone());
        let mut sidecar = match take_or_spawn_sidecar(&app, &debug_state, server) {
            Ok(sidecar) => sidecar,
            Err(error) => {
                fail_friend_point_summon_start(&app, &state, error);
                return;
            }
        };
        let (w, h) = match sidecar.start_stream(
            adb_dev.path(),
            &jar_path,
            serial.as_deref(),
            STREAM_MAX_SIZE,
            STREAM_BIT_RATE,
            STREAM_MAX_FPS,
        ) {
            Ok(size) => size,
            Err(error) => {
                fail_friend_point_summon_start(
                    &app,
                    &state,
                    format!("启动 scrcpy 视频流失败: {error}"),
                );
                return;
            }
        };
        if !stream_meets_minimum_resolution(w, h) {
            let _ = sidecar.stop_stream();
            fail_friend_point_summon_start(&app, &state, stream_resolution_error(w, h));
            return;
        }
        let input_size = input_size_for_taps(adb_dev.screen_size(), (w, h));
        emit_stream_mapping_debug(&app, "friend-point-summon", input_size, (w, h));
        let runner = FriendPointSummonRunner::new(
            adb_dev,
            sidecar,
            app,
            state,
            cancel,
            input_size,
            Some(debug_sidecar),
        );
        runner.run();
    });
    Ok(())
}

#[tauri::command]
pub(crate) fn stop_friend_point_summon_automation(
    handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
) -> Result<(), String> {
    let handle = handle_state.lock().unwrap();
    handle.cancel.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
pub(crate) fn get_friend_point_summon_automation_status(
    handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
) -> FriendPointSummonRunnerState {
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

    #[test]
    fn stream_mapping_debug_message_reports_mismatched_dimensions() {
        assert_eq!(
            stream_mapping_debug_message("runner", (2560, 1440), (1920, 1080)),
            Some("[runner] using adb input size 2560x1440 with stream frame 1920x1080".to_string())
        );
        assert_eq!(
            stream_mapping_debug_message("runner", (1920, 1080), (1920, 1080)),
            None
        );
    }

    #[test]
    fn scrcpy_stream_start_user_message_recommends_adb_reset_for_offline_device() {
        let error = concat!(
            "start_stream failed: failed to start stream: adb -s 127.0.0.1:5555 push ",
            "/tmp/scrcpy-server.jar /data/local/tmp/scrcpy-server.jar failed (1): ",
            "adb: error: failed to get feature set: device offline"
        );

        assert_eq!(
            scrcpy_stream_start_user_message(error),
            Some(ADB_RESET_USER_MESSAGE)
        );
        assert_eq!(scrcpy_stream_start_user_message("decoder stopped"), None);
    }

    #[test]
    fn effective_recognition_settings_inherit_global_skill_activation_verification() {
        let global = RecognitionSettings {
            verify_skill_activation: true,
            ..RecognitionSettings::default()
        };

        let effective = effective_recognition_settings(global, None);

        assert!(effective.verify_skill_activation);
    }

    #[test]
    fn effective_recognition_settings_project_overrides_skill_activation_verification() {
        let global = RecognitionSettings {
            verify_skill_activation: true,
            ..RecognitionSettings::default()
        };
        let project = ProjectRecognitionSettings {
            verify_skill_activation: Some(false),
            ..ProjectRecognitionSettings::default()
        };

        let effective = effective_recognition_settings(global, Some(project));

        assert!(!effective.verify_skill_activation);
    }
}
