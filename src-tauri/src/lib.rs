mod adb;
mod runner;
mod screen;

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, OnceLock};
use tauri::Manager;

use runner::{RunConfig, RunnerHandle, RunnerState};
use screen::{ElementMatch, NormRect, SidecarClient};

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum Action {
    #[serde(rename = "servant")]
    Servant {
        id: String,
        servant: Option<String>,
        skill: Option<String>,
        target: Option<String>,
    },
    #[serde(rename = "equipment")]
    Equipment {
        id: String,
        skill: Option<String>,
    },
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct AttackCard {
    pub id: String,
    pub card: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct Turn {
    pub id: String,
    #[serde(rename = "servantActions")]
    pub servant_actions: Vec<Action>,
    #[serde(rename = "equipmentActions")]
    pub equipment_actions: Vec<Action>,
    #[serde(rename = "attackPriority")]
    pub attack_priority: Vec<AttackCard>,
}

// ---------------------------------------------------------------------------
// Project system
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
}

fn app_data_dir(app: &tauri::AppHandle) -> PathBuf {
    let dir = app.path().app_data_dir().expect("failed to resolve app data dir");
    fs::create_dir_all(&dir).ok();
    dir
}

fn projects_file_path(app: &tauri::AppHandle) -> PathBuf {
    app_data_dir(app).join("projects.json")
}

fn project_turns_path(app: &tauri::AppHandle, project_id: &str) -> PathBuf {
    let dir = app_data_dir(app).join("projects").join(project_id);
    fs::create_dir_all(&dir).ok();
    dir.join("turns.json")
}

fn read_projects(app: &tauri::AppHandle) -> Vec<Project> {
    let path = projects_file_path(app);
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_projects(app: &tauri::AppHandle, projects: &[Project]) -> Result<(), String> {
    let path = projects_file_path(app);
    let json = serde_json::to_string_pretty(projects).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_projects(app: tauri::AppHandle) -> Vec<Project> {
    read_projects(&app)
}

#[tauri::command]
fn create_project(app: tauri::AppHandle, name: String) -> Result<Project, String> {
    let project = Project {
        id: uuid::Uuid::new_v4().to_string(),
        name,
    };
    let mut projects = read_projects(&app);
    projects.push(project.clone());
    write_projects(&app, &projects)?;
    Ok(project)
}

#[tauri::command]
fn delete_project(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let mut projects = read_projects(&app);
    projects.retain(|p| p.id != id);
    write_projects(&app, &projects)?;
    let dir = app_data_dir(&app).join("projects").join(&id);
    if dir.exists() {
        let _ = fs::remove_dir_all(&dir);
    }
    Ok(())
}

#[tauri::command]
fn save_turns(app: tauri::AppHandle, project_id: String, turns: Vec<Turn>) -> Result<(), String> {
    let path = project_turns_path(&app, &project_id);
    let json = serde_json::to_string_pretty(&turns).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())
}

#[tauri::command]
fn load_turns(app: tauri::AppHandle, project_id: String) -> Vec<Turn> {
    let path = project_turns_path(&app, &project_id);
    match fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

#[derive(serde::Serialize, Clone)]
struct ServantInfo {
    id: u32,
    name_cn: String,
    name_jp: String,
    name_en: String,
    name_other: Option<String>,
    class: String,
    rarity: u32,
}

fn servants_data() -> &'static [ServantInfo] {
    static SERVANTS: OnceLock<Vec<ServantInfo>> = OnceLock::new();
    SERVANTS.get_or_init(|| {
        let raw: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("resources/servants.json"))
                .expect("invalid servants.json");

        raw.iter()
            .enumerate()
            .filter_map(|(idx, s)| {
                let result = (|| {
                    let id = s.get("id")?.as_u64()? as u32;
                    let name_cn = s.get("name_cn")?.as_str()?.to_string();
                    let name_jp = s.get("name_jp")?.as_str()?.to_string();
                    let name_en = s.get("name_en")?.as_str()?.to_string();
                    let name_other = s
                        .get("name_other")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    let base_stats = s.get("base_stats")?.as_object()?;

                    let (class, rarity) = if let Some(cls) = base_stats.get("class") {
                        let class = cls.as_str()?.to_string();
                        let rarity = base_stats.get("rarity")?.as_u64()? as u32;
                        (class, rarity)
                    } else {
                        let first_variant = base_stats.values().next()?.as_object()?;
                        let class = first_variant.get("class")?.as_str()?.to_string();
                        let rarity = first_variant.get("rarity")?.as_u64()? as u32;
                        (class, rarity)
                    };

                    Some(ServantInfo {
                        id,
                        name_cn,
                        name_jp,
                        name_en,
                        name_other,
                        class,
                        rarity,
                    })
                })();

                if result.is_none() {
                    let id_hint = s.get("id").and_then(|v| v.as_u64());
                    eprintln!(
                        "[servants] dropping entry at index {idx} (id={id_hint:?}): missing or invalid fields"
                    );
                }
                result
            })
            .collect()
    })
}

#[tauri::command]
fn get_servants() -> &'static [ServantInfo] {
    servants_data()
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AdbStatus {
    connected: bool,
    device_name: Option<String>,
}

fn adb_settings_path(app: &tauri::AppHandle) -> PathBuf {
    let dir = app.path().app_data_dir().expect("failed to resolve app data dir");
    fs::create_dir_all(&dir).ok();
    dir.join("adb_settings.json")
}

fn load_bluestack_setting(app: &tauri::AppHandle) -> bool {
    let path = adb_settings_path(app);
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("useBluestack")?.as_bool())
        .unwrap_or(false)
}

