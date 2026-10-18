//! Friend-point summon automation commands and startup lifecycle handling.

use super::*;

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
    crate::recognition_diagnostics::logs::record(app, screen, message);
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

#[tauri::command]
pub(crate) fn start_friend_point_summon_automation(
    app: tauri::AppHandle,
    handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
    coordinator: tauri::State<'_, AutomationCoordinator>,
    debug_state: tauri::State<'_, debug::DebugSidecar>,
) -> Result<(), String> {
    let automation_lease = coordinator.reserve(AutomationKind::FriendPointSummon)?;

    let server = *app.state::<Mutex<Server>>().lock().unwrap();
    if !friend_point_summon_server_supported(server) {
        return Err("当前仅支持国服友情点抽取自动化".into());
    }
    let selected_adb_serial = app
        .state::<Mutex<AdbDeviceSettings>>()
        .lock()
        .unwrap()
        .selected_adb_serial
        .clone();
    let state = Arc::new(Mutex::new(FriendPointSummonRunnerState::Starting));
    let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let home_reference = {
        let mut handle = handle_state.lock().unwrap();
        handle.state = state.clone();
        handle.cancel = cancel.clone();
        handle.begin_manual_start()
    };

    let debug_sidecar = debug_state.0.clone();
    std::thread::spawn(move || {
        let _automation_lease = automation_lease;
        let Some(AutomationSession {
            adb: adb_dev,
            sidecar,
            input_size,
            ..
        }) = prepare_automation_session(
            &app,
            server,
            selected_adb_serial,
            &debug::DebugSidecar(debug_sidecar.clone()),
            &cancel,
            StartupFeedback {
                scope: "friend-point-summon",
                progress: &|message| emit_friend_point_summon_status(&app, &state, "", message),
                failed: &|message| fail_friend_point_summon_start(&app, &state, message),
                cancelled: &|| stop_friend_point_summon_start(&app, &state),
            },
            |_| Ok(()),
        )
        else {
            return;
        };
        let runner = FriendPointSummonRunner::new(
            adb_dev,
            sidecar,
            app,
            state,
            cancel,
            input_size,
            Some(debug_sidecar),
            home_reference,
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
