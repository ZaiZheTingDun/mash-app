//! Versioned, server-local CE recommendation indexes. Raw JSON is discarded
//! after loading; expired events and their bonuses never remain in the cache.

mod display;
use display::{reward_details, BonusDetail, ItemIcons};

use super::craft_essences::{craft_essence_category, craft_essences_data, CraftEssenceInfo};
use crate::commands::assets::read_asset_version;
use crate::commands::runtime::{resolve_ce_assets_dir, resolve_event_assets_dir};
use crate::paths::app_assets_dir;
use crate::server::Server;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CraftEssencePickerData {
    pub craft_essences: Vec<CraftEssenceInfo>,
    pub recommended_ids: Vec<u32>,
    pub bonus_details: BTreeMap<u32, Vec<BonusDetail>>,
    pub empty_reason: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Event {
    id: u32,
    started_at: u64,
    ended_at: u64,
}

#[derive(Deserialize)]
struct EventCatalog {
    regions: HashMap<String, Vec<Event>>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Bonus {
    event_id: u32,
    limit_break: u8,
    #[serde(rename = "type")]
    kind: String,
    function_id: u32,
    function_type: String,
    buff_type: Option<String>,
    target_id: Option<u32>,
    flat: Option<f64>,
    percent: Option<f64>,
    dynamic: bool,
    group: serde_json::Value,
    conditions: serde_json::Value,
    values: Vec<RewardBonusValue>,
    support_values: Option<Vec<RewardBonusValue>>,
}

#[derive(Default, Deserialize, PartialEq)]
#[serde(default)]
struct RewardBonusValue {
    percent: Option<f64>,
    flat: Option<f64>,
    conditions: RewardBonusConditions,
}

#[derive(Default, Deserialize, PartialEq)]
#[serde(default)]
struct RewardBonusConditions {
    #[serde(rename = "EventId")]
    event_id: u32,
    #[serde(flatten)]
    extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompactCe {
    collection_no: u32,
    name: String,
    rarity: u8,
    #[serde(default)]
    flag: String,
    #[serde(default)]
    flags: Vec<String>,
    #[serde(default)]
    bond_equip_owner: Option<u32>,
    #[serde(default)]
    bond_bonuses: Vec<Bonus>,
    #[serde(default)]
    qp_bonuses: Vec<Bonus>,
    #[serde(default)]
    master_exp_bonuses: Vec<Bonus>,
    #[serde(default)]
    mystic_code_exp_bonuses: Vec<Bonus>,
    #[serde(default)]
    friend_point_bonuses: Vec<Bonus>,
    #[serde(default)]
    event_bonuses: Vec<Bonus>,
}

#[derive(Default)]
struct CeBonuses {
    owner: Option<u32>,
    // Permanent percentage bond/QP bonuses; event-limited rewards use `events`.
    permanent: Vec<BonusDetail>,
    events: Vec<(u32, BonusDetail)>,
    // Informational rewards do not change recommendation eligibility.
    additional: Vec<(u32, BonusDetail)>,
}

struct PickerIndex {
    catalog: BTreeMap<u32, CraftEssenceInfo>,
    campaign_equip_ids: HashSet<u32>,
    events: HashMap<u32, Event>,
    bonuses: HashMap<u32, CeBonuses>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Source {
    ces: Option<PathBuf>,
    events: Option<PathBuf>,
    version: Option<u32>,
}

#[derive(Default)]
struct PickerCache {
    source: Option<Source>,
    servers: HashMap<Server, PickerIndex>,
    #[cfg(test)]
    loads: usize,
}

impl PickerCache {
    fn index(
        &mut self,
        source: &Source,
        server: Server,
        now: u64,
    ) -> Result<&mut PickerIndex, String> {
        if self.source.as_ref() != Some(source) {
            self.servers.clear();
            self.source = Some(source.clone());
        }
        // The caller holds the cache mutex through initialization so concurrent
        // requests share one load. Failed loads are never inserted.
        if let std::collections::hash_map::Entry::Vacant(entry) = self.servers.entry(server) {
            entry.insert(PickerIndex::load(source, server, now)?);
            #[cfg(test)]
            {
                self.loads += 1;
            }
        }
        let index = self.servers.get_mut(&server).unwrap();
        index.prune(now);
        Ok(index)
    }

    fn invalidate(&mut self) {
        *self = Self::default();
    }
}

fn cache() -> &'static Mutex<PickerCache> {
    static CACHE: OnceLock<Mutex<PickerCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(PickerCache::default()))
}

fn reset_cache() {
    cache()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .invalidate();
}

pub(crate) struct PickerCacheReset;

impl Drop for PickerCacheReset {
    fn drop(&mut self) {
        reset_cache();
    }
}

pub(crate) fn reset_picker_cache_on_drop() -> PickerCacheReset {
    reset_cache();
    PickerCacheReset
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let raw = fs::read_to_string(path)
        .map_err(|error| format!("读取礼装推荐数据失败 ({}): {error}", path.display()))?;
    serde_json::from_str(&raw)
        .map_err(|error| format!("解析礼装推荐数据失败 ({}): {error}", path.display()))
}

// A constant skill still applies at MLB even if the source has no MLB variant.
fn applies_at_mlb(bonus: &Bonus, variants: &[Bonus]) -> bool {
    bonus.limit_break == 4
        || (bonus.limit_break == 0
            && !variants.iter().any(|other| {
                other.limit_break == 4
                    && other.function_id == bonus.function_id
                    && other.event_id == bonus.event_id
            }))
}

fn push_detail(details: &mut Vec<BonusDetail>, detail: BonusDetail) {
    if !detail.text.is_empty() && !details.contains(&detail) {
        details.push(detail);
    }
}

impl PickerIndex {
    fn load(source: &Source, server: Server, now: u64) -> Result<Self, String> {
        let mut index = Self {
            catalog: craft_essences_data()
                .iter()
                .cloned()
                .map(|ce| (ce.id, ce))
                .collect(),
            campaign_equip_ids: HashSet::new(),
            events: HashMap::new(),
            bonuses: HashMap::new(),
        };
        if let Some(root) = &source.events {
            let path = root.join("catalog.json");
            if path.is_file() {
                let mut catalog: EventCatalog = read_json(&path)?;
                index.events = catalog
                    .regions
                    .remove(&server.to_string())
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|event| event.ended_at > now)
                    .map(|event| (event.id, event))
                    .collect();
            }
        }
        let item_icons = ItemIcons::load(source.events.as_deref())?;
        let Some(root) = &source.ces else {
            return Ok(index);
        };
        let directories =
            fs::read_dir(root).map_err(|error| format!("读取礼装目录失败: {error}"))?;
        for entry in directories {
            let entry = entry.map_err(|error| format!("读取礼装目录失败: {error}"))?;
            if !entry.path().is_dir() || entry.file_name().to_string_lossy().parse::<u32>().is_err()
            {
                continue;
            }
            let jp = entry.path().join("craft-essence.json");
            let local = entry.path().join(match server {
                Server::Jp => "craft-essence.json",
                Server::Cn => "craft-essence-cn.json",
            });
            let has_server_data = local.is_file();
            let path = if has_server_data { &local } else { &jp };
            if !path.is_file() {
                continue;
            }
            let ce: CompactCe = read_json(path)?;
            if !(1..=5).contains(&ce.rarity) {
                continue;
            }
            index
                .catalog
                .entry(ce.collection_no)
                .or_insert_with(|| CraftEssenceInfo {
                    id: ce.collection_no,
                    rarity: ce.rarity,
                    category: craft_essence_category(
                        &serde_json::json!({"flag": ce.flag, "flags": ce.flags}),
                    ),
                    name: ce.name.clone(),
                    name_aliases: Vec::new(),
                    name_link: None,
                });
            if ce.flag == "svtEquipCampaign"
                || ce.flags.iter().any(|flag| flag == "svtEquipCampaign")
            {
                index.campaign_equip_ids.insert(ce.collection_no);
            }
            // A missing CN file must not activate JP bonuses/owners in CN.
            if !has_server_data {
                continue;
            }
            let mut bonuses = CeBonuses {
                owner: ce.bond_equip_owner,
                ..Default::default()
            };
            for bonus in &ce.event_bonuses {
                if index.events.contains_key(&bonus.event_id) {
                    let mut detail = item_icons.event_detail(bonus);
                    detail.at_mlb = applies_at_mlb(bonus, &ce.event_bonuses);
                    if !bonuses.events.contains(&(bonus.event_id, detail.clone())) {
                        bonuses.events.push((bonus.event_id, detail));
                    }
                }
            }
            for (reward_bonuses, kind) in [(&ce.bond_bonuses, "牵绊"), (&ce.qp_bonuses, "量子")]
            {
                for bonus in reward_bonuses {
                    for (event_id, mut detail) in reward_details(bonus, kind) {
                        detail.at_mlb = applies_at_mlb(bonus, reward_bonuses);
                        if event_id == 0 {
                            push_detail(&mut bonuses.permanent, detail);
                        } else if index.events.contains_key(&event_id)
                            && !bonuses.events.contains(&(event_id, detail.clone()))
                        {
                            bonuses.events.push((event_id, detail));
                        }
                    }
                }
            }
            for (additional_bonuses, kind) in [
                (&ce.master_exp_bonuses, "御主经验"),
                (&ce.mystic_code_exp_bonuses, "魔术礼装经验"),
                (&ce.friend_point_bonuses, "友情点数"),
            ] {
                for bonus in additional_bonuses {
                    let applies_at_mlb = applies_at_mlb(bonus, additional_bonuses);
                    if !applies_at_mlb {
                        continue;
                    }
                    for (event_id, mut detail) in reward_details(bonus, kind) {
                        detail.at_mlb = true;
                        if (event_id == 0 || index.events.contains_key(&event_id))
                            && !bonuses.additional.contains(&(event_id, detail.clone()))
                        {
                            bonuses.additional.push((event_id, detail));
                        }
                    }
                }
            }
            if bonuses.owner.is_some()
                || !bonuses.additional.is_empty()
                || !bonuses.permanent.is_empty()
                || !bonuses.events.is_empty()
            {
                index.bonuses.insert(ce.collection_no, bonuses);
            }
        }
        Ok(index)
    }

    fn prune(&mut self, now: u64) {
        self.events.retain(|_, event| event.ended_at > now);
        self.bonuses.retain(|_, bonuses| {
            bonuses
                .events
                .retain(|(id, _)| self.events.contains_key(id));
            bonuses
                .additional
                .retain(|(id, _)| *id == 0 || self.events.contains_key(id));
            bonuses.owner.is_some()
                || !bonuses.permanent.is_empty()
                || !bonuses.events.is_empty()
                || !bonuses.additional.is_empty()
        });
    }

    fn picker_data(
        &self,
        now: u64,
        grand_bond_only: bool,
        servant_id: Option<u32>,
    ) -> CraftEssencePickerData {
        let allowed: Option<u32> = if grand_bond_only {
            // IPC and projects use collection numbers; bondEquipOwner uses
            // the base Atlas svt ID, never a portrait/ascension ID.
            servant_id
                .and_then(|id| {
                    super::servants_data()
                        .iter()
                        .find(|servant| servant.id == id)
                })
                .and_then(|servant| {
                    self.catalog.keys().copied().find(|id| {
                        self.bonuses
                            .get(id)
                            .is_some_and(|bonus| bonus.owner == Some(servant.atlas_id))
                    })
                })
        } else {
            None
        };
        let catalog: Vec<_> = self
            .catalog
            .values()
            .filter(|ce| !grand_bond_only || allowed == Some(ce.id))
            .cloned()
            .collect();
        let mut event_ids = Vec::new();
        let mut permanent_ids = Vec::new();
        let mut details = BTreeMap::new();
        for ce in &catalog {
            let Some(bonuses) = self.bonuses.get(&ce.id) else {
                continue;
            };
            let mut ce_details = Vec::new();
            let mut active = false;
            for (id, detail) in &bonuses.events {
                if self
                    .events
                    .get(id)
                    .is_some_and(|event| event.started_at <= now && now < event.ended_at)
                {
                    active = true;
                    if detail.at_mlb {
                        push_detail(&mut ce_details, detail.clone());
                    }
                }
            }
            if active && !self.campaign_equip_ids.contains(&ce.id) {
                event_ids.push(ce.id);
            } else if !active
                && !bonuses.permanent.is_empty()
                && !self.campaign_equip_ids.contains(&ce.id)
            {
                permanent_ids.push(ce.id);
            }
            for detail in &bonuses.permanent {
                if detail.at_mlb {
                    push_detail(&mut ce_details, detail.clone());
                }
            }
            for (id, detail) in &bonuses.additional {
                if *id == 0
                    || self
                        .events
                        .get(id)
                        .is_some_and(|event| event.started_at <= now && now < event.ended_at)
                {
                    push_detail(&mut ce_details, detail.clone());
                }
            }
            if !ce_details.is_empty() {
                details.insert(ce.id, ce_details);
            }
        }
        // Collection numbers are stable across servers and localized names.
        const COMMON_REWARD_CES: [u32; 4] = [910, 330, 988, 80];
        let priority = |id: &u32| {
            COMMON_REWARD_CES
                .iter()
                .position(|common| common == id)
                .unwrap_or(COMMON_REWARD_CES.len())
        };
        let bond_percent = |id: &u32| {
            self.bonuses[id]
                .permanent
                .iter()
                .filter(|detail| detail.at_mlb)
                .map(|detail| detail.bond_percent)
                .fold(0.0, f64::max)
        };
        permanent_ids.sort_by(|left, right| {
            priority(left)
                .cmp(&priority(right))
                .then_with(|| bond_percent(right).total_cmp(&bond_percent(left)))
                .then_with(|| left.cmp(right))
        });
        event_ids.extend(permanent_ids);
        // The seven FSN 英灵逢魔 CEs always follow the other recommendations.
        event_ids.sort_by_key(|id| {
            if (1973..=1979).contains(id) {
                (true, *id)
            } else {
                (false, 0)
            }
        });
        CraftEssencePickerData {
            craft_essences: catalog,
            recommended_ids: event_ids,
            bonus_details: details,
            empty_reason: if grand_bond_only && allowed.is_none() {
                Some(
                    if servant_id.is_none() {
                        "请先选择冠位从者"
                    } else {
                        "未找到该从者的牵绊礼装数据"
                    }
                    .to_string(),
                )
            } else {
                None
            },
        }
    }
}

fn source(app: &tauri::AppHandle) -> Source {
    Source {
        ces: resolve_ce_assets_dir(app),
        events: resolve_event_assets_dir(app),
        version: read_asset_version(&app_assets_dir(app)).map(|record| record.version),
    }
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(crate) fn craft_essence_picker_data(
    app: &tauri::AppHandle,
    server: Server,
    grand_bond_only: bool,
    servant_id: Option<u32>,
) -> Result<CraftEssencePickerData, String> {
    let source = source(app);
    let now = now_seconds();
    let mut cache = cache().lock().map_err(|error| error.to_string())?;
    Ok(cache
        .index(&source, server, now)?
        .picker_data(now, grand_bond_only, servant_id))
}

#[tauri::command]
pub(crate) async fn get_craft_essence_picker_data(
    app: tauri::AppHandle,
    server_state: tauri::State<'_, Mutex<Server>>,
    grand_bond_only: bool,
    servant_id: Option<u32>,
) -> Result<CraftEssencePickerData, String> {
    let server = *server_state.lock().unwrap();
    tauri::async_runtime::spawn_blocking(move || {
        craft_essence_picker_data(&app, server, grand_bond_only, servant_id)
    })
    .await
    .map_err(|error| format!("加载礼装推荐任务失败: {error}"))?
}

#[cfg(test)]
mod tests;
