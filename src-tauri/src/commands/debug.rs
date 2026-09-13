use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::adb;
use crate::commands::settings::{AdbDeviceSettings, RecognitionSettings};
use crate::craft_essence_enhancement_runner::CraftEssenceEnhancementRunnerHandle;
use crate::enhancement_runner::{
    EnhancementRunnerHandle, SERVANT_FACE_MATCH_CROP, SERVANT_FACE_TEMPLATE_SIZE,
    SERVANT_LIST_REGION,
};
use crate::friend_point_summon_runner::FriendPointSummonRunnerHandle;
use crate::runner::{self, aggregate_np_gauge_samples, RunnerHandle};
use crate::screen::{
    BondLevelUpReadResult, CommandCardMatch, ElementMatch, FindEnhancementServantGridResult,
    FindSupportsResult, NoblePhantasmMatch, NormRect, Point, ServantGridAnchor, ServantGridCell,
    ServantGridFaceMatch, SupportCeArtworkCheck, SupportCeIconCheck, SupportCeVerificationOptions,
    SupportDiagnostics, SupportRowMatch,
};
use crate::{
    load_servant_metadata_for_variant, resolve_ce_assets_dir, resolve_cv_config_path,
    resolve_servant_assets_dir, resolve_template_dirs, Server,
};

mod session;

use session::{
    current_server, debug_image_path, debug_stream_status, ensure_debug_sidecar,
    ensure_debug_stream, ensure_debug_stream_for_current_device, require_automation_idle,
    DebugScreenSize,
};
pub(crate) use session::{
    DebugCaptureResult, DebugSidecar, DebugStreamFrameResult, DebugStreamStatus,
};

fn cv_element_spec(
    app: &tauri::AppHandle,
    server: Server,
    screen: &str,
    element: &str,
) -> Result<(String, NormRect, f64), String> {
    let path = resolve_cv_config_path(app, server)
        .ok_or_else(|| "resource_dir not available".to_string())?;
    let contents = fs::read_to_string(&path).map_err(|e| format!("读取 cv.json 失败: {e}"))?;
    let cfg: serde_json::Value =
        serde_json::from_str(&contents).map_err(|e| format!("解析 cv.json 失败: {e}"))?;
    let screen_spec = cfg
        .get("screens")
        .and_then(|screens| screens.get(screen))
        .ok_or_else(|| format!("cv.json 缺少 screen 配置: {screen}"))?;
    let spec = find_cv_target_spec(screen_spec, element)
        .ok_or_else(|| format!("cv.json 缺少元素配置: {screen}.{element}"))?;
    let template = spec
        .get("template")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("cv.json 元素缺少 template: {screen}.{element}"))?
        .to_string();
    let region = spec
        .get("region")
        .and_then(parse_norm_rect)
        .ok_or_else(|| format!("cv.json 元素缺少 region: {screen}.{element}"))?;
    let threshold = spec
        .get("threshold")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.8);
    Ok((template, region, threshold))
}

fn find_cv_target_spec<'a>(
    screen: &'a serde_json::Value,
    element: &str,
) -> Option<&'a serde_json::Value> {
    if element == "detect" {
        return screen.get("detect");
    }
    if screen
        .get("detect")
        .and_then(|detect| detect.get("template"))
        .and_then(|template| template.as_str())
        == Some(element)
    {
        return screen.get("detect");
    }
    if let Some(spec) = screen
        .get("elements")
        .and_then(|elements| elements.get(element))
    {
        return Some(spec);
    }

    let variants = screen.get("variants").and_then(|v| v.as_object())?;
    if let Some(rest) = element.strip_prefix("variants.") {
        let mut parts = rest.splitn(2, '.');
        let variant_name = parts.next()?;
        let target_name = parts.next()?;
        let variant = variants.get(variant_name)?;
        if target_name == "detect" {
            return variant.get("detect");
        }
        if let Some(element_name) = target_name.strip_prefix("elements.") {
            return variant
                .get("elements")
                .and_then(|elements| elements.get(element_name));
        }
        return variant
            .get("elements")
            .and_then(|elements| elements.get(target_name));
    }

    for variant in variants.values() {
        if variant
            .get("detect")
            .and_then(|detect| detect.get("template"))
            .and_then(|template| template.as_str())
            == Some(element)
        {
            return variant.get("detect");
        }
        if let Some(spec) = variant
            .get("elements")
            .and_then(|elements| elements.get(element))
        {
            return Some(spec);
        }
    }
    None
}

