//! Servant, craft essence, and template catalog access.
//! Localization rules live here so runners receive server-specific metadata.

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

mod portraits;
pub(crate) use portraits::*;

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
