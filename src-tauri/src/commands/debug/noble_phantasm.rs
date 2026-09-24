use super::*;

#[tauri::command]
pub fn debug_read_noble_phantasm_gauges_live(
    app: tauri::AppHandle,
    adb_settings_state: tauri::State<'_, Mutex<AdbDeviceSettings>>,
    server_state: tauri::State<'_, Mutex<Server>>,
    debug_state: tauri::State<'_, DebugSidecar>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    ce_enhancement_handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    friend_point_summon_handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
) -> Result<Vec<NoblePhantasmMatch>, String> {
    require_automation_idle(
        &handle_state,
        &enhancement_handle_state,
        &ce_enhancement_handle_state,
        &friend_point_summon_handle_state,
    )?;
    ensure_debug_stream_for_current_device(&app, &adb_settings_state, &server_state, &debug_state)?;

    let started = Instant::now();
    let sample_interval = Duration::from_millis(200);
    let sample_window = Duration::from_secs(1);
    let mut samples: Vec<Vec<NoblePhantasmMatch>> = Vec::new();

    loop {
        let sample = {
            let mut guard = debug_state.0.lock().unwrap();
            let client = guard
                .as_mut()
                .ok_or_else(|| "debug sidecar not initialized".to_string())?;
            client.find_noble_phantasms_with_digit_debug(None, None, true)?
        };

        samples.push(sample);

        if started.elapsed() >= sample_window {
            break;
        }
        thread::sleep(sample_interval);
    }

    let slots = aggregate_np_gauge_samples(&samples);
    Ok(slots)
}

fn read_noble_phantasm_gauges(
    app: &tauri::AppHandle,
    server_state: &tauri::State<'_, Mutex<Server>>,
    debug_state: &tauri::State<'_, DebugSidecar>,
    handle_state: &tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: &tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    ce_enhancement_handle_state: &tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    friend_point_summon_handle_state: &tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
) -> Result<Vec<NoblePhantasmMatch>, String> {
    require_automation_idle(
        handle_state,
        enhancement_handle_state,
        ce_enhancement_handle_state,
        friend_point_summon_handle_state,
    )?;

    let image_path = debug_image_path(app);
    if !image_path.exists() {
        return Err("尚未截取画面，请先点击 截取画面".into());
    }

    ensure_debug_sidecar(app, debug_state, current_server(server_state))?;

    let mut guard = debug_state.0.lock().unwrap();
    let client = guard
        .as_mut()
        .ok_or_else(|| "debug sidecar not initialized".to_string())?;
    client.find_noble_phantasms_with_digit_debug(Some(&image_path), None, true)
}

/// Debug the bottom NP-gauge percentage detector against the most recent
/// debug screenshot. Returns one record per front-line servant, including
/// the gauge ROI and visible digit count.
#[tauri::command]
pub fn debug_read_noble_phantasm_gauges(
    app: tauri::AppHandle,
    server_state: tauri::State<'_, Mutex<Server>>,
    debug_state: tauri::State<'_, DebugSidecar>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    ce_enhancement_handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    friend_point_summon_handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
) -> Result<Vec<NoblePhantasmMatch>, String> {
    let slots = read_noble_phantasm_gauges(
        &app,
        &server_state,
        &debug_state,
        &handle_state,
        &enhancement_handle_state,
        &ce_enhancement_handle_state,
        &friend_point_summon_handle_state,
    )?;
    eprintln!(
        "[debug_read_noble_phantasm_gauges] {} slot(s) found, ready={}",
        slots.len(),
        slots.iter().filter(|s| s.ready).count(),
    );
    Ok(slots)
}
