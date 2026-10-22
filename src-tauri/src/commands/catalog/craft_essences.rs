use super::data::CatalogCache;
use super::summaries::catalog_summaries;
use crate::commands::runtime::resolve_ce_assets_dir;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// One craft-essence entry exposed to the frontend. Mirrors the shape of
/// `catalogs/craft_essences.json` (using Atlas `collectionNo` as the
/// stable app-facing CE id). Recommendation metadata stays in the backend's
/// cached picker index rather than traveling with every catalog entry.
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

pub(super) fn craft_essence_category(flag: &str, flags: &[String]) -> CraftEssenceCategory {
    let has_flag = |expected: &str| flags.iter().any(|flag| flag == expected);
    if flag == "normal" {
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

pub(crate) fn craft_essences_data() -> Result<Arc<Vec<CraftEssenceInfo>>, String> {
    static CES: CatalogCache<Vec<CraftEssenceInfo>> = CatalogCache::new();
    CES.get_or_try_init(|| {
        Ok(catalog_summaries()?.ces.iter()
            .enumerate()
            .filter_map(|(idx, ce)| {
                let result = (|| {
                    let id = ce.collection_no;
                    let rarity = ce.rarity;
                    if !(1..=5).contains(&rarity) {
                        return None;
                    }
                    let raw_name = ce.name_cn.as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .unwrap_or(&ce.name)
                        .to_string();
                    let name = raw_name;
                    let name_aliases = Vec::new();
                    let name_link = ce.name_link.clone();
                    Some(CraftEssenceInfo {
                        id,
                        rarity,
                        category: craft_essence_category(&ce.flag, &ce.flags),
                        name,
                        name_aliases,
                        name_link,
                    })
                })();
                if result.is_none() {
                    let id_hint = ce.collection_no;
                    eprintln!(
                        "[craft_essences] dropping entry at index {idx} (id={id_hint:?}): missing or invalid fields"
                    );
                }
                result
            })
            .collect())
    })
}

#[tauri::command]
pub(crate) async fn get_craft_essences() -> Result<Vec<CraftEssenceInfo>, String> {
    // Loading a basic list must not initialize recommendations or read details.
    tauri::async_runtime::spawn_blocking(|| Ok(craft_essences_data()?.to_vec()))
        .await
        .map_err(|error| format!("加载礼装目录任务失败: {error}"))?
}

pub(crate) fn ce_card_path(ce_root: &Path, ce_id: u32) -> PathBuf {
    ce_root.join(ce_id.to_string()).join("card_ce.png")
}

/// Return `<ce_root>/<id>/card_ce.png` when that card asset exists.
pub(crate) fn pick_ce_card_in(ce_root: &Path, ce_id: u32) -> Option<PathBuf> {
    let candidate = ce_card_path(ce_root, ce_id);
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
