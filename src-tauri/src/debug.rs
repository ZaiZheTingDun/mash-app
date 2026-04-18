use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::adb;
use crate::runner::{self, RunnerHandle, RunnerState};
use crate::screen::{
    CommandCardMatch, ElementMatch, NoblePhantasmMatch, NormRect, SidecarClient,
};
use crate::{
    app_data_dir, resolve_assets_dir, resolve_cv_config_path, resolve_scrcpy_jar,
    resolve_templates_dir, STREAM_BIT_RATE, STREAM_MAX_SIZE,
};

// ---------------------------------------------------------------------------
// Debug: CV probe + coordinate visualization
// ---------------------------------------------------------------------------

pub struct DebugSidecar(pub Mutex<Option<SidecarClient>>);

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DebugScreenSize {
    w: u32,
    h: u32,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugCaptureResult {
    image_path: String,
    screen: String,
    score: f64,
    screen_size: Option<DebugScreenSize>,
}

fn debug_image_path(app: &tauri::AppHandle) -> PathBuf {
    let dir = app_data_dir(app).join("debug");
    fs::create_dir_all(&dir).ok();
    dir.join("last.jpg")
}

fn ensure_debug_sidecar(
    app: &tauri::AppHandle,
    debug_state: &DebugSidecar,
) -> Result<(), String> {
    let mut guard = debug_state.0.lock().unwrap();
    if guard.is_none() {
        let tdir = resolve_templates_dir(app);
        let cfg = resolve_cv_config_path(app);
        eprintln!(
            "[debug] spawning mash-cv sidecar (templates_dir={}, config={})",
            tdir.as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "<none>".into()),
            cfg.as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "<none>".into()),
        );
        let client = SidecarClient::spawn(app, tdir.as_deref(), cfg.as_deref())?;
        eprintln!("[debug] sidecar ready");
        *guard = Some(client);
    }
    Ok(())
}

/// Ensure the debug sidecar has a live scrcpy stream. Idempotent.
fn ensure_debug_stream(
    app: &tauri::AppHandle,
    debug_state: &DebugSidecar,
    serial: Option<&str>,
) -> Result<(), String> {
    ensure_debug_sidecar(app, debug_state)?;

    let mut guard = debug_state.0.lock().unwrap();
    let client = guard
        .as_mut()
        .ok_or_else(|| "debug sidecar not initialized".to_string())?;

    // Cheap no-op probe: ``stream_size`` is set by ``start_stream`` and
    // cleared by ``stop_stream``, so a Some value means we believe a stream
    // is live. We follow up with a zero-wait ``get_frame`` to make sure the
    // sidecar agrees (catches the case where the decoder thread died after
    // a successful ``start_stream`` returned).
    if client.stream_size().is_some() && client.get_frame_jpeg(0.0).is_ok() {
        return Ok(());
    }

    let jar = resolve_scrcpy_jar(app)
        .ok_or_else(|| "找不到 scrcpy-server.jar 资源".to_string())?;
    if !jar.exists() {
        return Err(format!("scrcpy-server.jar 不存在: {}", jar.display()));
    }

    eprintln!(
        "[debug] starting scrcpy stream (serial={}, jar={})",
        serial.unwrap_or("<auto>"),
        jar.display()
    );
    let (w, h) = client.start_stream(&jar, serial, STREAM_MAX_SIZE, STREAM_BIT_RATE)?;
    eprintln!("[debug] scrcpy stream started: {w}x{h}");
    Ok(())
}

/// Returns Err with a user-facing message when automation is currently
/// running. Debug commands route through this so we don't end up with two
/// scrcpy servers + sidecars touching the same device at the same time.
fn require_automation_idle(
    handle_state: &Mutex<RunnerHandle>,
) -> Result<(), String> {
    let handle = handle_state.lock().unwrap();
    let state = handle.state.lock().unwrap().clone();
    if matches!(state, RunnerState::Running) {
        Err("自动化正在运行中，请先停止后再使用调试功能".into())
    } else {
        Ok(())
    }
}

