mod adb;
mod debug;
mod enhancement_runner;
mod runner;
mod screen;

use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, OnceLock};
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;
use zip::ZipArchive;

use enhancement_runner::{
    server_supported as enhancement_server_supported, EnhancementConfig, EnhancementRunner,
    EnhancementRunnerHandle, EnhancementRunnerState, EnhancementTarget,
};
use runner::{RunConfig, RunnerHandle, RunnerState};

// ---------------------------------------------------------------------------
// Server selection (global app setting). Drives which template/config bundle
// the sidecar loads, which OCR model RapidOCR pins, and how
// `load_servant_metadata` localizes the servant + NP names it sends into
// `find_supports`. Default is JP because that's the only data the project
// originally shipped — flipping the default here would break every existing
// install whose templates assume Japanese UI text.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Server {
    Jp,
    Cn,
}

impl Default for Server {
    fn default() -> Self {
        Self::Jp
    }
}

impl Server {
    /// Lowercase directory token used under `resources/servers/{token}/...`.
    /// Kept intentionally tiny so the resource resolvers stay one-liners.
    pub fn dir_token(&self) -> &'static str {
        match self {
            Self::Jp => "jp",
            Self::Cn => "cn",
        }
    }
}

impl fmt::Display for Server {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Jp => write!(f, "JP"),
            Self::Cn => write!(f, "CN"),
        }
    }
}

impl FromStr for Server {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_uppercase().as_str() {
            "JP" => Ok(Self::Jp),
            "CN" => Ok(Self::Cn),
            other => Err(format!("unknown server: {other}")),
        }
    }
}

// ---------------------------------------------------------------------------
// scrcpy stream tunables. Keep the live frame below native emulator
// resolution so PyAV decode + OpenCV matching stay responsive on M1-class
// laptops. Bit rate is the H.264 budget.
// ---------------------------------------------------------------------------
pub(crate) const STREAM_MAX_SIZE: u32 = 1280;
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
    #[serde(rename = "commandSpell")]
    CommandSpell {
        id: String,
        spell: Option<String>,
        #[serde(default)]
        target: Option<String>,
    },
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct AttackCard {
    pub id: String,
    pub card: Option<String>,
}

/// One configured battle-scene block. The runner picks which block to
/// execute by reading the `BATTLE m/n` HUD strip and indexing on `m - 1`,
/// so each block represents the per-scene action plan rather than a
/// per-turn one. JSON shape on the wire is unchanged from the legacy
/// `Turn` struct so old `turns.json` files can be migrated in place.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct BattleScene {
    pub id: String,
    #[serde(rename = "preparationActions", default)]
    pub preparation_actions: Vec<Action>,
    #[serde(rename = "servantActions", default, skip_serializing)]
    pub servant_actions: Vec<Action>,
    #[serde(rename = "equipmentActions", default, skip_serializing)]
    pub equipment_actions: Vec<Action>,
    /// Per-scene Command Spell taps (令咒). Optional for backwards
    /// compatibility: legacy `battle_scenes.json` files written before
    /// this field was added deserialize with an empty list.
    #[serde(rename = "commandSpellActions", default, skip_serializing)]
    pub command_spell_actions: Vec<Action>,
    #[serde(rename = "attackPriority")]
    pub attack_priority: Vec<AttackCard>,
}

impl BattleScene {
    fn normalize_preparation_actions(mut self) -> Self {
        if self.preparation_actions.is_empty() {
            self.preparation_actions
                .extend(self.servant_actions.iter().cloned());
            self.preparation_actions
                .extend(self.equipment_actions.iter().cloned());
            self.preparation_actions
                .extend(self.command_spell_actions.iter().cloned());
        }
        self.servant_actions.clear();
        self.equipment_actions.clear();
        self.command_spell_actions.clear();
        self
    }
}

// ---------------------------------------------------------------------------
// Project system
// ---------------------------------------------------------------------------

/// One cell of the team-builder grid. The frontend stores six of these per
/// project (5 servant slots + 1 support slot) along with their order, so
/// drag-and-drop layouts and chosen servants survive across sessions.
///
/// `kind` is either `"servant"` or `"support"`. For support slots,
/// `servant_id` is ignored — the pinned servant lives on
/// `Project::support_servant_id` (kept separate because the runner reads
/// it through `RunConfig::support_servant_id` and we don't want two
/// sources of truth for the same value).
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSlot {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub servant_id: Option<u32>,
    /// Optional UI variant key for servants with multiple gameplay
    /// variants. The runner still acts on the base servant id; this
    /// keeps the team builder showing the exact variant the user picked.
    #[serde(default)]
    pub servant_variant_key: Option<String>,
    /// Pinned craft-essence id for this slot. Persisted alongside the
    /// servant so each loadout can carry its own equipment plan; for
    /// support slots the runner uses this to verify candidate rows on
    /// the support-select screen, party slots store it for future use.
    #[serde(default)]
    pub craft_essence_id: Option<u32>,
}

/// Default 6-slot layout used both when creating a fresh project and when
/// deserializing a legacy `projects.json` that predates the `slots` field.
fn default_project_slots() -> Vec<ProjectSlot> {
    let new_slot = |id: &str, kind: &str| ProjectSlot {
        id: id.into(),
        kind: kind.into(),
        servant_id: None,
        servant_variant_key: None,
        craft_essence_id: None,
    };
    vec![
        new_slot("slot-0", "servant"),
        new_slot("slot-1", "servant"),
        new_slot("slot-2", "support"),
        new_slot("slot-3", "servant"),
        new_slot("slot-4", "servant"),
        new_slot("slot-5", "servant"),
    ]
}

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
    #[serde(default)]
    pub support_servant_variant_key: Option<String>,
    /// Team-builder grid layout (chosen servants + slot order). Persisted
    /// so the user's selections survive app restarts and project switches.
    /// Defaulted via `default_project_slots` for legacy rows.
    #[serde(default = "default_project_slots")]
    pub slots: Vec<ProjectSlot>,
    /// When `true`, the runner taps "Next" on the post-battle continue
    /// screen so the same quest is queued again; when `false`, it taps
    /// "Close" and the run terminates. `#[serde(default)]` keeps legacy
    /// rows (no field) defaulting to `false` = single-run behaviour.
    #[serde(default)]
    pub repeat_mission: bool,
}

pub(crate) fn app_data_dir(app: &tauri::AppHandle) -> PathBuf {
    let dir = app
        .path()
        .app_data_dir()
        .expect("failed to resolve app data dir");
    fs::create_dir_all(&dir).ok();
    dir
}

fn app_assets_dir(app: &tauri::AppHandle) -> PathBuf {
    let dir = app_data_dir(app).join("assets");
    fs::create_dir_all(&dir).ok();
    dir
}

fn projects_file_path(app: &tauri::AppHandle) -> PathBuf {
    app_data_dir(app).join("projects.json")
}

fn project_battle_scenes_path(app: &tauri::AppHandle, project_id: &str) -> PathBuf {
    let dir = app_data_dir(app).join("projects").join(project_id);
    fs::create_dir_all(&dir).ok();
    dir.join("battle_scenes.json")
}

/// Legacy filename used before the per-scene rename. Kept around so
/// `load_battle_scenes` can migrate any pre-existing project data on
/// first launch after the rename.
fn legacy_project_turns_path(app: &tauri::AppHandle, project_id: &str) -> PathBuf {
    app_data_dir(app)
        .join("projects")
        .join(project_id)
        .join("turns.json")
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
        support_servant_variant_key: None,
        slots: default_project_slots(),
        repeat_mission: false,
    };
    let mut projects = read_projects(&app);
    projects.push(project.clone());
    write_projects(&app, &projects)?;
    Ok(project)
}

fn copy_project_dir(src: &Path, dst: &Path) -> Result<(), String> {
    if !src.exists() {
        return Ok(());
    }
    fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let entry_src = entry.path();
        let entry_dst = dst.join(entry.file_name());
        if entry_src.is_dir() {
            copy_project_dir(&entry_src, &entry_dst)?;
        } else {
            fs::copy(&entry_src, &entry_dst).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[tauri::command]
fn duplicate_project(
    app: tauri::AppHandle,
    source_id: String,
    name: String,
) -> Result<Project, String> {
    let mut projects = read_projects(&app);
    let source = projects
        .iter()
        .find(|p| p.id == source_id)
        .cloned()
        .ok_or_else(|| format!("project not found: {source_id}"))?;
    let project = Project {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        ..source
    };
    projects.push(project.clone());
    write_projects(&app, &projects)?;

    let projects_dir = app_data_dir(&app).join("projects");
    copy_project_dir(
        &projects_dir.join(&source_id),
        &projects_dir.join(&project.id),
    )?;
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
fn save_battle_scenes(
    app: tauri::AppHandle,
    project_id: String,
    scenes: Vec<BattleScene>,
) -> Result<(), String> {
    let path = project_battle_scenes_path(&app, &project_id);
    let scenes: Vec<BattleScene> = scenes
        .into_iter()
        .map(BattleScene::normalize_preparation_actions)
        .collect();
    let json = serde_json::to_string_pretty(&scenes).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())
}

#[tauri::command]
fn load_battle_scenes(app: tauri::AppHandle, project_id: String) -> Vec<BattleScene> {
    let path = project_battle_scenes_path(&app, &project_id);
    if let Ok(contents) = fs::read_to_string(&path) {
        return serde_json::from_str::<Vec<BattleScene>>(&contents)
            .unwrap_or_default()
            .into_iter()
            .map(BattleScene::normalize_preparation_actions)
            .collect();
    }

    // One-shot migration: pre-rename projects stored their per-scene
    // config under `turns.json`. The on-disk JSON shape is identical
    // (BattleScene was just renamed from Turn), so we can read it as-is,
    // write it under the new filename, and remove the legacy file.
    let legacy = legacy_project_turns_path(&app, &project_id);
    if let Ok(contents) = fs::read_to_string(&legacy) {
        let scenes: Vec<BattleScene> = serde_json::from_str::<Vec<BattleScene>>(&contents)
            .unwrap_or_default()
            .into_iter()
            .map(BattleScene::normalize_preparation_actions)
            .collect();
        if let Ok(json) = serde_json::to_string_pretty(&scenes) {
            let _ = fs::write(&path, json);
        }
        let _ = fs::remove_file(&legacy);
        return scenes;
    }

    Vec::new()
}

#[derive(serde::Serialize, Clone)]
struct ServantInfo {
    id: u32,
    #[serde(rename = "variantKey")]
    variant_key: String,
    #[serde(rename = "faceId")]
    face_id: Option<u32>,
    name_cn: String,
    name_jp: String,
    name_en: String,
    name_other: Option<String>,
    class: String,
    rarity: u32,
    #[serde(rename = "noblePhantasmName")]
    noble_phantasm_name: Option<String>,
}

fn load_enhancement_target(
    app: &tauri::AppHandle,
    variant_key: &str,
) -> Result<EnhancementTarget, String> {
    let servant = servants_data()
        .iter()
        .find(|s| s.variant_key == variant_key)
        .ok_or_else(|| format!("未找到目标从者 variantKey: {variant_key}"))?;
    let root = resolve_servant_assets_dir(app)
        .ok_or_else(|| "未找到从者资源目录，无法进行头像匹配".to_string())?;
    let servant_dir = root.join(servant.id.to_string());
    let face_template_paths = pick_faces_desc_in(&servant_dir);
    if face_template_paths.is_empty() {
        return Err(format!(
            "缺少从者头像模板资源，无法在无名称列表中自动选择: {} ({})",
            servant.name_jp, servant.variant_key
        ));
    }
    Ok(EnhancementTarget {
        id: servant.id,
        variant_key: servant.variant_key.clone(),
        name_jp: servant.name_jp.clone(),
        class_name: servant.class.clone(),
        rarity: servant.rarity,
        face_template_paths,
    })
}

fn first_np_name(s: &serde_json::Value) -> Option<String> {
    let nps = s.get("noble_phantasms")?;
    let entry = if let Some(arr) = nps.as_array() {
        arr.first()?
    } else {
        nps.as_object()?.values().next()?
    };
    entry
        .get("name_cn")
        .or_else(|| entry.get("name"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn last_variant_np_name(variant: &serde_json::Value) -> Option<String> {
    let arr = variant
        .get("noblePhantasms_cn")
        .or_else(|| variant.get("noblePhantasms"))?
        .as_array()?;
    arr.iter()
        .rev()
        .find_map(|v| v.as_str().map(str::trim).filter(|s| !s.is_empty()))
        .map(str::to_string)
}

fn variant_face_id(variant: &serde_json::Value) -> Option<u32> {
    variant
        .get("ids")?
        .as_array()?
        .iter()
        .filter_map(|v| v.as_u64().map(|n| n as u32))
        .max()
}

fn servants_data() -> &'static [ServantInfo] {
    static SERVANTS: OnceLock<Vec<ServantInfo>> = OnceLock::new();
    SERVANTS.get_or_init(|| {
        let raw: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("resources/servants.json"))
                .expect("invalid servants.json");
        let variants_raw: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("resources/servants_variants.json"))
                .expect("invalid servants_variants.json");
        let variants_by_id: HashMap<u32, Vec<serde_json::Value>> = variants_raw
            .into_iter()
            .filter_map(|entry| {
                let id = entry.get("id")?.as_u64()? as u32;
                let variants = entry.get("variants")?.as_array()?.clone();
                Some((id, variants))
            })
            .collect();

        raw.iter()
            .enumerate()
            .flat_map(|(idx, s)| {
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

                    let base_np = first_np_name(s);
                    let variants = variants_by_id.get(&id);
                    let infos: Vec<ServantInfo> = if let Some(variants) = variants {
                        variants
                            .iter()
                            .enumerate()
                            .map(|(variant_idx, variant)| ServantInfo {
                                id,
                                variant_key: format!("{id}:{}", variant_idx + 1),
                                face_id: variant_face_id(variant),
                                name_cn: name_cn.clone(),
                                name_jp: name_jp.clone(),
                                name_en: name_en.clone(),
                                name_other: name_other.clone(),
                                class: class.clone(),
                                rarity,
                                noble_phantasm_name: last_variant_np_name(variant)
                                    .or_else(|| base_np.clone()),
                            })
                            .collect()
                    } else {
                        vec![ServantInfo {
                            id,
                            variant_key: id.to_string(),
                            face_id: None,
                            name_cn,
                            name_jp,
                            name_en,
                            name_other,
                            class,
                            rarity,
                            noble_phantasm_name: base_np,
                        }]
                    };
                    Some(infos)
                })();

                if result.is_none() {
                    let id_hint = s.get("id").and_then(|v| v.as_u64());
                    eprintln!(
                        "[servants] dropping entry at index {idx} (id={id_hint:?}): missing or invalid fields"
                    );
                }
                result.unwrap_or_default()
            })
            .collect()
    })
}

