mod adb;
mod debug;
mod runner;
mod screen;

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, OnceLock};
use tauri::Manager;

use runner::{RunConfig, RunnerHandle, RunnerState};

// ---------------------------------------------------------------------------
// scrcpy stream tunables. ``STREAM_MAX_SIZE = 0`` means "do not downscale";
// the device transmits at native resolution. Bit rate is the H.264 budget.
// ---------------------------------------------------------------------------
pub(crate) const STREAM_MAX_SIZE: u32 = 0;
pub(crate) const STREAM_BIT_RATE: u32 = 8_000_000;

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
        #[serde(default)]
        target: Option<String>,
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
    /// Pinned support-select servant id. The runner's `handle_support_select`
    /// reads this through `RunConfig::support_servant_id` to drive the OCR
    /// detector. `None` means the user hasn't pinned anyone yet, in which
    /// case the runner falls back to tapping the topmost visible support.
    /// `#[serde(default)]` so legacy `projects.json` rows without the field
    /// continue to deserialize.
    #[serde(default)]
    pub support_servant_id: Option<u32>,
}

pub(crate) fn app_data_dir(app: &tauri::AppHandle) -> PathBuf {
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
        support_servant_id: None,
    };
    let mut projects = read_projects(&app);
    projects.push(project.clone());
    write_projects(&app, &projects)?;
    Ok(project)
}

/// Replace the stored project entry whose ``id`` matches ``project.id`` with
/// the supplied value. Used by the team-builder support slot to persist the
/// pinned servant id without a dedicated single-field setter (so future
/// project-level fields don't each need their own command).
#[tauri::command]
fn update_project(app: tauri::AppHandle, project: Project) -> Result<Project, String> {
    let mut projects = read_projects(&app);
    let Some(slot) = projects.iter_mut().find(|p| p.id == project.id) else {
        return Err(format!("project not found: {}", project.id));
    };
    *slot = project.clone();
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

/// Subset of `assets/servants/{id}/servant.json` needed by the OCR-based
/// support detector: the servant's primary name and every Noble Phantasm
/// name. The frontend uses this to seed `find_supports` from a chosen
/// servant id without shipping the full Atlas Academy blob over IPC.
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ServantMetadata {
    pub id: u32,
    pub name: String,
    pub np_names: Vec<String>,
}

/// Parse and cache the (id, name, np_names) triple for one servant.
///
/// Reads `<servant_assets_dir>/{id}/servant.json` (the Atlas Academy dump
/// committed under `src-tauri/assets/servants/`), pulls the top-level
/// `name` field plus every entry of `noblePhantasms[].name`. Cached in a
/// process-wide `OnceLock<Mutex<HashMap>>` so repeat lookups (e.g. the
/// debug page calling `find_supports` repeatedly) are free.
pub(crate) fn load_servant_metadata(
    app: &tauri::AppHandle,
    id: u32,
) -> Result<ServantMetadata, String> {
    static CACHE: OnceLock<Mutex<HashMap<u32, ServantMetadata>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    {
        let map = cache.lock().unwrap();
        if let Some(meta) = map.get(&id) {
            return Ok(meta.clone());
        }
    }

    let assets_dir = resolve_servant_assets_dir(app)
        .ok_or_else(|| "未找到 servant 资源目录 (src-tauri/assets/servants/)".to_string())?;
    let path = assets_dir.join(id.to_string()).join("servant.json");
    let raw = fs::read_to_string(&path).map_err(|e| {
        format!("无法读取 servant.json ({}): {e}", path.display())
    })?;
    let json: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|e| format!("servant.json 解析失败 ({}): {e}", path.display()))?;
    let name = json
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("servant.json 缺少 'name' 字段: {}", path.display()))?
        .to_string();

    // Deduplicate while preserving discovery order: a few servants list the
    // same NP under multiple `num` overcharge tiers and we only want the
    // distinct names for fuzzy matching.
    let mut np_names: Vec<String> = Vec::new();
    if let Some(arr) = json.get("noblePhantasms").and_then(|v| v.as_array()) {
        for entry in arr {
            if let Some(n) = entry.get("name").and_then(|v| v.as_str()) {
                let trimmed = n.trim();
                if !trimmed.is_empty() && !np_names.iter().any(|x| x == trimmed) {
                    np_names.push(trimmed.to_string());
                }
            }
        }
    }

    let meta = ServantMetadata { id, name, np_names };
    cache.lock().unwrap().insert(id, meta.clone());
    Ok(meta)
}