#[tauri::command]
pub fn debug_capture(
    app: tauri::AppHandle,
    adb_settings_state: tauri::State<'_, Mutex<AdbDeviceSettings>>,
    server_state: tauri::State<'_, Mutex<Server>>,
    debug_state: tauri::State<'_, DebugSidecar>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    ce_enhancement_handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    friend_point_summon_handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
) -> Result<DebugCaptureResult, String> {
    require_automation_idle(
        &handle_state,
        &enhancement_handle_state,
        &ce_enhancement_handle_state,
        &friend_point_summon_handle_state,
    )?;

    let selected_adb_serial = adb_settings_state
        .lock()
        .unwrap()
        .selected_adb_serial
        .clone();
    let server = current_server(&server_state);
    eprintln!(
        "[debug_capture] begin (selected_adb_serial={selected_adb_serial:?}, server={server})"
    );

    let mut adb_dev = adb::Adb::new(&app, selected_adb_serial);
    adb_dev.connect().map_err(|e| {
        eprintln!("[debug_capture] adb connect failed: {e}");
        e
    })?;
    let serial = adb_dev.serial().map(|s| s.to_string());
    eprintln!("[debug_capture] adb connected, serial={serial:?}");

    ensure_debug_stream(&app, &debug_state, server, serial.as_deref()).map_err(|e| {
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
        let (screen, score) = client.detect_label_full(Some(&dest)).map_err(|e| {
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
        screen,
        score,
        screen_size: stream_size.map(|(w, h)| DebugScreenSize { w, h }),
    })
}

#[tauri::command]
pub fn debug_stream_connect(
    app: tauri::AppHandle,
    adb_settings_state: tauri::State<'_, Mutex<AdbDeviceSettings>>,
    server_state: tauri::State<'_, Mutex<Server>>,
    debug_state: tauri::State<'_, DebugSidecar>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    ce_enhancement_handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    friend_point_summon_handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
) -> Result<DebugStreamStatus, String> {
    require_automation_idle(
        &handle_state,
        &enhancement_handle_state,
        &ce_enhancement_handle_state,
        &friend_point_summon_handle_state,
    )?;
    ensure_debug_stream_for_current_device(&app, &adb_settings_state, &server_state, &debug_state)?;
    Ok(debug_stream_status(&debug_state))
}

#[tauri::command]
pub fn debug_stream_disconnect(
    debug_state: tauri::State<'_, DebugSidecar>,
) -> Result<DebugStreamStatus, String> {
    {
        let mut guard = debug_state.0.lock().unwrap();
        if let Some(client) = guard.as_mut() {
            if client.stream_size().is_some() {
                client.stop_stream()?;
            }
        }
    }
    Ok(debug_stream_status(&debug_state))
}

#[tauri::command]
pub fn debug_stream_frame(
    app: tauri::AppHandle,
    adb_settings_state: tauri::State<'_, Mutex<AdbDeviceSettings>>,
    server_state: tauri::State<'_, Mutex<Server>>,
    debug_state: tauri::State<'_, DebugSidecar>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    ce_enhancement_handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    friend_point_summon_handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
    detect_screen: Option<bool>,
) -> Result<DebugStreamFrameResult, String> {
    require_automation_idle(
        &handle_state,
        &enhancement_handle_state,
        &ce_enhancement_handle_state,
        &friend_point_summon_handle_state,
    )?;
    ensure_debug_stream_for_current_device(&app, &adb_settings_state, &server_state, &debug_state)?;

    let (jpeg_base64, width, height, screen, score) = {
        let mut guard = debug_state.0.lock().unwrap();
        let client = guard
            .as_mut()
            .ok_or_else(|| "debug sidecar not initialized".to_string())?;
        let (jpeg_base64, width, height) = client.get_frame_jpeg_base64(0.2)?;
        let (screen, score) = if detect_screen.unwrap_or(false) {
            let (screen, score) = client.detect_label_full(None)?;
            (Some(screen), Some(score))
        } else {
            (None, None)
        };
        (jpeg_base64, width, height, screen, score)
    };

    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);

    Ok(DebugStreamFrameResult {
        jpeg_base64,
        width,
        height,
        screen,
        score,
        timestamp_ms,
    })
}

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
            client.find_noble_phantasms(None, None)?
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

#[tauri::command]
pub fn debug_find_element(
    app: tauri::AppHandle,
    server_state: tauri::State<'_, Mutex<Server>>,
    debug_state: tauri::State<'_, DebugSidecar>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    ce_enhancement_handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    friend_point_summon_handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
    template_key: String,
    region: Option<NormRect>,
    threshold: Option<f64>,
) -> Result<ElementMatch, String> {
    require_automation_idle(
        &handle_state,
        &enhancement_handle_state,
        &ce_enhancement_handle_state,
        &friend_point_summon_handle_state,
    )?;

    let image_path = debug_image_path(&app);
    if !image_path.exists() {
        eprintln!(
            "[debug_find_element] no screenshot at {}",
            image_path.display()
        );
        return Err("尚未截取画面，请先点击 截取画面".into());
    }

    ensure_debug_sidecar(&app, &debug_state, current_server(&server_state))?;

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
pub fn debug_read_bond_level_up(
    app: tauri::AppHandle,
    server_state: tauri::State<'_, Mutex<Server>>,
    debug_state: tauri::State<'_, DebugSidecar>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    ce_enhancement_handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    friend_point_summon_handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
) -> Result<BondLevelUpReadResult, String> {
    require_automation_idle(
        &handle_state,
        &enhancement_handle_state,
        &ce_enhancement_handle_state,
        &friend_point_summon_handle_state,
    )?;

    let image_path = debug_image_path(&app);
    if !image_path.exists() {
        eprintln!(
            "[debug_read_bond_level_up] no screenshot at {}",
            image_path.display()
        );
        return Err("尚未截取画面，请先点击 截取画面".into());
    }

    ensure_debug_sidecar(&app, &debug_state, current_server(&server_state))?;

    let mut guard = debug_state.0.lock().unwrap();
    let client = guard
        .as_mut()
        .ok_or_else(|| "debug sidecar not initialized".to_string())?;
    let result = client.read_bond_level_up(Some(&image_path), true)?;
    eprintln!(
        "[debug_read_bond_level_up] ok={} reason={:?} level={:?} servant={:?}",
        result.ok, result.reason, result.bond_level_after, result.servant_name_matched
    );
    Ok(result)
}

#[tauri::command]
pub fn debug_list_templates(
    app: tauri::AppHandle,
    server_state: tauri::State<'_, Mutex<Server>>,
) -> Vec<String> {
    let server = current_server(&server_state);
    let mut keys = Vec::new();
    for spec in resolve_template_dirs(&app, server) {
        match collect_template_keys(
            &spec.dir,
            &spec.dir,
            spec.key_prefix.as_deref().unwrap_or(""),
            &mut keys,
        ) {
            Ok(()) => {}
            Err(e) => {
                eprintln!(
                    "[debug_list_templates] cannot read {}: {e}",
                    spec.dir.display()
                );
            }
        }
    }
    keys.sort();
    keys.dedup();
    eprintln!(
        "[debug_list_templates] {} template(s) for server={server}",
        keys.len(),
    );
    keys
}

fn collect_template_keys(
    root: &std::path::Path,
    dir: &std::path::Path,
    key_prefix: &str,
    keys: &mut Vec<String>,
) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_template_keys(root, &path, key_prefix, keys)?;
            continue;
        }
        if path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("png"))
            != Some(true)
        {
            continue;
        }
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let mut key = relative.with_extension("").to_string_lossy().into_owned();
        if std::path::MAIN_SEPARATOR != '/' {
            key = key.replace(std::path::MAIN_SEPARATOR, "/");
        }
        let prefix = key_prefix.trim_matches('/');
        if !prefix.is_empty() {
            key = format!("{prefix}/{key}");
        }
        keys.push(key);
    }
    Ok(())
}

