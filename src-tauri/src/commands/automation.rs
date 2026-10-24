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
use crate::commands::rank_up_quest::{apply_rank_up_quest_workflow, RankUpQuestCaptureState};
use crate::commands::runtime::{
    resolve_ce_assets_dir, resolve_mystic_code_assets_dir, resolve_scrcpy_jar,
    resolve_servant_assets_dir,
};
use crate::commands::settings::{AdbDeviceSettings, DebugSettings, RecognitionSettings};
use crate::craft_essence_enhancement_runner::{
    lifecycle_transition as ce_lifecycle_transition, server_supported as ce_server_supported,
    CraftEssenceEnhancementAutomationEvent, CraftEssenceEnhancementMode,
    CraftEssenceEnhancementRunner, CraftEssenceEnhancementRunnerHandle,
    CraftEssenceEnhancementRunnerState, LifecycleEvent as CeLifecycleEvent,
    EVENT_NAME as CE_EVENT_NAME,
};
use crate::device_operation::{AutomationKind, DeviceOperation, DeviceOperationCoordinator};
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
use crate::operation_log::{self, DebugSource};
use crate::paths::{AppUiSettings, MysticCodeGender};
use crate::runner::{
    build_team_check_targets, grand_strategy, runner_lifecycle_transition, AutomationEvent,
    LogLevel, RankUpQuestStartRequest, RunConfig, Runner, RunnerHandle, RunnerLifecycleEvent,
    RunnerState, TeamVerificationControl, TeamVerificationDecision, TeamVerificationReport,
};
use crate::screen;
use crate::server::{Server, STREAM_BIT_RATE, STREAM_MAX_FPS, STREAM_MAX_SIZE};
use crate::{resolve_cv_config_paths, resolve_template_dirs};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};

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
        if client.is_healthy() {
            eprintln!("[mash-cv] reusing cached sidecar");
            return Ok(client);
        }
        eprintln!("[mash-cv] discarding sidecar after template load failure");
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
    operation_log::emit_debug(app, DebugSource::App, message);
}

fn resolve_mystic_code_item_template(
    app: &tauri::AppHandle,
    config: &RunConfig,
    gender: MysticCodeGender,
) -> Option<std::path::PathBuf> {
    let id = config.mystic_code_id?;
    let assets_dir = resolve_mystic_code_assets_dir(app)?;
    let gender_name = match gender {
        crate::paths::MysticCodeGender::Female => "female",
        crate::paths::MysticCodeGender::Male => "male",
    };
    let path = assets_dir
        .join(id.to_string())
        .join(format!("item-{gender_name}.png"));
    path.is_file().then_some(path)
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
    crate::recognition_diagnostics::logs::record(app, screen, message);
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

fn fail_automation_start(app: &tauri::AppHandle, state: &Arc<Mutex<RunnerState>>, message: String) {
    apply_runner_lifecycle_event(
        state,
        RunnerLifecycleEvent::Failed {
            message: message.clone(),
        },
    );
    emit_automation_status(app, state, "", &format!("启动失败: {message}"));
}

fn stop_automation_start(app: &tauri::AppHandle, state: &Arc<Mutex<RunnerState>>) {
    apply_runner_lifecycle_event(state, RunnerLifecycleEvent::StopRequested);
    emit_automation_status(app, state, "", "自动化已停止");
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
        auto_friend_request: global.auto_friend_request,
    }
}

#[tauri::command]
pub(crate) fn start_automation(
    app: tauri::AppHandle,
    config: RunConfig,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    coordinator: tauri::State<'_, DeviceOperationCoordinator>,
    debug_state: tauri::State<'_, debug::DebugSidecar>,
) -> Result<(), String> {
    start_automation_with_config(&app, config, &handle_state, &coordinator, &debug_state)
}

#[tauri::command]
pub(crate) fn start_rank_up_quest_automation(
    app: tauri::AppHandle,
    mut config: RunConfig,
    workflow: RankUpQuestStartRequest,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    coordinator: tauri::State<'_, DeviceOperationCoordinator>,
    debug_state: tauri::State<'_, debug::DebugSidecar>,
    capture_state: tauri::State<'_, RankUpQuestCaptureState>,
) -> Result<(), String> {
    let server = *app.state::<Mutex<Server>>().lock().unwrap();
    if server != Server::Cn {
        return Err("强化任务自动化首版仅支持国服".into());
    }
    apply_rank_up_quest_workflow(&mut config, workflow, &capture_state)?;
    start_automation_with_config(&app, config, &handle_state, &coordinator, &debug_state)
}

