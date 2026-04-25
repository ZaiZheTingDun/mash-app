mod adb;
mod debug;
mod runner;
mod screen;

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, OnceLock};
use tauri::Manager;

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

/// One configured battle-scene block. The runner picks which block to
/// execute by reading the `BATTLE m/n` HUD strip and indexing on `m - 1`,
/// so each block represents the per-scene action plan rather than a
/// per-turn one. JSON shape on the wire is unchanged from the legacy
/// `Turn` struct so old `turns.json` files can be migrated in place.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct BattleScene {
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
    let dir = app.path().app_data_dir().expect("failed to resolve app data dir");
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
        slots: default_project_slots(),
        repeat_mission: false,
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
fn save_battle_scenes(
    app: tauri::AppHandle,
    project_id: String,
    scenes: Vec<BattleScene>,
) -> Result<(), String> {
    let path = project_battle_scenes_path(&app, &project_id);
    let json = serde_json::to_string_pretty(&scenes).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())
}

#[tauri::command]
fn load_battle_scenes(app: tauri::AppHandle, project_id: String) -> Vec<BattleScene> {
    let path = project_battle_scenes_path(&app, &project_id);
    if let Ok(contents) = fs::read_to_string(&path) {
        return serde_json::from_str(&contents).unwrap_or_default();
    }

    // One-shot migration: pre-rename projects stored their per-scene
    // config under `turns.json`. The on-disk JSON shape is identical
    // (BattleScene was just renamed from Turn), so we can read it as-is,
    // write it under the new filename, and remove the legacy file.
    let legacy = legacy_project_turns_path(&app, &project_id);
    if let Ok(contents) = fs::read_to_string(&legacy) {
        let scenes: Vec<BattleScene> =
            serde_json::from_str(&contents).unwrap_or_default();
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
) -> Result<Option<String>, String> {
    let Some(root) = resolve_servant_assets_dir(&app) else {
        return Ok(None);
    };
    Ok(pick_portrait_in(&root.join(servant_id.to_string()))
        .map(|p| p.to_string_lossy().into_owned()))
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
            let jp = entry.get("name_jp").and_then(|v| v.as_str()).unwrap_or("").trim();
            let cn = entry.get("name_cn").and_then(|v| v.as_str()).unwrap_or("").trim();
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
            let Some(nps) = s.get("noble_phantasms") else { continue };
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
    let raw = fs::read_to_string(&path).map_err(|e| {
        format!("无法读取 servant.json ({}): {e}", path.display())
    })?;
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

fn server_settings_path(app: &tauri::AppHandle) -> PathBuf {
    let dir = app.path().app_data_dir().expect("failed to resolve app data dir");
    fs::create_dir_all(&dir).ok();
    dir.join("server_settings.json")
}

fn load_server_setting(app: &tauri::AppHandle) -> Server {
    let path = server_settings_path(app);
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("server").and_then(|s| s.as_str()).map(|s| s.to_string()))
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
    fs::write(&path, serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?)
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

    *state.lock().unwrap() = value;
    let path = server_settings_path(&app);
    let json = serde_json::json!({ "server": value.to_string() });
    fs::write(&path, serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;

    // Tear down any cached debug sidecar so the next debug call respawns
    // it with the new server's templates / OCR model. The automation
    // sidecar is short-lived (born inside `start_automation`) so it
    // always sees the fresh setting.
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

#[tauri::command]
fn start_automation(
    app: tauri::AppHandle,
    config: RunConfig,
    bluestack_state: tauri::State<'_, Mutex<bool>>,
    server_state: tauri::State<'_, Mutex<Server>>,
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

    let scenes = load_battle_scenes(app.clone(), config.project_id.clone());

    let use_bluestack = *bluestack_state.lock().unwrap();
    let server = *server_state.lock().unwrap();

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

    let templates_dir = resolve_templates_dir(&app, server);
    let cv_config = resolve_cv_config_path(&app, server);
    let mut sidecar = screen::SidecarClient::spawn(
        &app,
        templates_dir.as_deref(),
        cv_config.as_deref(),
        server,
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
    let ce_assets_dir = resolve_ce_assets_dir(&app);
    let runner = runner::Runner::new(
        adb_dev,
        sidecar,
        config,
        scenes,
        app,
        state,
        cancel,
        screen_size,
        assets_dir,
        ce_assets_dir,
        server,
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

/// Resolve the bundled templates directory for the given server. Each
/// server (JP/CN) ships its own subtree under
/// `resources/servers/{token}/templates/` so flipping the global server
/// setting hands the sidecar a different template set without touching
/// any JP fixture.
pub(crate) fn resolve_templates_dir(
    app: &tauri::AppHandle,
    server: Server,
) -> Option<PathBuf> {
    let base = app.path().resource_dir().ok()?;
    Some(
        base.join("resources")
            .join("servers")
            .join(server.dir_token())
            .join("templates"),
    )
}

/// Resolve the bundled cv.json path for the given server.
pub(crate) fn resolve_cv_config_path(
    app: &tauri::AppHandle,
    server: Server,
) -> Option<PathBuf> {
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

/// Resolve the per-craft-essence assets directory (containing
/// `{ce_id}/card_ce.png`). Mirrors `resolve_servant_assets_dir` — the
/// runner uses these templates to verify support rows on the fly, so we
/// look up bundled assets first, then fall back to the dev-time source
/// tree. Returns `None` when neither path exists; callers fall back to
/// the legacy behaviour (pick the first OCR match) in that case.
pub(crate) fn resolve_ce_assets_dir(app: &tauri::AppHandle) -> Option<PathBuf> {
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
            let server = load_server_setting(&app.handle());
            app.manage(Mutex::new(use_bluestack));
            app.manage(Mutex::new(server));
            app.manage(Mutex::new(RunnerHandle::new_idle()));
            app.manage(debug::DebugSidecar(Mutex::new(None)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_servants,
            get_craft_essences,
            get_servant_portrait_path,
            get_craft_essence_card_path,
            save_battle_scenes,
            load_battle_scenes,
            list_projects,
            create_project,
            update_project,
            delete_project,
            check_adb,
            get_use_bluestack,
            set_use_bluestack,
            get_server,
            set_server,
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
            assert!(
                !ce.name.is_empty(),
                "CE id {} has an empty name",
                ce.id
            );
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
}