#[tauri::command]
pub fn debug_find_element_by_name(
    app: tauri::AppHandle,
    server_state: tauri::State<'_, Mutex<Server>>,
    debug_state: tauri::State<'_, DebugSidecar>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    ce_enhancement_handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    friend_point_summon_handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
    screen: String,
    element: String,
) -> Result<ElementMatch, String> {
    require_automation_idle(
        &handle_state,
        &enhancement_handle_state,
        &ce_enhancement_handle_state,
        &friend_point_summon_handle_state,
    )?;

    let image_path = debug_image_path(&app);
    if !image_path.exists() {
        return Err("尚未截取画面，请先点击 截取画面".into());
    }

    ensure_debug_sidecar(&app, &debug_state, current_server(&server_state))?;

    eprintln!("[debug_find_element_by_name] screen={screen} element={element}");

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
pub fn debug_get_cv_config(
    app: tauri::AppHandle,
    server_state: tauri::State<'_, Mutex<Server>>,
) -> Result<serde_json::Value, String> {
    let server = current_server(&server_state);
    let path = resolve_cv_config_path(&app, server)
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
    server_state: tauri::State<'_, Mutex<Server>>,
    debug_state: tauri::State<'_, DebugSidecar>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    ce_enhancement_handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    friend_point_summon_handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
) -> Result<(), String> {
    require_automation_idle(
        &handle_state,
        &enhancement_handle_state,
        &ce_enhancement_handle_state,
        &friend_point_summon_handle_state,
    )?;
    {
        let mut guard = debug_state.0.lock().unwrap();
        guard.take();
    }
    ensure_debug_sidecar(&app, &debug_state, current_server(&server_state))
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
    server_state: tauri::State<'_, Mutex<Server>>,
    debug_state: tauri::State<'_, DebugSidecar>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    ce_enhancement_handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    friend_point_summon_handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
    servant_ids: Vec<u32>,
) -> Result<Vec<CommandCardMatch>, String> {
    require_automation_idle(
        &handle_state,
        &enhancement_handle_state,
        &ce_enhancement_handle_state,
        &friend_point_summon_handle_state,
    )?;

    let image_path = debug_image_path(&app);
    if !image_path.exists() {
        return Err("尚未截取画面，请先点击 截取画面".into());
    }

    ensure_debug_sidecar(&app, &debug_state, current_server(&server_state))?;

    let assets_dir = resolve_servant_assets_dir(&app);
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
    let cards =
        client.find_command_cards(Some(&image_path), None, &servant_ids, assets_dir.as_deref())?;
    eprintln!("[debug_find_command_cards] {} card(s) found", cards.len());
    Ok(cards)
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
    client.find_noble_phantasms(Some(&image_path), None)
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

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DebugEnhancementServantMatchResult {
    pub servant_id: u32,
    pub search_region: NormRect,
    pub template_crop: NormRect,
    pub template_size: DebugTemplateSize,
    pub threshold: f64,
    pub found: bool,
    pub x: f64,
    pub y: f64,
    pub score: f64,
    pub best: Option<ServantGridFaceMatch>,
    pub anchors: Vec<ServantGridAnchor>,
    pub reference_anchor: Option<ServantGridAnchor>,
    pub grid_cells: Vec<ServantGridCell>,
    pub matches: Vec<ServantGridFaceMatch>,
    pub diagnostics: crate::screen::ServantGridDiagnostics,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DebugTemplateSize {
    pub w: u32,
    pub h: u32,
}

fn face_template_stage(path: &std::path::Path) -> u32 {
    path.file_stem()
        .and_then(|n| n.to_str())
        .and_then(|n| n.strip_prefix("face_servant_"))
        .and_then(|n| n.parse::<u32>().ok())
        .unwrap_or(0)
}

fn list_face_templates_desc(servant_dir: &std::path::Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(servant_dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .map(|name| name.starts_with("face_servant_") && name.ends_with(".png"))
                .unwrap_or(false)
        })
        .collect();
    paths.sort_by(|a, b| face_template_stage(b).cmp(&face_template_stage(a)));
    paths
}

/// Run the enhancement servant-select face matcher against the most recent
/// debug screenshot for one servant id. This mirrors the production
/// enhancement runner crop/search region and returns every face template's
/// score so the crop can be tuned from the UI.
#[tauri::command]
pub fn debug_find_enhancement_servant(
    app: tauri::AppHandle,
    server_state: tauri::State<'_, Mutex<Server>>,
    debug_state: tauri::State<'_, DebugSidecar>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    ce_enhancement_handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    friend_point_summon_handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
    servant_id: u32,
    threshold: Option<f64>,
) -> Result<DebugEnhancementServantMatchResult, String> {
    require_automation_idle(
        &handle_state,
        &enhancement_handle_state,
        &ce_enhancement_handle_state,
        &friend_point_summon_handle_state,
    )?;

    let image_path = debug_image_path(&app);
    if !image_path.exists() {
        return Err("尚未截取画面，请先点击 截取画面".into());
    }

    let assets_dir = resolve_servant_assets_dir(&app)
        .ok_or_else(|| "未找到从者资源目录，无法进行头像匹配".to_string())?;
    let servant_dir = assets_dir.join(servant_id.to_string());
    let templates = list_face_templates_desc(&servant_dir);
    if templates.is_empty() {
        return Err(format!(
            "从者 #{servant_id} 缺少 face_servant_*.png: {}",
            servant_dir.display()
        ));
    }

    let threshold = threshold.unwrap_or(0.85);
    ensure_debug_sidecar(&app, &debug_state, current_server(&server_state))?;

    let mut guard = debug_state.0.lock().unwrap();
    let client = guard
        .as_mut()
        .ok_or_else(|| "debug sidecar not initialized".to_string())?;
    let result: FindEnhancementServantGridResult = client.find_enhancement_servant_grid(
        Some(&image_path),
        &templates,
        SERVANT_LIST_REGION,
        SERVANT_FACE_MATCH_CROP,
        Some(SERVANT_FACE_TEMPLATE_SIZE),
        threshold,
        0.0,
    )?;
    eprintln!(
        "[debug_find_enhancement_servant] servant_id={} anchors={} cells={} best={:.3}",
        servant_id,
        result.anchors.len(),
        result.grid_cells.len(),
        result.score,
    );
    Ok(DebugEnhancementServantMatchResult {
        servant_id,
        search_region: SERVANT_LIST_REGION,
        template_crop: SERVANT_FACE_MATCH_CROP,
        template_size: DebugTemplateSize {
            w: SERVANT_FACE_TEMPLATE_SIZE.0,
            h: SERVANT_FACE_TEMPLATE_SIZE.1,
        },
        threshold,
        found: result.found,
        x: result.x,
        y: result.y,
        score: result.score,
        best: result.best,
        anchors: result.anchors,
        reference_anchor: result.reference_anchor,
        grid_cells: result.grid_cells,
        matches: result.matches,
        diagnostics: result.diagnostics,
    })
}

mod battle;
pub use battle::*;
mod support;
pub(crate) use support::*;

/// List every servant id under ``assets/servants/`` that has at least one
/// ``card_servant_*.png`` file. The Debug UI uses this to populate the
/// candidate-id picker without the user having to know what ships.
#[tauri::command]
pub fn debug_list_servant_assets(app: tauri::AppHandle) -> Vec<u32> {
    let Some(dir) = resolve_servant_assets_dir(&app) else {
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
        let Ok(id) = name.parse::<u32>() else {
            continue;
        };

        let mut has_face = false;
        if let Ok(inner) = fs::read_dir(&path) {
            for f in inner.flatten() {
                let fname = f.file_name();
                let Some(fname) = fname.to_str() else {
                    continue;
                };
                if fname.starts_with("card_servant_") && fname.to_lowercase().ends_with(".png") {
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
/// Debug page doesn't pay the 20-40s PyInstaller cold-boot cost. Idempotent
/// while automation is idle. The frontend should invoke this on app startup
/// (or on Debug page mount) to move the cold-boot off the critical path.
///
/// This intentionally does *not* start the scrcpy stream -- streaming would
/// conflict with a running automation session and is started lazily by
/// `debug_capture` itself.
#[tauri::command]
pub fn warm_sidecar(
    app: tauri::AppHandle,
    server_state: tauri::State<'_, Mutex<Server>>,
    debug_state: tauri::State<'_, DebugSidecar>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    ce_enhancement_handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    friend_point_summon_handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
) -> Result<(), String> {
    require_automation_idle(
        &handle_state,
        &enhancement_handle_state,
        &ce_enhancement_handle_state,
        &friend_point_summon_handle_state,
    )?;
    ensure_debug_sidecar(&app, &debug_state, current_server(&server_state))
}
