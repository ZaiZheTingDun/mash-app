//! Mystic-code catalog and skill metadata.

use super::servants::{string_field, u32_field};
use crate::commands::runtime::resolve_mystic_code_assets_dir;
use std::fs;
use std::path::Path;

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

#[cfg(test)]
mod tests;
