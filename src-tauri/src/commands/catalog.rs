//! Servant, craft essence, and template catalog access.
//! Localization rules live here so runners receive server-specific metadata.

use super::*;

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServantNameAlias {
    pub(crate) name_jp: Option<String>,
    pub(crate) name_cn: Option<String>,
}

#[derive(serde::Serialize, Clone)]
pub(crate) struct ServantInfo {
    pub(crate) id: u32,
    #[serde(rename = "variantKey")]
    pub(crate) variant_key: String,
    #[serde(rename = "faceId")]
    pub(crate) face_id: Option<u32>,
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

pub(crate) fn display_class_name(raw: &str) -> String {
    match raw.trim() {
        "alterEgo" => "Alterego".into(),
        "moonCancer" => "Moon Cancer".into(),
        "uOlgaMarieAquaCollection" => "U-Olga Marie Aqua".into(),
        "uOlgaMarieFlareCollection" => "U-Olga Marie Flare".into(),
        "uOlgaMarieGrandCollection" => "U-Olga Marie Grand".into(),
        "uOlgaMarieStellarCollection" => "U-Olga Marie Stellar".into(),
        "unBeastOlgaMarie" => "U-Olga Marie".into(),
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
    let arr = variant
        .get("noblePhantasms_cn")
        .or_else(|| variant.get("noblePhantasms"))?
        .as_array()?;
    arr.iter()
        .rev()
        .find_map(|v| v.as_str().map(str::trim).filter(|s| !s.is_empty()))
        .map(str::to_string)
}

pub(crate) fn variant_face_id(variant: &serde_json::Value) -> Option<u32> {
    variant
        .get("ids")?
        .as_array()?
        .iter()
        .filter_map(|v| v.as_u64().map(|n| n as u32))
        .max()
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
                                ServantInfo {
                                    id,
                                    variant_key: format!("{id}:{}", variant_idx + 1),
                                    face_id: variant_face_id(variant),
                                    name_cn: name_cn.clone(),
                                    name_cn_server: name_cn_server.clone(),
                                    name_jp: name_jp.clone(),
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
                            variant_key: id.to_string(),
                            face_id: None,
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

#[tauri::command]
pub(crate) fn get_servants() -> &'static [ServantInfo] {
    servants_data()
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
    pub name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub name_aliases: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_link: Option<String>,
}

#[derive(serde::Deserialize)]
struct CraftEssenceTranslationFix {
    id: u32,
    name: String,
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
pub(crate) fn get_servant_portrait_path(
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
pub(crate) fn get_servant_face_path(
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

struct ServantSkillMaps {
    icon_map: HashMap<u32, String>,
    name_map: HashMap<u32, String>,
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

    if let Some(cn) = cn_json {
        if let Some(skills) = cn.get("skills").and_then(|v| v.as_array()) {
            for skill in skills {
                if let (Some(id), Some(name)) = (
                    skill.get("id").and_then(|v| v.as_u64()).map(|n| n as u32),
                    skill.get("name").and_then(|v| v.as_str()),
                ) {
                    name_map.insert(id, name.to_string());
                }
            }
        }
    }

    ServantSkillMaps { icon_map, name_map }
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
                    .and_then(|arr| arr.last())
                    .and_then(|entry| entry.get("id"))
                    .and_then(|id| id.as_u64())
                    .map(|n| n as u32)
            })
        })
}

fn servant_skill_maps(
    app: &tauri::AppHandle,
    servant_id: u32,
) -> Option<Arc<ServantSkillMaps>> {
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

    let cn_json: Option<serde_json::Value> = fs::read_to_string(servant_dir.join("servant-cn.json"))
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
    let empty = || [
        SkillIconEntry { path: None, name: String::new() },
        SkillIconEntry { path: None, name: String::new() },
        SkillIconEntry { path: None, name: String::new() },
    ];

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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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

        assert_eq!(maps.icon_map.get(&11).map(String::as_str), Some("skill_11.png"));
        assert_eq!(maps.icon_map.get(&22).map(String::as_str), Some("skill_22.png"));
        assert_eq!(maps.name_map.get(&11).map(String::as_str), Some("CN 一技"));
        assert_eq!(maps.name_map.get(&22).map(String::as_str), Some("JP 二技"));
    }

    #[test]
    fn variant_skill_ids_uses_last_entry_for_each_slot() {
        let variants = HashMap::from([(
            42,
            vec![json!({
                "skills": {
                    "1": [{ "id": 1001 }, { "id": 1002 }],
                    "2": [{ "id": 2001 }],
                    "3": []
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
    let meta = ServantMetadata {
        id,
        name,
        names,
        np_names,
        class_name,
    };
    cache.lock().unwrap().insert((id, server), meta.clone());
    Ok(meta)
}

#[tauri::command]
pub(crate) fn get_servant_metadata(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<Server>>,
    id: u32,
) -> Result<ServantMetadata, String> {
    let server = *state.lock().unwrap();
    load_servant_metadata(&app, id, server)
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AdbStatus {
    pub(crate) connected: bool,
    pub(crate) device_name: Option<String>,
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AdbResetStep {
    pub(crate) command: String,
    pub(crate) success: bool,
    pub(crate) status: Option<i32>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AdbResetResult {
    pub(crate) ok: bool,
    pub(crate) steps: Vec<AdbResetStep>,
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AdbResetStatusEvent {
    pub(crate) message: String,
    pub(crate) step: Option<AdbResetStep>,
    pub(crate) done: bool,
    pub(crate) ok: Option<bool>,
}
