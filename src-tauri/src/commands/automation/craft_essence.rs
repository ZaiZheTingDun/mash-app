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