#[tauri::command]
fn get_servants() -> &'static [ServantInfo] {
    servants_data()
}

/// One craft-essence entry exposed to the frontend. Mirrors the shape of
/// `resources/craft_essences.json` (which only ships `id`, `name`, and a
/// wiki link). Kept minimal — additional metadata lives in the per-CE
/// `assets/ces/{id}/craft-essence.json` Atlas dump and is loaded lazily
/// only when the runner actually needs it.
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CraftEssenceInfo {
    pub id: u32,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_link: Option<String>,
}

fn craft_essences_data() -> &'static [CraftEssenceInfo] {
    static CES: OnceLock<Vec<CraftEssenceInfo>> = OnceLock::new();
    CES.get_or_init(|| {
        let raw: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("resources/craft_essences.json"))
                .expect("invalid craft_essences.json");
        raw.iter()
            .enumerate()
            .filter_map(|(idx, ce)| {
                let result = (|| {
                    let id = ce.get("id")?.as_u64()? as u32;
                    let name = ce.get("name")?.as_str()?.to_string();
                    let name_link = ce
                        .get("name_link")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    Some(CraftEssenceInfo { id, name, name_link })
                })();
                if result.is_none() {
                    let id_hint = ce.get("id").and_then(|v| v.as_u64());
                    eprintln!(
                        "[craft_essences] dropping entry at index {idx} (id={id_hint:?}): missing or invalid fields"
                    );
                }
                result
            })
            .collect()
    })
}

#[tauri::command]
fn get_craft_essences() -> &'static [CraftEssenceInfo] {
    craft_essences_data()
}

/// Look inside a single servant's asset directory and return the path of
/// the highest-numbered `narrow_servant_*.png` portrait, or `None` when
/// no such file exists. `narrow_servant_<n>.png` corresponds to
/// ascension stage `n` (1-4 for typical servants, with `4` being the
/// final art); picking the lexicographic max is a stable proxy for
/// "most-recent ascension" since the source filenames are
/// zero-prefix-free single digits.
///
/// Pure helper so [`get_servant_portrait_path`] stays a thin wrapper and
/// the file-walk logic is unit-testable without spinning up a
/// `tauri::AppHandle`.
fn pick_portrait_in(servant_dir: &std::path::Path) -> Option<PathBuf> {
    let entries = fs::read_dir(servant_dir).ok()?;
    entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("narrow_servant_") && n.ends_with(".png"))
                .unwrap_or(false)
        })
        .max()
}

fn pick_portrait_by_id_in(servant_dir: &std::path::Path, portrait_id: u32) -> Option<PathBuf> {
    let candidate = servant_dir.join(format!("narrow_servant_{portrait_id}.png"));
    if candidate.is_file() {
        Some(candidate)
    } else {
        None
    }
}

fn pick_face_in(servant_dir: &std::path::Path) -> Option<PathBuf> {
    pick_faces_desc_in(servant_dir).into_iter().next()
}

fn pick_faces_desc_in(servant_dir: &std::path::Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(servant_dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| is_face_template_path(p))
        .collect();
    paths.sort_by(|a, b| face_template_stage(b).cmp(&face_template_stage(a)));
    paths
}

fn is_face_template_path(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with("face_servant_") && n.ends_with(".png"))
        .unwrap_or(false)
}

fn face_template_stage(path: &std::path::Path) -> u32 {
    path.file_stem()
        .and_then(|n| n.to_str())
        .and_then(|n| n.strip_prefix("face_servant_"))
        .and_then(|n| n.parse::<u32>().ok())
        .unwrap_or(0)
}

fn pick_face_by_id_in(servant_dir: &std::path::Path, face_id: u32) -> Option<PathBuf> {
    let candidate = servant_dir.join(format!("face_servant_{face_id}.png"));
    if candidate.is_file() {
        Some(candidate)
    } else {
        None
    }
}

/// Resolve the full-art portrait file for a single servant, returning
/// the absolute path so the frontend can hand it to `convertFileSrc()`.
///
/// Lookup chain mirrors [`resolve_servant_assets_dir`] (bundled
/// `<resource_dir>/assets/servants/` first, dev-time
/// `CARGO_MANIFEST_DIR/assets/servants/` second), then narrows to
/// `{servant_id}/narrow_servant_*.png` and picks the highest ascension
/// stage via [`pick_portrait_in`]. Returning `Ok(None)` (rather than an
/// `Err`) on a missing file lets the UI fall back to a placeholder
/// card without surfacing a scary error toast — a portrait being
/// absent is the expected default state for most servants today.
#[tauri::command]
fn get_servant_portrait_path(
    app: tauri::AppHandle,
    servant_id: u32,
    face_id: Option<u32>,
) -> Result<Option<String>, String> {
    let Some(root) = resolve_servant_assets_dir(&app) else {
        return Ok(None);
    };
    let servant_dir = root.join(servant_id.to_string());
    let picked = face_id
        .and_then(|id| pick_portrait_by_id_in(&servant_dir, id))
        .or_else(|| pick_portrait_in(&servant_dir));
    Ok(picked.map(|p| p.to_string_lossy().into_owned()))
}

#[tauri::command]
fn get_servant_face_path(
    app: tauri::AppHandle,
    servant_id: u32,
    face_id: Option<u32>,
) -> Result<Option<String>, String> {
    let Some(root) = resolve_servant_assets_dir(&app) else {
        return Ok(None);
    };
    let servant_dir = root.join(servant_id.to_string());
    let picked = face_id
        .and_then(|id| pick_face_by_id_in(&servant_dir, id))
        .or_else(|| pick_face_in(&servant_dir));
    Ok(picked.map(|p| p.to_string_lossy().into_owned()))
}

/// Pure helper so [`get_craft_essence_card_path`] stays a thin wrapper
/// and the file-walk logic is unit-testable without spinning up a
/// `tauri::AppHandle`. Returns `Some(path)` when
/// `<ce_root>/<id>/card_ce.png` exists, `None` otherwise.
fn pick_ce_card_in(ce_root: &std::path::Path, ce_id: u32) -> Option<PathBuf> {
    let candidate = ce_root.join(ce_id.to_string()).join("card_ce.png");
    if candidate.is_file() {
        Some(candidate)
    } else {
        None
    }
}

/// Resolve the card art for a single craft essence, returning the
/// absolute path so the frontend can hand it to `convertFileSrc()`.
///
/// Mirrors [`get_servant_portrait_path`] but for the CE asset tree
/// (`assets/ces/{id}/card_ce.png`). The single-file layout means there
/// is no glob/pick-highest step — the file either exists or it
/// doesn't. Returning `Ok(None)` (rather than an `Err`) on a missing
/// file keeps the empty-state placeholder a normal render path instead
/// of an error toast.
#[tauri::command]
fn get_craft_essence_card_path(
    app: tauri::AppHandle,
    craft_essence_id: u32,
) -> Result<Option<String>, String> {
    let Some(root) = resolve_ce_assets_dir(&app) else {
        return Ok(None);
    };
    Ok(pick_ce_card_in(&root, craft_essence_id).map(|p| p.to_string_lossy().into_owned()))
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
    /// Atlas Academy `className`, lowercased (e.g. `caster`, `alterego`,
    /// `mooncancer`). Drives the support-select class-tab tap so the
    /// runner only OCRs the filtered list instead of "all + mix".
    pub class_name: String,
}

/// NFKC + whitespace-normalize a Japanese string so two cosmetically
/// different forms (full-width vs. half-width punctuation, stray spaces
/// from a manual data dump, etc.) hash to the same key. Mirrors the
/// `_normalize_jp_text` pre-pass the sidecar runs on OCR output before
/// fuzzy matching, so the JP→CN bridge here lines up with what the
/// sidecar will actually see.
fn normalize_jp_key(s: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    s.nfkc()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("")
}

/// Process-wide `name_jp -> name_cn` map for every servant Noble
/// Phantasm we know about, built once from `resources/servants.json`.
/// Handles both shapes the source file uses:
///   - flat list:           `noble_phantasms: [ {...}, ... ]`
///   - dict-of-variants:    `noble_phantasms: { "初始": [...], "奥特瑙斯": [...] }`
/// Drops entries whose JP name is missing, blank, or whose CN
/// counterpart is the same as the JP name (a common placeholder when
/// the localizer hasn't filled in the translation). Used only when the
/// active server is `Server::Cn` to translate Atlas JP NP names into
/// the strings the OCR will actually see on a CN client.
fn np_jp_to_cn_index() -> &'static HashMap<String, String> {
    static INDEX: OnceLock<HashMap<String, String>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let raw: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("resources/servants.json"))
                .expect("invalid servants.json");

        let mut map: HashMap<String, String> = HashMap::new();
        let mut consider = |entry: &serde_json::Value| {
            let jp = entry
                .get("name_jp")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            let cn = entry
                .get("name_cn")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if jp.is_empty() || cn.is_empty() || jp == cn {
                return;
            }
            let key = normalize_jp_key(jp);
            if key.is_empty() {
                return;
            }
            map.entry(key).or_insert_with(|| cn.to_string());
        };

        for s in raw.iter() {
            let Some(nps) = s.get("noble_phantasms") else {
                continue;
            };
            if let Some(arr) = nps.as_array() {
                for entry in arr {
                    consider(entry);
                }
            } else if let Some(obj) = nps.as_object() {
                for (_variant, value) in obj {
                    if let Some(arr) = value.as_array() {
                        for entry in arr {
                            consider(entry);
                        }
                    }
                }
            }
        }
        map
    })
}

/// Process-wide `name_jp -> name_cn` map for the servants themselves
/// (as opposed to their NPs). Built once from `servants_data()`. Same
/// drop rule as the NP index: skip entries whose CN field is missing or
/// identical to the JP one.
fn servant_jp_to_cn_index() -> &'static HashMap<String, String> {
    static INDEX: OnceLock<HashMap<String, String>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let mut map: HashMap<String, String> = HashMap::new();
        for s in servants_data() {
            let jp = s.name_jp.trim();
            let cn = s.name_cn.trim();
            if jp.is_empty() || cn.is_empty() || jp == cn {
                continue;
            }
            let key = normalize_jp_key(jp);
            if key.is_empty() {
                continue;
            }
            map.entry(key).or_insert_with(|| cn.to_string());
        }
        map
    })
}