fn parse_first_ready_device(output: &str) -> Option<String> {
    output.lines().skip(1).find_map(|line| {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }
        let mut parts = trimmed.split('\t');
        let serial = parts.next()?.trim();
        let status = parts.next()?.trim();
        if status == "device" {
            Some(serial.to_string())
        } else {
            None
        }
    })
}

#[tauri::command]
fn get_use_bluestack(state: tauri::State<'_, Mutex<bool>>) -> bool {
    *state.lock().unwrap()
}

#[tauri::command]
fn set_use_bluestack(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<bool>>,
    value: bool,
) -> Result<(), String> {
    *state.lock().unwrap() = value;
    let path = adb_settings_path(&app);
    let json = serde_json::json!({ "useBluestack": value });
    fs::write(&path, serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn check_adb(state: tauri::State<'_, Mutex<bool>>) -> AdbStatus {
    let use_bluestack = *state.lock().unwrap();
    if use_bluestack {
        std::process::Command::new("adb")
            .args(["connect", "127.0.0.1:5555"])
            .output()
            .ok();
    }

    let device_name = std::process::Command::new("adb")
        .arg("devices")
        .output()
        .ok()
        .and_then(|out| parse_first_ready_device(&String::from_utf8_lossy(&out.stdout)));

    AdbStatus {
        connected: device_name.is_some(),
        device_name,
    }
}

#[tauri::command]
fn start_automation(
    app: tauri::AppHandle,
    config: RunConfig,
    bluestack_state: tauri::State<'_, Mutex<bool>>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
) -> Result<(), String> {
    let is_running = {
        let state = handle_state.lock().unwrap().state.clone();
        let running = matches!(*state.lock().unwrap(), RunnerState::Running);
        running
    };
    if is_running {
        return Err("自动化正在运行中".into());
    }

    let turns = load_turns(app.clone(), config.project_id.clone());

    let use_bluestack = *bluestack_state.lock().unwrap();

    let mut adb_dev = adb::Adb::new(use_bluestack);
    adb_dev.connect()?;
    let serial = adb_dev.serial().map(|s| s.to_string());

    let jar_path = resolve_scrcpy_jar(&app)
        .ok_or_else(|| "找不到 scrcpy-server.jar 资源".to_string())?;
    if !jar_path.exists() {
        return Err(format!(
            "scrcpy-server.jar 不存在: {}",
            jar_path.display()
        ));
    }

    let templates_dir = resolve_templates_dir(&app);
    let cv_config = resolve_cv_config_path(&app);
    let mut sidecar = screen::SidecarClient::spawn(
        &app,
        templates_dir.as_deref(),
        cv_config.as_deref(),
    )?;

    let (w, h) = sidecar
        .start_stream(&jar_path, serial.as_deref(), 0, 8_000_000)
        .map_err(|e| format!("启动 scrcpy 视频流失败: {e}"))?;
    let screen_size = Some((w, h));

    let state = Arc::new(Mutex::new(RunnerState::Running));
    let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));

    let mut handle = handle_state.lock().unwrap();
    handle.state = state.clone();
    handle.cancel = cancel.clone();

    let runner = runner::Runner::new(
        adb_dev, sidecar, config, turns, app, state, cancel, screen_size,
    );
    std::thread::spawn(move || runner.run());

    Ok(())
}

