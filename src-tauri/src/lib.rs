mod adb;
mod debug;
mod runner;
mod screen;

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
            debug::warm_sidecar,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
