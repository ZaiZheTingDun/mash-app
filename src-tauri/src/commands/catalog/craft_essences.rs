use crate::commands::runtime::resolve_ce_assets_dir;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

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

/// The subset of craft-essence categories exposed by the picker. Atlas has
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
            serde_json::from_str(include_str!("../../resources/craft_essences.json"))
                .expect("invalid craft_essences.json");
        let translation_fixes: HashMap<u32, String> = serde_json::from_str::<
            Vec<CraftEssenceTranslationFix>,
        >(include_str!(
            "../../resources/craft_essence_translation_fixes.json"
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

/// Return `<ce_root>/<id>/card_ce.png` when that card asset exists.
pub(crate) fn pick_ce_card_in(ce_root: &Path, ce_id: u32) -> Option<PathBuf> {
    let candidate = ce_root.join(ce_id.to_string()).join("card_ce.png");
    candidate.is_file().then_some(candidate)
}

/// Resolve the card art for a single craft essence, returning an absolute path
/// for the frontend or `None` when the asset tree or card file is unavailable.
#[tauri::command]
pub(crate) fn get_craft_essence_card_path(
    app: tauri::AppHandle,
    craft_essence_id: u32,
) -> Result<Option<String>, String> {
    let Some(root) = resolve_ce_assets_dir(&app) else {
        return Ok(None);
    };
    Ok(pick_ce_card_in(&root, craft_essence_id).map(|path| path.to_string_lossy().into_owned()))
}
