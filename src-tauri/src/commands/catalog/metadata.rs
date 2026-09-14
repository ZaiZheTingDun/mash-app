//! Localized servant metadata used by OCR and battle automation.
//!
//! This module owns JP/CN name indexes, variant-aware recognition names,
//! Noble Phantasm localization, and the cached metadata command.

use super::servants::{
    preferred_cn_name, push_unique_nonempty, servants_data, string_field, ServantInfo,
};
use crate::commands::runtime::resolve_servant_assets_dir;
use crate::server::Server;
use std::collections::HashMap;
use std::fs;
use std::sync::{Mutex, OnceLock};

/// Subset of `assets/servants/{id}/servant.json` needed by the OCR-based
/// support detector: the servant's primary name and every Noble Phantasm
/// name. The frontend uses this to seed `find_supports` from a chosen
/// servant id without shipping the full Atlas Academy blob over IPC.
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ServantMetadata {
    pub id: u32,
    pub name: String,
    #[serde(default)]
    pub names: Vec<String>,
    /// Other localized display names belonging to the same servant id but
    /// not to the selected variant. The sidecar uses these as negative
    /// candidates so a short target name cannot match inside a longer
    /// sibling-variant name.
    #[serde(skip_serializing)]
    pub excluded_names: Vec<String>,
    pub np_names: Vec<String>,
    /// Overlapping names cannot safely use the sidecar's usual name-only
    /// fallback; their variant-scoped NP must also be observed.
    #[serde(skip_serializing)]
    pub require_np_match: bool,
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
pub(crate) fn normalize_jp_key(s: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    s.nfkc()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("")
}

/// Process-wide `nameJP/name_jp -> CN OCR name` map for every servant Noble
/// Phantasm we know about, built once from `resources/servants.json`.
/// Handles both shapes the source file uses:
///   - Atlas list:          `noblePhantasms: [ {...}, ... ]`
///   - legacy flat list:    `noble_phantasms: [ {...}, ... ]`
///   - dict-of-variants:    `noble_phantasms: { "初始": [...], "奥特瑙斯": [...] }`
/// Prefer server aliases when present, otherwise use the formal CN name.
/// Keeps entries even when the CN value equals the JP value: some
/// legitimate names are identical across both languages. Used only when
/// the active server is `Server::Cn` to translate Atlas JP NP names into
/// the strings the OCR will actually see on a CN client.
pub(crate) fn np_jp_to_cn_index() -> &'static HashMap<String, String> {
    static INDEX: OnceLock<HashMap<String, String>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let raw: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../../resources/servants.json"))
                .expect("invalid servants.json");

        let mut map: HashMap<String, String> = HashMap::new();
        let mut consider = |entry: &serde_json::Value| {
            let jp = string_field(entry, &["nameJP", "name_jp"]).unwrap_or_default();
            let cn = preferred_cn_name(entry).unwrap_or_default();
            if jp.is_empty() || cn.is_empty() {
                return;
            }
            let key = normalize_jp_key(&jp);
            if key.is_empty() {
                return;
            }
            map.entry(key).or_insert(cn);
        };

        for s in raw.iter() {
            let Some(nps) = s.get("noblePhantasms").or_else(|| s.get("noble_phantasms")) else {
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

/// Process-wide `name_jp -> CN OCR name` map for the servants themselves
/// (as opposed to their NPs). Built once from `servants_data()`. Same
/// preference and drop rules as the NP index: use `name_cn_server` when
/// present, otherwise `name_cn`; skip only entries whose JP or CN field
/// is missing. This is only a fallback for data without a stable servant
/// id; id-aware lookups must use `localize_servant_name_by_id` because
/// several servants share the same JP display name.
pub(crate) fn servant_jp_to_cn_index() -> &'static HashMap<String, String> {
    static INDEX: OnceLock<HashMap<String, String>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let mut map: HashMap<String, String> = HashMap::new();
        for s in servants_data() {
            let jp = s.name_jp.trim();
            let cn = s.name_cn_server.as_deref().unwrap_or(&s.name_cn).trim();
            if jp.is_empty() || cn.is_empty() {
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

pub(crate) fn servant_id_to_cn_index() -> &'static HashMap<u32, String> {
    static INDEX: OnceLock<HashMap<u32, String>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let mut map: HashMap<u32, String> = HashMap::new();
        for s in servants_data() {
            let cn = s.name_cn_server.as_deref().unwrap_or(&s.name_cn).trim();
            if !cn.is_empty() {
                map.entry(s.id).or_insert_with(|| cn.to_string());
            }
        }
        map
    })
}

pub(crate) fn servant_id_to_names_index(server: Server) -> &'static HashMap<u32, Vec<String>> {
    static JP_INDEX: OnceLock<HashMap<u32, Vec<String>>> = OnceLock::new();
    static CN_INDEX: OnceLock<HashMap<u32, Vec<String>>> = OnceLock::new();
    let index = match server {
        Server::Jp => &JP_INDEX,
        Server::Cn => &CN_INDEX,
    };
    index.get_or_init(|| {
        let mut map: HashMap<u32, Vec<String>> = HashMap::new();
        for s in servants_data() {
            let names = map.entry(s.id).or_default();
            match server {
                Server::Jp => {
                    push_unique_nonempty(names, &s.name_jp);
                    for alias in &s.over_write_servant_names {
                        if let Some(name) = &alias.name_jp {
                            push_unique_nonempty(names, name);
                        }
                    }
                }
                Server::Cn => {
                    let cn = s.name_cn_server.as_deref().unwrap_or(&s.name_cn);
                    push_unique_nonempty(names, cn);
                    for alias in &s.over_write_servant_names {
                        if let Some(name) = &alias.name_cn {
                            push_unique_nonempty(names, name);
                        }
                    }
                }
            }
        }
        map
    })
}

pub(crate) fn localized_servant_names_by_id(
    id: u32,
    server: Server,
    primary_name: &str,
) -> Vec<String> {
    let mut names = Vec::new();
    push_unique_nonempty(&mut names, primary_name);
    if let Some(alias_names) = servant_id_to_names_index(server).get(&id) {
        for name in alias_names {
            push_unique_nonempty(&mut names, name);
        }
    }
    names
}

fn servant_names_overlap(left: &str, right: &str) -> bool {
    fn normalize_support_name_key(value: &str) -> String {
        const DROPPED_SEPARATORS: &str = "・·.,。、;:!?-_/|()（）[]【】「」『』〔〕";
        normalize_jp_key(value)
            .chars()
            .filter(|ch| !DROPPED_SEPARATORS.contains(*ch))
            .flat_map(char::to_lowercase)
            .collect()
    }

    let left = normalize_support_name_key(left);
    let right = normalize_support_name_key(right);
    !left.is_empty()
        && !right.is_empty()
        && (left == right || left.contains(&right) || right.contains(&left))
}

/// Detect names from different servant IDs that would otherwise be accepted
/// by the support OCR's substring-tolerant fuzzy matcher. This is intentionally
/// limited to equal/containing names so ordinary fuzzy OCR correction keeps
/// working for unrelated servants.
pub(crate) fn servant_name_overlaps_other_id(meta: &ServantMetadata, server: Server) -> bool {
    let target_names = meta.names.clone();
    for sibling in servants_data()
        .iter()
        .filter(|servant| servant.id != meta.id)
    {
        for sibling_name in localized_variant_names(sibling, server) {
            if target_names
                .iter()
                .any(|target_name| servant_names_overlap(target_name, &sibling_name))
            {
                return true;
            }
        }
    }
    false
}

fn localized_variant_name(servant: &ServantInfo, server: Server) -> String {
    match server {
        Server::Jp => servant.name_jp.trim().to_string(),
        Server::Cn => servant
            .name_cn_server
            .as_deref()
            .unwrap_or(&servant.name_cn)
            .trim()
            .to_string(),
    }
}

fn localized_variant_names(servant: &ServantInfo, server: Server) -> Vec<String> {
    match server {
        Server::Jp => servant.recognition_names_jp.clone(),
        Server::Cn => servant.recognition_names_cn.clone(),
    }
}

fn localized_variant_np_names(servant: &ServantInfo, server: Server) -> Vec<String> {
    match server {
        Server::Jp => servant.recognition_np_names_jp.clone(),
        Server::Cn => servant.recognition_np_names_cn.clone(),
    }
}

#[derive(Debug, PartialEq)]
pub(crate) struct ServantVariantCandidates {
    pub(crate) target_name: String,
    pub(crate) target_names: Vec<String>,
    pub(crate) excluded_names: Vec<String>,
    pub(crate) np_names: Vec<String>,
    pub(crate) shares_name_with_sibling: bool,
}

pub(crate) fn servant_variant_name_candidates(
    id: u32,
    variant_key: &str,
    server: Server,
) -> Result<ServantVariantCandidates, String> {
    let target = servants_data()
        .iter()
        .find(|servant| servant.id == id && servant.variant_key == variant_key)
        .ok_or_else(|| format!("从者 #{id} 不包含立绘集合 {variant_key}"))?;
    let target_name = localized_variant_name(target, server);
    if target_name.is_empty() {
        return Err(format!(
            "从者 #{id} 的立绘集合 {variant_key} 缺少本地化名称"
        ));
    }
    let mut target_names = localized_variant_names(target, server);
    push_unique_nonempty(&mut target_names, &target_name);

    let mut excluded_names = Vec::new();
    let mut shares_name_with_sibling = false;
    for sibling in servants_data().iter().filter(|servant| servant.id == id) {
        if sibling.variant_key == variant_key {
            continue;
        }
        for sibling_name in localized_variant_names(sibling, server) {
            if target_names.iter().any(|name| name == &sibling_name) {
                shares_name_with_sibling = true;
            } else {
                push_unique_nonempty(&mut excluded_names, sibling_name);
            }
        }
    }
    if target_names.iter().any(|target_name| {
        servants_data()
            .iter()
            .filter(|servant| servant.id != id)
            .flat_map(|servant| localized_variant_names(servant, server))
            .any(|sibling_name| servant_names_overlap(target_name, &sibling_name))
    }) {
        shares_name_with_sibling = true;
    }
    Ok(ServantVariantCandidates {
        target_name,
        target_names,
        excluded_names,
        np_names: localized_variant_np_names(target, server),
        shares_name_with_sibling,
    })
}

pub(crate) fn load_servant_metadata_for_variant(
    app: &tauri::AppHandle,
    id: u32,
    server: Server,
    variant_key: Option<&str>,
) -> Result<ServantMetadata, String> {
    let mut meta = load_servant_metadata(app, id, server)?;
    let Some(variant_key) = variant_key.filter(|key| !key.trim().is_empty()) else {
        return Ok(meta);
    };

    let candidates = servant_variant_name_candidates(id, variant_key, server)?;
    apply_servant_variant_candidates(&mut meta, candidates, server);
    Ok(meta)
}

pub(crate) fn apply_servant_variant_candidates(
    meta: &mut ServantMetadata,
    candidates: ServantVariantCandidates,
    server: Server,
) {
    meta.name = candidates.target_name;
    meta.names = candidates.target_names;
    meta.excluded_names = candidates.excluded_names;
    if !candidates.np_names.is_empty() {
        meta.np_names = candidates.np_names;
    }
    let ambiguous = servant_name_overlaps_other_id(meta, server);
    meta.require_np_match =
        (candidates.shares_name_with_sibling || ambiguous) && !meta.np_names.is_empty();
}

pub(crate) fn localize_servant_name_by_id(id: u32, jp: &str) -> String {
    servant_id_to_cn_index()
        .get(&id)
        .cloned()
        .unwrap_or_else(|| localize_servant_name(jp))
}

/// Translate one Atlas JP servant name into the string the CN client
/// renders. Falls back to the JP name if no mapping exists so OCR still
/// has *some* target to fuzzy-match against rather than no name at all.
pub(crate) fn localize_servant_name(jp: &str) -> String {
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
pub(crate) fn localize_np_names(jp_names: &[String]) -> Vec<String> {
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
/// Reads `<servant_assets_dir>/{id}/servant.json` for JP and
/// `<servant_assets_dir>/{id}/servant-cn.json` for CN when available,
/// pulling the top-level `name` field plus every distinct
/// `noblePhantasms[].name`. Older asset packs without `servant-cn.json`
/// fall back to JP metadata translated through `resources/servants.json`
/// (`nameJP/name_jp -> nameCN/name_cn`). Unmapped NPs are dropped — when
/// the resulting `np_names` list is empty the sidecar transparently
/// falls back to name-only matching, so support detection still proceeds
/// at lower precision.
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
    let servant_dir = assets_dir.join(id.to_string());
    let cn_path = servant_dir.join("servant-cn.json");
    let jp_path = servant_dir.join("servant.json");
    let path = if server == Server::Cn && cn_path.is_file() {
        cn_path
    } else {
        jp_path
    };
    let raw = fs::read_to_string(&path)
        .map_err(|e| format!("无法读取 servant 元数据 ({}): {e}", path.display()))?;
    let json: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|e| format!("servant 元数据解析失败 ({}): {e}", path.display()))?;
    let raw_name = json
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("servant 元数据缺少 'name' 字段: {}", path.display()))?
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
    let mut raw_np_names: Vec<String> = Vec::new();
    if let Some(arr) = json.get("noblePhantasms").and_then(|v| v.as_array()) {
        for entry in arr {
            if let Some(n) = entry.get("name").and_then(|v| v.as_str()) {
                let trimmed = n.trim();
                if !trimmed.is_empty() && !raw_np_names.iter().any(|x| x == trimmed) {
                    raw_np_names.push(trimmed.to_string());
                }
            }
        }
    }

    let (name, np_names) = match server {
        Server::Jp => (raw_name, raw_np_names),
        Server::Cn if path.ends_with("servant-cn.json") => (raw_name, raw_np_names),
        Server::Cn => {
            let cn_name = localize_servant_name_by_id(id, &raw_name);
            let cn_nps = localize_np_names(&raw_np_names);
            let dropped = raw_np_names.len().saturating_sub(cn_nps.len());
            if dropped > 0 {
                eprintln!(
                    "[load_servant_metadata] servant {id}: dropped {dropped} unmapped NP name(s) for CN (will fall back to name-only matching if all dropped)"
                );
            }
            (cn_name, cn_nps)
        }
    };

    let names = localized_servant_names_by_id(id, server, &name);
    let mut meta = ServantMetadata {
        id,
        name,
        names,
        excluded_names: Vec::new(),
        np_names,
        require_np_match: false,
        class_name,
    };
    let ambiguous = servant_name_overlaps_other_id(&meta, server);
    meta.require_np_match = ambiguous && !meta.np_names.is_empty();
    cache.lock().unwrap().insert((id, server), meta.clone());
    Ok(meta)
}

#[tauri::command]
pub(crate) fn get_servant_metadata(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<Server>>,
    id: u32,
    variant_key: Option<String>,
) -> Result<ServantMetadata, String> {
    let server = *state.lock().unwrap();
    load_servant_metadata_for_variant(&app, id, server, variant_key.as_deref())
}
