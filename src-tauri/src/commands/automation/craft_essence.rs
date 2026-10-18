//! Craft Essence enhancement automation commands and startup lifecycle handling.

use super::*;

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
    crate::recognition_diagnostics::logs::record(app, screen, message);
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

#[tauri::command]
pub(crate) fn start_craft_essence_enhancement_automation(
    app: tauri::AppHandle,
    handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    coordinator: tauri::State<'_, AutomationCoordinator>,
    debug_state: tauri::State<'_, debug::DebugSidecar>,
    mode: Option<CraftEssenceEnhancementMode>,
) -> Result<(), String> {
    let automation_lease = coordinator.reserve(AutomationKind::CraftEssenceEnhancement)?;

    let server = *app.state::<Mutex<Server>>().lock().unwrap();
    if !ce_server_supported(server) {
        return Err("当前仅支持国服概念礼装强化自动化".into());
    }
    let selected_adb_serial = app
        .state::<Mutex<AdbDeviceSettings>>()
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
                scope: "ce-enhancement",
                progress: &|message| emit_ce_enhancement_status(&app, &state, "", message),
                failed: &|message| fail_ce_enhancement_start(&app, &state, message),
                cancelled: &|| stop_ce_enhancement_start(&app, &state),
            },
            |_| Ok(()),
        )
        else {
            return;
        };
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