/// Translate one Atlas JP servant name into the string the CN client
/// renders. Falls back to the JP name if no mapping exists so OCR still
/// has *some* target to fuzzy-match against rather than no name at all.
fn localize_servant_name(jp: &str) -> String {
    let key = normalize_jp_key(jp);
    servant_jp_to_cn_index()
        .get(&key)
        .cloned()
        .unwrap_or_else(|| jp.to_string())
}

/// Translate Atlas JP NP names into their CN equivalents. **Drops**
/// unmapped NPs (the data-quality gap the user plans to fix later);
/// the sidecar's `_find_supports` falls back to name-only matching when
/// the resulting list is empty. Preserves input order and dedupes after
/// translation in case two JP entries map to the same CN string.
fn localize_np_names(jp_names: &[String]) -> Vec<String> {
    let index = np_jp_to_cn_index();
    let mut out: Vec<String> = Vec::new();
    for jp in jp_names {
        let key = normalize_jp_key(jp);
        if let Some(cn) = index.get(&key) {
            if !out.iter().any(|existing| existing == cn) {
                out.push(cn.clone());
            }
        }
    }
    out
}

/// Parse and cache the (id, name, np_names) triple for one servant.
///
/// Reads `<servant_assets_dir>/{id}/servant.json` (the Atlas Academy dump
/// committed under `src-tauri/assets/servants/`), pulls the top-level
/// `name` field plus every entry of `noblePhantasms[].name`. When
/// `server == Server::Cn`, both the servant name and every NP name are
/// translated through `resources/servants.json` (`name_jp -> name_cn`).
/// Unmapped NPs are dropped — when the resulting `np_names` list is
/// empty the sidecar transparently falls back to name-only matching, so
/// support detection still proceeds at lower precision.
///
/// Cached in a process-wide `OnceLock<Mutex<HashMap<(u32, Server), _>>>`
/// so repeat lookups (e.g. the debug page calling `find_supports`
/// repeatedly) are free, and so JP and CN entries for the same servant
/// id never clobber each other.
pub(crate) fn load_servant_metadata(
    app: &tauri::AppHandle,
    id: u32,
    server: Server,
) -> Result<ServantMetadata, String> {
    static CACHE: OnceLock<Mutex<HashMap<(u32, Server), ServantMetadata>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    {
        let map = cache.lock().unwrap();
        if let Some(meta) = map.get(&(id, server)) {
            return Ok(meta.clone());
        }
    }

    let assets_dir = resolve_servant_assets_dir(app)
        .ok_or_else(|| "未找到 servant 资源目录 (src-tauri/assets/servants/)".to_string())?;
    let path = assets_dir.join(id.to_string()).join("servant.json");
    let raw = fs::read_to_string(&path)
        .map_err(|e| format!("无法读取 servant.json ({}): {e}", path.display()))?;
    let json: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|e| format!("servant.json 解析失败 ({}): {e}", path.display()))?;
    let name_jp = json
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("servant.json 缺少 'name' 字段: {}", path.display()))?
        .to_string();
    // Atlas dump uses camelCase like "alterEgo" / "moonCancer"; lowercase
    // here so the runner's class-tab map can use simple lowercase keys.
    let class_name = json
        .get("className")
        .and_then(|v| v.as_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();

    // Deduplicate while preserving discovery order: a few servants list the
    // same NP under multiple `num` overcharge tiers and we only want the
    // distinct names for fuzzy matching.
    let mut np_names_jp: Vec<String> = Vec::new();
    if let Some(arr) = json.get("noblePhantasms").and_then(|v| v.as_array()) {
        for entry in arr {
            if let Some(n) = entry.get("name").and_then(|v| v.as_str()) {
                let trimmed = n.trim();
                if !trimmed.is_empty() && !np_names_jp.iter().any(|x| x == trimmed) {
                    np_names_jp.push(trimmed.to_string());
                }
            }
        }
    }

    let (name, np_names) = match server {
        Server::Jp => (name_jp, np_names_jp),
        Server::Cn => {
            let cn_name = localize_servant_name(&name_jp);
            let cn_nps = localize_np_names(&np_names_jp);
            let dropped = np_names_jp.len().saturating_sub(cn_nps.len());
            if dropped > 0 {
                eprintln!(
                    "[load_servant_metadata] servant {id}: dropped {dropped} unmapped NP name(s) for CN (will fall back to name-only matching if all dropped)"
                );
            }
            (cn_name, cn_nps)
        }
    };

    let meta = ServantMetadata {
        id,
        name,
        np_names,
        class_name,
    };
    cache.lock().unwrap().insert((id, server), meta.clone());
    Ok(meta)
}