#[tauri::command]
pub fn debug_capture(
    app: tauri::AppHandle,
    bluestack_state: tauri::State<'_, Mutex<bool>>,
    debug_state: tauri::State<'_, DebugSidecar>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
) -> Result<DebugCaptureResult, String> {
    require_automation_idle(&handle_state)?;

    let use_bluestack = *bluestack_state.lock().unwrap();
    eprintln!("[debug_capture] begin (use_bluestack={use_bluestack})");

    let mut adb_dev = adb::Adb::new(use_bluestack);
    adb_dev.connect().map_err(|e| {
        eprintln!("[debug_capture] adb connect failed: {e}");
        e
    })?;
    let serial = adb_dev.serial().map(|s| s.to_string());
    eprintln!("[debug_capture] adb connected, serial={serial:?}");

    ensure_debug_stream(&app, &debug_state, serial.as_deref()).map_err(|e| {
        eprintln!("[debug_capture] ensure stream failed: {e}");
        e
    })?;

    let dest = debug_image_path(&app);

    let (jpeg, screen, score, stream_size) = {
        let mut guard = debug_state.0.lock().unwrap();
        let client = guard
            .as_mut()
            .ok_or_else(|| "debug sidecar not initialized".to_string())?;

        // ``ensure_debug_stream`` already waited for the warmup frame inside
        // start_stream, so a single in-sidecar wait of 2s is enough to cover
        // the post-handshake key-frame gap. No need for a polling retry loop
        // on the Rust side.
        let jpeg = client.get_frame_jpeg(2.0).map_err(|e| {
            eprintln!("[debug_capture] get_frame failed: {e}");
            format!("未获取到视频帧: {e}")
        })?;

        fs::write(&dest, &jpeg).map_err(|e| {
            eprintln!("[debug_capture] write frame failed: {e}");
            format!("failed to write frame: {e}")
        })?;
        let (screen, score) = client.detect_full(Some(&dest)).map_err(|e| {
            eprintln!("[debug_capture] detect failed: {e}");
            e
        })?;
        (jpeg, screen, score, client.stream_size())
    };

    eprintln!(
        "[debug_capture] frame saved: {} ({} bytes), screen={screen} score={score:.3}",
        dest.display(),
        jpeg.len(),
    );

    Ok(DebugCaptureResult {
        image_path: dest.to_string_lossy().to_string(),
        screen: screen.to_string(),
        score,
        screen_size: stream_size.map(|(w, h)| DebugScreenSize { w, h }),
    })
}

#[tauri::command]
pub fn debug_find_element(
    app: tauri::AppHandle,
    debug_state: tauri::State<'_, DebugSidecar>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    template_key: String,
    region: Option<NormRect>,
    threshold: Option<f64>,
) -> Result<ElementMatch, String> {
    require_automation_idle(&handle_state)?;

    let image_path = debug_image_path(&app);
    if !image_path.exists() {
        eprintln!("[debug_find_element] no screenshot at {}", image_path.display());
        return Err("尚未截取画面，请先点击 截取画面".into());
    }

    ensure_debug_sidecar(&app, &debug_state)?;

    let region = region.unwrap_or(NormRect {
        x: 0.0,
        y: 0.0,
        w: 1.0,
        h: 1.0,
    });
    let threshold = threshold.unwrap_or(0.8);
    eprintln!(
        "[debug_find_element] key={template_key} threshold={threshold} region=({:.2},{:.2},{:.2},{:.2})",
        region.x, region.y, region.w, region.h
    );

    let mut guard = debug_state.0.lock().unwrap();
    let client = guard
        .as_mut()
        .ok_or_else(|| "debug sidecar not initialized".to_string())?;
    let result = client.find_element_full(Some(&image_path), &template_key, region, threshold)?;
    eprintln!(
        "[debug_find_element] result: found={} score={:.3} xy=({:.3},{:.3})",
        result.found, result.score, result.x, result.y
    );
    Ok(result)
}