#[tauri::command]
fn stop_automation(handle_state: tauri::State<'_, Mutex<RunnerHandle>>) -> Result<(), String> {
    let handle = handle_state.lock().unwrap();
    handle.cancel.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
fn get_automation_status(
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
) -> RunnerState {
    let handle = handle_state.lock().unwrap();
    let state = handle.state.lock().unwrap().clone();
    state
}

// ---------------------------------------------------------------------------
// Debug: CV probe
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
struct DebugCaptureResult {
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

/// Resolve the bundled templates directory. In dev and prod this lives under
/// the app's resource_dir (declared in tauri.conf.json > bundle.resources).
fn resolve_templates_dir(app: &tauri::AppHandle) -> Option<PathBuf> {
    let base = app.path().resource_dir().ok()?;
    Some(base.join("resources").join("templates"))
}

/// Resolve the bundled cv.json path.
fn resolve_cv_config_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    let base = app.path().resource_dir().ok()?;
    Some(base.join("resources").join("cv.json"))
}

/// Resolve the bundled scrcpy-server.jar path.
fn resolve_scrcpy_jar(app: &tauri::AppHandle) -> Option<PathBuf> {
    let base = app.path().resource_dir().ok()?;
    Some(base.join("resources").join("scrcpy").join("scrcpy-server.jar"))
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

    // Cheap no-op probe: get_frame will fail if the stream isn't running.
    if client.get_frame_jpeg().is_ok() {
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
    let (w, h) = client.start_stream(&jar, serial, 0, 8_000_000)?;
    eprintln!("[debug] scrcpy stream started: {w}x{h}");
    Ok(())
}

#[tauri::command]
fn debug_capture(
    app: tauri::AppHandle,
    bluestack_state: tauri::State<'_, Mutex<bool>>,
    debug_state: tauri::State<'_, DebugSidecar>,
) -> Result<DebugCaptureResult, String> {
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

    let (jpeg, screen, score) = {
        let mut guard = debug_state.0.lock().unwrap();
        let client = guard
            .as_mut()
            .ok_or_else(|| "debug sidecar not initialized".to_string())?;

        // Retry briefly: the first frame may still be in flight right after
        // start_stream returns.
        let mut jpeg: Option<Vec<u8>> = None;
        for _ in 0..20 {
            match client.get_frame_jpeg() {
                Ok(buf) => {
                    jpeg = Some(buf);
                    break;
                }
                Err(e) => {
                    eprintln!("[debug_capture] get_frame retry: {e}");
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            }
        }
        let jpeg = jpeg.ok_or_else(|| "未获取到视频帧".to_string())?;

        fs::write(&dest, &jpeg).map_err(|e| {
            eprintln!("[debug_capture] write frame failed: {e}");
            format!("failed to write frame: {e}")
        })?;
        let (screen, score) = client.detect_full(Some(&dest)).map_err(|e| {
            eprintln!("[debug_capture] detect failed: {e}");
            e
        })?;
        (jpeg, screen, score)
    };

    let size_bytes = jpeg.len();
    eprintln!(
        "[debug_capture] frame saved: {} ({} bytes), screen={screen} score={score:.3}",
        dest.display(),
        size_bytes,
    );

    // Screen size comes from the scrcpy stream metadata captured during
    // start_stream; we don't have a direct getter yet, so decode dimensions
    // from the JPEG header via a lightweight probe.
    let screen_size = probe_jpeg_size(&jpeg).map(|(w, h)| DebugScreenSize { w, h });

    Ok(DebugCaptureResult {
        image_path: dest.to_string_lossy().to_string(),
        screen: screen.to_string(),
        score,
        screen_size,
    })
}

/// Minimal JPEG SOF scanner to recover dimensions without pulling in an image
/// crate. Returns (width, height) or None on malformed input.
fn probe_jpeg_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return None;
    }
    let mut i = 2;
    while i + 3 < bytes.len() {
        if bytes[i] != 0xFF {
            return None;
        }
        let marker = bytes[i + 1];
        i += 2;
        // Standalone markers (no length)
        if marker == 0xD8 || marker == 0xD9 {
            return None;
        }
        let seg_len = u16::from_be_bytes([bytes[i], bytes[i + 1]]) as usize;
        if seg_len < 2 || i + seg_len > bytes.len() {
            return None;
        }
        // SOF0..SOF3, SOF5..SOF7, SOF9..SOF11, SOF13..SOF15 carry dimensions.
        let is_sof = matches!(
            marker,
            0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF
        );
        if is_sof && seg_len >= 7 {
            let h = u16::from_be_bytes([bytes[i + 3], bytes[i + 4]]) as u32;
            let w = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]) as u32;
            return Some((w, h));
        }
        i += seg_len;
    }
    None
}

