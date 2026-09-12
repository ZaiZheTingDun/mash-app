//! Servant, craft essence, and template catalog access.
//! Localization rules live here so runners receive server-specific metadata.

use super::projects::update_app_ui_settings;
use super::*;

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServantNameAlias {
    pub(crate) ids: Vec<u32>,
    pub(crate) name_jp: Option<String>,
    pub(crate) name_cn: Option<String>,
}

#[derive(serde::Serialize, Clone)]
pub(crate) struct ServantInfo {
    pub(crate) id: u32,
    #[serde(skip_serializing)]
    pub(crate) servant_type: String,
    #[serde(rename = "variantKey")]
    pub(crate) variant_key: String,
    #[serde(rename = "faceId")]
    pub(crate) face_id: Option<u32>,
    #[serde(skip_serializing)]
    pub(crate) portrait_ids: Vec<u32>,
    #[serde(skip_serializing)]
    pub(crate) recognition_names_cn: Vec<String>,
    #[serde(skip_serializing)]
    pub(crate) recognition_names_jp: Vec<String>,
    #[serde(skip_serializing)]
    pub(crate) recognition_np_names_cn: Vec<String>,
    #[serde(skip_serializing)]
    pub(crate) recognition_np_names_jp: Vec<String>,
    pub(crate) name_cn: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) name_cn_server: Option<String>,
    pub(crate) name_jp: String,
    pub(crate) name_en: String,
    pub(crate) name_other: Option<String>,
    #[serde(
        rename = "overWriteServantNames",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub(crate) over_write_servant_names: Vec<ServantNameAlias>,
    pub(crate) class: String,
    pub(crate) rarity: u32,
    #[serde(rename = "noblePhantasmName")]
    pub(crate) noble_phantasm_name: Option<String>,
    #[serde(rename = "noblePhantasmCard")]
    pub(crate) noble_phantasm_card: Option<String>,
}
pub(crate) fn load_enhancement_target(
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

pub(crate) fn preferred_cn_name(value: &serde_json::Value) -> Option<String> {
    string_field(value, &["name_cn_server", "nameCNServer"])
        .or_else(|| string_field(value, &["name_cn", "nameCN"]))
}

pub(crate) fn first_np_name(s: &serde_json::Value) -> Option<String> {
    let nps = s
        .get("noblePhantasms")
        .or_else(|| s.get("noble_phantasms"))?;
    let entry = if let Some(arr) = nps.as_array() {
        arr.first()?
    } else {
        nps.as_object()?.values().next()?
    };
    preferred_cn_name(entry).or_else(|| string_field(entry, &["name"]))
}

pub(crate) fn string_field(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .filter_map(|key| value.get(key))
        .filter_map(|v| v.as_str())
        .map(str::trim)
        .find(|s| !s.is_empty())
        .map(str::to_string)
}

pub(crate) fn push_unique_nonempty(out: &mut Vec<String>, value: impl AsRef<str>) {
    let trimmed = value.as_ref().trim();
    if !trimmed.is_empty() && !out.iter().any(|existing| existing == trimmed) {
        out.push(trimmed.to_string());
    }
}

pub(crate) fn servant_name_aliases(value: &serde_json::Value) -> Vec<ServantNameAlias> {
    let Some(arr) = value
        .get("overWriteServantNames")
        .or_else(|| value.get("overwriteServantNames"))
        .or_else(|| value.get("over_write_servant_names"))
        .and_then(|v| v.as_array())
    else {
        return Vec::new();
    };

    arr.iter()
        .filter_map(|entry| {
            let alias = ServantNameAlias {
                ids: u32_array_field(entry, &["ids"]),
                name_jp: string_field(entry, &["nameJP", "name_jp"]),
                name_cn: string_field(entry, &["nameCN", "name_cn"]),
            };
            if alias.name_jp.is_none() && alias.name_cn.is_none() {
                None
            } else {
                Some(alias)
            }
        })
        .collect()
}

pub(crate) fn u32_field(value: &serde_json::Value, keys: &[&str]) -> Option<u32> {
    keys.iter()
        .filter_map(|key| value.get(key))
        .find_map(|v| v.as_u64())
        .map(|n| n as u32)
}

pub(crate) fn u32_array_field(value: &serde_json::Value, keys: &[&str]) -> Vec<u32> {
    keys.iter()
        .filter_map(|key| value.get(key))
        .find_map(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_u64().map(|n| n as u32))
        .collect()
}

pub(crate) fn display_class_name(raw: &str) -> String {
    match raw.trim() {
        "alterEgo" => "Alterego".into(),
        "moonCancer" => "Moon Cancer".into(),
        "uOlgaMarieAquaCollection" => "U-Olga Marie Aqua".into(),
        "uOlgaMarieFlareCollection" => "U-Olga Marie Flare".into(),
        "uOlgaMarieGrandCollection" => "U-Olga Marie Grand".into(),
        "uOlgaMarieStellarCollection" => "U-Olga Marie Stellar".into(),
        "beastEresh" | "unBeastOlgaMarie" => "Beast".into(),
        other if other.is_empty() => String::new(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        }
    }
}

pub(crate) fn is_selectable_servant_type(servant_type: &str) -> bool {
    matches!(servant_type, "normal" | "heroine")
}

pub(crate) fn normalize_np_card(card: &str) -> Option<String> {
    match card.trim().to_ascii_lowercase().as_str() {
        "1" => Some("arts".into()),
        "2" => Some("buster".into()),
        "3" => Some("quick".into()),
        "buster" => Some("buster".into()),
        "arts" => Some("arts".into()),
        "quick" => Some("quick".into()),
        _ => None,
    }
}

pub(crate) fn first_np_card(s: &serde_json::Value) -> Option<String> {
    let nps = s
        .get("noblePhantasms")
        .or_else(|| s.get("noble_phantasms"))?;
    let entry = if let Some(arr) = nps.as_array() {
        arr.first()?
    } else {
        nps.as_object()?.values().next()?.as_array()?.first()?
    };
    string_field(entry, &["cardName", "card"])
        .as_deref()
        .and_then(normalize_np_card)
}

pub(crate) fn last_variant_np_name(variant: &serde_json::Value) -> Option<String> {
    variant_np_names(variant).into_iter().next_back()
}

pub(crate) fn variant_np_names(variant: &serde_json::Value) -> Vec<String> {
    let Some(arr) = variant
        .get("noblePhantasms_cn")
        .or_else(|| variant.get("noblePhantasms"))
        .and_then(serde_json::Value::as_array)
    else {
        return Vec::new();
    };
    let mut names = Vec::new();
    for value in arr {
        if let Some(name) = value.as_str() {
            push_unique_nonempty(&mut names, name);
        }
    }
    names
}

pub(crate) fn variant_face_id(variant: &serde_json::Value) -> Option<u32> {
    u32_array_field(variant, &["ids"]).into_iter().max()
}

/// Keep the displayed name aligned with the same ascension/costume ID chosen
/// as this variant's representative portrait.
pub(crate) fn variant_name_alias<'a>(
    aliases: &'a [ServantNameAlias],
    face_id: Option<u32>,
) -> Option<&'a ServantNameAlias> {
    let face_id = face_id?;
    aliases.iter().find(|alias| alias.ids.contains(&face_id))
}

fn variant_recognition_names(
    base_name: &str,
    aliases: &[ServantNameAlias],
    ids: &[u32],
    server: Server,
) -> Vec<String> {
    fn alias_name(alias: &ServantNameAlias, server: Server) -> Option<&str> {
        match server {
            Server::Jp => alias.name_jp.as_deref(),
            Server::Cn => alias.name_cn.as_deref(),
        }
    }
    let mut names = Vec::new();
    if ids.is_empty() {
        push_unique_nonempty(&mut names, base_name);
        for alias in aliases {
            if let Some(name) = alias_name(alias, server) {
                push_unique_nonempty(&mut names, name);
            }
        }
        return names;
    }

    for id in ids {
        let name = aliases
            .iter()
            .find(|alias| alias.ids.contains(id))
            .and_then(|alias| alias_name(alias, server))
            .unwrap_or(base_name);
        push_unique_nonempty(&mut names, name);
    }
    names
}

