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

mod capture;
mod session;
pub use capture::*;

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

mod battle;
pub use battle::*;
mod enhancement;
pub use enhancement::*;
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