#[tauri::command]
pub fn debug_list_templates(app: tauri::AppHandle) -> Vec<String> {
    let Some(dir) = resolve_templates_dir(&app) else {
        eprintln!("[debug_list_templates] resource_dir not available");
        return Vec::new();
    };
    let mut keys = Vec::new();
    match fs::read_dir(&dir) {
        Ok(entries) => {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase())
                    == Some("png".to_string())
                {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        keys.push(stem.to_string());
                    }
                }
            }
        }
        Err(e) => {
            eprintln!(
                "[debug_list_templates] cannot read {}: {e}",
                dir.display()
            );
        }
    }
    keys.sort();
    eprintln!(
        "[debug_list_templates] {} template(s) in {}",
        keys.len(),
        dir.display()
    );
    keys
}

#[tauri::command]
pub fn debug_find_element_by_name(
    app: tauri::AppHandle,
    debug_state: tauri::State<'_, DebugSidecar>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    screen: String,
    element: String,
) -> Result<ElementMatch, String> {
    require_automation_idle(&handle_state)?;

    let image_path = debug_image_path(&app);
    if !image_path.exists() {
        return Err("尚未截取画面，请先点击 截取画面".into());
    }

    ensure_debug_sidecar(&app, &debug_state)?;

    eprintln!(
        "[debug_find_element_by_name] screen={screen} element={element}"
    );

    let mut guard = debug_state.0.lock().unwrap();
    let client = guard
        .as_mut()
        .ok_or_else(|| "debug sidecar not initialized".to_string())?;
    let result = client.find_element_by_name(Some(&image_path), &screen, &element)?;
    eprintln!(
        "[debug_find_element_by_name] result: found={} score={:.3}",
        result.found, result.score
    );
    Ok(result)
}

#[tauri::command]
pub fn debug_get_cv_config(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    let path = resolve_cv_config_path(&app)
        .ok_or_else(|| "resource_dir not available".to_string())?;
    let contents = fs::read_to_string(&path).map_err(|e| {
        eprintln!("[debug_get_cv_config] read {} failed: {e}", path.display());
        format!("读取 cv.json 失败: {e}")
    })?;
    serde_json::from_str(&contents).map_err(|e| format!("解析 cv.json 失败: {e}"))
}

#[tauri::command]
pub fn debug_reload_sidecar(
    app: tauri::AppHandle,
    debug_state: tauri::State<'_, DebugSidecar>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
) -> Result<(), String> {
    require_automation_idle(&handle_state)?;
    {
        let mut guard = debug_state.0.lock().unwrap();
        guard.take();
    }
    ensure_debug_sidecar(&app, &debug_state)
}

#[tauri::command]
pub fn debug_shutdown(debug_state: tauri::State<'_, DebugSidecar>) -> Result<(), String> {
    let mut guard = debug_state.0.lock().unwrap();
    guard.take();
    Ok(())
}

/// Snapshot of every `Point` / `NormRect` constant the runner uses, grouped
/// for display in the debug UI. Returns static data — no device/sidecar
/// interaction required.
#[tauri::command]
pub fn debug_get_runner_coordinates() -> runner::DebugCoordinates {
    runner::debug_coordinates()
}

