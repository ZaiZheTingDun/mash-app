//! Shared process and stream lifecycle for Debug CV commands.

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::adb;
use crate::commands::settings::AdbDeviceSettings;
use crate::craft_essence_enhancement_runner::{
    CraftEssenceEnhancementRunnerHandle, CraftEssenceEnhancementRunnerState,
};
use crate::enhancement_runner::{EnhancementRunnerHandle, EnhancementRunnerState};
use crate::friend_point_summon_runner::{
    FriendPointSummonRunnerHandle, FriendPointSummonRunnerState,
};
use crate::runner::{RunnerHandle, RunnerState};
use crate::screen::SidecarClient;
use crate::{
    app_data_dir, resolve_cv_config_paths, resolve_scrcpy_jar, resolve_template_dirs, Server,
    STREAM_BIT_RATE, STREAM_MAX_FPS, STREAM_MAX_SIZE,
};

// ---------------------------------------------------------------------------
// Debug: CV probe + coordinate visualization
// ---------------------------------------------------------------------------

pub struct DebugSidecar(pub Arc<Mutex<Option<SidecarClient>>>);

impl DebugSidecar {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(None)))
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DebugScreenSize {
    pub(super) w: u32,
    pub(super) h: u32,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugCaptureResult {
    pub(super) image_path: String,
    pub(super) screen: String,
    pub(super) score: f64,
    pub(super) screen_size: Option<DebugScreenSize>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugStreamStatus {
    pub(super) connected: bool,
    screen_size: Option<DebugScreenSize>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugStreamFrameResult {
    pub(super) jpeg_base64: String,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) screen: Option<String>,
    pub(super) score: Option<f64>,
    pub(super) timestamp_ms: u128,
}

pub(super) fn debug_image_path(app: &tauri::AppHandle) -> PathBuf {
    let dir = app_data_dir(app).join("debug");
    fs::create_dir_all(&dir).ok();
    dir.join("last.jpg")
}

pub(super) fn current_server(server_state: &Mutex<Server>) -> Server {
    *server_state.lock().unwrap()
}

pub(super) fn ensure_debug_sidecar(
    app: &tauri::AppHandle,
    debug_state: &DebugSidecar,
    server: Server,
) -> Result<(), String> {
    let mut guard = debug_state.0.lock().unwrap();
    if guard.is_none() {
        let tdirs = resolve_template_dirs(app, server);
        let cfgs = resolve_cv_config_paths(app, server);
        eprintln!(
            "[debug] spawning mash-cv sidecar (server={server}, templates_dirs={}, configs={})",
            tdirs
                .iter()
                .map(|spec| match spec.key_prefix.as_deref() {
                    Some(prefix) => format!("{}=>{}", spec.dir.display(), prefix),
                    None => spec.dir.display().to_string(),
                })
                .collect::<Vec<_>>()
                .join(","),
            cfgs.iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join(","),
        );
        let client =
            crate::commands::automation::spawn_configured_sidecar(app, server).map_err(|err| {
                if let Some(user_message) = crate::screen::sidecar_startup_user_message(&err) {
                    eprintln!("[debug] sidecar startup detail: {err}");
                    user_message.to_string()
                } else {
                    err
                }
            })?;
        eprintln!("[debug] sidecar ready");
        *guard = Some(client);
    }
    Ok(())
}

/// Ensure the debug sidecar has a live scrcpy stream. Idempotent.
pub(super) fn ensure_debug_stream(
    app: &tauri::AppHandle,
    debug_state: &DebugSidecar,
    server: Server,
    serial: Option<&str>,
) -> Result<(), String> {
    ensure_debug_sidecar(app, debug_state, server)?;

    let mut guard = debug_state.0.lock().unwrap();
    let client = guard
        .as_mut()
        .ok_or_else(|| "debug sidecar not initialized".to_string())?;

    // Cheap no-op probe: ``stream_size`` is set by ``start_stream`` and
    // cleared by ``stop_stream``, so a Some value means we believe a stream
    // is live. We follow up with a zero-wait ``get_frame`` to make sure the
    // sidecar agrees (catches the case where the decoder thread died after
    // a successful ``start_stream`` returned).
    if let Some((w, h)) = client.stream_size() {
        if crate::stream_meets_minimum_resolution(w, h) && client.get_frame_jpeg(0.0).is_ok() {
            return Ok(());
        }
        if let Err(e) = client.stop_stream() {
            eprintln!("[debug] stop low-resolution/stale stream failed: {e}");
        }
    }

    let jar = resolve_scrcpy_jar(app).ok_or_else(|| "找不到 scrcpy-server.jar 资源".to_string())?;
    if !jar.exists() {
        return Err(format!("scrcpy-server.jar 不存在: {}", jar.display()));
    }

    eprintln!(
        "[debug] starting scrcpy stream (serial={}, jar={})",
        serial.unwrap_or("<auto>"),
        jar.display()
    );
    let adb_path = adb::resolve_adb_path(app);
    let (w, h) = client.start_stream(
        &adb_path,
        &jar,
        serial,
        STREAM_MAX_SIZE,
        STREAM_BIT_RATE,
        STREAM_MAX_FPS,
    )?;
    eprintln!("[debug] scrcpy stream started: {w}x{h}");
    if !crate::stream_meets_minimum_resolution(w, h) {
        if let Err(e) = client.stop_stream() {
            eprintln!("[debug] stop unsupported-resolution stream failed: {e}");
        }
        return Err(crate::stream_resolution_error(w, h));
    }
    Ok(())
}

pub(super) fn ensure_debug_stream_for_current_device(
    app: &tauri::AppHandle,
    adb_settings_state: &Mutex<AdbDeviceSettings>,
    server_state: &Mutex<Server>,
    debug_state: &DebugSidecar,
) -> Result<(), String> {
    let selected_adb_serial = adb_settings_state
        .lock()
        .unwrap()
        .selected_adb_serial
        .clone();
    let server = current_server(server_state);
    let mut adb_dev = adb::Adb::new(app, selected_adb_serial);
    adb_dev.connect()?;
    let serial = adb_dev.serial().map(|s| s.to_string());
    ensure_debug_stream(app, debug_state, server, serial.as_deref())
}

pub(super) fn debug_stream_status(debug_state: &DebugSidecar) -> DebugStreamStatus {
    let screen_size = debug_state
        .0
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|client| client.stream_size())
        .map(|(w, h)| DebugScreenSize { w, h });
    DebugStreamStatus {
        connected: screen_size.is_some(),
        screen_size,
    }
}

/// Returns Err with a user-facing message when automation is currently
/// running. Debug commands route through this so we don't end up with two
/// scrcpy servers + sidecars touching the same device at the same time.
pub(super) fn require_automation_idle(
    handle_state: &Mutex<RunnerHandle>,
    enhancement_handle_state: &Mutex<EnhancementRunnerHandle>,
    ce_enhancement_handle_state: &Mutex<CraftEssenceEnhancementRunnerHandle>,
    friend_point_summon_handle_state: &Mutex<FriendPointSummonRunnerHandle>,
) -> Result<(), String> {
    let handle = handle_state.lock().unwrap();
    let state = handle.state.lock().unwrap().clone();
    if matches!(state, RunnerState::Starting | RunnerState::Running) {
        return Err("自动化正在运行中，请先停止后再使用调试功能".into());
    }
    let handle = enhancement_handle_state.lock().unwrap();
    let state = handle.state.lock().unwrap().clone();
    if matches!(
        state,
        EnhancementRunnerState::Starting | EnhancementRunnerState::Running
    ) {
        return Err("强化自动化正在运行中，请先停止后再使用调试功能".into());
    }
    let handle = ce_enhancement_handle_state.lock().unwrap();
    let state = handle.state.lock().unwrap().clone();
    if matches!(
        state,
        CraftEssenceEnhancementRunnerState::Starting | CraftEssenceEnhancementRunnerState::Running
    ) {
        return Err("概念礼装强化自动化正在运行中，请先停止后再使用调试功能".into());
    }
    let handle = friend_point_summon_handle_state.lock().unwrap();
    let state = handle.state.lock().unwrap().clone();
    if matches!(
        state,
        FriendPointSummonRunnerState::Starting | FriendPointSummonRunnerState::Running
    ) {
        return Err("友情点抽取自动化正在运行中，请先停止后再使用调试功能".into());
    }
    Ok(())
}
