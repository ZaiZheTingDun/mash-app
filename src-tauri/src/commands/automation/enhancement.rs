//! Servant enhancement automation commands and startup lifecycle handling.

use super::*;

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
        let state = state.lock().unwrap();
        (format!("{:?}", *state), state.status())
    };
    crate::recognition_diagnostics::logs::record(app, screen, message);
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

fn stop_enhancement_start(app: &tauri::AppHandle, state: &Arc<Mutex<EnhancementRunnerState>>) {
    apply_enhancement_lifecycle_event(state, EnhancementLifecycleEvent::StopRequested);
    emit_enhancement_status(app, state, "", "强化自动化已停止");
}

#[tauri::command]
pub(crate) fn start_enhancement_automation(
    app: tauri::AppHandle,
    config: EnhancementConfig,
    handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    coordinator: tauri::State<'_, AutomationCoordinator>,
    debug_state: tauri::State<'_, debug::DebugSidecar>,
) -> Result<(), String> {
    let automation_lease = coordinator.reserve(AutomationKind::ServantEnhancement)?;

    let selected_adb_serial = app
        .state::<Mutex<AdbDeviceSettings>>()
        .lock()
        .unwrap()
        .selected_adb_serial
        .clone();
    let server = *app.state::<Mutex<Server>>().lock().unwrap();
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
                scope: "enhancement",
                progress: &|message| emit_enhancement_status(&app, &state, "", message),
                failed: &|message| fail_enhancement_start(&app, &state, message),
                cancelled: &|| stop_enhancement_start(&app, &state),
            },
            |_| Ok(()),
        )
        else {
            return;
        };

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