pub(crate) fn servants_data() -> &'static [ServantInfo] {
    static SERVANTS: OnceLock<Vec<ServantInfo>> = OnceLock::new();
    SERVANTS.get_or_init(|| {
        let raw: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../resources/servants.json"))
                .expect("invalid servants.json");
        let variants_raw: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../resources/servants_variants.json"))
                .expect("invalid servants_variants.json");
        let variants_cn_raw: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../resources/servants_variants_cn.json"))
                .expect("invalid servants_variants_cn.json");
        let variants_by_id: HashMap<u32, Vec<serde_json::Value>> = variants_raw
            .into_iter()
            .filter_map(|entry| {
                let id = entry.get("id")?.as_u64()? as u32;
                let variants = entry.get("variants")?.as_array()?.clone();
                Some((id, variants))
            })
            .collect();
        let cn_variants_by_id: HashMap<u32, Vec<serde_json::Value>> = variants_cn_raw
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
                    let id = u32_field(s, &["collectionNo", "id"])?;
                    let name_cn = string_field(s, &["nameCN", "name_cn"])?;
                    let name_cn_server = string_field(s, &["nameCNServer", "name_cn_server"]);
                    let name_jp = string_field(s, &["nameJP", "name_jp"])?;
                    let name_en = string_field(s, &["nameEN", "name_en", "name"]).unwrap_or_default();
                    let name_other = string_field(s, &["nameOther", "name_other"]);
                    let over_write_servant_names = servant_name_aliases(s);
                    let servant_type = string_field(s, &["type"])?;

                    let (class, rarity) = if let Some(class_name) =
                        string_field(s, &["className", "class"])
                    {
                        let rarity = u32_field(s, &["rarity"])?;
                        (display_class_name(&class_name), rarity)
                    } else {
                        let base_stats = s.get("base_stats")?.as_object()?;
                        if let Some(cls) = base_stats.get("class") {
                            let class = cls.as_str()?.to_string();
                            let rarity = base_stats.get("rarity")?.as_u64()? as u32;
                            (class, rarity)
                        } else {
                            let first_variant = base_stats.values().next()?.as_object()?;
                            let class = first_variant.get("class")?.as_str()?.to_string();
                            let rarity = first_variant.get("rarity")?.as_u64()? as u32;
                            (class, rarity)
                        }
                    };

                    let base_np = first_np_name(s);
                    let base_np_card = first_np_card(s);
                    let variants = variants_by_id.get(&id);
                    let cn_variants = cn_variants_by_id.get(&id);
                    let infos: Vec<ServantInfo> = if let Some(variants) = variants {
                        variants
                            .iter()
                            .enumerate()
                            .map(|(variant_idx, variant)| {
                                let np_variant = cn_variants
                                    .and_then(|entries| entries.get(variant_idx))
                                    .unwrap_or(variant);
                                let portrait_ids = u32_array_field(variant, &["ids"]);
                                let face_id = variant_face_id(variant);
                                let recognition_names_cn = variant_recognition_names(
                                    name_cn_server.as_deref().unwrap_or(&name_cn),
                                    &over_write_servant_names,
                                    &portrait_ids,
                                    Server::Cn,
                                );
                                let recognition_names_jp = variant_recognition_names(
                                    &name_jp,
                                    &over_write_servant_names,
                                    &portrait_ids,
                                    Server::Jp,
                                );
                                let recognition_np_names_cn = variant_np_names(np_variant);
                                let recognition_np_names_jp = variant_np_names(variant);
                                let name_alias =
                                    variant_name_alias(&over_write_servant_names, face_id);
                                let variant_name_cn = name_alias
                                    .and_then(|alias| alias.name_cn.clone())
                                    .unwrap_or_else(|| name_cn.clone());
                                let variant_name_jp = name_alias
                                    .and_then(|alias| alias.name_jp.clone())
                                    .unwrap_or_else(|| name_jp.clone());
                                let variant_name_cn_server = if name_alias
                                    .is_some_and(|alias| alias.name_cn.is_some())
                                {
                                    None
                                } else {
                                    name_cn_server.clone()
                                };
                                ServantInfo {
                                    id,
                                    servant_type: servant_type.clone(),
                                    variant_key: format!("{id}:{}", variant_idx + 1),
                                    face_id,
                                    portrait_ids,
                                    recognition_names_cn,
                                    recognition_names_jp,
                                    recognition_np_names_cn,
                                    recognition_np_names_jp,
                                    name_cn: variant_name_cn,
                                    name_cn_server: variant_name_cn_server,
                                    name_jp: variant_name_jp,
                                    name_en: name_en.clone(),
                                    name_other: name_other.clone(),
                                    over_write_servant_names: over_write_servant_names.clone(),
                                    class: class.clone(),
                                    rarity,
                                    noble_phantasm_name: last_variant_np_name(np_variant)
                                        .or_else(|| base_np.clone()),
                                    noble_phantasm_card: base_np_card.clone(),
                                }
                            })
                            .collect()
                    } else {
                        vec![ServantInfo {
                            id,
                            servant_type,
                            variant_key: id.to_string(),
                            face_id: None,
                            portrait_ids: Vec::new(),
                            recognition_names_cn: variant_recognition_names(
                                name_cn_server.as_deref().unwrap_or(&name_cn),
                                &over_write_servant_names,
                                &[],
                                Server::Cn,
                            ),
                            recognition_names_jp: variant_recognition_names(
                                &name_jp,
                                &over_write_servant_names,
                                &[],
                                Server::Jp,
                            ),
                            recognition_np_names_cn: Vec::new(),
                            recognition_np_names_jp: Vec::new(),
                            name_cn,
                            name_cn_server,
                            name_jp,
                            name_en,
                            name_other,
                            over_write_servant_names,
                            class,
                            rarity,
                            noble_phantasm_name: base_np,
                            noble_phantasm_card: base_np_card,
                        }]
                    };
                    Some(infos)
                })();

                if result.is_none() {
                    let id_hint = s
                        .get("collectionNo")
                        .or_else(|| s.get("id"))
                        .and_then(|v| v.as_u64());
                    eprintln!(
                        "[servants] dropping entry at index {idx} (id={id_hint:?}): missing or invalid fields"
                    );
                }
                result.unwrap_or_default()
            })
            .collect()
    })
}

pub(crate) fn servant_np_card(id: u32) -> Option<String> {
    servants_data()
        .iter()
        .find(|servant| servant.id == id)
        .and_then(|servant| servant.noble_phantasm_card.clone())
}

pub(crate) fn selectable_servants_data() -> &'static [ServantInfo] {
    static SELECTABLE_SERVANTS: OnceLock<Vec<ServantInfo>> = OnceLock::new();
    SELECTABLE_SERVANTS.get_or_init(|| {
        servants_data()
            .iter()
            .filter(|servant| is_selectable_servant_type(&servant.servant_type))
            .cloned()
            .collect()
    })
}

#[tauri::command]
pub(crate) fn get_servants() -> &'static [ServantInfo] {
    selectable_servants_data()
}

/// One craft-essence entry exposed to the frontend. Mirrors the shape of
/// `resources/craft_essences.json` (using Atlas `collectionNo` as the
/// stable app-facing CE id). Kept minimal — additional metadata lives in the per-CE
/// `assets/ces/{id}/craft-essence.json` Atlas dump and is loaded lazily
/// only when the runner actually needs it.
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CraftEssenceInfo {
    pub id: u32,
    pub rarity: u8,
    pub category: CraftEssenceCategory,
    pub name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub name_aliases: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_link: Option<String>,
}

/// The subset of craft-essence categories exposed by the picker.  Atlas has
/// additional flags (such as campaign and chocolate); those remain selectable
/// through the unfiltered list but intentionally have no dedicated filter.
#[derive(serde::Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CraftEssenceCategory {
    Normal,
    Bond,
    ManaExchange,
    Event,
    EventReward,
    Other,
}

#[derive(serde::Deserialize)]
struct CraftEssenceTranslationFix {
    id: u32,
    name: String,
}