#[tauri::command]
fn debug_find_element(
    app: tauri::AppHandle,
    debug_state: tauri::State<'_, DebugSidecar>,
    template_key: String,
    region: Option<NormRect>,
    threshold: Option<f64>,
) -> Result<ElementMatch, String> {
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
fn debug_list_templates(app: tauri::AppHandle) -> Vec<String> {
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
fn debug_find_element_by_name(
    app: tauri::AppHandle,
    debug_state: tauri::State<'_, DebugSidecar>,
    screen: String,
    element: String,
) -> Result<ElementMatch, String> {
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
fn debug_get_cv_config(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    let path = resolve_cv_config_path(&app)
        .ok_or_else(|| "resource_dir not available".to_string())?;
    let contents = fs::read_to_string(&path).map_err(|e| {
        eprintln!("[debug_get_cv_config] read {} failed: {e}", path.display());
        format!("读取 cv.json 失败: {e}")
    })?;
    serde_json::from_str(&contents).map_err(|e| format!("解析 cv.json 失败: {e}"))
}

#[tauri::command]
fn debug_reload_sidecar(
    app: tauri::AppHandle,
    debug_state: tauri::State<'_, DebugSidecar>,
) -> Result<(), String> {
    {
        let mut guard = debug_state.0.lock().unwrap();
        guard.take();
    }
    ensure_debug_sidecar(&app, &debug_state)
}

#[tauri::command]
fn debug_shutdown(debug_state: tauri::State<'_, DebugSidecar>) -> Result<(), String> {
    let mut guard = debug_state.0.lock().unwrap();
    guard.take();
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let use_bluestack = load_bluestack_setting(&app.handle());
            app.manage(Mutex::new(use_bluestack));
            app.manage(Mutex::new(RunnerHandle::new_idle()));
            app.manage(DebugSidecar(Mutex::new(None)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_servants,
            save_turns,
            load_turns,
            list_projects,
            create_project,
            delete_project,
            check_adb,
            get_use_bluestack,
            set_use_bluestack,
            start_automation,
            stop_automation,
            get_automation_status,
            debug_capture,
            debug_find_element,
            debug_find_element_by_name,
            debug_list_templates,
            debug_get_cv_config,
            debug_reload_sidecar,
            debug_shutdown,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