fn start_automation_with_config(
    app: &tauri::AppHandle,
    mut config: RunConfig,
    handle_state: &Mutex<RunnerHandle>,
    coordinator: &DeviceOperationCoordinator,
    debug_state: &debug::DebugSidecar,
) -> Result<(), String> {
    let automation_lease =
        coordinator.reserve(DeviceOperation::Automation(AutomationKind::Battle))?;

    let project = read_projects(app)?
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
        load_battle_scenes(app.clone(), config.project_id.clone())?
    };
    let advanced_scenes = if advanced_mode {
        load_advanced_battle_scenes(app.clone(), config.project_id.clone())?
    } else {
        Vec::new()
    };

    let selected_adb_serial = app
        .state::<Mutex<AdbDeviceSettings>>()
        .lock()
        .unwrap()
        .selected_adb_serial
        .clone();
    let server = *app.state::<Mutex<Server>>().lock().unwrap();
    let recognition_settings = effective_recognition_settings(
        *app.state::<Mutex<RecognitionSettings>>().lock().unwrap(),
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
    config.support_class_filter_mode = project
        .as_ref()
        .and_then(|project| project.recognition_settings)
        .and_then(|settings| settings.support_class_filter_mode)
        .unwrap_or_default();
    config.support_full_list_ocr_fallback = recognition_settings.support_full_list_ocr_fallback;
    config.unknown_screen_timeout_count = recognition_settings.unknown_screen_timeout_count;
    let debug_settings = *app.state::<Mutex<DebugSettings>>().lock().unwrap();
    config.auto_capture_battle_before_attack = debug_settings.auto_capture_battle_before_attack;
    config.auto_capture_battle_result_loot = debug_settings.auto_capture_battle_result_loot;
    config.auto_friend_request = recognition_settings.auto_friend_request;
    config.auto_capture_skill_use_probe = debug_settings.auto_capture_skill_use_probe;
    config.auto_capture_unrecognized_critical_chance =
        debug_settings.auto_capture_unrecognized_critical_chance;
    config.prefer_higher_critical_chance = project
        .as_ref()
        .is_some_and(|project| project.prefer_higher_critical_chance);
    let mystic_code_gender = app
        .state::<Mutex<AppUiSettings>>()
        .lock()
        .unwrap()
        .mystic_code_gender;
    let mystic_code_item_template =
        resolve_mystic_code_item_template(&app, &config, mystic_code_gender);

    let team_check_targets = match project.as_ref() {
        Some(project) => build_team_check_targets(
            project,
            resolve_servant_assets_dir(app).as_deref(),
            resolve_ce_assets_dir(app).as_deref(),
        )?,
        None => return Err("未找到当前编队项目".into()),
    };
    let mystic_code_name = config.mystic_code_id.and_then(|id| {
        crate::commands::catalog::get_mystic_codes(app.clone())
            .ok()?
            .into_iter()
            .find(|code| code.id == id)
            .map(|code| code.name)
    });
    let team_verification = Arc::new(Mutex::new(TeamVerificationControl::default()));
    let app = app.clone();
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
        handle.team_verification = team_verification.clone();
        handle.state = state.clone();
        handle.cancel = cancel.clone();
        handle.stop_after_current = stop_after_current.clone();
    }

    let debug_sidecar = debug_state.0.clone();
    std::thread::spawn(move || {
        let _automation_lease = automation_lease;
        let _run_finish_guard = BattleRunFinishGuard {
            recorder: run_recorder.clone(),
            state: state.clone(),
        };
        let Some(AutomationSession {
            adb: adb_dev,
            sidecar,
            input_size,
            frame_size,
        }) = prepare_automation_session(
            &app,
            server,
            selected_adb_serial,
            &debug::DebugSidecar(debug_sidecar.clone()),
            &cancel,
            StartupFeedback {
                scope: "runner",
                progress: &|message| emit_automation_status(&app, &state, "", message),
                failed: &|message| fail_automation_start(&app, &state, message),
                cancelled: &|| stop_automation_start(&app, &state),
            },
            |sidecar| {
                sidecar
                    .set_digit_recognition_mode(
                        debug_settings
                            .image_recognition_debug_mode
                            .as_sidecar_value(),
                    )
                    .map_err(|error| format!("配置图像识别调试模式失败: {error}"))?;
                sidecar
                    .set_sequence_recognition_mode(
                        debug_settings
                            .sequence_recognition_debug_mode
                            .as_sidecar_value(),
                    )
                    .map_err(|error| format!("配置序列识别调试模式失败: {error}"))
            },
        )
        else {
            return;
        };
        let screen_size = Some(input_size);
        let frame_size = Some(frame_size);
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
            mystic_code_item_template,
            server,
            Some(debug_sidecar),
            run_recorder,
            team_verification,
            team_check_targets,
            mystic_code_name,
        );
        runner.run();
    });

    Ok(())
}

#[tauri::command]
pub(crate) fn get_team_verification(
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
) -> Result<Option<TeamVerificationReport>, String> {
    let handle = handle_state.lock().unwrap();
    let report = handle.team_verification.lock().unwrap().report.clone();
    Ok(report)
}

#[tauri::command]
pub(crate) fn resolve_team_verification(
    run_id: String,
    check_id: u64,
    decision: TeamVerificationDecision,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
) -> Result<(), String> {
    let handle = handle_state.lock().unwrap();
    if handle.cancel.load(Ordering::Relaxed) || *handle.state.lock().unwrap() != RunnerState::Paused
    {
        return Err("当前没有待处理的编队检查".into());
    }
    let result = handle
        .team_verification
        .lock()
        .unwrap()
        .resolve(&run_id, check_id, decision);
    result
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

mod craft_essence;
mod enhancement;
mod friend_point;
mod startup;
pub(crate) use craft_essence::*;
pub(crate) use enhancement::*;
pub(crate) use friend_point::*;
use startup::{prepare_automation_session, AutomationSession, StartupFeedback};

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