fn craft_essence_category(value: &serde_json::Value) -> CraftEssenceCategory {
    let has_flag = |expected: &str| {
        value
            .get("flags")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|flags| flags.iter().any(|flag| flag.as_str() == Some(expected)))
    };

    if value.get("flag").and_then(serde_json::Value::as_str) == Some("normal") {
        CraftEssenceCategory::Normal
    } else if has_flag("svtEquipFriendShip") {
        CraftEssenceCategory::Bond
    } else if has_flag("svtEquipManaExchange") {
        CraftEssenceCategory::ManaExchange
    } else if has_flag("svtEquipEvent") {
        CraftEssenceCategory::Event
    } else if has_flag("svtEquipEventReward") {
        CraftEssenceCategory::EventReward
    } else {
        CraftEssenceCategory::Other
    }
}

pub(crate) fn craft_essences_data() -> &'static [CraftEssenceInfo] {
    static CES: OnceLock<Vec<CraftEssenceInfo>> = OnceLock::new();
    CES.get_or_init(|| {
        let raw: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../resources/craft_essences.json"))
                .expect("invalid craft_essences.json");
        let translation_fixes: HashMap<u32, String> = serde_json::from_str::<
            Vec<CraftEssenceTranslationFix>,
        >(include_str!(
            "../resources/craft_essence_translation_fixes.json"
        ))
        .expect("invalid craft_essence_translation_fixes.json")
        .into_iter()
        .map(|fix| (fix.id, fix.name))
        .collect();
        raw.iter()
            .enumerate()
            .filter_map(|(idx, ce)| {
                let result = (|| {
                    let id = ce.get("collectionNo")?.as_u64()? as u32;
                    let rarity = u8::try_from(ce.get("rarity")?.as_u64()?).ok()?;
                    if !(1..=5).contains(&rarity) {
                        return None;
                    }
                    let raw_name = ce
                        .get("name_cn")
                        .and_then(|v| v.as_str())
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .or_else(|| ce.get("name").and_then(|v| v.as_str()))
                        .map(str::to_string)?;
                    let fixed_name = translation_fixes.get(&id);
                    let name = fixed_name.cloned().unwrap_or_else(|| raw_name.clone());
                    let name_aliases = if fixed_name.is_some() && raw_name != name {
                        vec![raw_name]
                    } else {
                        Vec::new()
                    };
                    let name_link = ce
                        .get("name_link")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    Some(CraftEssenceInfo {
                        id,
                        rarity,
                        category: craft_essence_category(ce),
                        name,
                        name_aliases,
                        name_link,
                    })
                })();
                if result.is_none() {
                    let id_hint = ce.get("collectionNo").and_then(|v| v.as_u64());
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
pub(crate) fn get_craft_essences() -> &'static [CraftEssenceInfo] {
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
pub(crate) fn pick_portrait_in(servant_dir: &std::path::Path) -> Option<PathBuf> {
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

pub(crate) fn pick_portrait_by_id_in(
    servant_dir: &std::path::Path,
    portrait_id: u32,
) -> Option<PathBuf> {
    let candidate = servant_dir.join(format!("narrow_servant_{portrait_id}.png"));
    if candidate.is_file() {
        Some(candidate)
    } else {
        None
    }
}

pub(crate) fn pick_face_in(servant_dir: &std::path::Path) -> Option<PathBuf> {
    pick_faces_desc_in(servant_dir).into_iter().next()
}

pub(crate) fn pick_faces_desc_in(servant_dir: &std::path::Path) -> Vec<PathBuf> {
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

pub(crate) fn is_face_template_path(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with("face_servant_") && n.ends_with(".png"))
        .unwrap_or(false)
}

pub(crate) fn face_template_stage(path: &std::path::Path) -> u32 {
    path.file_stem()
        .and_then(|n| n.to_str())
        .and_then(|n| n.strip_prefix("face_servant_"))
        .and_then(|n| n.parse::<u32>().ok())
        .unwrap_or(0)
}

pub(crate) fn pick_face_by_id_in(servant_dir: &std::path::Path, face_id: u32) -> Option<PathBuf> {
    let candidate = servant_dir.join(format!("face_servant_{face_id}.png"));
    if candidate.is_file() {
        Some(candidate)
    } else {
        None
    }
}

/// Enumerate all `narrow_servant_*.png` files in `servant_dir` in
/// natural numeric order, returning `(id, path)` pairs.
pub(crate) fn list_portraits_in(servant_dir: &std::path::Path) -> Vec<(u32, PathBuf)> {
    let Ok(entries) = fs::read_dir(servant_dir) else {
        return Vec::new();
    };
    let mut portraits: Vec<(u32, PathBuf)> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter_map(|p| {
            let name = p.file_name()?.to_str()?;
            let stem = name.strip_prefix("narrow_servant_")?.strip_suffix(".png")?;
            let id = stem.parse::<u32>().ok()?;
            Some((id, p))
        })
        .collect();
    portraits.sort_by_key(|(id, _)| *id);
    portraits
}

pub(crate) fn portrait_id_is_allowed(allowed_ids: &[u32], portrait_id: u32) -> bool {
    allowed_ids.is_empty() || allowed_ids.contains(&portrait_id)
}

pub(crate) fn list_portraits_for_ids_in(
    servant_dir: &std::path::Path,
    allowed_ids: &[u32],
) -> Vec<(u32, PathBuf)> {
    list_portraits_in(servant_dir)
        .into_iter()
        .filter(|(id, _)| portrait_id_is_allowed(allowed_ids, *id))
        .collect()
}

pub(crate) fn pick_portrait_for_ids_with_preferences_in(
    servant_dir: &std::path::Path,
    global_id: Option<u32>,
    face_id: Option<u32>,
    allowed_ids: &[u32],
) -> Option<PathBuf> {
    global_id
        .filter(|id| portrait_id_is_allowed(allowed_ids, *id))
        .and_then(|id| pick_portrait_by_id_in(servant_dir, id))
        .or_else(|| {
            face_id
                .filter(|id| portrait_id_is_allowed(allowed_ids, *id))
                .and_then(|id| pick_portrait_by_id_in(servant_dir, id))
        })
        .or_else(|| {
            if allowed_ids.is_empty() {
                pick_portrait_in(servant_dir)
            } else {
                list_portraits_for_ids_in(servant_dir, allowed_ids)
                    .into_iter()
                    .next_back()
                    .map(|(_, path)| path)
            }
        })
}

pub(crate) fn pick_portrait_with_preferences_in(
    servant_dir: &std::path::Path,
    global_id: Option<u32>,
    face_id: Option<u32>,
) -> Option<PathBuf> {
    pick_portrait_for_ids_with_preferences_in(servant_dir, global_id, face_id, &[])
}

pub(crate) fn pick_face_for_ids_with_preferences_in(
    servant_dir: &std::path::Path,
    global_id: Option<u32>,
    face_id: Option<u32>,
    allowed_ids: &[u32],
) -> Option<PathBuf> {
    global_id
        .filter(|id| portrait_id_is_allowed(allowed_ids, *id))
        .and_then(|id| pick_face_by_id_in(servant_dir, id))
        .or_else(|| {
            face_id
                .filter(|id| portrait_id_is_allowed(allowed_ids, *id))
                .and_then(|id| pick_face_by_id_in(servant_dir, id))
        })
        .or_else(|| {
            if allowed_ids.is_empty() {
                pick_face_in(servant_dir)
            } else {
                pick_faces_desc_in(servant_dir)
                    .into_iter()
                    .find(|path| portrait_id_is_allowed(allowed_ids, face_template_stage(path)))
            }
        })
}

fn servant_for_variant(servant_id: u32, variant_key: &str) -> Result<&'static ServantInfo, String> {
    servants_data()
        .iter()
        .find(|servant| servant.id == servant_id && servant.variant_key == variant_key)
        .ok_or_else(|| format!("从者 #{servant_id} 不包含立绘集合 {variant_key}"))
}

fn servant_by_variant_key(variant_key: &str) -> Result<&'static ServantInfo, String> {
    servants_data()
        .iter()
        .find(|servant| servant.variant_key == variant_key)
        .ok_or_else(|| format!("未找到立绘集合 {variant_key}"))
}

/// Resolve the full-art portrait file for a single servant, returning
/// the absolute path so the frontend can hand it to `convertFileSrc()`.
///
/// Priority: (1) global portrait selection stored in `app_ui_settings.json`
/// for this `variant_key`, (2) explicit `face_id` variant default,
/// (3) default "highest ascension" portrait via [`pick_portrait_in`].
/// Returning `Ok(None)` on a missing file lets the UI fall back to a
/// placeholder without surfacing an error toast.
#[tauri::command]
pub(crate) fn get_servant_portrait_path(
    app: tauri::AppHandle,
    settings_state: tauri::State<'_, Mutex<AppUiSettings>>,
    servant_id: u32,
    face_id: Option<u32>,
    variant_key: Option<String>,
) -> Result<Option<String>, String> {
    let Some(root) = resolve_servant_assets_dir(&app) else {
        return Ok(None);
    };
    let servant_dir = root.join(servant_id.to_string());
    let variant = variant_key
        .as_deref()
        .map(|vk| servant_for_variant(servant_id, vk))
        .transpose()?;
    let allowed_ids = variant
        .map(|servant| servant.portrait_ids.as_slice())
        .unwrap_or_default();
    let settings = settings_state.lock().unwrap();
    let global_id = variant_key
        .as_deref()
        .and_then(|vk| settings.servant_portrait_selections.get(vk).copied());
    let picked = if allowed_ids.is_empty() {
        pick_portrait_with_preferences_in(&servant_dir, global_id, face_id)
    } else {
        pick_portrait_for_ids_with_preferences_in(&servant_dir, global_id, face_id, allowed_ids)
    };
    Ok(picked.map(|p| p.to_string_lossy().into_owned()))
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PortraitOption {
    pub(crate) id: u32,
    pub(crate) path: String,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServantPortraitOptions {
    pub(crate) options: Vec<PortraitOption>,
    pub(crate) selected_id: Option<u32>,
}

/// List all available full-art portraits for `servant_id` in ascending
/// numeric order, along with the currently-selected portrait id (if any)
/// for this `variant_key` from the global settings.
#[tauri::command]
pub(crate) fn list_servant_portraits(
    app: tauri::AppHandle,
    settings_state: tauri::State<'_, Mutex<AppUiSettings>>,
    servant_id: u32,
    variant_key: String,
) -> Result<ServantPortraitOptions, String> {
    let servant = servant_for_variant(servant_id, &variant_key)?;
    let Some(root) = resolve_servant_assets_dir(&app) else {
        return Ok(ServantPortraitOptions {
            options: Vec::new(),
            selected_id: None,
        });
    };
    let servant_dir = root.join(servant_id.to_string());
    let options: Vec<PortraitOption> =
        list_portraits_for_ids_in(&servant_dir, &servant.portrait_ids)
            .into_iter()
            .map(|(id, path)| PortraitOption {
                id,
                path: path.to_string_lossy().into_owned(),
            })
            .collect();
    let settings = settings_state.lock().unwrap();
    let selected_id = settings
        .servant_portrait_selections
        .get(&variant_key)
        .copied()
        .filter(|id| {
            portrait_id_is_allowed(&servant.portrait_ids, *id)
                && options.iter().any(|option| option.id == *id)
        });
    Ok(ServantPortraitOptions {
        options,
        selected_id,
    })
}

/// Persist a global portrait selection for `variant_key`. This overrides
/// the default highest-ascension pick for all teams using this variant.
#[tauri::command]
pub(crate) fn save_servant_portrait_selection(
    app: tauri::AppHandle,
    settings_state: tauri::State<'_, Mutex<AppUiSettings>>,
    variant_key: String,
    portrait_id: u32,
) -> Result<(), String> {
    let servant = servant_by_variant_key(&variant_key)?;
    if !portrait_id_is_allowed(&servant.portrait_ids, portrait_id) {
        return Err(format!(
            "立绘 {portrait_id} 不属于从者 {} 的当前集合",
            servant.name_cn
        ));
    }
    let root =
        resolve_servant_assets_dir(&app).ok_or_else(|| "未找到从者立绘资源目录".to_string())?;
    if pick_portrait_by_id_in(&root.join(servant.id.to_string()), portrait_id).is_none() {
        return Err(format!("从者 {} 缺少立绘 {portrait_id}", servant.name_cn));
    }
    update_app_ui_settings(&app, settings_state.inner(), |settings| {
        settings
            .servant_portrait_selections
            .insert(variant_key, portrait_id);
    })?;
    Ok(())
}

#[tauri::command]
pub(crate) fn get_servant_face_path(
    app: tauri::AppHandle,
    settings_state: tauri::State<'_, Mutex<AppUiSettings>>,
    servant_id: u32,
    face_id: Option<u32>,
    variant_key: Option<String>,
) -> Result<Option<String>, String> {
    let Some(root) = resolve_servant_assets_dir(&app) else {
        return Ok(None);
    };
    let servant_dir = root.join(servant_id.to_string());
    let variant = variant_key
        .as_deref()
        .map(|vk| servant_for_variant(servant_id, vk))
        .transpose()?;
    let allowed_ids = variant
        .map(|servant| servant.portrait_ids.as_slice())
        .unwrap_or_default();
    let settings = settings_state.lock().unwrap();
    let global_id = variant_key
        .as_deref()
        .and_then(|vk| settings.servant_portrait_selections.get(vk).copied());
    let picked =
        pick_face_for_ids_with_preferences_in(&servant_dir, global_id, face_id, allowed_ids);
    Ok(picked.map(|p| p.to_string_lossy().into_owned()))
}

/// Pure helper so [`get_craft_essence_card_path`] stays a thin wrapper
/// and the file-walk logic is unit-testable without spinning up a
/// `tauri::AppHandle`. Returns `Some(path)` when
/// `<ce_root>/<id>/card_ce.png` exists, `None` otherwise.
pub(crate) fn pick_ce_card_in(ce_root: &std::path::Path, ce_id: u32) -> Option<PathBuf> {
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
pub(crate) fn get_craft_essence_card_path(
    app: tauri::AppHandle,
    craft_essence_id: u32,
) -> Result<Option<String>, String> {
    let Some(root) = resolve_ce_assets_dir(&app) else {
        return Ok(None);
    };
    Ok(pick_ce_card_in(&root, craft_essence_id).map(|p| p.to_string_lossy().into_owned()))
}

fn variants_raw_data() -> &'static HashMap<u32, Vec<serde_json::Value>> {
    static VARIANTS: OnceLock<HashMap<u32, Vec<serde_json::Value>>> = OnceLock::new();
    VARIANTS.get_or_init(|| {
        let raw: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../resources/servants_variants.json"))
                .expect("invalid servants_variants.json");
        raw.into_iter()
            .filter_map(|entry| {
                let id = entry.get("id")?.as_u64()? as u32;
                let variants = entry.get("variants")?.as_array()?.clone();
                Some((id, variants))
            })
            .collect()
    })
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SkillIconEntry {
    pub(crate) path: Option<String>,
    pub(crate) name: String,
}

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SkillTargetingEntry {
    pub(crate) servant_collection_no: u32,
    pub(crate) skill_id: u32,
    pub(crate) skill_num: u32,
    pub(crate) func_target_types: Vec<String>,
    pub(crate) targeting_mode: SkillTargetingMode,
}

#[derive(serde::Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SkillTargetingMode {
    NeedsTarget,
    NoTarget,
    Mixed,
    Unknown,
}

#[derive(serde::Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum MysticCodeSkillMode {
    NeedsTarget,
    NoTarget,
    OrderChange,
    Unknown,
}

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MysticCodeSkillInfo {
    pub(crate) id: u32,
    pub(crate) slot: u32,
    pub(crate) name: String,
    pub(crate) icon_path: Option<String>,
    pub(crate) targeting_mode: MysticCodeSkillMode,
}

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MysticCodeInfo {
    pub(crate) id: u32,
    pub(crate) name: String,
    pub(crate) item_male_path: Option<String>,
    pub(crate) item_female_path: Option<String>,
    pub(crate) skills: Vec<MysticCodeSkillInfo>,
}

fn mystic_code_skill_mode(skill: &serde_json::Value) -> MysticCodeSkillMode {
    let Some(functions) = skill.get("functions").and_then(|value| value.as_array()) else {
        return MysticCodeSkillMode::Unknown;
    };
    let target_types: Vec<&str> = functions
        .iter()
        .filter_map(|function| {
            function
                .get("funcTargetType")
                .and_then(|value| value.as_str())
        })
        .collect();
    if target_types.iter().any(|value| *value == "ptselectOneSub") {
        MysticCodeSkillMode::OrderChange
    } else if target_types
        .iter()
        .any(|value| matches!(*value, "ptOne" | "ptOneOther"))
    {
        MysticCodeSkillMode::NeedsTarget
    } else {
        MysticCodeSkillMode::NoTarget
    }
}

fn mystic_code_item_path(code_dir: &Path, gender: &str) -> Option<String> {
    let path = code_dir.join(format!("item-{gender}.png"));
    path.is_file().then(|| path.to_string_lossy().into_owned())
}

fn mystic_code_icon_path(assets_root: &Path, skill: &serde_json::Value) -> Option<String> {
    let filename = skill
        .get("icon")
        .and_then(|value| value.as_str())
        .and_then(|value| value.rsplit('/').next())?;
    let path = assets_root.join("icons").join(filename);
    path.is_file().then(|| path.to_string_lossy().into_owned())
}

fn load_mystic_codes_from_dir(root: &Path) -> Vec<MysticCodeInfo> {
    let assets_root = root.parent().unwrap_or(root);
    let mut ids = fs::read_dir(root)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            path.is_dir()
                .then(|| entry.file_name().to_string_lossy().parse::<u32>().ok())
                .flatten()
        })
        .collect::<Vec<_>>();
    ids.sort_unstable();

    ids.into_iter()
        .filter_map(|id| {
            let code_dir = root.join(id.to_string());
            let jp: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(code_dir.join("mystic-code.json")).ok()?)
                    .ok()?;
            let cn = fs::read_to_string(code_dir.join("mystic-code-cn.json"))
                .ok()
                .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok());
            let name = cn
                .as_ref()
                .and_then(|value| string_field(value, &["name"]))
                .or_else(|| string_field(&jp, &["name"]))
                .unwrap_or_else(|| format!("御主礼装 {id}"));
            let jp_skills = jp.get("skills").and_then(|value| value.as_array())?;
            let cn_skills = cn
                .as_ref()
                .and_then(|value| value.get("skills"))
                .and_then(|value| value.as_array());
            let skills = jp_skills
                .iter()
                .enumerate()
                .filter_map(|(index, skill)| {
                    let slot = index as u32 + 1;
                    let skill_id = u32_field(skill, &["id"])?;
                    let localized = cn_skills
                        .and_then(|values| values.get(index))
                        .and_then(|value| string_field(value, &["name"]))
                        .or_else(|| string_field(skill, &["name"]))
                        .unwrap_or_else(|| format!("技能 {slot}"));
                    Some(MysticCodeSkillInfo {
                        id: skill_id,
                        slot,
                        name: localized,
                        icon_path: mystic_code_icon_path(assets_root, skill),
                        targeting_mode: mystic_code_skill_mode(skill),
                    })
                })
                .take(3)
                .collect();
            Some(MysticCodeInfo {
                id,
                name,
                item_male_path: mystic_code_item_path(&code_dir, "male"),
                item_female_path: mystic_code_item_path(&code_dir, "female"),
                skills,
            })
        })
        .collect()
}

#[tauri::command]
pub(crate) fn get_mystic_codes(app: tauri::AppHandle) -> Result<Vec<MysticCodeInfo>, String> {
    let root =
        resolve_mystic_code_assets_dir(&app).ok_or_else(|| "未找到御主礼装资源目录".to_string())?;
    Ok(load_mystic_codes_from_dir(&root))
}

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SkillSelectionOption {
    pub(crate) index: u32,
    pub(crate) label: String,
}

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SkillSelectionEntry {
    pub(crate) servant_collection_no: u32,
    pub(crate) skill_id: u32,
    pub(crate) skill_num: u32,
    pub(crate) selection_type: String,
    pub(crate) supplementary_types: Vec<String>,
    pub(crate) options: Vec<SkillSelectionOption>,
}

struct ServantSkillMaps {
    icon_map: HashMap<u32, String>,
    name_map: HashMap<u32, String>,
    target_type_map: HashMap<u32, Vec<String>>,
    selection_map: HashMap<u32, SkillSelectionEntry>,
}

fn skill_target_types(skill: &serde_json::Value) -> Vec<String> {
    let mut values = skill
        .get("functions")
        .and_then(|v| v.as_array())
        .map(|functions| {
            functions
                .iter()
                .filter_map(|func| func.get("funcTargetType").and_then(|v| v.as_str()))
                .filter(|value| matches!(*value, "ptOne" | "ptOneOther"))
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    values.sort();
    values.dedup();
    values
}

fn first_script_entry<'a>(
    skill: &'a serde_json::Value,
    key: &str,
) -> Option<&'a serde_json::Value> {
    skill
        .get("script")?
        .get(key)?
        .as_array()
        .and_then(|values| values.first())
}

fn string_value(value: &serde_json::Value) -> Option<String> {
    value
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
        .or_else(|| value.as_i64().map(|n| n.to_string()))
        .or_else(|| value.as_u64().map(|n| n.to_string()))
}

fn first_non_empty_string<'a>(
    values: impl IntoIterator<Item = Option<&'a serde_json::Value>>,
) -> Option<String> {
    values
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str())
        .find(|value| !value.trim().is_empty())
        .map(str::to_string)
}

