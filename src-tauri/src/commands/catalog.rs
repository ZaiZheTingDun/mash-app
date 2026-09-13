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

mod craft_essences;
pub(crate) use craft_essences::*;
mod skills;
pub(crate) use skills::*;

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

mod metadata;
pub(crate) use metadata::*;