#[tauri::command]
fn get_servant_metadata(
    app: tauri::AppHandle,
    id: u32,
) -> Result<ServantMetadata, String> {
    load_servant_metadata(&app, id)
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
        .start_stream(
            &jar_path,
            serial.as_deref(),
            STREAM_MAX_SIZE,
            STREAM_BIT_RATE,
        )
        .map_err(|e| format!("启动 scrcpy 视频流失败: {e}"))?;
    let screen_size = Some((w, h));

    let state = Arc::new(Mutex::new(RunnerState::Running));
    let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));

    let mut handle = handle_state.lock().unwrap();
    handle.state = state.clone();
    handle.cancel = cancel.clone();

    let assets_dir = resolve_servant_assets_dir(&app);
    let runner = runner::Runner::new(
        adb_dev, sidecar, config, turns, app, state, cancel, screen_size, assets_dir,
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
// Shared resource-path resolvers (used by both automation + debug paths)
// ---------------------------------------------------------------------------

/// Resolve the bundled templates directory. In dev and prod this lives under
/// the app's resource_dir (declared in tauri.conf.json > bundle.resources).
pub(crate) fn resolve_templates_dir(app: &tauri::AppHandle) -> Option<PathBuf> {
    let base = app.path().resource_dir().ok()?;
    Some(base.join("resources").join("templates"))
}

/// Resolve the bundled cv.json path.
pub(crate) fn resolve_cv_config_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    let base = app.path().resource_dir().ok()?;
    Some(base.join("resources").join("cv.json"))
}

/// Resolve the bundled scrcpy-server.jar path.
pub(crate) fn resolve_scrcpy_jar(app: &tauri::AppHandle) -> Option<PathBuf> {
    let base = app.path().resource_dir().ok()?;
    Some(base.join("resources").join("scrcpy").join("scrcpy-server.jar"))
}

/// Resolve the per-servant assets directory (containing
/// `{servant_id}/card_servant_*.png`). This is the `servants/` subtree
/// of the broader `assets/` tree (which also holds `ces/` for craft
/// essences). The dir is intentionally NOT bundled into the app yet
/// (production bundling is a future decision); in dev we read it
/// directly from the source tree.
///
/// Lookup order:
/// 1. `<resource_dir>/assets/servants/` — present once the user opts to bundle it.
/// 2. `<CARGO_MANIFEST_DIR>/assets/servants/` — the dev-time source location.
///
/// Returns ``None`` if neither exists; callers should treat that as
/// "no per-servant identification available" rather than an error.
pub(crate) fn resolve_servant_assets_dir(app: &tauri::AppHandle) -> Option<PathBuf> {
    if let Ok(base) = app.path().resource_dir() {
        let bundled = base.join("assets").join("servants");
        if bundled.is_dir() {
            return Some(bundled);
        }
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join("servants");
    if dev.is_dir() {
        return Some(dev);
    }
    None
}

/// Resolve the bundled mash-cv sidecar executable path. The sidecar is shipped
/// as a PyInstaller --onedir directory under `binaries/mash-cv/` (containing
/// the executable and a sibling `_internal/` directory).
pub(crate) fn resolve_sidecar_exe(app: &tauri::AppHandle) -> Option<PathBuf> {
    let base = app.path().resource_dir().ok()?;
    let exe_name = if cfg!(windows) { "mash-cv.exe" } else { "mash-cv" };
    Some(base.join("binaries").join("mash-cv").join(exe_name))
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
            app.manage(debug::DebugSidecar(Mutex::new(None)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_servants,
            save_turns,
            load_turns,
            list_projects,
            create_project,
            update_project,
            delete_project,
            check_adb,
            get_use_bluestack,
            set_use_bluestack,
            start_automation,
            stop_automation,
            get_automation_status,
            debug::debug_capture,
            debug::debug_find_element,
            debug::debug_find_element_by_name,
            debug::debug_list_templates,
            debug::debug_get_cv_config,
            debug::debug_reload_sidecar,
            debug::debug_shutdown,
            debug::debug_get_runner_coordinates,
            debug::debug_find_command_cards,
            debug::debug_find_noble_phantasms,
            debug::debug_find_supports,
            debug::debug_list_servant_assets,
            debug::warm_sidecar,
            get_servant_metadata,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