fn option_label(index: usize, label: Option<String>) -> SkillSelectionOption {
    SkillSelectionOption {
        index: index as u32,
        label: label.unwrap_or_else(|| format!("选项 {}", index + 1)),
    }
}

fn select_add_info_options(skill: &serde_json::Value) -> Vec<SkillSelectionOption> {
    first_script_entry(skill, "SelectAddInfo")
        .and_then(|entry| entry.get("btn"))
        .and_then(|value| value.as_array())
        .map(|buttons| {
            buttons
                .iter()
                .enumerate()
                .map(|(index, button)| {
                    option_label(index, first_non_empty_string([button.get("name")]))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn select_treasure_device_options(skill: &serde_json::Value) -> Vec<SkillSelectionOption> {
    first_script_entry(skill, "selectTreasureDeviceInfo")
        .and_then(|entry| entry.get("treasureDevices"))
        .and_then(|value| value.as_array())
        .map(|devices| {
            devices
                .iter()
                .enumerate()
                .map(|(index, device)| {
                    option_label(index, first_non_empty_string([device.get("message")]))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn command_card_order(label: Option<&str>) -> Option<usize> {
    let label = label?.trim();
    let lower = label.to_ascii_lowercase();
    if lower.contains("quick") || label.contains('迅') {
        return Some(0);
    }
    if lower.contains("arts") || label.contains('技') {
        return Some(1);
    }
    if lower.contains("buster") || label.contains('力') {
        return Some(2);
    }
    None
}

fn command_type_self_treasure_device_options(
    skill: &serde_json::Value,
) -> Vec<SkillSelectionOption> {
    let mut act_sets: Vec<String> = Vec::new();
    let mut options: Vec<(usize, Option<String>)> = Vec::new();
    let Some(functions) = skill.get("functions").and_then(|value| value.as_array()) else {
        return Vec::new();
    };

    for function in functions {
        if function
            .get("funcTargetType")
            .and_then(|value| value.as_str())
            != Some("commandTypeSelfTreasureDevice")
        {
            continue;
        }
        let label = first_non_empty_string([
            function.get("funcPopupText"),
            function.get("popupText"),
            function
                .get("buffs")
                .and_then(|value| value.as_array())
                .and_then(|buffs| buffs.first())
                .and_then(|buff| buff.get("detail")),
            function
                .get("buffs")
                .and_then(|value| value.as_array())
                .and_then(|buffs| buffs.first())
                .and_then(|buff| buff.get("name")),
        ]);
        let Some(svals) = function.get("svals").and_then(|value| value.as_array()) else {
            continue;
        };
        for sval in svals {
            let Some(act_set) = sval.get("ActSet").and_then(string_value) else {
                continue;
            };
            if act_sets.contains(&act_set) {
                continue;
            }
            act_sets.push(act_set);
            options.push((options.len(), label.clone()));
        }
    }

    if act_sets.len() < 2 {
        return Vec::new();
    }

    options.sort_by_key(|(original_index, label)| {
        let order = command_card_order(label.as_deref()).unwrap_or(usize::MAX);
        (order, *original_index)
    });

    options
        .into_iter()
        .enumerate()
        .map(|(index, (_, label))| option_label(index, label))
        .collect()
}

fn skill_selection_entry(
    servant_id: u32,
    skill_id: u32,
    skill_num: u32,
    skill: &serde_json::Value,
) -> Option<SkillSelectionEntry> {
    let select_add_info = select_add_info_options(skill);
    let select_treasure_device = select_treasure_device_options(skill);
    let command_type = command_type_self_treasure_device_options(skill);

    let mut supplementary_types = Vec::new();
    let (selection_type, options) = if !select_add_info.is_empty() {
        if !select_treasure_device.is_empty() {
            supplementary_types.push("selectTreasureDeviceInfo".to_string());
        }
        if !command_type.is_empty() {
            supplementary_types.push("commandTypeSelfTreasureDevice".to_string());
        }
        ("SelectAddInfo".to_string(), select_add_info)
    } else if !select_treasure_device.is_empty() {
        if !command_type.is_empty() {
            supplementary_types.push("commandTypeSelfTreasureDevice".to_string());
        }
        (
            "selectTreasureDeviceInfo".to_string(),
            select_treasure_device,
        )
    } else if !command_type.is_empty() {
        ("commandTypeSelfTreasureDevice".to_string(), command_type)
    } else {
        return None;
    };

    Some(SkillSelectionEntry {
        servant_collection_no: servant_id,
        skill_id,
        skill_num,
        selection_type,
        supplementary_types,
        options,
    })
}

fn parse_servant_skill_maps(
    jp_json: &serde_json::Value,
    cn_json: Option<&serde_json::Value>,
) -> ServantSkillMaps {
    let skills = jp_json.get("skills").and_then(|v| v.as_array());

    let icon_map: HashMap<u32, String> = skills
        .map(|skills| {
            skills
                .iter()
                .filter_map(|skill| {
                    let id = skill.get("id")?.as_u64()? as u32;
                    let icon_url = skill.get("icon")?.as_str()?;
                    let filename = icon_url.split('/').next_back()?.to_string();
                    Some((id, filename))
                })
                .collect()
        })
        .unwrap_or_default();

    let mut name_map: HashMap<u32, String> = skills
        .map(|skills| {
            skills
                .iter()
                .filter_map(|skill| {
                    let id = skill.get("id")?.as_u64()? as u32;
                    let name = skill.get("name")?.as_str()?.to_string();
                    Some((id, name))
                })
                .collect()
        })
        .unwrap_or_default();

    let target_type_map: HashMap<u32, Vec<String>> = skills
        .map(|skills| {
            skills
                .iter()
                .filter_map(|skill| {
                    let id = skill.get("id")?.as_u64()? as u32;
                    Some((id, skill_target_types(skill)))
                })
                .collect()
        })
        .unwrap_or_default();

    let mut selection_map: HashMap<u32, SkillSelectionEntry> = skills
        .map(|skills| {
            skills
                .iter()
                .filter_map(|skill| {
                    let id = skill.get("id")?.as_u64()? as u32;
                    skill_selection_entry(0, id, 0, skill).map(|entry| (id, entry))
                })
                .collect()
        })
        .unwrap_or_default();

    if let Some(cn) = cn_json {
        if let Some(skills) = cn.get("skills").and_then(|v| v.as_array()) {
            for skill in skills {
                if let Some(id) = skill.get("id").and_then(|v| v.as_u64()).map(|n| n as u32) {
                    if let Some(name) = skill.get("name").and_then(|v| v.as_str()) {
                        name_map.insert(id, name.to_string());
                    }
                    if let Some(entry) = skill_selection_entry(0, id, 0, skill) {
                        selection_map.insert(id, entry);
                    }
                }
            }
        }
    }

    ServantSkillMaps {
        icon_map,
        name_map,
        target_type_map,
        selection_map,
    }
}

fn variant_skill_ids(
    variants_by_id: &HashMap<u32, Vec<serde_json::Value>>,
    servant_id: u32,
    variant_key: &str,
) -> Option<[Option<u32>; 3]> {
    let variant_index: usize = variant_key
        .split(':')
        .nth(1)
        .and_then(|s| s.parse::<usize>().ok())
        .map(|n| n.saturating_sub(1))
        .unwrap_or(0);

    variants_by_id
        .get(&servant_id)
        .and_then(|v| v.get(variant_index))
        .map(|variant| {
            ["1", "2", "3"].map(|slot| {
                variant
                    .get("skills")
                    .and_then(|s| s.get(slot))
                    .and_then(|arr| arr.as_array())
                    .and_then(|arr| {
                        arr.iter().rev().find(|entry| {
                            entry.get("runtime").and_then(|value| value.as_bool()) != Some(true)
                        })
                    })
                    .and_then(|entry| entry.get("id"))
                    .and_then(|id| id.as_u64())
                    .map(|n| n as u32)
            })
        })
}

fn variant_skill_form_ids(
    variants_by_id: &HashMap<u32, Vec<serde_json::Value>>,
    servant_id: u32,
    variant_key: &str,
) -> Option<[Vec<u32>; 3]> {
    let variant_index: usize = variant_key
        .split(':')
        .nth(1)
        .and_then(|s| s.parse::<usize>().ok())
        .map(|n| n.saturating_sub(1))
        .unwrap_or(0);

    variants_by_id
        .get(&servant_id)
        .and_then(|variants| variants.get(variant_index))
        .map(|variant| {
            ["1", "2", "3"].map(|slot| {
                variant
                    .get("skills")
                    .and_then(|skills| skills.get(slot))
                    .and_then(|entries| entries.as_array())
                    .map(|entries| {
                        entries
                            .iter()
                            .filter_map(|entry| entry.get("id").and_then(|id| id.as_u64()))
                            .map(|id| id as u32)
                            .fold(Vec::new(), |mut ids, id| {
                                if !ids.contains(&id) {
                                    ids.push(id);
                                }
                                ids
                            })
                    })
                    .unwrap_or_default()
            })
        })
}

fn skill_target_types_from_asset(skills_dir: &Path, skill_id: u32) -> Option<Vec<String>> {
    let raw = fs::read_to_string(skills_dir.join(skill_id.to_string()).join("skill.json")).ok()?;
    let skill: serde_json::Value = serde_json::from_str(&raw).ok()?;
    (skill.get("id").and_then(|id| id.as_u64()) == Some(skill_id as u64))
        .then(|| skill_target_types(&skill))
}

fn resolve_skill_targeting_entry(
    servant_id: u32,
    skill_num: u32,
    skill_ids: &[u32],
    maps: &ServantSkillMaps,
    skills_dir: &Path,
) -> Option<SkillTargetingEntry> {
    let skill_id = *skill_ids.last()?;
    let mut func_target_types = Vec::new();
    let mut has_targeted_form = false;
    let mut has_untargeted_form = false;
    let mut has_unresolved_form = false;

    for skill_id in skill_ids {
        let target_types = maps
            .target_type_map
            .get(skill_id)
            .cloned()
            .or_else(|| skill_target_types_from_asset(skills_dir, *skill_id));
        let Some(target_types) = target_types else {
            has_unresolved_form = true;
            continue;
        };
        if target_types.is_empty() {
            has_untargeted_form = true;
        } else {
            has_targeted_form = true;
            func_target_types.extend(target_types);
        }
    }
    func_target_types.sort();
    func_target_types.dedup();

    let targeting_mode = if has_unresolved_form {
        SkillTargetingMode::Unknown
    } else if has_targeted_form && has_untargeted_form {
        SkillTargetingMode::Mixed
    } else if has_targeted_form {
        SkillTargetingMode::NeedsTarget
    } else {
        SkillTargetingMode::NoTarget
    };

    Some(SkillTargetingEntry {
        servant_collection_no: servant_id,
        skill_id,
        skill_num,
        func_target_types,
        targeting_mode,
    })
}

fn servant_skill_maps(app: &tauri::AppHandle, servant_id: u32) -> Option<Arc<ServantSkillMaps>> {
    static CACHE: OnceLock<Mutex<HashMap<u32, Arc<ServantSkillMaps>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    {
        let guard = cache.lock().unwrap();
        if let Some(cached) = guard.get(&servant_id) {
            return Some(Arc::clone(cached));
        }
    }

    let servant_dir = resolve_servant_assets_dir(app)?.join(servant_id.to_string());

    let jp_raw = fs::read_to_string(servant_dir.join("servant.json")).ok()?;
    let jp_json: serde_json::Value = serde_json::from_str(&jp_raw).ok()?;

    let cn_json: Option<serde_json::Value> =
        fs::read_to_string(servant_dir.join("servant-cn.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok());
    let arc = Arc::new(parse_servant_skill_maps(&jp_json, cn_json.as_ref()));
    cache.lock().unwrap().insert(servant_id, Arc::clone(&arc));
    Some(arc)
}

/// Returns icon path and localized name for the three skill slots of a given servant variant.
/// Path is `None` when the local asset file is absent (UI falls back to text label).
/// Name prefers CN from `servant-cn.json`, falls back to JP from `servant.json`.
/// Derived skill maps are cached by `servant_id`; raw JSON is discarded after extraction.
#[tauri::command]
pub(crate) fn get_skill_icon_paths(
    app: tauri::AppHandle,
    servant_id: u32,
    variant_key: String,
) -> [SkillIconEntry; 3] {
    let empty = || {
        [
            SkillIconEntry {
                path: None,
                name: String::new(),
            },
            SkillIconEntry {
                path: None,
                name: String::new(),
            },
            SkillIconEntry {
                path: None,
                name: String::new(),
            },
        ]
    };

    let Some(skill_ids) = variant_skill_ids(variants_raw_data(), servant_id, &variant_key) else {
        return empty();
    };
    let Some(maps) = servant_skill_maps(&app, servant_id) else {
        return empty();
    };

    let icons_dir = app_assets_dir(&app).join("icons");
    skill_ids.map(|maybe_id| {
        let path = maybe_id
            .and_then(|id| maps.icon_map.get(&id))
            .map(|filename| icons_dir.join(filename.as_str()))
            .filter(|p| p.is_file())
            .map(|p| p.to_string_lossy().into_owned());
        let name = maybe_id
            .and_then(|id| maps.name_map.get(&id))
            .cloned()
            .unwrap_or_default();
        SkillIconEntry { path, name }
    })
}

#[tauri::command]
pub(crate) fn get_servant_skill_targeting(
    app: tauri::AppHandle,
    servant_id: u32,
    variant_key: String,
) -> Result<Vec<SkillTargetingEntry>, String> {
    let skill_ids = variant_skill_form_ids(variants_raw_data(), servant_id, &variant_key)
        .ok_or_else(|| format!("未找到从者技能配置: {servant_id} ({variant_key})"))?;
    let maps = servant_skill_maps(&app, servant_id)
        .ok_or_else(|| format!("未找到从者资源: {servant_id}"))?;
    let skills_dir = resolve_servant_assets_dir(&app)
        .and_then(|servants_dir| {
            servants_dir
                .parent()
                .map(|assets_dir| assets_dir.join("skills"))
        })
        .unwrap_or_else(|| app_assets_dir(&app).join("skills"));

    Ok(skill_ids
        .iter()
        .enumerate()
        .filter_map(|(index, ids)| {
            resolve_skill_targeting_entry(servant_id, index as u32 + 1, ids, &maps, &skills_dir)
        })
        .collect())
}

#[tauri::command]
pub(crate) fn get_servant_skill_selection(
    app: tauri::AppHandle,
    servant_id: u32,
    variant_key: String,
) -> Result<Vec<SkillSelectionEntry>, String> {
    let skill_ids = variant_skill_ids(variants_raw_data(), servant_id, &variant_key)
        .ok_or_else(|| format!("未找到从者技能配置: {servant_id} ({variant_key})"))?;
    let maps = servant_skill_maps(&app, servant_id)
        .ok_or_else(|| format!("未找到从者资源: {servant_id}"))?;

    Ok(skill_ids
        .into_iter()
        .enumerate()
        .filter_map(|(index, maybe_id)| {
            let skill_id = maybe_id?;
            let mut entry = maps.selection_map.get(&skill_id)?.clone();
            entry.servant_collection_no = servant_id;
            entry.skill_num = index as u32 + 1;
            Some(entry)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn mystic_code_skill_mode_distinguishes_target_none_and_order_change() {
        assert_eq!(
            mystic_code_skill_mode(&json!({
                "functions": [{ "funcTargetType": "ptOne" }, { "funcTargetType": "enemy" }]
            })),
            MysticCodeSkillMode::NeedsTarget
        );
        assert_eq!(
            mystic_code_skill_mode(&json!({
                "functions": [{ "funcTargetType": "enemy" }, { "funcTargetType": "self" }]
            })),
            MysticCodeSkillMode::NoTarget
        );
        assert_eq!(
            mystic_code_skill_mode(&json!({
                "functions": [{ "funcTargetType": "ptselectOneSub" }]
            })),
            MysticCodeSkillMode::OrderChange
        );
    }

    #[test]
    fn parse_servant_skill_maps_prefers_cn_names_and_extracts_icon_filenames() {
        let jp = json!({
            "skills": [
                { "id": 11, "name": "JP 一技", "icon": "https://cdn.example.com/icons/skill_11.png" },
                { "id": 22, "name": "JP 二技", "icon": "skill_22.png" }
            ]
        });
        let cn = json!({
            "skills": [
                { "id": 11, "name": "CN 一技" }
            ]
        });

        let maps = parse_servant_skill_maps(&jp, Some(&cn));

        assert_eq!(
            maps.icon_map.get(&11).map(String::as_str),
            Some("skill_11.png")
        );
        assert_eq!(
            maps.icon_map.get(&22).map(String::as_str),
            Some("skill_22.png")
        );
        assert_eq!(maps.name_map.get(&11).map(String::as_str), Some("CN 一技"));
        assert_eq!(maps.name_map.get(&22).map(String::as_str), Some("JP 二技"));
    }

    #[test]
    fn parse_servant_skill_maps_extracts_ally_target_skill_types() {
        let jp = json!({
            "skills": [
                {
                    "id": 11,
                    "name": "target ally",
                    "functions": [
                        { "funcTargetType": "ptOneOther" },
                        { "funcTargetType": "enemy" },
                        { "funcTargetType": "ptOne" },
                        { "funcTargetType": "ptOne" }
                    ]
                },
                {
                    "id": 22,
                    "name": "no ally target",
                    "functions": [
                        { "funcTargetType": "enemy" },
                        { "funcTargetType": "ptAll" }
                    ]
                },
                { "id": 33, "name": "no functions" }
            ]
        });

        let maps = parse_servant_skill_maps(&jp, None);

        assert_eq!(
            maps.target_type_map.get(&11),
            Some(&vec!["ptOne".to_string(), "ptOneOther".to_string()])
        );
        assert!(maps.target_type_map.get(&22).is_some_and(Vec::is_empty));
        assert!(maps.target_type_map.get(&33).is_some_and(Vec::is_empty));
    }

    #[test]
    fn parse_servant_skill_maps_extracts_select_add_info_and_prefers_cn_labels() {
        let jp = json!({
            "skills": [
                {
                    "id": 11,
                    "name": "JP skill",
                    "script": {
                        "SelectAddInfo": [
                            { "btn": [{ "name": "JP A" }, { "name": "JP B" }] },
                            { "btn": [{ "name": "JP ignored" }] }
                        ]
                    }
                }
            ]
        });
        let cn = json!({
            "skills": [
                {
                    "id": 11,
                    "script": {
                        "SelectAddInfo": [
                            { "btn": [{ "name": "CN A" }, { "name": "CN B" }] }
                        ]
                    }
                }
            ]
        });

        let maps = parse_servant_skill_maps(&jp, Some(&cn));
        let entry = maps.selection_map.get(&11).unwrap();

        assert_eq!(entry.selection_type, "SelectAddInfo");
        assert_eq!(
            entry.options,
            vec![
                SkillSelectionOption {
                    index: 0,
                    label: "CN A".into(),
                },
                SkillSelectionOption {
                    index: 1,
                    label: "CN B".into(),
                },
            ]
        );
    }

    #[test]
    fn parse_servant_skill_maps_prefers_treasure_device_ui_over_command_type() {
        let jp = json!({
            "skills": [
                {
                    "id": 22,
                    "name": "NP selector",
                    "script": {
                        "selectTreasureDeviceInfo": [
                            {
                                "treasureDevices": [
                                    { "message": "攻击" },
                                    { "message": "防御" }
                                ]
                            }
                        ]
                    },
                    "functions": [
                        {
                            "funcTargetType": "commandTypeSelfTreasureDevice",
                            "funcPopupText": "Arts",
                            "svals": [{ "ActSet": 1 }, { "ActSet": 2 }]
                        }
                    ]
                }
            ]
        });

        let maps = parse_servant_skill_maps(&jp, None);
        let entry = maps.selection_map.get(&22).unwrap();

        assert_eq!(entry.selection_type, "selectTreasureDeviceInfo");
        assert_eq!(
            entry.supplementary_types,
            vec!["commandTypeSelfTreasureDevice"]
        );
        assert_eq!(entry.options[0].label, "攻击");
        assert_eq!(entry.options[1].label, "防御");
    }

    #[test]
    fn parse_servant_skill_maps_extracts_command_type_act_sets() {
        let jp = json!({
            "skills": [
                {
                    "id": 33,
                    "name": "Card switch",
                    "functions": [
                        {
                            "funcTargetType": "commandTypeSelfTreasureDevice",
                            "funcPopupText": "Arts",
                            "svals": [{ "ActSet": "arts" }]
                        },
                        {
                            "funcTargetType": "commandTypeSelfTreasureDevice",
                            "funcPopupText": "Quick",
                            "svals": [{ "ActSet": "quick" }]
                        },
                        {
                            "funcTargetType": "commandTypeSelfTreasureDevice",
                            "buffs": [{ "name": "Buster" }],
                            "svals": [{ "ActSet": "buster" }, { "ActSet": "buster" }]
                        }
                    ]
                }
            ]
        });

        let maps = parse_servant_skill_maps(&jp, None);
        let entry = maps.selection_map.get(&33).unwrap();

        assert_eq!(entry.selection_type, "commandTypeSelfTreasureDevice");
        assert_eq!(
            entry.options,
            vec![
                SkillSelectionOption {
                    index: 0,
                    label: "Quick".into(),
                },
                SkillSelectionOption {
                    index: 1,
                    label: "Arts".into(),
                },
                SkillSelectionOption {
                    index: 2,
                    label: "Buster".into(),
                },
            ]
        );
    }

    #[test]
    fn variant_skill_ids_uses_last_static_entry_for_each_slot() {
        let variants = HashMap::from([(
            42,
            vec![json!({
                "skills": {
                    "1": [
                        { "id": 1001 },
                        { "id": 1002 },
                        { "id": 1003, "runtime": true }
                    ],
                    "2": [{ "id": 2001 }],
                    "3": [{ "id": 3001, "runtime": true }]
                }
            })],
        )]);

        let ids = variant_skill_ids(&variants, 42, "42:1").unwrap();

        assert_eq!(ids, [Some(1002), Some(2001), None]);
    }

    #[test]
    fn variant_skill_ids_returns_none_for_missing_variant() {
        let variants = HashMap::from([(42, vec![json!({ "skills": {} })])]);

        assert!(variant_skill_ids(&variants, 42, "42:2").is_none());
        assert!(variant_skill_ids(&variants, 7, "7:1").is_none());
    }

    #[test]
    fn mash_runtime_skill_does_not_replace_the_latest_static_icon_form() {
        let static_ids = variant_skill_ids(variants_raw_data(), 1, "1:3").unwrap();
        let form_ids = variant_skill_form_ids(variants_raw_data(), 1, "1:3").unwrap();

        // Asset v9 adds a later non-runtime skill form (2550) after 970660.
        // Keep choosing that latest static form while retaining the runtime
        // form for targeting metadata.
        assert_eq!(static_ids[1], Some(2550));
        assert!(form_ids[1].contains(&2477450));
    }

    #[test]
    fn variant_skill_form_ids_returns_every_form_in_each_slot() {
        let variants = HashMap::from([(
            42,
            vec![json!({
                "skills": {
                    "1": [{ "id": 1001 }, { "id": 1002 }, { "id": 1001 }],
                    "2": [{ "id": 2001, "runtime": true }],
                    "3": []
                }
            })],
        )]);

        let ids = variant_skill_form_ids(&variants, 42, "42:1").unwrap();

        assert_eq!(ids, [vec![1001, 1002], vec![2001], vec![]]);
    }

    #[test]
    fn resolve_skill_targeting_entry_reads_runtime_skill_and_marks_mixed() {
        let maps = parse_servant_skill_maps(
            &json!({
                "skills": [{
                    "id": 2550,
                    "functions": [{ "funcTargetType": "ptOne" }]
                }]
            }),
            None,
        );
        let temp = tempfile::tempdir().unwrap();
        let runtime_dir = temp.path().join("2477450");
        fs::create_dir_all(&runtime_dir).unwrap();
        fs::write(
            runtime_dir.join("skill.json"),
            serde_json::to_vec(&json!({
                "id": 2477450,
                "functions": [{ "funcTargetType": "self" }]
            }))
            .unwrap(),
        )
        .unwrap();

        let entry =
            resolve_skill_targeting_entry(1, 2, &[2550, 2477450], &maps, temp.path()).unwrap();

        assert_eq!(entry.skill_id, 2477450);
        assert_eq!(entry.skill_num, 2);
        assert_eq!(entry.func_target_types, vec!["ptOne"]);
        assert_eq!(entry.targeting_mode, SkillTargetingMode::Mixed);
    }

    #[test]
    fn resolve_skill_targeting_entry_stays_unknown_when_a_form_is_missing() {
        let maps = parse_servant_skill_maps(
            &json!({
                "skills": [{
                    "id": 2550,
                    "functions": [{ "funcTargetType": "ptOne" }]
                }]
            }),
            None,
        );
        let temp = tempfile::tempdir().unwrap();

        let entry =
            resolve_skill_targeting_entry(1, 2, &[2550, 2477450], &maps, temp.path()).unwrap();

        assert_eq!(entry.targeting_mode, SkillTargetingMode::Unknown);
    }
}

#[tauri::command]
pub(crate) fn get_template_asset_path(
    app: tauri::AppHandle,
    server_state: tauri::State<'_, Mutex<Server>>,
    template_key: String,
) -> Result<Option<String>, String> {
    let key = template_key.replace('\\', "/");
    if key.is_empty() || key.starts_with('.') || key.contains("/../") {
        return Err("invalid template key".into());
    }
    let server = *server_state.lock().unwrap();
    let Some(root) = resolve_templates_dir(&app, server) else {
        return Ok(None);
    };
    let path = root.join(format!("{key}.png"));
    if path.is_file() {
        Ok(Some(path.to_string_lossy().into_owned()))
    } else {
        Ok(None)
    }
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
            serde_json::from_str(include_str!("../resources/servants.json"))
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
