//! Mystic-code and servant-skill catalog commands.
//!
//! This module owns skill-form resolution, targeting metadata, selection
//! options, and the associated asset lookups.

use super::*;

fn variants_raw_data() -> &'static HashMap<u32, Vec<serde_json::Value>> {
    static VARIANTS: OnceLock<HashMap<u32, Vec<serde_json::Value>>> = OnceLock::new();
    VARIANTS.get_or_init(|| {
        let raw: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../../resources/servants_variants.json"))
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
mod tests;