/// Run the command-card detector against the most recent debug screenshot.
/// Pass any number of candidate ``servant_ids`` to attempt face
/// identification; pass an empty list to only locate the 5 slots + suits.
#[tauri::command]
pub fn debug_find_command_cards(
    app: tauri::AppHandle,
    debug_state: tauri::State<'_, DebugSidecar>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    servant_ids: Vec<u32>,
) -> Result<Vec<CommandCardMatch>, String> {
    require_automation_idle(&handle_state)?;

    let image_path = debug_image_path(&app);
    if !image_path.exists() {
        return Err("尚未截取画面，请先点击 截取画面".into());
    }

    ensure_debug_sidecar(&app, &debug_state)?;

    let assets_dir = resolve_assets_dir(&app);
    eprintln!(
        "[debug_find_command_cards] servant_ids={servant_ids:?} assets_dir={}",
        assets_dir
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "<none>".into()),
    );

    let mut guard = debug_state.0.lock().unwrap();
    let client = guard
        .as_mut()
        .ok_or_else(|| "debug sidecar not initialized".to_string())?;
    let cards = client.find_command_cards(
        Some(&image_path),
        None,
        &servant_ids,
        assets_dir.as_deref(),
    )?;
    eprintln!("[debug_find_command_cards] {} card(s) found", cards.len());
    Ok(cards)
}

/// Run the NP-readiness detector against the most recent debug screenshot.
/// Returns one record per Noble Phantasm card slot, each with a
/// ``ready`` flag plus the underlying edge-density / std measurements
/// for tuning.
#[tauri::command]
pub fn debug_find_noble_phantasms(
    app: tauri::AppHandle,
    debug_state: tauri::State<'_, DebugSidecar>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
) -> Result<Vec<NoblePhantasmMatch>, String> {
    require_automation_idle(&handle_state)?;

    let image_path = debug_image_path(&app);
    if !image_path.exists() {
        return Err("尚未截取画面，请先点击 截取画面".into());
    }

    ensure_debug_sidecar(&app, &debug_state)?;

    let mut guard = debug_state.0.lock().unwrap();
    let client = guard
        .as_mut()
        .ok_or_else(|| "debug sidecar not initialized".to_string())?;
    let slots = client.find_noble_phantasms(Some(&image_path), None)?;
    eprintln!(
        "[debug_find_noble_phantasms] {} slot(s) found, ready={}",
        slots.len(),
        slots.iter().filter(|s| s.ready).count(),
    );
    Ok(slots)
}

/// List every servant id under ``assets/`` that has at least one
/// ``card_servant_*.png`` file. The Debug UI uses this to populate the
/// candidate-id picker without the user having to know what ships.
#[tauri::command]
pub fn debug_list_servant_assets(app: tauri::AppHandle) -> Vec<u32> {
    let Some(dir) = resolve_assets_dir(&app) else {
        eprintln!("[debug_list_servant_assets] no assets dir resolved");
        return Vec::new();
    };
    let mut ids: Vec<u32> = Vec::new();
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!(
                "[debug_list_servant_assets] cannot read {}: {e}",
                dir.display()
            );
            return Vec::new();
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Ok(id) = name.parse::<u32>() else { continue };

        let mut has_face = false;
        if let Ok(inner) = fs::read_dir(&path) {
            for f in inner.flatten() {
                let fname = f.file_name();
                let Some(fname) = fname.to_str() else { continue };
                if fname.starts_with("card_servant_")
                    && fname.to_lowercase().ends_with(".png")
                {
                    has_face = true;
                    break;
                }
            }
        }
        if has_face {
            ids.push(id);
        }
    }
    ids.sort_unstable();
    eprintln!(
        "[debug_list_servant_assets] {} servant(s) under {}",
        ids.len(),
        dir.display()
    );
    ids
}

/// Pre-warm the debug sidecar process so the first user interaction with the
/// Debug page doesn't pay the 20-40s PyInstaller cold-boot cost. Safe to call
/// at any time; idempotent. The frontend should invoke this on app startup
/// (or on Debug page mount) to move the cold-boot off the critical path.
///
/// This intentionally does *not* start the scrcpy stream -- streaming would
/// conflict with a running automation session and is started lazily by
/// `debug_capture` itself.
#[tauri::command]
pub fn warm_sidecar(
    app: tauri::AppHandle,
    debug_state: tauri::State<'_, DebugSidecar>,
) -> Result<(), String> {
    ensure_debug_sidecar(&app, &debug_state)
}
