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