#[tauri::command]
fn get_servant_metadata(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<Server>>,
    id: u32,
) -> Result<ServantMetadata, String> {
    let server = *state.lock().unwrap();
    load_servant_metadata(&app, id, server)
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AdbStatus {
    connected: bool,
    device_name: Option<String>,
}

fn adb_settings_path(app: &tauri::AppHandle) -> PathBuf {
    let dir = app
        .path()
        .app_data_dir()
        .expect("failed to resolve app data dir");
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

fn server_settings_path(app: &tauri::AppHandle) -> PathBuf {
    let dir = app
        .path()
        .app_data_dir()
        .expect("failed to resolve app data dir");
    fs::create_dir_all(&dir).ok();
    dir.join("server_settings.json")
}

fn load_server_setting(app: &tauri::AppHandle) -> Server {
    let path = server_settings_path(app);
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| {
            v.get("server")
                .and_then(|s| s.as_str())
                .map(|s| s.to_string())
        })
        .and_then(|s| Server::from_str(&s).ok())
        .unwrap_or_default()
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
    fs::write(
        &path,
        serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_server(state: tauri::State<'_, Mutex<Server>>) -> Server {
    *state.lock().unwrap()
}

#[tauri::command]
fn set_server(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<Server>>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    debug_state: tauri::State<'_, debug::DebugSidecar>,
    value: Server,
) -> Result<(), String> {
    // Refuse to flip mid-run: the runner cached templates / OCR model /
    // localized servant metadata for the *previous* server when it spawned;
    // changing the global setting now would silently desync those caches.
    {
        let handle = handle_state.lock().unwrap();
        let running = matches!(*handle.state.lock().unwrap(), RunnerState::Running);
        if running {
            return Err("自动化正在运行中，请先停止后再切换服务器".into());
        }
    }
    {
        let handle = enhancement_handle_state.lock().unwrap();
        let running = matches!(
            *handle.state.lock().unwrap(),
            EnhancementRunnerState::Running
        );
        if running {
            return Err("强化自动化正在运行中，请先停止后再切换服务器".into());
        }
    }

    *state.lock().unwrap() = value;
    let path = server_settings_path(&app);
    let json = serde_json::json!({ "server": value.to_string() });
    fs::write(
        &path,
        serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    // Tear down any idle sidecar so the next debug or automation call
    // respawns it with the new server's templates / OCR model.
    {
        let mut guard = debug_state.0.lock().unwrap();
        guard.take();
    }

    Ok(())
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

pub(crate) fn spawn_configured_sidecar(
    app: &tauri::AppHandle,
    server: Server,
) -> Result<screen::SidecarClient, String> {
    let templates_dir = resolve_templates_dir(app, server);
    let cv_config = resolve_cv_config_path(app, server);
    screen::SidecarClient::spawn(app, templates_dir.as_deref(), cv_config.as_deref(), server)
}

fn take_or_spawn_sidecar(
    app: &tauri::AppHandle,
    shared_sidecar: &debug::DebugSidecar,
    server: Server,
) -> Result<screen::SidecarClient, String> {
    if let Some(client) = shared_sidecar.0.lock().unwrap().take() {
        eprintln!("[mash-cv] reusing cached sidecar");
        return Ok(client);
    }
    spawn_configured_sidecar(app, server)
}

fn input_size_for_taps(adb_size: Option<(u32, u32)>, stream_size: (u32, u32)) -> (u32, u32) {
    let Some((adb_w, adb_h)) = adb_size else {
        return stream_size;
    };
    let (stream_w, stream_h) = stream_size;
    let adb_landscape = adb_w >= adb_h;
    let stream_landscape = stream_w >= stream_h;
    if adb_landscape == stream_landscape {
        (adb_w, adb_h)
    } else {
        (adb_h, adb_w)
    }
}

#[tauri::command]
fn start_automation(
    app: tauri::AppHandle,
    config: RunConfig,
    bluestack_state: tauri::State<'_, Mutex<bool>>,
    server_state: tauri::State<'_, Mutex<Server>>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    debug_state: tauri::State<'_, debug::DebugSidecar>,
) -> Result<(), String> {
    let is_running = {
        let state = handle_state.lock().unwrap().state.clone();
        let running = matches!(*state.lock().unwrap(), RunnerState::Running);
        running
    };
    if is_running {
        return Err("自动化正在运行中".into());
    }
    {
        let handle = enhancement_handle_state.lock().unwrap();
        let running = matches!(
            *handle.state.lock().unwrap(),
            EnhancementRunnerState::Running
        );
        if running {
            return Err("强化自动化正在运行中".into());
        }
    }

    let scenes = load_battle_scenes(app.clone(), config.project_id.clone());

    let use_bluestack = *bluestack_state.lock().unwrap();
    let server = *server_state.lock().unwrap();

    let mut adb_dev = adb::Adb::new(use_bluestack);
    adb_dev.connect()?;
    let serial = adb_dev.serial().map(|s| s.to_string());

    let jar_path =
        resolve_scrcpy_jar(&app).ok_or_else(|| "找不到 scrcpy-server.jar 资源".to_string())?;
    if !jar_path.exists() {
        return Err(format!("scrcpy-server.jar 不存在: {}", jar_path.display()));
    }

    let mut sidecar = take_or_spawn_sidecar(&app, &debug_state, server)?;

    let (w, h) = sidecar
        .start_stream(
            &jar_path,
            serial.as_deref(),
            STREAM_MAX_SIZE,
            STREAM_BIT_RATE,
        )
        .map_err(|e| format!("启动 scrcpy 视频流失败: {e}"))?;
    let input_size = input_size_for_taps(adb_dev.screen_size(), (w, h));
    if input_size != (w, h) {
        eprintln!(
            "[runner] using adb input size {}x{} with stream frame {}x{}",
            input_size.0, input_size.1, w, h
        );
    }
    let screen_size = Some(input_size);

    let state = Arc::new(Mutex::new(RunnerState::Running));
    let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop_after_current = Arc::new(std::sync::atomic::AtomicBool::new(false));

    let mut handle = handle_state.lock().unwrap();
    handle.state = state.clone();
    handle.cancel = cancel.clone();
    handle.stop_after_current = stop_after_current.clone();

    let assets_dir = resolve_servant_assets_dir(&app);
    let ce_assets_dir = resolve_ce_assets_dir(&app);
    let runner = runner::Runner::new(
        adb_dev,
        sidecar,
        config,
        scenes,
        app,
        state,
        cancel,
        stop_after_current,
        screen_size,
        assets_dir,
        ce_assets_dir,
        server,
        Some(debug_state.0.clone()),
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
fn stop_automation_after_current(
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
) -> Result<(), String> {
    let handle = handle_state.lock().unwrap();
    handle.stop_after_current.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
fn get_automation_status(handle_state: tauri::State<'_, Mutex<RunnerHandle>>) -> RunnerState {
    let handle = handle_state.lock().unwrap();
    let state = handle.state.lock().unwrap().clone();
    state
}

#[tauri::command]
fn start_enhancement_automation(
    app: tauri::AppHandle,
    config: EnhancementConfig,
    bluestack_state: tauri::State<'_, Mutex<bool>>,
    server_state: tauri::State<'_, Mutex<Server>>,
    battle_handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    debug_state: tauri::State<'_, debug::DebugSidecar>,
) -> Result<(), String> {
    {
        let handle = handle_state.lock().unwrap();
        let running = matches!(
            *handle.state.lock().unwrap(),
            EnhancementRunnerState::Running
        );
        if running {
            return Err("强化自动化正在运行中".into());
        }
    }
    {
        let handle = battle_handle_state.lock().unwrap();
        let running = matches!(*handle.state.lock().unwrap(), RunnerState::Running);
        if running {
            return Err("战斗自动化正在运行中，请先停止".into());
        }
    }

    let use_bluestack = *bluestack_state.lock().unwrap();
    let server = *server_state.lock().unwrap();
    if !enhancement_server_supported(server) {
        return Err("当前仅支持日服强化自动化".into());
    }

    let target = load_enhancement_target(&app, &config.target_servant_variant_key)?;
    if target.id != config.target_servant_id {
        return Err("目标从者 id 与 variantKey 不匹配".into());
    }

    let mut adb_dev = adb::Adb::new(use_bluestack);
    adb_dev.connect()?;
    let serial = adb_dev.serial().map(|s| s.to_string());

    let jar_path =
        resolve_scrcpy_jar(&app).ok_or_else(|| "找不到 scrcpy-server.jar 资源".to_string())?;
    if !jar_path.exists() {
        return Err(format!("scrcpy-server.jar 不存在: {}", jar_path.display()));
    }

    let mut sidecar = take_or_spawn_sidecar(&app, &debug_state, server)?;
    let (w, h) = sidecar
        .start_stream(
            &jar_path,
            serial.as_deref(),
            STREAM_MAX_SIZE,
            STREAM_BIT_RATE,
        )
        .map_err(|e| format!("启动 scrcpy 视频流失败: {e}"))?;
    let input_size = input_size_for_taps(adb_dev.screen_size(), (w, h));
    if input_size != (w, h) {
        eprintln!(
            "[enhancement] using adb input size {}x{} with stream frame {}x{}",
            input_size.0, input_size.1, w, h
        );
    }

    let state = Arc::new(Mutex::new(EnhancementRunnerState::Running));
    let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut handle = handle_state.lock().unwrap();
    handle.state = state.clone();
    handle.cancel = cancel.clone();

    let runner = EnhancementRunner::new(
        adb_dev,
        sidecar,
        app,
        state,
        cancel,
        input_size,
        target,
        Some(debug_state.0.clone()),
    );
    std::thread::spawn(move || runner.run());
    Ok(())
}

#[tauri::command]
fn stop_enhancement_automation(
    handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
) -> Result<(), String> {
    let handle = handle_state.lock().unwrap();
    handle.cancel.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
fn get_enhancement_automation_status(
    handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
) -> EnhancementRunnerState {
    let handle = handle_state.lock().unwrap();
    let state = handle.state.lock().unwrap().clone();
    state
}

// ---------------------------------------------------------------------------
// mash-cv runtime management
// ---------------------------------------------------------------------------

const RUNTIME_MANIFEST_JSON: &str = include_str!("../resources/runtime-manifest.json");
const RUNTIME_DIR_NAME: &str = "mash-cv";

#[derive(serde::Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct RuntimeManifest {
    mash_cv_runtime_version: String,
    mash_cv_code_version: String,
    platforms: HashMap<String, RuntimePlatformArtifact>,
}

#[derive(serde::Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct RuntimePlatformArtifact {
    runtime_url: String,
    runtime_sha256: String,
    code_url: String,
    code_sha256: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct RuntimeVersionRecord {
    version: String,
    platform: String,
}

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct RuntimeStatus {
    required_runtime_version: String,
    installed_runtime_version: Option<String>,
    runtime_installed: bool,
    required_code_version: String,
    installed_code_version: Option<String>,
    code_installed: bool,
    installed: bool,
    platform: String,
    runtime_download_url: Option<String>,
    runtime_expected_sha256: Option<String>,
    runtime_install_dir: String,
    executable_path: String,
    code_download_url: Option<String>,
    code_expected_sha256: Option<String>,
    code_install_dir: String,
    code_path: String,
}

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct RuntimeInstallResult {
    installed_kind: String,
    installed_version: String,
    platform: String,
    install_dir: String,
    executable_path: Option<String>,
    code_path: Option<String>,
}

fn runtime_platform_key() -> String {
    runtime_platform_key_from(std::env::consts::OS, std::env::consts::ARCH)
}

fn runtime_platform_key_from(os: &str, arch: &str) -> String {
    let os = match os {
        "macos" => "darwin",
        other => other,
    };
    format!("{os}-{arch}")
}

fn runtime_exe_name() -> &'static str {
    if cfg!(windows) {
        "mash-cv.exe"
    } else {
        "mash-cv"
    }
}

fn runtime_bundle_root() -> &'static str {
    "mash-cv-runtime"
}

fn code_bundle_root() -> &'static str {
    "mash-cv-code"
}

fn parse_runtime_manifest(contents: &str) -> Result<RuntimeManifest, String> {
    serde_json::from_str(contents).map_err(|e| format!("解析 runtime manifest 失败: {e}"))
}

fn runtime_manifest(app: &tauri::AppHandle) -> Result<RuntimeManifest, String> {
    let contents = app
        .path()
        .resource_dir()
        .ok()
        .map(|base| base.join("resources").join("runtime-manifest.json"))
        .and_then(|path| fs::read_to_string(path).ok())
        .unwrap_or_else(|| RUNTIME_MANIFEST_JSON.to_string());
    parse_runtime_manifest(&contents)
}

fn runtime_root_dir(app: &tauri::AppHandle) -> PathBuf {
    let dir = app_data_dir(app).join("runtime").join(RUNTIME_DIR_NAME);
    fs::create_dir_all(&dir).ok();
    dir
}

fn runtime_base_root(runtime_root: &Path) -> PathBuf {
    runtime_root.join("runtime")
}

fn runtime_code_root(runtime_root: &Path) -> PathBuf {
    runtime_root.join("code")
}

fn runtime_version_path(runtime_root: &Path) -> PathBuf {
    runtime_root.join("runtime").join("runtime-version.json")
}

fn code_version_path(runtime_root: &Path) -> PathBuf {
    runtime_root.join("code").join("code-version.json")
}

fn runtime_version_dir(runtime_root: &Path, version: &str) -> PathBuf {
    runtime_base_root(runtime_root).join(version)
}

fn code_version_dir(runtime_root: &Path, version: &str) -> PathBuf {
    runtime_code_root(runtime_root).join(version)
}

fn runtime_executable_path(runtime_root: &Path, version: &str) -> PathBuf {
    runtime_version_dir(runtime_root, version)
        .join(runtime_bundle_root())
        .join(runtime_exe_name())
}

fn read_runtime_version(runtime_root: &Path) -> Option<RuntimeVersionRecord> {
    fs::read_to_string(runtime_version_path(runtime_root))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}

fn read_code_version(runtime_root: &Path) -> Option<RuntimeVersionRecord> {
    fs::read_to_string(code_version_path(runtime_root))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}

fn runtime_code_path(runtime_root: &Path, version: &str) -> PathBuf {
    code_version_dir(runtime_root, version).join(code_bundle_root())
}

fn runtime_models_path(runtime_root: &Path, version: &str) -> PathBuf {
    runtime_version_dir(runtime_root, version)
        .join(runtime_bundle_root())
        .join("_internal")
        .join("mash_cv")
        .join("models")
}

fn runtime_status_from_manifest(
    manifest: &RuntimeManifest,
    runtime_root: &Path,
    platform: &str,
) -> RuntimeStatus {
    let installed_runtime_version = read_runtime_version(runtime_root).map(|record| record.version);
    let installed_code_version = read_code_version(runtime_root).map(|record| record.version);
    let executable = runtime_executable_path(runtime_root, &manifest.mash_cv_runtime_version);
    let code_path = runtime_code_path(runtime_root, &manifest.mash_cv_code_version);
    let artifact = manifest.platforms.get(platform);
    let runtime_installed = installed_runtime_version.as_deref()
        == Some(manifest.mash_cv_runtime_version.as_str())
        && executable.is_file();
    let code_installed = installed_code_version.as_deref()
        == Some(manifest.mash_cv_code_version.as_str())
        && code_path.join("mash_cv").is_dir();

    RuntimeStatus {
        required_runtime_version: manifest.mash_cv_runtime_version.clone(),
        installed_runtime_version,
        runtime_installed,
        required_code_version: manifest.mash_cv_code_version.clone(),
        installed_code_version,
        code_installed,
        installed: runtime_installed && code_installed,
        platform: platform.to_string(),
        runtime_download_url: artifact.map(|item| item.runtime_url.clone()),
        runtime_expected_sha256: artifact.map(|item| item.runtime_sha256.clone()),
        runtime_install_dir: runtime_version_dir(runtime_root, &manifest.mash_cv_runtime_version)
            .to_string_lossy()
            .into_owned(),
        executable_path: executable.to_string_lossy().into_owned(),
        code_download_url: artifact.map(|item| item.code_url.clone()),
        code_expected_sha256: artifact.map(|item| item.code_sha256.clone()),
        code_install_dir: code_version_dir(runtime_root, &manifest.mash_cv_code_version)
            .to_string_lossy()
            .into_owned(),
        code_path: code_path.to_string_lossy().into_owned(),
    }
}

fn runtime_status_for_app(app: &tauri::AppHandle) -> Result<RuntimeStatus, String> {
    let manifest = runtime_manifest(app)?;
    let platform = runtime_platform_key();
    let runtime_root = runtime_root_dir(app);
    Ok(runtime_status_from_manifest(
        &manifest,
        &runtime_root,
        &platform,
    ))
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| format!("无法打开 runtime 压缩包: {e}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|e| format!("读取 runtime 压缩包失败: {e}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn zip_has_valid_root(relative: &Path, root: &str) -> bool {
    relative.components().next().and_then(|part| match part {
        std::path::Component::Normal(name) => name.to_str(),
        _ => None,
    }) == Some(root)
}

#[cfg(unix)]
fn make_runtime_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)
        .map_err(|e| format!("读取 runtime 可执行权限失败: {e}"))?
        .permissions();
    permissions.set_mode(permissions.mode() | 0o755);
    fs::set_permissions(path, permissions).map_err(|e| format!("设置 runtime 可执行权限失败: {e}"))
}

#[cfg(not(unix))]
fn make_runtime_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

fn extract_zip_with_root(zip_path: &Path, destination: &Path, root: &str) -> Result<(), String> {
    let file = fs::File::open(zip_path).map_err(|e| format!("无法打开 runtime 压缩包: {e}"))?;
    let mut archive =
        ZipArchive::new(file).map_err(|e| format!("无法读取 runtime zip 压缩包: {e}"))?;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| format!("读取 runtime zip 条目失败: {e}"))?;
        let Some(relative) = entry.enclosed_name().map(|p| p.to_path_buf()) else {
            return Err("runtime zip 内包含非法路径".to_string());
        };
        if !zip_has_valid_root(&relative, root) {
            return Err(format!("runtime zip 必须以 {root}/ 作为根目录"));
        }

        let output = destination.join(relative);
        if entry.name().ends_with('/') {
            fs::create_dir_all(&output).map_err(|e| format!("创建 runtime 目录失败: {e}"))?;
            continue;
        }
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建 runtime 目录失败: {e}"))?;
        }
        let mut out =
            fs::File::create(&output).map_err(|e| format!("写入 runtime 文件失败: {e}"))?;
        io::copy(&mut entry, &mut out).map_err(|e| format!("解压 runtime 文件失败: {e}"))?;
        out.flush()
            .map_err(|e| format!("写入 runtime 文件失败: {e}"))?;
    }

    Ok(())
}

fn extract_runtime_zip(zip_path: &Path, destination: &Path) -> Result<(), String> {
    extract_zip_with_root(zip_path, destination, runtime_bundle_root())?;
    let exe = destination
        .join(runtime_bundle_root())
        .join(runtime_exe_name());
    if !exe.is_file() {
        return Err(format!(
            "runtime zip 内未找到 {}/{}",
            runtime_bundle_root(),
            runtime_exe_name()
        ));
    }
    make_runtime_executable(&exe)?;
    Ok(())
}

fn extract_code_zip(zip_path: &Path, destination: &Path) -> Result<(), String> {
    extract_zip_with_root(zip_path, destination, code_bundle_root())?;
    let package = destination.join(code_bundle_root()).join("mash_cv");
    if !package.is_dir() {
        return Err(format!("code zip 内未找到 {}/mash_cv/", code_bundle_root()));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RuntimeBundleKind {
    Runtime,
    Code,
}

fn import_runtime_bundle_from_zip_path(
    zip_path: &Path,
    manifest: &RuntimeManifest,
    runtime_root: &Path,
    platform: &str,
) -> Result<RuntimeInstallResult, String> {
    if !zip_path.is_file() {
        return Err("选择的 runtime 压缩包不存在".to_string());
    }
    let artifact = manifest
        .platforms
        .get(platform)
        .ok_or_else(|| format!("当前平台不支持独立 CV runtime: {platform}"))?;
    let actual_sha = sha256_file(zip_path)?;
    let kind = if actual_sha.eq_ignore_ascii_case(&artifact.runtime_sha256) {
        RuntimeBundleKind::Runtime
    } else if actual_sha.eq_ignore_ascii_case(&artifact.code_sha256) {
        RuntimeBundleKind::Code
    } else {
        return Err(format!(
            "runtime 压缩包 sha256 不匹配，期望 runtime={} 或 code={}，实际 {}",
            artifact.runtime_sha256, artifact.code_sha256, actual_sha
        ));
    };

    fs::create_dir_all(runtime_root).map_err(|e| format!("创建 runtime 目录失败: {e}"))?;
    let (kind_name, version, target, version_path) = match kind {
        RuntimeBundleKind::Runtime => (
            "runtime",
            manifest.mash_cv_runtime_version.as_str(),
            runtime_version_dir(runtime_root, &manifest.mash_cv_runtime_version),
            runtime_version_path(runtime_root),
        ),
        RuntimeBundleKind::Code => (
            "code",
            manifest.mash_cv_code_version.as_str(),
            code_version_dir(runtime_root, &manifest.mash_cv_code_version),
            code_version_path(runtime_root),
        ),
    };
    let staging = runtime_root.join(format!(
        ".install-{kind_name}-{version}-{}",
        uuid::Uuid::new_v4()
    ));
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|e| format!("清理 runtime 临时目录失败: {e}"))?;
    }
    fs::create_dir_all(&staging).map_err(|e| format!("创建 runtime 临时目录失败: {e}"))?;

    let extract_result = match kind {
        RuntimeBundleKind::Runtime => extract_runtime_zip(zip_path, &staging),
        RuntimeBundleKind::Code => extract_code_zip(zip_path, &staging),
    };
    if let Err(err) = extract_result {
        fs::remove_dir_all(&staging).ok();
        return Err(err);
    }

    if target.exists() {
        fs::remove_dir_all(&target).map_err(|e| format!("替换旧 runtime 失败: {e}"))?;
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建 runtime 目录失败: {e}"))?;
    }
    fs::rename(&staging, &target).map_err(|e| format!("安装 runtime 失败: {e}"))?;

    let record = RuntimeVersionRecord {
        version: version.to_string(),
        platform: platform.to_string(),
    };
    fs::write(
        version_path,
        serde_json::to_string_pretty(&record).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("写入 runtime 版本记录失败: {e}"))?;

    let exe = runtime_executable_path(runtime_root, &manifest.mash_cv_runtime_version);
    let code_path = runtime_code_path(runtime_root, &manifest.mash_cv_code_version);
    Ok(RuntimeInstallResult {
        installed_kind: kind_name.to_string(),
        installed_version: version.to_string(),
        platform: platform.to_string(),
        install_dir: target.to_string_lossy().into_owned(),
        executable_path: matches!(kind, RuntimeBundleKind::Runtime)
            .then(|| exe.to_string_lossy().into_owned()),
        code_path: matches!(kind, RuntimeBundleKind::Code)
            .then(|| code_path.to_string_lossy().into_owned()),
    })
}

#[tauri::command]
fn get_runtime_status(app: tauri::AppHandle) -> Result<RuntimeStatus, String> {
    runtime_status_for_app(&app)
}

#[tauri::command]
async fn pick_runtime_bundle(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let picked = app
        .dialog()
        .file()
        .add_filter("ZIP", &["zip"])
        .set_title("选择 CV runtime 压缩包")
        .blocking_pick_file();
    let Some(path) = picked else {
        return Ok(None);
    };
    let path = path.into_path().map_err(|e| e.to_string())?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

#[tauri::command]
async fn import_runtime_bundle(
    app: tauri::AppHandle,
    zip_path: String,
) -> Result<RuntimeInstallResult, String> {
    let manifest = runtime_manifest(&app)?;
    let platform = runtime_platform_key();
    import_runtime_bundle_from_zip_path(
        &PathBuf::from(zip_path),
        &manifest,
        &runtime_root_dir(&app),
        &platform,
    )
}

// ---------------------------------------------------------------------------
// Shared resource-path resolvers (used by both automation + debug paths)
// ---------------------------------------------------------------------------

/// Resolve the bundled templates directory for the given server. Each
/// server (JP/CN) ships its own subtree under
/// `resources/servers/{token}/templates/` so flipping the global server
/// setting hands the sidecar a different template set without touching
/// any JP fixture.
pub(crate) fn resolve_templates_dir(app: &tauri::AppHandle, server: Server) -> Option<PathBuf> {
    let base = app.path().resource_dir().ok()?;
    Some(
        base.join("resources")
            .join("servers")
            .join(server.dir_token())
            .join("templates"),
    )
}

/// Resolve the bundled cv.json path for the given server.
pub(crate) fn resolve_cv_config_path(app: &tauri::AppHandle, server: Server) -> Option<PathBuf> {
    let base = app.path().resource_dir().ok()?;
    Some(
        base.join("resources")
            .join("servers")
            .join(server.dir_token())
            .join("cv.json"),
    )
}

/// Resolve the bundled scrcpy-server.jar path.
pub(crate) fn resolve_scrcpy_jar(app: &tauri::AppHandle) -> Option<PathBuf> {
    let base = app.path().resource_dir().ok()?;
    Some(
        base.join("resources")
            .join("scrcpy")
            .join("scrcpy-server.jar"),
    )
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
    let imported = app_assets_dir(app).join("servants");
    if imported.is_dir() {
        return Some(imported);
    }
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

/// Resolve the per-craft-essence assets directory (containing
/// `{ce_id}/card_ce.png`). Mirrors `resolve_servant_assets_dir` — the
/// runner uses these templates to verify support rows on the fly, so we
/// look up bundled assets first, then fall back to the dev-time source
/// tree. Returns `None` when neither path exists; callers fall back to
/// the legacy behaviour (pick the first OCR match) in that case.
pub(crate) fn resolve_ce_assets_dir(app: &tauri::AppHandle) -> Option<PathBuf> {
    let imported = app_assets_dir(app).join("ces");
    if imported.is_dir() {
        return Some(imported);
    }
    if let Ok(base) = app.path().resource_dir() {
        let bundled = base.join("assets").join("ces");
        if bundled.is_dir() {
            return Some(bundled);
        }
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join("ces");
    if dev.is_dir() {
        return Some(dev);
    }
    None
}

/// Resolve the installed mash-cv runtime executable path. The sidecar is no
/// longer bundled inside the app; users install the PyInstaller --onedir zip
/// under `app_data_dir()/runtime/mash-cv/<version>/mash-cv/`.
pub(crate) fn resolve_sidecar_exe(app: &tauri::AppHandle) -> Option<PathBuf> {
    let manifest = runtime_manifest(app).ok()?;
    Some(runtime_executable_path(
        &runtime_root_dir(app),
        &manifest.mash_cv_runtime_version,
    ))
}

pub(crate) fn resolve_sidecar_code_dir(app: &tauri::AppHandle) -> Option<PathBuf> {
    let manifest = runtime_manifest(app).ok()?;
    Some(runtime_code_path(
        &runtime_root_dir(app),
        &manifest.mash_cv_code_version,
    ))
}

pub(crate) fn resolve_sidecar_models_dir(app: &tauri::AppHandle) -> Option<PathBuf> {
    let manifest = runtime_manifest(app).ok()?;
    Some(runtime_models_path(
        &runtime_root_dir(app),
        &manifest.mash_cv_runtime_version,
    ))
}

#[derive(serde::Serialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct AssetBundleImportResult {
    imported_servants: bool,
    imported_craft_essences: bool,
    servant_files: u64,
    craft_essence_files: u64,
    install_dir: String,
}

#[derive(serde::Serialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct AssetBundleStatus {
    installed: bool,
    imported_servants: bool,
    imported_craft_essences: bool,
    servant_files: u64,
    craft_essence_files: u64,
    install_dir: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct FileCopyStats {
    files: u64,
    bytes: u64,
}

impl std::ops::AddAssign for FileCopyStats {
    fn add_assign(&mut self, rhs: Self) {
        self.files += rhs.files;
        self.bytes += rhs.bytes;
    }
}

fn refresh_asset_protocol_scope(app: &tauri::AppHandle) -> Result<(), String> {
    let asset_scope = app.asset_protocol_scope();
    if let Some(dir) = resolve_servant_assets_dir(app) {
        asset_scope
            .allow_directory(dir, true)
            .map_err(|e| e.to_string())?;
    }
    if let Some(dir) = resolve_ce_assets_dir(app) {
        asset_scope
            .allow_directory(dir, true)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn extract_zip_archive(zip_path: &Path, destination: &Path) -> Result<(), String> {
    let file = fs::File::open(zip_path).map_err(|e| format!("无法打开压缩包: {e}"))?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("无法读取 zip 压缩包: {e}"))?;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| format!("读取 zip 条目失败: {e}"))?;
        let Some(relative) = entry.enclosed_name().map(|p| p.to_path_buf()) else {
            continue;
        };
        let output = destination.join(relative);
        if entry.name().ends_with('/') {
            fs::create_dir_all(&output).map_err(|e| format!("创建目录失败: {e}"))?;
            continue;
        }
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
        }
        let mut out = fs::File::create(&output).map_err(|e| format!("写入解压文件失败: {e}"))?;
        io::copy(&mut entry, &mut out).map_err(|e| format!("解压文件失败: {e}"))?;
        out.flush().map_err(|e| format!("写入解压文件失败: {e}"))?;
    }

    Ok(())
}

fn locate_import_root(extracted_root: &Path) -> PathBuf {
    let nested_assets = extracted_root.join("assets");
    if nested_assets.is_dir() {
        nested_assets
    } else {
        extracted_root.to_path_buf()
    }
}

fn copy_asset_tree(source: &Path, destination: &Path) -> Result<FileCopyStats, String> {
    let mut stats = FileCopyStats::default();
    fs::create_dir_all(destination).map_err(|e| format!("创建素材目录失败: {e}"))?;
    let entries = fs::read_dir(source).map_err(|e| format!("读取素材目录失败: {e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("读取素材目录失败: {e}"))?;
        let file_type = entry
            .file_type()
            .map_err(|e| format!("读取素材类型失败: {e}"))?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if file_type.is_dir() {
            stats += copy_asset_tree(&source_path, &destination_path)?;
        } else if file_type.is_file() {
            if let Some(parent) = destination_path.parent() {
                fs::create_dir_all(parent).map_err(|e| format!("创建素材目录失败: {e}"))?;
            }
            let bytes = fs::copy(&source_path, &destination_path)
                .map_err(|e| format!("复制素材文件失败: {e}"))?;
            stats += FileCopyStats { files: 1, bytes };
        }
    }
    Ok(stats)
}

fn replace_asset_tree(source: &Path, destination: &Path) -> Result<FileCopyStats, String> {
    let staging = destination.with_extension(format!("import-{}", uuid::Uuid::new_v4()));
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|e| format!("清理临时目录失败: {e}"))?;
    }
    let stats = copy_asset_tree(source, &staging)?;
    if destination.exists() {
        fs::remove_dir_all(destination).map_err(|e| format!("替换旧素材失败: {e}"))?;
    }
    fs::rename(&staging, destination).map_err(|e| format!("安装素材失败: {e}"))?;
    Ok(stats)
}

fn import_asset_bundle_from_zip_path(
    zip_path: &Path,
    assets_root: &Path,
) -> Result<AssetBundleImportResult, String> {
    let temp = tempfile::Builder::new()
        .prefix("asset-import-")
        .tempdir_in(
            assets_root
                .parent()
                .ok_or_else(|| "无法定位素材根目录".to_string())?,
        )
        .map_err(|e| format!("创建临时目录失败: {e}"))?;
    let extracted_root = temp.path().join("unzipped");
    fs::create_dir_all(&extracted_root).map_err(|e| format!("创建临时目录失败: {e}"))?;
    extract_zip_archive(zip_path, &extracted_root)?;

    let import_root = locate_import_root(&extracted_root);
    let servant_source = import_root.join("servants");
    let ce_source = import_root.join("ces");
    let has_servants = servant_source.is_dir();
    let has_ces = ce_source.is_dir();
    if !has_servants && !has_ces {
        return Err("压缩包内未找到 assets/servants 或 assets/ces 目录".to_string());
    }

    fs::create_dir_all(assets_root).map_err(|e| format!("创建素材目录失败: {e}"))?;
    let servant_stats = if has_servants {
        replace_asset_tree(&servant_source, &assets_root.join("servants"))?
    } else {
        FileCopyStats::default()
    };
    let ce_stats = if has_ces {
        replace_asset_tree(&ce_source, &assets_root.join("ces"))?
    } else {
        FileCopyStats::default()
    };

    Ok(AssetBundleImportResult {
        imported_servants: has_servants,
        imported_craft_essences: has_ces,
        servant_files: servant_stats.files,
        craft_essence_files: ce_stats.files,
        install_dir: assets_root.to_string_lossy().into_owned(),
    })
}

fn count_files_recursive(dir: &Path) -> Result<u64, String> {
    if !dir.is_dir() {
        return Ok(0);
    }
    let mut count = 0;
    for entry in fs::read_dir(dir).map_err(|e| format!("读取素材目录失败: {e}"))? {
        let entry = entry.map_err(|e| format!("读取素材目录失败: {e}"))?;
        let file_type = entry
            .file_type()
            .map_err(|e| format!("读取素材类型失败: {e}"))?;
        if file_type.is_dir() {
            count += count_files_recursive(&entry.path())?;
        } else if file_type.is_file() {
            count += 1;
        }
    }
    Ok(count)
}

fn asset_bundle_status_for_app(app: &tauri::AppHandle) -> Result<AssetBundleStatus, String> {
    asset_bundle_status_from_root(&app_assets_dir(app))
}

fn asset_bundle_status_from_root(assets_root: &Path) -> Result<AssetBundleStatus, String> {
    let servants = assets_root.join("servants");
    let craft_essences = assets_root.join("ces");
    let servant_files = count_files_recursive(&servants)?;
    let craft_essence_files = count_files_recursive(&craft_essences)?;
    let imported_servants = servant_files > 0;
    let imported_craft_essences = craft_essence_files > 0;
    Ok(AssetBundleStatus {
        installed: imported_servants && imported_craft_essences,
        imported_servants,
        imported_craft_essences,
        servant_files,
        craft_essence_files,
        install_dir: assets_root.to_string_lossy().into_owned(),
    })
}

#[tauri::command]
fn get_asset_bundle_status(app: tauri::AppHandle) -> Result<AssetBundleStatus, String> {
    asset_bundle_status_for_app(&app)
}

#[tauri::command]
async fn pick_asset_bundle(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let picked = app
        .dialog()
        .file()
        .add_filter("ZIP", &["zip"])
        .set_title("选择素材压缩包")
        .blocking_pick_file();
    let Some(path) = picked else {
        return Ok(None);
    };
    let path = path.into_path().map_err(|e| e.to_string())?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

#[tauri::command]
async fn import_asset_bundle(
    app: tauri::AppHandle,
    zip_path: String,
) -> Result<AssetBundleImportResult, String> {
    let zip_path = PathBuf::from(zip_path);
    if !zip_path.is_file() {
        return Err("选择的压缩包不存在".to_string());
    }
    let result = import_asset_bundle_from_zip_path(&zip_path, &app_assets_dir(&app))?;
    refresh_asset_protocol_scope(&app)?;
    Ok(result)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let use_bluestack = load_bluestack_setting(&app.handle());
            let server = load_server_setting(&app.handle());
            refresh_asset_protocol_scope(&app.handle())?;
            app.manage(Mutex::new(use_bluestack));
            app.manage(Mutex::new(server));
            app.manage(Mutex::new(RunnerHandle::new_idle()));
            app.manage(Mutex::new(EnhancementRunnerHandle::new_idle()));
            app.manage(debug::DebugSidecar::new());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_servants,
            get_craft_essences,
            get_asset_bundle_status,
            pick_asset_bundle,
            import_asset_bundle,
            get_runtime_status,
            pick_runtime_bundle,
            import_runtime_bundle,
            get_servant_portrait_path,
            get_servant_face_path,
            get_craft_essence_card_path,
            save_battle_scenes,
            load_battle_scenes,
            list_projects,
            create_project,
            duplicate_project,
            update_project,
            delete_project,
            check_adb,
            get_use_bluestack,
            set_use_bluestack,
            get_server,
            set_server,
            start_automation,
            stop_automation,
            stop_automation_after_current,
            get_automation_status,
            start_enhancement_automation,
            stop_enhancement_automation,
            get_enhancement_automation_status,
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
            debug::debug_find_enhancement_servant,
            debug::debug_find_attack_button,
            debug::debug_read_battle_scene,
            debug::debug_find_supports,
            debug::debug_list_servant_assets,
            debug::warm_sidecar,
            get_servant_metadata,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::io::Cursor;
    use zip::write::SimpleFileOptions;

    // --- input coordinate sizing --------------------------------------

    #[test]
    fn input_size_for_taps_uses_stream_when_adb_size_missing() {
        assert_eq!(input_size_for_taps(None, (1280, 720)), (1280, 720));
    }

    #[test]
    fn input_size_for_taps_prefers_adb_size_with_matching_orientation() {
        assert_eq!(
            input_size_for_taps(Some((2560, 1440)), (1280, 720)),
            (2560, 1440)
        );
    }

    #[test]
    fn input_size_for_taps_swaps_adb_size_to_match_stream_orientation() {
        assert_eq!(
            input_size_for_taps(Some((1080, 1920)), (1280, 720)),
            (1920, 1080)
        );
    }

    // --- default_project_slots -----------------------------------------

    #[test]
    fn default_project_slots_yields_six_slots_with_support_at_index_two() {
        let slots = default_project_slots();
        assert_eq!(slots.len(), 6);
        for (i, slot) in slots.iter().enumerate() {
            assert_eq!(slot.id, format!("slot-{i}"));
            assert!(slot.servant_id.is_none());
            assert!(slot.craft_essence_id.is_none());
        }
        // Slot 2 is the support pin; everything else is a party slot.
        assert_eq!(slots[2].kind, "support");
        for i in [0, 1, 3, 4, 5] {
            assert_eq!(slots[i].kind, "servant");
        }
    }

    // --- ProjectSlot serde --------------------------------------------

    #[test]
    fn project_slot_legacy_json_without_ce_field_deserializes_with_none() {
        // Mirrors a row from a pre-CE-picker `projects.json`. The
        // `#[serde(default)]` on `craft_essence_id` is what keeps these
        // legacy rows loading; this test guards against accidentally
        // dropping that attribute.
        let json = serde_json::json!({
            "id": "slot-0",
            "type": "servant",
            "servantId": 284,
        });
        let slot: ProjectSlot = serde_json::from_value(json).unwrap();
        assert_eq!(slot.id, "slot-0");
        assert_eq!(slot.kind, "servant");
        assert_eq!(slot.servant_id, Some(284));
        assert!(slot.craft_essence_id.is_none());
    }

    #[test]
    fn project_slot_round_trips_craft_essence_id() {
        let json = serde_json::json!({
            "id": "slot-2",
            "type": "support",
            "servantId": 284,
            "craftEssenceId": 1485,
        });
        let slot: ProjectSlot = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(slot.craft_essence_id, Some(1485));

        // Camel-case rename round-trips on serialize too.
        let serialized = serde_json::to_value(&slot).unwrap();
        assert_eq!(serialized["craftEssenceId"], serde_json::json!(1485));
        assert_eq!(serialized["servantId"], serde_json::json!(284));
        assert_eq!(serialized["type"], serde_json::json!("support"));
    }

    #[test]
    fn project_slot_servant_id_also_defaults_when_missing() {
        // Sanity-check the sibling `#[serde(default)]` on `servant_id`
        // so a slot row with neither id field still parses (legacy
        // empty slots).
        let json = serde_json::json!({
            "id": "slot-0",
            "type": "servant",
        });
        let slot: ProjectSlot = serde_json::from_value(json).unwrap();
        assert!(slot.servant_id.is_none());
        assert!(slot.craft_essence_id.is_none());
    }

    // --- Project (top-level legacy JSON) -------------------------------

    #[test]
    fn project_legacy_json_without_slots_falls_back_to_defaults() {
        // The `#[serde(default = "default_project_slots")]` attribute is
        // what makes pre-team-builder `projects.json` rows continue to
        // load; this test pins that contract.
        let json = serde_json::json!({
            "id": "abc",
            "name": "Legacy",
        });
        let project: Project = serde_json::from_value(json).unwrap();
        assert_eq!(project.slots.len(), 6);
        assert!(project.support_servant_id.is_none());
        assert_eq!(project.repeat_mission, false);
    }

    #[test]
    fn copy_project_dir_recursively_copies_saved_project_files() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let nested = src.join("nested");
        let dst = tmp.path().join("dst");
        fs::create_dir_all(&nested).unwrap();
        fs::write(src.join("battle_scenes.json"), br#"[{"id":"scene-1"}]"#).unwrap();
        fs::write(nested.join("notes.json"), br#"{"ok":true}"#).unwrap();

        copy_project_dir(&src, &dst).unwrap();

        assert_eq!(
            fs::read_to_string(dst.join("battle_scenes.json")).unwrap(),
            r#"[{"id":"scene-1"}]"#
        );
        assert_eq!(
            fs::read_to_string(dst.join("nested").join("notes.json")).unwrap(),
            r#"{"ok":true}"#
        );
    }

    // --- pick_portrait_in ----------------------------------------------

    #[test]
    fn pick_portrait_in_returns_none_for_missing_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("does-not-exist");
        assert!(pick_portrait_in(&missing).is_none());
    }

    #[test]
    fn pick_portrait_in_returns_none_when_only_face_and_card_files_present() {
        // Mirrors the real `assets/servants/1/` layout for servants that
        // haven't had a `narrow_servant_*.png` portrait dropped in yet —
        // face and card art exist but they aren't full-body portraits
        // and must not be served as one.
        let tmp = tempfile::tempdir().unwrap();
        for name in ["face_servant_1.png", "card_servant_1.png", "servant.json"] {
            fs::write(tmp.path().join(name), b"").unwrap();
        }
        assert!(pick_portrait_in(tmp.path()).is_none());
    }

    #[test]
    fn pick_portrait_in_picks_highest_ascension_stage() {
        // With multiple `narrow_servant_<n>.png` siblings, the resolver
        // must hand back the lexicographically-largest filename — which
        // for the single-digit ascension scheme used by the Atlas dump
        // is also the highest stage (i.e. the final-ascension full art).
        let tmp = tempfile::tempdir().unwrap();
        for name in [
            "narrow_servant_3.png",
            "narrow_servant_4.png",
            "narrow_servant_1.png",
        ] {
            fs::write(tmp.path().join(name), b"").unwrap();
        }
        let picked = pick_portrait_in(tmp.path()).expect("expected a match");
        assert_eq!(
            picked.file_name().and_then(|n| n.to_str()),
            Some("narrow_servant_4.png")
        );
    }

    #[test]
    fn pick_portrait_by_id_in_prefers_exact_variant_asset() {
        let tmp = tempfile::tempdir().unwrap();
        for name in ["narrow_servant_4.png", "narrow_servant_800170.png"] {
            fs::write(tmp.path().join(name), b"").unwrap();
        }
        let picked = pick_portrait_by_id_in(tmp.path(), 800170).expect("expected a match");
        assert_eq!(
            picked.file_name().and_then(|n| n.to_str()),
            Some("narrow_servant_800170.png")
        );
        assert!(pick_portrait_by_id_in(tmp.path(), 800151).is_none());
    }

    #[test]
    fn pick_face_in_picks_highest_ascension_stage() {
        let tmp = tempfile::tempdir().unwrap();
        for name in [
            "face_servant_1.png",
            "face_servant_4.png",
            "narrow_servant_4.png",
        ] {
            fs::write(tmp.path().join(name), b"").unwrap();
        }
        let picked = pick_face_in(tmp.path()).expect("expected a face match");
        assert_eq!(
            picked.file_name().and_then(|n| n.to_str()),
            Some("face_servant_4.png")
        );
    }

    #[test]
    fn pick_faces_desc_in_returns_all_faces_high_to_low() {
        let tmp = tempfile::tempdir().unwrap();
        for name in [
            "face_servant_1.png",
            "face_servant_10.png",
            "face_servant_4.png",
            "narrow_servant_4.png",
        ] {
            fs::write(tmp.path().join(name), b"").unwrap();
        }
        let picked = pick_faces_desc_in(tmp.path());
        let names: Vec<_> = picked
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();
        assert_eq!(
            names,
            vec![
                "face_servant_10.png",
                "face_servant_4.png",
                "face_servant_1.png"
            ]
        );
    }

    fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut cursor);
            let options = SimpleFileOptions::default();
            for (name, contents) in entries {
                writer.start_file(name, options).unwrap();
                writer.write_all(contents).unwrap();
            }
            writer.finish().unwrap();
        }
        cursor.into_inner()
    }

    #[test]
    fn import_asset_bundle_from_zip_path_accepts_assets_wrapper() {
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("bundle.zip");
        fs::write(
            &zip_path,
            build_zip(&[
                ("assets/servants/1/narrow_servant_4.png", b"portrait"),
                ("assets/ces/2/card_ce.png", b"ce"),
            ]),
        )
        .unwrap();

        let result = import_asset_bundle_from_zip_path(&zip_path, &tmp.path().join("installed"))
            .expect("import should succeed");

        assert!(result.imported_servants);
        assert!(result.imported_craft_essences);
        assert_eq!(result.servant_files, 1);
        assert_eq!(result.craft_essence_files, 1);
        assert!(tmp
            .path()
            .join("installed")
            .join("servants")
            .join("1")
            .join("narrow_servant_4.png")
            .is_file());
        assert!(tmp
            .path()
            .join("installed")
            .join("ces")
            .join("2")
            .join("card_ce.png")
            .is_file());
    }

    #[test]
    fn import_asset_bundle_from_zip_path_replaces_existing_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let install_root = tmp.path().join("installed");
        let existing = install_root.join("servants").join("1");
        fs::create_dir_all(&existing).unwrap();
        fs::write(existing.join("old.png"), b"old").unwrap();

        let zip_path = tmp.path().join("bundle.zip");
        fs::write(&zip_path, build_zip(&[("servants/1/new.png", b"new")])).unwrap();

        let result = import_asset_bundle_from_zip_path(&zip_path, &install_root)
            .expect("import should work");

        assert!(result.imported_servants);
        assert_eq!(result.servant_files, 1);
        assert!(!install_root
            .join("servants")
            .join("1")
            .join("old.png")
            .exists());
        assert!(install_root
            .join("servants")
            .join("1")
            .join("new.png")
            .is_file());
    }

    #[test]
    fn import_asset_bundle_from_zip_path_rejects_zip_without_asset_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("bundle.zip");
        fs::write(&zip_path, build_zip(&[("docs/readme.txt", b"no assets")])).unwrap();

        let err = import_asset_bundle_from_zip_path(&zip_path, &tmp.path().join("installed"))
            .expect_err("import should fail");
        assert!(err.contains("assets/servants") || err.contains("assets/ces"));
    }

    #[test]
    fn asset_bundle_status_requires_servants_and_craft_essences() {
        let tmp = tempfile::tempdir().unwrap();
        let assets_root = tmp.path().join("assets");
        let missing = asset_bundle_status_from_root(&assets_root).unwrap();
        assert!(!missing.installed);
        assert!(!missing.imported_servants);
        assert!(!missing.imported_craft_essences);

        fs::create_dir_all(assets_root.join("servants").join("1")).unwrap();
        fs::write(
            assets_root
                .join("servants")
                .join("1")
                .join("face_servant_1.png"),
            b"face",
        )
        .unwrap();
        let partial = asset_bundle_status_from_root(&assets_root).unwrap();
        assert!(!partial.installed);
        assert!(partial.imported_servants);
        assert!(!partial.imported_craft_essences);
        assert_eq!(partial.servant_files, 1);

        fs::create_dir_all(assets_root.join("ces").join("2")).unwrap();
        fs::write(assets_root.join("ces").join("2").join("card_ce.png"), b"ce").unwrap();
        let installed = asset_bundle_status_from_root(&assets_root).unwrap();
        assert!(installed.installed);
        assert_eq!(installed.servant_files, 1);
        assert_eq!(installed.craft_essence_files, 1);
    }

    fn build_runtime_manifest(
        runtime_version: &str,
        code_version: &str,
        platform: &str,
        runtime_sha256: &str,
        code_sha256: &str,
    ) -> RuntimeManifest {
        parse_runtime_manifest(&format!(
            r#"{{
                "mashCvRuntimeVersion": "{runtime_version}",
                "mashCvCodeVersion": "{code_version}",
                "platforms": {{
                    "{platform}": {{
                        "runtimeUrl": "https://cdn.example.com/runtime.zip",
                        "runtimeSha256": "{runtime_sha256}",
                        "codeUrl": "https://cdn.example.com/code.zip",
                        "codeSha256": "{code_sha256}"
                    }}
                }}
            }}"#
        ))
        .unwrap()
    }

    fn sha256_bytes(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }

    #[test]
    fn runtime_manifest_selects_platform_artifact() {
        let manifest = build_runtime_manifest(
            "2026.05.08-runtime1",
            "2026.05.08-code1",
            "darwin-aarch64",
            "runtime-sha",
            "code-sha",
        );
        let artifact = manifest.platforms.get("darwin-aarch64").unwrap();
        assert_eq!(manifest.mash_cv_runtime_version, "2026.05.08-runtime1");
        assert_eq!(manifest.mash_cv_code_version, "2026.05.08-code1");
        assert_eq!(artifact.runtime_url, "https://cdn.example.com/runtime.zip");
        assert_eq!(artifact.runtime_sha256, "runtime-sha");
        assert_eq!(artifact.code_url, "https://cdn.example.com/code.zip");
        assert_eq!(artifact.code_sha256, "code-sha");
        assert!(manifest.platforms.get("darwin-x86_64").is_none());
    }

    #[test]
    fn runtime_platform_key_normalizes_macos_to_darwin() {
        assert_eq!(
            runtime_platform_key_from("macos", "aarch64"),
            "darwin-aarch64"
        );
        assert_eq!(
            runtime_platform_key_from("macos", "x86_64"),
            "darwin-x86_64"
        );
        assert_eq!(runtime_platform_key_from("linux", "x86_64"), "linux-x86_64");
    }

    #[test]
    fn runtime_status_reports_missing_matching_and_stale_versions() {
        let tmp = tempfile::tempdir().unwrap();
        let platform = "darwin-aarch64";
        let manifest = build_runtime_manifest("runtime-v2", "code-v2", platform, "abc", "def");

        let missing = runtime_status_from_manifest(&manifest, tmp.path(), platform);
        assert!(!missing.installed);
        assert!(!missing.runtime_installed);
        assert!(!missing.code_installed);
        assert_eq!(missing.installed_runtime_version, None);
        assert_eq!(missing.installed_code_version, None);

        fs::create_dir_all(runtime_version_path(tmp.path()).parent().unwrap()).unwrap();
        fs::write(
            runtime_version_path(tmp.path()),
            r#"{"version":"runtime-v1","platform":"darwin-aarch64"}"#,
        )
        .unwrap();
        fs::create_dir_all(code_version_path(tmp.path()).parent().unwrap()).unwrap();
        fs::write(
            code_version_path(tmp.path()),
            r#"{"version":"code-v1","platform":"darwin-aarch64"}"#,
        )
        .unwrap();
        let stale = runtime_status_from_manifest(&manifest, tmp.path(), platform);
        assert!(!stale.installed);
        assert_eq!(
            stale.installed_runtime_version.as_deref(),
            Some("runtime-v1")
        );
        assert_eq!(stale.installed_code_version.as_deref(), Some("code-v1"));

        let exe = runtime_executable_path(tmp.path(), "runtime-v2");
        fs::create_dir_all(exe.parent().unwrap()).unwrap();
        fs::write(&exe, b"exe").unwrap();
        let code_pkg = runtime_code_path(tmp.path(), "code-v2").join("mash_cv");
        fs::create_dir_all(&code_pkg).unwrap();
        fs::write(
            runtime_version_path(tmp.path()),
            r#"{"version":"runtime-v2","platform":"darwin-aarch64"}"#,
        )
        .unwrap();
        fs::write(
            code_version_path(tmp.path()),
            r#"{"version":"code-v2","platform":"darwin-aarch64"}"#,
        )
        .unwrap();
        let matching = runtime_status_from_manifest(&manifest, tmp.path(), platform);
        assert!(matching.installed);
        assert_eq!(
            matching.installed_runtime_version.as_deref(),
            Some("runtime-v2")
        );
        assert_eq!(matching.installed_code_version.as_deref(), Some("code-v2"));
        assert!(matching.executable_path.ends_with(runtime_exe_name()));
    }

    #[test]
    fn import_runtime_bundle_rejects_sha_mismatch() {
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("runtime.zip");
        fs::write(
            &zip_path,
            build_zip(&[("mash-cv-runtime/mash-cv", b"runtime executable")]),
        )
        .unwrap();
        let manifest = build_runtime_manifest(
            "runtime-v1",
            "code-v1",
            "darwin-aarch64",
            "bad-runtime-sha",
            "bad-code-sha",
        );

        let err = import_runtime_bundle_from_zip_path(
            &zip_path,
            &manifest,
            &tmp.path().join("runtime"),
            "darwin-aarch64",
        )
        .expect_err("import should fail");
        assert!(err.contains("sha256 不匹配"));
    }

    #[test]
    fn import_runtime_bundle_rejects_missing_executable() {
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("runtime.zip");
        let bytes = build_zip(&[("mash-cv-runtime/readme.txt", b"no exe")]);
        fs::write(&zip_path, &bytes).unwrap();
        let manifest = build_runtime_manifest(
            "runtime-v1",
            "code-v1",
            "darwin-aarch64",
            &sha256_bytes(&bytes),
            "code-sha",
        );

        let err = import_runtime_bundle_from_zip_path(
            &zip_path,
            &manifest,
            &tmp.path().join("runtime"),
            "darwin-aarch64",
        )
        .expect_err("import should fail");
        assert!(err.contains("未找到 mash-cv"));
    }

    #[test]
    fn import_runtime_bundle_rejects_path_traversal_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("runtime.zip");
        let bytes = build_zip(&[
            ("mash-cv-runtime/mash-cv", b"runtime executable"),
            ("../evil.txt", b"evil"),
        ]);
        fs::write(&zip_path, &bytes).unwrap();
        let manifest = build_runtime_manifest(
            "runtime-v1",
            "code-v1",
            "darwin-aarch64",
            &sha256_bytes(&bytes),
            "code-sha",
        );

        let err = import_runtime_bundle_from_zip_path(
            &zip_path,
            &manifest,
            &tmp.path().join("runtime"),
            "darwin-aarch64",
        )
        .expect_err("import should fail");
        assert!(err.contains("非法路径"));
        assert!(!tmp.path().join("evil.txt").exists());
    }

    #[test]
    fn import_runtime_bundle_installs_version_record_and_executable() {
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("runtime.zip");
        let exe_entry = format!("mash-cv-runtime/{}", runtime_exe_name());
        let bytes = build_zip(&[(exe_entry.as_str(), b"runtime executable")]);
        fs::write(&zip_path, &bytes).unwrap();
        let manifest = build_runtime_manifest(
            "runtime-v1",
            "code-v1",
            "darwin-aarch64",
            &sha256_bytes(&bytes),
            "code-sha",
        );
        let runtime_root = tmp.path().join("runtime");

        let result = import_runtime_bundle_from_zip_path(
            &zip_path,
            &manifest,
            &runtime_root,
            "darwin-aarch64",
        )
        .expect("import should work");

        assert_eq!(result.installed_kind, "runtime");
        assert_eq!(result.installed_version, "runtime-v1");
        let exe = runtime_executable_path(&runtime_root, "runtime-v1");
        assert!(exe.is_file());
        let record = read_runtime_version(&runtime_root).unwrap();
        assert_eq!(record.version, "runtime-v1");
        assert_eq!(record.platform, "darwin-aarch64");
    }

    #[test]
    fn import_runtime_bundle_installs_code_package() {
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("code.zip");
        let bytes = build_zip(&[
            ("mash-cv-code/mash_cv/__init__.py", b""),
            ("mash-cv-code/mash_cv/cv.py", b"code"),
        ]);
        fs::write(&zip_path, &bytes).unwrap();
        let manifest = build_runtime_manifest(
            "runtime-v1",
            "code-v1",
            "darwin-aarch64",
            "runtime-sha",
            &sha256_bytes(&bytes),
        );
        let runtime_root = tmp.path().join("runtime");

        let result = import_runtime_bundle_from_zip_path(
            &zip_path,
            &manifest,
            &runtime_root,
            "darwin-aarch64",
        )
        .expect("import should work");

        assert_eq!(result.installed_kind, "code");
        assert_eq!(result.installed_version, "code-v1");
        assert!(runtime_code_path(&runtime_root, "code-v1")
            .join("mash_cv")
            .join("cv.py")
            .is_file());
        let record = read_code_version(&runtime_root).unwrap();
        assert_eq!(record.version, "code-v1");
        assert_eq!(record.platform, "darwin-aarch64");
    }

    #[test]
    fn first_np_name_prefers_cn_then_legacy_name() {
        let flat = serde_json::json!({
            "noble_phantasms": [{ "name_cn": "流星一条", "name": "Stella" }]
        });
        assert_eq!(first_np_name(&flat).as_deref(), Some("流星一条"));

        let fallback = serde_json::json!({
            "noble_phantasms": [{ "name": "Stella" }]
        });
        assert_eq!(first_np_name(&fallback).as_deref(), Some("Stella"));
    }

    #[test]
    fn servants_data_expands_variants_with_last_cn_np_and_face_id() {
        let mash_variants: Vec<&ServantInfo> =
            servants_data().iter().filter(|s| s.id == 1).collect();
        assert_eq!(mash_variants.len(), 3);
        assert_eq!(mash_variants[0].variant_key, "1:1");
        assert_eq!(mash_variants[0].face_id, Some(800170));
        assert_eq!(
            mash_variants[0].noble_phantasm_name.as_deref(),
            Some("已然遥远的理想之城")
        );
        assert_eq!(mash_variants[1].face_id, Some(800151));
        assert_eq!(
            mash_variants[1].noble_phantasm_name.as_deref(),
            Some("依然存在的梦想之城")
        );
    }

    // --- pick_ce_card_in -----------------------------------------------

    #[test]
    fn pick_ce_card_in_returns_none_when_directory_missing() {
        let tmp = tempfile::tempdir().unwrap();
        // No `123/` subdirectory ever created.
        assert!(pick_ce_card_in(tmp.path(), 123).is_none());
    }

    #[test]
    fn pick_ce_card_in_returns_none_when_only_metadata_present() {
        // CE directories may exist with only `craft-essence.json` but
        // no `card_ce.png` if the asset hasn't been pulled yet — the
        // resolver must report `None` in that case so the frontend
        // renders the gray placeholder rather than a broken `<img>`.
        let tmp = tempfile::tempdir().unwrap();
        let ce_dir = tmp.path().join("42");
        fs::create_dir_all(&ce_dir).unwrap();
        fs::write(ce_dir.join("craft-essence.json"), b"{}").unwrap();
        assert!(pick_ce_card_in(tmp.path(), 42).is_none());
    }

    #[test]
    fn pick_ce_card_in_returns_path_when_file_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let ce_dir = tmp.path().join("42");
        fs::create_dir_all(&ce_dir).unwrap();
        let card = ce_dir.join("card_ce.png");
        fs::write(&card, b"fake png bytes").unwrap();
        let picked = pick_ce_card_in(tmp.path(), 42).expect("expected a match");
        assert_eq!(picked, card);
    }

    // --- craft_essences_data -------------------------------------------

    #[test]
    fn craft_essences_data_parses_and_has_unique_ids() {
        let ces = craft_essences_data();
        assert!(
            !ces.is_empty(),
            "bundled craft_essences.json parsed to an empty list"
        );

        let mut seen: HashSet<u32> = HashSet::with_capacity(ces.len());
        for ce in ces {
            assert!(!ce.name.is_empty(), "CE id {} has an empty name", ce.id);
            assert!(
                seen.insert(ce.id),
                "duplicate CE id {} in craft_essences.json",
                ce.id
            );
        }
    }

    #[test]
    fn craft_essences_data_is_memoized_via_oncelock() {
        // OnceLock-backed `&'static [CraftEssenceInfo]` should hand back
        // the exact same slice on repeated calls (same pointer + len).
        // If somebody refactors away the cache, this catches it.
        let a = craft_essences_data();
        let b = craft_essences_data();
        assert_eq!(a.as_ptr(), b.as_ptr());
        assert_eq!(a.len(), b.len());
    }

    // --- Server enum --------------------------------------------------

    #[test]
    fn server_default_is_jp() {
        assert_eq!(Server::default(), Server::Jp);
    }

    #[test]
    fn server_display_and_dir_token_match_serde() {
        for s in [Server::Jp, Server::Cn] {
            // Display + dir_token must agree with the serde tag (the
            // value the frontend sees) so JSON, file paths, and stderr
            // logs all line up.
            let serialized = serde_json::to_string(&s).unwrap();
            assert_eq!(serialized, format!("\"{}\"", s));
            assert_eq!(serialized.to_lowercase().trim_matches('"'), s.dir_token());
        }
    }

    #[test]
    fn server_from_str_round_trips_both_cases() {
        // Persisted settings file stores `"JP"` / `"CN"`; defensive
        // lower / mixed-case parsing keeps a hand-edited file working.
        for (input, expected) in [
            ("JP", Server::Jp),
            ("jp", Server::Jp),
            ("Jp", Server::Jp),
            ("CN", Server::Cn),
            ("cn", Server::Cn),
        ] {
            assert_eq!(Server::from_str(input).unwrap(), expected, "input={input}");
        }
        assert!(Server::from_str("us").is_err());
    }

    #[test]
    fn server_serde_uses_uppercase_tag() {
        let value: Server = serde_json::from_str("\"CN\"").unwrap();
        assert_eq!(value, Server::Cn);
        assert!(serde_json::from_str::<Server>("\"cn\"").is_err());
    }

    // --- Localization indices -----------------------------------------

    #[test]
    fn normalize_jp_key_strips_whitespace_and_nfkc_folds() {
        // Full-width vs. half-width digits should collapse to the
        // same key so JP→CN lookups don't miss on cosmetic
        // differences between mooncell and Atlas dumps.
        assert_eq!(normalize_jp_key("Ｌｖ１"), normalize_jp_key("Lv1"));
        // Stray spaces in the source data must not split the key.
        assert_eq!(
            normalize_jp_key("アルトリア ペンドラゴン"),
            normalize_jp_key("アルトリアペンドラゴン")
        );
    }

    #[test]
    fn servant_jp_to_cn_index_maps_known_servant_name() {
        // Servant 2 = Altria Pendragon — the row exists in the
        // bundled mooncell `servants.json` with both `name_jp` and
        // `name_cn` populated, so the index must produce the CN
        // string.
        let idx = servant_jp_to_cn_index();
        assert_eq!(
            idx.get(&normalize_jp_key("アルトリア・ペンドラゴン"))
                .map(|s| s.as_str()),
            Some("阿尔托莉雅·潘德拉贡")
        );
    }

    #[test]
    fn np_index_handles_both_shapes() {
        // Servant 1 uses the dict-of-variants `noble_phantasms`
        // shape; servant 2 uses the flat-list shape. Both must be
        // discoverable by the same builder.
        let idx = np_jp_to_cn_index();
        // Dict-of-variants entry from servant 1 (variant "初始").
        assert_eq!(
            idx.get(&normalize_jp_key("いまは遙か理想の城"))
                .map(|s| s.as_str()),
            Some("已然遥远的理想之城"),
        );
        // Flat-list entry from servant 2.
        assert_eq!(
            idx.get(&normalize_jp_key("約束された勝利の剣"))
                .map(|s| s.as_str()),
            Some("誓约胜利之剑"),
        );
        // Sanity: index contains a non-trivial number of entries —
        // catches an outright build error in the parser.
        assert!(idx.len() > 100, "NP index too small: {}", idx.len());
    }

    #[test]
    fn np_index_dedupes_repeated_jp_names() {
        // Servant 2 lists the same `name_jp` twice (two NP variants
        // share the same wording). The OnceLock builder uses
        // `entry().or_insert` so the duplicate is discarded; this
        // test pins that contract — if a refactor flips to
        // `insert()`, the second translation would silently overwrite
        // the first, which is harmless here but a footgun for cases
        // where the two CN translations actually disagree.
        let idx = np_jp_to_cn_index();
        let key = normalize_jp_key("約束された勝利の剣");
        assert!(idx.contains_key(&key));
    }

    #[test]
    fn localize_servant_name_falls_back_to_jp_when_unmapped() {
        // A name we know isn't in the index must come back unchanged
        // so OCR has *some* target.
        let unknown = "存在しないサーヴァント";
        assert_eq!(localize_servant_name(unknown), unknown);
    }

    #[test]
    fn localize_np_names_drops_unmapped_and_dedupes() {
        // One known + one unknown -> only the known one survives.
        let mapped = localize_np_names(&[
            "約束された勝利の剣".to_string(),
            "存在しない宝具".to_string(),
        ]);
        assert_eq!(mapped, vec!["誓约胜利之剑".to_string()]);

        // Two inputs that translate to the same CN string -> single
        // output entry (preserves order, dedupes by post-translation
        // string).
        let mapped = localize_np_names(&[
            "約束された勝利の剣".to_string(),
            "約束された勝利の剣".to_string(),
        ]);
        assert_eq!(mapped, vec!["誓约胜利之剑".to_string()]);

        // All-unknown input collapses to empty list, which is the
        // signal `_find_supports` uses to switch into name-only mode.
        let mapped = localize_np_names(&["存在しない宝具".to_string()]);
        assert!(mapped.is_empty());
    }

    // --- Action::CommandSpell + BattleScene serde --------------------------

    #[test]
    fn action_command_spell_serializes_with_camel_case_tag() {
        // The frontend identifies the variant by `type:"commandSpell"`;
        // pin that wire shape so a refactor that drops the
        // `#[serde(rename = "commandSpell")]` attribute breaks loudly.
        let action = Action::CommandSpell {
            id: "cs_1".into(),
            spell: Some("np_release".into()),
            target: Some("servant_2".into()),
        };
        let json = serde_json::to_value(&action).unwrap();
        assert_eq!(json["type"], serde_json::json!("commandSpell"));
        assert_eq!(json["spell"], serde_json::json!("np_release"));
        assert_eq!(json["target"], serde_json::json!("servant_2"));
    }

    #[test]
    fn action_command_spell_target_defaults_to_none_when_missing() {
        // Mirror the equipment-action behaviour: an action stored
        // without a target field still loads (target left null).
        let json = serde_json::json!({
            "type": "commandSpell",
            "id": "cs_1",
            "spell": "restore",
        });
        let action: Action = serde_json::from_value(json).unwrap();
        match action {
            Action::CommandSpell { id, spell, target } => {
                assert_eq!(id, "cs_1");
                assert_eq!(spell.as_deref(), Some("restore"));
                assert!(target.is_none());
            }
            _ => panic!("expected Action::CommandSpell"),
        }
    }

    #[test]
    fn battle_scene_legacy_json_merges_old_action_buckets_in_fixed_order() {
        // Pre-ordered-action configs stored three separate preparation
        // buckets. Normalization preserves their historical execution
        // order so old projects keep behaving the same after loading.
        let json = serde_json::json!({
            "id": "scene_1",
            "servantActions": [{
                "type": "servant",
                "id": "sa_1",
                "servant": "servant_1",
                "skill": "skill_1",
                "target": null
            }],
            "equipmentActions": [{
                "type": "equipment",
                "id": "eq_1",
                "skill": "skill_2",
                "target": null
            }],
            "commandSpellActions": [{
                "type": "commandSpell",
                "id": "cs_1",
                "spell": "restore",
                "target": "servant_2"
            }],
            "attackPriority": [],
        });
        let scene: BattleScene = serde_json::from_value::<BattleScene>(json)
            .unwrap()
            .normalize_preparation_actions();
        assert_eq!(scene.id, "scene_1");
        assert_eq!(scene.preparation_actions.len(), 3);
        assert!(matches!(
            scene.preparation_actions[0],
            Action::Servant { .. }
        ));
        assert!(matches!(
            scene.preparation_actions[1],
            Action::Equipment { .. }
        ));
        assert!(matches!(
            scene.preparation_actions[2],
            Action::CommandSpell { .. }
        ));
        assert!(scene.servant_actions.is_empty());
        assert!(scene.equipment_actions.is_empty());
        assert!(scene.command_spell_actions.is_empty());
    }

    #[test]
    fn battle_scene_round_trips_preparation_actions_under_camel_case_key() {
        let scene = BattleScene {
            id: "scene_1".into(),
            preparation_actions: vec![Action::CommandSpell {
                id: "cs_1".into(),
                spell: Some("np_release".into()),
                target: Some("servant_1".into()),
            }],
            servant_actions: vec![],
            equipment_actions: vec![],
            command_spell_actions: vec![],
            attack_priority: vec![],
        };
        let json = serde_json::to_value(&scene).unwrap();
        assert!(json["preparationActions"].is_array());
        assert!(json["commandSpellActions"].is_null());
        assert_eq!(
            json["preparationActions"][0]["type"],
            serde_json::json!("commandSpell")
        );

        // Deserialize back — symmetry guards against accidentally
        // exposing a field under one name and reading it under another.
        let parsed: BattleScene = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.preparation_actions.len(), 1);
        match &parsed.preparation_actions[0] {
            Action::CommandSpell { spell, target, .. } => {
                assert_eq!(spell.as_deref(), Some("np_release"));
                assert_eq!(target.as_deref(), Some("servant_1"));
            }
            _ => panic!("expected Action::CommandSpell"),
        }
    }
}
